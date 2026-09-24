//! Endpoint catalogue: every documented PiKVM route with its category, importance and
//! parameters. It drives the API Explorer and keeps the UI's "core vs. advanced" split in one
//! place.

use std::collections::BTreeMap;

use reqwest::Method;
use serde_json::json;

use crate::client::{Body, PikvmClient, Query, RawResponse};
use crate::error::{PikvmError, Result};

/// Functional grouping shown in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Auth,
    System,
    Hid,
    Atx,
    Msd,
    Gpio,
    Streamer,
    Switch,
    Redfish,
    Misc,
}

impl Category {
    pub const ALL: [Category; 10] = [
        Category::Auth,
        Category::System,
        Category::Hid,
        Category::Atx,
        Category::Msd,
        Category::Gpio,
        Category::Streamer,
        Category::Switch,
        Category::Redfish,
        Category::Misc,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Category::Auth => "Authentication",
            Category::System => "System",
            Category::Hid => "Keyboard & mouse (HID)",
            Category::Atx => "ATX power",
            Category::Msd => "Mass storage (MSD)",
            Category::Gpio => "GPIO",
            Category::Streamer => "Streamer & OCR",
            Category::Switch => "PiKVM Switch",
            Category::Redfish => "Redfish",
            Category::Misc => "Miscellaneous",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Category::Auth => "dialog-password-symbolic",
            Category::System => "computer-symbolic",
            Category::Hid => "input-keyboard-symbolic",
            Category::Atx => "system-shutdown-symbolic",
            Category::Msd => "drive-removable-media-symbolic",
            Category::Gpio => "preferences-other-symbolic",
            Category::Streamer => "camera-video-symbolic",
            Category::Switch => "network-wired-symbolic",
            Category::Redfish => "network-server-symbolic",
            Category::Misc => "view-more-symbolic",
        }
    }
}

/// Default visibility of an endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Importance {
    /// Shown by default.
    Core,
    /// Behind a "Show advanced" switch.
    Advanced,
    /// Only reachable in the API Explorer.
    Explorer,
}

impl Importance {
    pub fn label(self) -> &'static str {
        match self {
            Importance::Core => "core",
            Importance::Advanced => "advanced",
            Importance::Explorer => "explorer",
        }
    }
}

/// Parameter value type for form generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamKind {
    Text,
    Int,
    Float,
    Bool,
    Enum(&'static [&'static str]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub kind: ParamKind,
    pub required: bool,
    pub description: &'static str,
}

