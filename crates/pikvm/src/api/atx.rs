//! `/api/atx/*`

use crate::client::{PikvmClient, Query};
use crate::error::Result;
use crate::models::{AtxButton, AtxState, PowerAction};

pub struct AtxApi<'a>(pub(crate) &'a PikvmClient);

impl AtxApi<'_> {
    /// `GET /api/atx`
    pub async fn state(&self) -> Result<AtxState> {
        self.0.get_result("/api/atx", Query::new()).await
    }

    /// `POST /api/atx/power?action=&wait=`
    pub async fn power(&self, action: PowerAction, wait: bool) -> Result<()> {
        let q = Query::new().push("action", action.as_str()).flag("wait", wait);
        self.0.post_ok("/api/atx/power", q).await
    }

    /// `POST /api/atx/click?button=&wait=`
    pub async fn click(&self, button: AtxButton, wait: bool) -> Result<()> {
        let q = Query::new().push("button", button.as_str()).flag("wait", wait);
        self.0.post_ok("/api/atx/click", q).await
    }
}
