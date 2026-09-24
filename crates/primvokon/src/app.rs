//! `adw::Application` subclass: startup, actions, single main window.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use primvokon_core::paths::APP_ID;
use primvokon_core::storage::Storage;

use crate::state;
use crate::util::RES_PREFIX;
use crate::window::PvWindow;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PvApplication;

    #[glib::object_subclass]
    impl ObjectSubclass for PvApplication {
        const NAME: &'static str = "PvApplication";
        type Type = super::PvApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for PvApplication {}

    impl ApplicationImpl for PvApplication {
        fn startup(&self) {
            self.parent_startup();
            let app = self.obj();
            app.load_css();
            app.setup_state();
            app.setup_actions();
        }

        fn activate(&self) {
            let app = self.obj().clone();
            let window = match app.active_window() {
                Some(w) => w,
                None => PvWindow::new(&app).upcast(),
            };
            window.present();
        }
    }

    impl GtkApplicationImpl for PvApplication {}
    impl AdwApplicationImpl for PvApplication {}
}

glib::wrapper! {
    pub struct PvApplication(ObjectSubclass<imp::PvApplication>)
        @extends adw::Application, gtk::Application, gio::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl Default for PvApplication {
    fn default() -> Self {
        Self::new()
    }
}

impl PvApplication {
    pub fn new() -> Self {
        glib::Object::builder()
            .property("application-id", APP_ID)
            .property("flags", gio::ApplicationFlags::default())
            .property("resource-base-path", RES_PREFIX)
            .build()
    }

    fn load_css(&self) {
        let provider = gtk::CssProvider::new();
        provider.load_from_resource(&format!("{RES_PREFIX}/style.css"));
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    fn setup_state(&self) {
        let storage = Storage::open_default().unwrap_or_else(|e| {
            tracing::error!("database unavailable ({e}); using in-memory storage");
            Storage::open_in_memory().expect("in-memory storage")
        });
        let st = state::init(storage);
        // Open the secret store off the main thread; pages wait for `secrets-ready`.
        crate::util::spawn_then(primvokon_core::secrets::open(), move |store| {
            *st.secrets.borrow_mut() = Some(store);
            st.bus.emit("secrets-ready");
        });
    }

    fn setup_actions(&self) {
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &Self, _, _| app.quit())
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &Self, _, _| app.show_about())
            .build();
        let prefs = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                if let Some(window) = app.active_window() {
                    crate::prefs::PreferencesDialog::new().present(Some(&window));
                }
            })
            .build();
        self.add_action_entries([quit, about, prefs]);
        self.set_accels_for_action("app.quit", &["<primary>q"]);
        self.set_accels_for_action("app.preferences", &["<primary>comma"]);
        self.set_accels_for_action("win.fullscreen", &["F11"]);
        self.set_accels_for_action("win.paste-text", &["<primary><shift>v"]);
        self.set_accels_for_action("win.show-help-overlay", &["<primary>question"]);
    }

    fn show_about(&self) {
        let about = adw::AboutDialog::builder()
            .application_name("PRIMVOKON")
            .application_icon(APP_ID)
            .developer_name("Jonas Bickel")
            .version(env!("CARGO_PKG_VERSION"))
            .website("https://github.com/jonas-bickel/primvokon")
            .issue_url("https://github.com/jonas-bickel/primvokon/issues")
            .license_type(gtk::License::MitX11)
            .comments("Agentic remote for PiKVM: live console, full API control centre, screen watcher with ntfy, screen recall and an AI agent.")
            .build();
        if let Some(window) = self.active_window() {
            about.present(Some(&window));
        }
    }
}
