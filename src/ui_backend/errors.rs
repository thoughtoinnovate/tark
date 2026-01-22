//! Typed errors for BFF services
//!
//! Each service has its own error type for better error handling at the UI layer.

use thiserror::Error;

/// Errors from ConversationService
#[derive(Debug, Error)]
pub enum ConversationError {
    #[error("LLM not connected")]
    NotConnected,

    #[error("Rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Context exceeded: {current}/{max} tokens")]
    ContextExceeded { current: usize, max: usize },

    #[error("Interrupted by user")]
    Interrupted,

    #[error("Provider error: {0}")]
    ProviderError(String),

    #[error("Authentication required for {provider}")]
    AuthRequired { provider: String },

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors from CatalogService
#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("Provider not found: {0}")]
    ProviderNotFound(String),

    #[error("Model not found: {provider}/{model}")]
    ModelNotFound { provider: String, model: String },

    #[error("Provider {0} does not support device flow authentication")]
    DeviceFlowNotSupported(String),

    #[error("No active device flow session")]
    NoActiveDeviceFlow,

    #[error("Device flow expired")]
    DeviceFlowExpired,

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Authentication error: {0}")]
    AuthError(String),

    #[error("Network error: {0}")]
    NetworkError(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors from ToolExecutionService
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    #[error("Tool {tool} not available in {mode:?} mode")]
    NotAvailableInMode {
        tool: String,
        mode: crate::core::AgentMode,
    },

    #[error("Operation denied by user")]
    Denied,

    #[error("Operation blocked: {reason}")]
    Blocked { reason: String },

    #[error("Approval timeout")]
    ApprovalTimeout,

    #[error("Pattern already exists: {0}")]
    PatternExists(String),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Errors from StorageFacade
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),

    #[error("Plugin not found: {0}")]
    PluginNotFound(String),

    #[error("Rule not found: {0}")]
    RuleNotFound(String),

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Invalid session file: {0}")]
    InvalidSessionFile(String),

    #[error("Import failed: {0}")]
    ImportFailed(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}
