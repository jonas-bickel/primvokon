//! Recall page: capture status, day list, summaries and raw timeline (USP 2).

use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::time::Duration;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use primvokon_core::recall::{export, CaptureService, RecallEvent, Scheduler, SchedulerEvent, Summariser};
use primvokon_core::screen::{OcrExtractor, PikvmScreen, TextExtractor, VisionExtractor};
use primvokon_core::secrets::SharedSecrets;
use primvokon_core::settings::{ExtractStrategy, Settings};
use primvokon_core::storage::Storage;
use primvokon_core::storage::{today, DayOverview};

use crate::state::{state, AppState};
use crate::util::{self, format_time, spawn_then, toast, toast_error};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct RecallPage {
        pub status_row: adw::ActionRow,
        pub toggle: gtk::Button,
        pub days: gtk::ListBox,
        pub day_rows: RefCell<Vec<(String, gtk::ListBoxRow)>>,
        pub day_title: gtk::Label,
        pub summary_view: gtk::TextView,
        pub summary_meta: gtk::Label,
        pub timeline: gtk::ListBox,
        pub timeline_rows: RefCell<Vec<gtk::Widget>>,
        pub selected_day: RefCell<Option<String>>,
        pub want_running: Cell<bool>,
        pub content_stack: gtk::Stack,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RecallPage {
        const NAME: &'static str = "PvRecallPage";
        type Type = super::RecallPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for RecallPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }
    impl WidgetImpl for RecallPage {}
    impl BinImpl for RecallPage {}
}

