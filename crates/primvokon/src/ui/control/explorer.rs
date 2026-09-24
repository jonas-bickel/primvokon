//! API Explorer sub-page: every endpoint of the catalogue with a generated run form.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use adw::prelude::*;
use pikvm::catalog::{self, Category, Endpoint, Importance, ParamKind};

use super::client;
use crate::util::{self, confirm, spawn_then};

/// Widgets that produce one parameter value.
enum ParamWidget {
    Entry(adw::EntryRow),
    Bool(adw::ComboRow),
    Enum(adw::ComboRow, &'static [&'static str]),
}

impl ParamWidget {
    fn value(&self) -> String {
        match self {
            ParamWidget::Entry(e) => e.text().to_string(),
            ParamWidget::Bool(c) => match c.selected() {
                1 => "1".into(),
                2 => "0".into(),
                _ => String::new(),
            },
            ParamWidget::Enum(c, values) => match c.selected() {
                0 => String::new(),
                i => values.get(i as usize - 1).map(|s| s.to_string()).unwrap_or_default(),
            },
        }
    }
}

pub fn page() -> adw::NavigationPage {
    let split = adw::OverlaySplitView::builder()
        .sidebar_width_fraction(0.35)
        .min_sidebar_width(260.0)
        .build();

    // Sidebar: importance filter + endpoint list grouped by category.
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let filter = gtk::DropDown::from_strings(&["All endpoints", "Core only", "Core + advanced", "Explorer only"]);
    filter.set_margin_top(6);
    filter.set_margin_start(6);
    filter.set_margin_end(6);
    sidebar.append(&filter);
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    list.add_css_class("navigation-sidebar");
    list.set_header_func(|row, before| {
        let cat = row_category(row);
        let prev = before.map(row_category);
        if prev != Some(cat) {
            let label = gtk::Label::new(Some(cat.label()));
            label.add_css_class("heading");
            label.add_css_class("dim-label");
            label.set_xalign(0.0);
            label.set_margin_top(12);
            label.set_margin_start(12);
            label.set_margin_bottom(4);
            row.set_header(Some(&label));
        } else {
            row.set_header(None::<&gtk::Widget>);
        }
    });
    for ep in catalog::ENDPOINTS {
        let row = gtk::ListBoxRow::new();
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        b.set_margin_top(4);
        b.set_margin_bottom(4);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let title = gtk::Label::new(Some(ep.name));
        title.set_xalign(0.0);
        let route = gtk::Label::new(Some(&format!("{} {}", ep.method, ep.route)));
        route.set_xalign(0.0);
        route.add_css_class("dim-label");
        route.add_css_class("caption");
        route.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);
        text.append(&route);
        text.set_hexpand(true);
        b.append(&text);
        b.append(&util::make_pill(
            ep.importance.label(),
            Some(match ep.importance {
                Importance::Core => "success",
                Importance::Advanced => "accent",
                Importance::Explorer => "warning",
            }),
        ));
        row.set_child(Some(&b));
        unsafe {
            row.set_data("endpoint-id", ep.id);
            row.set_data("category", ep.category as u8);
        }
        list.append(&row);
    }
    let list_ref = list.clone();
    filter.connect_selected_notify(move |f| {
        let sel = f.selected();
        list_ref.set_filter_func(move |row| {
            let id: &str = unsafe {
                row.data::<&'static str>("endpoint-id")
                    .map(|d| *d.as_ref())
                    .unwrap_or("")
            };
            let Some(ep) = catalog::find(id) else { return true };
            match sel {
                1 => ep.importance == Importance::Core,
                2 => ep.importance != Importance::Explorer,
                3 => ep.importance == Importance::Explorer,
                _ => true,
            }
        });
    });
    let scrolled = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();
    sidebar.append(&scrolled);
    split.set_sidebar(Some(&sidebar));

    // Content: form for the selected endpoint.
    let (content_scroll, content) = util::cards_page();
    let placeholder = util::status_page(
        "system-search-symbolic",
        "API Explorer",
        "Pick an endpoint on the left.",
    );
    content.append(&placeholder);
    split.set_content(Some(&content_scroll));

    let current: Rc<RefCell<Option<gtk::Widget>>> = Rc::new(RefCell::new(None));
    let content_ref = content.clone();
    list.connect_row_selected(move |_, row| {
        let Some(row) = row else { return };
        let id: &str = unsafe {
            row.data::<&'static str>("endpoint-id")
                .map(|d| *d.as_ref())
                .unwrap_or("")
        };
        let Some(ep) = catalog::find(id) else { return };
        placeholder.set_visible(false);
        if let Some(old) = current.borrow_mut().take() {
            content_ref.remove(&old);
        }
        let form = build_form(ep);
        content_ref.append(&form);
        *current.borrow_mut() = Some(form.upcast());
    });

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&adw::HeaderBar::new());
    toolbar.set_content(Some(&split));
    adw::NavigationPage::builder()
        .title("API Explorer")
        .child(&toolbar)
        .build()
}

