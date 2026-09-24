//! Connection manager: builds the PiKVM client from a profile, runs the websocket session and
//! exposes the aggregated KVM state to widgets through a GObject with a `changed` signal.

use std::cell::RefCell;
use std::sync::Arc;
use std::time::Duration;

use gtk::glib;
use gtk::glib::subclass::prelude::*;
use gtk::prelude::*;
use pikvm::auth::{AuthMethod, Credentials};
use pikvm::client::ConnectionConfig;
use pikvm::events::{KvmEvent, KvmState, StateSection};
use pikvm::ws::{ReconnectPolicy, WsClient, WsMessage, WsStatus};
use pikvm::PikvmClient;
use primvokon_core::runtime;
use primvokon_core::secrets::SharedSecrets;
use primvokon_core::settings::{ConnectionProfile, TotpMode};

mod imp {
    use super::*;
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct KvmStore {
        pub state: RefCell<KvmState>,
        pub connected: RefCell<bool>,
        pub status: RefCell<String>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for KvmStore {
        const NAME: &'static str = "PvKvmStore";
        type Type = super::KvmStore;
    }

    impl ObjectImpl for KvmStore {
        fn signals() -> &'static [glib::subclass::Signal] {
            static SIGNALS: OnceLock<Vec<glib::subclass::Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    // Emitted after a websocket event updated the state; arg = StateSection as u32.
                    glib::subclass::Signal::builder("changed").param_types([u32::static_type()]).build(),
                    // Emitted when connection status changed.
                    glib::subclass::Signal::builder("connection").build(),
                ]
            })
        }
    }
}

glib::wrapper! {
    pub struct KvmStore(ObjectSubclass<imp::KvmStore>);
}

impl Default for KvmStore {
    fn default() -> Self {
        glib::Object::new()
    }
}

/// Section ids used in the `changed` signal.
pub fn section_id(s: StateSection) -> u32 {
    match s {
        StateSection::Info => 1,
        StateSection::Hid => 2,
        StateSection::Keymaps => 3,
        StateSection::Atx => 4,
        StateSection::Msd => 5,
        StateSection::Gpio => 6,
        StateSection::Streamer => 7,
        StateSection::Switch => 8,
        StateSection::Wol => 9,
        StateSection::Loop => 10,
        StateSection::Pong | StateSection::Unknown => 0,
    }
}

impl KvmStore {
    pub fn with_state<T>(&self, f: impl FnOnce(&KvmState) -> T) -> T {
        f(&self.imp().state.borrow())
    }

    pub fn snapshot(&self) -> KvmState {
        self.imp().state.borrow().clone()
    }

    pub fn is_connected(&self) -> bool {
        *self.imp().connected.borrow()
    }

    pub fn status(&self) -> String {
        self.imp().status.borrow().clone()
    }

    fn apply(&self, ev: &KvmEvent) {
        let section = self.imp().state.borrow_mut().apply(ev);
        self.emit_by_name::<()>("changed", &[&section_id(section)]);
    }

    fn set_connection(&self, connected: bool, status: &str) {
        *self.imp().connected.borrow_mut() = connected;
        *self.imp().status.borrow_mut() = status.to_string();
        if !connected {
            *self.imp().state.borrow_mut() = KvmState::default();
            self.emit_by_name::<()>("changed", &[&0u32]);
        }
        self.emit_by_name::<()>("connection", &[]);
    }

    pub fn connect_changed(&self, f: impl Fn(&Self, u32) + 'static) -> glib::SignalHandlerId {
        self.connect_closure(
            "changed",
            false,
            glib::closure_local!(move |store: &Self, section: u32| f(store, section)),
        )
    }

    pub fn connect_connection(&self, f: impl Fn(&Self) + 'static) -> glib::SignalHandlerId {
        self.connect_closure("connection", false, glib::closure_local!(move |store: &Self| f(store)))
    }
}

/// Owns the client and websocket session for the active profile.
pub struct Connection {
    pub store: KvmStore,
    client: RefCell<Option<PikvmClient>>,
    ws: RefCell<Option<WsClient>>,
    profile: RefCell<Option<ConnectionProfile>>,
    generation: RefCell<u64>,
}

impl Default for Connection {
    fn default() -> Self {
        Self {
            store: KvmStore::default(),
            client: RefCell::new(None),
            ws: RefCell::new(None),
            profile: RefCell::new(None),
            generation: RefCell::new(0),
        }
    }
}

/// Result of a connection test.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    pub ok: bool,
    pub summary: String,
}

