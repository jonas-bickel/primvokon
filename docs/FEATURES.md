# PRIMVOKON – Feature Catalogue

PRIMVOKON is a native Linux desktop application written in **Rust** with **gtk-rs**, **libadwaita**
and **Blueprint** UI files. It connects to a [PiKVM](https://pikvm.org) on the local network (or
anywhere through Tailscale) and turns the PiKVM-attached host machine into an observed,
AI-assisted remote workstation.

Each feature has an ID (`F-<area>-<n>`) that the stories in `STORIES.md` and the tracker in
`TASKS.md` refer to. **Tier** describes default visibility in the UI:

| Tier | Meaning |
|------|---------|
| `core` | Always visible. |
| `advanced` | Hidden until the *Show advanced* switch of the page/card is enabled. |
| `explorer` | Only reachable through the API Explorer (every documented endpoint lives there). |

Every optional capability (Watcher, Recall, Agent, per-tool agent capabilities, ntfy, AI vision)
can be switched off individually (see §8).

---

## 1. Connection & Security (`F-CON`)

| ID | Feature | Tier |
|----|---------|------|
| F-CON-1 | Enter the PiKVM base URL (scheme, host, port, optional path prefix). Plain host names work, so Tailscale `100.x.y.z` / MagicDNS names need no extra configuration. | core |
| F-CON-2 | Choose the auth strategy: **session token** (`POST /api/auth/login` → `auth_token` cookie), **header auth** (`X-KVMD-User` / `X-KVMD-Passwd` on every request), **HTTP Basic**. | core |
| F-CON-3 | Store username and password; optional TOTP secret (2FA) so the app appends the one-time code automatically, or prompt for the code at login. | core |
| F-CON-4 | Paste an existing `auth_token` instead of credentials. | advanced |
| F-CON-5 | Accept self-signed certificates (per profile switch; default on because PiKVM ships self-signed). | core |
| F-CON-6 | *Test connection* button runs `GET /api/auth/check` and `GET /api/info` and reports a diagnostic (HTTP status, kvmd version, latency). | core |
| F-CON-7 | Multiple saved PiKVM profiles, one active; switch from the header bar. | advanced |
| F-CON-8 | Secrets (password, TOTP secret, tokens, AI API keys, ntfy token) are stored in the Secret Service (GNOME Keyring / KWallet via the Freedesktop portal); file fallback with `0600` permissions and a visible warning when no Secret Service is available. | core |
| F-CON-9 | Auto-connect on startup to the active profile; reconnect with exponential backoff. | core |

## 2. Live Console (`F-LIVE`)

| ID | Feature | Tier |
|----|---------|------|
| F-LIVE-1 | Main page shows the live video stream (MJPEG `/streamer/stream`) authenticated with the session. | core |
| F-LIVE-2 | The websocket `/api/ws` is opened together with the stream (it keeps the streamer alive) and feeds a live status strip: video online/resolution/fps, HID online, ATX power/HDD LEDs, MSD connected. | core |
| F-LIVE-3 | Snapshot-polling fallback (`GET /api/streamer/snapshot`) when the MJPEG stream cannot be opened. | core |
| F-LIVE-4 | Quick actions: power short press, reset, Ctrl+Alt+Del, paste text, fullscreen. Destructive ones ask for confirmation. | core |
| F-LIVE-5 | Keyboard capture: while the video has focus, key presses are forwarded as websocket `key` events (using the browser `KeyboardEvent.code` names PiKVM expects). | core |
| F-LIVE-6 | Mouse capture: pointer motion, buttons and wheel over the video are forwarded as websocket `mouse_move` / `mouse_button` / `mouse_wheel` events with absolute coordinates. | core |
| F-LIVE-7 | Websocket auto-reconnect with exponential backoff and a visible connection state (Connected / Reconnecting / Offline). | core |
| F-LIVE-8 | Stream parameter controls: JPEG quality, desired FPS, H.264 bitrate/GOP (`POST /api/streamer/set_params`, present in kvmd). | advanced |

## 3. Control Centre – PiKVM API coverage (`F-API`)

Every endpoint documented at https://docs.pikvm.org/api/ is implemented in the `pikvm` crate and
surfaced in the UI. `API-COVERAGE.md` has the endpoint → UI matrix.

