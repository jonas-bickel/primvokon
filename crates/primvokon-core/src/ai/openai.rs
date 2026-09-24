//! OpenAI Chat Completions compatible client, used for OpenAI and OpenRouter.

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{b64, ChatMessage, ChatRequest, ChatResponse, ContentPart, LlmProvider, Role, StopReason, Usage};
use crate::settings::{ProviderConfig, ProviderKind};

pub struct OpenAiCompatProvider {
    http: reqwest::Client,
    kind: ProviderKind,
    base_url: String,
    model: String,
    api_key: String,
}

impl OpenAiCompatProvider {
    pub fn new(kind: ProviderKind, cfg: &ProviderConfig, api_key: String) -> anyhow::Result<Self> {
        Ok(Self {
            http: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(600))
                .build()?,
            kind,
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            model: cfg.model.clone(),
            api_key,
        })
    }

    pub fn build_body(&self, req: &ChatRequest) -> Value {
        let mut messages: Vec<Value> = Vec::new();
        if !req.system.trim().is_empty() {
            messages.push(json!({"role": "system", "content": req.system}));
        }
        for m in &req.messages {
            messages.extend(message_to_json(m));
        }
        let mut body = json!({"model": self.model, "messages": messages});
        let max_key = if self.kind == ProviderKind::OpenAi {
            "max_completion_tokens"
        } else {
            "max_tokens"
        };
        body[max_key] = json!(req.max_tokens);
        if !req.tools.is_empty() {
            body["tools"] = json!(req
                .tools
                .iter()
                .map(|t| json!({"type": "function", "function": {"name": t.name, "description": t.description, "parameters": t.input_schema}}))
                .collect::<Vec<_>>());
        }
        body
    }

    pub fn parse_response(v: &Value) -> anyhow::Result<ChatResponse> {
        if let Some(err) = v.get("error") {
            anyhow::bail!("provider error: {}", err["message"].as_str().unwrap_or("unknown"));
        }
        let choice = &v["choices"][0];
        let msg = &choice["message"];
        let mut parts = Vec::new();
        match &msg["content"] {
            Value::String(s) if !s.is_empty() => parts.push(ContentPart::text(s)),
            Value::Array(blocks) => {
                for b in blocks {
                    if let Some(t) = b["text"].as_str() {
                        parts.push(ContentPart::text(t));
                    }
                }
            }
            _ => {}
        }
        for call in msg["tool_calls"].as_array().cloned().unwrap_or_default() {
            let args = call["function"]["arguments"].as_str().unwrap_or("{}");
            let input: Value = serde_json::from_str(args).unwrap_or(json!({}));
            parts.push(ContentPart::ToolUse {
                id: call["id"].as_str().unwrap_or_default().to_string(),
                name: call["function"]["name"].as_str().unwrap_or_default().to_string(),
                input,
            });
        }
        let stop_reason = match choice["finish_reason"].as_str() {
            Some("stop") | None => {
                if parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. })) {
                    StopReason::ToolUse
                } else {
                    StopReason::EndTurn
                }
            }
            Some("tool_calls") | Some("function_call") => StopReason::ToolUse,
            Some("length") => StopReason::MaxTokens,
            Some("content_filter") => StopReason::Refusal,
            Some(other) => StopReason::Other(other.to_string()),
        };
        Ok(ChatResponse {
            parts,
            stop_reason,
            usage: Usage {
                input_tokens: v["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                output_tokens: v["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            },
            model: v["model"].as_str().unwrap_or_default().to_string(),
        })
    }
}

