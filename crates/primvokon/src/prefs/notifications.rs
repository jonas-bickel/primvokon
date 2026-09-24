//! ntfy settings.

use adw::prelude::*;
use primvokon_core::ntfy::{Notification, Notifier};
use primvokon_core::secrets::set_or_delete;
use primvokon_core::settings::{NtfyAuth, NtfySettings};

use super::{bind_combo, bind_entry, bind_spin, bind_switch, page as new_page, split_list};
use crate::state::{state, AppState};
use crate::util::{self, spawn_then, toast, toast_error};

pub fn page() -> adw::PreferencesPage {
    let page = new_page("Notifications", "preferences-system-notifications-symbolic");
    let s = state().settings();
    let n = &s.ntfy;

    let g = util::card("ntfy server", Some("Self-hosted (e.g. on TrueNAS) or ntfy.sh"));
    let server = util::entry_row(&g, "Server URL", &n.server_url);
    bind_entry(&server, |s, v| s.ntfy.server_url = v);
    let topic = util::entry_row(&g, "Topic", &n.topic);
    bind_entry(&topic, |s, v| s.ntfy.topic = v);
    let auth = util::combo_row(
        &g,
        "Authentication",
        None,
        &["None", "Access token", "Username & password"],
        match n.auth {
            NtfyAuth::None => 0,
            NtfyAuth::Token => 1,
            NtfyAuth::Basic => 2,
        },
    );
    bind_combo(&auth, |s, i| {
        s.ntfy.auth = match i {
            1 => NtfyAuth::Token,
            2 => NtfyAuth::Basic,
            _ => NtfyAuth::None,
        }
    });
    let user = util::entry_row(&g, "Username", &n.username);
    bind_entry(&user, |s, v| s.ntfy.username = v);
    let token = secret_row(&g, "Access token", NtfySettings::SECRET_KEY_TOKEN);
    let password = secret_row(&g, "Password", NtfySettings::SECRET_KEY_PASSWORD);
    let update_vis = {
        let (auth, user, token, password) = (auth.clone(), user.clone(), token.clone(), password.clone());
        move || {
            let sel = auth.selected();
            token.set_visible(sel == 1);
            user.set_visible(sel == 2);
            password.set_visible(sel == 2);
        }
    };
    update_vis();
    auth.connect_selected_notify(move |_| update_vis());
    page.add(&g);

    let m = util::card("Message", None);
    let priority = util::spin_row(
        &m,
        "Priority",
        Some("1 = min … 5 = max"),
        1.0,
        5.0,
        1.0,
        f64::from(n.priority),
    );
    bind_spin(&priority, |s, v| s.ntfy.priority = v as u8);
    let tags = util::entry_row(&m, "Tags (comma separated emoji short codes)", &n.tags.join(","));
    bind_entry(&tags, |s, v| s.ntfy.tags = split_list(&v));
    let title = util::entry_row(&m, "Title template ({reason}, {host})", &n.title_template);
    bind_entry(&title, |s, v| s.ntfy.title_template = v);
    let attach = util::switch_row(
        &m,
        "Attach snapshot",
        Some("Send the screenshot with the notification"),
        n.attach_snapshot,
    );
    bind_switch(&attach, |s, v| s.ntfy.attach_snapshot = v);
    page.add(&m);

    let t = util::card("Test", None);
    let send = util::suggested_button("Send test notification");
    let row = util::button_row(&t, "Test notification", Some("Publishes a message to the topic"), &send);
    send.connect_clicked(move |_| {
        let Some(secrets) = state().secrets() else { return };
        let settings = state().settings();
        let row = row.clone();
        spawn_then(
            async move {
                let n = AppState::ntfy_notifier(&settings, &secrets).await?;
                n.notify(&Notification {
                    title: "PRIMVOKON test".into(),
                    message: "If you can read this, ntfy is configured correctly.".into(),
                    priority: None,
                    tags: vec!["white_check_mark".into()],
                    attachment: None,
                })
                .await
            },
            move |r| match r {
                Ok(()) => {
                    row.set_subtitle("✅ Sent");
                    toast("Test notification sent");
                }
                Err(e) => {
                    row.set_subtitle(&format!("❌ {e}"));
                    toast_error("ntfy", &e);
                }
            },
        );
    });
    page.add(&t);
    page
}

fn secret_row(group: &adw::PreferencesGroup, title: &str, key: &'static str) -> adw::PasswordEntryRow {
    let row = util::password_row(group, title, "");
    if let Some(secrets) = state().secrets() {
        let r = row.clone();
        spawn_then(async move { secrets.get(key).await.ok().flatten() }, move |v| {
            r.set_text(&v.unwrap_or_default())
        });
    }
    row.set_show_apply_button(true);
    row.connect_apply(move |r| {
        let Some(secrets) = state().secrets() else { return };
        let value = r.text().to_string();
        spawn_then(
            async move { set_or_delete(secrets.as_ref(), key, &value).await },
            |r| match r {
                Ok(()) => toast("Saved"),
                Err(e) => toast_error("Secret", &e),
            },
        );
    });
    row
}
