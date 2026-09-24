//! The agent loop: model ↔ tools with an approval gate (ask mode) or announcements (auto mode).

use std::sync::Arc;

use serde_json::{json, Value};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::ai::{ChatMessage, ChatRequest, ContentPart, SharedProvider, StopReason, Usage};
use crate::settings::{AgentMode, AgentSettings, Playbook};
use crate::storage::Storage;

use super::tools::{ToolExecutor, ToolKind};

/// The user's answer to an approval request.
#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    /// Run the tool, possibly with edited input.
    Approve(Value),
    Reject(String),
}

/// A side-effecting tool call waiting for the user.
#[derive(Debug)]
pub struct ApprovalRequest {
    pub id: String,
    pub tool: ToolKind,
    pub input: Value,
    pub summary: String,
    pub reply: oneshot::Sender<Decision>,
}

#[derive(Debug)]
pub enum AgentEvent {
    /// Assistant prose (in auto mode this includes "Next steps" announcements).
    Text(String),
    ToolCall {
        id: String,
        tool: ToolKind,
        input: Value,
        summary: String,
    },
    ToolResult {
        id: String,
        tool: ToolKind,
        output: String,
        is_error: bool,
    },
    AwaitingApproval(ApprovalRequest),
    Usage(Usage),
    Finished(String),
    Stopped,
    Error(String),
}

#[derive(Clone)]
pub struct AgentRun {
    cancel: CancellationToken,
}

impl AgentRun {
    pub fn stop(&self) {
        self.cancel.cancel();
    }

    pub fn is_running(&self) -> bool {
        !self.cancel.is_cancelled()
    }
}

pub struct AgentConfig {
    pub provider: SharedProvider,
    pub executor: Arc<ToolExecutor>,
    pub settings: AgentSettings,
    pub playbook: Option<Playbook>,
    pub task: String,
    pub storage: Option<Storage>,
    pub session_id: String,
}

pub fn system_prompt(settings: &AgentSettings, playbook: Option<&Playbook>) -> String {
    let mut s = String::from(
        "You are PRIMVOKON, an assistant operating a remote computer through a PiKVM. You see the host only via \
the read_screen tool and act only through the provided tools. Work step by step: read the screen, act, read again \
to verify. Be conservative: never delete data, close unsaved work or change settings unless the task requires it. \
When the task is complete or impossible, call finish with a short report.\n",
    );
    match settings.mode {
        AgentMode::Ask => s.push_str(
            "Every side-effecting action is shown to the user for approval before it runs; a rejected action \
returns an error explaining why. Adapt to rejections instead of retrying the same action.\n",
        ),
        AgentMode::Auto => s.push_str(
            "You run autonomously. Before every batch of actions write one short paragraph starting with \
\"Next steps:\" that explains what you are about to do and why, then call the tools.\n",
        ),
    }
    if let Some(p) = playbook {
        s.push_str("\nPlaybook: ");
        s.push_str(&p.name);
        s.push('\n');
        s.push_str(&p.prompt);
        s.push('\n');
    }
    if !settings.system_prompt.trim().is_empty() {
        s.push('\n');
        s.push_str(&settings.system_prompt);
        s.push('\n');
    }
    s
}

/// Start a run; events arrive on the receiver until `Finished`, `Stopped` or `Error`.
pub fn start(cfg: AgentConfig) -> (AgentRun, mpsc::Receiver<AgentEvent>) {
    let (tx, rx) = mpsc::channel(64);
    let cancel = CancellationToken::new();
    let run = AgentRun { cancel: cancel.clone() };
    tokio::spawn(async move {
        let mut loop_ = AgentLoop {
            cfg,
            tx,
            cancel,
            usage: Usage::default(),
        };
        loop_.persist("user", &json!({"task": loop_.cfg.task}));
        let outcome = loop_.run().await;
        match outcome {
            Ok(Some(report)) => loop_.emit(AgentEvent::Finished(report)).await,
            Ok(None) => loop_.emit(AgentEvent::Stopped).await,
            Err(e) => loop_.emit(AgentEvent::Error(e.to_string())).await,
        }
    });
    (run, rx)
}

struct AgentLoop {
    cfg: AgentConfig,
    tx: mpsc::Sender<AgentEvent>,
    cancel: CancellationToken,
    usage: Usage,
}

impl AgentLoop {
    fn persist(&self, kind: &str, content: &Value) {
        if let Some(storage) = &self.cfg.storage {
            if let Err(e) = storage.append_agent_message(&self.cfg.session_id, kind, &content.to_string()) {
                tracing::warn!("transcript not saved: {e}");
            }
        }
    }

