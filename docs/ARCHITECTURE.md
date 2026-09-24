# Architecture

```
crates/
  pikvm/            Pure PiKVM API client (reqwest + tokio-tungstenite). No GTK.
    auth.rs         AuthStrategy (session cookie / headers / basic / token) + TOTP
    client.rs       PikvmClient: base URL, TLS policy, request builder, ApiResult<T>
    api/*.rs        One module per category (system, hid, atx, msd, gpio, streamer, switch, redfish, misc)
    ws.rs           WsClient: events in, input events out, ping, reconnect
    stream.rs       MJPEG multipart reader + snapshot poller (Frame = JPEG bytes)
    events.rs       Typed websocket events (KvmEvent) + full state (KvmState)
    catalog.rs      Endpoint catalogue (category, importance, params) for the API Explorer
    keycodes.rs     evdev keycode → KeyboardEvent.code (kvmd web_name)
  primvokon-core/   Application domain, still GTK-free
    settings.rs     Settings (TOML), profiles, capability toggles, defaults
    secrets.rs      SecretStore trait, SecretServiceStore (oo7), FileStore fallback
    storage.rs      SQLite (rusqlite): captures, summaries, watch events, transcripts
    ai/             LlmProvider trait, Anthropic / OpenAI / OpenRouter (OpenAI-compatible) impls, ChatRequest with images + tools
    ntfy.rs         Notifier trait + NtfyNotifier
    screen.rs       ScreenSource trait (snapshot / OCR via PiKVM), TextExtractor strategies
    watcher/        Detectors (OcrDiff, PixelDiff, AiClassifier, Combined) + WatcherService
    recall/         CaptureService, Summariser, scheduling (startup / resume / manual), retention
    agent/          Tools, AgentLoop (ask / auto), approval channel, playbooks, budgets
    runtime.rs      Shared tokio runtime
  primvokon/        GTK4 + libadwaita application
    build.rs        blueprint-compiler → .ui → GResource
    data/ui/*.blp   Blueprint templates
    src/app.rs      Application (actions, startup, services)
    src/window.rs   Main window (view switcher, breakpoint, toasts)
    src/state/      KvmStateObject (GObject facade over KvmState), ConnectionManager
    src/pages/      console, control (cards), watch, recall, agent
    src/prefs/      Preferences dialog pages: connection, capabilities, ai, notifications, watcher, recall, agent
    src/widgets/    Reusable widgets (StatusPill, KeyValueRow, ApprovalCard, VideoView)
```

## Principles
- **Dependency direction**: `primvokon → primvokon-core → pikvm`. Lower crates never know about GTK.
- **Seams are traits**: `ScreenSource`, `TextExtractor`, `ChangeDetector`, `Notifier`, `LlmProvider`,
  `SecretStore`, `AgentTool`. Strategies are chosen from settings via small factories.
- **One tokio runtime** (`runtime::spawn`). UI code awaits `JoinHandle`s in `glib::spawn_future_local`
  and receives streams over `async_channel`. Nothing blocks the GTK main thread.
- **Single source of truth for KVM state**: websocket events update `KvmState`; the GTK layer wraps
  it in a GObject with notify signals; cards bind to it. REST calls are used for actions and the
  initial fetch only.
- **Progressive disclosure**: every card has `core` rows and an *advanced* section. The endpoint
  catalogue carries the same importance flags so the Explorer and the cards agree.
- **Safety**: destructive actions confirm; agent side effects go through an `Approval` channel in
  ask mode; all capabilities are individually switchable and default-off except the console.
