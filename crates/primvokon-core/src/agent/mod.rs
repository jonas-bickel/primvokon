//! USP 3 – agent mode: an LLM drives the host through PiKVM with an approval gate.

pub mod playbooks;
pub mod run;
pub mod tools;

pub use run::{AgentConfig, AgentEvent, AgentRun, ApprovalRequest, Decision};
pub use tools::{ToolExecutor, ToolKind};
