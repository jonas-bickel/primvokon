//! PiKVM Switch card (visible when a Switch is detected).

use std::cell::RefCell;

use adw::prelude::*;
use pikvm::events::KvmState;
use pikvm::models::{AtxButton, BeaconTarget, PowerAction, SwitchPortParams, SwitchState};

use super::{call, Card};
use crate::util::{self, confirm};

pub struct SwitchCard {
    group: adw::PreferencesGroup,
    summary_row: adw::ActionRow,
    ports_rows: RefCell<Vec<gtk::Widget>>,
    ports_fingerprint: RefCell<String>,
    advanced: RefCell<Vec<gtk::Widget>>,
    edids_row: adw::ExpanderRow,
    edid_rows: RefCell<Vec<gtk::Widget>>,
}

impl SwitchCard {
    pub fn new() -> Self {
        let group = util::card("PiKVM Switch", Some("Multi-port switch attached to the PiKVM"));
        group.set_visible(false);
        let summary_row = util::kv_row(&group, "Active port");
        let prev = util::button("Previous");
        prev.connect_clicked(|_| call("Switch", |c| async move { c.switch().set_active_prev().await }));
        let next = util::button("Next");
        next.connect_clicked(|_| call("Switch", |c| async move { c.switch().set_active_next().await }));
        summary_row.add_suffix(&util::button_box(&[&prev, &next]));

        let mut advanced: Vec<gtk::Widget> = Vec::new();
        let colors = adw::EntryRow::builder()
            .title("Beacon colour RRGGBB:BRIGHTNESS:BLINK (e.g. FFA500:BF:0028)")
            .build();
        let apply = util::button("Apply");
        apply.set_valign(gtk::Align::Center);
        let cref = colors.clone();
        apply.connect_clicked(move |_| {
            let spec = cref.text().to_string();
            call(
                "Switch colours",
                move |c| async move { c.switch().set_colors(&spec).await },
            );
        });
        colors.add_suffix(&apply);
        group.add(&colors);
        advanced.push(colors.upcast());

        let unit = util::spin_row(&group, "Reboot unit", Some("Unit number 0..4"), 0.0, 4.0, 1.0, 0.0);
        let reboot = util::destructive_button("Reboot");
        let bootloader = util::destructive_button("Bootloader");
        let u = unit.clone();
        reboot.connect_clicked(move |b| {
            let unit = u.value() as u32;
            confirm(
                b,
                "Reboot switch unit?",
                "The switch restarts; video may drop briefly.",
                "Reboot",
                move || {
                    call(
                        "Switch reset",
                        move |c| async move { c.switch().reset(unit, false).await },
                    )
                },
            );
        });
        let u = unit.clone();
        bootloader.connect_clicked(move |b| {
            let unit = u.value() as u32;
            confirm(
                b,
                "Enter bootloader?",
                "The unit reboots into reflashing mode.",
                "Reboot",
                move || {
                    call(
                        "Switch reset",
                        move |c| async move { c.switch().reset(unit, true).await },
                    )
                },
            );
        });
        unit.add_suffix(&util::button_box(&[&reboot, &bootloader]));
        advanced.push(unit.upcast());

        let edids_row = adw::ExpanderRow::builder().title("EDID configurations").build();
        let create = util::button("Create EDID…");
        create.set_valign(gtk::Align::Center);
        create.connect_clicked(|b| edid_dialog(b, None));
        edids_row.add_suffix(&create);
        group.add(&edids_row);
        advanced.push(edids_row.clone().upcast());

        Self {
            group,
            summary_row,
            ports_rows: RefCell::new(Vec::new()),
            ports_fingerprint: RefCell::new(String::new()),
            advanced: RefCell::new(advanced),
            edids_row,
            edid_rows: RefCell::new(Vec::new()),
        }
    }

