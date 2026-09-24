//! Persistent, non-secret settings (TOML). Secrets are referenced by key and stored in a
//! [`crate::secrets::SecretStore`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use pikvm::auth::AuthMethod;
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TotpMode {
    #[default]
    Off,
    /// Compute codes from a stored secret.
    Secret,
    /// Ask for the code every time we connect.
    Ask,
}

/// One saved PiKVM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConnectionProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub auth_method: AuthMethod,
    pub user: String,
    pub totp: TotpMode,
    pub accept_invalid_certs: bool,
}

impl ConnectionProfile {
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            base_url: String::new(),
            auth_method: AuthMethod::Session,
            user: "admin".into(),
            totp: TotpMode::Off,
            accept_invalid_certs: true,
        }
    }

    pub fn secret_key_password(&self) -> String {
        format!("profile:{}:password", self.id)
    }

    pub fn secret_key_totp(&self) -> String {
        format!("profile:{}:totp_secret", self.id)
    }

    pub fn secret_key_token(&self) -> String {
        format!("profile:{}:token", self.id)
    }
}

/// Master switches for optional capabilities (F-CAP-1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub watcher: bool,
    pub recall: bool,
    pub agent: bool,
    pub ntfy: bool,
    pub ai_vision: bool,
    /// Privacy switch: never send screen images or OCR text to an AI provider.
    pub never_send_screen_to_ai: bool,
}

impl Default for Capabilities {
    fn default() -> Self {
        Self {
            watcher: false,
            recall: false,
            agent: false,
            ntfy: true,
            ai_vision: true,
            never_send_screen_to_ai: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    #[default]
    Anthropic,
    OpenAi,
    OpenRouter,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 3] = [ProviderKind::Anthropic, ProviderKind::OpenAi, ProviderKind::OpenRouter];

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "Anthropic (Claude)",
            ProviderKind::OpenAi => "OpenAI",
            ProviderKind::OpenRouter => "OpenRouter",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenAi => "openai",
            ProviderKind::OpenRouter => "openrouter",
        }
    }

    pub fn default_model(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "claude-opus-5",
            ProviderKind::OpenAi => "gpt-5",
            ProviderKind::OpenRouter => "anthropic/claude-opus-5",
        }
    }

    pub fn default_base_url(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::OpenRouter => "https://openrouter.ai/api/v1",
        }
    }

    pub fn secret_key(self) -> String {
        format!("ai:{}:api_key", self.id())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub model: String,
    pub base_url: String,
    /// Anthropic only: opt into server-side refusal fallbacks.
    pub refusal_fallbacks: bool,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            model: String::new(),
            base_url: String::new(),
            refusal_fallbacks: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub text_provider: ProviderKind,
    pub vision_provider: ProviderKind,
    pub providers: BTreeMap<ProviderKind, ProviderConfig>,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            text_provider: ProviderKind::Anthropic,
            vision_provider: ProviderKind::Anthropic,
            providers: BTreeMap::new(),
        }
    }
}

