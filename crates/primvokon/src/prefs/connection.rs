//! Connection profiles: URL, auth method, credentials, 2FA, TLS, test button.

use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use adw::prelude::*;
use pikvm::auth::AuthMethod;
use pikvm::client::ConnectionConfig;
use primvokon_core::secrets::set_or_delete;
use primvokon_core::settings::{ConnectionProfile, TotpMode};

use crate::connection::{build_client, credentials_for, diagnose, TEST_TIMEOUT};
use crate::state::state;
use crate::util::{self, spawn_then, toast, toast_error};

pub fn page() -> adw::PreferencesPage {
    let page = super::page("Connection", "network-server-symbolic");
    let editor = ProfileEditor::build();
    if state().secrets().is_some_and(|s| s.is_fallback()) {
        let banner = adw::Banner::builder()
            .title("No Secret Service found – secrets are kept in a plain file (mode 0600)")
            .revealed(true)
            .build();
        let g = adw::PreferencesGroup::new();
        g.add(&banner);
        page.add(&g);
    }
    page.add(&editor.selector_group);
    page.add(&editor.group);
    page.add(&editor.secrets_group);
    page.add(&editor.advanced_group);
    page.add(&editor.actions_group);
    editor.load_active();
    // The editor is referenced weakly from its closures; keep it alive with the page.
    unsafe {
        page.set_data("editor", editor);
    }
    page
}

struct ProfileEditor {
    selector_group: adw::PreferencesGroup,
    group: adw::PreferencesGroup,
    secrets_group: adw::PreferencesGroup,
    advanced_group: adw::PreferencesGroup,
    actions_group: adw::PreferencesGroup,
    profile_combo: adw::ComboRow,
    name: adw::EntryRow,
    url: adw::EntryRow,
    auth: adw::ComboRow,
    user: adw::EntryRow,
    password: adw::PasswordEntryRow,
    totp_mode: adw::ComboRow,
    totp_secret: adw::PasswordEntryRow,
    token: adw::PasswordEntryRow,
    accept_certs: adw::SwitchRow,
    test_result: adw::ActionRow,
    editing: RefCell<Option<ConnectionProfile>>,
    loading: Cell<bool>,
}

macro_rules! weak_call {
    ($weak:ident, |$e:ident| $body:expr) => {
        move |_| {
            if let Some($e) = $weak.upgrade() {
                $body
            }
        }
    };
}

