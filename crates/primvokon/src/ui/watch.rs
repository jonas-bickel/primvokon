//! Watch page: screen watcher status, controls and the event log (USP 1).

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use primvokon_core::ntfy::Notifier;
use primvokon_core::screen::PikvmScreen;
use primvokon_core::storage::WatchEvent;
use primvokon_core::watcher::{build_detector, WatcherEvent, WatcherService};

use crate::state::{state, AppState};
use crate::util::{self, format_time, format_ts, spawn_then, toast, toast_error};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct WatchPage {
        pub status_row: adw::ActionRow,
        pub last_row: adw::ActionRow,
        pub toggle: gtk::Button,
        pub list: gtk::ListBox,
        pub empty: adw::StatusPage,
        pub want_running: Cell<bool>,
        pub rows: RefCell<Vec<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WatchPage {
        const NAME: &'static str = "PvWatchPage";
        type Type = super::WatchPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for WatchPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }
    impl WidgetImpl for WatchPage {}
    impl BinImpl for WatchPage {}
}

glib::wrapper! {
    pub struct WatchPage(ObjectSubclass<imp::WatchPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for WatchPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl WatchPage {
    fn setup(&self) {
        let imp = self.imp();
        let (scrolled, content) = util::cards_page();

        let card = util::card(
            "Screen watcher",
            Some("Samples the host screen and sends an ntfy push when something new appears."),
        );
        imp.status_row.set_title("Status");
        card.add(&imp.status_row);
        imp.last_row.set_title("Last check");
        imp.last_row.set_subtitle("—");
        card.add(&imp.last_row);
        imp.toggle.set_label("Start watching");
        imp.toggle.add_css_class("suggested-action");
        let settings_btn = util::button("Settings…");
        settings_btn.set_action_name(Some("app.preferences"));
        let test = util::button("Test notification");
        let row = adw::ActionRow::builder().title("Actions").build();
        row.add_suffix(&util::button_box(&[&imp.toggle, &test, &settings_btn]));
        card.add(&row);
        content.append(&card);

        let events = util::card("Detected changes", None);
        let clear = util::button("Clear log");
        let header = adw::ActionRow::builder()
            .title("Event log")
            .subtitle("Newest first")
            .build();
        header.add_suffix(&clear);
        clear.set_valign(gtk::Align::Center);
        events.add(&header);
        imp.list.add_css_class("boxed-list");
        imp.list.set_selection_mode(gtk::SelectionMode::None);
        events.add(&imp.list);
        imp.empty.set_icon_name(Some("view-reveal-symbolic"));
        imp.empty.set_title("Nothing detected yet");
        imp.empty.set_description(Some(
            "Start the watcher; detected changes appear here with a thumbnail.",
        ));
        events.add(&imp.empty);
        content.append(&events);
        self.set_child(Some(&scrolled));

        let this = self.clone();
        imp.toggle.connect_clicked(move |_| {
            if this.is_running() {
                this.imp().want_running.set(false);
                this.stop();
            } else {
                this.imp().want_running.set(true);
                this.start();
            }
        });
        test.connect_clicked(|_| send_test_notification());
        let this = self.clone();
        clear.connect_clicked(move |_| {
            if let Err(e) = state().storage.clear_watch_events() {
                toast_error("Clear", &e);
            }
            this.load_events();
        });

        let st = state();
        let this = self.clone();
        st.connection.store.connect_connection(move |store| {
            if store.is_connected() {
                if this.imp().want_running.get() && !this.is_running() {
                    this.start();
                }
            } else if this.is_running() {
                this.stop();
            }
            this.refresh_status();
        });
        let this = self.clone();
        st.bus.connect_signal("settings-changed", move || {
            if !state().settings.borrow().capabilities.watcher && this.is_running() {
                this.imp().want_running.set(false);
                this.stop();
            }
            this.refresh_status();
        });
        self.load_events();
        self.refresh_status();
    }

    fn is_running(&self) -> bool {
        state()
            .services
            .borrow()
            .watcher
            .as_ref()
            .is_some_and(|w| w.is_running())
    }

    /// Called once at startup: run automatically when the capability is enabled.
    pub fn autostart(&self) {
        let enabled = state().settings.borrow().capabilities.watcher;
        self.imp().want_running.set(enabled);
        if enabled && state().connection.store.is_connected() {
            self.start();
        }
    }

    fn refresh_status(&self) {
        let imp = self.imp();
        let s = state().settings();
        let running = self.is_running();
        imp.toggle
            .set_label(if running { "Stop watching" } else { "Start watching" });
        if running {
            imp.toggle.remove_css_class("suggested-action");
            imp.toggle.add_css_class("destructive-action");
        } else {
            imp.toggle.remove_css_class("destructive-action");
            imp.toggle.add_css_class("suggested-action");
        }
        let text = format!(
            "{} · {} · every {} s · cooldown {} s · ntfy {}",
            if running { "watching" } else { "stopped" },
            s.watcher.detector.label(),
            s.watcher.interval_secs,
            s.watcher.cooldown_secs,
            if s.capabilities.ntfy {
                s.ntfy.topic.as_str()
            } else {
                "off"
            }
        );
        imp.status_row.set_subtitle(&text);
    }

    fn start(&self) {
        if self.is_running() {
            return;
        }
        let st = state();
        let Some(client) = st.connection.client() else {
            toast("Connect to a PiKVM first");
            return;
        };
        let Some(secrets) = st.secrets() else {
            toast("Secret store not ready yet");
            return;
        };
        let settings = st.settings();
        if !settings.capabilities.watcher {
            toast("Enable the watcher capability in Preferences → Capabilities");
            return;
        }
        let storage = st.storage.clone();
        let host = st.connection.host_name();
        let this = self.clone();
        spawn_then(
            async move {
                let vision = if settings.watcher.detector.needs_ai() {
                    let v = AppState::vision_provider(&settings, &secrets).await;
                    if v.is_none() {
                        anyhow::bail!("this detector needs a vision provider (check AI settings, the AI vision capability and the privacy switch)");
                    }
                    v
                } else {
                    None
                };
                let detector = build_detector(&settings.watcher, vision)?;
                let notifier: Option<Arc<dyn Notifier>> = if settings.capabilities.ntfy {
                    Some(Arc::new(AppState::ntfy_notifier(&settings, &secrets).await?))
                } else {
                    None
                };
                let service = WatcherService {
                    screen: Arc::new(PikvmScreen::new(client)),
                    detector,
                    notifier,
                    storage,
                    settings: settings.watcher.clone(),
                    host,
                    title_template: settings.ntfy.title_template.clone(),
                    attach_snapshot: settings.ntfy.attach_snapshot,
                };
                Ok(service.start())
            },
            move |r| match r {
                Ok((handle, mut rx)) => {
                    state().services.borrow_mut().watcher = Some(handle);
                    state().services_changed();
                    this.refresh_status();
                    let page = this.clone();
                    glib::spawn_future_local(async move {
                        while let Some(ev) = rx.recv().await {
                            page.on_event(ev);
                        }
                        page.refresh_status();
                    });
                }
                Err(e) => {
                    this.imp().want_running.set(false);
                    toast_error("Watcher", &e);
                }
            },
        );
    }

    fn stop(&self) {
        if let Some(h) = state().services.borrow_mut().watcher.take() {
            h.stop();
        }
        state().services_changed();
        self.refresh_status();
    }

    fn on_event(&self, ev: WatcherEvent) {
        match ev {
            WatcherEvent::Started => toast("Watcher started"),
            WatcherEvent::Sampled { ts } => self.imp().last_row.set_subtitle(&format_time(ts)),
            WatcherEvent::Change(event) => {
                self.imp().empty.set_visible(false);
                let row = event_row(&event);
                self.imp().list.prepend(&row);
                self.imp().rows.borrow_mut().insert(0, row.upcast());
            }
            WatcherEvent::Error(e) => self.imp().last_row.set_subtitle(&format!("error: {e}")),
            WatcherEvent::Stopped => {
                state().services_changed();
                self.refresh_status();
            }
        }
    }

    fn load_events(&self) {
        let imp = self.imp();
        for w in imp.rows.borrow_mut().drain(..) {
            imp.list.remove(&w);
        }
        let events = state().storage.watch_events(200).unwrap_or_default();
        imp.empty.set_visible(events.is_empty());
        for ev in &events {
            let row = event_row(ev);
            imp.list.append(&row);
            imp.rows.borrow_mut().push(row.upcast());
        }
    }
}

fn event_row(ev: &WatchEvent) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(glib::markup_escape_text(&ev.reason))
        .subtitle(glib::markup_escape_text(&format!(
            "{} · {}",
            format_ts(ev.ts),
            ev.detail.replace('\n', " · ")
        )))
        .subtitle_lines(3)
        .build();
    if let Some(thumb) = &ev.thumbnail {
        if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib::Bytes::from(thumb.as_slice())) {
            let pic = gtk::Picture::for_paintable(&texture);
            pic.set_size_request(96, 54);
            pic.set_content_fit(gtk::ContentFit::Contain);
            row.add_prefix(&pic);
        }
    }
    let pill = match (&ev.error, ev.notified) {
        (Some(e), _) => {
            let p = util::make_pill("failed", Some("error"));
            p.set_tooltip_text(Some(e));
            p
        }
        (None, true) => util::make_pill("sent", Some("success")),
        (None, false) => util::make_pill("logged", None),
    };
    row.add_suffix(&pill);
    row
}

fn send_test_notification() {
    let Some(secrets) = state().secrets() else {
        toast("Secret store not ready yet");
        return;
    };
    let settings = state().settings();
    spawn_then(
        async move {
            let n = AppState::ntfy_notifier(&settings, &secrets).await?;
            n.notify(&primvokon_core::ntfy::Notification {
                title: "PRIMVOKON test".into(),
                message: "Watcher test notification".into(),
                priority: None,
                tags: vec!["white_check_mark".into()],
                attachment: None,
            })
            .await
        },
        |r| match r {
            Ok(()) => toast("Test notification sent"),
            Err(e) => toast_error("ntfy", &e),
        },
    );
}
