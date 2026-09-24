//! Miscellaneous endpoints.

use reqwest::Method;

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;

pub struct MiscApi<'a>(pub(crate) &'a PikvmClient);

impl MiscApi<'_> {
    /// `GET /api/export/prometheus/metrics` – raw Prometheus exposition text.
    pub async fn prometheus_metrics(&self) -> Result<String> {
        let r = self
            .0
            .call_ok(
                Method::GET,
                "/api/export/prometheus/metrics",
                &Query::new(),
                Body::Empty,
            )
            .await?;
        Ok(r.text())
    }
}
