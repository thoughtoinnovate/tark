//! UI Backend - Backend-for-Frontend (BFF) Layer
//!
//! This module provides a clean separation between business logic and UI rendering,
//! enabling support for multiple frontend implementations (TUI, Web, Desktop).
//!
//! ## Architecture
//!
//! - **AppService**: Business logic (LLM, providers, sessions, files)
//! - **AppEvent**: Async event channel for streaming updates
//! - **UiRenderer**: Trait that frontends implement
//! - **SharedState**: Thread-safe application state
//! - **Command**: User actions mapped from keybindings/UI

#![allow(dead_code)]

pub mod approval;
pub mod catalog;
mod commands;
pub mod conversation;
pub mod errors;
mod events;
mod git_service;
pub mod middleware;
pub mod questionnaire;
mod service;
pub mod session_service;
mod state;
pub mod storage_facade;
pub mod tool_execution;
mod traits;
mod types;

#[allow(unused_imports)]
pub use catalog::AuthStatus;
pub use catalog::CatalogService;
pub use commands::Command;
#[allow(unused_imports)]
pub use conversation::ConversationService;
// Re-export canonical types from core
pub use crate::core::types::{AgentMode, BuildMode};
// Errors are kept internal for now
// pub use errors::{CatalogError, ConversationError, StorageError, ToolError};
pub use events::AppEvent;
#[allow(unused_imports)]
pub use git_service::GitService;
// Middleware kept internal for now
// pub use middleware::{
//     logging_middleware, normalization_middleware, validation_middleware, CommandPipeline,
//     MiddlewareFn, MiddlewareResult,
// };
pub use service::AppService;
pub use session_service::SessionService;
pub use state::{
    DeviceFlowSession, ErrorLevel, ErrorNotification, FocusedComponent, ModalType, SharedState,
    VimMode,
};
pub use storage_facade::StorageFacade;
pub use tool_execution::ToolExecutionService;
pub use traits::UiRenderer;
pub use types::{
    ArchiveChunkInfo, DiffViewMode, MessageRole, ModelInfo, ProviderInfo, ProviderSource,
    ThemePreset,
};

// Re-export for future use
#[allow(unused_imports)]
pub use types::{
    ActiveToolInfo, AttachmentInfo, ContextFile, GitChangeInfo, GitStatus, Message, SessionInfo,
    StatusInfo, TaskInfo, TaskStatus, ToolStatus,
};