/// One of our messages may expand into several OpenAI messages (tool results are separate).
fn message_to_json(m: &ChatMessage) -> Vec<Value> {
    match m.role {
        Role::User => {
            let mut out = Vec::new();
            let mut content = Vec::new();
            for p in &m.parts {
                match p {
                    ContentPart::Text { text } => content.push(json!({"type": "text", "text": text})),
                    ContentPart::Image { media_type, data } => content.push(json!({
                        "type": "image_url",
                        "image_url": {"url": format!("data:{};base64,{}", media_type, b64(data))}
                    })),
                    ContentPart::ToolResult {
                        tool_use_id,
                        content: c,
                        is_error,
                    } => {
                        let text = if *is_error { format!("ERROR: {c}") } else { c.clone() };
                        out.push(json!({"role": "tool", "tool_call_id": tool_use_id, "content": text}));
                    }
                    ContentPart::ToolUse { .. } => {}
                }
            }
            if !content.is_empty() {
                out.push(json!({"role": "user", "content": content}));
            }
            out
        }
        Role::Assistant => {
            let text = m.text();
            let tool_calls: Vec<Value> = m
                .parts
                .iter()
                .filter_map(|p| match p {
                    ContentPart::ToolUse { id, name, input } => Some(json!({
                        "id": id, "type": "function",
                        "function": {"name": name, "arguments": input.to_string()}
                    })),
                    _ => None,
                })
                .collect();
            let mut msg =
                json!({"role": "assistant", "content": if text.is_empty() { Value::Null } else { json!(text) }});
            if !tool_calls.is_empty() {
                msg["tool_calls"] = json!(tool_calls);
            }
            vec![msg]
        }
    }
}

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    async fn chat(&self, req: &ChatRequest) -> anyhow::Result<ChatResponse> {
        let body = self.build_body(req);
        let mut rb = self
            .http
            .post(format!("{}/chat/completions", self.base_url))
            .bearer_auth(&self.api_key)
            .header("content-type", "application/json");
        if self.kind == ProviderKind::OpenRouter {
            rb = rb
                .header("HTTP-Referer", "https://github.com/jonas-bickel/primvokon")
                .header("X-Title", "PRIMVOKON");
        }
        let resp = rb.json(&body).send().await?;
        let status = resp.status();
        let text = resp.text().await?;
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"error": {"message": text}}));
        if !status.is_success() {
            anyhow::bail!(
                "{} HTTP {}: {}",
                self.kind.label(),
                status,
                v["error"]["message"].as_str().unwrap_or(&text)
            );
        }
        Self::parse_response(&v)
    }

    fn kind(&self) -> ProviderKind {
        self.kind
    }

    fn model(&self) -> &str {
        &self.model
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_tool_round_trip() {
        let cfg = ProviderConfig {
            model: "gpt-5".into(),
            base_url: "https://api.openai.com/v1".into(),
            refusal_fallbacks: false,
        };
        let p = OpenAiCompatProvider::new(ProviderKind::OpenAi, &cfg, "k".into()).unwrap();
        let mut req = ChatRequest::new("sys");
        req.messages.push(ChatMessage::user_text("do it"));
        req.messages.push(ChatMessage::assistant(vec![ContentPart::ToolUse {
            id: "c1".into(),
            name: "type_text".into(),
            input: json!({"text": "hi"}),
        }]));
        req.messages.push(ChatMessage::user(vec![ContentPart::ToolResult {
            tool_use_id: "c1".into(),
            content: "done".into(),
            is_error: false,
        }]));
        let body = p.build_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[2]["tool_calls"][0]["function"]["arguments"], "{\"text\":\"hi\"}");
        assert_eq!(msgs[3]["role"], "tool");
        assert_eq!(body["max_completion_tokens"], 16000);
    }

    #[test]
    fn parses_tool_calls() {
        let v = json!({
            "model": "gpt-5",
            "choices": [{"finish_reason": "tool_calls", "message": {"content": null,
                "tool_calls": [{"id": "c1", "type": "function", "function": {"name": "wait", "arguments": "{\"seconds\": 2}"}}]}}],
            "usage": {"prompt_tokens": 3, "completion_tokens": 4}
        });
        let r = OpenAiCompatProvider::parse_response(&v).unwrap();
        assert_eq!(r.stop_reason, StopReason::ToolUse);
        let (_, name, input) = r.tool_uses().next().unwrap();
        assert_eq!(name, "wait");
        assert_eq!(input["seconds"], 2);
    }
}
