//! Agent page: task composer, transcript with approval cards, session history (USP 3).

use std::cell::RefCell;
use std::sync::Arc;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::glib;
use primvokon_core::agent::run::{start, AgentConfig};
use primvokon_core::agent::{AgentEvent, ApprovalRequest, Decision, ToolExecutor, ToolKind};
use primvokon_core::screen::PikvmScreen;
use primvokon_core::settings::AgentMode;
use primvokon_core::storage::AgentSession;

use crate::state::{state, AppState};
use crate::util::{self, format_ts, spawn_then, toast, toast_error};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct AgentPage {
        pub transcript: gtk::Box,
        pub transcript_scroll: gtk::ScrolledWindow,
        pub task: gtk::TextView,
        pub playbook: gtk::DropDown,
        pub mode: gtk::DropDown,
        pub run: gtk::Button,
        pub stop: gtk::Button,
        pub usage: gtk::Label,
        pub sessions: gtk::ListBox,
        pub session_rows: RefCell<Vec<(String, gtk::ListBoxRow)>>,
        pub playbook_ids: RefCell<Vec<Option<String>>>,
        pub pending: RefCell<Vec<gtk::Widget>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for AgentPage {
        const NAME: &'static str = "PvAgentPage";
        type Type = super::AgentPage;
        type ParentType = adw::Bin;
    }

    impl ObjectImpl for AgentPage {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup();
        }
    }
    impl WidgetImpl for AgentPage {}
    impl BinImpl for AgentPage {}
}

