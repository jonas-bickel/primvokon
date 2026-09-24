//! Access to the host screen through PiKVM and strategies that turn it into text.

use async_trait::async_trait;
use bytes::Bytes;
use pikvm::models::{OcrOptions, OcrRegion, SnapshotOptions};
use pikvm::PikvmClient;

use crate::ai::{LlmProvider, SharedProvider};
use crate::settings::Region;

/// Something that can capture the host screen.
#[async_trait]
pub trait ScreenSource: Send + Sync {
    /// Full-resolution JPEG of the current frame.
    async fn snapshot_jpeg(&self) -> anyhow::Result<Bytes>;
    /// Text recognised by PiKVM's OCR, optionally restricted to a region.
    async fn ocr_text(&self, langs: &[String], region: Option<Region>) -> anyhow::Result<String>;
}

/// [`ScreenSource`] backed by a PiKVM.
#[derive(Clone)]
pub struct PikvmScreen {
    client: PikvmClient,
}

impl PikvmScreen {
    pub fn new(client: PikvmClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl ScreenSource for PikvmScreen {
    async fn snapshot_jpeg(&self) -> anyhow::Result<Bytes> {
        let opts = SnapshotOptions {
            allow_offline: false,
            ..Default::default()
        };
        Ok(self.client.streamer().snapshot(&opts).await?)
    }

    async fn ocr_text(&self, langs: &[String], region: Option<Region>) -> anyhow::Result<String> {
        let opts = OcrOptions {
            langs: langs.to_vec(),
            region: region.map(|r| OcrRegion {
                left: r.left,
                top: r.top,
                right: r.left + r.width,
                bottom: r.top + r.height,
            }),
            allow_offline: false,
        };
        Ok(self.client.streamer().snapshot_ocr(&opts).await?)
    }
}

/// Strategy that produces text from the screen.
#[async_trait]
pub trait TextExtractor: Send + Sync {
    async fn extract(&self, screen: &dyn ScreenSource) -> anyhow::Result<String>;
    fn name(&self) -> &'static str;
}

/// PiKVM Tesseract OCR.
pub struct OcrExtractor {
    pub langs: Vec<String>,
}

#[async_trait]
impl TextExtractor for OcrExtractor {
    async fn extract(&self, screen: &dyn ScreenSource) -> anyhow::Result<String> {
        screen.ocr_text(&self.langs, None).await
    }

    fn name(&self) -> &'static str {
        "ocr"
    }
}

/// Vision model description of the screen.
pub struct VisionExtractor {
    pub provider: SharedProvider,
}

pub const VISION_PROMPT: &str = "Describe everything visible on this screenshot of a work computer as structured text: \
open applications and windows, document or page titles, chat or e-mail messages (sender, gist), notifications, \
and any visible task lists. Transcribe important text verbatim. Be complete but do not speculate.";

#[async_trait]
impl TextExtractor for VisionExtractor {
    async fn extract(&self, screen: &dyn ScreenSource) -> anyhow::Result<String> {
        let jpeg = screen.snapshot_jpeg().await?;
        self.provider.describe_image(&jpeg, VISION_PROMPT).await
    }

    fn name(&self) -> &'static str {
        "vision"
    }
}

/// Crop a JPEG to a region (used by the pixel detector and notification thumbnails).
pub fn crop_jpeg(jpeg: &[u8], region: Region) -> anyhow::Result<image::DynamicImage> {
    let img = image::load_from_memory(jpeg)?;
    let (w, h) = (img.width(), img.height());
    let left = region.left.min(w.saturating_sub(1));
    let top = region.top.min(h.saturating_sub(1));
    let width = region.width.min(w - left).max(1);
    let height = region.height.min(h - top).max(1);
    Ok(img.crop_imm(left, top, width, height))
}

/// Downscale a JPEG into a small JPEG thumbnail.
pub fn thumbnail_jpeg(jpeg: &[u8], max_width: u32) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory(jpeg)?;
    let thumb = img.thumbnail(max_width, max_width);
    let mut out = std::io::Cursor::new(Vec::new());
    thumb.write_to(&mut out, image::ImageFormat::Jpeg)?;
    Ok(out.into_inner())
}

/// Convenience for the "read screen" agent tool.
pub async fn read_screen_text(
    screen: &dyn ScreenSource,
    langs: &[String],
    vision: Option<&dyn LlmProvider>,
) -> anyhow::Result<String> {
    match vision {
        Some(p) => {
            let jpeg = screen.snapshot_jpeg().await?;
            p.describe_image(&jpeg, VISION_PROMPT).await
        }
        None => screen.ocr_text(langs, None).await,
    }
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use tokio::sync::Mutex;

    /// Scripted screen for tests: pops frames/texts in order, repeats the last one.
    pub struct FakeScreen {
        pub frames: Mutex<Vec<Bytes>>,
        pub texts: Mutex<Vec<String>>,
    }

    impl FakeScreen {
        pub fn new(frames: Vec<Bytes>, texts: Vec<String>) -> Self {
            Self {
                frames: Mutex::new(frames),
                texts: Mutex::new(texts),
            }
        }
    }

    fn pop<T: Clone>(v: &mut Vec<T>) -> Option<T> {
        if v.len() > 1 {
            Some(v.remove(0))
        } else {
            v.first().cloned()
        }
    }

    #[async_trait]
    impl ScreenSource for FakeScreen {
        async fn snapshot_jpeg(&self) -> anyhow::Result<Bytes> {
            pop(&mut *self.frames.lock().await).ok_or_else(|| anyhow::anyhow!("no frame"))
        }

        async fn ocr_text(&self, _langs: &[String], _region: Option<Region>) -> anyhow::Result<String> {
            pop(&mut *self.texts.lock().await).ok_or_else(|| anyhow::anyhow!("no text"))
        }
    }

    /// Encode a solid-colour JPEG for tests.
    pub fn solid_jpeg(w: u32, h: u32, rgb: [u8; 3]) -> Bytes {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        Bytes::from(out.into_inner())
    }
}
