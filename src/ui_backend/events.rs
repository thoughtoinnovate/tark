//! Application Events
//!
//! Async events sent from the backend to the frontend for streaming updates.

use serde_json::Value;

use super::types::{Message, ThemePreset};

/// Events emitted by the backend to the frontend
///
/// These are sent asynchronously via an mpsc channel to update the UI
/// during long-running operations (LLM streaming, tool execution, etc.).
#[derive(Debug, Clone)]
pub enum AppEvent {
    // ========== LLM Events ==========
    /// LLM started processing a message
    LlmStarted,

    /// Text chunk received from LLM (streaming)
    LlmTextChunk(String),

    /// Thinking/reasoning chunk received from LLM
    LlmThinkingChunk(String),

    /// LLM completed processing
    LlmCompleted {
        text: String,
        input_tokens: usize,
        output_tokens: usize,
    },

    /// LLM encountered an error
    LlmError(String),

    /// LLM request was interrupted by user
    LlmInterrupted,

    // ========== Tool Events ==========
    /// Tool execution started
    ToolStarted { name: String, args: Value },

    /// Tool execution completed successfully
    ToolCompleted { name: String, result: String },

    /// Tool execution failed
    ToolFailed { name: String, error: String },

    // ========== UI State Events ==========
    /// A new message was added to the conversation
    MessageAdded(Message),

    /// The active LLM provider changed
    ProviderChanged(String),

    /// The active LLM model changed
    ModelChanged(String),

    /// The UI theme changed
    ThemeChanged(ThemePreset),

    /// A context file was added
    ContextFileAdded(String),

    /// A context file was removed
    ContextFileRemoved(String),

    /// Session information updated
    SessionUpdated {
        session_id: String,
        branch: String,
        total_cost: f64,
    },

    /// Task queue updated
    TaskQueueUpdated { count: usize },

    /// Status message changed
    StatusChanged(String),
}
