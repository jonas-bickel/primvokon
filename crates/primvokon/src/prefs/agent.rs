//! Agent settings: mode, budgets, playbooks, extra system prompt.

use adw::prelude::*;
use primvokon_core::settings::{AgentMode, Playbook};

use super::{bind_combo, bind_entry, bind_spin, page as new_page, text_row};
use crate::state::state;
use crate::util;

pub fn page() -> adw::PreferencesPage {
    let page = new_page("Agent", "user-available-symbolic");
    let s = state().settings();
    let a = &s.agent;

    let g = util::card("Behaviour", None);
    let mode = util::combo_row(
        &g,
        "Mode",
        Some("Ask: approve every side effect · Auto: act autonomously but announce next steps"),
        &["Ask before acting", "Auto (announce only)"],
        match a.mode {
            AgentMode::Ask => 0,
            AgentMode::Auto => 1,
        },
    );
    bind_combo(&mode, |s, i| {
        s.agent.mode = if i == 1 { AgentMode::Auto } else { AgentMode::Ask }
    });
    let steps = util::spin_row(
        &g,
        "Step limit",
        Some("Model turns per task"),
        1.0,
        500.0,
        1.0,
        f64::from(a.step_limit),
    );
    bind_spin(&steps, |s, v| s.agent.step_limit = v as u32);
    let budget = util::spin_row(
        &g,
        "Token budget",
        Some("Input + output tokens per task"),
        1000.0,
        10_000_000.0,
        10_000.0,
        a.token_budget as f64,
    );
    bind_spin(&budget, |s, v| s.agent.token_budget = v as u64);
    text_row(
        &g,
        "Additional system prompt",
        Some("Appended to the built-in instructions"),
        &a.system_prompt,
        |s, v| s.agent.system_prompt = v,
    );
    page.add(&g);

    let p = util::card(
        "Playbooks",
        Some("Prompt sections for recurring tasks such as replying in Teams or Outlook."),
    );
    for (i, pb) in a.playbooks.iter().enumerate() {
        let name = util::entry_row(&p, "Name", &pb.name);
        bind_entry(&name, move |s, v| {
            if let Some(pb) = s.agent.playbooks.get_mut(i) {
                pb.name = v;
            }
        });
        text_row(&p, &format!("Prompt: {}", pb.name), None, &pb.prompt, move |s, v| {
            if let Some(pb) = s.agent.playbooks.get_mut(i) {
                pb.prompt = v;
            }
        });
    }
    let add = util::button("Add playbook");
    add.connect_clicked(|_| {
        state().update_settings(|s| {
            s.agent.playbooks.push(Playbook {
                id: uuid_like(),
                name: "New playbook".into(),
                prompt: String::new(),
            })
        });
        util::toast("Playbook added – reopen preferences to edit it");
    });
    let reset = util::button("Restore defaults");
    reset.connect_clicked(|_| {
        state().update_settings(|s| s.agent.playbooks = primvokon_core::agent::playbooks::defaults());
        util::toast("Default playbooks restored – reopen preferences");
    });
    let row = adw::ActionRow::builder().title("Manage").build();
    row.add_suffix(&util::button_box(&[&add, &reset]));
    p.add(&row);
    page.add(&p);
    page
}

fn uuid_like() -> String {
    format!("pb-{}", chrono::Utc::now().timestamp_millis())
}
