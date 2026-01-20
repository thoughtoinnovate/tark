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

    // ========== Error Notification Events ==========
    /// An error occurred that should be shown to the user
    ErrorOccurred {
        message: String,
        level: crate::ui_backend::state::ErrorLevel,
    },

    /// Error notification was cleared
    ErrorCleared,

    // ========== Attachment Events ==========
    /// An attachment was added
    AttachmentAdded { path: String, size: String },

    /// An attachment was removed
    AttachmentRemoved { path: String },

    /// All attachments were cleared
    AttachmentsCleared,

    // ========== Questionnaire Events ==========
    /// A questionnaire (ask_user) was requested by the agent
    QuestionnaireRequested {
        question: String,
        question_type: crate::ui_backend::questionnaire::QuestionType,
        options: Vec<crate::ui_backend::questionnaire::QuestionOption>,
    },

    /// The questionnaire was answered
    QuestionnaireAnswered { answer: serde_json::Value },

    // ========== Approval Events ==========
    /// Approval requested for a risky operation
    ApprovalRequested {
        operation: String,
        risk_level: crate::ui_backend::approval::RiskLevel,
        description: String,
        command: String,
        affected_paths: Vec<String>,
    },

    /// Operation was approved
    OperationApproved,

    /// Operation was rejected
    OperationRejected,

    // ========== Session Events ==========
    /// A new session was created
    SessionCreated { session_id: String },

    /// Session was switched
    SessionSwitched { session_id: String },

    /// Session was loaded successfully
    SessionLoaded {
        session_id: String,
        message_count: usize,
    },

    /// Session was exported
    SessionExported { path: String },

    // ========== Rate Limiting Events ==========
    /// Rate limit was hit
    RateLimitHit {
        retry_after_seconds: u64,
        message: String,
    },

    /// Rate limit expired, retrying pending message
    RateLimitExpired,
}
