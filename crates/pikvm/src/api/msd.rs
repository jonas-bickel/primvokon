//! `/api/msd/*`

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures_util::StreamExt;
use reqwest::Method;
use tokio::io::AsyncReadExt;

use crate::client::{Body, PikvmClient, Query};
use crate::error::{PikvmError, Result};
use crate::models::MsdState;

pub struct MsdApi<'a>(pub(crate) &'a PikvmClient);

/// Progress callback `(bytes_sent, total_bytes)`.
pub type Progress = Arc<dyn Fn(u64, u64) + Send + Sync>;

impl MsdApi<'_> {
    /// `GET /api/msd`
    pub async fn state(&self) -> Result<MsdState> {
        self.0.get_result("/api/msd", Query::new()).await
    }

    /// `POST /api/msd/write?image=` – upload raw bytes as `image_name`.
    pub async fn write_bytes(&self, image_name: &str, data: Bytes) -> Result<()> {
        let q = Query::new().push("image", image_name);
        let _: serde_json::Value = self.0.post_result("/api/msd/write", q, Body::Bytes(data)).await?;
        Ok(())
    }

    /// `POST /api/msd/write?image=` – stream a local file with progress reporting.
    pub async fn write_file(&self, path: &Path, image_name: Option<&str>, progress: Option<Progress>) -> Result<()> {
        let name = image_name
            .map(str::to_string)
            .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
            .ok_or_else(|| PikvmError::Decode("image name required".into()))?;
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|e| PikvmError::Stream(e.to_string()))?;
        let total = file.metadata().await.map(|m| m.len()).unwrap_or(0);
        let sent = Arc::new(AtomicU64::new(0));
        let sent_clone = sent.clone();
        let stream = futures_util::stream::unfold(file, |mut f| async move {
            let mut buf = vec![0u8; 1 << 20];
            match f.read(&mut buf).await {
                Ok(0) => None,
                Ok(n) => {
                    buf.truncate(n);
                    Some((Ok::<Bytes, std::io::Error>(Bytes::from(buf)), f))
                }
                Err(e) => Some((Err(e), f)),
            }
        })
        .inspect(move |chunk| {
            if let Ok(b) = chunk {
                let done = sent_clone.fetch_add(b.len() as u64, Ordering::Relaxed) + b.len() as u64;
                if let Some(cb) = &progress {
                    cb(done, total);
                }
            }
        });
        let mut url = self.0.url("/api/msd/write");
        url.query_pairs_mut().append_pair("image", &name);
        self.0.ensure_authenticated().await?;
        let mut rb = self.0.stream_request(url).await?;
        rb = rb.header(reqwest::header::CONTENT_LENGTH, total);
        let req = rb.build()?;
        let mut req = req;
        *req.method_mut() = Method::POST;
        *req.body_mut() = Some(reqwest::Body::wrap_stream(stream));
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(self.0.config().accept_invalid_certs)
            .danger_accept_invalid_hostnames(self.0.config().accept_invalid_certs)
            .build()?;
        let resp = http.execute(req).await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(crate::client::map_status(status, body));
        }
        Ok(())
    }

    /// `POST /api/msd/write_remote?url=&image=&timeout=` – long-polling download on the PiKVM.
    /// The call returns when the download finished; progress arrives via `msd_state` events.
    pub async fn write_remote(&self, url: &str, image_name: Option<&str>, timeout_secs: Option<u32>) -> Result<String> {
        let q = Query::new()
            .push("url", url)
            .opt("image", image_name)
            .opt("timeout", timeout_secs);
        let mut api_url = self.0.url("/api/msd/write_remote");
        api_url
            .query_pairs_mut()
            .extend_pairs(q.0.iter().map(|(k, v)| (*k, v.as_str())));
        let mut rb = self.0.stream_request(api_url).await?;
        rb = rb.header(reqwest::header::CONTENT_LENGTH, 0);
        let req = {
            let mut r = rb.build()?;
            *r.method_mut() = Method::POST;
            r
        };
        let http = reqwest::Client::builder()
            .danger_accept_invalid_certs(self.0.config().accept_invalid_certs)
            .danger_accept_invalid_hostnames(self.0.config().accept_invalid_certs)
            .build()?;
        let resp = http.execute(req).await?;
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(crate::client::map_status(status, body));
        }
        Ok(body)
    }

    /// `POST /api/msd/set_params?image=&cdrom=&rw=`
    pub async fn set_params(&self, image: Option<&str>, cdrom: Option<bool>, rw: Option<bool>) -> Result<()> {
        let q = Query::new()
            .opt("image", image)
            .opt_flag("cdrom", cdrom)
            .opt_flag("rw", rw);
        self.0.post_ok("/api/msd/set_params", q).await
    }

    /// `POST /api/msd/set_connected?connected=`
    pub async fn set_connected(&self, connected: bool) -> Result<()> {
        self.0
            .post_ok("/api/msd/set_connected", Query::new().flag("connected", connected))
            .await
    }

    /// `POST /api/msd/remove?image=`
    pub async fn remove(&self, image: &str) -> Result<()> {
        self.0
            .post_ok("/api/msd/remove", Query::new().push("image", image))
            .await
    }

    /// `POST /api/msd/reset`
    pub async fn reset(&self) -> Result<()> {
        self.0.post_ok("/api/msd/reset", Query::new()).await
    }
}
