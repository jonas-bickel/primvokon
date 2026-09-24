//! The watcher loop: sample → detect → cooldown → store → notify.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::ntfy::{Notification, Notifier, NtfyNotifier};
use crate::screen::{thumbnail_jpeg, ScreenSource};
use crate::settings::WatcherSettings;
use crate::storage::{now_ts, Storage, WatchEvent};

use super::detectors::{ChangeDetector, Sample};

#[derive(Debug, Clone)]
pub enum WatcherEvent {
    Started,
    /// A sample was taken (for the "last check" label).
    Sampled { ts: i64 },
    Change(WatchEvent),
    Error(String),
    Stopped,
}

/// Running watcher.
#[derive(Clone)]
pub struct WatcherHandle {
    cancel: CancellationToken,
}

impl WatcherHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.cancel.is_cancelled()
    }
}

pub struct WatcherService {
    pub screen: Arc<dyn ScreenSource>,
    pub detector: Box<dyn ChangeDetector>,
    pub notifier: Option<Arc<dyn Notifier>>,
    pub storage: Storage,
    pub settings: WatcherSettings,
    /// Host name used in notification titles.
    pub host: String,
    pub title_template: String,
    pub attach_snapshot: bool,
}

impl WatcherService {
    pub fn start(self) -> (WatcherHandle, mpsc::Receiver<WatcherEvent>) {
        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let handle = WatcherHandle {
            cancel: cancel.clone(),
        };
        tokio::spawn(self.run(tx, cancel));
        (handle, rx)
    }

    async fn run(mut self, tx: mpsc::Sender<WatcherEvent>, cancel: CancellationToken) {
        let _ = tx.send(WatcherEvent::Started).await;
        let interval = Duration::from_secs(self.settings.interval_secs.max(1));
        let mut last_notified: Option<i64> = None;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(interval) => {}
            }
            match self.step(&mut last_notified).await {
                Ok(Some(ev)) => {
                    let _ = tx.send(WatcherEvent::Change(ev)).await;
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = tx.send(WatcherEvent::Error(e.to_string())).await;
                }
            }
            let _ = tx.send(WatcherEvent::Sampled { ts: now_ts() }).await;
        }
        let _ = tx.send(WatcherEvent::Stopped).await;
    }

    /// One iteration; returns the stored event when a change was detected.
    pub async fn step(&mut self, last_notified: &mut Option<i64>) -> anyhow::Result<Option<WatchEvent>> {
        let sample = self.sample().await?;
        let Some(change) = self.detector.observe(&sample).await? else {
            return Ok(None);
        };
        let ts = now_ts();
        if let Some(prev) = *last_notified {
            if ts - prev < self.settings.cooldown_secs as i64 {
                tracing::debug!("change suppressed by cooldown: {}", change.reason);
                return Ok(None);
            }
        }
        *last_notified = Some(ts);

        let thumbnail = sample
            .jpeg
            .as_ref()
            .and_then(|j| thumbnail_jpeg(j, 320).ok());
        let mut notified = false;
        let mut error = None;
        if let Some(notifier) = &self.notifier {
            let n = Notification {
                title: NtfyNotifier::render_title(&self.title_template, &change.reason, &self.host),
                message: if change.detail.is_empty() {
                    change.reason.clone()
                } else {
                    change.detail.clone()
                },
                priority: None,
                tags: Vec::new(),
                attachment: if self.attach_snapshot {
                    sample.jpeg.clone().map(|j| ("screen.jpg".to_string(), j))
                } else {
                    None
                },
            };
            match notifier.notify(&n).await {
                Ok(()) => notified = true,
                Err(e) => error = Some(e.to_string()),
            }
        }
        let id = self
            .storage
            .insert_watch_event(&change.reason, &change.detail, thumbnail.as_deref(), notified, error.as_deref())?;
        Ok(Some(WatchEvent {
            id,
            ts,
            reason: change.reason,
            detail: change.detail,
            thumbnail,
            notified,
            error,
        }))
    }

    async fn sample(&self) -> anyhow::Result<Sample> {
        let want_image = self.detector.needs_image() || self.attach_snapshot;
        let jpeg = if want_image {
            Some(self.screen.snapshot_jpeg().await?)
        } else {
            None
        };
        let text = if self.detector.needs_text() {
            let region = self.settings.regions.first().copied();
            Some(self.screen.ocr_text(&self.settings.ocr_langs, region).await?)
        } else {
            None
        };
        Ok(Sample {
            ts: now_ts(),
            jpeg,
            text,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfy::RecordingNotifier;
    use crate::screen::testing::{solid_jpeg, FakeScreen};
    use crate::watcher::detectors::OcrDiffDetector;

    #[tokio::test]
    async fn detects_stores_and_notifies_with_cooldown() {
        let screen = Arc::new(FakeScreen::new(
            vec![solid_jpeg(64, 64, [0, 0, 0])],
            vec!["quiet".into(), "quiet".into(), "New message from Anna".into(), "Another new line here".into()],
        ));
        let notifier = Arc::new(RecordingNotifier::default());
        let storage = Storage::open_in_memory().unwrap();
        let mut svc = WatcherService {
            screen,
            detector: Box::new(OcrDiffDetector::new(0.05, vec![])),
            notifier: Some(notifier.clone()),
            storage: storage.clone(),
            settings: WatcherSettings {
                cooldown_secs: 3600,
                ..Default::default()
            },
            host: "pikvm".into(),
            title_template: "{host}: {reason}".into(),
            attach_snapshot: true,
        };
        let mut last = None;
        assert!(svc.step(&mut last).await.unwrap().is_none()); // first sample: baseline
        assert!(svc.step(&mut last).await.unwrap().is_none()); // unchanged
        let ev = svc.step(&mut last).await.unwrap().expect("change");
        assert!(ev.notified);
        assert!(ev.thumbnail.is_some());
        assert!(svc.step(&mut last).await.unwrap().is_none(), "cooldown suppresses");
        let sent = notifier.sent.lock().await;
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].title, "pikvm: New text on screen");
        assert!(sent[0].attachment.is_some());
        assert_eq!(storage.watch_events(10).unwrap().len(), 1);
    }
}
