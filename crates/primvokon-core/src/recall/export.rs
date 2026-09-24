//! Export one day (summary + raw timeline) as Markdown or JSON.

use serde_json::json;

use crate::storage::{Capture, Storage, Summary};

pub fn day_markdown(day: &str, summary: Option<&Summary>, captures: &[Capture]) -> String {
    let mut out = format!("# {day}\n\n");
    match summary {
        Some(s) => {
            out.push_str("## Summary\n\n");
            out.push_str(&s.summary);
            out.push_str(&format!("\n\n_Model: {}_\n\n", s.model));
        }
        None => out.push_str("_No summary yet._\n\n"),
    }
    out.push_str("## Raw captures\n\n");
    for c in captures {
        let time = chrono::DateTime::from_timestamp(c.ts, 0)
            .map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
            .unwrap_or_default();
        out.push_str(&format!("### {time} ({})\n\n```\n{}\n```\n\n", c.source, c.text.trim()));
    }
    out
}

pub fn day_json(day: &str, summary: Option<&Summary>, captures: &[Capture]) -> String {
    json!({ "day": day, "summary": summary, "captures": captures }).to_string()
}

pub fn export_day(storage: &Storage, day: &str, markdown: bool) -> anyhow::Result<String> {
    let summary = storage.summary_for_day(day)?;
    let captures = storage.captures_for_day(day)?;
    Ok(if markdown {
        day_markdown(day, summary.as_ref(), &captures)
    } else {
        day_json(day, summary.as_ref(), &captures)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_markdown() {
        let md = day_markdown("2026-09-24", None, &[]);
        assert!(md.starts_with("# 2026-09-24"));
        assert!(md.contains("No summary yet"));
    }
}
