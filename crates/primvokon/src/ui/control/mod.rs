//! Control centre: one card per PiKVM subsystem plus log viewer and API explorer sub-pages.

mod atx;
mod explorer;
mod gpio;
mod hid;
mod log;
mod msd;
mod streamer;
mod switch;
mod system;

pub use streamer::show_text_dialog;

use std::cell::RefCell;
use std::future::Future;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use pikvm::events::KvmState;
use pikvm::PikvmClient;

use crate::state::state;
use crate::util::{self, spawn_result, toast};

/// A card that renders part of the KVM state.
pub trait Card {
    fn group(&self) -> &adw::PreferencesGroup;
    fn update(&self, state: &KvmState, connected: bool);
    fn set_advanced(&self, advanced: bool);
}

/// Active client or a toast.
pub fn client() -> Option<PikvmClient> {
    let c = state().connection.client();
    if c.is_none() {
        toast("Not connected");
    }
    c
}

/// Run an API call with the active client and toast the outcome.
pub fn call<F>(context: &'static str, f: impl FnOnce(PikvmClient) -> F)
where
    F: Future<Output = pikvm::Result<()>> + Send + 'static,
{
    let Some(c) = client() else { return };
    let fut = f(c);
    spawn_result(context, async move { Ok(fut.await?) }, move |_| {
        toast(&format!("{context}: done"))
    });
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct ControlPage {
        pub nav: adw::NavigationView,
        pub cards: RefCell<Vec<Box<dyn Card>>>,
        pub advanced_switch: gtk::Switch,
        pub offline: adw::StatusPage,
        pub content: RefCell<Option<gtk::Box>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ControlPage {
        const NAME: &'static str = "PvControlPage";
        type Type = super::ControlPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for ControlPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }
    impl WidgetImpl for ControlPage {}
    impl BinImpl for ControlPage {}
}

glib::wrapper! {
    pub struct ControlPage(ObjectSubclass<imp::ControlPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for ControlPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl ControlPage {
    fn setup(&self) {
        let imp = self.imp();
        let st = state();

        let (scrolled, content) = util::cards_page();

        // Header row: advanced switch + refresh.
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let label = gtk::Label::new(Some("Show advanced"));
        imp.advanced_switch.set_active(st.settings.borrow().ui.show_advanced);
        imp.advanced_switch.set_valign(gtk::Align::Center);
        header.append(&imp.advanced_switch);
        header.append(&label);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        let refresh = gtk::Button::from_icon_name("view-refresh-symbolic");
        refresh.set_tooltip_text(Some("Fetch all states"));
        header.append(&refresh);
        content.append(&header);

        imp.offline.set_icon_name(Some("network-offline-symbolic"));
        imp.offline.set_title("Not connected");
        imp.offline.set_description(Some("Connect to a PiKVM to control it."));
        content.append(&imp.offline);

        let cards: Vec<Box<dyn Card>> = vec![
            Box::new(atx::AtxCard::new()),
            Box::new(hid::HidCard::new()),
            Box::new(msd::MsdCard::new()),
            Box::new(gpio::GpioCard::new()),
            Box::new(switch::SwitchCard::new()),
            Box::new(streamer::StreamerCard::new()),
            Box::new(system::SystemCard::new()),
        ];
        for c in &cards {
            content.append(c.group());
        }
        *imp.cards.borrow_mut() = cards;

        // Tools card with sub-pages.
        let tools = util::card("Tools", None);
        let log_row = adw::ActionRow::builder()
            .title("Log viewer")
            .subtitle("kvmd service logs with follow mode")
            .activatable(true)
            .build();
        log_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let nav = imp.nav.clone();
        log_row.connect_activated(move |_| nav.push(&log::page()));
        tools.add(&log_row);
        let explorer_row = adw::ActionRow::builder()
            .title("API Explorer")
            .subtitle("Every documented endpoint with a run form")
            .activatable(true)
            .build();
        explorer_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let nav = imp.nav.clone();
        explorer_row.connect_activated(move |_| nav.push(&explorer::page()));
        tools.add(&explorer_row);
        content.append(&tools);

        let root = adw::NavigationPage::builder().title("Control").child(&scrolled).build();
        imp.nav.add(&root);
        self.set_child(Some(&imp.nav));
        *imp.content.borrow_mut() = Some(content);

        // Wiring.
        let this = self.clone();
        imp.advanced_switch.connect_active_notify(move |s| {
            let active = s.is_active();
            state().update_settings(|st| st.ui.show_advanced = active);
            this.apply_advanced();
        });
        let this = self.clone();
        st.bus.connect_signal("settings-changed", move || {
            let adv = state().settings.borrow().ui.show_advanced;
            if this.imp().advanced_switch.is_active() != adv {
                this.imp().advanced_switch.set_active(adv);
            }
        });
        let this = self.clone();
        st.connection.store.connect_changed(move |_, _| this.refresh());
        let this = self.clone();
        st.connection.store.connect_connection(move |_| this.refresh());
        let this = self.clone();
        refresh.connect_clicked(move |_| this.fetch_all());

        self.apply_advanced();
        self.refresh();
    }

    fn apply_advanced(&self) {
        let adv = self.imp().advanced_switch.is_active();
        for c in self.imp().cards.borrow().iter() {
            c.set_advanced(adv);
        }
    }

    fn refresh(&self) {
        let st = state();
        let connected = st.connection.store.is_connected();
        self.imp().offline.set_visible(!connected);
        st.connection.store.with_state(|s| {
            for c in self.imp().cards.borrow().iter() {
                c.update(s, connected);
            }
        });
    }

    /// Pull every state through REST (the websocket already streams them; this is a manual
    /// refresh that also validates the REST endpoints).
    fn fetch_all(&self) {
        let Some(client) = client() else { return };
        spawn_result(
            "Refresh",
            async move {
                let info = client.system().info(&[]).await?;
                let (hid_api, atx_api, msd_api, gpio_api, streamer_api) = (
                    client.hid(),
                    client.atx(),
                    client.msd(),
                    client.gpio(),
                    client.streamer(),
                );
                let (hid, atx, msd, gpio, streamer) = tokio::join!(
                    hid_api.state(),
                    atx_api.state(),
                    msd_api.state(),
                    gpio_api.state(),
                    streamer_api.state(),
                );
                Ok((info, hid?, atx?, msd?, gpio?, streamer?))
            },
            |(info, hid, atx, msd, gpio, streamer)| {
                let store = state().connection.store.clone();
                // Feed the REST results through the same path as websocket events.
                let mut events = vec![
                    ("info_hw_state", serde_json::to_value(&info.hw).unwrap_or_default()),
                    (
                        "info_system_state",
                        serde_json::to_value(&info.system).unwrap_or_default(),
                    ),
                    ("info_meta_state", info.meta.clone()),
                    (
                        "info_extras_state",
                        serde_json::to_value(&info.extras).unwrap_or_default(),
                    ),
                    ("info_fan_state", info.fan.clone()),
                    ("info_auth_state", info.auth.clone()),
                    ("hid_state", serde_json::to_value(&hid).unwrap_or_default()),
                    ("atx_state", serde_json::to_value(&atx).unwrap_or_default()),
                    ("msd_state", serde_json::to_value(&msd).unwrap_or_default()),
                    (
                        "gpio_model_state",
                        serde_json::to_value(&gpio.model).unwrap_or_default(),
                    ),
                    ("gpio_state", serde_json::to_value(&gpio.state).unwrap_or_default()),
                    ("streamer_state", serde_json::to_value(&streamer).unwrap_or_default()),
                ];
                for (name, event) in events.drain(..) {
                    store.apply_external(&pikvm::events::KvmEvent {
                        event_type: name.into(),
                        event,
                    });
                }
                toast("States refreshed");
            },
        );
    }
}
