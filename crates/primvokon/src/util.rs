//! Small helpers shared by every page: async bridging, toasts, formatting, pills.

use std::cell::RefCell;
use std::future::Future;

use adw::prelude::*;
use gtk::glib;

thread_local! {
    static TOAST_OVERLAY: RefCell<Option<adw::ToastOverlay>> = const { RefCell::new(None) };
}

pub const RES_PREFIX: &str = "/io/github/jonas_bickel/Primvokon";

/// Register the window's toast overlay so any page can show feedback.
pub fn set_toast_overlay(overlay: &adw::ToastOverlay) {
    TOAST_OVERLAY.with(|t| *t.borrow_mut() = Some(overlay.clone()));
}

pub fn toast(message: &str) {
    TOAST_OVERLAY.with(|t| {
        if let Some(overlay) = t.borrow().as_ref() {
            overlay.add_toast(adw::Toast::builder().title(message).timeout(3).build());
        }
    });
}

/// Error toast with a "Details" button that opens the full message.
pub fn toast_error(context: &str, err: &dyn std::fmt::Display) {
    let full = err.to_string();
    let short = full
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(90)
        .collect::<String>();
    tracing::warn!("{context}: {full}");
    TOAST_OVERLAY.with(|t| {
        if let Some(overlay) = t.borrow().as_ref() {
            let toast = adw::Toast::builder()
                .title(format!("{context}: {short}"))
                .timeout(6)
                .button_label("Details")
                .build();
            let context = context.to_string();
            let overlay_ref = overlay.clone();
            toast.connect_button_clicked(move |_| {
                let dialog = adw::AlertDialog::builder().heading(&context).body(&full).build();
                dialog.add_response("close", "Close");
                dialog.present(Some(&overlay_ref));
            });
            overlay.add_toast(toast);
        }
    });
}

/// Run `fut` on the tokio runtime and call `done` on the main thread with its output.
pub fn spawn_then<F, T, D>(fut: F, done: D)
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
    D: FnOnce(T) + 'static,
{
    let handle = primvokon_core::runtime::spawn(fut);
    glib::spawn_future_local(async move {
        match handle.await {
            Ok(v) => done(v),
            Err(e) => tracing::error!("background task failed: {e}"),
        }
    });
}

/// Like [`spawn_then`] for fallible work: toasts errors with `context`.
pub fn spawn_result<F, T, D>(context: &'static str, fut: F, done: D)
where
    F: Future<Output = anyhow::Result<T>> + Send + 'static,
    T: Send + 'static,
    D: FnOnce(T) + 'static,
{
    spawn_then(fut, move |r| match r {
        Ok(v) => done(v),
        Err(e) => toast_error(context, &e),
    });
}

/// Ask for confirmation before running `on_confirm`.
pub fn confirm(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    body: &str,
    action: &str,
    on_confirm: impl FnOnce() + 'static,
) {
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).build();
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("confirm", action);
    dialog.set_response_appearance("confirm", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let cb = RefCell::new(Some(on_confirm));
    dialog.connect_response(None, move |_, resp| {
        if resp == "confirm" {
            if let Some(f) = cb.borrow_mut().take() {
                f();
            }
        }
    });
    dialog.present(Some(parent));
}

/// Set the pill style class (`success`, `warning`, `error`, `accent`, or none) on a label.
pub fn set_pill(label: &gtk::Label, text: &str, class: Option<&str>) {
    label.set_text(text);
    for c in ["success", "warning", "error", "accent"] {
        label.remove_css_class(c);
    }
    if let Some(c) = class {
        label.add_css_class(c);
    }
}

pub fn make_pill(text: &str, class: Option<&str>) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("pill");
    if let Some(c) = class {
        l.add_css_class(c);
    }
    l
}

/// LED-like indicator.
pub fn make_led() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    b.add_css_class("led");
    b.set_valign(gtk::Align::Center);
    b
}

pub fn set_led(led: &gtk::Box, on: bool, busy: bool) {
    led.remove_css_class("on");
    led.remove_css_class("busy");
    if busy {
        led.add_css_class("busy");
    } else if on {
        led.add_css_class("on");
    }
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

pub fn format_ts(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string())
        .unwrap_or_default()
}

