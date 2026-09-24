//! Pluggable change detectors. Each looks at a [`Sample`] and reports a [`Change`] when the
//! screen changed in a way worth notifying about.

use async_trait::async_trait;
use bytes::Bytes;
use serde_json::Value;

use crate::ai::{ChatMessage, ChatRequest, ContentPart, SharedProvider};
use crate::settings::{DetectorKind, Region, WatcherSettings};

/// One observation of the screen.
#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub ts: i64,
    pub jpeg: Option<Bytes>,
    pub text: Option<String>,
}

/// A detected change.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Short reason, e.g. "New text on screen".
    pub reason: String,
    /// Longer detail (new lines, AI explanation).
    pub detail: String,
    /// 0..1 confidence / magnitude.
    pub score: f64,
}

#[async_trait]
pub trait ChangeDetector: Send + Sync {
    async fn observe(&mut self, sample: &Sample) -> anyhow::Result<Option<Change>>;
    fn needs_text(&self) -> bool;
    fn needs_image(&self) -> bool;
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------------------

/// Compares normalised OCR lines with the previous sample.
pub struct OcrDiffDetector {
    prev: Option<Vec<String>>,
    threshold: f64,
    ignore: Vec<String>,
}

impl OcrDiffDetector {
    pub fn new(threshold: f64, ignore: Vec<String>) -> Self {
        Self {
            prev: None,
            threshold,
            ignore: ignore.into_iter().map(|s| s.to_lowercase()).collect(),
        }
    }

    /// Lower-case, trimmed, deduplicated lines with ignored phrases and clock-like lines removed.
    pub fn normalise(&self, text: &str) -> Vec<String> {
        let mut lines: Vec<String> = text
            .lines()
            .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase())
            .filter(|l| l.len() >= 3)
            .filter(|l| !looks_like_clock(l))
            .filter(|l| !self.ignore.iter().any(|ig| !ig.is_empty() && l.contains(ig)))
            .collect();
        lines.sort();
        lines.dedup();
        lines
    }
}

/// `12:34`, `9:05 pm` and similar lines change every minute and are not interesting.
pub fn looks_like_clock(line: &str) -> bool {
    let core = line.trim_end_matches(" am").trim_end_matches(" pm");
    let Some((h, m)) = core.split_once(':') else {
        return false;
    };
    h.len() <= 2 && h.chars().all(|c| c.is_ascii_digit()) && m.len() == 2 && m.chars().all(|c| c.is_ascii_digit())
}

#[async_trait]
impl ChangeDetector for OcrDiffDetector {
    async fn observe(&mut self, sample: &Sample) -> anyhow::Result<Option<Change>> {
        let Some(text) = &sample.text else { return Ok(None) };
        let lines = self.normalise(text);
        let result = match &self.prev {
            None => None,
            Some(prev) => {
                let new: Vec<&String> = lines.iter().filter(|l| !prev.contains(l)).collect();
                let denom = lines.len().max(prev.len()).max(1) as f64;
                let score = new.len() as f64 / denom;
                if !new.is_empty() && score >= self.threshold {
                    let detail = new.iter().take(12).map(|s| s.as_str()).collect::<Vec<_>>().join("\n");
                    Some(Change {
                        reason: "New text on screen".into(),
                        detail,
                        score: score.min(1.0),
                    })
                } else {
                    None
                }
            }
        };
        self.prev = Some(lines);
        Ok(result)
    }

    fn needs_text(&self) -> bool {
        true
    }

    fn needs_image(&self) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "ocr_diff"
    }
}

// ---------------------------------------------------------------------------------------

/// Compares a heavily downscaled grayscale version of the frame (or regions of it).
pub struct PixelDiffDetector {
    prev: Option<Vec<u8>>,
    threshold: f64,
    regions: Vec<Region>,
}

const THUMB_W: u32 = 96;
const THUMB_H: u32 = 54;
const PIXEL_DELTA: i16 = 24;

impl PixelDiffDetector {
    pub fn new(threshold: f64, regions: Vec<Region>) -> Self {
        Self {
            prev: None,
            threshold,
            regions,
        }
    }