impl ProfileEditor {
    fn build() -> Rc<Self> {
        let selector_group = util::card("Profiles", Some("Each profile is one PiKVM. Tailscale hosts work like any other host name."));
        let profile_combo = util::combo_row(&selector_group, "Active profile", None, &[], 0);
        let add = util::button("New profile");
        let remove = util::destructive_button("Delete");
        let row = adw::ActionRow::builder().title("Manage").build();
        row.add_suffix(&util::button_box(&[&add, &remove]));
        selector_group.add(&row);

        let group = util::card("PiKVM", None);
        let name = util::entry_row(&group, "Profile name", "");
        let url = util::entry_row(&group, "Base URL (https://pikvm.local, https://100.64.0.5, http://host:8080/prefix)", "");
        let labels: Vec<&str> = AuthMethod::ALL.iter().map(|m| m.label()).collect();
        let auth = util::combo_row(&group, "Authentication", Some("How each request is authenticated"), &labels, 0);
        let user = util::entry_row(&group, "User", "admin");

        let secrets_group = util::card("Credentials", Some("Stored in the Secret Service, never in settings.toml"));
        let password = util::password_row(&secrets_group, "Password", "");
        let totp_mode = util::combo_row(
            &secrets_group,
            "Two-factor (TOTP)",
            Some("PiKVM appends the one-time code to the password"),
            &["Off", "Stored secret (auto)", "Ask for the code on connect"],
            0,
        );
        let totp_secret = util::password_row(&secrets_group, "TOTP secret (base32, from /etc/kvmd/totp.secret)", "");

        let advanced_group = util::card("Advanced", None);
        let token = util::password_row(&advanced_group, "Existing auth_token (for the token auth method)", "");
        let accept_certs = util::switch_row(
            &advanced_group,
            "Accept self-signed certificate",
            Some("PiKVM ships with a self-signed certificate by default"),
            true,
        );

        let actions_group = util::card("Save & test", None);
        let save = util::suggested_button("Save profile");
        let test = util::button("Test connection");
        let connect = util::button("Save and connect");
        let arow = adw::ActionRow::builder().title("Actions").build();
        arow.add_suffix(&util::button_box(&[&test, &save, &connect]));
        actions_group.add(&arow);
        let test_result = adw::ActionRow::builder().title("Diagnostics").subtitle("Not tested yet").build();
        actions_group.add(&test_result);

        let editor = Rc::new(Self {
            selector_group,
            group,
            secrets_group,
            advanced_group,
            actions_group,
            profile_combo,
            name,
            url,
            auth,
            user,
            password,
            totp_mode,
            totp_secret,
            token,
            accept_certs,
            test_result,
            editing: RefCell::new(None),
            loading: Cell::new(false),
        });

        let weak: Weak<Self> = Rc::downgrade(&editor);
        editor.profile_combo.connect_selected_notify(move |combo| {
            let Some(e) = weak.upgrade() else { return };
            if e.loading.get() {
                return;
            }
            let profiles = state().settings().profiles;
            if let Some(p) = profiles.get(combo.selected() as usize) {
                let id = p.id.clone();
                state().update_settings(|s| s.active_profile = Some(id));
                e.load_active();
            }
        });
        let weak = Rc::downgrade(&editor);
        add.connect_clicked(weak_call!(weak, |e| {
            let p = ConnectionProfile::new("New PiKVM");
            let id = p.id.clone();
            state().update_settings(|s| {
                s.upsert_profile(p);
                s.active_profile = Some(id);
            });
            e.load_active();
        }));
        let weak = Rc::downgrade(&editor);
        remove.connect_clicked(move |b| {
            let Some(e) = weak.upgrade() else { return };
            let Some(p) = e.editing.borrow().clone() else { return };
            let weak2 = weak.clone();
            util::confirm(b, "Delete profile?", &format!("Remove “{}” and its stored secrets.", p.name), "Delete", move || {
                let keys = [p.secret_key_password(), p.secret_key_totp(), p.secret_key_token()];
                if let Some(secrets) = state().secrets() {
                    spawn_then(
                        async move {
                            for k in keys {
                                let _ = secrets.delete(&k).await;
                            }
                        },
                        |_| {},
                    );
                }
                state().update_settings(|s| s.remove_profile(&p.id));
                state().connection.disconnect();
                if let Some(e) = weak2.upgrade() {
                    e.load_active();
                }
            });
        });
        let weak = Rc::downgrade(&editor);
        save.connect_clicked(weak_call!(weak, |e| {
            e.save();
        }));
        let weak = Rc::downgrade(&editor);
        test.connect_clicked(weak_call!(weak, |e| e.test()));
        let weak = Rc::downgrade(&editor);
        connect.connect_clicked(move |_| {
            let Some(e) = weak.upgrade() else { return };
            if e.save() {
                if let Some(win) = crate::window::PvWindow::current() {
                    win.connect_active_profile(None);
                }
            }
        });
        let weak = Rc::downgrade(&editor);
        editor.auth.connect_selected_notify(weak_call!(weak, |e| e.update_visibility()));
        let weak = Rc::downgrade(&editor);
        editor.totp_mode.connect_selected_notify(weak_call!(weak, |e| e.update_visibility()));
        editor
    }

    fn set_profile_names(&self) {
        let settings = state().settings();
        let names: Vec<String> = settings.profiles.iter().map(|p| p.name.clone()).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        self.loading.set(true);
        self.profile_combo.set_model(Some(&gtk::StringList::new(&refs)));
        if let Some(idx) = settings
            .active_profile
            .as_ref()
            .and_then(|a| settings.profiles.iter().position(|p| p.id == *a))
        {
            self.profile_combo.set_selected(idx as u32);
        }
        self.loading.set(false);
    }

    fn load_active(&self) {
        self.set_profile_names();
        self.loading.set(true);
        let p = state()
            .settings()
            .active_profile()
            .cloned()
            .unwrap_or_else(|| ConnectionProfile::new("New PiKVM"));
        self.name.set_text(&p.name);
        self.url.set_text(&p.base_url);
        self.auth
            .set_selected(AuthMethod::ALL.iter().position(|m| *m == p.auth_method).unwrap_or(0) as u32);
        self.user.set_text(&p.user);
        self.totp_mode.set_selected(match p.totp {
            TotpMode::Off => 0,
            TotpMode::Secret => 1,
            TotpMode::Ask => 2,
        });
        self.accept_certs.set_active(p.accept_invalid_certs);
        self.password.set_text("");
        self.totp_secret.set_text("");
        self.token.set_text("");
        self.test_result.set_subtitle("Not tested yet");
        *self.editing.borrow_mut() = Some(p.clone());
        self.update_visibility();
        self.loading.set(false);

        if let Some(secrets) = state().secrets() {
            let (pw, ts, tk) = (self.password.clone(), self.totp_secret.clone(), self.token.clone());
            let keys = (p.secret_key_password(), p.secret_key_totp(), p.secret_key_token());
            spawn_then(
                async move {
                    (
                        secrets.get(&keys.0).await.ok().flatten(),
                        secrets.get(&keys.1).await.ok().flatten(),
                        secrets.get(&keys.2).await.ok().flatten(),
                    )
                },
                move |(a, b, c)| {
                    pw.set_text(&a.unwrap_or_default());
                    ts.set_text(&b.unwrap_or_default());
                    tk.set_text(&c.unwrap_or_default());
                },
            );
        }
    }

