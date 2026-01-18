//! tark: AI-powered CLI agent with LSP server
//!
//! This library provides:
//! - LSP server with AI-powered completions, hover, code actions, and diagnostics
//! - HTTP server for ghost text completions and chat API
//! - Chat agent with filesystem and shell tools
//! - Support for multiple LLM providers (Claude, OpenAI)
//! - Terminal UI (TUI) for standalone chat
//! - Plugin system for extensibility

pub mod agent;
pub mod completion;
pub mod config;
pub mod diagnostics;
pub mod llm;
pub mod lsp;
pub mod plugins;
pub mod services;
pub mod storage;
pub mod tools;
pub mod transport;
pub mod tui;
pub mod tui_new;

pub use config::Config;
pub use services::PlanService;
pub use storage::{TarkStorage, WorkspaceConfig};
