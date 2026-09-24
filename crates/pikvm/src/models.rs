//! Typed views of kvmd state objects. Every struct tolerates missing fields because the
//! websocket sends partial updates and the schema differs slightly between kvmd versions.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------------------
// System info
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThrottleFlag {
    pub now: bool,
    pub past: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Throttling {
    pub raw_flags: i64,
    pub ignore_past: bool,
    pub parsed_flags: BTreeMap<String, ThrottleFlag>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CpuHealth {
    pub percent: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MemHealth {
    pub available: u64,
    pub percent: f64,
    pub total: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HwHealth {
    pub cpu: CpuHealth,
    pub mem: MemHealth,
    pub temp: BTreeMap<String, f64>,
    pub throttling: Throttling,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Platform {
    pub base: String,
    pub board: String,
    pub model: String,
    pub serial: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub video: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoHw {
    pub health: HwHealth,
    pub platform: Platform,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Versioned {
    pub version: String,
    pub app: String,
    pub features: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Kernel {
    pub system: String,
    pub release: String,
    pub version: String,
    pub machine: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoSystem {
    pub kvmd: Versioned,
    pub streamer: Versioned,
    pub kernel: Kernel,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoExtra {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub path: String,
    pub daemon: String,
    pub port: u32,
    pub place: i32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InfoFan {
    pub monitored: bool,
    pub state: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Info {
    pub auth: Value,
    pub extras: BTreeMap<String, InfoExtra>,
    pub fan: Value,
    pub hw: InfoHw,
    pub meta: Value,
    pub system: InfoSystem,
}

// ---------------------------------------------------------------------------------------
// HID
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyboardLeds {
    pub caps: bool,
    pub num: bool,
    pub scroll: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Outputs {
    pub active: String,
    pub available: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HidKeyboard {
    pub online: bool,
    pub leds: KeyboardLeds,
    pub outputs: Outputs,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HidMouse {
    pub online: bool,
    pub absolute: bool,
    pub outputs: Outputs,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Jiggler {
    pub enabled: bool,
    pub active: bool,
    pub interval: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HidState {
    pub online: bool,
    pub busy: bool,
    pub enabled: bool,
    pub connected: Option<bool>,
    pub jiggler: Jiggler,
    pub keyboard: HidKeyboard,
    pub mouse: HidMouse,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keymaps {
    pub available: Vec<String>,
    pub default: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeymapsResult {
    pub keymaps: Keymaps,
}

/// Emulated keyboard output type for `POST /api/hid/set_params`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardOutput {
    Usb,
    Ps2,
    Disabled,
}

impl KeyboardOutput {
    pub fn as_str(self) -> &'static str {
        match self {
            KeyboardOutput::Usb => "usb",
            KeyboardOutput::Ps2 => "ps2",
            KeyboardOutput::Disabled => "disabled",
        }
    }
}

/// Emulated mouse output type for `POST /api/hid/set_params`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseOutput {
    Usb,
    UsbWin98,
    UsbRel,
    Ps2,
    Disabled,
}

impl MouseOutput {
    pub fn as_str(self) -> &'static str {
        match self {
            MouseOutput::Usb => "usb",
            MouseOutput::UsbWin98 => "usb_win98",
            MouseOutput::UsbRel => "usb_rel",
            MouseOutput::Ps2 => "ps2",
            MouseOutput::Disabled => "disabled",
        }
    }
}

/// Mouse buttons accepted by kvmd.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Up,
    Down,
}

impl MouseButton {
    pub fn as_str(self) -> &'static str {
        match self {
            MouseButton::Left => "left",
            MouseButton::Middle => "middle",
            MouseButton::Right => "right",
            MouseButton::Up => "up",
            MouseButton::Down => "down",
        }
    }
}

// ---------------------------------------------------------------------------------------
// ATX
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AtxLeds {
    pub power: bool,
    pub hdd: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AtxState {
    pub enabled: bool,
    pub busy: bool,
    pub leds: AtxLeds,
}

/// `action` for `POST /api/atx/power`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PowerAction {
    On,
    Off,
    OffHard,
    ResetHard,
}

impl PowerAction {
    pub fn as_str(self) -> &'static str {
        match self {
            PowerAction::On => "on",
            PowerAction::Off => "off",
            PowerAction::OffHard => "off_hard",
            PowerAction::ResetHard => "reset_hard",
        }
    }

    /// Actions that can lose unsaved work on the host.
    pub fn is_destructive(self) -> bool {
        matches!(self, PowerAction::OffHard | PowerAction::ResetHard)
    }
}

/// `button` for `POST /api/atx/click`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AtxButton {
    Power,
    PowerLong,
    Reset,
}

impl AtxButton {
    pub fn as_str(self) -> &'static str {
        match self {
            AtxButton::Power => "power",
            AtxButton::PowerLong => "power_long",
            AtxButton::Reset => "reset",
        }
    }

    pub fn is_destructive(self) -> bool {
        matches!(self, AtxButton::PowerLong | AtxButton::Reset)
    }
}

// ---------------------------------------------------------------------------------------
// MSD
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdImage {
    pub size: u64,
    pub complete: bool,
    pub in_storage: bool,
    pub removable: bool,
    #[serde(rename = "mod")]
    pub modified: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdPart {
    pub size: u64,
    pub free: u64,
    pub writable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdTransfer {
    pub name: String,
    pub size: u64,
    pub written: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdStorage {
    pub size: u64,
    pub free: u64,
    pub images: BTreeMap<String, MsdImage>,
    pub parts: BTreeMap<String, MsdPart>,
    pub uploading: Option<MsdTransfer>,
    pub downloading: Option<MsdTransfer>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdDrive {
    pub image: Option<Value>,
    pub connected: bool,
    pub cdrom: bool,
    pub rw: bool,
}

impl MsdDrive {
    /// Name of the selected image, if any.
    pub fn image_name(&self) -> Option<String> {
        match &self.image {
            Some(Value::String(s)) => Some(s.clone()),
            Some(Value::Object(o)) => o.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MsdState {
    pub enabled: bool,
    pub online: bool,
    pub busy: bool,
    pub storage: MsdStorage,
    pub drive: MsdDrive,
    pub features: BTreeMap<String, bool>,
}

// ---------------------------------------------------------------------------------------
// GPIO
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioPulse {
    pub delay: f64,
    pub min_delay: f64,
    pub max_delay: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioOutputScheme {
    pub switch: bool,
    pub pulse: GpioPulse,
    pub hw: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioScheme {
    pub inputs: BTreeMap<String, Value>,
    pub outputs: BTreeMap<String, GpioOutputScheme>,
}

/// One cell of the GPIO view table as rendered by the PiKVM web UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioViewCell {
    #[serde(rename = "type")]
    pub kind: String,
    pub text: String,
    pub channel: String,
    pub color: String,
    pub confirm: bool,
    pub hide: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioView {
    pub header: Value,
    /// Rows; `None` rows are separators.
    pub table: Vec<Option<Vec<GpioViewCell>>>,
}

impl GpioView {
    pub fn title(&self) -> String {
        match &self.header["title"] {
            Value::String(s) => s.clone(),
            Value::Array(cells) => cells
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<Vec<_>>()
                .join(" "),
            _ => "GPIO".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioModel {
    pub scheme: GpioScheme,
    pub view: GpioView,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioChannelState {
    pub online: bool,
    pub state: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioState {
    pub inputs: BTreeMap<String, GpioChannelState>,
    pub outputs: BTreeMap<String, GpioChannelState>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GpioFull {
    pub model: GpioModel,
    pub state: GpioState,
}

// ---------------------------------------------------------------------------------------
// Streamer
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerSource {
    pub online: bool,
    pub captured_fps: u32,
    pub desired_fps: u32,
    pub resolution: Resolution,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerStream {
    pub clients: u32,
    pub queued_fps: u32,
    pub clients_stat: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerEncoder {
    #[serde(rename = "type")]
    pub kind: String,
    pub quality: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerRuntime {
    pub instance_id: String,
    pub encoder: StreamerEncoder,
    pub source: StreamerSource,
    pub stream: StreamerStream,
    pub h264: Value,
    pub sinks: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerParams {
    pub desired_fps: u32,
    pub quality: u32,
    pub h264_bitrate: u32,
    pub h264_gop: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StreamerState {
    pub features: BTreeMap<String, bool>,
    pub limits: Value,
    pub params: StreamerParams,
    pub snapshot: Value,
    pub streamer: Option<StreamerRuntime>,
}

impl StreamerState {
    pub fn online(&self) -> bool {
        self.streamer.as_ref().map(|s| s.source.online).unwrap_or(false)
    }

    pub fn resolution(&self) -> Option<Resolution> {
        self.streamer
            .as_ref()
            .map(|s| s.source.resolution.clone())
            .filter(|r| r.width > 0)
    }

    pub fn captured_fps(&self) -> u32 {
        self.streamer.as_ref().map(|s| s.source.captured_fps).unwrap_or(0)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrLangs {
    pub available: Vec<String>,
    pub default: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrState {
    pub enabled: bool,
    pub langs: OcrLangs,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OcrResult {
    pub ocr: OcrState,
}

/// Parameters for `GET /api/streamer/snapshot`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SnapshotOptions {
    pub save: bool,
    pub load: bool,
    pub allow_offline: bool,
    pub preview: bool,
    pub preview_max_width: Option<u32>,
    pub preview_max_height: Option<u32>,
    pub preview_quality: Option<u32>,
}

/// OCR region in pixels, origin top-left. `None` means the whole frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrRegion {
    pub left: u32,
    pub top: u32,
    pub right: u32,
    pub bottom: u32,
}

/// Parameters for `GET /api/streamer/snapshot?ocr=1`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OcrOptions {
    pub langs: Vec<String>,
    pub region: Option<OcrRegion>,
    pub allow_offline: bool,
}

/// Parameters for `POST /api/streamer/set_params`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct StreamerParamsUpdate {
    pub quality: Option<u32>,
    pub desired_fps: Option<u32>,
    pub h264_bitrate: Option<u32>,
    pub h264_gop: Option<u32>,
}

// ---------------------------------------------------------------------------------------
// Switch
// ---------------------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchPort {
    pub unit: u32,
    pub channel: u32,
    pub name: String,
    pub id: String,
    pub atx: Value,
    pub video: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchModel {
    pub firmware: Value,
    pub units: Vec<Value>,
    pub ports: Vec<SwitchPort>,
    pub limits: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchSummary {
    pub active_port: i64,
    pub active_id: String,
    pub synced: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchEdid {
    pub name: String,
    pub data: String,
    pub parsed: Value,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchEdids {
    pub all: BTreeMap<String, SwitchEdid>,
    pub used: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchBeacons {
    pub uplinks: Vec<bool>,
    pub downlinks: Vec<bool>,
    pub ports: Vec<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchAtxLeds {
    pub power: Vec<bool>,
    pub hdd: Vec<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchAtx {
    pub busy: Vec<bool>,
    pub leds: SwitchAtxLeds,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchLinks {
    pub links: Vec<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SwitchState {
    pub model: SwitchModel,
    pub summary: SwitchSummary,
    pub edids: SwitchEdids,
    pub colors: Value,
    pub video: SwitchLinks,
    pub usb: SwitchLinks,
    pub beacons: SwitchBeacons,
    pub atx: SwitchAtx,
}

impl SwitchState {
    /// A Switch is present when kvmd reports at least one port.
    pub fn is_present(&self) -> bool {
        !self.model.ports.is_empty()
    }
}

/// Port parameters for `POST /api/switch/set_port_params`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SwitchPortParams {
    pub edid_id: Option<String>,
    pub dummy: Option<bool>,
    pub name: Option<String>,
    pub atx_click_power_delay: Option<f64>,
    pub atx_click_power_long_delay: Option<f64>,
    pub atx_click_reset_delay: Option<f64>,
}

/// Beacon target for `POST /api/switch/set_beacon`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BeaconTarget {
    Port(f64),
    Uplink(u32),
    Downlink(u32),
}

// ---------------------------------------------------------------------------------------
// Redfish
// ---------------------------------------------------------------------------------------

/// `ResetType` values accepted by Redfish `ComputerSystem.Reset`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RedfishResetType {
    On,
    ForceOff,
    GracefulShutdown,
    ForceRestart,
    ForceOn,
    PushPowerButton,
}

impl RedfishResetType {
    pub const ALL: [RedfishResetType; 6] = [
        RedfishResetType::On,
        RedfishResetType::ForceOff,
        RedfishResetType::GracefulShutdown,
        RedfishResetType::ForceRestart,
        RedfishResetType::ForceOn,
        RedfishResetType::PushPowerButton,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RedfishResetType::On => "On",
            RedfishResetType::ForceOff => "ForceOff",
            RedfishResetType::GracefulShutdown => "GracefulShutdown",
            RedfishResetType::ForceRestart => "ForceRestart",
            RedfishResetType::ForceOn => "ForceOn",
            RedfishResetType::PushPowerButton => "PushPowerButton",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msd_state_parses_documented_example() {
        let json = r#"{"busy": false, "drive": {"cdrom": true, "connected": false, "image": null, "rw": false},
            "enabled": true, "online": true,
            "storage": {"downloading": null, "images": {"a.iso": {"size": 5, "complete": true, "in_storage": true, "removable": true, "mod": 1.0}},
            "parts": {"": {"free": 1, "size": 2, "writable": true}}, "uploading": null}}"#;
        let s: MsdState = serde_json::from_str(json).unwrap();
        assert!(s.enabled && s.online && !s.busy);
        assert_eq!(s.storage.images["a.iso"].size, 5);
        assert!(s.drive.image_name().is_none());
    }

    #[test]
    fn gpio_view_parses_rows_with_separators() {
        let json = r#"{"header": {"title": "Switches"}, "table": [[{"type": "label", "text": "x"}], null,
            [{"type": "input", "channel": "led1", "color": "green"}, {"type": "output", "channel": "b1", "text": "Click"}]]}"#;
        let v: GpioView = serde_json::from_str(json).unwrap();
        assert_eq!(v.title(), "Switches");
        assert_eq!(v.table.len(), 3);
        assert!(v.table[1].is_none());
        assert_eq!(v.table[2].as_ref().unwrap()[1].kind, "output");
    }

    #[test]
    fn streamer_state_helpers() {
        let json = r#"{"streamer": {"source": {"online": true, "captured_fps": 59, "resolution": {"width": 1280, "height": 720}}}}"#;
        let s: StreamerState = serde_json::from_str(json).unwrap();
        assert!(s.online());
        assert_eq!(s.resolution().unwrap().width, 1280);
        let off: StreamerState = serde_json::from_str(r#"{"streamer": null}"#).unwrap();
        assert!(!off.online());
    }
}
