//! Console page: live video, status pills, quick actions and keyboard/mouse forwarding.

use std::cell::{Cell, RefCell};
use std::time::{Duration, Instant};

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gdk, glib};
use pikvm::api::hid::PrintOptions;
use pikvm::events::{MouseDelta, MousePoint, OutEvent};
use pikvm::keycodes::web_name_for_gtk_keycode;
use pikvm::models::{AtxButton, StreamerParamsUpdate};
use pikvm::PikvmClient;
use primvokon_core::runtime;
use tokio_util::sync::CancellationToken;

use crate::state::state;
use crate::util::{self, confirm, set_pill, spawn_result, toast};

/// Decoded RGBA frame ready for a `MemoryTexture`.
struct Frame {
    width: i32,
    height: i32,
    rgba: Vec<u8>,
}

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/jonas_bickel/Primvokon/ui/console.ui")]
    pub struct ConsolePage {
        #[template_child]
        pub video: TemplateChild<gtk::Picture>,
        #[template_child]
        pub placeholder: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub banner: TemplateChild<gtk::Label>,
        #[template_child]
        pub video_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub hid_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub atx_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub msd_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub capture_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub power_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub reset_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub cad_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub paste_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub stream_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub quality_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub fps_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub bitrate_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub gop_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub apply_stream_button: TemplateChild<gtk::Button>,

        pub stream_cancel: RefCell<Option<CancellationToken>>,
        pub texture_size: Cell<(i32, i32)>,
        pub last_motion: Cell<Option<Instant>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ConsolePage {
        const NAME: &'static str = "PvConsolePage";
        type Type = super::ConsolePage;
        type ParentType = adw::Bin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for ConsolePage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for ConsolePage {}
    impl BinImpl for ConsolePage {}
}

glib::wrapper! {
    pub struct ConsolePage(ObjectSubclass<imp::ConsolePage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ConsolePage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ConsolePage {
    fn setup(&self) {
        let st = state();
        let imp = self.imp();
        imp.capture_toggle.set_active(st.settings.borrow().ui.capture_input);
        self.update_advanced_visibility();

        // Connection state → start/stop the video.
        let this = self.clone();
        st.connection.store.connect_connection(move |store| {
            this.on_connection_changed(store.is_connected(), &store.status());
        });
        let this = self.clone();
        st.connection.store.connect_changed(move |_, _| this.refresh_pills());
        let this = self.clone();
        st.bus
            .connect_signal("settings-changed", move || this.update_advanced_visibility());

        self.setup_actions();
        self.setup_input();
        self.on_connection_changed(false, "Offline");
    }

    fn update_advanced_visibility(&self) {
        let advanced = state().settings.borrow().ui.show_advanced;
        self.imp().stream_button.set_visible(advanced);
    }

    fn client(&self) -> Option<PikvmClient> {
        state().connection.client()
    }

    fn setup_actions(&self) {
        let imp = self.imp();
        let this = self.clone();
        imp.power_button
            .connect_clicked(move |_| this.atx_click(AtxButton::Power));
        let this = self.clone();
        imp.reset_button
            .connect_clicked(move |_| this.atx_click(AtxButton::Reset));
        let this = self.clone();
        imp.cad_button
            .connect_clicked(move |_| this.send_shortcut(&["ControlLeft", "AltLeft", "Delete"]));
        let this = self.clone();
        imp.paste_button.connect_clicked(move |_| this.show_paste_dialog());
        let this = self.clone();
        imp.apply_stream_button
            .connect_clicked(move |_| this.apply_stream_params());
        imp.capture_toggle.connect_toggled(|t| {
            state().update_settings(|s| s.ui.capture_input = t.is_active());
        });
    }

    pub fn atx_click(&self, button: AtxButton) {
        let Some(client) = self.client() else {
            toast("Not connected");
            return;
        };
        let run = move || {
            spawn_result(
                "ATX",
                async move { Ok(client.atx().click(button, false).await?) },
                |_| toast("ATX button sent"),
            );
        };
        if button.is_destructive() {
            confirm(
                self,
                "Press the reset button?",
                "The host will restart immediately and unsaved work is lost.",
                "Reset",
                run,
            );
        } else {
            run();
        }
    }

    pub fn send_shortcut(&self, keys: &[&str]) {
        let Some(client) = self.client() else {
            toast("Not connected");
            return;
        };
        let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
        spawn_result(
            "Shortcut",
            async move {
                let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
                Ok(client.hid().send_shortcut(&refs).await?)
            },
            |_| {},
        );
    }

    /// Dialog to type text on the host (`POST /api/hid/print`).
    pub fn show_paste_dialog(&self) {
        show_paste_dialog(self);
    }

    fn apply_stream_params(&self) {
        let Some(client) = self.client() else { return };
        let imp = self.imp();
        let update = StreamerParamsUpdate {
            quality: Some(imp.quality_row.value() as u32),
            desired_fps: Some(imp.fps_row.value() as u32),
            h264_bitrate: Some(imp.bitrate_row.value() as u32),
            h264_gop: Some(imp.gop_row.value() as u32),
        };
        spawn_result(
            "Stream parameters",
            async move { Ok(client.streamer().set_params(&update).await?) },
            |_| toast("Stream parameters applied"),
        );
    }

    // ----- connection / video ----------------------------------------------------------

    fn on_connection_changed(&self, connected: bool, status: &str) {
        let imp = self.imp();
        let has_profile = state().settings.borrow().active_profile().is_some();
        if connected {
            imp.video.add_css_class("pv-video");
            imp.placeholder.set_visible(false);
            imp.banner.set_visible(false);
            if imp.stream_cancel.borrow().is_none() {
                self.start_stream();
            }
        } else {
            self.stop_stream();
            imp.video.remove_css_class("pv-video");
            imp.video.set_paintable(None::<&gdk::Paintable>);
            if has_profile {
                imp.placeholder.set_title("Not connected");
                imp.placeholder.set_description(Some(status));
                imp.placeholder.set_icon_name(Some("network-offline-symbolic"));
            } else {
                imp.placeholder.set_title("No PiKVM configured");
                imp.placeholder
                    .set_description(Some("Add a connection profile to see the host screen."));
                imp.placeholder.set_icon_name(Some("network-server-symbolic"));
            }
            imp.placeholder.set_visible(true);
            imp.banner
                .set_visible(status.starts_with("Reconnecting") || status.starts_with("Connecting"));
            imp.banner.set_text(status);
        }
        self.refresh_pills();
    }

    fn stop_stream(&self) {
        if let Some(c) = self.imp().stream_cancel.borrow_mut().take() {
            c.cancel();
        }
    }

    fn start_stream(&self) {
        let Some(client) = self.client() else { return };
        let imp = self.imp();
        let cancel = CancellationToken::new();
        *imp.stream_cancel.borrow_mut() = Some(cancel.clone());
        let (tx, rx) = async_channel::bounded::<Frame>(1);
        let (status_tx, status_rx) = async_channel::unbounded::<String>();

        // Producer on tokio: MJPEG first, snapshot polling as fallback.
        runtime::spawn(async move {
            use futures_util::StreamExt;
            loop {
                if cancel.is_cancelled() {
                    break;
                }
                let mjpeg = tokio::select! {
                    _ = cancel.cancelled() => break,
                    r = pikvm::stream::open_mjpeg(&client) => r,
                };
                match mjpeg {
                    Ok(stream) => {
                        let _ = status_tx.send("mjpeg".into()).await;
                        let mut stream = std::pin::pin!(stream);
                        loop {
                            let next = tokio::select! {
                                _ = cancel.cancelled() => None,
                                n = stream.next() => n,
                            };
                            match next {
                                Some(Ok(jpeg)) => {
                                    if let Some(frame) = decode(&jpeg) {
                                        let _ = tx.force_send(frame);
                                    }
                                }
                                Some(Err(e)) => {
                                    tracing::warn!("mjpeg stream error: {e}");
                                    break;
                                }
                                None => break,
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("mjpeg unavailable ({e}); falling back to snapshots");
                        let _ = status_tx.send("snapshot".into()).await;
                        let mut snaps =
                            std::pin::pin!(pikvm::stream::snapshot_stream(client.clone(), Duration::from_secs(1)));
                        let mut failures = 0;
                        loop {
                            let next = tokio::select! {
                                _ = cancel.cancelled() => None,
                                n = snaps.next() => n,
                            };
                            match next {
                                Some(Ok(jpeg)) => {
                                    failures = 0;
                                    if let Some(frame) = decode(&jpeg) {
                                        let _ = tx.force_send(frame);
                                    }
                                }
                                Some(Err(_)) => {
                                    failures += 1;
                                    if failures > 5 {
                                        break;
                                    }
                                }
                                None => break,
                            }
                        }
                    }
                }
                if cancel.is_cancelled() {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });

        // Consumer on the main loop.
        let this = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(frame) = rx.recv().await {
                this.show_frame(frame);
            }
        });
        let this = self.clone();
        glib::spawn_future_local(async move {
            while let Ok(mode) = status_rx.recv().await {
                let imp = this.imp();
                if mode == "snapshot" {
                    imp.banner.set_text("Snapshot fallback (MJPEG unavailable)");
                    imp.banner.set_visible(true);
                } else {
                    imp.banner.set_visible(false);
                }
            }
        });
    }

    fn show_frame(&self, frame: Frame) {
        let imp = self.imp();
        let bytes = glib::Bytes::from_owned(frame.rgba);
        let texture = gdk::MemoryTexture::new(
            frame.width,
            frame.height,
            gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            (frame.width * 4) as usize,
        );
        imp.texture_size.set((frame.width, frame.height));
        imp.video.set_paintable(Some(&texture));
    }

    // ----- pills -------------------------------------------------------------------------

    fn refresh_pills(&self) {
        let imp = self.imp();
        let st = state();
        let connected = st.connection.store.is_connected();
        st.connection.store.with_state(|s| {
            if !connected {
                set_pill(&imp.video_pill, "Offline", Some("error"));
                set_pill(&imp.hid_pill, "HID", None);
                set_pill(&imp.atx_pill, "Power", None);
                set_pill(&imp.msd_pill, "MSD", None);
                return;
            }
            match s.streamer.resolution() {
                Some(r) if s.streamer.online() => set_pill(
                    &imp.video_pill,
                    &format!("{}×{} · {} fps", r.width, r.height, s.streamer.captured_fps()),
                    Some("success"),
                ),
                _ => set_pill(&imp.video_pill, "No signal", Some("warning")),
            }
            set_pill(
                &imp.hid_pill,
                if s.hid.online { "HID online" } else { "HID offline" },
                Some(if s.hid.online { "success" } else { "warning" }),
            );
            let power = s.atx.leds.power;
            set_pill(
                &imp.atx_pill,
                &format!(
                    "Power {}{}",
                    util::on_off(power),
                    if s.atx.leds.hdd { " · HDD" } else { "" }
                ),
                Some(if s.atx.busy {
                    "warning"
                } else if power {
                    "success"
                } else {
                    "error"
                }),
            );
            match s.msd.drive.image_name() {
                Some(name) if s.msd.drive.connected => set_pill(&imp.msd_pill, &format!("MSD: {name}"), Some("accent")),
                Some(name) => set_pill(&imp.msd_pill, &format!("MSD: {name} (detached)"), None),
                None => set_pill(&imp.msd_pill, "MSD idle", None),
            }
            imp.quality_row.set_value(f64::from(s.streamer.params.quality));
            imp.fps_row.set_value(f64::from(s.streamer.params.desired_fps));
            imp.bitrate_row.set_value(f64::from(s.streamer.params.h264_bitrate));
            imp.gop_row.set_value(f64::from(s.streamer.params.h264_gop));
        });
    }

    // ----- input forwarding -----------------------------------------------------------

    fn capturing(&self) -> bool {
        self.imp().capture_toggle.is_active() && state().connection.store.is_connected()
    }

    fn send_ws(&self, ev: OutEvent) {
        if let Some(ws) = state().connection.ws() {
            ws.send(ev);
        }
    }

    /// Map widget coordinates to the displayed frame (content-fit: contain).
    fn frame_point(&self, x: f64, y: f64) -> Option<MousePoint> {
        let imp = self.imp();
        let (tw, th) = imp.texture_size.get();
        if tw <= 0 || th <= 0 {
            return None;
        }
        let pw = f64::from(imp.video.width());
        let ph = f64::from(imp.video.height());
        let scale = (pw / f64::from(tw)).min(ph / f64::from(th));
        let dw = f64::from(tw) * scale;
        let dh = f64::from(th) * scale;
        let ox = (pw - dw) / 2.0;
        let oy = (ph - dh) / 2.0;
        if x < ox || y < oy || x > ox + dw || y > oy + dh {
            return None;
        }
        Some(MousePoint::from_frame(x - ox, y - oy, dw, dh))
    }

    fn setup_input(&self) {
        let imp = self.imp();
        let video = imp.video.get();

        // Keyboard.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let this = self.clone();
        keys.connect_key_pressed(move |_, _keyval, keycode, _state| {
            if !this.capturing() {
                return glib::Propagation::Proceed;
            }
            match web_name_for_gtk_keycode(keycode) {
                Some(name) => {
                    this.send_ws(OutEvent::Key {
                        key: name.to_string(),
                        state: true,
                    });
                    glib::Propagation::Stop
                }
                None => glib::Propagation::Proceed,
            }
        });
        let this = self.clone();
        keys.connect_key_released(move |_, _keyval, keycode, _state| {
            if !this.capturing() {
                return;
            }
            if let Some(name) = web_name_for_gtk_keycode(keycode) {
                this.send_ws(OutEvent::Key {
                    key: name.to_string(),
                    state: false,
                });
            }
        });
        video.add_controller(keys);

        // Mouse buttons.
        let click = gtk::GestureClick::new();
        click.set_button(0);
        let this = self.clone();
        click.connect_pressed(move |g, _, x, y| {
            this.imp().video.grab_focus();
            if !this.capturing() {
                return;
            }
            if let Some(p) = this.frame_point(x, y) {
                this.send_ws(OutEvent::MouseMove { to: p });
            }
            if let Some(b) = button_name(g.current_button()) {
                this.send_ws(OutEvent::MouseButton {
                    button: b.into(),
                    state: true,
                });
            }
        });
        let this = self.clone();
        click.connect_released(move |g, _, _, _| {
            if !this.capturing() {
                return;
            }
            if let Some(b) = button_name(g.current_button()) {
                this.send_ws(OutEvent::MouseButton {
                    button: b.into(),
                    state: false,
                });
            }
        });
        video.add_controller(click);

        // Motion (throttled to ~60 Hz).
        let motion = gtk::EventControllerMotion::new();
        let this = self.clone();
        motion.connect_motion(move |_, x, y| {
            if !this.capturing() {
                return;
            }
            let now = Instant::now();
            if let Some(last) = this.imp().last_motion.get() {
                if now.duration_since(last) < Duration::from_millis(16) {
                    return;
                }
            }
            this.imp().last_motion.set(Some(now));
            if let Some(p) = this.frame_point(x, y) {
                this.send_ws(OutEvent::MouseMove { to: p });
            }
        });
        video.add_controller(motion);

        // Wheel.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        let this = self.clone();
        scroll.connect_scroll(move |_, dx, dy| {
            if !this.capturing() {
                return glib::Propagation::Proceed;
            }
            let delta = MouseDelta {
                x: (dx.signum() * 5.0) as i32,
                y: (-dy.signum() * 5.0) as i32,
            };
            if delta.x != 0 || delta.y != 0 {
                this.send_ws(OutEvent::MouseWheel { delta, squash: true });
            }
            glib::Propagation::Stop
        });
        video.add_controller(scroll);

        // Release all modifier keys when focus leaves so nothing gets stuck on the host.
        let focus = gtk::EventControllerFocus::new();
        let this = self.clone();
        focus.connect_leave(move |_| {
            for key in [
                "ControlLeft",
                "ControlRight",
                "AltLeft",
                "AltRight",
                "ShiftLeft",
                "ShiftRight",
                "MetaLeft",
                "MetaRight",
            ] {
                this.send_ws(OutEvent::Key {
                    key: key.into(),
                    state: false,
                });
            }
        });
        video.add_controller(focus);
    }
}

/// Dialog to type text on the host (`POST /api/hid/print`); usable from any page.
pub fn show_paste_dialog(parent: &impl IsA<gtk::Widget>) {
    let Some(client) = state().connection.client() else {
        toast("Not connected");
        return;
    };
    let st = state();
    let keymaps = st.connection.store.with_state(|s| s.keymaps.clone());
    let dialog = adw::Dialog::builder()
        .title("Type text on the host")
        .content_width(520)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.set_margin_end(12);
    let text_view = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).build();
    if let Some(clip) = parent.clipboard().read_text_future().now_or_never_text() {
        text_view.buffer().set_text(&clip);
    }
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(160)
        .build();
    scrolled.add_css_class("card");
    content.append(&scrolled);
    let group = adw::PreferencesGroup::new();
    let names: Vec<String> = keymaps.as_ref().map(|k| k.available.clone()).unwrap_or_default();
    let default_idx = keymaps
        .as_ref()
        .and_then(|k| names.iter().position(|n| *n == k.default))
        .unwrap_or(0) as u32;
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let keymap_row = util::combo_row(
        &group,
        "Keymap",
        Some("Layout of the host keyboard"),
        &name_refs,
        default_idx,
    );
    keymap_row.set_visible(!names.is_empty());
    let slow_row = util::switch_row(&group, "Slow typing", Some("Larger pauses between keys"), false);
    content.append(&group);
    let send = util::suggested_button("Type");
    send.set_halign(gtk::Align::End);
    content.append(&send);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    let dialog_ref = dialog.clone();
    send.connect_clicked(move |_| {
        let buffer = text_view.buffer();
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).to_string();
        if text.is_empty() {
            return;
        }
        let keymap = names.get(keymap_row.selected() as usize).cloned();
        let opts = PrintOptions {
            keymap,
            limit: Some(0),
            slow: slow_row.is_active(),
            delay: None,
        };
        let client = client.clone();
        let dialog = dialog_ref.clone();
        spawn_result(
            "Type text",
            async move { Ok(client.hid().print(&text, &opts).await?) },
            move |_| {
                toast("Text typed");
                dialog.close();
            },
        );
    });
    dialog.present(Some(parent));
}

fn button_name(button: u32) -> Option<&'static str> {
    match button {
        1 => Some("left"),
        2 => Some("middle"),
        3 => Some("right"),
        _ => None,
    }
}

fn decode(jpeg: &[u8]) -> Option<Frame> {
    let img = image::load_from_memory(jpeg).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    Some(Frame {
        width: w as i32,
        height: h as i32,
        rgba: img.into_raw(),
    })
}

/// Tiny extension so the paste dialog can prefill from the clipboard without blocking.
trait NowOrNever {
    fn now_or_never_text(self) -> Option<String>;
}

impl<F: std::future::Future<Output = Result<Option<glib::GString>, glib::Error>>> NowOrNever for F {
    fn now_or_never_text(self) -> Option<String> {
        use futures_util::FutureExt;
        self.now_or_never()
            .and_then(|r| r.ok().flatten())
            .map(|s| s.to_string())
    }
}