| ID | Feature | Tier |
|----|---------|------|
| F-API-ATX | ATX card: power/HDD LEDs, busy state, actions **Power on / Power off (soft) / Power off (hard) / Reset (hard)** and **Click power / power long / reset**; hard actions ask for confirmation. | core |
| F-API-HID | HID card: online state, keyboard LEDs, active outputs, jiggler; **type text** (keymap, slow mode, delay), **shortcut palette** (Ctrl+Alt+Del, Alt+Tab, Win, …), custom shortcut, single key, mouse button/move/relative/wheel forms, reset, connect/disconnect, set outputs and jiggler. | core (state, type text, shortcuts); advanced (rest) |
| F-API-MSD | MSD card: drive state, image list with sizes, **select image + CD-ROM/Flash + RW**, **connect/disconnect**, upload from URL with progress, upload local file, remove image, reset. | core (state, select, connect); advanced (upload, remove, reset) |
| F-API-GPIO | GPIO card: rendered from the `view.table` model exactly like the PiKVM web UI (labels, input LEDs, output switches/pulse buttons); switch and pulse with `wait`. Hidden when no channels exist. | core |
| F-API-STREAM | Streamer card: encoder, source resolution/fps, sinks, clients; snapshot preview with OCR (languages, region) and save/load/delete; OCR state. | advanced |
| F-API-SWITCH | PiKVM Switch card: state, active port prev/next/set, beacon per port/uplink/downlink, port parameters (name, EDID, dummy, ATX delays), colours, reset/bootloader, EDID create/change/remove, per-port ATX power and click. Hidden when no Switch is detected. | core (when detected) |
| F-API-INFO | System info: `GET /api/info` grouped as **Health** (CPU/mem/temp/throttling), **Platform**, **System** (kvmd, streamer, kernel), **Meta**, **Extras**, **Fan**, **Auth**. | core (health); advanced (rest) |
| F-API-LOG | Log viewer: `GET /api/log` with `seek`, follow mode (long-poll) and text filter. | advanced |
| F-API-REDFISH | Redfish: service root, systems collection, system detail, PATCH no-op, reset with every `ResetType`. | explorer |
| F-API-PROM | Prometheus metrics raw view. | explorer |
| F-API-AUTH | Auth: login, check, logout (used by the connection layer and shown in the explorer). | core |
| F-API-WS | Websocket: every `*_state` event updates a central KVM state model that all cards observe; `ping`/`pong` keep-alive; `stream=0` sessions for background services. | core |
| F-API-EXPLORER | API Explorer: catalogue of every endpoint with category, importance, method, route, parameters and a *Run* form that shows the raw response. | explorer |

## 4. UX & Information Architecture (`F-UX`)

| ID | Feature | Tier |
|----|---------|------|
| F-UX-1 | `AdwApplicationWindow` with a view switcher: **Console · Control · Watch · Recall · Agent**; settings in an `AdwPreferencesDialog`. On narrow windows the switcher moves to the bottom bar (`AdwBreakpoint`). | core |
| F-UX-2 | Follows the system light/dark style and accent colour (libadwaita). | core |
| F-UX-3 | Data is grouped in `AdwPreferencesGroup` cards with status pills, key/value rows and progressive disclosure (*Show advanced* per card). | core |
| F-UX-4 | Confirmation dialogs (`AdwAlertDialog`) for destructive actions (hard power off, reset, MSD remove, Switch reset). | core |
| F-UX-5 | Toast feedback (`AdwToastOverlay`) for every API call with an expandable error detail. | core |
| F-UX-6 | Empty/status pages (`AdwStatusPage`) that explain what is missing (no profile, offline, no Switch, no GPIO). | core |
| F-UX-7 | Keyboard shortcuts (`Ctrl+,` preferences, `F11` fullscreen, `Ctrl+Shift+V` paste text) and a shortcuts window. | core |
| F-UX-8 | Desktop integration: `.desktop` file, app icon, `AdwAboutDialog`. | core |

## 5. USP 1 – Screen Watcher with ntfy (`F-WATCH`)

| ID | Feature | Tier |
|----|---------|------|
| F-WATCH-1 | Periodic screen sampling (default **5 s**) while the watcher is enabled, using `GET /api/streamer/snapshot`. | core |
| F-WATCH-2 | Pluggable change detectors: **OCR text diff** (PiKVM built-in Tesseract, free), **pixel diff** (local, no OCR needed, thresholded), **AI classifier** (vision model answers "did a notification / badge / pop-up appear?"), **combined** (cheap detector triggers, AI confirms). | core |
| F-WATCH-3 | Watch regions: restrict detection to rectangles (e.g. taskbar or notification corner) drawn over a snapshot. | advanced |
| F-WATCH-4 | Debounce, cooldown (minimum time between notifications) and an ignore list of phrases (e.g. the clock). | advanced |
| F-WATCH-5 | ntfy publisher: custom server URL (TrueNAS-hosted), topic, auth (none / access token / basic), priority, tags, title template, optional snapshot attachment. | core |
| F-WATCH-6 | Local event log of detected changes with thumbnail, reason and delivery status. | core |
| F-WATCH-7 | *Send test notification* button. | core |
| F-WATCH-8 | Watcher runs in the background as long as the app is running (also minimised); state shown in the header bar. | core |