impl AiSettings {
    /// Effective config with defaults filled in.
    pub fn provider(&self, kind: ProviderKind) -> ProviderConfig {
        let mut cfg = self.providers.get(&kind).cloned().unwrap_or_default();
        if cfg.model.trim().is_empty() {
            cfg.model = kind.default_model().to_string();
        }
        if cfg.base_url.trim().is_empty() {
            cfg.base_url = kind.default_base_url().to_string();
        }
        cfg
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NtfyAuth {
    #[default]
    None,
    Token,
    Basic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NtfySettings {
    pub server_url: String,
    pub topic: String,
    pub auth: NtfyAuth,
    pub username: String,
    /// 1 (min) .. 5 (max)
    pub priority: u8,
    pub tags: Vec<String>,
    /// `{reason}` and `{host}` are substituted.
    pub title_template: String,
    pub attach_snapshot: bool,
}

impl Default for NtfySettings {
    fn default() -> Self {
        Self {
            server_url: "https://ntfy.sh".into(),
            topic: "primvokon".into(),
            auth: NtfyAuth::None,
            username: String::new(),
            priority: 3,
            tags: vec!["desktop_computer".into()],
            title_template: "PRIMVOKON: {reason}".into(),
            attach_snapshot: true,
        }
    }
}

impl NtfySettings {
    pub const SECRET_KEY_TOKEN: &'static str = "ntfy:token";
    pub const SECRET_KEY_PASSWORD: &'static str = "ntfy:password";
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DetectorKind {
    #[default]
    OcrDiff,
    PixelDiff,
    AiClassifier,
    Combined,
}

impl DetectorKind {
    pub const ALL: [DetectorKind; 4] = [
        DetectorKind::OcrDiff,
        DetectorKind::PixelDiff,
        DetectorKind::AiClassifier,
        DetectorKind::Combined,
    ];

    pub fn label(self) -> &'static str {
        match self {
            DetectorKind::OcrDiff => "OCR text diff (PiKVM Tesseract, free)",
            DetectorKind::PixelDiff => "Pixel diff (local, no OCR)",
            DetectorKind::AiClassifier => "AI classifier (vision model)",
            DetectorKind::Combined => "Combined (pixel/OCR triggers, AI confirms)",
        }
    }

    pub fn needs_ai(self) -> bool {
        matches!(self, DetectorKind::AiClassifier | DetectorKind::Combined)
    }
}

/// Rectangle in stream pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub left: u32,
    pub top: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WatcherSettings {
    pub interval_secs: u64,
    pub detector: DetectorKind,
    pub ocr_langs: Vec<String>,
    pub regions: Vec<Region>,
    pub cooldown_secs: u64,
    pub ignore_phrases: Vec<String>,
    /// Fraction of changed OCR words that counts as a change.
    pub text_threshold: f64,
    /// Fraction of changed pixels that counts as a change.
    pub pixel_threshold: f64,
}

impl Default for WatcherSettings {
    fn default() -> Self {
        Self {
            interval_secs: 5,
            detector: DetectorKind::OcrDiff,
            ocr_langs: vec!["eng".into()],
            regions: Vec::new(),
            cooldown_secs: 60,
            ignore_phrases: Vec::new(),
            text_threshold: 0.05,
            pixel_threshold: 0.01,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExtractStrategy {
    #[default]
    Ocr,
    Vision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RecallSettings {
    pub interval_secs: u64,
    pub strategy: ExtractStrategy,
    pub ocr_langs: Vec<String>,
    pub retention_days: u32,
    pub summary_language: String,
    pub style_prompt: String,
    pub summarise_on_startup: bool,
}

impl RecallSettings {
    pub const MIN_INTERVAL: u64 = 10;
    pub const MAX_INTERVAL: u64 = 30 * 60;
}

impl Default for RecallSettings {
    fn default() -> Self {
        Self {
            interval_secs: 60,
            strategy: ExtractStrategy::Ocr,
            ocr_langs: vec!["eng".into()],
            retention_days: 7,
            summary_language: "English".into(),
            style_prompt: "Write a concise daily journal: what was worked on, conversations and decisions, open tasks. Use short sections with bullet points.".into(),
            summarise_on_startup: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    /// Ask before every side effect.
    #[default]
    Ask,
    /// Execute autonomously but announce next steps.
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playbook {
    pub id: String,
    pub name: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentSettings {
    pub mode: AgentMode,
    /// Per-tool switches; missing entries default to enabled.
    pub tools: BTreeMap<String, bool>,
    pub step_limit: u32,
    pub token_budget: u64,
    pub playbooks: Vec<Playbook>,
    pub system_prompt: String,
}

impl Default for AgentSettings {
    fn default() -> Self {
        Self {
            mode: AgentMode::Ask,
            tools: BTreeMap::new(),
            step_limit: 25,
            token_budget: 400_000,
            playbooks: crate::agent::playbooks::defaults(),
            system_prompt: String::new(),
        }
    }
}

impl AgentSettings {
    pub fn tool_enabled(&self, name: &str) -> bool {
        self.tools.get(name).copied().unwrap_or(true)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub show_advanced: bool,
    pub capture_input: bool,
    pub window_width: i32,
    pub window_height: i32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            show_advanced: false,
            capture_input: true,
            window_width: 1200,
            window_height: 800,
        }
    }
}

/// Root settings document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub active_profile: Option<String>,
    pub profiles: Vec<ConnectionProfile>,
    pub capabilities: Capabilities,
    pub ai: AiSettings,
    pub ntfy: NtfySettings,
    pub watcher: WatcherSettings,
    pub recall: RecallSettings,
    pub agent: AgentSettings,
    pub ui: UiSettings,
}

impl Settings {
    pub fn load() -> Self {
        Self::load_from(&paths::settings_file())
    }

    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("settings unreadable ({e}), using defaults");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        paths::ensure_dirs()?;
        self.save_to(&paths::settings_file())
    }

    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        let tmp: PathBuf = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }

    pub fn active_profile(&self) -> Option<&ConnectionProfile> {
        let id = self.active_profile.as_deref()?;
        self.profiles.iter().find(|p| p.id == id)
    }

    pub fn profile_mut(&mut self, id: &str) -> Option<&mut ConnectionProfile> {
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    /// Insert or replace a profile and make it active when none is.
    pub fn upsert_profile(&mut self, profile: ConnectionProfile) {
        match self.profiles.iter_mut().find(|p| p.id == profile.id) {
            Some(existing) => *existing = profile.clone(),
            None => self.profiles.push(profile.clone()),
        }
        if self.active_profile.is_none() {
            self.active_profile = Some(profile.id);
        }
    }

    pub fn remove_profile(&mut self, id: &str) {
        self.profiles.retain(|p| p.id != id);
        if self.active_profile.as_deref() == Some(id) {
            self.active_profile = self.profiles.first().map(|p| p.id.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips_through_toml() {
        let mut s = Settings::default();
        let mut p = ConnectionProfile::new("Home");
        p.base_url = "https://pikvm.local".into();
        s.upsert_profile(p.clone());
        s.watcher.regions.push(Region {
            left: 0,
            top: 0,
            width: 10,
            height: 10,
        });
        let text = toml::to_string_pretty(&s).unwrap();
        let back: Settings = toml::from_str(&text).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.active_profile.as_deref(), Some(p.id.as_str()));
    }

    #[test]
    fn defaults_are_sound() {
        let s = Settings::default();
        assert_eq!(s.recall.interval_secs, 60);
        assert_eq!(s.watcher.interval_secs, 5);
        assert_eq!(s.agent.mode, AgentMode::Ask);
        assert!(!s.capabilities.agent && !s.capabilities.watcher && !s.capabilities.recall);
        assert!(s.agent.tool_enabled("type_text"));
        assert_eq!(s.ai.provider(ProviderKind::Anthropic).model, "claude-opus-5");
    }

    #[test]
    fn removing_active_profile_falls_back() {
        let mut s = Settings::default();
        let a = ConnectionProfile::new("A");
        let b = ConnectionProfile::new("B");
        s.upsert_profile(a.clone());
        s.upsert_profile(b.clone());
        s.remove_profile(&a.id);
        assert_eq!(s.active_profile.as_deref(), Some(b.id.as_str()));
    }

    #[test]
    fn missing_file_yields_defaults() {
        let s = Settings::load_from(Path::new("/nonexistent/settings.toml"));
        assert_eq!(s, Settings::default());
    }
}