    async fn emit(&self, ev: AgentEvent) {
        let record = match &ev {
            AgentEvent::Text(t) => Some(("assistant", json!({"text": t}))),
            AgentEvent::ToolCall {
                tool, input, summary, ..
            } => Some((
                "tool_call",
                json!({"tool": tool.name(), "input": input, "summary": summary}),
            )),
            AgentEvent::ToolResult {
                tool, output, is_error, ..
            } => Some((
                "tool_result",
                json!({"tool": tool.name(), "output": output, "is_error": is_error}),
            )),
            AgentEvent::Finished(r) => Some(("finished", json!({"report": r}))),
            AgentEvent::Error(e) => Some(("error", json!({"error": e}))),
            AgentEvent::Stopped => Some(("stopped", json!({}))),
            AgentEvent::Usage(_) | AgentEvent::AwaitingApproval(_) => None,
        };
        if let Some((kind, content)) = record {
            self.persist(kind, &content);
        }
        let _ = self.tx.send(ev).await;
    }

    fn enabled_tools(&self) -> Vec<ToolKind> {
        ToolKind::ALL
            .into_iter()
            .filter(|t| self.cfg.settings.tool_enabled(t.name()))
            .collect()
    }

    /// Returns `Ok(Some(report))` when finished, `Ok(None)` when stopped by the user.
    async fn run(&mut self) -> anyhow::Result<Option<String>> {
        let tools = self.enabled_tools();
        let mut req = ChatRequest::new(system_prompt(&self.cfg.settings, self.cfg.playbook.as_ref()));
        req.tools = tools.iter().map(|t| t.spec()).collect();
        req.messages.push(ChatMessage::user_text(self.cfg.task.clone()));

        for _step in 0..self.cfg.settings.step_limit {
            if self.cancel.is_cancelled() {
                return Ok(None);
            }
            let resp = tokio::select! {
                _ = self.cancel.cancelled() => return Ok(None),
                r = self.cfg.provider.chat(&req) => r?,
            };
            self.usage.add(resp.usage);
            self.emit(AgentEvent::Usage(self.usage)).await;
            let text = resp.text();
            if !text.trim().is_empty() {
                self.emit(AgentEvent::Text(text)).await;
            }
            match resp.stop_reason {
                StopReason::Refusal => anyhow::bail!("the model declined to continue (refusal)"),
                StopReason::MaxTokens => anyhow::bail!("the model hit the output token limit"),
                StopReason::ToolUse => {}
                StopReason::EndTurn | StopReason::Other(_) => {
                    return Ok(Some(resp.text()));
                }
            }
            if self.usage.total() > self.cfg.settings.token_budget {
                anyhow::bail!("token budget of {} exceeded", self.cfg.settings.token_budget);
            }

            req.messages.push(ChatMessage::assistant(resp.parts.clone()));
            let mut results = Vec::new();
            let mut finished: Option<String> = None;
            for (id, name, input) in resp.tool_uses() {
                if self.cancel.is_cancelled() {
                    return Ok(None);
                }
                let (output, is_error) = match ToolKind::from_name(name) {
                    Some(tool) if tools.contains(&tool) => match self.call_tool(id, tool, input).await? {
                        ToolOutcome::Done(out) => (out, false),
                        ToolOutcome::Failed(err) => (err, true),
                        ToolOutcome::Finished(report) => {
                            finished = Some(report.clone());
                            (report, false)
                        }
                        ToolOutcome::Cancelled => return Ok(None),
                    },
                    Some(_) => (format!("tool {name} is disabled by the user"), true),
                    None => (format!("unknown tool {name}"), true),
                };
                results.push(ContentPart::ToolResult {
                    tool_use_id: id.to_string(),
                    content: output,
                    is_error,
                });
            }
            if let Some(report) = finished {
                return Ok(Some(report));
            }
            if results.is_empty() {
                return Ok(Some(resp.text()));
            }
            req.messages.push(ChatMessage::user(results));
        }
        anyhow::bail!("step limit of {} reached", self.cfg.settings.step_limit)
    }

    async fn call_tool(&self, id: &str, tool: ToolKind, input: &Value) -> anyhow::Result<ToolOutcome> {
        let summary = tool.describe_call(input);
        self.emit(AgentEvent::ToolCall {
            id: id.to_string(),
            tool,
            input: input.clone(),
            summary: summary.clone(),
        })
        .await;

        let mut effective_input = input.clone();
        if tool.is_side_effect() && self.cfg.settings.mode == AgentMode::Ask {
            let (reply_tx, reply_rx) = oneshot::channel();
            self.emit(AgentEvent::AwaitingApproval(ApprovalRequest {
                id: id.to_string(),
                tool,
                input: input.clone(),
                summary,
                reply: reply_tx,
            }))
            .await;
            let decision = tokio::select! {
                _ = self.cancel.cancelled() => return Ok(ToolOutcome::Cancelled),
                d = reply_rx => d,
            };
            match decision {
                Ok(Decision::Approve(edited)) => effective_input = edited,
                Ok(Decision::Reject(reason)) => {
                    let msg = format!("rejected by the user: {reason}");
                    self.emit(AgentEvent::ToolResult {
                        id: id.to_string(),
                        tool,
                        output: msg.clone(),
                        is_error: true,
                    })
                    .await;
                    return Ok(ToolOutcome::Failed(msg));
                }
                Err(_) => return Ok(ToolOutcome::Cancelled),
            }
        }

        let outcome = match self.cfg.executor.execute(tool, &effective_input).await {
            Ok(out) if tool == ToolKind::Finish => ToolOutcome::Finished(out),
            Ok(out) => ToolOutcome::Done(out),
            Err(e) => ToolOutcome::Failed(e.to_string()),
        };
        let (output, is_error) = match &outcome {
            ToolOutcome::Done(o) | ToolOutcome::Finished(o) => (o.clone(), false),
            ToolOutcome::Failed(e) => (e.clone(), true),
            ToolOutcome::Cancelled => ("cancelled".into(), true),
        };
        self.emit(AgentEvent::ToolResult {
            id: id.to_string(),
            tool,
            output,
            is_error,
        })
        .await;
        Ok(outcome)
    }
}

