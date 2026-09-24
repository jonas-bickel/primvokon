//! Capture loop: every interval, turn the screen into text and store it (deduplicated).

use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

use crate::screen::{ScreenSource, TextExtractor};
use crate::storage::{now_ts, Storage};

#[derive(Debug, Clone)]
pub enum RecallEvent {
    Started,
    Captured { id: i64, day: String, chars: usize },
    Duplicate,
    Error(String),
    Stopped,
}

#[derive(Clone)]
pub struct CaptureHandle {
    cancel: CancellationToken,
}

impl CaptureHandle {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.cancel.is_cancelled()
    }
}

pub struct CaptureService {
    pub screen: Arc<dyn ScreenSource>,
    pub extractor: Box<dyn TextExtractor>,
    pub storage: Storage,
    pub interval: Duration,
}

/// Stable hash of the text after whitespace normalisation.
pub fn text_hash(text: &str) -> String {
    let normalised: String = text.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let digest = Sha256::digest(normalised.as_bytes());
    format!("{digest:x}")
}

impl CaptureService {
    pub fn start(self) -> (CaptureHandle, mpsc::Receiver<RecallEvent>) {
        let (tx, rx) = mpsc::channel(64);
        let cancel = CancellationToken::new();
        let handle = CaptureHandle {
            cancel: cancel.clone(),
        };
        tokio::spawn(self.run(tx, cancel));
        (handle, rx)
    }

    async fn run(self, tx: mpsc::Sender<RecallEvent>, cancel: CancellationToken) {
        let _ = tx.send(RecallEvent::Started).await;
        loop {
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = tokio::time::sleep(self.interval) => {}
            }
            let ev = match self.capture_once().await {
                Ok(ev) => ev,
                Err(e) => RecallEvent::Error(e.to_string()),
            };
            let _ = tx.send(ev).await;
        }
        let _ = tx.send(RecallEvent::Stopped).await;
    }

    /// One capture; returns `Duplicate` when the text did not change.
    pub async fn capture_once(&self) -> anyhow::Result<RecallEvent> {
        let text = self.extractor.extract(self.screen.as_ref()).await?;
        let text = text.trim().to_string();
        if text.is_empty() {
            return Ok(RecallEvent::Duplicate);
        }
        let hash = text_hash(&text);
        if self.storage.last_capture_hash()? == Some(hash.clone()) {
            return Ok(RecallEvent::Duplicate);
        }
        let ts = now_ts();
        let id = self.storage.insert_capture(ts, &text, &hash, self.extractor.name())?;
        Ok(RecallEvent::Captured {
            id,
            day: crate::storage::day_of(ts),
            chars: text.chars().count(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::testing::FakeScreen;
    use crate::screen::OcrExtractor;

    #[tokio::test]
    async fn deduplicates_consecutive_captures() {
        let screen = Arc::new(FakeScreen::new(vec![], vec!["A  b".into(), "a B".into(), "c".into()]));
        let svc = CaptureService {
            screen,
            extractor: Box::new(OcrExtractor { langs: vec![] }),
            storage: Storage::open_in_memory().unwrap(),
            interval: Duration::from_secs(60),
        };
        assert!(matches!(svc.capture_once().await.unwrap(), RecallEvent::Captured { .. }));
        assert!(matches!(svc.capture_once().await.unwrap(), RecallEvent::Duplicate));
        assert!(matches!(svc.capture_once().await.unwrap(), RecallEvent::Captured { .. }));
        assert_eq!(text_hash("A  b"), text_hash("a b"));
    }
}
