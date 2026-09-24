//! Daily summaries and their scheduling (startup, resume from suspend, day rollover, manual).

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, Mutex};
use tokio_util::sync::CancellationToken;

use crate::ai::SharedProvider;
use crate::settings::RecallSettings;
use crate::storage::{today, Capture, Storage, Summary};

/// Maximum characters of raw captures sent in one summarisation request.
const CHUNK_CHARS: usize = 250_000;

#[derive(Clone)]
pub struct Summariser {
    pub provider: SharedProvider,
    pub storage: Storage,
    pub settings: RecallSettings,
}

impl Summariser {
    pub fn system_prompt(settings: &RecallSettings) -> String {
        format!(
            "You summarise a day of screen captures (OCR text or descriptions) from one person's work computer. \
             {} Write in {}. Never invent events that are not in the captures. Mention people, projects, \
             messages and decisions with times when visible. Keep sensitive data (passwords, tokens) out.",
            settings.style_prompt, settings.summary_language
        )
    }

    fn render_captures(captures: &[Capture]) -> Vec<String> {
        let mut chunks = Vec::new();
        let mut current = String::new();
        for c in captures {
            let time = chrono::DateTime::from_timestamp(c.ts, 0)
                .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
                .unwrap_or_default();
            let entry = format!("### {time}\n{}\n\n", c.text.trim());
            if current.len() + entry.len() > CHUNK_CHARS && !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            current.push_str(&entry);
        }
        if !current.is_empty() {
            chunks.push(current);
        }
        chunks
    }

    /// Summarise one day from its raw captures and store the result.
    pub async fn summarise_day(&self, day: &str) -> anyhow::Result<Summary> {
        let captures = self.storage.captures_for_day(day)?;
        if captures.is_empty() {
            anyhow::bail!("no captures for {day}");
        }
        let system = Self::system_prompt(&self.settings);
        let chunks = Self::render_captures(&captures);
        let mut partials = Vec::new();
        for (i, chunk) in chunks.iter().enumerate() {
            let prompt = if chunks.len() == 1 {
                format!("Date: {day}\n\nCaptures:\n\n{chunk}")
            } else {
                format!(
                    "Date: {day} (part {} of {})\n\nCaptures:\n\n{chunk}",
                    i + 1,
                    chunks.len()
                )
            };
            partials.push(self.provider.complete(&system, &prompt).await?);
        }
        let summary = if partials.len() == 1 {
            partials.remove(0)
        } else {
            let merged = partials.join("\n\n---\n\n");
            self.provider
                .complete(
                    &system,
                    &format!("Merge these partial summaries of {day} into one daily summary:\n\n{merged}"),
                )
                .await?
        };
        self.storage
            .upsert_summary(day, &summary, self.provider.model(), captures.len() as i64)?;
        Ok(Summary {
            day: day.to_string(),
            summary,
            model: self.provider.model().to_string(),
            created_ts: crate::storage::now_ts(),
            capture_count: captures.len() as i64,
        })
    }

    /// Summarise every past day without a summary, oldest first. Returns the days done.
    pub async fn run_pending(&self) -> anyhow::Result<Vec<String>> {
        let today = today();
        let pending = self.storage.days_pending_summary(&today)?;
        let mut done = Vec::new();
        for day in pending {
            match self.summarise_day(&day).await {
                Ok(_) => done.push(day),
                Err(e) => tracing::warn!("summary for {day} failed: {e}"),
            }
        }
        self.storage.prune_captures(&today, self.settings.retention_days)?;
        Ok(done)
    }
}

#[derive(Debug, Clone)]
pub enum SchedulerEvent {
    /// Summaries were generated for these days.
    Summarised(Vec<String>),
    Error(String),
}

/// Runs pending summaries on startup, after a resume from suspend (detected as a wall-clock
/// jump) and after midnight.
pub struct Scheduler {
    pub summariser: Arc<Mutex<Option<Summariser>>>,
    cancel: CancellationToken,
}

const TICK: Duration = Duration::from_secs(60);
/// A tick that arrives this late means the machine slept.
const SLEEP_GAP: Duration = Duration::from_secs(180);

impl Scheduler {
    pub fn start(summariser: Option<Summariser>, run_on_startup: bool) -> (Scheduler, mpsc::Receiver<SchedulerEvent>) {
        let (tx, rx) = mpsc::channel(16);
        let cancel = CancellationToken::new();
        let shared = Arc::new(Mutex::new(summariser));
        let this = Scheduler {
            summariser: shared.clone(),
            cancel: cancel.clone(),
        };
        tokio::spawn(async move {
            if run_on_startup {
                Self::run(&shared, &tx).await;
            }
            let mut last_tick = std::time::Instant::now();
            let mut last_day = today();
            loop {
                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = tokio::time::sleep(TICK) => {}
                }
                let now = std::time::Instant::now();
                let slept = now.duration_since(last_tick) > TICK + SLEEP_GAP;
                last_tick = now;
                let day = today();
                let rolled = day != last_day;
                last_day = day;
                if slept || rolled {
                    tracing::info!("recall scheduler: resume={slept} new_day={rolled}, checking pending summaries");
                    Self::run(&shared, &tx).await;
                }
            }
        });
        (this, rx)
    }

    async fn run(shared: &Arc<Mutex<Option<Summariser>>>, tx: &mpsc::Sender<SchedulerEvent>) {
        let s = shared.lock().await.clone();
        if let Some(s) = s {
            match s.run_pending().await {
                Ok(days) if !days.is_empty() => {
                    let _ = tx.send(SchedulerEvent::Summarised(days)).await;
                }
                Ok(_) => {}
                Err(e) => {
                    let _ = tx.send(SchedulerEvent::Error(e.to_string())).await;
                }
            }
        }
    }

    /// Replace the summariser (settings or provider changed).
    pub async fn set_summariser(&self, s: Option<Summariser>) {
        *self.summariser.lock().await = s;
    }

    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::testing::FakeProvider;
    use chrono::{Local, TimeZone};

    #[tokio::test]
    async fn summarises_pending_days_and_prunes() {
        let storage = Storage::open_in_memory().unwrap();
        let ts = Local.with_ymd_and_hms(2020, 1, 1, 9, 0, 0).unwrap().timestamp();
        storage.insert_capture(ts, "Wrote the spec", "h1", "ocr").unwrap();
        storage.insert_capture(ts + 60, "Answered Anna", "h2", "ocr").unwrap();
        let provider = Arc::new(FakeProvider::new(vec![FakeProvider::text("Day summary")]));
        let s = Summariser {
            provider: provider.clone(),
            storage: storage.clone(),
            settings: RecallSettings::default(),
        };
        let done = s.run_pending().await.unwrap();
        assert_eq!(done, vec!["2020-01-01"]);
        let sum = storage.summary_for_day("2020-01-01").unwrap().unwrap();
        assert_eq!(sum.summary, "Day summary");
        assert_eq!(sum.capture_count, 2);
        // raw captures older than retention are pruned after summarising
        assert!(storage.captures_for_day("2020-01-01").unwrap().is_empty());
        let req = &provider.requests.lock().await[0];
        assert!(req.messages[0].text().contains("Answered Anna"));
        assert!(req.system.contains("English"));
    }
}
