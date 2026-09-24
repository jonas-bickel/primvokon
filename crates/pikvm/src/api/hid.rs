//! `/api/hid/*`

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;
use crate::models::{HidState, KeyboardOutput, KeymapsResult, MouseButton, MouseOutput};

pub struct HidApi<'a>(pub(crate) &'a PikvmClient);

/// Options for `POST /api/hid/print`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintOptions {
    pub keymap: Option<String>,
    /// `0` = no limit, kvmd default 1024.
    pub limit: Option<u32>,
    pub slow: bool,
    pub delay: Option<f64>,
}

impl HidApi<'_> {
    /// `GET /api/hid`
    pub async fn state(&self) -> Result<HidState> {
        self.0.get_result("/api/hid", Query::new()).await
    }

    /// `POST /api/hid/set_params`
    pub async fn set_params(
        &self,
        keyboard_output: Option<KeyboardOutput>,
        mouse_output: Option<MouseOutput>,
        jiggler: Option<bool>,
    ) -> Result<()> {
        let q = Query::new()
            .opt("keyboard_output", keyboard_output.map(KeyboardOutput::as_str))
            .opt("mouse_output", mouse_output.map(MouseOutput::as_str))
            .opt_flag("jiggler", jiggler);
        self.0.post_ok("/api/hid/set_params", q).await
    }

    /// `POST /api/hid/set_connected`
    pub async fn set_connected(&self, connected: bool) -> Result<()> {
        self.0.post_ok("/api/hid/set_connected", Query::new().flag("connected", connected)).await
    }

    /// `POST /api/hid/reset`
    pub async fn reset(&self) -> Result<()> {
        self.0.post_ok("/api/hid/reset", Query::new()).await
    }

    /// `GET /api/hid/keymaps`
    pub async fn keymaps(&self) -> Result<KeymapsResult> {
        self.0.get_result("/api/hid/keymaps", Query::new()).await
    }

    /// `POST /api/hid/print` – type `text` on the host.
    pub async fn print(&self, text: &str, opts: &PrintOptions) -> Result<()> {
        let q = Query::new()
            .opt("keymap", opts.keymap.as_deref())
            .opt("limit", opts.limit)
            .flag("slow", opts.slow)
            .opt("delay", opts.delay);
        let _: serde_json::Value = self.0.post_result("/api/hid/print", q, Body::Text(text.to_string())).await?;
        Ok(())
    }

    /// `POST /api/hid/events/send_shortcut` – `keys` are web names, e.g. `["ControlLeft", "AltLeft", "Delete"]`.
    pub async fn send_shortcut(&self, keys: &[&str]) -> Result<()> {
        self.0
            .post_ok("/api/hid/events/send_shortcut", Query::new().push("keys", keys.join(",")))
            .await
    }

    /// `POST /api/hid/events/send_key`
    pub async fn send_key(&self, key: &str, state: Option<bool>, finish: bool) -> Result<()> {
        let q = Query::new().push("key", key).opt_flag("state", state).flag("finish", finish);
        self.0.post_ok("/api/hid/events/send_key", q).await
    }

    /// `POST /api/hid/events/send_mouse_button`
    pub async fn send_mouse_button(&self, button: MouseButton, state: Option<bool>) -> Result<()> {
        let q = Query::new().push("button", button.as_str()).opt_flag("state", state);
        self.0.post_ok("/api/hid/events/send_mouse_button", q).await
    }

    /// `POST /api/hid/events/send_mouse_move` – absolute, `0,0` is the centre.
    pub async fn send_mouse_move(&self, to_x: i32, to_y: i32) -> Result<()> {
        let q = Query::new().push("to_x", to_x).push("to_y", to_y);
        self.0.post_ok("/api/hid/events/send_mouse_move", q).await
    }

    /// `POST /api/hid/events/send_mouse_relative`
    pub async fn send_mouse_relative(&self, delta_x: i32, delta_y: i32) -> Result<()> {
        let q = Query::new().push("delta_x", delta_x).push("delta_y", delta_y);
        self.0.post_ok("/api/hid/events/send_mouse_relative", q).await
    }

    /// `POST /api/hid/events/send_mouse_wheel`
    pub async fn send_mouse_wheel(&self, delta_x: i32, delta_y: i32) -> Result<()> {
        let q = Query::new().push("delta_x", delta_x).push("delta_y", delta_y);
        self.0.post_ok("/api/hid/events/send_mouse_wheel", q).await
    }
}

/// Frequently used shortcuts, as web key names.
pub const SHORTCUTS: &[(&str, &[&str])] = &[
    ("Ctrl+Alt+Del", &["ControlLeft", "AltLeft", "Delete"]),
    ("Alt+Tab", &["AltLeft", "Tab"]),
    ("Alt+F4", &["AltLeft", "F4"]),
    ("Win", &["MetaLeft"]),
    ("Win+L (lock)", &["MetaLeft", "KeyL"]),
    ("Win+D (desktop)", &["MetaLeft", "KeyD"]),
    ("Ctrl+Shift+Esc", &["ControlLeft", "ShiftLeft", "Escape"]),
    ("Ctrl+C", &["ControlLeft", "KeyC"]),
    ("Ctrl+V", &["ControlLeft", "KeyV"]),
    ("Enter", &["Enter"]),
    ("Escape", &["Escape"]),
    ("Print Screen", &["PrintScreen"]),
];
