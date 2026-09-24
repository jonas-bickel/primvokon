//! WebSocket events and the aggregated KVM state they build.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::*;

/// One message from `/api/ws`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KvmEvent {
    pub event_type: String,
    #[serde(default)]
    pub event: Value,
}

impl KvmEvent {
    pub fn parse(text: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(text)
    }
}

/// Events sent from the client to kvmd.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event_type", content = "event", rename_all = "snake_case")]
pub enum OutEvent {
    Ping {},
    Key { key: String, state: bool },
    MouseButton { button: String, state: bool },
    MouseMove { to: MousePoint },
    MouseRelative { delta: MouseDelta, squash: bool },
    MouseWheel { delta: MouseDelta, squash: bool },
}

/// Absolute mouse coordinates in kvmd's `-32768..32767` space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MousePoint {
    pub x: i32,
    pub y: i32,
}

impl MousePoint {
    /// Convert a position inside a `width × height` frame into kvmd's absolute space.
    pub fn from_frame(x: f64, y: f64, width: f64, height: f64) -> Self {
        let fx = (x / width).clamp(0.0, 1.0);
        let fy = (y / height).clamp(0.0, 1.0);
        Self {
            x: (fx * 65535.0 - 32768.0).round() as i32,
            y: (fy * 65535.0 - 32768.0).round() as i32,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MouseDelta {
    pub x: i32,
    pub y: i32,
}

/// Aggregated state of every kvmd subsystem, updated from websocket events.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KvmState {
    pub info_hw: Option<InfoHw>,
    pub info_system: Option<InfoSystem>,
    pub info_meta: Value,
    pub info_extras: Value,
    pub info_fan: Value,
    pub info_auth: Value,
    pub hid: HidState,
    pub keymaps: Option<Keymaps>,
    pub atx: AtxState,
    pub msd: MsdState,
    pub gpio_model: GpioModel,
    pub gpio: GpioState,
    pub streamer: StreamerState,
    pub switch: Option<SwitchState>,
    pub wol: Value,
    /// `true` once the initial burst ended with a `loop` event.
    pub in_loop: bool,
}

/// Which part of the state an event touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateSection {
    Info,
    Hid,
    Keymaps,
    Atx,
    Msd,
    Gpio,
    Streamer,
    Switch,
    Wol,
    Loop,
    Pong,
    Unknown,
}

impl KvmState {
    /// Apply one event; returns the affected section.
    pub fn apply(&mut self, ev: &KvmEvent) -> StateSection {
        let e = &ev.event;
        match ev.event_type.as_str() {
            "info_hw_state" => {
                self.info_hw = merge_typed(self.info_hw.take(), e);
                StateSection::Info
            }
            "info_system_state" => {
                self.info_system = merge_typed(self.info_system.take(), e);
                StateSection::Info
            }
            "info_meta_state" => {
                merge_value(&mut self.info_meta, e);
                StateSection::Info
            }
            "info_extras_state" => {
                merge_value(&mut self.info_extras, e);
                StateSection::Info
            }
            "info_fan_state" => {
                merge_value(&mut self.info_fan, e);
                StateSection::Info
            }
            "info_auth_state" => {
                merge_value(&mut self.info_auth, e);
                StateSection::Info
            }
            "hid_state" => {
                self.hid = merge_into(&self.hid, e);
                StateSection::Hid
            }
            "hid_keymaps_state" => {
                let km: KeymapsResult = serde_json::from_value(e.clone()).unwrap_or_default();
                self.keymaps = Some(km.keymaps);
                StateSection::Keymaps
            }
            "atx_state" => {
                self.atx = merge_into(&self.atx, e);
                StateSection::Atx
            }
            "msd_state" => {
                self.msd = merge_into(&self.msd, e);
                StateSection::Msd
            }
            "gpio_model_state" => {
                self.gpio_model = serde_json::from_value(e.clone()).unwrap_or_default();
                StateSection::Gpio
            }
            "gpio_state" => {
                self.gpio = merge_into(&self.gpio, e);
                StateSection::Gpio
            }
            "streamer_state" => {
                self.streamer = merge_into(&self.streamer, e);
                StateSection::Streamer
            }
            "switch_state" => {
                let merged = merge_into(&self.switch.clone().unwrap_or_default(), e);
                self.switch = Some(merged);
                StateSection::Switch
            }
            "wol_state" => {
                merge_value(&mut self.wol, e);
                StateSection::Wol
            }
            "loop" => {
                self.in_loop = true;
                StateSection::Loop
            }
            "pong" => StateSection::Pong,
            _ => StateSection::Unknown,
        }
    }