/// Where the request body comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource {
    None,
    /// Plain text taken from the named parameter (not sent as a query value).
    TextParam(&'static str),
    /// JSON object `{key: <param value>}` from the named parameter.
    JsonParam(&'static str, &'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Endpoint {
    pub id: &'static str,
    pub category: Category,
    pub name: &'static str,
    pub method: &'static str,
    /// Route template; `{id}` placeholders are filled from parameters of the same name.
    pub route: &'static str,
    pub importance: Importance,
    pub description: &'static str,
    pub params: &'static [Param],
    pub body: BodySource,
    /// Ask for confirmation before running.
    pub destructive: bool,
}

const fn p(name: &'static str, kind: ParamKind, required: bool, description: &'static str) -> Param {
    Param {
        name,
        kind,
        required,
        description,
    }
}

const BOOL: ParamKind = ParamKind::Bool;
const INT: ParamKind = ParamKind::Int;
const FLOAT: ParamKind = ParamKind::Float;
const TEXT: ParamKind = ParamKind::Text;

macro_rules! ep {
    ($id:literal, $cat:ident, $name:literal, $method:literal, $route:literal, $imp:ident, $desc:literal, [$($param:expr),*] $(, body: $body:expr)? $(, destructive: $d:literal)?) => {
        Endpoint {
            id: $id,
            category: Category::$cat,
            name: $name,
            method: $method,
            route: $route,
            importance: Importance::$imp,
            description: $desc,
            params: &[$($param),*],
            body: ep!(@body $($body)?),
            destructive: ep!(@d $($d)?),
        }
    };
    (@body) => { BodySource::None };
    (@body $b:expr) => { $b };
    (@d) => { false };
    (@d $d:literal) => { $d };
}

/// Every documented endpoint.
pub static ENDPOINTS: &[Endpoint] = &[
    ep!("auth.login", Auth, "Login", "POST", "/api/auth/login", Core, "Obtain a session token (auth_token cookie).", [
        p("user", TEXT, true, "User name"), p("passwd", TEXT, true, "Password (+TOTP code with 2FA)")]),
    ep!("auth.check", Auth, "Check", "GET", "/api/auth/check", Core, "200 when authenticated, 401/403 otherwise.", []),
    ep!("auth.logout", Auth, "Logout", "POST", "/api/auth/logout", Advanced, "Invalidate the session token.", []),

    ep!("system.info", System, "System info", "GET", "/api/info", Core, "General information about the PiKVM device.", [
        p("fields", TEXT, false, "Comma separated: auth, extras, fan, hw, meta, system")]),
    ep!("system.log", System, "Log", "GET", "/api/log", Advanced, "Plain-text log of all KVMD services.", [
        p("seek", INT, false, "Seconds to look back"), p("follow", BOOL, false, "Long-poll for new lines")]),

    ep!("hid.state", Hid, "HID state", "GET", "/api/hid", Core, "Keyboard and mouse state.", []),
    ep!("hid.set_params", Hid, "Set HID parameters", "POST", "/api/hid/set_params", Advanced, "Emulated device types and jiggler.", [
        p("keyboard_output", ParamKind::Enum(&["usb", "ps2", "disabled"]), false, "Keyboard type"),
        p("mouse_output", ParamKind::Enum(&["usb", "usb_win98", "usb_rel", "ps2", "disabled"]), false, "Mouse type"),
        p("jiggler", BOOL, false, "Mouse jiggler")]),
    ep!("hid.set_connected", Hid, "Set HID connected", "POST", "/api/hid/set_connected", Advanced, "Connect or disconnect the HID devices.", [
        p("connected", BOOL, true, "Connected")]),
    ep!("hid.reset", Hid, "Reset HID", "POST", "/api/hid/reset", Advanced, "Reset the HID devices.", []),
    ep!("hid.keymaps", Hid, "Keymaps", "GET", "/api/hid/keymaps", Core, "Available keyboard layouts.", []),
    ep!("hid.print", Hid, "Type text", "POST", "/api/hid/print", Core, "Type text on the host.", [
        p("text", TEXT, true, "Text to type"), p("keymap", TEXT, false, "Keymap, e.g. de"),
        p("limit", INT, false, "Max characters (0 = unlimited)"), p("slow", BOOL, false, "Slow typing"),
        p("delay", FLOAT, false, "Delay between keys in slow mode (0..5)")], body: BodySource::TextParam("text")),
    ep!("hid.send_shortcut", Hid, "Send shortcut", "POST", "/api/hid/events/send_shortcut", Core, "Key combination, comma separated web names.", [
        p("keys", TEXT, true, "e.g. ControlLeft,AltLeft,Delete")]),
    ep!("hid.send_key", Hid, "Send key", "POST", "/api/hid/events/send_key", Advanced, "Single key event.", [
        p("key", TEXT, true, "Web key name"), p("state", BOOL, false, "Press (1) / release (0)"), p("finish", BOOL, false, "Release non-modifiers")]),
    ep!("hid.send_mouse_button", Hid, "Mouse button", "POST", "/api/hid/events/send_mouse_button", Advanced, "Mouse button event.", [
        p("button", ParamKind::Enum(&["left", "middle", "right", "up", "down"]), true, "Button"), p("state", BOOL, false, "Press / release")]),
    ep!("hid.send_mouse_move", Hid, "Mouse move", "POST", "/api/hid/events/send_mouse_move", Advanced, "Absolute move, 0,0 is the centre.", [
        p("to_x", INT, true, "X"), p("to_y", INT, true, "Y")]),
    ep!("hid.send_mouse_relative", Hid, "Mouse move relative", "POST", "/api/hid/events/send_mouse_relative", Advanced, "Relative move.", [
        p("delta_x", INT, true, "dx"), p("delta_y", INT, true, "dy")]),
    ep!("hid.send_mouse_wheel", Hid, "Mouse wheel", "POST", "/api/hid/events/send_mouse_wheel", Advanced, "Scroll.", [
        p("delta_x", INT, true, "dx"), p("delta_y", INT, true, "dy")]),

    ep!("atx.state", Atx, "ATX state", "GET", "/api/atx", Core, "Power and HDD LEDs, busy flag.", []),
    ep!("atx.power", Atx, "Set power", "POST", "/api/atx/power", Core, "Change the ATX power state.", [
        p("action", ParamKind::Enum(&["on", "off", "off_hard", "reset_hard"]), true, "Action"), p("wait", BOOL, false, "Wait for completion")], destructive: true),
    ep!("atx.click", Atx, "Click button", "POST", "/api/atx/click", Core, "Press a case button.", [
        p("button", ParamKind::Enum(&["power", "power_long", "reset"]), true, "Button"), p("wait", BOOL, false, "Wait for completion")], destructive: true),

    ep!("msd.state", Msd, "MSD state", "GET", "/api/msd", Core, "Drive state and images.", []),
    ep!("msd.write", Msd, "Upload image", "POST", "/api/msd/write", Advanced, "Upload an image (binary body). Use the MSD card for file uploads.", [
        p("image", TEXT, true, "Image name")]),
    ep!("msd.write_remote", Msd, "Upload from URL", "POST", "/api/msd/write_remote", Advanced, "Download an image from a URL onto the PiKVM (long-polling).", [
        p("url", TEXT, true, "HTTP(S) URL"), p("image", TEXT, false, "Image name"), p("timeout", INT, false, "Remote timeout seconds")]),
    ep!("msd.set_params", Msd, "Set MSD parameters", "POST", "/api/msd/set_params", Core, "Select image and drive mode.", [
        p("image", TEXT, false, "Image name"), p("cdrom", BOOL, false, "CD-ROM (1) or Flash (0)"), p("rw", BOOL, false, "Read-write")]),
    ep!("msd.set_connected", Msd, "Connect MSD", "POST", "/api/msd/set_connected", Core, "Connect or disconnect the drive.", [
        p("connected", BOOL, true, "Connected")]),
    ep!("msd.remove", Msd, "Remove image", "POST", "/api/msd/remove", Advanced, "Delete an image.", [
        p("image", TEXT, true, "Image name")], destructive: true),
    ep!("msd.reset", Msd, "Reset MSD", "POST", "/api/msd/reset", Advanced, "Reset the drive to defaults.", [], destructive: true),

    ep!("gpio.state", Gpio, "GPIO state", "GET", "/api/gpio", Core, "Model and state of all channels.", []),
    ep!("gpio.switch", Gpio, "Switch channel", "POST", "/api/gpio/switch", Core, "Set a channel on or off.", [
        p("channel", TEXT, true, "Channel"), p("state", BOOL, true, "State"), p("wait", BOOL, false, "Wait")]),
    ep!("gpio.pulse", Gpio, "Pulse channel", "POST", "/api/gpio/pulse", Core, "Pulse a channel.", [
        p("channel", TEXT, true, "Channel"), p("delay", FLOAT, false, "Seconds (0 = default)"), p("wait", BOOL, false, "Wait")]),

    ep!("streamer.state", Streamer, "Streamer state", "GET", "/api/streamer", Advanced, "Encoder, source, sinks and clients.", []),
    ep!("streamer.snapshot", Streamer, "Snapshot", "GET", "/api/streamer/snapshot", Advanced, "JPEG snapshot, optionally OCR or preview.", [
        p("save", BOOL, false, "Save"), p("load", BOOL, false, "Load saved"), p("allow_offline", BOOL, false, "Allow offline"),
        p("ocr", BOOL, false, "Run OCR"), p("ocr_langs", TEXT, false, "e.g. eng,deu"),
        p("ocr_left", INT, false, "Region left"), p("ocr_top", INT, false, "Region top"), p("ocr_right", INT, false, "Region right"), p("ocr_bottom", INT, false, "Region bottom"),
        p("preview", BOOL, false, "Preview"), p("preview_max_width", INT, false, "Max width"), p("preview_max_height", INT, false, "Max height"), p("preview_quality", INT, false, "JPEG quality")]),
    ep!("streamer.delete_snapshot", Streamer, "Delete snapshot", "DELETE", "/api/streamer/snapshot", Advanced, "Remove the saved snapshot.", []),
    ep!("streamer.ocr", Streamer, "OCR state", "GET", "/api/streamer/ocr", Advanced, "OCR availability and languages.", []),
    ep!("streamer.set_params", Streamer, "Set stream parameters", "POST", "/api/streamer/set_params", Advanced, "Quality, FPS and H.264 settings.", [
        p("quality", INT, false, "JPEG quality"), p("desired_fps", INT, false, "FPS"), p("h264_bitrate", INT, false, "kbps"), p("h264_gop", INT, false, "GOP")]),

    ep!("switch.state", Switch, "Switch state", "GET", "/api/switch", Core, "PiKVM Switch information.", []),
    ep!("switch.set_active_prev", Switch, "Previous port", "POST", "/api/switch/set_active_prev", Core, "Activate the previous port.", []),
    ep!("switch.set_active_next", Switch, "Next port", "POST", "/api/switch/set_active_next", Core, "Activate the next port.", []),
    ep!("switch.set_active", Switch, "Set active port", "POST", "/api/switch/set_active", Core, "Activate a specific port.", [
        p("port", TEXT, true, "0..19 or unit.port")]),
    ep!("switch.set_beacon", Switch, "Set beacon", "POST", "/api/switch/set_beacon", Advanced, "Beacon lights.", [
        p("state", BOOL, true, "On/off"), p("port", TEXT, false, "Port"), p("uplink", INT, false, "Uplink"), p("downlink", INT, false, "Downlink")]),
    ep!("switch.set_port_params", Switch, "Set port parameters", "POST", "/api/switch/set_port_params", Advanced, "Per-port configuration.", [
        p("port", TEXT, true, "Port"), p("edid_id", TEXT, false, "EDID id"), p("dummy", BOOL, false, "Dummy display"), p("name", TEXT, false, "Name"),
        p("atx_click_power_delay", FLOAT, false, "0..10"), p("atx_click_power_long_delay", FLOAT, false, "0..10"), p("atx_click_reset_delay", FLOAT, false, "0..10")]),
    ep!("switch.set_colors", Switch, "Set beacon colour", "POST", "/api/switch/set_colors", Advanced, "Body RRGGBB:BRIGHT:BLINK, e.g. FFA500:BF:0028.", [
        p("beacon", TEXT, true, "Colour spec")], body: BodySource::TextParam("beacon")),
    ep!("switch.reset", Switch, "Reboot switch", "POST", "/api/switch/reset", Advanced, "Reboot a unit, optionally into the bootloader.", [
        p("unit", INT, true, "Unit 0..4"), p("bootloader", BOOL, false, "Enter bootloader")], destructive: true),
    ep!("switch.edid_create", Switch, "Create EDID", "POST", "/api/switch/edids/create", Advanced, "Create an EDID configuration.", [
        p("name", TEXT, true, "Name"), p("data", TEXT, true, "Hex data")]),
    ep!("switch.edid_change", Switch, "Change EDID", "POST", "/api/switch/edids/change", Advanced, "Modify an EDID configuration.", [
        p("id", TEXT, true, "EDID id"), p("name", TEXT, false, "Name"), p("data", TEXT, false, "Hex data")]),
    ep!("switch.edid_remove", Switch, "Remove EDID", "POST", "/api/switch/edids/remove", Advanced, "Delete an EDID configuration.", [
        p("id", TEXT, true, "EDID id")], destructive: true),
    ep!("switch.atx_power", Switch, "Port ATX power", "POST", "/api/switch/atx/power", Advanced, "ATX power for a port.", [
        p("port", TEXT, true, "Port"), p("action", ParamKind::Enum(&["on", "off", "off_hard", "reset_hard"]), true, "Action")], destructive: true),
    ep!("switch.atx_click", Switch, "Port ATX click", "POST", "/api/switch/atx/click", Advanced, "ATX button for a port.", [
        p("port", TEXT, true, "Port"), p("button", ParamKind::Enum(&["power", "power_long", "reset"]), true, "Button")], destructive: true),

    ep!("redfish.root", Redfish, "Service root", "GET", "/api/redfish/v1", Explorer, "Redfish service discovery.", []),
    ep!("redfish.systems", Redfish, "Systems", "GET", "/api/redfish/v1/Systems", Explorer, "Computer systems collection.", []),
    ep!("redfish.system", Redfish, "System", "GET", "/api/redfish/v1/Systems/{id}", Explorer, "System details.", [
        p("id", TEXT, true, "0 or SwitchPortN")]),
    ep!("redfish.patch", Redfish, "Patch system", "PATCH", "/api/redfish/v1/Systems/{id}", Explorer, "No-op returning 204.", [
        p("id", TEXT, true, "0 or SwitchPortN")]),
    ep!("redfish.reset", Redfish, "Reset", "POST", "/api/redfish/v1/Systems/{id}/Actions/ComputerSystem.Reset", Explorer, "Power control via Redfish.", [
        p("id", TEXT, true, "0 or SwitchPortN"),
        p("ResetType", ParamKind::Enum(&["On", "ForceOff", "GracefulShutdown", "ForceRestart", "ForceOn", "PushPowerButton"]), true, "Reset type")],
        body: BodySource::JsonParam("ResetType", "ResetType"), destructive: true),

    ep!("misc.prometheus", Misc, "Prometheus metrics", "GET", "/api/export/prometheus/metrics", Explorer, "Metrics in Prometheus format.", []),
];

/// Endpoints in one category, in catalogue order.
pub fn by_category(cat: Category) -> impl Iterator<Item = &'static Endpoint> {
    ENDPOINTS.iter().filter(move |e| e.category == cat)
}

pub fn find(id: &str) -> Option<&'static Endpoint> {
    ENDPOINTS.iter().find(|e| e.id == id)
}

