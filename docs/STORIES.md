# PRIMVOKON – User Stories

Stories are grouped by epic and ordered by value. Each story lists the features it delivers
(`F-…` from `FEATURES.md`), a MoSCoW priority, and acceptance criteria written as
*Given / When / Then* so that "done" is unambiguous. `TASKS.md` tracks implementation status.

Persona: **Jonas**, a power user who runs a PiKVM in front of a work machine (Windows with
Outlook, Teams and Confluence) and wants to observe and operate it from a Linux laptop, at home
over LAN and on the go over Tailscale.

---

## Epic A – Connect to a PiKVM

### A1 · Configure a connection profile  (Must) — F-CON-1, F-CON-2, F-CON-3, F-CON-5
*As Jonas I want to enter everything the app needs to reach my PiKVM so that I never have to edit a config file.*

- Given the app starts with no profile, then the Console shows a status page "No PiKVM configured" with an *Open settings* button.
- Given Settings → Connection, when I enter a base URL (`https://pikvm.tail1234.ts.net`, `https://192.168.1.20`, or `http://pikvm:8080/prefix`), a name, user, password, then the profile can be saved; the URL is validated and shown normalised.
- Given the *Auth method* row, when I pick *Session token*, *Header auth* or *HTTP Basic*, then the client uses exactly that scheme for every request (verified by unit tests on the request builder).
- Given a TOTP secret is entered, when the client authenticates, then it appends the current 6-digit code to the password; if none is entered and *Ask for 2FA code* is on, a dialog asks for the code on connect.
- Given *Accept self-signed certificate* is on (default), then TLS errors for self-signed certs are ignored for that profile only.

### A2 · Test and diagnose the connection  (Must) — F-CON-6
- Given a filled-in profile, when I press *Test connection*, then within 10 s I see one of: ✅ "Connected · kvmd 4.x · 38 ms", ⚠️ "Reachable but credentials rejected (403)", ❌ "Unreachable: <error>". The result never blocks the UI.

### A3 · Secrets are stored safely  (Must) — F-CON-8
- Given I saved a profile, then password, TOTP secret, tokens and API keys are not present in `settings.toml`; they live in the Secret Service.
- Given no Secret Service is reachable, then secrets are written to `$XDG_DATA_HOME/primvokon/secrets.json` with mode `0600` and a persistent banner in Settings says so.

### A4 · Paste an existing token  (Should) — F-CON-4
- Given *Show advanced* in Connection, when I paste an `auth_token`, then the client sends it as the `auth_token` cookie and no login call is made.

### A5 · Multiple profiles and auto-connect  (Should) — F-CON-7, F-CON-9
- Given two profiles, when I choose one in the header-bar menu, then the app disconnects, switches and reconnects; the choice is remembered across restarts.
- Given a saved active profile, when the app starts, then it connects automatically and retries with backoff (1 s → 30 s cap) while showing "Reconnecting…".

---

## Epic B – Live Console (main page)

### B1 · See the host screen live  (Must) — F-LIVE-1, F-LIVE-2, F-LIVE-3
- Given a connected profile, when the Console page is shown, then the MJPEG stream plays inside the window, keeping aspect ratio, within 2 s of connecting.
- Given the websocket is connected, then the status strip shows video resolution and FPS, HID online, ATX power LED and MSD state, updated live from `*_state` events.
- Given the MJPEG stream fails (HTTP error or timeout), then the console falls back to snapshot polling every second and shows a "fallback" pill.

### B2 · Control the host with keyboard and mouse  (Must) — F-LIVE-5, F-LIVE-6
- Given the video widget has focus, when I press keys, then each press/release is sent as a websocket `key` event with the correct `KeyboardEvent.code` name (mapping covered by tests); the host sees the input.
- Given the pointer is over the video, when I move/click/scroll, then `mouse_move` (absolute coordinates scaled to the stream resolution), `mouse_button` and `mouse_wheel` events are sent; a *Capture input* toggle disables forwarding.

### B3 · Quick actions  (Must) — F-LIVE-4, F-UX-4
- Given the console header, then buttons *Power*, *Reset*, *Ctrl+Alt+Del*, *Paste text*, *Fullscreen* exist; *Reset* and long power press require confirmation; *Paste text* opens a dialog with keymap selection and calls `POST /api/hid/print`.

