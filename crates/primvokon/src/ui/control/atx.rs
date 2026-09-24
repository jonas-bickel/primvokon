//! ATX power card.

use adw::prelude::*;
use pikvm::events::KvmState;
use pikvm::models::{AtxButton, PowerAction};

use super::{call, Card};
use crate::util::{self, confirm, make_led, set_led, set_pill};

pub struct AtxCard {
    group: adw::PreferencesGroup,
    power_led: gtk::Box,
    hdd_led: gtk::Box,
    busy_pill: gtk::Label,
    core_buttons: Vec<gtk::Button>,
    advanced_row: adw::ActionRow,
    advanced_buttons: Vec<gtk::Button>,
}

impl AtxCard {
    pub fn new() -> Self {
        let group = util::card(
            "ATX power",
            Some("Power and reset buttons of the host, LED state from the ATX board"),
        );
        let status = adw::ActionRow::builder().title("Status").build();
        let power_led = make_led();
        let hdd_led = make_led();
        let leds = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        leds.set_valign(gtk::Align::Center);
        leds.append(&gtk::Label::new(Some("Power")));
        leds.append(&power_led);
        leds.append(&gtk::Label::new(Some("HDD")));
        leds.append(&hdd_led);
        let busy_pill = util::make_pill("idle", None);
        leds.append(&busy_pill);
        status.add_suffix(&leds);
        group.add(&status);

        let on = util::suggested_button("Power on");
        let off = util::button("Power off");
        let click = util::button("Click power");
        wire_power(&on, PowerAction::On);
        wire_power(&off, PowerAction::Off);
        wire_click(&click, AtxButton::Power);
        let row = adw::ActionRow::builder()
            .title("Actions")
            .subtitle("Soft actions")
            .build();
        row.add_suffix(&util::button_box(&[&on, &off, &click]));
        group.add(&row);

        let off_hard = util::destructive_button("Hard off");
        let reset_hard = util::destructive_button("Hard reset");
        let power_long = util::destructive_button("Power long");
        let reset = util::destructive_button("Click reset");
        wire_power(&off_hard, PowerAction::OffHard);
        wire_power(&reset_hard, PowerAction::ResetHard);
        wire_click(&power_long, AtxButton::PowerLong);
        wire_click(&reset, AtxButton::Reset);
        let advanced_row = adw::ActionRow::builder()
            .title("Hard actions")
            .subtitle("Ask for confirmation")
            .build();
        advanced_row.add_suffix(&util::button_box(&[&off_hard, &reset_hard, &power_long, &reset]));
        group.add(&advanced_row);

        Self {
            group,
            power_led,
            hdd_led,
            busy_pill,
            core_buttons: vec![on, off, click],
            advanced_row,
            advanced_buttons: vec![off_hard, reset_hard, power_long, reset],
        }
    }
}

fn wire_power(button: &gtk::Button, action: PowerAction) {
    button.connect_clicked(move |b| {
        let run = move || call("ATX power", move |c| async move { c.atx().power(action, false).await });
        if action.is_destructive() {
            confirm(
                b,
                "Hard power action?",
                &format!("Send `{}` to the host. Unsaved work is lost.", action.as_str()),
                "Proceed",
                run,
            );
        } else {
            run();
        }
    });
}

fn wire_click(button: &gtk::Button, atx_button: AtxButton) {
    button.connect_clicked(move |b| {
        let run = move || {
            call(
                "ATX click",
                move |c| async move { c.atx().click(atx_button, false).await },
            )
        };
        if atx_button.is_destructive() {
            confirm(
                b,
                "Press this button?",
                &format!("Send `{}` to the host.", atx_button.as_str()),
                "Press",
                run,
            );
        } else {
            run();
        }
    });
}

impl Card for AtxCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        self.group.set_visible(connected);
        set_led(&self.power_led, s.atx.leds.power, false);
        set_led(&self.hdd_led, s.atx.leds.hdd, false);
        set_pill(
            &self.busy_pill,
            if !s.atx.enabled {
                "disabled"
            } else if s.atx.busy {
                "busy"
            } else {
                "idle"
            },
            if s.atx.busy { Some("warning") } else { None },
        );
        let enabled = s.atx.enabled && !s.atx.busy;
        for b in self.core_buttons.iter().chain(&self.advanced_buttons) {
            b.set_sensitive(enabled);
        }
    }

    fn set_advanced(&self, advanced: bool) {
        self.advanced_row.set_visible(advanced);
    }
}