glib::wrapper! {
    pub struct AgentPage(ObjectSubclass<imp::AgentPage>)
        @extends adw::Bin, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for AgentPage {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl AgentPage {
    fn setup(&self) {
        let imp = self.imp();
        let split = adw::OverlaySplitView::builder()
            .sidebar_width_fraction(0.25)
            .min_sidebar_width(200.0)
            .build();

        // Sidebar: sessions.
        let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let sb_title = gtk::Label::new(Some("Sessions"));
        sb_title.add_css_class("heading");
        sb_title.set_margin_top(12);
        sidebar.append(&sb_title);
        imp.sessions.add_css_class("navigation-sidebar");
        let sessions_scroll = gtk::ScrolledWindow::builder()
            .child(&imp.sessions)
            .vexpand(true)
            .build();
        sidebar.append(&sessions_scroll);
        let new_session = util::button("New task");
        new_session.set_margin_start(12);
        new_session.set_margin_end(12);
        new_session.set_margin_bottom(12);
        sidebar.append(&new_session);
        split.set_sidebar(Some(&sidebar));

        // Content: transcript + composer.
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        imp.transcript.set_orientation(gtk::Orientation::Vertical);
        imp.transcript.set_spacing(8);
        imp.transcript.set_margin_top(12);
        imp.transcript.set_margin_bottom(12);
        imp.transcript.set_margin_start(12);
        imp.transcript.set_margin_end(12);
        let clamp = adw::Clamp::builder().maximum_size(860).child(&imp.transcript).build();
        imp.transcript_scroll.set_child(Some(&clamp));
        imp.transcript_scroll.set_vexpand(true);
        content.append(&imp.transcript_scroll);

        let composer = gtk::Box::new(gtk::Orientation::Vertical, 6);
        composer.add_css_class("toolbar");
        composer.set_margin_top(6);
        composer.set_margin_bottom(6);
        composer.set_margin_start(12);
        composer.set_margin_end(12);
        imp.task.set_wrap_mode(gtk::WrapMode::WordChar);
        imp.task.set_accepts_tab(false);
        imp.task.set_top_margin(6);
        imp.task.set_bottom_margin(6);
        imp.task.set_left_margin(8);
        imp.task.set_right_margin(8);
        let task_scroll = gtk::ScrolledWindow::builder()
            .child(&imp.task)
            .min_content_height(72)
            .max_content_height(180)
            .propagate_natural_height(true)
            .build();
        task_scroll.add_css_class("card");
        composer.append(&task_scroll);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        controls.append(&gtk::Label::new(Some("Playbook")));
        controls.append(&imp.playbook);
        controls.append(&gtk::Label::new(Some("Mode")));
        imp.mode
            .set_model(Some(&gtk::StringList::new(&["Ask before acting", "Auto (announce)"])));
        controls.append(&imp.mode);
        imp.usage.add_css_class("dim-label");
        imp.usage.set_hexpand(true);
        imp.usage.set_xalign(1.0);
        controls.append(&imp.usage);
        imp.stop.set_label("Stop");
        imp.stop.add_css_class("destructive-action");
        imp.stop.set_sensitive(false);
        controls.append(&imp.stop);
        imp.run.set_label("Run task");
        imp.run.add_css_class("suggested-action");
        controls.append(&imp.run);
        composer.append(&controls);
        content.append(&composer);
        split.set_content(Some(&content));
        self.set_child(Some(&split));

        // Wiring.
        let this = self.clone();
        imp.run.connect_clicked(move |_| this.run_task());
        imp.stop.connect_clicked(|_| {
            if let Some(run) = state().services.borrow().agent.as_ref() {
                run.stop();
            }
        });
        imp.mode.connect_selected_notify(|d| {
            let mode = if d.selected() == 1 {
                AgentMode::Auto
            } else {
                AgentMode::Ask
            };
            if state().settings.borrow().agent.mode != mode {
                state().update_settings(|s| s.agent.mode = mode);
            }
        });
        let this = self.clone();
        new_session.connect_clicked(move |_| {
            this.clear_transcript();
            this.imp().sessions.unselect_all();
        });
        let this = self.clone();
        imp.sessions.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let id = this
                .imp()
                .session_rows
                .borrow()
                .iter()
                .find(|(_, r)| r == row)
                .map(|(id, _)| id.clone());
            if let Some(id) = id {
                this.show_session(&id);
            }
        });
        let this = self.clone();
        state()
            .bus
            .connect_signal("settings-changed", move || this.sync_settings());

        self.sync_settings();
        self.load_sessions();
        self.append_hint();
    }

    fn sync_settings(&self) {
        let imp = self.imp();
        let s = state().settings();
        imp.mode.set_selected(match s.agent.mode {
            AgentMode::Ask => 0,
            AgentMode::Auto => 1,
        });
        let mut names = vec!["No playbook".to_string()];
        let mut ids = vec![None];
        for pb in &s.agent.playbooks {
            names.push(pb.name.clone());
            ids.push(Some(pb.id.clone()));
        }
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let selected = imp.playbook.selected();
        imp.playbook.set_model(Some(&gtk::StringList::new(&refs)));
        let keep = if selected == gtk::INVALID_LIST_POSITION {
            0
        } else {
            selected
        };
        imp.playbook.set_selected(keep.min(refs.len() as u32 - 1));
        *imp.playbook_ids.borrow_mut() = ids;
    }

    fn append_hint(&self) {
        let hint = util::status_page(
            "user-available-symbolic",
            "Agent mode",
            "Describe a task such as “Reply to the Teams message from Anna that I’ll be 10 minutes late”. \
In ask mode every action is shown for approval first.",
        );
        hint.set_vexpand(false);
        self.imp().transcript.append(&hint);
    }

    fn clear_transcript(&self) {
        let imp = self.imp();
        while let Some(child) = imp.transcript.first_child() {
            imp.transcript.remove(&child);
        }
        imp.pending.borrow_mut().clear();
        imp.usage.set_text("");
    }

    fn append(&self, widget: &impl IsA<gtk::Widget>) {
        let imp = self.imp();
        imp.transcript.append(widget);
        let scroll = imp.transcript_scroll.clone();
        glib::idle_add_local_once(move || {
            let adj = scroll.vadjustment();
            adj.set_value(adj.upper() - adj.page_size());
        });
    }

    fn bubble(&self, text: &str, class: &str) {
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_xalign(0.0);
        label.set_selectable(true);
        label.add_css_class(class);
        self.append(&label);
    }

    fn is_running(&self) -> bool {
        state().services.borrow().agent.as_ref().is_some_and(|a| a.is_running())
    }

    fn run_task(&self) {
        if self.is_running() {
            toast("A task is already running");
            return;
        }
        let imp = self.imp();
        let buffer = imp.task.buffer();
        let task = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .trim()
            .to_string();
        if task.is_empty() {
            toast("Describe the task first");
            return;
        }
        let st = state();
        let Some(client) = st.connection.client() else {
            toast("Connect to a PiKVM first");
            return;
        };
        let Some(secrets) = st.secrets() else {
            toast("Secret store not ready yet");
            return;
        };
        let settings = st.settings();
        if !settings.capabilities.agent {
            toast("Enable agent mode in Preferences → Capabilities");
            return;
        }
        let playbook_id = imp
            .playbook_ids
            .borrow()
            .get(imp.playbook.selected() as usize)
            .cloned()
            .flatten();
        let playbook = playbook_id.and_then(|id| settings.agent.playbooks.iter().find(|p| p.id == id).cloned());
        let storage = st.storage.clone();
        let session_id = uuid::Uuid::new_v4().to_string();
        let title: String = task.chars().take(60).collect();
        let mode = match settings.agent.mode {
            AgentMode::Ask => "ask",
            AgentMode::Auto => "auto",
        };
        if let Err(e) = storage.create_agent_session(&session_id, &title, mode) {
            toast_error("Session", &e);
        }
        self.clear_transcript();
        self.bubble(&task, "chat-user");
        imp.run.set_sensitive(false);
        imp.stop.set_sensitive(true);
        buffer.set_text("");

        let this = self.clone();
        let task_clone = task.clone();
        spawn_then(
            async move {
                let provider = AppState::provider(settings.ai.text_provider, &settings, &secrets).await?;
                let vision = AppState::vision_provider(&settings, &secrets).await;
                let executor = Arc::new(ToolExecutor {
                    client: client.clone(),
                    screen: Arc::new(PikvmScreen::new(client)),
                    vision,
                    ocr_langs: settings.recall.ocr_langs.clone(),
                    keymap: None,
                });
                Ok::<_, anyhow::Error>(AgentConfig {
                    provider,
                    executor,
                    settings: settings.agent.clone(),
                    playbook,
                    task: task_clone,
                    storage: Some(storage),
                    session_id,
                })
            },
            move |r| match r {
                Ok(cfg) => {
                    let (run, mut rx) = {
                        let _guard = primvokon_core::runtime::enter();
                        start(cfg)
                    };
                    state().services.borrow_mut().agent = Some(run);
                    state().services_changed();
                    this.load_sessions();
                    let page = this.clone();
                    glib::spawn_future_local(async move {
                        while let Some(ev) = rx.recv().await {
                            page.on_event(ev);
                        }
                        page.finish_run();
                    });
                }
                Err(e) => {
                    toast_error("Agent", &e);
                    this.finish_run();
                }
            },
        );
    }

    fn finish_run(&self) {
        let imp = self.imp();
        imp.run.set_sensitive(true);
        imp.stop.set_sensitive(false);
        state().services.borrow_mut().agent = None;
        state().services_changed();
    }

    fn on_event(&self, ev: AgentEvent) {
        match ev {
            AgentEvent::Text(t) => {
                let class = if t.trim_start().starts_with("Next steps") {
                    "chat-tool"
                } else {
                    "chat-assistant"
                };
                self.bubble(&t, class);
            }
            AgentEvent::ToolCall { tool, summary, .. } => {
                self.bubble(&format!("▶ {} — {}", tool.label(), summary), "chat-tool");
            }
            AgentEvent::ToolResult {
                tool, output, is_error, ..
            } => {
                let preview: String = output.chars().take(2000).collect();
                let expander = gtk::Expander::builder()
                    .label(format!("{}{}", if is_error { "✖ " } else { "✔ " }, tool.label()))
                    .build();
                let label = gtk::Label::new(Some(&preview));
                label.set_wrap(true);
                label.set_xalign(0.0);
                label.set_selectable(true);
                label.add_css_class("mono");
                expander.set_child(Some(&label));
                expander.add_css_class(if is_error { "chat-error" } else { "chat-tool" });
                self.append(&expander);
            }
            AgentEvent::AwaitingApproval(req) => self.show_approval(req),
            AgentEvent::Usage(u) => self
                .imp()
                .usage
                .set_text(&format!("{} in · {} out tokens", u.input_tokens, u.output_tokens)),
            AgentEvent::Finished(report) => self.bubble(&format!("✅ Finished: {report}"), "chat-assistant"),
            AgentEvent::Stopped => self.bubble("⏹ Stopped", "chat-error"),
            AgentEvent::Error(e) => self.bubble(&format!("⚠ {e}"), "chat-error"),
        }
    }

    fn show_approval(&self, req: ApprovalRequest) {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("approval-card");
        let title = gtk::Label::new(Some(&format!("Approve: {}", req.summary)));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        title.set_wrap(true);
        card.append(&title);
        let hint = gtk::Label::new(Some("You can edit the tool input before approving."));
        hint.add_css_class("dim-label");
        hint.set_xalign(0.0);
        card.append(&hint);
        let editor = gtk::TextView::builder()
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .build();
        editor
            .buffer()
            .set_text(&serde_json::to_string_pretty(&req.input).unwrap_or_default());
        editor.set_top_margin(6);
        editor.set_bottom_margin(6);
        editor.set_left_margin(6);
        editor.set_right_margin(6);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&editor)
            .min_content_height(80)
            .max_content_height(240)
            .propagate_natural_height(true)
            .build();
        scroll.add_css_class("card");
        card.append(&scroll);
        let reason = gtk::Entry::builder()
            .placeholder_text("Reason when rejecting (optional)")
            .build();
        card.append(&reason);
        let approve = util::suggested_button("Approve");
        let reject = util::destructive_button("Reject");
        let buttons = util::button_box(&[&reject, &approve]);
        buttons.set_halign(gtk::Align::End);
        card.append(&buttons);
        self.append(&card);

        let reply = RefCell::new(Some(req.reply));
        let reply = std::rc::Rc::new(reply);
        let original = req.input.clone();
        let tool: ToolKind = req.tool;
        let (r1, c1, e1) = (reply.clone(), card.clone(), editor.clone());
        approve.connect_clicked(move |_| {
            let buffer = e1.buffer();
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
            let input = serde_json::from_str(&text).unwrap_or_else(|_| original.clone());
            if let Some(tx) = r1.borrow_mut().take() {
                let _ = tx.send(Decision::Approve(input));
            }
            settle(&c1, &format!("✔ Approved {}", tool.label()));
        });
        let (r2, c2, re) = (reply, card.clone(), reason.clone());
        reject.connect_clicked(move |_| {
            let why = re.text().to_string();
            if let Some(tx) = r2.borrow_mut().take() {
                let _ = tx.send(Decision::Reject(if why.is_empty() {
                    "the user declined".into()
                } else {
                    why
                }));
            }
            settle(&c2, &format!("✖ Rejected {}", tool.label()));
        });
    }

    fn load_sessions(&self) {
        let imp = self.imp();
        for (_, row) in imp.session_rows.borrow_mut().drain(..) {
            imp.sessions.remove(&row);
        }
        let sessions: Vec<AgentSession> = state().storage.agent_sessions(50).unwrap_or_default();
        for s in &sessions {
            let row = gtk::ListBoxRow::new();
            let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
            b.set_margin_top(6);
            b.set_margin_bottom(6);
            b.set_margin_start(6);
            let title = gtk::Label::new(Some(&s.title));
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let sub = gtk::Label::new(Some(&format!("{} · {}", format_ts(s.created_ts), s.mode)));
            sub.set_xalign(0.0);
            sub.add_css_class("dim-label");
            sub.add_css_class("caption");
            b.append(&title);
            b.append(&sub);
            row.set_child(Some(&b));
            imp.sessions.append(&row);
            imp.session_rows.borrow_mut().push((s.id.clone(), row));
        }
    }

    /// Render a stored transcript (read-only).
    fn show_session(&self, id: &str) {
        if self.is_running() {
            return;
        }
        self.clear_transcript();
        let messages = state().storage.agent_messages(id).unwrap_or_default();
        for m in messages {
            let v: serde_json::Value = serde_json::from_str(&m.content).unwrap_or_default();
            match m.kind.as_str() {
                "user" => self.bubble(v["task"].as_str().unwrap_or_default(), "chat-user"),
                "assistant" => self.bubble(v["text"].as_str().unwrap_or_default(), "chat-assistant"),
                "tool_call" => self.bubble(&format!("▶ {}", v["summary"].as_str().unwrap_or_default()), "chat-tool"),
                "tool_result" => {
                    let is_error = v["is_error"].as_bool().unwrap_or(false);
                    let out: String = v["output"].as_str().unwrap_or_default().chars().take(600).collect();
                    self.bubble(
                        &format!("{} {}", if is_error { "✖" } else { "✔" }, out),
                        if is_error { "chat-error" } else { "chat-tool" },
                    );
                }
                "finished" => self.bubble(
                    &format!("✅ {}", v["report"].as_str().unwrap_or_default()),
                    "chat-assistant",
                ),
                "error" => self.bubble(&format!("⚠ {}", v["error"].as_str().unwrap_or_default()), "chat-error"),
                "stopped" => self.bubble("⏹ Stopped", "chat-error"),
                _ => {}
            }
        }
    }
}

/// Replace an approval card's contents with a one-line outcome.
fn settle(card: &gtk::Box, text: &str) {
    while let Some(child) = card.first_child() {
        card.remove(&child);
    }
    card.remove_css_class("approval-card");
    card.add_css_class("chat-tool");
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    card.append(&label);
}