enum ToolOutcome {
    Done(String),
    Failed(String),
    Finished(String),
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::testing::FakeProvider;
    use crate::screen::testing::FakeScreen;
    use pikvm::auth::{AuthMethod, Credentials};
    use pikvm::client::ConnectionConfig;
    use pikvm::PikvmClient;

    fn executor() -> Arc<ToolExecutor> {
        let cfg = ConnectionConfig::new(
            ConnectionConfig::parse_base_url("http://127.0.0.1:9").unwrap(),
            AuthMethod::Headers,
            Credentials::default(),
        );
        Arc::new(ToolExecutor {
            client: PikvmClient::new(cfg).unwrap(),
            screen: Arc::new(FakeScreen::new(vec![], vec!["Teams: Anna: are you joining?".into()])),
            vision: None,
            ocr_langs: vec![],
            keymap: None,
        })
    }

    #[tokio::test]
    async fn ask_mode_requests_approval_and_honours_rejection() {
        let provider = Arc::new(FakeProvider::new(vec![
            FakeProvider::tool_use("1", "type_text", json!({"text": "On my way"}), Some("Replying.")),
            FakeProvider::tool_use("2", "finish", json!({"report": "Stopped as asked"}), None),
        ]));
        let cfg = AgentConfig {
            provider: provider.clone(),
            executor: executor(),
            settings: AgentSettings::default(),
            playbook: None,
            task: "reply".into(),
            storage: Some(Storage::open_in_memory().unwrap()),
            session_id: "s".into(),
        };
        let storage = cfg.storage.clone().unwrap();
        storage.create_agent_session("s", "t", "ask").unwrap();
        let (_run, mut rx) = start(cfg);
        let mut saw_text = false;
        let mut finished = None;
        while let Some(ev) = rx.recv().await {
            match ev {
                AgentEvent::Text(t) => saw_text = t.contains("Replying"),
                AgentEvent::AwaitingApproval(req) => {
                    assert_eq!(req.tool, ToolKind::TypeText);
                    req.reply.send(Decision::Reject("not now".into())).unwrap();
                }
                AgentEvent::Finished(r) => finished = Some(r),
                AgentEvent::Error(e) => panic!("{e}"),
                _ => {}
            }
        }
        assert!(saw_text);
        assert_eq!(finished.as_deref(), Some("Stopped as asked"));
        // the rejection was fed back to the model as an error tool result
        let reqs = provider.requests.lock().await;
        let last = &reqs[1].messages.last().unwrap().parts[0];
        assert!(matches!(last, ContentPart::ToolResult { is_error: true, content, .. } if content.contains("not now")));
        assert!(storage.agent_messages("s").unwrap().len() >= 4);
    }

    #[tokio::test]
    async fn disabled_tools_are_not_offered_and_step_limit_stops() {
        let provider = Arc::new(FakeProvider::new(vec![
            FakeProvider::tool_use("1", "read_screen", json!({}), None),
            FakeProvider::tool_use("2", "read_screen", json!({}), None),
        ]));
        let mut settings = AgentSettings::default();
        settings.tools.insert("atx_power".into(), false);
        settings.step_limit = 2;
        settings.mode = AgentMode::Auto;
        let cfg = AgentConfig {
            provider: provider.clone(),
            executor: executor(),
            settings,
            playbook: None,
            task: "look".into(),
            storage: None,
            session_id: "s".into(),
        };
        let (_run, mut rx) = start(cfg);
        let mut error = None;
        while let Some(ev) = rx.recv().await {
            if let AgentEvent::Error(e) = ev {
                error = Some(e);
            }
        }
        assert!(error.unwrap().contains("step limit"));
        let reqs = provider.requests.lock().await;
        assert!(!reqs[0].tools.iter().any(|t| t.name == "atx_power"));
        assert!(reqs[0].system.contains("Next steps:"));
    }
}
