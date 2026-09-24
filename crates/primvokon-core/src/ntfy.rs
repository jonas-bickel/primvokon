//! Push notifications through an ntfy server (self-hosted or ntfy.sh).

use async_trait::async_trait;
use bytes::Bytes;

use crate::settings::{NtfyAuth, NtfySettings};

/// A notification to deliver.
#[derive(Debug, Clone, Default)]
pub struct Notification {
    pub title: String,
    pub message: String,
    /// 1..5, `None` uses the configured default.
    pub priority: Option<u8>,
    pub tags: Vec<String>,
    /// Optional JPEG attachment `(file name, bytes)`.
    pub attachment: Option<(String, Bytes)>,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, n: &Notification) -> anyhow::Result<()>;
}

/// ntfy publisher configured from [`NtfySettings`].
#[derive(Clone)]
pub struct NtfyNotifier {
    settings: NtfySettings,
    token: Option<String>,
    password: Option<String>,
    http: reqwest::Client,
}

impl NtfyNotifier {
    pub fn new(settings: NtfySettings, token: Option<String>, password: Option<String>) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .danger_accept_invalid_certs(true)
            .build()?;
        Ok(Self {
            settings,
            token,
            password,
            http,
        })
    }

    pub fn topic_url(&self) -> String {
        format!(
            "{}/{}",
            self.settings.server_url.trim_end_matches('/'),
            self.settings.topic.trim_matches('/')
        )
    }

    /// Render the title template with `{reason}` and `{host}`.
    pub fn render_title(template: &str, reason: &str, host: &str) -> String {
        template.replace("{reason}", reason).replace("{host}", host)
    }

    fn apply_auth(&self, rb: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match self.settings.auth {
            NtfyAuth::None => rb,
            NtfyAuth::Token => match &self.token {
                Some(t) if !t.is_empty() => rb.bearer_auth(t),
                _ => rb,
            },
            NtfyAuth::Basic => rb.basic_auth(&self.settings.username, self.password.as_deref()),
        }
    }
}

/// ntfy rejects non-ASCII header values; encode the way its docs suggest (RFC 2047).
fn header_safe(s: &str) -> String {
    if s.is_ascii() {
        s.replace(['\r', '\n'], " ")
    } else {
        use base64::Engine;
        format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(s))
    }
}

#[async_trait]
impl Notifier for NtfyNotifier {
    async fn notify(&self, n: &Notification) -> anyhow::Result<()> {
        let priority = n.priority.unwrap_or(self.settings.priority).clamp(1, 5);
        let mut tags: Vec<String> = self.settings.tags.clone();
        tags.extend(n.tags.iter().cloned());
        let mut rb = self
            .http
            .post(self.topic_url())
            .header("Title", header_safe(&n.title))
            .header("Priority", priority.to_string());
        if !tags.is_empty() {
            rb = rb.header("Tags", header_safe(&tags.join(",")));
        }
        rb = match &n.attachment {
            Some((name, bytes)) => rb
                .header("Filename", header_safe(name))
                .header("Message", header_safe(&n.message))
                .body(bytes.clone()),
            None => rb.body(n.message.clone()),
        };
        let resp = self.apply_auth(rb).send().await?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("ntfy returned {status}: {body}");
        }
        Ok(())
    }
}

/// Notifier that records notifications (tests, dry runs).
#[derive(Default)]
pub struct RecordingNotifier {
    pub sent: tokio::sync::Mutex<Vec<Notification>>,
}

#[async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, n: &Notification) -> anyhow::Result<()> {
        self.sent.lock().await.push(n.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_topic_url_and_title() {
        let s = NtfySettings {
            server_url: "https://ntfy.truenas.lan/".into(),
            topic: "/alerts/".into(),
            ..Default::default()
        };
        let n = NtfyNotifier::new(s, None, None).unwrap();
        assert_eq!(n.topic_url(), "https://ntfy.truenas.lan/alerts");
        assert_eq!(
            NtfyNotifier::render_title("{host}: {reason}", "Teams", "pikvm"),
            "pikvm: Teams"
        );
    }

    #[test]
    fn non_ascii_headers_are_encoded() {
        assert_eq!(header_safe("plain"), "plain");
        assert!(header_safe("ünïcode").starts_with("=?UTF-8?B?"));
    }
}
