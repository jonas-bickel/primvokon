//! `/api/gpio/*`

use crate::client::{PikvmClient, Query};
use crate::error::Result;
use crate::models::GpioFull;

pub struct GpioApi<'a>(pub(crate) &'a PikvmClient);

impl GpioApi<'_> {
    /// `GET /api/gpio`
    pub async fn state(&self) -> Result<GpioFull> {
        self.0.get_result("/api/gpio", Query::new()).await
    }

    /// `POST /api/gpio/switch?channel=&state=&wait=`
    pub async fn switch(&self, channel: &str, state: bool, wait: bool) -> Result<()> {
        let q = Query::new().push("channel", channel).flag("state", state).flag("wait", wait);
        self.0.post_ok("/api/gpio/switch", q).await
    }

    /// `POST /api/gpio/pulse?channel=&delay=&wait=` – `delay` `0.0` uses the channel default.
    pub async fn pulse(&self, channel: &str, delay: Option<f64>, wait: bool) -> Result<()> {
        let q = Query::new().push("channel", channel).opt("delay", delay).flag("wait", wait);
        self.0.post_ok("/api/gpio/pulse", q).await
    }
}
