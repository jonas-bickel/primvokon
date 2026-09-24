//! Preferences dialog with one page per concern, plus binding helpers that persist a setting
//! the moment a row changes.

mod agent;
mod ai;
mod capabilities;
mod connection;
mod notifications;
mod recall;
mod watcher;

use adw::prelude::*;
use primvokon_core::settings::Settings;

use crate::state::state;

pub struct PreferencesDialog;

impl PreferencesDialog {
    #[allow(clippy::new_ret_no_self)]
    pub fn new() -> adw::PreferencesDialog {
        let dialog = adw::PreferencesDialog::builder()
            .title("Preferences")
            .search_enabled(true)
            .build();
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

pub(crate) fn bind_switch(row: &adw::SwitchRow, apply: impl Fn(&mut Settings, bool) + 'static) {
    row.connect_active_notify(move |r| {
        let v = r.is_active();
        state().update_settings(|s| apply(s, v));
    });
}

pub(crate) fn bind_entry(row: &adw::EntryRow, apply: impl Fn(&mut Settings, String) + 'static) {
    row.connect_apply(move |r| {
        let v = r.text().to_string();
        state().update_settings(|s| apply(s, v));
    });
    row.set_show_apply_button(true);
}

pub(crate) fn bind_spin(row: &adw::SpinRow, apply: impl Fn(&mut Settings, f64) + 'static) {
    row.connect_value_notify(move |r| {
        let v = r.value();
        state().update_settings(|s| apply(s, v));
    });
}

pub(crate) fn bind_combo(row: &adw::ComboRow, apply: impl Fn(&mut Settings, u32) + 'static) {
    row.connect_selected_notify(move |r| {
        let v = r.selected();
        state().update_settings(|s| apply(s, v));
    });
}

/// Multi-line text setting inside an expander row; saved on focus loss.
pub(crate) fn text_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: Option<&str>,
    text: &str,
    apply: impl Fn(&mut Settings, String) + 'static,
) {
    let expander = adw::ExpanderRow::builder().title(title).build();
    if let Some(s) = subtitle {
        expander.set_subtitle(s);
    }
    let view = gtk::TextView::builder().wrap_mode(gtk::WrapMode::WordChar).build();
    view.buffer().set_text(text);
    view.set_margin_top(6);
    view.set_margin_bottom(6);
    view.set_margin_start(12);
    view.set_margin_end(12);
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&view)
        .min_content_height(120)
        .build();
    let save = gtk::Button::with_label("Save");
    save.set_halign(gtk::Align::End);
    save.set_margin_end(12);
    save.set_margin_bottom(6);
    let buffer = view.buffer();
    save.connect_clicked(move |_| {
        let t = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true).to_string();
        state().update_settings(|s| apply(s, t));
        crate::util::toast("Saved");
    });
    let holder = gtk::Box::new(gtk::Orientation::Vertical, 6);
    holder.append(&scrolled);
    holder.append(&save);
    expander.add_row(&holder);
    group.add(&expander);
}

/// Comma separated list → Vec<String>.
pub(crate) fn split_list(text: &str) -> Vec<String> {
    text.split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