glib::wrapper! {
    pub struct RecallPage(ObjectSubclass<imp::RecallPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for RecallPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl RecallPage {
    fn setup(&self) {
        let imp = self.imp();
        let split = adw::OverlaySplitView::builder()
            .sidebar_width_fraction(0.3)
            .min_sidebar_width(240.0)
            .build();

        // Sidebar: status + day list.
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let head = gtk::Box::new(gtk::Orientation::Vertical, 6);
        head.set_margin_top(12);
        head.set_margin_start(12);
        head.set_margin_end(12);
        imp.status_row.set_title("Recall");
        imp.status_row.set_subtitle("stopped");
        let status_group = adw::PreferencesGroup::new();
        status_group.add(&imp.status_row);
        head.append(&status_group);
        imp.toggle.set_label("Start recording");
        imp.toggle.add_css_class("suggested-action");
        let summarise_pending = util::button("Summarise pending days");
        let bbox = gtk::Box::new(gtk::Orientation::Vertical, 6);
        bbox.append(&imp.toggle);
        bbox.append(&summarise_pending);
        head.append(&bbox);
        sidebar.append(&head);
        imp.days.add_css_class("navigation-sidebar");
        imp.days.set_selection_mode(gtk::SelectionMode::Single);
        let days_scroll = gtk::ScrolledWindow::builder().child(&imp.days).vexpand(true).build();
        sidebar.append(&days_scroll);
        split.set_sidebar(Some(&sidebar));

        // Content: summary + timeline.
        let (content_scroll, content) = util::cards_page();
        let empty = util::status_page(
            "document-open-recent-symbolic",
            "No day selected",
            "Captures are grouped by day. Pick one on the left.",
        );
        imp.content_stack.add_named(&empty, Some("empty"));
        let day_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        imp.day_title.add_css_class("title-2");
        imp.day_title.set_xalign(0.0);
        imp.day_title.set_hexpand(true);
        title_row.append(&imp.day_title);
        let summarise = util::suggested_button("Summarise");
        let export_md = util::button("Export Markdown");
        let export_json = util::button("Export JSON");
        let delete = util::destructive_button("Delete day");
        title_row.append(&util::button_box(&[&summarise, &export_md, &export_json, &delete]));
        day_box.append(&title_row);
        let summary_card = util::card("Daily summary", None);
        imp.summary_view.set_editable(false);
        imp.summary_view.set_wrap_mode(gtk::WrapMode::WordChar);
        imp.summary_view.set_margin_top(8);
        imp.summary_view.set_margin_bottom(8);
        imp.summary_view.set_margin_start(8);
        imp.summary_view.set_margin_end(8);
        let summary_scroll = gtk::ScrolledWindow::builder()
            .child(&imp.summary_view)
            .min_content_height(160)
            .max_content_height(400)
            .propagate_natural_height(true)
            .build();
        summary_scroll.add_css_class("card");
        summary_card.add(&summary_scroll);
        imp.summary_meta.add_css_class("dim-label");
        imp.summary_meta.set_xalign(0.0);
        summary_card.add(&imp.summary_meta);
        day_box.append(&summary_card);
        let timeline_card = util::card("Raw captures", Some("Click a capture to read the full text"));
        imp.timeline.add_css_class("boxed-list");
        imp.timeline.set_selection_mode(gtk::SelectionMode::None);
        timeline_card.add(&imp.timeline);
        day_box.append(&timeline_card);
        imp.content_stack.add_named(&day_box, Some("day"));
        content.append(&imp.content_stack);
        split.set_content(Some(&content_scroll));
        self.set_child(Some(&split));

        // Wiring.
        let this = self.clone();
        imp.toggle.connect_clicked(move |_| {
            if this.is_running() {
                this.imp().want_running.set(false);
                this.stop();
            } else {
                this.imp().want_running.set(true);
                this.start();
            }
        });
        let this = self.clone();
        summarise_pending.connect_clicked(move |_| this.run_summaries(None));
        let this = self.clone();
        summarise.connect_clicked(move |_| {
            let day = this.imp().selected_day.borrow().clone();
            this.run_summaries(day);
        });
        let this = self.clone();
        export_md.connect_clicked(move |b| this.export(b, true));
        let this = self.clone();
        export_json.connect_clicked(move |b| this.export(b, false));
        let this = self.clone();
        delete.connect_clicked(move |b| {
            let Some(day) = this.imp().selected_day.borrow().clone() else {
                return;
            };
            let page = this.clone();
            util::confirm(
                b,
                "Delete this day?",
                &format!("Remove all captures and the summary of {day}."),
                "Delete",
                move || {
                    if let Err(e) = state().storage.delete_day(&day) {
                        toast_error("Delete", &e);
                    }
                    page.load_days();
                    page.show_day(None);
                },
            );
        });
        let this = self.clone();
        imp.days.connect_row_selected(move |_, row| {
            let day = row.and_then(|r| {
                this.imp()
                    .day_rows
                    .borrow()
                    .iter()
                    .find(|(_, rr)| rr == r)
                    .map(|(d, _)| d.clone())
            });
            this.show_day(day);
        });

        let st = state();
        let this = self.clone();
        st.connection.store.connect_connection(move |store| {
            if store.is_connected() {
                if this.imp().want_running.get() && !this.is_running() {
                    this.start();
                }
            } else if this.is_running() {
                this.stop();
            }
        });
        let this = self.clone();
        st.bus.connect_signal("settings-changed", move || {
            if !state().settings.borrow().capabilities.recall && this.is_running() {
                this.imp().want_running.set(false);
                this.stop();
            }
            this.refresh_status();
        });

        self.load_days();
        self.show_day(None);
        self.refresh_status();
    }

    fn is_running(&self) -> bool {
        state()
            .services
            .borrow()
            .recall
            .as_ref()
            .is_some_and(|r| r.is_running())
    }

    pub fn autostart(&self) {
        let s = state().settings();
        self.imp().want_running.set(s.capabilities.recall);
        if s.capabilities.recall {
            self.start_scheduler(s.recall.summarise_on_startup);
            if state().connection.store.is_connected() {
                self.start();
            }
        }
    }

    fn refresh_status(&self) {
        let imp = self.imp();
        let s = state().settings();
        let running = self.is_running();
        imp.toggle
            .set_label(if running { "Stop recording" } else { "Start recording" });
        if running {
            imp.toggle.remove_css_class("suggested-action");
            imp.toggle.add_css_class("destructive-action");
        } else {
            imp.toggle.remove_css_class("destructive-action");
            imp.toggle.add_css_class("suggested-action");
        }
        imp.status_row.set_subtitle(&format!(
            "{} · every {} s · {}",
            if running { "recording" } else { "stopped" },
            s.recall.interval_secs,
            match s.recall.strategy {
                ExtractStrategy::Ocr => "OCR",
                ExtractStrategy::Vision => "vision",
            }
        ));
    }

    /// Inputs for [`Self::build_summariser`], gathered on the main thread.
    fn summariser_inputs() -> Option<(SharedSecrets, Settings, Storage)> {
        let st = state();
        Some((st.secrets()?, st.settings(), st.storage.clone()))
    }

    /// Build a summariser with the text provider (None when no key is configured).
    async fn build_summariser(inputs: Option<(SharedSecrets, Settings, Storage)>) -> Option<Summariser> {
        let (secrets, settings, storage) = inputs?;
        match AppState::provider(settings.ai.text_provider, &settings, &secrets).await {
            Ok(provider) => Some(Summariser {
                provider,
                storage,
                settings: settings.recall.clone(),
            }),
            Err(e) => {
                tracing::warn!("no summariser: {e}");
                None
            }
        }
    }

    fn start_scheduler(&self, run_on_startup: bool) {
        if state().services.borrow().scheduler.is_some() {
            return;
        }
        let this = self.clone();
        let inputs = Self::summariser_inputs();
        spawn_then(
            async move {
                let summariser = Self::build_summariser(inputs).await;
                Scheduler::start(summariser, run_on_startup)
            },
            move |(scheduler, mut rx)| {
                state().services.borrow_mut().scheduler = Some(scheduler);
                let page = this.clone();
                glib::spawn_future_local(async move {
                    while let Some(ev) = rx.recv().await {
                        match ev {
                            SchedulerEvent::Summarised(days) => {
                                toast(&format!("Summarised {}", days.join(", ")));
                                page.load_days();
                            }
                            SchedulerEvent::Error(e) => toast_error("Summaries", &e),
                        }
                    }
                });
            },
        );
    }

    fn start(&self) {
        if self.is_running() {
            return;
        }
        let st = state();
        let Some(client) = st.connection.client() else {
            toast("Connect to a PiKVM first");
            return;
        };
        let settings = st.settings();
        if !settings.capabilities.recall {
            toast("Enable recall in Preferences → Capabilities");
            return;
        }
        let secrets = st.secrets();
        let storage = st.storage.clone();
        let this = self.clone();
        spawn_then(
            async move {
                let extractor: Box<dyn TextExtractor> = match settings.recall.strategy {
                    ExtractStrategy::Ocr => Box::new(OcrExtractor {
                        langs: settings.recall.ocr_langs.clone(),
                    }),
                    ExtractStrategy::Vision => {
                        let secrets = secrets.ok_or_else(|| anyhow::anyhow!("secret store not ready"))?;
                        let provider = AppState::vision_provider(&settings, &secrets).await.ok_or_else(|| {
                            anyhow::anyhow!(
                                "vision strategy needs a vision provider (check AI settings and the privacy switch)"
                            )
                        })?;
                        Box::new(VisionExtractor { provider })
                    }
                };
                let interval = settings.recall.interval_secs.clamp(10, 30 * 60);
                let service = CaptureService {
                    screen: Arc::new(PikvmScreen::new(client)),
                    extractor,
                    storage,
                    interval: Duration::from_secs(interval),
                };
                Ok::<_, anyhow::Error>(service.start())
            },
            move |r| match r {
                Ok((handle, mut rx)) => {
                    state().services.borrow_mut().recall = Some(handle);
                    state().services_changed();
                    this.refresh_status();
                    let page = this.clone();
                    glib::spawn_future_local(async move {
                        while let Some(ev) = rx.recv().await {
                            match ev {
                                RecallEvent::Captured { day, .. } => {
                                    page.load_days();
                                    if page.imp().selected_day.borrow().as_deref() == Some(day.as_str()) {
                                        page.show_day(Some(day));
                                    }
                                }
                                RecallEvent::Error(e) => tracing::warn!("recall capture: {e}"),
                                _ => {}
                            }
                        }
                        page.refresh_status();
                    });
                }
                Err(e) => {
                    this.imp().want_running.set(false);
                    toast_error("Recall", &e);
                }
            },
        );
    }

    fn stop(&self) {
        if let Some(h) = state().services.borrow_mut().recall.take() {
            h.stop();
        }
        state().services_changed();
        self.refresh_status();
    }

    /// Summarise one day, or every pending day when `day` is `None`.
    fn run_summaries(&self, day: Option<String>) {
        let this = self.clone();
        toast("Summarising…");
        let inputs = Self::summariser_inputs();
        spawn_then(
            async move {
                let s = Self::build_summariser(inputs)
                    .await
                    .ok_or_else(|| anyhow::anyhow!("no text provider configured (Preferences → AI)"))?;
                match day {
                    Some(d) => s.summarise_day(&d).await.map(|_| vec![d]),
                    None => s.run_pending().await,
                }
            },
            move |r| match r {
                Ok(days) if days.is_empty() => toast("Nothing to summarise"),
                Ok(days) => {
                    toast(&format!("Summarised {}", days.join(", ")));
                    this.load_days();
                    let sel = this.imp().selected_day.borrow().clone();
                    this.show_day(sel);
                }
                Err(e) => toast_error("Summary", &e),
            },
        );
    }

    fn load_days(&self) {
        let imp = self.imp();
        let selected = imp.selected_day.borrow().clone();
        for (_, row) in imp.day_rows.borrow_mut().drain(..) {
            imp.days.remove(&row);
        }
        let days: Vec<DayOverview> = state().storage.days().unwrap_or_default();
        for d in &days {
            let row = gtk::ListBoxRow::new();
            let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
            b.set_margin_top(6);
            b.set_margin_bottom(6);
            b.set_margin_start(6);
            let title = gtk::Label::new(Some(if d.day == today() { "Today" } else { &d.day }));
            title.set_xalign(0.0);
            let sub = gtk::Label::new(Some(&format!(
                "{} capture(s) · {}",
                d.capture_count,
                if d.has_summary { "summary ✓" } else { "no summary" }
            )));
            sub.set_xalign(0.0);
            sub.add_css_class("dim-label");
            sub.add_css_class("caption");
            b.append(&title);
            b.append(&sub);
            row.set_child(Some(&b));
            imp.days.append(&row);
            imp.day_rows.borrow_mut().push((d.day.clone(), row.clone()));
            if selected.as_deref() == Some(d.day.as_str()) {
                imp.days.select_row(Some(&row));
            }
        }
    }

    fn show_day(&self, day: Option<String>) {
        let imp = self.imp();
        *imp.selected_day.borrow_mut() = day.clone();
        let Some(day) = day else {
            imp.content_stack.set_visible_child_name("empty");
            return;
        };
        imp.content_stack.set_visible_child_name("day");
        imp.day_title.set_text(&day);
        let storage = &state().storage;
        match storage.summary_for_day(&day) {
            Ok(Some(s)) => {
                imp.summary_view.buffer().set_text(&s.summary);
                imp.summary_meta.set_text(&format!(
                    "Generated {} by {} from {} captures",
                    util::format_ts(s.created_ts),
                    s.model,
                    s.capture_count
                ));
            }
            _ => {
                imp.summary_view.buffer().set_text(
                    "No summary yet. Past days are summarised automatically on startup; press Summarise to run it now.",
                );
                imp.summary_meta.set_text("");
            }
        }
        for w in imp.timeline_rows.borrow_mut().drain(..) {
            imp.timeline.remove(&w);
        }
        let captures = storage.captures_for_day(&day).unwrap_or_default();
        for c in captures.iter().rev() {
            let preview: String = c
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .chars()
                .take(140)
                .collect();
            let row = adw::ActionRow::builder()
                .title(format!("{} · {}", format_time(c.ts), c.source))
                .subtitle(glib::markup_escape_text(&preview))
                .activatable(true)
                .build();
            let text = c.text.clone();
            let title = format!("{} {}", day, format_time(c.ts));
            row.connect_activated(move |r| crate::ui::control::show_text_dialog(r, &title, &text));
            imp.timeline.append(&row);
            imp.timeline_rows.borrow_mut().push(row.upcast());
        }
        if captures.is_empty() {
            let row = adw::ActionRow::builder()
                .title("No raw captures (pruned or none recorded)")
                .build();
            imp.timeline.append(&row);
            imp.timeline_rows.borrow_mut().push(row.upcast());
        }
    }

    fn export(&self, parent: &impl IsA<gtk::Widget>, markdown: bool) {
        let Some(day) = self.imp().selected_day.borrow().clone() else {
            return;
        };
        let content = match export::export_day(&state().storage, &day, markdown) {
            Ok(c) => c,
            Err(e) => {
                toast_error("Export", &e);
                return;
            }
        };
        let dialog = gtk::FileDialog::builder()
            .title("Export day")
            .initial_name(format!("primvokon-{day}.{}", if markdown { "md" } else { "json" }))
            .build();
        let window = parent.root().and_downcast::<gtk::Window>();
        dialog.save(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    match std::fs::write(&path, content) {
                        Ok(()) => toast(&format!("Exported to {}", path.display())),
                        Err(e) => toast_error("Export", &e),
                    }
                }
            }
        });
    }
}
