//! Preferences dialog with one page per concern.

mod agent;
mod ai;
mod capabilities;
mod connection;
mod notifications;
mod recall;
mod watcher;

use adw::prelude::*;

pub struct PreferencesDialog;

impl PreferencesDialog {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> adw::PreferencesDialog {
        let dialog = adw::PreferencesDialog::builder().title("Preferences").search_enabled(true).build();
        dialog.add(&connection::page());
        dialog.add(&capabilities::page());
        dialog.add(&ai::page());
        dialog.add(&notifications::page());
        dialog.add(&watcher::page());
        dialog.add(&recall::page());
        dialog.add(&agent::page());
        dialog
    }
}

/// Common page builder.
pub(crate) fn page(title: &str, icon: &str) -> adw::PreferencesPage {
    adw::PreferencesPage::builder().title(title).icon_name(icon).build()
}
