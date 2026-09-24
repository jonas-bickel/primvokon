//! Keyboard & mouse (HID) card.

use adw::prelude::*;
use pikvm::api::hid::SHORTCUTS;
use pikvm::events::KvmState;
use pikvm::models::{KeyboardOutput, MouseButton, MouseOutput};

use super::{call, Card};
use crate::util::{self, set_pill};

pub struct HidCard {
    group: adw::PreferencesGroup,
    status_pill: gtk::Label,
    leds_row: adw::ActionRow,
    outputs_row: adw::ActionRow,
    jiggler_row: adw::ActionRow,
    advanced: Vec<gtk::Widget>,
    keyboard_output: adw::ComboRow,
    mouse_output: adw::ComboRow,
    jiggler_switch: adw::SwitchRow,
}

const KEYBOARD_OUTPUTS: [KeyboardOutput; 3] = [KeyboardOutput::Usb, KeyboardOutput::Ps2, KeyboardOutput::Disabled];
const MOUSE_OUTPUTS: [MouseOutput; 5] = [
    MouseOutput::Usb,
    MouseOutput::UsbWin98,
    MouseOutput::UsbRel,
    MouseOutput::Ps2,
    MouseOutput::Disabled,
];

impl HidCard {
    pub fn new() -> Self {
        let group = util::card("Keyboard & mouse", Some("Emulated HID devices of the PiKVM"));
        let status_pill = util::make_pill("offline", None);
        let status = adw::ActionRow::builder().title("Status").build();
        status.add_suffix(&status_pill);
        group.add(&status);
        let leds_row = util::kv_row(&group, "Keyboard LEDs");
        let outputs_row = util::kv_row(&group, "Outputs");
        let jiggler_row = util::kv_row(&group, "Mouse jiggler");

        // Type text.
        let type_button = util::suggested_button("Type text…");
        type_button.connect_clicked(crate::ui::console::show_paste_dialog);
        util::button_row(
            &group,
            "Type text",
            Some("Paste text into the host via POST /api/hid/print"),
            &type_button,
        );

        // Shortcut palette.
        let palette = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(6)
            .column_spacing(6)
            .row_spacing(6)
            .build();
        for (label, keys) in SHORTCUTS {
            let b = util::button(label);
            b.add_css_class("flat");
            let keys: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
            b.connect_clicked(move |_| {
                let keys = keys.clone();
                call("Shortcut", move |c| async move {
                    let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
                    c.hid().send_shortcut(&refs).await
                });
            });
            palette.append(&b);
        }
        let palette_row = adw::ExpanderRow::builder()
            .title("Shortcut palette")
            .subtitle("Common key combinations")
            .build();
        let holder = gtk::Box::new(gtk::Orientation::Vertical, 6);
        holder.set_margin_top(6);
        holder.set_margin_bottom(6);
        holder.set_margin_start(12);
        holder.set_margin_end(12);
        holder.append(&palette);
        let custom = adw::EntryRow::builder()
            .title("Custom shortcut (web names, comma separated)")
            .text("ControlLeft,KeyL")
            .build();
        let send = util::button("Send");
        send.set_valign(gtk::Align::Center);
        let custom_ref = custom.clone();
        send.connect_clicked(move |_| {
            let keys: Vec<String> = custom_ref
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            call("Shortcut", move |c| async move {
                let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
                c.hid().send_shortcut(&refs).await
            });
        });
        custom.add_suffix(&send);
        holder.append(&custom);
        palette_row.add_row(&holder);
        group.add(&palette_row);

        let mut advanced: Vec<gtk::Widget> = Vec::new();

        // Single key.
        let key_row = adw::EntryRow::builder()
            .title("Single key (web name, e.g. Escape)")
            .build();
        let press = util::button("Press+release");
        let key_ref = key_row.clone();
        press.connect_clicked(move |_| {
            let key = key_ref.text().to_string();
            call("Key", move |c| async move { c.hid().send_key(&key, None, true).await });
        });
        press.set_valign(gtk::Align::Center);
        key_row.add_suffix(&press);
        group.add(&key_row);
        advanced.push(key_row.upcast());

        // Mouse.
        let mouse_row = adw::ExpanderRow::builder()
            .title("Mouse events")
            .subtitle("Absolute / relative moves, buttons, wheel")
            .build();
        let mouse_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        mouse_box.set_margin_top(6);
        mouse_box.set_margin_bottom(6);
        mouse_box.set_margin_start(12);
        mouse_box.set_margin_end(12);
        let mgroup = adw::PreferencesGroup::new();
        let button_combo = util::combo_row(&mgroup, "Button", None, &["left", "middle", "right", "up", "down"], 0);
        let click = util::button("Click");
        click.set_valign(gtk::Align::Center);
        let combo_ref = button_combo.clone();
        click.connect_clicked(move |_| {
            let button = [
                MouseButton::Left,
                MouseButton::Middle,
                MouseButton::Right,
                MouseButton::Up,
                MouseButton::Down,
            ][combo_ref.selected() as usize];
            call("Mouse button", move |c| async move {
                c.hid().send_mouse_button(button, Some(true)).await?;
                c.hid().send_mouse_button(button, Some(false)).await
            });
        });
        button_combo.add_suffix(&click);
        let move_x = util::spin_row(
            &mgroup,
            "Move to X",
            Some("0,0 is the centre; −32768..32767"),
            -32768.0,
            32767.0,
            100.0,
            0.0,
        );
        let move_y = util::spin_row(&mgroup, "Move to Y", None, -32768.0, 32767.0, 100.0, 0.0);
        let mv = util::button("Move");
        mv.set_valign(gtk::Align::Center);
        let (mx, my) = (move_x.clone(), move_y.clone());
        mv.connect_clicked(move |_| {
            let (x, y) = (mx.value() as i32, my.value() as i32);
            call(
                "Mouse move",
                move |c| async move { c.hid().send_mouse_move(x, y).await },
            );
        });
        move_y.add_suffix(&mv);
        let rel_x = util::spin_row(&mgroup, "Relative ΔX", None, -1000.0, 1000.0, 10.0, 0.0);
        let rel_y = util::spin_row(&mgroup, "Relative ΔY", None, -1000.0, 1000.0, 10.0, 0.0);
        let rel = util::button("Move relative");
        rel.set_valign(gtk::Align::Center);
        let (rx, ry) = (rel_x.clone(), rel_y.clone());
        rel.connect_clicked(move |_| {
            let (x, y) = (rx.value() as i32, ry.value() as i32);
            call("Mouse relative", move |c| async move {
                c.hid().send_mouse_relative(x, y).await
            });
        });
        rel_y.add_suffix(&rel);
        let wheel_y = util::spin_row(
            &mgroup,
            "Wheel ΔY",
            Some("Positive scrolls up"),
            -1000.0,
            1000.0,
            5.0,
            5.0,
        );
        let wheel = util::button("Scroll");
        wheel.set_valign(gtk::Align::Center);
        let wy = wheel_y.clone();
        wheel.connect_clicked(move |_| {
            let y = wy.value() as i32;
            call(
                "Mouse wheel",
                move |c| async move { c.hid().send_mouse_wheel(0, y).await },
            );
        });
        wheel_y.add_suffix(&wheel);
        mouse_box.append(&mgroup);
        mouse_row.add_row(&mouse_box);
        group.add(&mouse_row);
        advanced.push(mouse_row.upcast());

        // Parameters.
        let params_row = adw::ExpanderRow::builder()
            .title("Device parameters")
            .subtitle("Emulated device types, jiggler, connection, reset")
            .build();
        let pbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        pbox.set_margin_top(6);
        pbox.set_margin_bottom(6);
        pbox.set_margin_start(12);
        pbox.set_margin_end(12);
        let pgroup = adw::PreferencesGroup::new();
        let kb_labels: Vec<&str> = KEYBOARD_OUTPUTS.iter().map(|k| k.as_str()).collect();
        let keyboard_output = util::combo_row(&pgroup, "Keyboard output", None, &kb_labels, 0);
        let m_labels: Vec<&str> = MOUSE_OUTPUTS.iter().map(|m| m.as_str()).collect();
        let mouse_output = util::combo_row(&pgroup, "Mouse output", None, &m_labels, 0);
        let jiggler_switch = util::switch_row(&pgroup, "Mouse jiggler", Some("Keeps the host awake"), false);
        let apply = util::suggested_button("Apply parameters");
        let (ko, mo, js) = (keyboard_output.clone(), mouse_output.clone(), jiggler_switch.clone());
        apply.connect_clicked(move |_| {
            let k = KEYBOARD_OUTPUTS[ko.selected() as usize];
            let m = MOUSE_OUTPUTS[mo.selected() as usize];
            let j = js.is_active();
            call("HID parameters", move |c| async move {
                c.hid().set_params(Some(k), Some(m), Some(j)).await
            });
        });
        let connect = util::button("Connect");
        connect.connect_clicked(|_| call("HID connect", |c| async move { c.hid().set_connected(true).await }));
        let disconnect = util::button("Disconnect");
        disconnect.connect_clicked(|_| call("HID disconnect", |c| async move { c.hid().set_connected(false).await }));
        let reset = util::destructive_button("Reset HID");
        reset.connect_clicked(|_| call("HID reset", |c| async move { c.hid().reset().await }));
        let arow = adw::ActionRow::builder().title("Actions").build();
        arow.add_suffix(&util::button_box(&[&apply, &connect, &disconnect, &reset]));
        pgroup.add(&arow);
        pbox.append(&pgroup);
        params_row.add_row(&pbox);
        group.add(&params_row);
        advanced.push(params_row.upcast());

        Self {
            group,
            status_pill,
            leds_row,
            outputs_row,
            jiggler_row,
            advanced,
            keyboard_output,
            mouse_output,
            jiggler_switch,
        }
    }
}

