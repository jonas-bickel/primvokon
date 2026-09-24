//! `/api/switch/*` – PiKVM Switch

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;
use crate::models::{AtxButton, BeaconTarget, PowerAction, SwitchPortParams, SwitchState};

pub struct SwitchApi<'a>(pub(crate) &'a PikvmClient);

fn fmt_port(port: f64) -> String {
    if port.fract() == 0.0 {
        format!("{}", port as i64)
    } else {
        format!("{port}")
    }
}

impl SwitchApi<'_> {
    /// `GET /api/switch`
    pub async fn state(&self) -> Result<SwitchState> {
        self.0.get_result("/api/switch", Query::new()).await
    }

    /// `POST /api/switch/set_active_prev`
    pub async fn set_active_prev(&self) -> Result<()> {
        self.0.post_ok("/api/switch/set_active_prev", Query::new()).await
    }

    /// `POST /api/switch/set_active_next`
    pub async fn set_active_next(&self) -> Result<()> {
        self.0.post_ok("/api/switch/set_active_next", Query::new()).await
    }

    /// `POST /api/switch/set_active?port=` – continuous (`0..19`) or `unit.port` float numbering.
    pub async fn set_active(&self, port: f64) -> Result<()> {
        self.0.post_ok("/api/switch/set_active", Query::new().push("port", fmt_port(port))).await
    }

    /// `POST /api/switch/set_beacon?state=&port=|uplink=|downlink=`
    pub async fn set_beacon(&self, target: BeaconTarget, state: bool) -> Result<()> {
        let q = Query::new().flag("state", state);
        let q = match target {
            BeaconTarget::Port(p) => q.push("port", fmt_port(p)),
            BeaconTarget::Uplink(u) => q.push("uplink", u),
            BeaconTarget::Downlink(d) => q.push("downlink", d),
        };
        self.0.post_ok("/api/switch/set_beacon", q).await
    }

    /// `POST /api/switch/set_port_params?port=&…`
    pub async fn set_port_params(&self, port: f64, p: &SwitchPortParams) -> Result<()> {
        let q = Query::new()
            .push("port", fmt_port(port))
            .opt("edid_id", p.edid_id.as_deref())
            .opt_flag("dummy", p.dummy)
            .opt("name", p.name.as_deref())
            .opt("atx_click_power_delay", p.atx_click_power_delay)
            .opt("atx_click_power_long_delay", p.atx_click_power_long_delay)
            .opt("atx_click_reset_delay", p.atx_click_reset_delay);
        self.0.post_ok("/api/switch/set_port_params", q).await
    }

    /// `POST /api/switch/set_colors` with body `RRGGBB:BRIGHTNESS:BLINK_MS_HEX`, e.g. `FFA500:BF:0028`.
    pub async fn set_colors(&self, beacon: &str) -> Result<()> {
        let _: serde_json::Value = self
            .0
            .post_result("/api/switch/set_colors", Query::new().push("beacon", beacon), Body::Text(beacon.to_string()))
            .await?;
        Ok(())
    }

    /// `POST /api/switch/reset?unit=&bootloader=`
    pub async fn reset(&self, unit: u32, bootloader: bool) -> Result<()> {
        let q = Query::new().push("unit", unit).flag("bootloader", bootloader);
        self.0.post_ok("/api/switch/reset", q).await
    }

    /// `POST /api/switch/edids/create?name=&data=`
    pub async fn edid_create(&self, name: &str, data_hex: &str) -> Result<()> {
        let q = Query::new().push("name", name).push("data", data_hex);
        self.0.post_ok("/api/switch/edids/create", q).await
    }

    /// `POST /api/switch/edids/change?id=&name=&data=`
    pub async fn edid_change(&self, id: &str, name: Option<&str>, data_hex: Option<&str>) -> Result<()> {
        let q = Query::new().push("id", id).opt("name", name).opt("data", data_hex);
        self.0.post_ok("/api/switch/edids/change", q).await
    }

    /// `POST /api/switch/edids/remove?id=`
    pub async fn edid_remove(&self, id: &str) -> Result<()> {
        self.0.post_ok("/api/switch/edids/remove", Query::new().push("id", id)).await
    }

    /// `POST /api/switch/atx/power?port=&action=`
    pub async fn atx_power(&self, port: f64, action: PowerAction) -> Result<()> {
        let q = Query::new().push("port", fmt_port(port)).push("action", action.as_str());
        self.0.post_ok("/api/switch/atx/power", q).await
    }

    /// `POST /api/switch/atx/click?port=&button=`
    pub async fn atx_click(&self, port: f64, button: AtxButton) -> Result<()> {
        let q = Query::new().push("port", fmt_port(port)).push("button", button.as_str());
        self.0.post_ok("/api/switch/atx/click", q).await
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_port;

    #[test]
    fn formats_ports() {
        assert_eq!(fmt_port(2.0), "2");
        assert_eq!(fmt_port(2.2), "2.2");
    }
}
