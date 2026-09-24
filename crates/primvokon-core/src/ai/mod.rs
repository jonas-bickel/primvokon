//! LLM providers behind one trait. The agent, the recall summariser and the AI change
//! detector only depend on [`LlmProvider`].

pub mod anthropic;
pub mod openai;

use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::settings::{ProviderConfig, ProviderKind};

pub type SharedProvider = Arc<dyn LlmProvider>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

/// One block of a message, mirroring the Anthropic content model (the OpenAI adapter
/// translates it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text {
        text: String,
    },
    Image {
        media_type: String,
        data: Bytes,
    },
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

impl ContentPart {
    pub fn text(s: impl Into<String>) -> Self {
        ContentPart::Text { text: s.into() }
    }

    pub fn jpeg(data: Bytes) -> Self {
        ContentPart::Image {
            media_type: "image/jpeg".into(),
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: Role,
    pub parts: Vec<ContentPart>,
}

impl ChatMessage {
    pub fn user(parts: Vec<ContentPart>) -> Self {
        Self {
            role: Role::User,
            parts,
        }
    }

    pub fn user_text(text: impl Into<String>) -> Self {
        Self::user(vec![ContentPart::text(text)])
    }

    pub fn assistant(parts: Vec<ContentPart>) -> Self {
        Self {
            role: Role::Assistant,
            parts,
        }
    }

    /// Concatenated text blocks.
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    /// JSON schema of the input object.
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
}

impl ChatRequest {
    pub fn new(system: impl Into<String>) -> Self {
        Self {
            system: system.into(),
            messages: Vec::new(),
            tools: Vec::new(),
            max_tokens: 16_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    Refusal,
    Other(String),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

impl Usage {
    pub fn total(self) -> u64 {
        self.input_tokens + self.output_tokens
    }

    pub fn add(&mut self, other: Usage) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatResponse {
    pub parts: Vec<ContentPart>,
    pub stop_reason: StopReason,
    pub usage: Usage,
    pub model: String,
}

impl ChatResponse {
    pub fn text(&self) -> String {
        ChatMessage::assistant(self.parts.clone()).text()
    }

    pub fn tool_uses(&self) -> impl Iterator<Item = (&str, &str, &Value)> {
        self.parts.iter().filter_map(|p| match p {
            ContentPart::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        })
    }
}

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, req: &ChatRequest) -> anyhow::Result<ChatResponse>;
    fn kind(&self) -> ProviderKind;
    fn model(&self) -> &str;

    /// Ask the model to describe an image (vision).
    async fn describe_image(&self, jpeg: &Bytes, prompt: &str) -> anyhow::Result<String> {
        let mut req = ChatRequest::new("You describe screenshots precisely and completely.");
        req.max_tokens = 4_000;
        req.messages.push(ChatMessage::user(vec![
            ContentPart::jpeg(jpeg.clone()),
            ContentPart::text(prompt),
        ]));
        let resp = self.chat(&req).await?;
        ensure_not_refused(&resp)?;
        Ok(resp.text())
    }

    /// Plain text completion.
    async fn complete(&self, system: &str, prompt: &str) -> anyhow::Result<String> {
        let mut req = ChatRequest::new(system);
        req.messages.push(ChatMessage::user_text(prompt));
        let resp = self.chat(&req).await?;
        ensure_not_refused(&resp)?;
        Ok(resp.text())
    }

    /// Cheap connectivity check used by the "Test provider" button.
    async fn ping(&self) -> anyhow::Result<String> {
        let mut req = ChatRequest::new("Reply with the single word OK.");
        req.max_tokens = 16;
        req.messages.push(ChatMessage::user_text("ping"));
        let resp = self.chat(&req).await?;
        Ok(format!("{} answered: {}", resp.model, resp.text().trim()))
    }
}

pub fn ensure_not_refused(resp: &ChatResponse) -> anyhow::Result<()> {
    if resp.stop_reason == StopReason::Refusal {
        anyhow::bail!("the model declined this request (refusal)");
    }
    Ok(())
}

pub(crate) fn b64(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

/// Build the provider for `kind` with the user's configuration and API key.
pub fn build_provider(kind: ProviderKind, cfg: &ProviderConfig, api_key: String) -> anyhow::Result<SharedProvider> {
    if api_key.trim().is_empty() {
        anyhow::bail!("no API key configured for {}", kind.label());
    }
    let provider: SharedProvider = match kind {
        ProviderKind::Anthropic => Arc::new(anthropic::AnthropicProvider::new(cfg, api_key)?),
        ProviderKind::OpenAi | ProviderKind::OpenRouter => {
            Arc::new(openai::OpenAiCompatProvider::new(kind, cfg, api_key)?)
        }
    };
    Ok(provider)
}

#[cfg(test)]
pub mod testing {
    use super::*;
    use tokio::sync::Mutex;

    /// Scripted provider: returns queued responses in order.
    pub struct FakeProvider {
        pub responses: Mutex<Vec<ChatResponse>>,
        pub requests: Mutex<Vec<ChatRequest>>,
    }

    impl FakeProvider {
        pub fn new(responses: Vec<ChatResponse>) -> Self {
            Self {
                responses: Mutex::new(responses),
                requests: Mutex::new(Vec::new()),
            }
        }

        pub fn text(text: &str) -> ChatResponse {
            ChatResponse {
                parts: vec![ContentPart::text(text)],
                stop_reason: StopReason::EndTurn,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
                model: "fake".into(),
            }
        }

        pub fn tool_use(id: &str, name: &str, input: Value, text: Option<&str>) -> ChatResponse {
            let mut parts = Vec::new();
            if let Some(t) = text {
                parts.push(ContentPart::text(t));
            }
            parts.push(ContentPart::ToolUse {
                id: id.into(),
                name: name.into(),
                input,
            });
            ChatResponse {
                parts,
                stop_reason: StopReason::ToolUse,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
                model: "fake".into(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for FakeProvider {
        async fn chat(&self, req: &ChatRequest) -> anyhow::Result<ChatResponse> {
            self.requests.lock().await.push(req.clone());
            let mut r = self.responses.lock().await;
            if r.is_empty() {
                anyhow::bail!("no scripted response left");
            }
            Ok(r.remove(0))
        }

        fn kind(&self) -> ProviderKind {
            ProviderKind::Anthropic
        }

        fn model(&self) -> &str {
            "fake"
        }
    }
}