pub fn format_time(ts: i64) -> String {
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

pub fn yes_no(b: bool) -> &'static str {
    if b {
        "yes"
    } else {
        "no"
    }
}

pub fn on_off(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

/// A preferences-style card.
pub fn card(title: &str, description: Option<&str>) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::builder()
        .title(glib::markup_escape_text(title))
        .build();
    if let Some(d) = description {
        g.set_description(Some(&glib::markup_escape_text(d)));
    }
    g
}

/// Key/value row; the value is shown as the subtitle.
pub fn kv_row(group: &adw::PreferencesGroup, title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle("—")
        .subtitle_selectable(true)
        .build();
    row.add_css_class("property");
    group.add(&row);
    row
}

/// Row with a button at the end.
pub fn button_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: Option<&str>,
    button: &gtk::Button,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .activatable_widget(button)
        .build();
    if let Some(s) = subtitle {
        row.set_subtitle(s);
    }
    button.set_valign(gtk::Align::Center);
    row.add_suffix(button);
    group.add(&row);
    row
}

/// Horizontal box of buttons used as a row suffix / card header.
pub fn button_box(buttons: &[&gtk::Button]) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    b.set_valign(gtk::Align::Center);
    for btn in buttons {
        btn.set_valign(gtk::Align::Center);
        b.append(*btn);
    }
    b
}

pub fn button(label: &str) -> gtk::Button {
    gtk::Button::builder().label(label).build()
}

pub fn destructive_button(label: &str) -> gtk::Button {
    let b = button(label);
    b.add_css_class("destructive-action");
    b
}

pub fn suggested_button(label: &str) -> gtk::Button {
    let b = button(label);
    b.add_css_class("suggested-action");
    b
}

/// Text entry row helper (returns the row; read with `.text()`).
pub fn entry_row(group: &adw::PreferencesGroup, title: &str, text: &str) -> adw::EntryRow {
    let r = adw::EntryRow::builder().title(title).text(text).build();
    group.add(&r);
    r
}

pub fn password_row(group: &adw::PreferencesGroup, title: &str, text: &str) -> adw::PasswordEntryRow {
    let r = adw::PasswordEntryRow::builder().title(title).text(text).build();
    group.add(&r);
    r
}

pub fn switch_row(group: &adw::PreferencesGroup, title: &str, subtitle: Option<&str>, active: bool) -> adw::SwitchRow {
    let r = adw::SwitchRow::builder().title(title).active(active).build();
    if let Some(s) = subtitle {
        r.set_subtitle(s);
    }
    group.add(&r);
    r
}

pub fn spin_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: Option<&str>,
    min: f64,
    max: f64,
    step: f64,
    value: f64,
) -> adw::SpinRow {
    let r = adw::SpinRow::with_range(min, max, step);
    r.set_title(title);
    if let Some(s) = subtitle {
        r.set_subtitle(s);
    }
    r.set_value(value);
    group.add(&r);
    r
}

pub fn combo_row(
    group: &adw::PreferencesGroup,
    title: &str,
    subtitle: Option<&str>,
    items: &[&str],
    selected: u32,
) -> adw::ComboRow {
    let model = gtk::StringList::new(items);
    let r = adw::ComboRow::builder()
        .title(title)
        .model(&model)
        .selected(selected)
        .build();
    if let Some(s) = subtitle {
        r.set_subtitle(s);
    }
    group.add(&r);
    r
}

/// Scrolled container for a page made of cards.
pub fn cards_page() -> (gtk::ScrolledWindow, gtk::Box) {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
    content.set_margin_top(18);
    content.set_margin_bottom(24);
    content.set_margin_start(18);
    content.set_margin_end(18);
    let clamp = adw::Clamp::builder()
        .maximum_size(900)
        .tightening_threshold(700)
        .child(&content)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&clamp)
        .vexpand(true)
        .build();
    (scrolled, content)
}

pub fn status_page(icon: &str, title: &str, description: &str) -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name(icon)
        .title(title)
        .description(description)
        .vexpand(true)
        .build()
}

/// Copy text to the clipboard.
pub fn copy_to_clipboard(widget: &impl IsA<gtk::Widget>, text: &str) {
    widget.clipboard().set_text(text);
    toast("Copied to clipboard");
}
