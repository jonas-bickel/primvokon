//! Log viewer sub-page: `GET /api/log` with seek, follow and filter.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use futures_util::StreamExt;
use tokio_util::sync::CancellationToken;

use super::client;
use crate::util::{self, spawn_result, toast_error};

pub fn page() -> adw::NavigationPage {
    let toolbar = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    toolbar.add_top_bar(&header);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_margin_top(6);
    controls.set_margin_bottom(6);
    controls.set_margin_start(12);
    controls.set_margin_end(12);
    let seek = gtk::SpinButton::with_range(0.0, 86400.0 * 7.0, 60.0);
    seek.set_value(3600.0);
    seek.set_tooltip_text(Some("Seconds to look back"));
    controls.append(&gtk::Label::new(Some("Seek (s)")));
    controls.append(&seek);
    let load = util::button("Load");
    controls.append(&load);
    let follow = gtk::ToggleButton::with_label("Follow");
    controls.append(&follow);
    let filter = gtk::SearchEntry::builder()
        .placeholder_text("Filter lines")
        .hexpand(true)
        .build();
    controls.append(&filter);
    let clear = gtk::Button::from_icon_name("edit-clear-all-symbolic");
    controls.append(&clear);

    let view = gtk::TextView::builder().editable(false).monospace(true).build();
    view.add_css_class("mono");
    let scrolled = gtk::ScrolledWindow::builder().child(&view).vexpand(true).build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&controls);
    content.append(&scrolled);
    toolbar.set_content(Some(&content));

    // Raw lines are kept so the filter can be re-applied.
    let lines: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    let follow_cancel: Rc<RefCell<Option<CancellationToken>>> = Rc::new(RefCell::new(None));

    let render = {
        let lines = lines.clone();
        let view = view.clone();
        let filter = filter.clone();
        Rc::new(move || {
            let needle = filter.text().to_lowercase();
            let text: String = lines
                .borrow()
                .iter()
                .filter(|l| needle.is_empty() || l.to_lowercase().contains(&needle))
                .map(|l| format!("{l}\n"))
                .collect();
            view.buffer().set_text(&text);
            let mut end = view.buffer().end_iter();
            view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
        })
    };

    let (l, r, s) = (lines.clone(), render.clone(), seek.clone());
    load.connect_clicked(move |_| {
        let Some(c) = client() else { return };
        let seek = s.value() as u64;
        let (l, r) = (l.clone(), r.clone());
        spawn_result(
            "Log",
            async move { Ok(c.system().log(Some(seek)).await?) },
            move |text| {
                *l.borrow_mut() = text.lines().map(str::to_string).collect();
                r();
            },
        );
    });
    let r = render.clone();
    filter.connect_search_changed(move |_| r());
    let (l, r) = (lines.clone(), render.clone());
    clear.connect_clicked(move |_| {
        l.borrow_mut().clear();
        r();
    });

    let (l, r, s, fc) = (lines.clone(), render.clone(), seek.clone(), follow_cancel.clone());
    follow.connect_toggled(move |t| {
        if let Some(c) = fc.borrow_mut().take() {
            c.cancel();
        }
        if !t.is_active() {
            return;
        }
        let Some(client) = client() else {
            t.set_active(false);
            return;
        };
        let cancel = CancellationToken::new();
        *fc.borrow_mut() = Some(cancel.clone());
        let seek = s.value() as u64;
        let (tx, rx) = async_channel::unbounded::<String>();
        primvokon_core::runtime::spawn(async move {
            match client.system().log_follow(Some(seek)).await {
                Ok(stream) => {
                    let mut stream = std::pin::pin!(stream);
                    loop {
                        let next = tokio::select! {
                            _ = cancel.cancelled() => break,
                            n = stream.next() => n,
                        };
                        match next {
                            Some(Ok(chunk)) => {
                                let _ = tx.send(chunk).await;
                            }
                            Some(Err(e)) => {
                                let _ = tx.send(format!("[follow error: {e}]\n")).await;
                                break;
                            }
                            None => break,
                        }
                    }
                }
                Err(e) => {
                    let _ = tx.send(format!("[follow failed: {e}]\n")).await;
                }
            }
        });
        let (l, r) = (l.clone(), r.clone());
        gtk::glib::spawn_future_local(async move {
            let mut partial = String::new();
            while let Ok(chunk) = rx.recv().await {
                partial.push_str(&chunk);
                while let Some(pos) = partial.find('\n') {
                    let line = partial[..pos].to_string();
                    partial = partial[pos + 1..].to_string();
                    l.borrow_mut().push(line);
                }
                r();
            }
        });
    });

    let page = adw::NavigationPage::builder()
        .title("Log viewer")
        .child(&toolbar)
        .build();
    let fc = follow_cancel.clone();
    page.connect_hidden(move |_| {
        if let Some(c) = fc.borrow_mut().take() {
            c.cancel();
        }
    });
    if client().is_none() {
        toast_error("Log viewer", &"connect to a PiKVM first");
    } else {
        load.emit_clicked();
    }
    page
}
