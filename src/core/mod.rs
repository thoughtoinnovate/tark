//! Core domain modules
//!
//! This module contains domain logic and types shared across the application,
//! including the ui_backend layer and presentation layers.

#![allow(dead_code)]

pub mod attachments;
pub mod context_tracker;
pub mod conversation_manager;
pub mod errors;
pub mod session_manager;
pub mod tokenizer;
pub mod traits;
pub mod types;

// Re-export canonical types
pub use types::AgentMode;

// Re-export context tracking types (public API)
#[allow(unused_imports)]
pub use context_tracker::{ContextBreakdown, ContextTracker};

// Re-export main types for convenience
