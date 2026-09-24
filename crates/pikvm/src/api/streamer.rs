//! `/api/streamer/*`

use bytes::Bytes;
use reqwest::Method;

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;
use crate::models::{OcrOptions, OcrResult, SnapshotOptions, StreamerParamsUpdate, StreamerState};

pub struct StreamerApi<'a>(pub(crate) &'a PikvmClient);

impl StreamerApi<'_> {
    /// `GET /api/streamer`
    pub async fn state(&self) -> Result<StreamerState> {
        self.0.get_result("/api/streamer", Query::new()).await
    }

    /// `GET /api/streamer/snapshot` – JPEG bytes of the current frame (or preview).
    pub async fn snapshot(&self, opts: &SnapshotOptions) -> Result<Bytes> {
        let q = Query::new()
            .flag("save", opts.save)
            .flag("load", opts.load)
            .flag("allow_offline", opts.allow_offline)
            .flag("preview", opts.preview)
            .opt("preview_max_width", opts.preview_max_width)
            .opt("preview_max_height", opts.preview_max_height)
            .opt("preview_quality", opts.preview_quality);
        let resp = self.0.call_ok(Method::GET, "/api/streamer/snapshot", &q, Body::Empty).await?;
        Ok(resp.body)
    }

    /// `GET /api/streamer/snapshot?ocr=1` – recognised text of the current frame.
    pub async fn snapshot_ocr(&self, opts: &OcrOptions) -> Result<String> {
        let mut q = Query::new().flag("ocr", true).flag("allow_offline", opts.allow_offline);
        if !opts.langs.is_empty() {
            q = q.push("ocr_langs", opts.langs.join(","));
        }
        if let Some(r) = opts.region {
            q = q
                .push("ocr_left", r.left)
                .push("ocr_top", r.top)
                .push("ocr_right", r.right)
                .push("ocr_bottom", r.bottom);
        }
        let resp = self.0.call_ok(Method::GET, "/api/streamer/snapshot", &q, Body::Empty).await?;
        Ok(resp.text())
    }

    /// `DELETE /api/streamer/snapshot`
    pub async fn delete_snapshot(&self) -> Result<()> {
        let resp = self.0.call(Method::DELETE, "/api/streamer/snapshot", &Query::new(), Body::Empty).await?;
        let _: serde_json::Value = crate::client::decode_envelope(resp)?;
        Ok(())
    }

    /// `GET /api/streamer/ocr`
    pub async fn ocr_state(&self) -> Result<OcrResult> {
        self.0.get_result("/api/streamer/ocr", Query::new()).await
    }

    /// `POST /api/streamer/set_params` – quality / fps / h264 tuning (present in kvmd, undocumented).
    pub async fn set_params(&self, p: &StreamerParamsUpdate) -> Result<()> {
        let q = Query::new()
            .opt("quality", p.quality)
            .opt("desired_fps", p.desired_fps)
            .opt("h264_bitrate", p.h264_bitrate)
            .opt("h264_gop", p.h264_gop);
        self.0.post_ok("/api/streamer/set_params", q).await
    }
}
