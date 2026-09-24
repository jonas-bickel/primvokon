# PRIMVOKON – Task Tracker

Status legend: `[ ]` todo · `[~]` in progress · `[x]` done · `[-]` dropped/deferred.
Tasks reference stories (`STORIES.md`) and features (`FEATURES.md`).

## Milestone 0 – Product definition
- [x] T-000 Feature catalogue (`docs/FEATURES.md`)
- [x] T-001 User stories with acceptance criteria (`docs/STORIES.md`)
- [x] T-002 API coverage matrix (`docs/API-COVERAGE.md`)
- [x] T-003 Architecture notes (`docs/ARCHITECTURE.md`)

## Milestone 1 – Foundation (H1)
- [ ] T-100 Cargo workspace: `crates/pikvm`, `crates/primvokon-core`, `crates/primvokon`
- [ ] T-101 Blueprint → GResource build pipeline (`build.rs`)
- [ ] T-102 Tokio runtime bridge for GTK (`runtime::spawn`, `glib::spawn_future_local`)
- [ ] T-103 Settings model (TOML in `$XDG_CONFIG_HOME/primvokon/settings.toml`)
- [ ] T-104 `SecretStore` trait: Secret Service + file fallback (A3)
- [ ] T-105 SQLite storage (`recall.db`) with migrations
- [ ] T-106 README, desktop file, icon, Makefile

## Milestone 2 – PiKVM client (A, C)
- [ ] T-200 Auth strategies + TOTP + request builder (A1, A4) with unit tests
- [ ] T-201 System: info (fields), log (seek/follow)
- [ ] T-202 HID: state, set_params, set_connected, reset, keymaps, print, send_shortcut, send_key, mouse button/move/relative/wheel
- [ ] T-203 ATX: state, power, click
- [ ] T-204 MSD: state, write (stream upload), write_remote, set_params, set_connected, remove, reset
- [ ] T-205 GPIO: state, switch, pulse
- [ ] T-206 Streamer: state, snapshot (jpeg/ocr/preview), delete snapshot, ocr state, set_params
- [ ] T-207 Switch: state, set_active_prev/next/set, beacon, port params, colours, reset, edids create/change/remove, atx power/click
- [ ] T-208 Redfish: root, systems, system, patch, reset
- [ ] T-209 Prometheus metrics; auth login/check/logout
- [ ] T-210 Websocket client: event parsing, key/mouse events, ping, reconnect with backoff (B4)
- [ ] T-211 MJPEG stream reader (multipart parser) + snapshot poller (B1)
- [ ] T-212 Endpoint catalogue (category, importance, params) for the explorer (C10)

## Milestone 3 – Application shell & Console (A, B, D)
- [ ] T-300 Application, main window, view switcher, breakpoint (D1)
- [ ] T-301 Connection preferences page + test connection (A1, A2)
- [ ] T-302 Profile switcher & auto-connect (A5)
- [ ] T-303 Console page: video widget, status strip, fallback (B1)
- [ ] T-304 Keyboard/mouse capture → websocket (B2)
- [ ] T-305 Quick actions, paste-text dialog, fullscreen (B3)
- [ ] T-306 Toasts, confirmations, status pages (D2)
- [ ] T-307 Shortcuts window, about dialog (D3)

## Milestone 4 – Control Centre (C)
- [ ] T-400 KVM state model (GObject) fed by websocket events
- [ ] T-401 ATX card (C1)
- [ ] T-402 HID card (C2)
- [ ] T-403 MSD card incl. uploads (C3)
- [ ] T-404 GPIO card from view model (C4)
- [ ] T-405 Streamer/OCR card (C5)
- [ ] T-406 Switch card (C6)
- [ ] T-407 System info card (C7)
- [ ] T-408 Log viewer (C8)
- [ ] T-409 API Explorer incl. Redfish/Prometheus (C9, C10)

## Milestone 5 – Watcher + ntfy (E)
- [ ] T-500 ntfy client + notifications preferences + test (E1)
- [ ] T-501 Watcher service: sampling loop, detectors (OCR diff, pixel diff, AI classifier, combined) (E2)
- [ ] T-502 Regions, cooldown, ignore list (E3)
- [ ] T-503 Watch page: enable, status, event log (E4)

## Milestone 6 – Recall (F)
- [ ] T-600 Capture loop + dedup + storage (F1)
- [ ] T-601 Summariser: startup / resume / manual scheduling (F2)
- [ ] T-602 Recall page: day list, summary, raw timeline, export (F3)
- [ ] T-603 Retention pruning (F4)

## Milestone 7 – Agent (G)
- [ ] T-700 LLM provider abstraction: Anthropic, OpenAI, OpenRouter (+ vision, tool calls) (G1)
- [ ] T-701 AI preferences page + test (G1)
- [ ] T-702 Agent loop with tools, ask/auto modes, approval flow (G2, G3)
- [ ] T-703 Capabilities page, per-tool toggles, kill switch, privacy switch (G4)
- [ ] T-704 Playbooks (Teams/Outlook/Confluence) (G5)
- [ ] T-705 Budgets + transcript persistence/export (G6)
- [ ] T-706 Agent page UI

## Milestone 8 – Quality
- [ ] T-800 Unit tests for client, detectors, recall scheduling
- [ ] T-801 `cargo clippy -D warnings`, `cargo fmt`
