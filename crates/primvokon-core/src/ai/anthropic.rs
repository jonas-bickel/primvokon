//! Anthropic Messages API over raw HTTP (`POST /v1/messages`).

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{b64, ChatRequest, ChatResponse, ContentPart, LlmProvider, Role, StopReason, Usage};
use crate::settings::{ProviderConfig, ProviderKind};

const API_VERSION: &str = "2023-06-01";
/// Beta header for `fallbacks: "default"` (server-side refusal fallbacks).
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

pub struct AnthropicProvider {
    http: reqwest::Client,
    base_url: String,
    model: String,
    api_key: String,
    refusal_fallbacks: bool,
}

impl AnthropicProvider {
    pub fn new(cfg: &ProviderConfig, api_key: String) -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(600)).build()?,
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            model: cfg.model.clone(),
            api_key,
            refusal_fallbacks: cfg.refusal_fallbacks,
        })
    }

    /// Whether the configured model is a Fable/Opus 5 generation model, where refusal
    /// fallbacks are useful.
    fn supports_fallbacks(&self) -> bool {
        self.model.starts_with("claude-fable") || self.model.starts_with("claude-opus-5")
    }

    pub fn build_body(&self, req: &ChatRequest) -> Value {
        let messages: Vec<Value> = req.messages.iter().map(message_to_json).collect();
        let mut body = json!({
            "model": self.model,
            "max_tokens": req.max_tokens,
            "messages": messages,
        });
        if !req.system.trim().is_empty() {
            body["system"] = json!(req.system);
        }
        if !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({"name": t.name, "description": t.description, "input_schema": t.input_schema}))
                .collect::<Vec<_>>());
        }
        if self.refusal_fallbacks && self.supports_fallbacks() {
            body["fallbacks"] = json!("default");
        }
        body
    }

    pub fn parse_response(v: &Value) -> anyhow::Result<ChatResponse> {
        if let Some(err) = v.get("error") {
            anyhow::bail!("Anthropic error: {}", err["message"].as_str().unwrap_or("unknown"));
        }
        let mut parts = Vec::new();
        for block in v["content"].as_array().cloned().unwrap_or_default() {
            match block["type"].as_str() {
                Some("text") => parts.push(ContentPart::text(block["text"].as_str().unwrap_or_default())),
                Some("tool_use") => parts.push(ContentPart::ToolUse {
                    id: block["id"].as_str().unwrap_or_default().to_string(),
                    name: block["name"].as_str().unwrap_or_default().to_string(),
                    input: block["input"].clone(),
                }),
                _ => {}
            }
        }
        let stop_reason = match v["stop_reason"].as_str() {
            Some("end_turn") | Some("stop_sequence") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            Some("refusal") => StopReason::Refusal,
            Some(other) => StopReason::Other(other.to_string()),
            None => StopReason::EndTurn,
        };
        Ok(ChatResponse {
            parts,
            stop_reason,
            usage: Usage {
                input_tokens: v["usage"]["input_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["output_tokens"].as_u64().unwrap_or(0),
            },
            model: v["model"].as_str().unwrap_or_default().to_string(),
        })
    }
}

fn message_to_json(m: &super::ChatMessage) -> Value {
    let role = match m.role {
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let content: Vec<Value> = m
        .parts
        .iter()
        .map(|p| match p {
            ContentPart::Text { text } => json!({"type": "text", "text": text}),
            ContentPart::Image { media_type, data } => json!({
                "type": "image",
                "source": {"type": "base64", "media_type": media_type, "data": b64(data)}
            }),
            ContentPart::ToolUse { id, name, input } => json!({"type": "tool_use", "id": id, "name": name, "input": input}),
            ContentPart::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content, "is_error": is_error}),
        })
        .collect();
    json!({"role": role, "content": content})
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn chat(&self, req: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let body = self.build_body(req);
        let mut rb = self
            .http
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .header("content-type", "application/json");
        if body.get("fallbacks").is_some() {
            rb = rb.header("anthropic-beta", FALLBACK_BETA);
        }
        let resp = rb.json(&body).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"error": {"message": text}}));
        if !status.is_success() {
            anyhow::bail!(
                "Anthropic HTTP {}: {}",
                status,
                v["error"]["message"].as_str().unwrap_or(&text)
            );
        }
        Self::parse_response(&v)
    }

    fn kind(&self) -> ProviderKind {
        ProviderKind::Anthropic
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, ToolSpec};

    fn provider() -> AnthropicProvider {
        let cfg = ProviderConfig {
            model: "claude-opus-5".into(),
            base_url: "https://api.anthropic.com".into(),
            refusal_fallbacks: true,
        };
        AnthropicProvider::new(&cfg, "k".into()).unwrap()
    }

    #[test]
    fn builds_request_body() {
        let p = provider();
        let mut req = ChatRequest::new("sys");
        req.tools.push(ToolSpec {
            name: "t".into(),
            description: "d".into(),
            input_schema: json!({"type": "object"}),
        });
        req.messages.push(ChatMessage::user(vec![
            ContentPart::jpeg(bytes::Bytes::from_static(b"\xff\xd8")),
            ContentPart::text("hi"),
        ]));
        req.messages.push(ChatMessage::assistant(vec![ContentPart::ToolUse {
            id: "1".into(),
            name: "t".into(),
            input: json!({}),
        }]));
        req.messages.push(ChatMessage::user(vec![ContentPart::ToolResult {
            tool_use_id: "1".into(),
            content: "ok".into(),
            is_error: false,
        }]));
        let body = p.build_body(&req);
        assert_eq!(body["model"], "claude-opus-5");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["fallbacks"], "default");
        assert_eq!(body["messages"][0]["content"][0]["source"]["media_type"], "image/jpeg");
        assert_eq!(body["messages"][1]["content"][0]["type"], "tool_use");
        assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "1");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn parses_tool_use_response() {
        let v = json!({
            "model": "claude-opus-5", "stop_reason": "tool_use",
            "content": [{"type": "text", "text": "Let me look."}, {"type": "tool_use", "id": "toolu_1", "name": "read_screen", "input": {}}],
            "usage": {"input_tokens": 12, "output_tokens": 7}
        });
        let r = AnthropicProvider::parse_response(&v).unwrap();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        assert_eq!(r.tool_uses().count(), 1);
        assert_eq!(r.usage.total(), 19);
        let refusal = json!({"stop_reason": "refusal", "content": [], "usage": {}});
        assert_eq!(AnthropicProvider::parse_response(&refusal).unwrap().stop_reason, StopReason::Refusal);
    }
}