    pub fn switch_present(&self) -> bool {
        self.switch.as_ref().map(SwitchState::is_present).unwrap_or(false)
    }

    pub fn gpio_present(&self) -> bool {
        self.gpio_model.view.table.iter().any(|r| r.is_some())
    }
}

/// Deep-merge `patch` into `base` (objects merge key by key, everything else is replaced).
pub fn merge_value(base: &mut Value, patch: &Value) {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            for (k, v) in p {
                match b.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => merge_value(existing, v),
                    _ => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, p) => *b = p.clone(),
    }
}

fn merge_into<T>(current: &T, patch: &Value) -> T
where
    T: Serialize + for<'de> Deserialize<'de> + Clone,
{
    let mut v = serde_json::to_value(current).unwrap_or(Value::Null);
    merge_value(&mut v, patch);
    serde_json::from_value(v).unwrap_or_else(|_| current.clone())
}

fn merge_typed<T>(current: Option<T>, patch: &Value) -> Option<T>
where
    T: Serialize + for<'de> Deserialize<'de> + Clone + Default,
{
    Some(merge_into(&current.unwrap_or_default(), patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_applies_events() {
        let mut st = KvmState::default();
        let ev = KvmEvent::parse(r#"{"event_type": "atx_state", "event": {"enabled": true, "busy": false, "leds": {"power": true, "hdd": false}}}"#).unwrap();
        assert_eq!(st.apply(&ev), StateSection::Atx);
        assert!(st.atx.leds.power);
        // partial update keeps unrelated fields
        let ev = KvmEvent::parse(r#"{"event_type": "atx_state", "event": {"busy": true}}"#).unwrap();
        st.apply(&ev);
        assert!(st.atx.busy && st.atx.leds.power);
        let ev = KvmEvent::parse(r#"{"event_type": "loop", "event": {}}"#).unwrap();
        assert_eq!(st.apply(&ev), StateSection::Loop);
        assert!(st.in_loop);
    }

    #[test]
    fn out_events_serialize_like_kvmd_expects() {
        let key = serde_json::to_string(&OutEvent::Key {
            key: "Enter".into(),
            state: true,
        })
        .unwrap();
        assert_eq!(key, r#"{"event_type":"key","event":{"key":"Enter","state":true}}"#);
        let mv = serde_json::to_string(&OutEvent::MouseMove {
            to: MousePoint { x: 0, y: 50 },
        })
        .unwrap();
        assert_eq!(mv, r#"{"event_type":"mouse_move","event":{"to":{"x":0,"y":50}}}"#);
        let ping = serde_json::to_string(&OutEvent::Ping {}).unwrap();
        assert_eq!(ping, r#"{"event_type":"ping","event":{}}"#);
    }

    #[test]
    fn mouse_point_scales_to_absolute_space() {
        assert_eq!(
            MousePoint::from_frame(0.0, 0.0, 100.0, 100.0),
            MousePoint { x: -32768, y: -32768 }
        );
        assert_eq!(
            MousePoint::from_frame(100.0, 100.0, 100.0, 100.0),
            MousePoint { x: 32767, y: 32767 }
        );
        let mid = MousePoint::from_frame(50.0, 50.0, 100.0, 100.0);
        assert!(mid.x.abs() <= 1 && mid.y.abs() <= 1);
    }

    #[test]
    fn switch_presence_requires_ports() {
        let mut st = KvmState::default();
        assert!(!st.switch_present());
        let ev = KvmEvent::parse(r#"{"event_type": "switch_state", "event": {"model": {"ports": [{"unit": 0, "channel": 0, "name": "", "id": "1"}]}, "summary": {"active_port": 0, "active_id": "1", "synced": true}}}"#).unwrap();
        st.apply(&ev);
        assert!(st.switch_present());
    }
}