### B4 · Robust connection state  (Must) — F-LIVE-7
- Given the websocket drops, then a "Reconnecting" banner appears and the app reconnects with backoff; on success the banner disappears without user action.

### B5 · Tune the stream  (Could) — F-LIVE-8
- Given *Show advanced* on the console, when I change quality/FPS/bitrate, then `POST /api/streamer/set_params` is called and the new values are reflected from `streamer_state`.

---

## Epic C – Control Centre (full API coverage)

### C1 · ATX power management  (Must) — F-API-ATX
- Given the ATX card, then LEDs and busy state are live; *Power on*, *Power off*, *Hard off*, *Hard reset*, *Click power*, *Click power long*, *Click reset* call the documented endpoints; hard actions confirm first; buttons disable while `busy`.

### C2 · HID  (Must) — F-API-HID
- Given the HID card, then state (online, LEDs, outputs, jiggler) is live; *Type text* with keymap/slow/delay, shortcut palette, and custom shortcut are visible by default; single key, mouse forms, set params, connect/disconnect and reset appear under *Show advanced*.

### C3 · Mass Storage Drive  (Must) — F-API-MSD
- Given the MSD card, then drive state and image list (name, size) are live; I can select an image, choose CD-ROM/Flash and RW, connect/disconnect. Under advanced: upload by URL with live progress from `msd_state.storage.downloading`, upload a local file with a progress bar, remove (confirm), reset.

### C4 · GPIO  (Should) — F-API-GPIO
- Given the `gpio_model_state` view table, then the card renders labels, LEDs (inputs, coloured) and buttons/switches (outputs) row by row; switch and pulse actions work; the card is hidden when the table is empty.

### C5 · Streamer and OCR  (Should) — F-API-STREAM
- Given the Streamer card (advanced), then encoder/source/sinks/clients are shown; *Snapshot* shows a preview; *OCR* runs with language and region and shows text; delete snapshot works; OCR state lists available languages.

### C6 · PiKVM Switch  (Should) — F-API-SWITCH
- Given `switch_state` reports a Switch, then the card lists ports with active highlight, *Prev/Next/Set active*, beacon toggles, port params dialog, colours, reset (confirm), EDID management, per-port ATX. Hidden otherwise.

### C7 · System information  (Must) — F-API-INFO
- Given the System card, then Health (CPU %, memory, temperature, throttling flags) is visible by default; Platform/System/Meta/Extras/Fan/Auth under advanced; values refresh from `info_*_state` events and `GET /api/info`.

### C8 · Log viewer  (Should) — F-API-LOG
- Given Control → Log (advanced), when I open it, then the last hour (`seek=3600`) loads; *Follow* long-polls and appends lines; a filter entry narrows lines.

### C9 · Redfish and Prometheus  (Could) — F-API-REDFISH, F-API-PROM
- Given the API Explorer, then Redfish root/systems/system detail can be fetched and *Reset* sent with a chosen `ResetType`; Prometheus metrics are shown raw.

### C10 · API Explorer  (Should) — F-API-EXPLORER
- Given Control → API Explorer, then every endpoint from the reference is listed grouped by category and marked *core/advanced/explorer*; selecting one shows a form built from its parameter descriptors; *Run* shows status, headers and body.

---

## Epic D – UX foundation

### D1 · Navigation and adaptive layout  (Must) — F-UX-1, F-UX-2
- Given a window ≥ 700 px wide, then the view switcher is in the header bar; below that it is a bottom bar. Light/dark follows the system.

### D2 · Feedback and safety  (Must) — F-UX-3, F-UX-4, F-UX-5, F-UX-6
- Every API call shows a toast on failure (with *Details*), and on success for state-changing calls; destructive calls confirm; every page has a meaningful empty state.

### D3 · Shortcuts, about, desktop file  (Should) — F-UX-7, F-UX-8

---

## Epic E – Screen Watcher with ntfy (USP 1)

### E1 · Configure ntfy  (Must) — F-WATCH-5, F-WATCH-7
- Given Settings → Notifications, when I enter server URL (e.g. `https://ntfy.truenas.lan`), topic, auth (none/token/basic), priority, tags and title template, then *Send test notification* publishes and I see the message on my phone; failures show the HTTP error.

