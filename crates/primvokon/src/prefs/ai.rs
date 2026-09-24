//! AI provider settings: Anthropic, OpenAI, OpenRouter.

use adw::prelude::*;
use primvokon_core::secrets::set_or_delete;
use primvokon_core::settings::ProviderKind;

use super::{bind_combo, bind_entry, bind_switch, page as new_page};
use crate::state::{state, AppState};
use crate::util::{self, spawn_then, toast, toast_error};

pub fn page() -> adw::PreferencesPage {
    let page = new_page("AI", "user-available-symbolic");
    let s = state().settings();
    let labels: Vec<&str> = ProviderKind::ALL.iter().map(|k| k.label()).collect();

    let g = util::card(
        "Active providers",
        Some("Text is used by the agent and daily summaries, vision by screen reading and the AI change detector."),
    );
    let text = util::combo_row(&g, "Text provider", None, &labels, kind_index(s.ai.text_provider));
    bind_combo(&text, |s, i| s.ai.text_provider = ProviderKind::ALL[i as usize]);
    let vision = util::combo_row(&g, "Vision provider", None, &labels, kind_index(s.ai.vision_provider));
    bind_combo(&vision, |s, i| s.ai.vision_provider = ProviderKind::ALL[i as usize]);
    page.add(&g);

    for kind in ProviderKind::ALL {
        let cfg = s.ai.provider(kind);
        let g = util::card(kind.label(), None);
        let key = util::password_row(&g, "API key", "");
        if let Some(secrets) = state().secrets() {
            let k = key.clone();
            let name = kind.secret_key();
            spawn_then(async move { secrets.get(&name).await.ok().flatten() }, move |v| {
                k.set_text(&v.unwrap_or_default())
            });
        }
        key.set_show_apply_button(true);
        key.connect_apply(move |r| {
            let Some(secrets) = state().secrets() else {
                toast("Secret store not ready");
                return;
            };
            let value = r.text().to_string();
            let name = kind.secret_key();
            spawn_then(
                async move { set_or_delete(secrets.as_ref(), &name, &value).await },
                |r| match r {
                    Ok(()) => toast("API key saved"),
                    Err(e) => toast_error("API key", &e),
                },
            );
        });
        let model = util::entry_row(&g, &format!("Model (default {})", kind.default_model()), &cfg.model);
        bind_entry(&model, move |s, v| s.ai.providers.entry(kind).or_default().model = v);
        let base = util::entry_row(
            &g,
            &format!("Base URL (default {})", kind.default_base_url()),
            &cfg.base_url,
        );
        bind_entry(&base, move |s, v| s.ai.providers.entry(kind).or_default().base_url = v);
        if kind == ProviderKind::Anthropic {
            let fb = util::switch_row(
                &g,
                "Refusal fallbacks",
                Some("Let the API re-run a declined request on a fallback model (server-side-fallback beta)"),
                cfg.refusal_fallbacks,
            );
            bind_switch(&fb, move |s, v| {
                s.ai.providers.entry(kind).or_default().refusal_fallbacks = v
            });
        }
        let test = util::button("Test provider");
        let result = util::button_row(&g, "Connectivity", Some("Sends a one-word request"), &test);
        let result_row = result.clone();
        test.connect_clicked(move |_| {
            let Some(secrets) = state().secrets() else { return };
            let settings = state().settings();
            let row = result_row.clone();
            row.set_subtitle("Testing…");
            spawn_then(
                async move {
                    let p = AppState::provider(kind, &settings, &secrets).await?;
                    p.ping().await
                },
                move |r| match r {
                    Ok(msg) => row.set_subtitle(&format!("✅ {msg}")),
                    Err(e) => row.set_subtitle(&format!("❌ {e}")),
                },
            );
        });
        page.add(&g);
    }
    page
}

fn kind_index(kind: ProviderKind) -> u32 {
    ProviderKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u32
}
