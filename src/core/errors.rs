//! Domain error types
//!
//! These errors represent business logic failures, distinct from infrastructure errors.
//! Using thiserror for ergonomic error handling with proper Display implementations.

use thiserror::Error;

/// Errors related to context window management
#[derive(Debug, Error)]
pub enum ContextError {
    /// Context window exceeded the maximum token limit
    #[error("Context window exceeded: {current}/{max} tokens")]
    WindowExceeded { current: usize, max: usize },

    /// Compaction failed
    #[error("Compaction failed: {0}")]
    CompactionFailed(String),

    /// Invalid token count
    #[error("Invalid token count: {0}")]
    InvalidTokenCount(String),

    /// Tokenizer error
    #[error("Tokenizer error: {0}")]
    TokenizerError(String),
}

/// Errors related to session management
#[derive(Debug, Error)]
pub enum SessionError {
    /// Session not found
    #[error("Session not found: {0}")]
    NotFound(String),

    /// Cannot delete the current session
    #[error("Cannot delete the current session")]
    CannotDeleteCurrent,

    /// Session already exists
    #[error("Session already exists: {0}")]
    AlreadyExists(String),

    /// Invalid session state
    #[error("Invalid session state: {0}")]
    InvalidState(String),

    /// Storage error (wraps infrastructure errors)
    #[error("Storage error: {0}")]
    Storage(String),
}

/// Errors related to conversation management
#[derive(Debug, Error)]
pub enum ConversationError {
    /// Invalid streaming state transition
    #[error("Invalid streaming state transition: {from} -> {to}")]
    InvalidStateTransition { from: String, to: String },

    /// No streaming in progress
    #[error("No streaming in progress")]
    NoStreamingInProgress,

    /// Message error
    #[error("Message error: {0}")]
    MessageError(String),

    /// Attachment error
    #[error("Attachment error: {0}")]
    AttachmentError(String),

    /// LLM communication error
    #[error("LLM error: {0}")]
    LlmError(String),

    /// Not connected to LLM
    #[error("Not connected to LLM")]
    NotConnected,

    /// Other error
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl From<anyhow::Error> for SessionError {
    fn from(err: anyhow::Error) -> Self {
        SessionError::Storage(err.to_string())
    }
}