    fn rebuild_ports(&self, sw: &SwitchState, advanced: bool) {
        for w in self.ports_rows.borrow_mut().drain(..) {
            self.group.remove(&w);
        }
        for (i, port) in sw.model.ports.iter().enumerate() {
            let idx = i as f64;
            let active = sw.summary.active_port == i as i64;
            let title = if port.name.is_empty() {
                format!("Port {}", port.id)
            } else {
                format!("Port {} · {}", port.id, port.name)
            };
            let row = adw::ActionRow::builder().title(title).build();
            let mut subtitle = Vec::new();
            if active {
                subtitle.push("active".to_string());
            }
            if let Some(v) = sw.video.links.get(i) {
                subtitle.push(format!("video {}", if *v { "linked" } else { "no link" }));
            }
            if let Some(u) = sw.usb.links.get(i) {
                subtitle.push(format!("usb {}", if *u { "linked" } else { "no link" }));
            }
            if let Some(p) = sw.atx.leds.power.get(i) {
                subtitle.push(format!("power {}", util::on_off(*p)));
            }
            row.set_subtitle(&subtitle.join(" · "));
            let activate = if active {
                util::suggested_button("Active")
            } else {
                util::button("Activate")
            };
            activate.set_sensitive(!active);
            activate.connect_clicked(move |_| {
                call("Switch port", move |c| async move { c.switch().set_active(idx).await })
            });
            let beacon = gtk::ToggleButton::builder()
                .icon_name("weather-clear-symbolic")
                .tooltip_text("Beacon")
                .build();
            beacon.set_active(sw.beacons.ports.get(i).copied().unwrap_or(false));
            beacon.connect_toggled(move |t| {
                let state = t.is_active();
                call("Beacon", move |c| async move {
                    c.switch().set_beacon(BeaconTarget::Port(idx), state).await
                })
            });
            beacon.set_valign(gtk::Align::Center);
            let mut widgets: Vec<gtk::Widget> = vec![activate.upcast(), beacon.upcast()];
            if advanced {
                let power = util::button("Power");
                power.connect_clicked(move |b| {
                    confirm(
                        b,
                        "Click the power button?",
                        "Short press on this port's ATX.",
                        "Press",
                        move || {
                            call("Port ATX", move |c| async move {
                                c.switch().atx_click(idx, AtxButton::Power).await
                            })
                        },
                    )
                });
                let reset = util::destructive_button("Hard reset");
                reset.connect_clicked(move |b| {
                    confirm(
                        b,
                        "Hard reset this port?",
                        "The host on this port restarts immediately.",
                        "Reset",
                        move || {
                            call("Port ATX", move |c| async move {
                                c.switch().atx_power(idx, PowerAction::ResetHard).await
                            })
                        },
                    )
                });
                let params = util::button("Params…");
                let port_name = port.name.clone();
                let edid_ids: Vec<(String, String)> = sw
                    .edids
                    .all
                    .iter()
                    .map(|(id, e)| (id.clone(), e.name.clone()))
                    .collect();
                let current_edid = sw.edids.used.get(i).cloned().unwrap_or_default();
                let dummy = port.video["dummy"].as_bool().unwrap_or(false);
                params.connect_clicked(move |b| {
                    port_params_dialog(b, idx, &port_name, dummy, &current_edid, &edid_ids);
                });
                widgets.push(power.upcast());
                widgets.push(reset.upcast());
                widgets.push(params.upcast());
            }
            let bbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            bbox.set_valign(gtk::Align::Center);
            for w in &widgets {
                bbox.append(w);
            }
            row.add_suffix(&bbox);
            self.group.add(&row);
            self.ports_rows.borrow_mut().push(row.upcast());
        }

        for w in self.edid_rows.borrow_mut().drain(..) {
            self.edids_row.remove(&w);
        }
        for (id, edid) in &sw.edids.all {
            let row = adw::ActionRow::builder().title(&edid.name).subtitle(id).build();
            let edit = util::button("Edit");
            let (id_c, name_c, data_c) = (id.clone(), edid.name.clone(), edid.data.clone());
            edit.connect_clicked(move |b| edid_dialog(b, Some((id_c.clone(), name_c.clone(), data_c.clone()))));
            let remove = util::destructive_button("Remove");
            let id_c = id.clone();
            remove.connect_clicked(move |b| {
                let id = id_c.clone();
                confirm(
                    b,
                    "Remove EDID?",
                    "Ports using it fall back to the default.",
                    "Remove",
                    move || call("EDID", move |c| async move { c.switch().edid_remove(&id).await }),
                )
            });
            row.add_suffix(&util::button_box(&[&edit, &remove]));
            self.edids_row.add_row(&row);
            self.edid_rows.borrow_mut().push(row.upcast());
        }
    }
}

