//! Tools the agent can call. Each has a JSON schema for the model and an executor that maps
//! to PiKVM calls.

use std::sync::Arc;
use std::time::Duration;

use pikvm::events::MousePoint;
use pikvm::models::{MouseButton, PowerAction};
use pikvm::PikvmClient;
use serde_json::{json, Value};

use crate::ai::{SharedProvider, ToolSpec};
use crate::screen::{read_screen_text, ScreenSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToolKind {
    ReadScreen,
    TypeText,
    SendShortcut,
    SendKey,
    ClickMouse,
    MoveMouse,
    Scroll,
    AtxPower,
    Wait,
    Finish,
}

impl ToolKind {
    pub const ALL: [ToolKind; 10] = [
        ToolKind::ReadScreen,
        ToolKind::TypeText,
        ToolKind::SendShortcut,
        ToolKind::SendKey,
        ToolKind::ClickMouse,
        ToolKind::MoveMouse,
        ToolKind::Scroll,
        ToolKind::AtxPower,
        ToolKind::Wait,
        ToolKind::Finish,
    ];

    pub fn name(self) -> &'static str {
        match self {
            ToolKind::ReadScreen => "read_screen",
            ToolKind::TypeText => "type_text",
            ToolKind::SendShortcut => "send_shortcut",
            ToolKind::SendKey => "send_key",
            ToolKind::ClickMouse => "click_mouse",
            ToolKind::MoveMouse => "move_mouse",
            ToolKind::Scroll => "scroll",
            ToolKind::AtxPower => "atx_power",
            ToolKind::Wait => "wait",
            ToolKind::Finish => "finish",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }

    pub fn label(self) -> &'static str {
        match self {
            ToolKind::ReadScreen => "Read screen",
            ToolKind::TypeText => "Type text",
            ToolKind::SendShortcut => "Send shortcut",
            ToolKind::SendKey => "Send single key",
            ToolKind::ClickMouse => "Click mouse",
            ToolKind::MoveMouse => "Move mouse",
            ToolKind::Scroll => "Scroll",
            ToolKind::AtxPower => "ATX power",
            ToolKind::Wait => "Wait",
            ToolKind::Finish => "Finish task",
        }
    }

    /// Side-effecting tools need approval in ask mode.
    pub fn is_side_effect(self) -> bool {
        !matches!(self, ToolKind::ReadScreen | ToolKind::Wait | ToolKind::Finish)
    }

    pub fn description(self) -> &'static str {
        match self {
            ToolKind::ReadScreen => "Capture the host screen and return its content as text (OCR, or a vision description when enabled) together with the screen resolution. Call this before and after actions.",
            ToolKind::TypeText => "Type text on the host keyboard exactly as given. Use \\n for Enter only when the field is multi-line; otherwise use send_shortcut Enter.",
            ToolKind::SendShortcut => "Press a key combination. Keys use browser KeyboardEvent.code names, e.g. [\"ControlLeft\",\"KeyC\"], [\"Enter\"], [\"AltLeft\",\"Tab\"].",
            ToolKind::SendKey => "Press and release one key by KeyboardEvent.code name, e.g. \"Escape\" or \"Tab\".",
            ToolKind::ClickMouse => "Click at screen pixel coordinates (origin top-left, use the resolution from read_screen).",
            ToolKind::MoveMouse => "Move the pointer to screen pixel coordinates without clicking.",
            ToolKind::Scroll => "Scroll the mouse wheel; positive delta_y scrolls down, negative up.",
            ToolKind::AtxPower => "Power control of the host: action on | off | off_hard | reset_hard. Destructive; only when explicitly asked.",
            ToolKind::Wait => "Wait for the given seconds (max 30) so the host can react.",
            ToolKind::Finish => "End the task with a short report of what was done and what is left.",
        }
    }

    pub fn schema(self) -> Value {
        let obj =
            |props: Value, required: Vec<&str>| json!({"type": "object", "properties": props, "required": required});
        match self {
            ToolKind::ReadScreen => obj(json!({}), vec![]),
            ToolKind::TypeText => obj(json!({"text": {"type": "string"}}), vec!["text"]),
            ToolKind::SendShortcut => obj(
                json!({"keys": {"type": "array", "items": {"type": "string"}}}),
                vec!["keys"],
            ),
            ToolKind::SendKey => obj(json!({"key": {"type": "string"}}), vec!["key"]),
            ToolKind::ClickMouse => obj(
                json!({"x": {"type": "integer"}, "y": {"type": "integer"}, "button": {"type": "string", "enum": ["left", "right", "middle"]}, "double": {"type": "boolean"}}),
                vec!["x", "y"],
            ),
            ToolKind::MoveMouse => obj(
                json!({"x": {"type": "integer"}, "y": {"type": "integer"}}),
                vec!["x", "y"],
            ),
            ToolKind::Scroll => obj(
                json!({"delta_y": {"type": "integer"}, "delta_x": {"type": "integer"}}),
                vec!["delta_y"],
            ),
            ToolKind::AtxPower => obj(
                json!({"action": {"type": "string", "enum": ["on", "off", "off_hard", "reset_hard"]}}),
                vec!["action"],
            ),
            ToolKind::Wait => obj(json!({"seconds": {"type": "number"}}), vec!["seconds"]),
            ToolKind::Finish => obj(json!({"report": {"type": "string"}}), vec!["report"]),
        }
    }

    pub fn spec(self) -> ToolSpec {
        ToolSpec {
            name: self.name().into(),
            description: self.description().into(),
            input_schema: self.schema(),
        }
    }

    /// One-line human description of a call, for approval cards and transcripts.
    pub fn describe_call(self, input: &Value) -> String {
        match self {
            ToolKind::ReadScreen => "Read the screen".into(),
            ToolKind::TypeText => format!("Type: {:?}", input["text"].as_str().unwrap_or_default()),
            ToolKind::SendShortcut => format!(
                "Press {}",
                input["keys"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|k| k.as_str()).collect::<Vec<_>>().join("+"))
                    .unwrap_or_default()
            ),
            ToolKind::SendKey => format!("Press {}", input["key"].as_str().unwrap_or_default()),
            ToolKind::ClickMouse => format!(
                "{} click at ({}, {})",
                input["button"].as_str().unwrap_or("left"),
                input["x"],
                input["y"]
            ),
            ToolKind::MoveMouse => format!("Move mouse to ({}, {})", input["x"], input["y"]),
            ToolKind::Scroll => format!("Scroll by {}", input["delta_y"]),
            ToolKind::AtxPower => format!("ATX power: {}", input["action"].as_str().unwrap_or_default()),
            ToolKind::Wait => format!("Wait {} s", input["seconds"]),
            ToolKind::Finish => "Finish".into(),
        }
    }
}