fn row_category(row: &gtk::ListBoxRow) -> Category {
    let idx: u8 = unsafe { row.data::<u8>("category").map(|d| *d.as_ref()).unwrap_or(0) };
    Category::ALL[idx as usize % Category::ALL.len()]
}

fn build_form(ep: &'static Endpoint) -> gtk::Box {
    let form = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let head = util::card(ep.name, Some(ep.description));
    let route = util::kv_row(&head, "Route");
    route.set_subtitle(&format!("{} {}", ep.method, ep.route));
    let importance = util::kv_row(&head, "Category / importance");
    importance.set_subtitle(&format!("{} · {}", ep.category.label(), ep.importance.label()));
    form.append(&head);

    let mut widgets: Vec<(&'static str, ParamWidget)> = Vec::new();
    if !ep.params.is_empty() {
        let params = util::card("Parameters", None);
        for p in ep.params {
            let title = if p.required {
                format!("{} *", p.name)
            } else {
                p.name.to_string()
            };
            let widget = match p.kind {
                ParamKind::Bool => {
                    let c = util::combo_row(&params, &title, Some(p.description), &["(unset)", "true", "false"], 0);
                    ParamWidget::Bool(c)
                }
                ParamKind::Enum(values) => {
                    let mut items = vec!["(unset)"];
                    items.extend_from_slice(values);
                    let c = util::combo_row(&params, &title, Some(p.description), &items, 0);
                    ParamWidget::Enum(c, values)
                }
                ParamKind::Text | ParamKind::Int | ParamKind::Float => {
                    let e = adw::EntryRow::builder()
                        .title(format!("{title} — {}", p.description))
                        .build();
                    if matches!(p.kind, ParamKind::Int | ParamKind::Float) {
                        e.set_input_purpose(gtk::InputPurpose::Number);
                    }
                    params.add(&e);
                    ParamWidget::Entry(e)
                }
            };
            widgets.push((p.name, widget));
        }
        form.append(&params);
    }

    let run_group = util::card("Run", None);
    let run = util::suggested_button("Run request");
    let status_row = util::kv_row(&run_group, "Status");
    status_row.set_subtitle("not run yet");
    let result = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    let result_scroll = gtk::ScrolledWindow::builder()
        .child(&result)
        .min_content_height(220)
        .build();
    result_scroll.add_css_class("card");
    util::button_row(
        &run_group,
        "Execute",
        Some(if ep.destructive {
            "Asks for confirmation"
        } else {
            "Read-only or safe"
        }),
        &run,
    );
    form.append(&run_group);
    form.append(&result_scroll);

    let widgets = Rc::new(widgets);
    run.connect_clicked(move |b| {
        let Some(c) = client() else { return };
        let values: BTreeMap<String, String> = widgets.iter().map(|(n, w)| (n.to_string(), w.value())).collect();
        let (status_row, result) = (status_row.clone(), result.clone());
        let exec = move || {
            status_row.set_subtitle("running…");
            spawn_then(async move { ep.run(&c, &values).await }, move |r| match r {
                Ok(resp) => {
                    status_row.set_subtitle(&format!("HTTP {} · {}", resp.status, resp.content_type));
                    let body = if resp.content_type.contains("json") {
                        resp.json()
                            .map(|v| serde_json::to_string_pretty(&v).unwrap_or_default())
                            .unwrap_or_else(|_| resp.text())
                    } else if resp.content_type.starts_with("image/") {
                        format!("<{} bytes of {}>", resp.body.len(), resp.content_type)
                    } else {
                        resp.text()
                    };
                    result.buffer().set_text(&body);
                }
                Err(e) => {
                    status_row.set_subtitle("error");
                    result.buffer().set_text(&e.to_string());
                }
            });
        };
        if ep.destructive {
            confirm(
                b,
                "Run this request?",
                &format!("{} {} may affect the host.", ep.method, ep.route),
                "Run",
                exec,
            );
        } else {
            exec();
        }
    });
    form
}