    fn update_visibility(&self) {
        let method = AuthMethod::ALL[self.auth.selected() as usize];
        self.token.set_visible(method == AuthMethod::Token);
        self.password.set_visible(method != AuthMethod::Token);
        self.user.set_visible(method != AuthMethod::Token);
        self.totp_secret.set_visible(self.totp_mode.selected() == 1);
    }

    fn collect(&self) -> anyhow::Result<ConnectionProfile> {
        let mut p = self.editing.borrow().clone().unwrap_or_else(|| ConnectionProfile::new("New PiKVM"));
        p.name = self.name.text().trim().to_string();
        if p.name.is_empty() {
            p.name = "PiKVM".into();
        }
        let url = ConnectionConfig::parse_base_url(&self.url.text())?;
        p.base_url = url.to_string().trim_end_matches('/').to_string();
        p.auth_method = AuthMethod::ALL[self.auth.selected() as usize];
        p.user = self.user.text().trim().to_string();
        p.totp = match self.totp_mode.selected() {
            1 => TotpMode::Secret,
            2 => TotpMode::Ask,
            _ => TotpMode::Off,
        };
        p.accept_invalid_certs = self.accept_certs.is_active();
        Ok(p)
    }

    /// Persist the profile and its secrets; returns `false` when validation failed.
    fn save(&self) -> bool {
        let p = match self.collect() {
            Ok(p) => p,
            Err(e) => {
                toast_error("Profile", &e);
                return false;
            }
        };
        let id = p.id.clone();
        state().update_settings(|s| {
            s.upsert_profile(p.clone());
            s.active_profile = Some(id);
        });
        *self.editing.borrow_mut() = Some(p.clone());
        self.url.set_text(&p.base_url);
        self.set_profile_names();
        let Some(secrets) = state().secrets() else {
            toast("Profile saved (secret store not ready, credentials not stored)");
            return true;
        };
        let (pw, ts, tk) = (
            self.password.text().to_string(),
            self.totp_secret.text().to_string(),
            self.token.text().to_string(),
        );
        let keys = (p.secret_key_password(), p.secret_key_totp(), p.secret_key_token());
        spawn_then(
            async move {
                set_or_delete(secrets.as_ref(), &keys.0, &pw).await?;
                set_or_delete(secrets.as_ref(), &keys.1, &ts).await?;
                set_or_delete(secrets.as_ref(), &keys.2, &tk).await?;
                Ok::<_, anyhow::Error>(())
            },
            |r| match r {
                Ok(()) => toast("Profile saved"),
                Err(e) => toast_error("Saving secrets", &e),
            },
        );
        true
    }

    fn test(&self) {
        let p = match self.collect() {
            Ok(p) => p,
            Err(e) => {
                self.test_result.set_subtitle(&format!("❌ {e}"));
                return;
            }
        };
        let creds = pikvm::auth::Credentials {
            user: p.user.clone(),
            password: self.password.text().to_string(),
            totp_secret: (p.totp == TotpMode::Secret).then(|| self.totp_secret.text().to_string()),
            totp_code: None,
            token: Some(self.token.text().to_string()).filter(|t| !t.is_empty()),
        };
        self.test_result.set_subtitle("Testing…");
        let result_row = self.test_result.clone();
        let secrets = state().secrets();
        spawn_then(
            async move {
                let creds = match (creds.password.is_empty() && creds.token.is_none(), secrets) {
                    (true, Some(s)) => credentials_for(&p, &s, None).await.unwrap_or(creds),
                    _ => creds,
                };
                let client = build_client(&p, creds)?;
                let diag = tokio::time::timeout(TEST_TIMEOUT, diagnose(&client))
                    .await
                    .map_err(|_| anyhow::anyhow!("timed out after {} s", TEST_TIMEOUT.as_secs()))?;
                Ok::<_, anyhow::Error>(diag)
            },
            move |r| match r {
                Ok(d) => result_row.set_subtitle(&format!("{} {}", if d.ok { "✅" } else { "⚠️" }, d.summary)),
                Err(e) => result_row.set_subtitle(&format!("❌ {e}")),
            },
        );
    }
}