/// Build client credentials for a profile from the secret store.
pub async fn credentials_for(profile: &ConnectionProfile, secrets: &SharedSecrets, totp_code: Option<String>) -> anyhow::Result<Credentials> {
    let password = secrets.get(&profile.secret_key_password()).await?.unwrap_or_default();
    let totp_secret = match profile.totp {
        TotpMode::Secret => secrets.get(&profile.secret_key_totp()).await?,
        _ => None,
    };
    let token = match profile.auth_method {
        AuthMethod::Token => secrets.get(&profile.secret_key_token()).await?,
        _ => None,
    };
    Ok(Credentials {
        user: profile.user.clone(),
        password,
        totp_secret,
        totp_code,
        token,
    })
}

pub fn build_client(profile: &ConnectionProfile, creds: Credentials) -> anyhow::Result<PikvmClient> {
    let url = ConnectionConfig::parse_base_url(&profile.base_url)?;
    let mut cfg = ConnectionConfig::new(url, profile.auth_method, creds);
    cfg.accept_invalid_certs = profile.accept_invalid_certs;
    Ok(PikvmClient::new(cfg)?)
}

/// `GET /api/auth/check` + `GET /api/info` with timing.
pub async fn diagnose(client: &PikvmClient) -> Diagnostics {
    let start = std::time::Instant::now();
    match client.auth().check().await {
        Ok(()) => {}
        Err(e) if e.is_auth() => {
            return Diagnostics {
                ok: false,
                summary: format!("Reachable but credentials rejected ({e})"),
            }
        }
        Err(e) => {
            return Diagnostics {
                ok: false,
                summary: format!("Unreachable: {e}"),
            }
        }
    }
    let ms = start.elapsed().as_millis();
    match client.system().info(&["system", "hw"]).await {
        Ok(info) => Diagnostics {
            ok: true,
            summary: format!(
                "Connected · kvmd {} · {} · {} ms",
                info.system.kvmd.version,
                if info.hw.platform.base.is_empty() { "unknown board".to_string() } else { info.hw.platform.base.clone() },
                ms
            ),
        },
        Err(e) => Diagnostics {
            ok: false,
            summary: format!("Authenticated but /api/info failed: {e}"),
        },
    }
}

impl Connection {
    pub fn client(&self) -> Option<PikvmClient> {
        self.client.borrow().clone()
    }

    pub fn ws(&self) -> Option<WsClient> {
        self.ws.borrow().clone()
    }

    pub fn profile(&self) -> Option<ConnectionProfile> {
        self.profile.borrow().clone()
    }

    pub fn host_name(&self) -> String {
        self.profile
            .borrow()
            .as_ref()
            .map(|p| p.name.clone())
            .unwrap_or_else(|| "PiKVM".into())
    }

    /// Disconnect and drop the client.
    pub fn disconnect(&self) {
        *self.generation.borrow_mut() += 1;
        if let Some(ws) = self.ws.borrow_mut().take() {
            ws.close();
        }
        *self.client.borrow_mut() = None;
        self.store.set_connection(false, "Offline");
    }

    /// Connect with a prepared client: start the websocket and feed the store.
    pub fn attach(self: &Arc<Self>, profile: ConnectionProfile, client: PikvmClient) {
        self.disconnect();
        let generation = {
            let mut g = self.generation.borrow_mut();
            *g += 1;
            *g
        };
        *self.profile.borrow_mut() = Some(profile);
        *self.client.borrow_mut() = Some(client.clone());
        self.store.set_connection(false, "Connecting…");

        let (ws, mut rx) = {
            let _guard = runtime::enter();
            WsClient::connect(client, true, ReconnectPolicy::default())
        };
        *self.ws.borrow_mut() = Some(ws);

        let this = Arc::clone(self);
        glib::spawn_future_local(async move {
            while let Some(msg) = rx.recv().await {
                if *this.generation.borrow() != generation {
                    break;
                }
                match msg {
                    WsMessage::Status(WsStatus::Connecting) => this.store.set_connection(false, "Connecting…"),
                    WsMessage::Status(WsStatus::Connected) => this.store.set_connection(true, "Connected"),
                    WsMessage::Status(WsStatus::Disconnected { reason, retry_in }) => {
                        this.store
                            .set_connection(false, &format!("Reconnecting in {}s ({reason})", retry_in.as_secs()))
                    }
                    WsMessage::Status(WsStatus::Closed) => {
                        this.store.set_connection(false, "Offline");
                        break;
                    }
                    WsMessage::Event(ev) => this.store.apply(&ev),
                }
            }
        });
    }
}

/// Timeout used for connection tests.
pub const TEST_TIMEOUT: Duration = Duration::from_secs(10);