## 6. USP 2 – Screen Recall: capture and daily summaries (`F-RECALL`)

| ID | Feature | Tier |
|----|---------|------|
| F-RECALL-1 | Capture loop at a user-defined interval (default **60 s**, allowed 10 s – 30 min) that turns the screen into text. | core |
| F-RECALL-2 | Text extraction strategies: PiKVM OCR (default) or vision-model description (needs an AI provider and the "send screen to AI" permission). | core |
| F-RECALL-3 | Deduplicate consecutive identical captures (hash) to save storage. | core |
| F-RECALL-4 | Daily summarisation with the configured LLM, run **on startup** and **after resume from suspend** for every past day that has captures but no summary; also runnable manually. | core |
| F-RECALL-5 | Compression: raw captures older than the retention period (default 7 days) are pruned once their day has a summary. | advanced |
| F-RECALL-6 | Browse days: sidebar list of days with summary status; detail shows the summary and the raw timeline. | core |
| F-RECALL-7 | Export a day (summary + raw) as Markdown or JSON. | advanced |
| F-RECALL-8 | Customisable summary language and style prompt. | advanced |
| F-RECALL-9 | All data stays local in SQLite under `$XDG_DATA_HOME/primvokon/`. | core |

## 7. USP 3 – Agent Mode (`F-AGENT`)

| ID | Feature | Tier |
|----|---------|------|
| F-AGENT-1 | AI provider settings: **Anthropic (Claude)**, **OpenAI**, **OpenRouter**; API key, model and optional base URL per provider; one active provider for text and one for vision (may be the same). *Test provider* button. | core |
| F-AGENT-2 | Chat interface where the user gives the agent a task ("reply to the Teams message from Anna", "document this in Confluence"). | core |
| F-AGENT-3 | Agent tools: `read_screen` (snapshot → OCR/vision), `type_text`, `send_shortcut`, `send_key`, `click_mouse`, `move_mouse`, `scroll`, `atx_power`, `wait`, `finish`. | core |
| F-AGENT-4 | **Ask mode** (default): every side-effecting tool call pauses the loop and shows an approval card with the exact payload; the user approves, edits or rejects. | core |
| F-AGENT-5 | **Auto mode**: side effects execute without asking, but the agent must announce its next steps before every batch; the transcript records every action. | core |
| F-AGENT-6 | Per-tool capability toggles (turn off `atx_power`, `type_text`, …) and a global agent kill-switch that aborts the running loop. | core |
| F-AGENT-7 | Autonomous reply drafting for Outlook and Microsoft Teams: built-in editable playbooks (prompt sections) the agent uses to read the message on screen and draft a reply for approval. | core |
| F-AGENT-8 | Step limit and token budget per task; the agent stops and reports when exceeded. | advanced |
| F-AGENT-9 | Transcripts persisted per session; export. | advanced |

## 8. Capability Management (`F-CAP`)

| ID | Feature | Tier |
|----|---------|------|
| F-CAP-1 | Every capability (Watcher, Recall, Agent, each agent tool, ntfy, AI vision) has an individual on/off switch in Settings → Capabilities. | core |
| F-CAP-2 | Disabled capabilities are hidden from the UI and their background loops are stopped. | core |
| F-CAP-3 | Master privacy switch *Never send screen content to an AI provider* forces OCR-only strategies everywhere. | core |

## 9. Platform & Engineering (`F-ENG`)

| ID | Feature |
|----|---------|
| F-ENG-1 | Rust 2021, `gtk4` (v4_14), `libadwaita` (v1_5), Blueprint compiled at build time into a GResource. |
| F-ENG-2 | Cargo workspace: `pikvm` (API client, no GTK), `primvokon-core` (settings, secrets, storage, AI providers, ntfy, watcher, recall, agent), `primvokon` (GTK application). |
| F-ENG-3 | Async: dedicated tokio runtime thread; UI receives results through `glib::spawn_future_local` and channels. No blocking on the main thread. |
| F-ENG-4 | Traits at the seams (`ScreenSource`, `TextExtractor`, `ChangeDetector`, `Notifier`, `LlmProvider`, `SecretStore`) so every strategy is swappable and testable. |
| F-ENG-5 | Unit tests for URL/auth building, event parsing, detectors, recall dedup and summarisation scheduling; `cargo clippy` clean. |
| F-ENG-6 | Build with `cargo build --release`; install target via `make install` (desktop file, icon, gschema not required). |
