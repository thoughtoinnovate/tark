//! Chat agent with tool execution

mod chat;
mod context;
mod rate_limit;
mod tool_orchestrator;

// These are re-exported for use by the TUI module
#[allow(unused_imports)]
pub use chat::{AgentResponse, ChatAgent, CompactResult, ToolCallLog};
pub use context::ConversationContext;
#[allow(unused_imports)] // API for future use
pub use tool_orchestrator::{OrchestratorConfig, ToolOrchestrator};
