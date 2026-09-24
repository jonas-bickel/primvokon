//! `/api/info` and `/api/log`

use futures_util::{Stream, StreamExt};
use reqwest::Method;

use crate::client::{Body, PikvmClient, Query};
use crate::error::{PikvmError, Result};
use crate::models::Info;

pub struct SystemApi<'a>(pub(crate) &'a PikvmClient);

/// Categories accepted by `GET /api/info?fields=`.
pub const INFO_FIELDS: [&str; 6] = ["auth", "extras", "fan", "hw", "meta", "system"];

impl SystemApi<'_> {
    /// `GET /api/info` with optional `fields` filter.
    pub async fn info(&self, fields: &[&str]) -> Result<Info> {
        let q = if fields.is_empty() {
            Query::new()
        } else {
            Query::new().push("fields", fields.join(","))
        };
        self.0.get_result("/api/info", q).await
    }

    /// `GET /api/log?seek=` – plain-text log of the last `seek` seconds.
    pub async fn log(&self, seek_seconds: Option<u64>) -> Result<String> {
        let q = Query::new().opt("seek", seek_seconds);
        let resp = self.0.call_ok(Method::GET, "/api/log", &q, Body::Empty).await?;
        Ok(resp.text())
    }

    /// `GET /api/log?follow=1` – stream of log chunks until the stream is dropped.
    pub async fn log_follow(&self, seek_seconds: Option<u64>) -> Result<impl Stream<Item = Result<String>>> {
        let mut url = self.0.url("/api/log");
        let mut q = vec![("follow", "1".to_string())];
        if let Some(s) = seek_seconds {
            q.push(("seek", s.to_string()));
        }
        url.query_pairs_mut().extend_pairs(q);
        let resp = self.0.stream_request(url).await?.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(crate::client::map_status(status, body));
        }
        Ok(resp.bytes_stream().map(|chunk| {
            chunk
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .map_err(PikvmError::from)
        }))
    }
}
