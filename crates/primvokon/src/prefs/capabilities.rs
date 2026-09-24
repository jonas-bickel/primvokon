//! Capability switches (F-CAP).

use adw::prelude::*;
use primvokon_core::agent::ToolKind;

use super::{bind_switch, page as new_page};
use crate::state::state;
use crate::util;

pub fn page() -> adw::PreferencesPage {
    let page = new_page("Capabilities", "emblem-ok-symbolic");
    let s = state().settings();

    let g = util::card(
        "Optional capabilities",
        Some("Disabled capabilities are hidden from the UI and their background loops stop."),
    );
    let watcher = util::switch_row(
        &g,
        "Screen watcher",
        Some("Detect changes on the host screen and notify via ntfy"),
        s.capabilities.watcher,
    );
    bind_switch(&watcher, |s, v| s.capabilities.watcher = v);
    let recall = util::switch_row(
        &g,
        "Screen recall",
        Some("Capture the screen as text and summarise every day"),
        s.capabilities.recall,
    );
    bind_switch(&recall, |s, v| s.capabilities.recall = v);
    let agent = util::switch_row(
        &g,
        "Agent mode",
        Some("Let an AI operate the host through PiKVM"),
        s.capabilities.agent,
    );
    bind_switch(&agent, |s, v| s.capabilities.agent = v);
    let ntfy = util::switch_row(
        &g,
        "ntfy notifications",
        Some("Push notifications from the watcher"),
        s.capabilities.ntfy,
    );
    bind_switch(&ntfy, |s, v| s.capabilities.ntfy = v);
    let vision = util::switch_row(
        &g,
        "AI vision",
        Some("Allow sending screenshots to the vision provider"),
        s.capabilities.ai_vision,
    );
    bind_switch(&vision, |s, v| s.capabilities.ai_vision = v);
    page.add(&g);

    let p = util::card("Privacy", None);
    let never = util::switch_row(
        &p,
        "Never send screen content to an AI provider",
        Some("Forces OCR-only strategies everywhere; the agent still works but cannot see screenshots"),
        s.capabilities.never_send_screen_to_ai,
    );
    bind_switch(&never, |s, v| s.capabilities.never_send_screen_to_ai = v);
    page.add(&p);

    let t = util::card(
        "Agent tools",
        Some("Each tool can be switched off individually; disabled tools are not offered to the model."),
    );
    for tool in ToolKind::ALL {
        let row = util::switch_row(
            &t,
            tool.label(),
            Some(tool.description()),
            s.agent.tool_enabled(tool.name()),
        );
        let name = tool.name();
        bind_switch(&row, move |s, v| {
            s.agent.tools.insert(name.to_string(), v);
        });
    }
    page.add(&t);
    page
}
