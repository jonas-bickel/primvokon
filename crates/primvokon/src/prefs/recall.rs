//! Recall settings.

use adw::prelude::*;
use primvokon_core::settings::{ExtractStrategy, RecallSettings};

use super::{bind_combo, bind_entry, bind_spin, bind_switch, page as new_page, split_list, text_row};
use crate::state::state;
use crate::util;

pub fn page() -> adw::PreferencesPage {
    let page = new_page("Recall", "document-open-recent-symbolic");
    let s = state().settings();
    let r = &s.recall;

    let g = util::card("Capture", None);
    let interval = util::spin_row(
        &g,
        "Interval (s)",
        Some("Default 60 s; allowed 10 s – 30 min"),
        RecallSettings::MIN_INTERVAL as f64,
        RecallSettings::MAX_INTERVAL as f64,
        10.0,
        r.interval_secs as f64,
    );
    bind_spin(&interval, |s, v| s.recall.interval_secs = v as u64);
    let strategy = util::combo_row(
        &g,
        "Text extraction",
        Some("OCR runs on the PiKVM for free; vision needs a provider"),
        &["PiKVM OCR", "Vision model description"],
        match r.strategy {
            ExtractStrategy::Ocr => 0,
            ExtractStrategy::Vision => 1,
        },
    );
    bind_combo(&strategy, |s, i| {
        s.recall.strategy = if i == 1 {
            ExtractStrategy::Vision
        } else {
            ExtractStrategy::Ocr
        }
    });
    let langs = util::entry_row(&g, "OCR languages (comma separated)", &r.ocr_langs.join(","));
    bind_entry(&langs, |s, v| s.recall.ocr_langs = split_list(&v));
    page.add(&g);

    let sg = util::card("Summaries", None);
    let startup = util::switch_row(
        &sg,
        "Summarise on startup / resume",
        Some("Pending days are summarised when the app starts or the machine wakes"),
        r.summarise_on_startup,
    );
    bind_switch(&startup, |s, v| s.recall.summarise_on_startup = v);
    let retention = util::spin_row(
        &sg,
        "Keep raw captures (days)",
        Some("Raw captures of summarised days older than this are deleted"),
        1.0,
        365.0,
        1.0,
        f64::from(r.retention_days),
    );
    bind_spin(&retention, |s, v| s.recall.retention_days = v as u32);
    let language = util::entry_row(&sg, "Summary language", &r.summary_language);
    bind_entry(&language, |s, v| s.recall.summary_language = v);
    text_row(
        &sg,
        "Style prompt",
        Some("How the daily summary should be written"),
        &r.style_prompt,
        |s, v| s.recall.style_prompt = v,
    );
    page.add(&sg);
    page
}