/// Executes tool calls against the PiKVM.
pub struct ToolExecutor {
    pub client: PikvmClient,
    pub screen: Arc<dyn ScreenSource>,
    pub vision: Option<SharedProvider>,
    pub ocr_langs: Vec<String>,
    pub keymap: Option<String>,
}

impl ToolExecutor {
    async fn screen_resolution(&self) -> (f64, f64) {
        match self.client.streamer().state().await.ok().and_then(|s| s.resolution()) {
            Some(r) => (f64::from(r.width), f64::from(r.height)),
            None => (1920.0, 1080.0),
        }
    }

    async fn move_to(&self, input: &Value) -> anyhow::Result<()> {
        let x = input["x"].as_f64().ok_or_else(|| anyhow::anyhow!("x missing"))?;
        let y = input["y"].as_f64().ok_or_else(|| anyhow::anyhow!("y missing"))?;
        let (w, h) = self.screen_resolution().await;
        let p = MousePoint::from_frame(x, y, w, h);
        self.client.hid().send_mouse_move(p.x, p.y).await?;
        Ok(())
    }

    pub async fn execute(&self, kind: ToolKind, input: &Value) -> anyhow::Result<String> {
        match kind {
            ToolKind::ReadScreen => {
                let text = read_screen_text(self.screen.as_ref(), &self.ocr_langs, self.vision.as_deref()).await?;
                let (w, h) = self.screen_resolution().await;
                Ok(format!("Resolution: {w}x{h}\n\n{text}"))
            }
            ToolKind::TypeText => {
                let text = input["text"].as_str().ok_or_else(|| anyhow::anyhow!("text missing"))?;
                let opts = pikvm::api::hid::PrintOptions {
                    keymap: self.keymap.clone(),
                    limit: Some(0),
                    ..Default::default()
                };
                self.client.hid().print(text, &opts).await?;
                Ok(format!("typed {} characters", text.chars().count()))
            }
            ToolKind::SendShortcut => {
                let keys: Vec<&str> = input["keys"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("keys missing"))?
                    .iter()
                    .filter_map(|k| k.as_str())
                    .collect();
                if keys.is_empty() {
                    anyhow::bail!("keys empty");
                }
                self.client.hid().send_shortcut(&keys).await?;
                Ok(format!("pressed {}", keys.join("+")))
            }
            ToolKind::SendKey => {
                let key = input["key"].as_str().ok_or_else(|| anyhow::anyhow!("key missing"))?;
                self.client.hid().send_key(key, None, true).await?;
                Ok(format!("pressed {key}"))
            }
            ToolKind::ClickMouse => {
                self.move_to(input).await?;
                let button = match input["button"].as_str() {
                    Some("right") => MouseButton::Right,
                    Some("middle") => MouseButton::Middle,
                    _ => MouseButton::Left,
                };
                let clicks = if input["double"].as_bool().unwrap_or(false) {
                    2
                } else {
                    1
                };
                for _ in 0..clicks {
                    self.client.hid().send_mouse_button(button, Some(true)).await?;
                    tokio::time::sleep(Duration::from_millis(60)).await;
                    self.client.hid().send_mouse_button(button, Some(false)).await?;
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
                Ok("clicked".into())
            }
            ToolKind::MoveMouse => {
                self.move_to(input).await?;
                Ok("moved".into())
            }
            ToolKind::Scroll => {
                let dy = input["delta_y"].as_i64().unwrap_or(0) as i32;
                let dx = input["delta_x"].as_i64().unwrap_or(0) as i32;
                // kvmd wheel delta is inverted relative to "positive scrolls down".
                self.client.hid().send_mouse_wheel(dx, -dy).await?;
                Ok("scrolled".into())
            }
            ToolKind::AtxPower => {
                let action = match input["action"].as_str() {
                    Some("on") => PowerAction::On,
                    Some("off") => PowerAction::Off,
                    Some("off_hard") => PowerAction::OffHard,
                    Some("reset_hard") => PowerAction::ResetHard,
                    other => anyhow::bail!("unknown action {other:?}"),
                };
                self.client.atx().power(action, true).await?;
                Ok(format!("atx {}", action.as_str()))
            }
            ToolKind::Wait => {
                let secs = input["seconds"].as_f64().unwrap_or(1.0).clamp(0.0, 30.0);
                tokio::time::sleep(Duration::from_secs_f64(secs)).await;
                Ok(format!("waited {secs} s"))
            }
            ToolKind::Finish => Ok(input["report"].as_str().unwrap_or("done").to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_specs_are_well_formed() {
        for t in ToolKind::ALL {
            let spec = t.spec();
            assert_eq!(spec.input_schema["type"], "object");
            assert_eq!(ToolKind::from_name(t.name()), Some(t));
        }
        assert!(ToolKind::TypeText.is_side_effect());
        assert!(!ToolKind::ReadScreen.is_side_effect());
        assert_eq!(
            ToolKind::SendShortcut.describe_call(&json!({"keys": ["ControlLeft", "KeyC"]})),
            "Press ControlLeft+KeyC"
        );
    }
}
