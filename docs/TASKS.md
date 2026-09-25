# PRIMVOKON – Task Tracker

Status legend: `[ ]` todo · `[~]` in progress · `[x]` done · `[-]` dropped/deferred.
Tasks reference stories (`STORIES.md`) and features (`FEATURES.md`).

## Milestone 0 – Product definition
- [x] T-000 Feature catalogue (`docs/FEATURES.md`)
- [x] T-001 User stories with acceptance criteria (`docs/STORIES.md`)
- [x] T-002 API coverage matrix (`docs/API-COVERAGE.md`)
- [x] T-003 Architecture notes (`docs/ARCHITECTURE.md`)

## Milestone 1 – Foundation (H1)
- [x] T-100 Cargo workspace: `crates/pikvm`, `crates/primvokon-core`, `crates/primvokon`
- [x] T-101 Blueprint → GResource build pipeline (`build.rs`)
- [x] T-102 Tokio runtime bridge for GTK (`runtime::spawn`, `glib::spawn_future_local`)
- [x] T-103 Settings model (TOML in `$XDG_CONFIG_HOME/primvokon/settings.toml`)
- [x] T-104 `SecretStore` trait: Secret Service + file fallback (A3)
- [x] T-105 SQLite storage (`recall.db`) with migrations
- [x] T-106 README, desktop file, icon, Makefile

## Milestone 2 – PiKVM client (A, C)
- [x] T-200 Auth strategies + TOTP + request builder (A1, A4) with unit tests
- [x] T-201 System: info (fields), log (seek/follow)
- [x] T-202 HID: state, set_params, set_connected, reset, keymaps, print, send_shortcut, send_key, mouse button/move/relative/wheel
- [x] T-203 ATX: state, power, click
- [x] T-204 MSD: state, write (stream upload), write_remote, set_params, set_connected, remove, reset
- [x] T-205 GPIO: state, switch, pulse
- [x] T-206 Streamer: state, snapshot (jpeg/ocr/preview), delete snapshot, ocr state, set_params
- [x] T-207 Switch: state, set_active_prev/next/set, beacon, port params, colours, reset, edids create/change/remove, atx power/click
- [x] T-208 Redfish: root, systems, system, patch, reset
- [x] T-209 Prometheus metrics; auth login/check/logout
- [x] T-210 Websocket client: event parsing, key/mouse events, ping, reconnect with backoff (B4)
- [x] T-211 MJPEG stream reader (multipart parser) + snapshot poller (B1)
- [x] T-212 Endpoint catalogue (category, importance, params) for the explorer (C10)

## Milestone 3 – Application shell & Console (A, B, D)
- [x] T-300 Application, main window, view switcher, breakpoint (D1)
- [x] T-301 Connection preferences page + test connection (A1, A2)
- [x] T-302 Profile switcher & auto-connect (A5)
- [x] T-303 Console page: video widget, status strip, fallback (B1)
- [x] T-304 Keyboard/mouse capture → websocket (B2)
- [x] T-305 Quick actions, paste-text dialog, fullscreen (B3)
- [x] T-306 Toasts, confirmations, status pages (D2)
- [x] T-307 Shortcuts window, about dialog (D3)

## Milestone 4 – Control Centre (C)
- [x] T-400 KVM state model (GObject) fed by websocket events
- [x] T-401 ATX card (C1)
- [x] T-402 HID card (C2)
- [x] T-403 MSD card incl. uploads (C3)
- [x] T-404 GPIO card from view model (C4)
- [x] T-405 Streamer/OCR card (C5)
- [x] T-406 Switch card (C6)
- [x] T-407 System info card (C7)
- [x] T-408 Log viewer (C8)
- [x] T-409 API Explorer incl. Redfish/Prometheus (C9, C10)

## Milestone 5 – Watcher + ntfy (E)
- [x] T-500 ntfy client + notifications preferences + test (E1)
- [x] T-501 Watcher service: sampling loop, detectors (OCR diff, pixel diff, AI classifier, combined) (E2)
- [x] T-502 Regions, cooldown, ignore list (E3) — regions are entered as `left,top,width,height` in Preferences; drawing them on a snapshot is deferred
- [x] T-503 Watch page: enable, status, event log (E4)

## Milestone 6 – Recall (F)
- [x] T-600 Capture loop + dedup + storage (F1)
- [x] T-601 Summariser: startup / resume / manual scheduling (F2) — resume comes from logind `PrepareForSleep`, with a wall-clock jump as fallback
- [x] T-602 Recall page: day list, summary, raw timeline, export (F3)
- [x] T-603 Retention pruning (F4)

## Milestone 7 – Agent (G)
- [x] T-700 LLM provider abstraction: Anthropic, OpenAI, OpenRouter (+ vision, tool calls) (G1)
- [x] T-701 AI preferences page + test (G1)
- [x] T-702 Agent loop with tools, ask/auto modes, approval flow (G2, G3)
- [x] T-703 Capabilities page, per-tool toggles, kill switch, privacy switch (G4)
- [x] T-704 Playbooks (Teams/Outlook/Confluence) (G5)
- [x] T-705 Budgets + transcript persistence/export (G6)
- [x] T-706 Agent page UI

## Milestone 8 – Quality
- [x] T-800 Unit tests for client, detectors, recall scheduling
- [x] T-801 `cargo clippy -D warnings`, `cargo fmt`

## Deferred / follow-ups
- [ ] T-900 Draw watch regions directly on a snapshot (currently a text field)
- [x] T-901 logind `PrepareForSleep` signal for resume detection (wall-clock jump remains the fallback without logind)
- [ ] T-902 Verify against a real PiKVM Switch (state schema was derived from kvmd source, docs say FIXME)
- [ ] T-903 Streaming responses / progress for long agent turns
- [ ] T-904 Flatpak manifest
