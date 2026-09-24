//! Process-wide application state shared by all pages.

use std::cell::RefCell;
use std::rc::Rc;

use gtk::glib;
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;
use primvokon_core::agent::AgentRun;
use primvokon_core::ai::{build_provider, SharedProvider};
use primvokon_core::ntfy::NtfyNotifier;
use primvokon_core::recall::{CaptureHandle, Scheduler};
use primvokon_core::secrets::SharedSecrets;
use primvokon_core::settings::{NtfyAuth, ProviderKind, Settings};
use primvokon_core::storage::Storage;
use primvokon_core::watcher::WatcherHandle;

use crate::connection::Connection;

mod imp {
    use super::*;
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Bus;

    #[glib::object_subclass]
    impl ObjectSubclass for Bus {
        const NAME: &'static str = "PvBus";
        type Type = super::Bus;
    }

    impl ObjectImpl for Bus {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    glib::subclass::Signal::builder("settings-changed").build(),
                    glib::subclass::Signal::builder("services-changed").build(),
                    glib::subclass::Signal::builder("secrets-ready").build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    /// Event bus for app-level changes (settings saved, services started/stopped).
    pub struct Bus(ObjectSubclass<imp::Bus>);
}

impl Default for Bus {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Bus {
    pub fn connect_signal(&self, name: &str, f: impl Fn() + 'static) -> glib::SignalHandlerId {
        self.connect_closure(name, false, glib::closure_local!(move |_bus: &Self| f()))
    }

    pub fn emit(&self, name: &str) {
        self.emit_by_name::<()>(name, &[]);
    }
}

#[derive(Default)]
pub struct Services {
    pub watcher: Option<WatcherHandle>,
    pub recall: Option<CaptureHandle>,
    pub scheduler: Option<Scheduler>,
    pub agent: Option<AgentRun>,
}

pub struct AppState {
    pub settings: RefCell<Settings>,
    pub secrets: RefCell<Option<SharedSecrets>>,
    pub storage: Storage,
    pub connection: Rc<Connection>,
    pub bus: Bus,
    pub services: RefCell<Services>,
}

thread_local! {
    static STATE: RefCell<Option<Rc<AppState>>> = const { RefCell::new(None) };
}

/// The global state; panics before [`init`].
pub fn state() -> Rc<AppState> {
    STATE.with(|s| s.borrow().clone().expect("app state initialised"))
}

pub fn init(storage: Storage) -> Rc<AppState> {
    let st = Rc::new(AppState {
        settings: RefCell::new(Settings::load()),
        secrets: RefCell::new(None),
        storage,
        connection: Rc::new(Connection::default()),
        bus: Bus::default(),
        services: RefCell::new(Services::default()),
    });
    STATE.with(|s| *s.borrow_mut() = Some(st.clone()));
    st
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.borrow().clone()
    }

    /// Mutate settings, persist them and notify listeners.
    pub fn update_settings(&self, f: impl FnOnce(&mut Settings)) {
        {
            let mut s = self.settings.borrow_mut();
            f(&mut s);
            if let Err(e) = s.save() {
                crate::util::toast_error("Saving settings", &e);
            }
        }
        self.bus.emit("settings-changed");
    }

    pub fn secrets(&self) -> Option<SharedSecrets> {
        self.secrets.borrow().clone()
    }

    /// Build the configured provider of a kind (API key from the secret store).
    pub async fn provider(
        kind: ProviderKind,
        settings: &Settings,
        secrets: &SharedSecrets,
    ) -> anyhow::Result<SharedProvider> {
        let key = secrets.get(&kind.secret_key()).await?.unwrap_or_default();
        build_provider(kind, &settings.ai.provider(kind), key)
    }

    /// The vision provider, or `None` when the privacy switch or capability forbids it.
    pub async fn vision_provider(settings: &Settings, secrets: &SharedSecrets) -> Option<SharedProvider> {
        if settings.capabilities.never_send_screen_to_ai || !settings.capabilities.ai_vision {
            return None;
        }
        Self::provider(settings.ai.vision_provider, settings, secrets)
            .await
            .ok()
    }

    pub async fn ntfy_notifier(settings: &Settings, secrets: &SharedSecrets) -> anyhow::Result<NtfyNotifier> {
        let token = match settings.ntfy.auth {
            NtfyAuth::Token => {
                secrets
                    .get(primvokon_core::settings::NtfySettings::SECRET_KEY_TOKEN)
                    .await?
            }
            _ => None,
        };
        let password = match settings.ntfy.auth {
            NtfyAuth::Basic => {
                secrets
                    .get(primvokon_core::settings::NtfySettings::SECRET_KEY_PASSWORD)
                    .await?
            }
            _ => None,
        };
        NtfyNotifier::new(settings.ntfy.clone(), token, password)
    }

    pub fn services_changed(&self) {
        self.bus.emit("services-changed");
    }
}
