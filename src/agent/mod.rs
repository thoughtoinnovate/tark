//! Chat agent with tool execution

mod chat;
mod context;

// These are re-exported for use by the TUI module
#[allow(unused_imports)]
pub use chat::{AgentResponse, ChatAgent, ToolCallLog};
pub use context::ConversationContext;