    /// Grayscale fingerprint of the frame restricted to the regions.
    pub fn fingerprint(&self, jpeg: &[u8]) -> anyhow::Result<Vec<u8>> {
        let img = image::load_from_memory(jpeg)?;
        let mut out = Vec::new();
        if self.regions.is_empty() {
            let g = img
                .resize_exact(THUMB_W, THUMB_H, image::imageops::FilterType::Triangle)
                .to_luma8();
            out.extend_from_slice(g.as_raw());
        } else {
            for r in &self.regions {
                let (w, h) = (img.width(), img.height());
                let left = r.left.min(w.saturating_sub(1));
                let top = r.top.min(h.saturating_sub(1));
                let width = r.width.min(w - left).max(1);
                let height = r.height.min(h - top).max(1);
                let g = img
                    .crop_imm(left, top, width, height)
                    .resize_exact(THUMB_W / 2, THUMB_H / 2, image::imageops::FilterType::Triangle)
                    .to_luma8();
                out.extend_from_slice(g.as_raw());
            }
        }
        Ok(out)
    }

    pub fn changed_fraction(a: &[u8], b: &[u8]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 1.0;
        }
        let changed = a
            .iter()
            .zip(b)
            .filter(|(x, y)| (i16::from(**x) - i16::from(**y)).abs() > PIXEL_DELTA)
            .count();
        changed as f64 / a.len() as f64
    }
}

#[async_trait]
impl ChangeDetector for PixelDiffDetector {
    async fn observe(&mut self, sample: &Sample) -> anyhow::Result<Option<Change>> {
        let Some(jpeg) = &sample.jpeg else { return Ok(None) };
        let fp = self.fingerprint(jpeg)?;
        let result = match &self.prev {
            None => None,
            Some(prev) => {
                let frac = Self::changed_fraction(prev, &fp);
                (frac >= self.threshold).then(|| Change {
                    reason: "Screen changed".into(),
                    detail: format!("{:.1}% of the watched area changed", frac * 100.0),
                    score: frac.min(1.0),
                })
            }
        };
        self.prev = Some(fp);
        Ok(result)
    }

    fn needs_text(&self) -> bool {
        false
    }

    fn needs_image(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "pixel_diff"
    }
}

// ---------------------------------------------------------------------------------------

/// Asks a vision model whether a notification-like change happened between two frames.
pub struct AiClassifierDetector {
    provider: SharedProvider,
    prev: Option<Bytes>,
}

pub const CLASSIFIER_PROMPT: &str = "You compare two screenshots of the same work computer: the first is BEFORE, the second is AFTER. \
Decide whether something the user would want to be notified about appeared: a new chat or e-mail message, \
a toast/pop-up notification, an unread badge or counter increasing, a dialog asking for input, an error, or a finished job. \
Ignore clock changes, cursor movement, video playback and scrolling by the user. \
Answer with strict JSON only: {\"changed\": true|false, \"reason\": \"<short reason>\", \"detail\": \"<what is new, quote visible text>\"}";

impl AiClassifierDetector {
    pub fn new(provider: SharedProvider) -> Self {
        Self { provider, prev: None }
    }

    /// Ask the model about `before` → `after`.
    pub async fn classify(&self, before: &Bytes, after: &Bytes) -> anyhow::Result<Option<Change>> {
        let mut req = ChatRequest::new("You are a precise screen-change classifier. Output JSON only.");
        req.max_tokens = 600;
        req.messages.push(ChatMessage::user(vec![
            ContentPart::text("BEFORE:"),
            ContentPart::jpeg(before.clone()),
            ContentPart::text("AFTER:"),
            ContentPart::jpeg(after.clone()),
            ContentPart::text(CLASSIFIER_PROMPT),
        ]));
        let resp = self.provider.chat(&req).await?;
        crate::ai::ensure_not_refused(&resp)?;
        Ok(parse_classification(&resp.text()))
    }
}

/// Extract `{changed, reason, detail}` from a model answer that may contain prose around JSON.
pub fn parse_classification(text: &str) -> Option<Change> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    let v: Value = serde_json::from_str(&text[start..=end]).ok()?;
    if !v["changed"].as_bool().unwrap_or(false) {
        return None;
    }
    Some(Change {
        reason: v["reason"].as_str().unwrap_or("Change detected").to_string(),
        detail: v["detail"].as_str().unwrap_or_default().to_string(),
        score: 1.0,
    })
}

#[async_trait]
impl ChangeDetector for AiClassifierDetector {
    async fn observe(&mut self, sample: &Sample) -> anyhow::Result<Option<Change>> {
        let Some(jpeg) = &sample.jpeg else { return Ok(None) };
        let result = match &self.prev {
            None => None,
            Some(prev) => self.classify(prev, jpeg).await?,
        };
        self.prev = Some(jpeg.clone());
        Ok(result)
    }