impl Card for HidCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        self.group.set_visible(connected);
        let h = &s.hid;
        set_pill(
            &self.status_pill,
            &format!(
                "{}{}",
                if h.online { "online" } else { "offline" },
                if h.busy { " · busy" } else { "" }
            ),
            Some(if h.online { "success" } else { "warning" }),
        );
        let leds = &h.keyboard.leds;
        self.leds_row.set_subtitle(&format!(
            "Caps {} · Num {} · Scroll {}",
            util::on_off(leds.caps),
            util::on_off(leds.num),
            util::on_off(leds.scroll)
        ));
        self.outputs_row.set_subtitle(&format!(
            "keyboard: {} ({}) · mouse: {} ({}), {}",
            or_dash(&h.keyboard.outputs.active),
            if h.keyboard.online { "online" } else { "offline" },
            or_dash(&h.mouse.outputs.active),
            if h.mouse.online { "online" } else { "offline" },
            if h.mouse.absolute { "absolute" } else { "relative" }
        ));
        self.jiggler_row.set_subtitle(&format!(
            "{} · {} · every {} s",
            if h.jiggler.enabled { "enabled" } else { "disabled" },
            if h.jiggler.active { "active" } else { "inactive" },
            h.jiggler.interval
        ));
        if let Some(i) = KEYBOARD_OUTPUTS
            .iter()
            .position(|k| k.as_str() == h.keyboard.outputs.active)
        {
            self.keyboard_output.set_selected(i as u32);
        }
        if let Some(i) = MOUSE_OUTPUTS.iter().position(|m| m.as_str() == h.mouse.outputs.active) {
            self.mouse_output.set_selected(i as u32);
        }
        self.jiggler_switch.set_active(h.jiggler.active);
    }

    fn set_advanced(&self, advanced: bool) {
        for w in &self.advanced {
            w.set_visible(advanced);
        }
    }
}

fn or_dash(s: &str) -> &str {
    if s.is_empty() {
        "—"
    } else {
        s
    }
}
