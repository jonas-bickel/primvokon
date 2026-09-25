//! Watcher settings.

use adw::prelude::*;
use primvokon_core::settings::{DetectorKind, Region};

use super::{bind_combo, bind_entry, bind_spin, page as new_page, split_list};
use crate::state::state;
use crate::util;

pub fn page() -> adw::PreferencesPage {
    let page = new_page("Watcher", "view-reveal-symbolic");
    let s = state().settings();
    let w = &s.watcher;

    let g = util::card("Sampling", None);
    let interval = util::spin_row(
        &g,
        "Interval (s)",
        Some("How often the screen is sampled"),
        1.0,
        600.0,
        1.0,
        w.interval_secs as f64,
    );
    bind_spin(&interval, |s, v| s.watcher.interval_secs = v as u64);
    let labels: Vec<&str> = DetectorKind::ALL.iter().map(|d| d.label()).collect();
    let detector = util::combo_row(
        &g,
        "Change detector",
        Some("AI detectors need a vision provider and the AI vision capability"),
        &labels,
        DetectorKind::ALL.iter().position(|d| *d == w.detector).unwrap_or(0) as u32,
    );
    bind_combo(&detector, |s, i| s.watcher.detector = DetectorKind::ALL[i as usize]);
    let langs = util::entry_row(
        &g,
        "OCR languages (comma separated, e.g. eng,deu)",
        &w.ocr_langs.join(","),
    );
    bind_entry(&langs, |s, v| s.watcher.ocr_langs = split_list(&v));
    page.add(&g);

    let t = util::card("Tuning", None);
    let cooldown = util::spin_row(
        &t,
        "Cooldown (s)",
        Some("Minimum time between two notifications"),
        0.0,
        3600.0,
        10.0,
        w.cooldown_secs as f64,
    );
    bind_spin(&cooldown, |s, v| s.watcher.cooldown_secs = v as u64);
    let text_thr = util::spin_row(
        &t,
        "Text change threshold",
        Some("Fraction of new OCR lines (0.01–1)"),
        0.01,
        1.0,
        0.01,
        w.text_threshold,
    );
    text_thr.set_digits(2);
    bind_spin(&text_thr, |s, v| s.watcher.text_threshold = v);
    let pixel_thr = util::spin_row(
        &t,
        "Pixel change threshold",
        Some("Fraction of changed pixels (0.001–1)"),
        0.001,
        1.0,
        0.005,
        w.pixel_threshold,
    );
    pixel_thr.set_digits(3);
    bind_spin(&pixel_thr, |s, v| s.watcher.pixel_threshold = v);
    let ignore = util::entry_row(&t, "Ignore phrases (comma separated)", &w.ignore_phrases.join(","));
    bind_entry(&ignore, |s, v| s.watcher.ignore_phrases = split_list(&v));
    let regions_text = format_regions(&w.regions);
    let regions = util::entry_row(
        &t,
        "Watch regions: left,top,width,height; … (empty = whole screen)",
        &regions_text,
    );
    bind_entry(&regions, |s, v| s.watcher.regions = parse_regions(&v));
    let draw = util::button("Draw on snapshot…");
    {
        let regions = regions.clone();
        draw.connect_clicked(move |b| {
            let current = state().settings().watcher.regions;
            let regions = regions.clone();
            super::regions::open(b, current, move |list| {
                regions.set_text(&format_regions(&list));
                state().update_settings(|s| s.watcher.regions = list);
                util::toast("Watch regions saved");
            });
        });
    }
    util::button_row(
        &t,
        "Region editor",
        Some("Drag rectangles over a live snapshot of the host screen"),
        &draw,
    );
    page.add(&t);
    page
}

pub fn format_regions(regions: &[Region]) -> String {
    regions
        .iter()
        .map(|r| format!("{},{},{},{}", r.left, r.top, r.width, r.height))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn parse_regions(text: &str) -> Vec<Region> {
    text.split(';')
        .filter_map(|part| {
            let nums: Vec<u32> = part.split(',').filter_map(|n| n.trim().parse().ok()).collect();
            (nums.len() == 4).then(|| Region {
                left: nums[0],
                top: nums[1],
                width: nums[2],
                height: nums[3],
            })
        })
        .collect()
}
