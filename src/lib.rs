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
pub mod core;
pub mod debug_logger;
pub mod diagnostics;
pub mod llm;
pub mod lsp;
pub mod plugins;
pub mod services;
pub mod storage;
pub mod tools;
pub mod transport;
// Old TUI removed - using tui_new architecture
pub mod tui_new;
pub mod ui_backend;

pub use config::Config;
pub use debug_logger::{DebugLogEntry, DebugLogger, DebugLoggerConfig, LogCategory};
pub use services::PlanService;
pub use storage::{TarkStorage, WorkspaceConfig};

use std::sync::OnceLock;

/// Global debug logger instance
static TARK_DEBUG_LOGGER: OnceLock<DebugLogger> = OnceLock::new();

/// Initialize the global debug logger
pub fn init_debug_logger(config: DebugLoggerConfig) -> anyhow::Result<()> {
    let logger = DebugLogger::new(config)?;
    TARK_DEBUG_LOGGER
        .set(logger)
        .map_err(|_| anyhow::anyhow!("Debug logger already initialized"))?;
    Ok(())
}

/// Get the global debug logger (if initialized)
pub fn debug_logger() -> Option<&'static DebugLogger> {
    TARK_DEBUG_LOGGER.get()
}

/// Log a debug entry to the global logger (if enabled)
pub fn debug_log(entry: DebugLogEntry) {
    if let Some(logger) = debug_logger() {
        logger.log(entry);
    }
}

/// Helper macro for logging debug entries
#[macro_export]
macro_rules! tark_debug_log {
    ($correlation_id:expr, $category:expr, $event:expr) => {
        if let Some(logger) = $crate::debug_logger() {
            logger.log($crate::DebugLogEntry::new(
                $correlation_id,
                $category,
                $event,
            ));
        }
    };
    ($correlation_id:expr, $category:expr, $event:expr, $data:expr) => {
        if let Some(logger) = $crate::debug_logger() {
            logger.log(
                $crate::DebugLogEntry::new($correlation_id, $category, $event).with_data($data),
            );
        }
    };
}
