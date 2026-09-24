//! System information card (`/api/info`).

use adw::prelude::*;
use pikvm::events::KvmState;

use super::Card;
use crate::util::{self, human_bytes};

pub struct SystemCard {
    group: adw::PreferencesGroup,
    cpu: adw::ActionRow,
    mem: adw::ActionRow,
    temp: adw::ActionRow,
    throttle: adw::ActionRow,
    platform: adw::ActionRow,
    system: adw::ActionRow,
    kernel: adw::ActionRow,
    meta: adw::ActionRow,
    extras: adw::ActionRow,
    fan: adw::ActionRow,
    auth: adw::ActionRow,
    advanced: Vec<adw::ActionRow>,
}

impl SystemCard {
    pub fn new() -> Self {
        let group = util::card("System", Some("Health and platform information of the PiKVM"));
        let cpu = util::kv_row(&group, "CPU");
        let mem = util::kv_row(&group, "Memory");
        let temp = util::kv_row(&group, "Temperature");
        let throttle = util::kv_row(&group, "Throttling");
        let platform = util::kv_row(&group, "Platform");
        let system = util::kv_row(&group, "Software");
        let kernel = util::kv_row(&group, "Kernel");
        let meta = util::kv_row(&group, "Meta");
        let extras = util::kv_row(&group, "Extras");
        let fan = util::kv_row(&group, "Fan");
        let auth = util::kv_row(&group, "Auth");
        let advanced = vec![
            platform.clone(),
            system.clone(),
            kernel.clone(),
            meta.clone(),
            extras.clone(),
            fan.clone(),
            auth.clone(),
        ];
        Self {
            group,
            cpu,
            mem,
            temp,
            throttle,
            platform,
            system,
            kernel,
            meta,
            extras,
            fan,
            auth,
            advanced,
        }
    }
}

impl Card for SystemCard {
    fn group(&self) -> &adw::PreferencesGroup {
        &self.group
    }

    fn update(&self, s: &KvmState, connected: bool) {
        self.group.set_visible(connected);
        if let Some(hw) = &s.info_hw {
            let h = &hw.health;
            self.cpu.set_subtitle(&format!("{:.0}%", h.cpu.percent));
            self.mem.set_subtitle(&format!(
                "{:.0}% used · {} available of {}",
                h.mem.percent,
                human_bytes(h.mem.available),
                human_bytes(h.mem.total)
            ));
            self.temp.set_subtitle(
                &h.temp
                    .iter()
                    .map(|(k, v)| format!("{k} {v:.1} °C"))
                    .collect::<Vec<_>>()
                    .join(" · "),
            );
            let flags: Vec<String> = h
                .throttling
                .parsed_flags
                .iter()
                .filter(|(_, f)| f.now || f.past)
                .map(|(k, f)| format!("{k}{}", if f.now { " (now)" } else { " (past)" }))
                .collect();
            let joined = flags.join(", ");
            self.throttle
                .set_subtitle(if flags.is_empty() { "none" } else { &joined });
            let p = &hw.platform;
            self.platform.set_subtitle(&format!(
                "{} · board {} · model {} · {} video · serial {}",
                p.base, p.board, p.model, p.video, p.serial
            ));
        }
        if let Some(sys) = &s.info_system {
            self.system.set_subtitle(&format!(
                "kvmd {} · {} {}",
                sys.kvmd.version, sys.streamer.app, sys.streamer.version
            ));
            self.kernel.set_subtitle(&format!(
                "{} {} ({})",
                sys.kernel.system, sys.kernel.release, sys.kernel.machine
            ));
        }
        self.meta.set_subtitle(&compact_json(&s.info_meta));
        self.extras.set_subtitle(&match s.info_extras.as_object() {
            Some(o) => o
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{k}{}",
                        if v["enabled"].as_bool().unwrap_or(true) {
                            ""
                        } else {
                            " (disabled)"
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
            None => "—".into(),
        });
        self.fan.set_subtitle(&compact_json(&s.info_fan));
        self.auth.set_subtitle(&compact_json(&s.info_auth));
    }

    fn set_advanced(&self, advanced: bool) {
        for r in &self.advanced {
            r.set_visible(advanced);
        }
    }
}

fn compact_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Null => "—".into(),
        serde_json::Value::Object(o) if o.is_empty() => "—".into(),
        other => {
            let s = other.to_string();
            if s.len() > 200 {
                format!("{}…", &s[..200])
            } else {
                s
            }
        }
    }
}
