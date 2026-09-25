# PRIMVOKON

**Agentic remote for PiKVM.** A native Linux desktop app (Rust · gtk-rs · libadwaita · Blueprint)
that connects to a [PiKVM](https://pikvm.org) on your LAN or over Tailscale and turns the attached
host into an observed, AI-assisted remote workstation.

- **Console** – live MJPEG video with keyboard/mouse forwarding over the kvmd websocket, quick
  actions (power, reset, Ctrl+Alt+Del, paste text, fullscreen).
- **Control centre** – every endpoint of the [PiKVM API](https://docs.pikvm.org/api/) as cards:
  ATX, HID, mass storage (incl. uploads), GPIO (rendered from the kvmd view model), streamer/OCR,
  PiKVM Switch, system info, log viewer, and an API explorer. Core actions are shown by default,
  the rest behind *Show advanced*.
- **Watch** – samples the host screen, detects new notifications/badges/pop-ups (OCR diff, pixel
  diff, AI classifier or combined) and pushes them through your own **ntfy** server.
- **Recall** – captures the screen as text at a configurable interval (default 60 s), stores it
  locally in SQLite, and summarises every day with an LLM on startup / after resume.
- **Agent** – an LLM (Anthropic Claude, OpenAI or OpenRouter) drives the host through PiKVM with
  tools (read screen, type, shortcuts, mouse, power). *Ask mode* shows every side effect for
  approval; *Auto mode* acts autonomously but announces its next steps. Every tool and capability
  can be switched off individually; a privacy switch keeps screenshots away from AI providers.

Product definition: [`docs/FEATURES.md`](docs/FEATURES.md), [`docs/STORIES.md`](docs/STORIES.md),
task tracker [`docs/TASKS.md`](docs/TASKS.md), [`docs/API-COVERAGE.md`](docs/API-COVERAGE.md),
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md).

## Install on your Linux machine

PRIMVOKON is a GTK4/libadwaita app; it is built from source with Cargo.

1. Install the build dependencies (GTK ≥ 4.14, libadwaita ≥ 1.5, blueprint-compiler, OpenSSL):

   ```sh
   # Ubuntu 24.04 / Debian 13
   sudo apt install build-essential pkg-config libgtk-4-dev libadwaita-1-dev blueprint-compiler libssl-dev
   # Fedora
   sudo dnf install gcc pkgconf-pkg-config gtk4-devel libadwaita-devel blueprint-compiler openssl-devel
   # Arch
   sudo pacman -S base-devel pkgconf gtk4 libadwaita blueprint-compiler openssl
   ```

2. Install Rust ≥ 1.85 if you do not have it: `curl https://sh.rustup.rs -sSf | sh`, then open a new shell.

3. Clone and build:

   ```sh
   git clone https://github.com/jonas-bickel/primvokon.git
   cd primvokon
   cargo build --release          # binary: target/release/primvokon
   ```

4. Install the binary, desktop entry and icons (default `PREFIX=/usr/local`; use `PREFIX=$HOME/.local` for a user-only install without sudo):

   ```sh
   sudo make install              # or: make install PREFIX=$HOME/.local
   ```

   PRIMVOKON then appears in your app launcher; `make uninstall` removes it again.

To try it without installing, run `./target/release/primvokon` directly.

If `blueprint-compiler` on `PATH` cannot import PyGObject, point the build at a working
interpreter: `BLUEPRINT_COMPILER="python3.12 /usr/bin/blueprint-compiler" cargo build --release`.

## Run

1. *Preferences → Connection*: base URL (`https://pikvm.local`, `https://100.64.0.5`, …), user,
   password, auth method (session cookie, `X-KVMD-*` headers, HTTP Basic or an existing token),
   optional TOTP secret for 2FA. *Test connection* checks `/api/auth/check` and `/api/info`.
2. *Preferences → Capabilities*: enable Watcher, Recall and Agent as needed.
3. *Preferences → AI* / *Notifications*: API keys and your ntfy server.

Secrets go to the Freedesktop Secret Service (GNOME Keyring / KWallet). Without one they are
stored in `$XDG_DATA_HOME/primvokon/secrets.json` with mode 0600 and the app says so.
Data lives in `$XDG_DATA_HOME/primvokon/primvokon.db`, settings in
`$XDG_CONFIG_HOME/primvokon/settings.toml`.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
python3 tools/mock_pikvm.py 8085 frame1.jpg frame2.jpg   # fake kvmd for UI work
PRIMVOKON_START_PAGE=control cargo run                   # open a given page (console|control|watch|recall|agent|prefs)
```

Workspace layout: `crates/pikvm` (API client, no GTK), `crates/primvokon-core` (settings,
secrets, storage, AI providers, ntfy, watcher, recall, agent), `crates/primvokon` (GTK app).