    fn needs_text(&self) -> bool {
        false
    }

    fn needs_image(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "ai_classifier"
    }
}

// ---------------------------------------------------------------------------------------

/// A cheap detector triggers, the AI confirms (and explains).
pub struct CombinedDetector {
    trigger: Box<dyn ChangeDetector>,
    confirm: AiClassifierDetector,
    prev_confirmed: Option<Bytes>,
}

impl CombinedDetector {
    pub fn new(trigger: Box<dyn ChangeDetector>, provider: SharedProvider) -> Self {
        Self {
            trigger,
            confirm: AiClassifierDetector::new(provider),
            prev_confirmed: None,
        }
    }
}

#[async_trait]
impl ChangeDetector for CombinedDetector {
    async fn observe(&mut self, sample: &Sample) -> anyhow::Result<Option<Change>> {
        let triggered = self.trigger.observe(sample).await?;
        let Some(jpeg) = &sample.jpeg else { return Ok(None) };
        let result = match (triggered, &self.prev_confirmed) {
            (Some(_), Some(prev)) => self.confirm.classify(prev, jpeg).await?,
            _ => None,
        };
        if self.prev_confirmed.is_none() || result.is_some() {
            self.prev_confirmed = Some(jpeg.clone());
        }
        Ok(result)
    }

    fn needs_text(&self) -> bool {
        self.trigger.needs_text()
    }

    fn needs_image(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "combined"
    }
}

/// Build the detector selected in settings. `vision` is required for AI based kinds.
pub fn build_detector(
    settings: &WatcherSettings,
    vision: Option<SharedProvider>,
) -> anyhow::Result<Box<dyn ChangeDetector>> {
    let ocr = || OcrDiffDetector::new(settings.text_threshold, settings.ignore_phrases.clone());
    let pixel = || PixelDiffDetector::new(settings.pixel_threshold, settings.regions.clone());
    Ok(match settings.detector {
        DetectorKind::OcrDiff => Box::new(ocr()),
        DetectorKind::PixelDiff => Box::new(pixel()),
        DetectorKind::AiClassifier => {
            Box::new(AiClassifierDetector::new(vision.ok_or_else(|| {
                anyhow::anyhow!("the AI classifier needs a vision provider")
            })?))
        }
        DetectorKind::Combined => Box::new(CombinedDetector::new(
            Box::new(pixel()),
            vision.ok_or_else(|| anyhow::anyhow!("the combined detector needs a vision provider"))?,
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen::testing::solid_jpeg;

    #[tokio::test]
    async fn ocr_diff_reports_new_lines_and_ignores_clock() {
        let mut d = OcrDiffDetector::new(0.05, vec!["battery".into()]);
        let s1 = Sample {
            text: Some("Inbox\n12:31\nBattery 80%\nHello team".into()),
            ..Default::default()
        };
        assert!(d.observe(&s1).await.unwrap().is_none());
        let s2 = Sample {
            text: Some("Inbox\n12:32\nBattery 79%\nHello team".into()),
            ..Default::default()
        };
        assert!(
            d.observe(&s2).await.unwrap().is_none(),
            "clock and ignored phrase are not changes"
        );
        let s3 = Sample {
            text: Some("Inbox\n12:33\nAnna: are you joining?\nHello team".into()),
            ..Default::default()
        };
        let c = d.observe(&s3).await.unwrap().expect("change");
        assert!(c.detail.contains("anna: are you joining?"));
    }

    #[tokio::test]
    async fn pixel_diff_detects_colour_change() {
        let mut d = PixelDiffDetector::new(0.01, vec![]);
        let a = Sample {
            jpeg: Some(solid_jpeg(320, 180, [20, 20, 20])),
            ..Default::default()
        };
        let b = Sample {
            jpeg: Some(solid_jpeg(320, 180, [200, 200, 200])),
            ..Default::default()
        };
        assert!(d.observe(&a).await.unwrap().is_none());
        assert!(d.observe(&a).await.unwrap().is_none());
        assert!(d.observe(&b).await.unwrap().is_some());
    }

    #[test]
    fn classification_json_is_parsed_leniently() {
        assert!(parse_classification("Sure: {\"changed\": false}").is_none());
        let c =
            parse_classification("{\"changed\": true, \"reason\": \"New Teams message\", \"detail\": \"Anna: hi\"}")
                .unwrap();
        assert_eq!(c.reason, "New Teams message");
        assert!(looks_like_clock("12:34"));
        assert!(!looks_like_clock("meeting at noon"));
    }
}