### E2 · Detect changes on the host screen  (Must) — F-WATCH-1, F-WATCH-2
- Given the watcher is enabled with the *OCR diff* detector, when a new toast or badge changes the OCR text by more than the threshold, then an event is created within one interval and an ntfy message with the changed text is sent.
- Given the *Pixel diff* detector, when more than N % of pixels change inside the watch region, then an event is created (no OCR/AI needed).
- Given the *AI classifier* detector and a configured vision provider, when the cheap detector triggers, then the AI is asked with the before/after images and only a "yes" produces a notification.

### E3 · Tune the watcher  (Should) — F-WATCH-3, F-WATCH-4
- Regions can be drawn on a snapshot; cooldown and ignore phrases suppress noise; defaults: cooldown 60 s, ignore the clock pattern.

### E4 · Event log and background operation  (Must) — F-WATCH-6, F-WATCH-8
- The Watch page lists events (time, reason, thumbnail, delivered ✓/✗); the watcher keeps running while the window is minimised; the header shows a watching indicator.

---

## Epic F – Screen Recall (USP 2)

### F1 · Capture the screen as text  (Must) — F-RECALL-1, F-RECALL-2, F-RECALL-3, F-RECALL-9
- Given Recall is enabled with interval 60 s (default) and OCR strategy, then every interval a capture is stored (timestamp, day, text, hash) unless identical to the previous; storage is SQLite in `$XDG_DATA_HOME/primvokon/recall.db`.
- Given the *vision* strategy and an AI provider, then the model's description of the screen is stored instead.

### F2 · Daily summaries  (Must) — F-RECALL-4
- Given yesterday has captures and no summary, when the app starts, then a summary is generated within a minute and stored with model name and time.
- Given the machine suspends across midnight, when it resumes (detected via `org.freedesktop.login1 PrepareForSleep` or a wall-clock jump), then pending days are summarised.
- Given the Recall page, when I press *Summarise now*, then today is summarised on demand.

### F3 · Browse and export  (Must / Should) — F-RECALL-6, F-RECALL-7, F-RECALL-8
- The Recall page has a day list; a day shows summary and raw timeline; *Export* writes Markdown/JSON via a file dialog; prompt language/style is editable.

### F4 · Retention  (Should) — F-RECALL-5
- Raw captures older than N days (default 7) are deleted after their summary exists; never before.

---

## Epic G – Agent Mode (USP 3)

### G1 · Configure AI providers  (Must) — F-AGENT-1
- Given Settings → AI, when I add Anthropic/OpenAI/OpenRouter with API key, model and optional base URL, then *Test* sends a one-token request and reports success; keys go to the Secret Service; I pick one provider for text and one for vision.

### G2 · Give the agent a task with approval  (Must) — F-AGENT-2, F-AGENT-3, F-AGENT-4
- Given Ask mode, when I type "Reply to the Teams message from Anna that I'll be 10 min late", then the agent reads the screen, plans, and before typing shows an approval card with the exact text/keys; *Approve* executes, *Edit* lets me change the payload, *Reject* returns the refusal to the model.

### G3 · Auto mode  (Must) — F-AGENT-5
- Given Auto mode, then side effects run without approval, but before every batch the transcript shows "Next steps: …"; a *Stop* button aborts immediately.

### G4 · Capability toggles and kill switch  (Must) — F-AGENT-6, F-CAP-1, F-CAP-2, F-CAP-3
- Every tool has a switch; disabled tools are not offered to the model; the global kill switch stops any running loop; the privacy switch forces OCR everywhere.

### G5 · Outlook/Teams playbooks  (Should) — F-AGENT-7
- Built-in playbooks "Reply in Teams", "Reply in Outlook", "Document in Confluence" are selectable in the composer and editable in Settings.

### G6 · Budgets and transcripts  (Should) — F-AGENT-8, F-AGENT-9
- Step limit (default 25) and token budget stop the loop with a report; transcripts are listed and exportable.

---

## Epic H – Engineering

### H1 · Workspace, build, tests  (Must) — F-ENG-1 … F-ENG-6
- `cargo build`, `cargo test`, `cargo clippy -- -D warnings` pass; Blueprint files compile at build time; README documents build and run.
