//! Main window: view switcher with the five pages, profile menu, connection status.

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::{gio, glib};
use primvokon_core::settings::TotpMode;

use crate::connection::{build_client, credentials_for};
use crate::state::state;
use crate::ui;
use crate::util::{self, set_pill, spawn_then, toast, toast_error};

mod imp {
    use super::*;

    #[derive(Default, gtk::CompositeTemplate)]
    #[template(resource = "/io/github/jonas_bickel/Primvokon/ui/window.ui")]
    pub struct PvWindow {
        #[template_child]
        pub toast_overlay: TemplateChild<adw::ToastOverlay>,
        #[template_child]
        pub view_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub connection_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub services_pill: TemplateChild<gtk::Label>,
        #[template_child]
        pub profile_button: TemplateChild<gtk::MenuButton>,
        #[template_child]
        pub profiles_section: TemplateChild<gio::Menu>,

        pub console: ui::console::ConsolePage,
        pub control: ui::control::ControlPage,
        pub watch: ui::watch::WatchPage,
        pub recall: ui::recall::RecallPage,
        pub agent: ui::agent::AgentPage,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PvWindow {
        const NAME: &'static str = "PvWindow";
        type Type = super::PvWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for PvWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }

    impl WidgetImpl for PvWindow {}
    impl WindowImpl for PvWindow {
        fn close_request(&self) -> glib::Propagation {
            let (w, h) = (self.obj().width(), self.obj().height());
            state().update_settings(|s| {
                s.ui.window_width = w;
                s.ui.window_height = h;
            });
            self.parent_close_request()
        }
    }
    impl ApplicationWindowImpl for PvWindow {}
    impl AdwApplicationWindowImpl for PvWindow {}
}

glib::wrapper! {
    pub struct PvWindow(ObjectSubclass<imp::PvWindow>)
        @extends adw::ApplicationWindow, gtk::ApplicationWindow, gtk::Window, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::Accessible, gtk::Buildable,
                    gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl PvWindow {
    pub fn new(app: &impl IsA<gtk::Application>) -> Self {
        glib::Object::builder().property("application", app).build()
    }

    /// The main window, if any.
    pub fn current() -> Option<Self> {
        gtk::Window::list_toplevels()
            .into_iter()
            .find_map(|w| w.downcast::<Self>().ok())
    }

    fn setup(&self) {
        let imp = self.imp();
        util::set_toast_overlay(&imp.toast_overlay);
        let st = state();
        {
            let ui = &st.settings.borrow().ui;
            self.set_default_size(ui.window_width.max(360), ui.window_height.max(400));
        }

        let stack = imp.view_stack.get();
        stack.add_titled_with_icon(&imp.console, Some("console"), "Console", "video-display-symbolic");
        stack.add_titled_with_icon(&imp.control, Some("control"), "Control", "preferences-system-symbolic");
        stack.add_titled_with_icon(&imp.watch, Some("watch"), "Watch", "view-reveal-symbolic");
        stack.add_titled_with_icon(&imp.recall, Some("recall"), "Recall", "document-open-recent-symbolic");
        stack.add_titled_with_icon(&imp.agent, Some("agent"), "Agent", "user-available-symbolic");

        self.setup_actions();
        self.rebuild_profile_menu();

        let this = self.clone();
        st.connection.store.connect_connection(move |store| {
            let (text, class) = if store.is_connected() {
                ("Connected", "success")
            } else if store.status().starts_with("Offline") {
                ("Offline", "error")
            } else {
                ("Connecting", "warning")
            };
            set_pill(&this.imp().connection_pill, text, Some(class));
            this.imp().connection_pill.set_tooltip_text(Some(&store.status()));
        });
        let this = self.clone();
        st.bus.connect_signal("settings-changed", move || {
            this.rebuild_profile_menu();
            this.update_capability_pages();
        });
        let this = self.clone();
        st.bus
            .connect_signal("services-changed", move || this.update_services_pill());
        let this = self.clone();
        st.bus.connect_signal("secrets-ready", move || {
            if state().secrets().map(|s| s.is_fallback()).unwrap_or(false) {
                toast("No Secret Service found: secrets are stored in a plain file (0600)");
            }
            this.connect_active_profile(None);
            this.autostart_services();
        });
        self.update_capability_pages();
        self.update_services_pill();

        // Debug aid: PRIMVOKON_START_PAGE=control|watch|recall|agent|prefs opens that view.
        if let Ok(page) = std::env::var("PRIMVOKON_START_PAGE") {
            if page == "prefs" {
                let this = self.clone();
                glib::idle_add_local_once(move || crate::prefs::PreferencesDialog::new().present(Some(&this)));
            } else {
                stack.set_visible_child_name(&page);
            }
        }
    }

    fn setup_actions(&self) {
        let connect = gio::ActionEntry::builder("connect")
            .activate(|win: &Self, _, _| win.connect_active_profile(None))
            .build();
        let disconnect = gio::ActionEntry::builder("disconnect")
            .activate(|_: &Self, _, _| state().connection.disconnect())
            .build();
        let logout = gio::ActionEntry::builder("logout")
            .activate(|_: &Self, _, _| {
                if let Some(client) = state().connection.client() {
                    util::spawn_result("Sign out", async move { Ok(client.auth().logout().await?) }, |_| {
                        state().connection.disconnect();
                        toast("Session token invalidated");
                    });
                }
            })
            .build();
        let select = gio::ActionEntry::builder("select-profile")
            .parameter_type(Some(&String::static_variant_type()))
            .activate(|win: &Self, _, param| {
                if let Some(id) = param.and_then(|p| p.get::<String>()) {
                    state().update_settings(|s| s.active_profile = Some(id));
                    win.connect_active_profile(None);
                }
            })
            .build();
        let fullscreen = gio::ActionEntry::builder("fullscreen")
            .activate(|win: &Self, _, _| {
                if win.is_fullscreen() {
                    win.unfullscreen();
                } else {
                    win.fullscreen();
                }
            })
            .build();
        let paste = gio::ActionEntry::builder("paste-text")
            .activate(|win: &Self, _, _| win.imp().console.show_paste_dialog())
            .build();
        self.add_action_entries([connect, disconnect, logout, select, fullscreen, paste]);
    }

    fn rebuild_profile_menu(&self) {
        let imp = self.imp();
        let menu = imp.profiles_section.get();
        menu.remove_all();
        let settings = state().settings();
        for p in &settings.profiles {
            let active = settings.active_profile.as_deref() == Some(p.id.as_str());
            let label = if active {
                format!("● {}", p.name)
            } else {
                format!("○ {}", p.name)
            };
            let item = gio::MenuItem::new(Some(&label), None);
            item.set_action_and_target_value(Some("win.select-profile"), Some(&p.id.to_variant()));
            menu.append_item(&item);
        }
        let title = settings
            .active_profile()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "No profile".into());
        imp.profile_button
            .set_tooltip_text(Some(&format!("PiKVM profile: {title}")));
    }

    /// Hide pages whose capability is switched off (F-CAP-2).
    fn update_capability_pages(&self) {
        let imp = self.imp();
        let caps = state().settings.borrow().capabilities.clone();
        let stack = imp.view_stack.get();
        for (child, enabled) in [
            (imp.watch.upcast_ref::<gtk::Widget>(), caps.watcher),
            (imp.recall.upcast_ref::<gtk::Widget>(), caps.recall),
            (imp.agent.upcast_ref::<gtk::Widget>(), caps.agent),
        ] {
            if let Ok(page) = stack.page(child).downcast::<adw::ViewStackPage>() {
                page.set_visible(enabled);
            }
        }
    }

    fn update_services_pill(&self) {
        let st = state();
        let services = st.services.borrow();
        let mut parts = Vec::new();
        if services.watcher.as_ref().is_some_and(|w| w.is_running()) {
            parts.push("Watching");
        }
        if services.recall.as_ref().is_some_and(|r| r.is_running()) {
            parts.push("Recording");
        }
        if services.agent.as_ref().is_some_and(|a| a.is_running()) {
            parts.push("Agent running");
        }
        let pill = &self.imp().services_pill;
        pill.set_visible(!parts.is_empty());
        pill.set_text(&parts.join(" · "));
    }

    /// Connect to the active profile; asks for a TOTP code when the profile requires it.
    pub fn connect_active_profile(&self, totp_code: Option<String>) {
        let st = state();
        let Some(profile) = st.settings.borrow().active_profile().cloned() else {
            return;
        };
        let Some(secrets) = st.secrets() else {
            return;
        };
        if profile.totp == TotpMode::Ask && totp_code.is_none() {
            self.ask_totp_code();
            return;
        }
        let connection = st.connection.clone();
        let profile_clone = profile.clone();
        spawn_then(
            async move {
                let creds = credentials_for(&profile_clone, &secrets, totp_code).await?;
                let client = build_client(&profile_clone, creds)?;
                client.ensure_authenticated().await?;
                Ok::<_, anyhow::Error>(client)
            },
            move |result| match result {
                Ok(client) => connection.attach(profile, client),
                Err(e) => {
                    connection.disconnect();
                    toast_error("Connecting", &e);
                }
            },
        );
    }

    fn ask_totp_code(&self) {
        let dialog = adw::AlertDialog::builder()
            .heading("Two-factor code")
            .body("Enter the current one-time code for the PiKVM login.")
            .build();
        let entry = gtk::Entry::builder()
            .input_purpose(gtk::InputPurpose::Digits)
            .max_length(6)
            .build();
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("ok", "Connect");
        dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("ok"));
        let this = self.clone();
        dialog.connect_response(None, move |_, resp| {
            if resp == "ok" {
                this.connect_active_profile(Some(entry.text().to_string()));
            }
        });
        dialog.present(Some(self));
    }

    /// Start background services whose capability is enabled (watcher, recall, summariser).
    fn autostart_services(&self) {
        let imp = self.imp();
        imp.watch.autostart();
        imp.recall.autostart();
    }
}