fn port_params_dialog(
    parent: &impl IsA<gtk::Widget>,
    port: f64,
    name: &str,
    dummy: bool,
    current_edid: &str,
    edids: &[(String, String)],
) {
    let dialog = adw::Dialog::builder()
        .title(format!("Port {} parameters", port as i64 + 1))
        .content_width(480)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let group = adw::PreferencesGroup::new();
    group.set_margin_top(12);
    group.set_margin_bottom(12);
    group.set_margin_start(12);
    group.set_margin_end(12);
    let name_row = util::entry_row(&group, "Name", name);
    let dummy_row = util::switch_row(&group, "Dummy display", Some("Pretend a monitor is attached"), dummy);
    let mut labels: Vec<String> = vec!["(default)".into()];
    labels.extend(edids.iter().map(|(_, n)| n.clone()));
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let selected = edids
        .iter()
        .position(|(id, _)| id == current_edid)
        .map(|i| i as u32 + 1)
        .unwrap_or(0);
    let edid_row = util::combo_row(&group, "EDID", None, &refs, selected);
    let p_delay = util::spin_row(&group, "ATX power click delay (s)", None, 0.0, 10.0, 0.1, 0.5);
    let pl_delay = util::spin_row(&group, "ATX power long delay (s)", None, 0.0, 10.0, 0.1, 5.0);
    let r_delay = util::spin_row(&group, "ATX reset delay (s)", None, 0.0, 10.0, 0.1, 0.5);
    let apply = util::suggested_button("Apply");
    apply.set_halign(gtk::Align::End);
    let edid_ids: Vec<String> = edids.iter().map(|(id, _)| id.clone()).collect();
    let d = dialog.clone();
    apply.connect_clicked(move |_| {
        let params = SwitchPortParams {
            edid_id: match edid_row.selected() {
                0 => Some(String::new()),
                i => edid_ids.get(i as usize - 1).cloned(),
            },
            dummy: Some(dummy_row.is_active()),
            name: Some(name_row.text().to_string()),
            atx_click_power_delay: Some(p_delay.value()),
            atx_click_power_long_delay: Some(pl_delay.value()),
            atx_click_reset_delay: Some(r_delay.value()),
        };
        call("Port parameters", move |c| async move {
            c.switch().set_port_params(port, &params).await
        });
        d.close();
    });
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&group);
    content.append(&apply);
    apply.set_margin_end(12);
    apply.set_margin_bottom(12);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}

fn edid_dialog(parent: &impl IsA<gtk::Widget>, existing: Option<(String, String, String)>) {
    let editing = existing.is_some();
    let dialog = adw::Dialog::builder()
        .title(if editing { "Change EDID" } else { "Create EDID" })
        .content_width(560)
        .build();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    let group = adw::PreferencesGroup::new();
    group.set_margin_top(12);
    group.set_margin_bottom(12);
    group.set_margin_start(12);
    group.set_margin_end(12);
    let (id, name, data) = existing.unwrap_or_default();
    let name_row = util::entry_row(&group, "Name", &name);
    let data_row = util::entry_row(&group, "EDID data (hex)", &data);
    let save = util::suggested_button(if editing { "Change" } else { "Create" });
    save.set_halign(gtk::Align::End);
    save.set_margin_end(12);
    save.set_margin_bottom(12);
    let d = dialog.clone();
    save.connect_clicked(move |_| {
        let (n, dat, id) = (name_row.text().to_string(), data_row.text().to_string(), id.clone());
        if editing {
            call("EDID", move |c| async move {
                c.switch().edid_change(&id, Some(&n), Some(&dat)).await
            });
        } else {
            call("EDID", move |c| async move { c.switch().edid_create(&n, &dat).await });
        }
        d.close();
    });
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&group);
    content.append(&save);
    toolbar.set_content(Some(&content));
    dialog.set_child(Some(&toolbar));
    dialog.present(Some(parent));
}

impl Card for SwitchCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        let present = connected && s.switch_present();
        self.group.set_visible(present);
        let Some(sw) = s.switch.as_ref().filter(|_| present) else {
            return;
        };
        self.summary_row.set_subtitle(&format!(
            "{} · {} port(s) · {}",
            if sw.summary.active_id.is_empty() {
                "none".to_string()
            } else {
                sw.summary.active_id.clone()
            },
            sw.model.ports.len(),
            if sw.summary.synced { "synced" } else { "syncing" }
        ));
        let advanced = self.advanced.borrow().first().map(|w| w.is_visible()).unwrap_or(false);
        let fingerprint = format!("{}|{advanced}", serde_json::to_string(sw).unwrap_or_default());
        if *self.ports_fingerprint.borrow() != fingerprint {
            self.rebuild_ports(sw, advanced);
            *self.ports_fingerprint.borrow_mut() = fingerprint;
        }
    }

    fn set_advanced(&self, advanced: bool) {
        for w in self.advanced.borrow().iter() {
            w.set_visible(advanced);
        }
        // Force a port rebuild with the new button set on the next update.
        self.ports_fingerprint.borrow_mut().clear();
    }
}