impl Endpoint {
    /// Execute this endpoint with user supplied parameter values (as strings).
    pub async fn run(&self, client: &PikvmClient, values: &BTreeMap<String, String>) -> Result<RawResponse> {
        let mut route = self.route.to_string();
        let mut query = Query::new();
        let mut body = Body::Empty;
        for param in self.params {
            let value = values.get(param.name).map(|v| v.trim()).filter(|v| !v.is_empty());
            let placeholder = format!("{{{}}}", param.name);
            if route.contains(&placeholder) {
                let v = value.ok_or_else(|| PikvmError::Decode(format!("missing parameter {}", param.name)))?;
                route = route.replace(&placeholder, v);
                continue;
            }
            match self.body {
                BodySource::TextParam(name) if name == param.name => {
                    if let Some(v) = value {
                        body = Body::Text(v.to_string());
                    }
                    continue;
                }
                BodySource::JsonParam(name, key) if name == param.name => {
                    if let Some(v) = value {
                        body = Body::Json(json!({ key: v }));
                    }
                    continue;
                }
                _ => {}
            }
            match (value, param.required) {
                (Some(v), _) => {
                    let encoded = match param.kind {
                        ParamKind::Bool => bool_value(v).to_string(),
                        _ => v.to_string(),
                    };
                    query = query.push(param.name, encoded);
                }
                (None, true) => return Err(PikvmError::Decode(format!("missing parameter {}", param.name))),
                (None, false) => {}
            }
        }
        if self.id == "auth.login" {
            let user = values.get("user").cloned().unwrap_or_default();
            let passwd = values.get("passwd").cloned().unwrap_or_default();
            query = Query::new();
            body = Body::Form(vec![("user".into(), user), ("passwd".into(), passwd)]);
        }
        let method = Method::from_bytes(self.method.as_bytes()).map_err(|e| PikvmError::Decode(e.to_string()))?;
        client.call(method, &route, &query, body).await
    }
}

fn bool_value(v: &str) -> &'static str {
    match v.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => "1",
        _ => "0",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalogue_has_unique_ids_and_every_category() {
        let mut ids: Vec<_> = ENDPOINTS.iter().map(|e| e.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), ENDPOINTS.len());
        for cat in Category::ALL {
            assert!(by_category(cat).next().is_some(), "{cat:?} empty");
        }
        assert!(ENDPOINTS.len() >= 50);
    }

    #[test]
    fn required_placeholders_match_params() {
        for e in ENDPOINTS {
            for seg in e.route.split('/') {
                if let Some(name) = seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    assert!(e.params.iter().any(|p| p.name == name), "{} lacks {name}", e.id);
                }
            }
        }
    }
}
