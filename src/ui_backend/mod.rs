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
mod commands;
mod events;
pub mod middleware;
pub mod questionnaire;
mod service;
mod state;
mod traits;
mod types;

pub use approval::{ApprovalCardState, RiskLevel};
pub use commands::{AgentMode, BuildMode, Command};
pub use events::AppEvent;
pub use middleware::{
    logging_middleware, normalization_middleware, validation_middleware, CommandPipeline,
    MiddlewareFn, MiddlewareResult,
};
pub use questionnaire::{QuestionOption, QuestionType, QuestionnaireState};
pub use service::AppService;
pub use state::{ErrorLevel, ErrorNotification, FocusedComponent, ModalType, SharedState};
pub use traits::UiRenderer;
pub use types::{MessageRole, ModelInfo, ProviderInfo, ThemePreset};

// Re-export for future use
#[allow(unused_imports)]
pub use types::{
    AttachmentInfo, ContextFile, GitChangeInfo, GitStatus, Message, SessionInfo, StatusInfo,
    TaskInfo, TaskStatus,
};
