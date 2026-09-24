//! GPIO card rendered from kvmd's view table.

use std::cell::RefCell;

use adw::prelude::*;
use pikvm::events::KvmState;
use pikvm::models::GpioModel;

use super::{call, Card};
use crate::util::{self, make_led, set_led};

pub struct GpioCard {
    group: adw::PreferencesGroup,
    rows: RefCell<Vec<gtk::Widget>>,
    leds: RefCell<Vec<(String, gtk::Box)>>,
    switches: RefCell<Vec<(String, gtk::Switch)>>,
    buttons: RefCell<Vec<(String, gtk::Button)>>,
    model_fingerprint: RefCell<String>,
}

impl GpioCard {
    pub fn new() -> Self {
        let group = util::card("GPIO", Some("User GPIO channels as configured on the PiKVM"));
        group.set_visible(false);
        Self {
            group,
            rows: RefCell::new(Vec::new()),
            leds: RefCell::new(Vec::new()),
            switches: RefCell::new(Vec::new()),
            buttons: RefCell::new(Vec::new()),
            model_fingerprint: RefCell::new(String::new()),
        }
    }

    fn rebuild(&self, model: &GpioModel) {
        for w in self.rows.borrow_mut().drain(..) {
            self.group.remove(&w);
        }
        self.leds.borrow_mut().clear();
        self.switches.borrow_mut().clear();
        self.buttons.borrow_mut().clear();
        self.group.set_title(&model.view.title());
        for row in model.view.table.iter().flatten() {
            let labels: Vec<&str> = row
                .iter()
                .filter(|c| c.kind == "label")
                .map(|c| c.text.as_str())
                .collect();
            let action_row = adw::ActionRow::builder().title(labels.join(" ")).build();
            for cell in row {
                match cell.kind.as_str() {
                    "input" => {
                        let led = make_led();
                        led.set_tooltip_text(Some(&cell.channel));
                        action_row.add_suffix(&led);
                        self.leds.borrow_mut().push((cell.channel.clone(), led));
                    }
                    "output" => {
                        let is_switch = model
                            .scheme
                            .outputs
                            .get(&cell.channel)
                            .map(|o| o.switch)
                            .unwrap_or(false);
                        if is_switch {
                            let sw = gtk::Switch::builder().valign(gtk::Align::Center).build();
                            let channel = cell.channel.clone();
                            let confirm_needed = cell.confirm;
                            sw.connect_state_set(move |w, state| {
                                let channel = channel.clone();
                                let run = move || {
                                    call("GPIO switch", move |c| async move {
                                        c.gpio().switch(&channel, state, false).await
                                    })
                                };
                                if confirm_needed {
                                    util::confirm(w, "Switch channel?", "Confirm the GPIO change.", "Switch", run);
                                } else {
                                    run();
                                }
                                gtk::glib::Propagation::Proceed
                            });
                            action_row.add_suffix(&sw);
                            self.switches.borrow_mut().push((cell.channel.clone(), sw));
                        } else {
                            let text = if cell.text.is_empty() {
                                "Pulse"
                            } else {
                                cell.text.as_str()
                            };
                            let b = util::button(text);
                            let channel = cell.channel.clone();
                            let confirm_needed = cell.confirm;
                            b.connect_clicked(move |w| {
                                let channel = channel.clone();
                                let run = move || {
                                    call("GPIO pulse", move |c| async move {
                                        c.gpio().pulse(&channel, None, false).await
                                    })
                                };
                                if confirm_needed {
                                    util::confirm(w, "Pulse channel?", "Confirm the GPIO pulse.", "Pulse", run);
                                } else {
                                    run();
                                }
                            });
                            b.set_valign(gtk::Align::Center);
                            action_row.add_suffix(&b);
                            self.buttons.borrow_mut().push((cell.channel.clone(), b));
                        }
                    }
                    _ => {}
                }
            }
            self.group.add(&action_row);
            self.rows.borrow_mut().push(action_row.upcast());
        }
    }
}

impl Card for GpioCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        let present = connected && s.gpio_present();
        self.group.set_visible(present);
        if !present {
            return;
        }
        let fingerprint = serde_json::to_string(&s.gpio_model.view).unwrap_or_default();
        if *self.model_fingerprint.borrow() != fingerprint {
            self.rebuild(&s.gpio_model);
            *self.model_fingerprint.borrow_mut() = fingerprint;
        }
        for (channel, led) in self.leds.borrow().iter() {
            if let Some(st) = s.gpio.inputs.get(channel) {
                set_led(led, st.state, !st.online);
            }
        }
        for (channel, sw) in self.switches.borrow().iter() {
            if let Some(st) = s.gpio.outputs.get(channel) {
                sw.set_sensitive(st.online && !st.busy);
                if sw.state() != st.state {
                    sw.set_state(st.state);
                    sw.set_active(st.state);
                }
            }
        }
        for (channel, b) in self.buttons.borrow().iter() {
            if let Some(st) = s.gpio.outputs.get(channel) {
                b.set_sensitive(st.online && !st.busy);
            }
        }
    }

    fn set_advanced(&self, _advanced: bool) {}
}
