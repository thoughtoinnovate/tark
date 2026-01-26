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
pub mod auth;
pub mod channels;
pub mod completion;
pub mod config;
pub mod core;
pub mod debug_logger;
pub mod diagnostics;
pub mod llm;
pub mod lsp;
pub mod plugins;
pub mod policy;
pub mod secure_store;
pub mod services;
pub mod storage;
pub mod tools;
pub mod transport;
// Old TUI removed - using tui_new architecture
pub mod tui_new;
pub mod ui_backend;

// MCP client (behind feature flag)
#[cfg(feature = "mcp-client")]
pub mod mcp;

pub use config::Config;
pub use debug_logger::{DebugLogEntry, DebugLogger, DebugLoggerConfig, LogCategory};
pub use services::PlanService;
pub use storage::{TarkStorage, WorkspaceConfig};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

/// Global debug logger instance
static TARK_DEBUG_LOGGER: OnceLock<DebugLogger> = OnceLock::new();

/// Fast path check for whether debug logging is enabled.
/// This atomic bool avoids the overhead of checking OnceLock on every log call.
static DEBUG_LOGGING_ENABLED: AtomicBool = AtomicBool::new(false);

/// Initialize the global debug logger
pub fn init_debug_logger(config: DebugLoggerConfig) -> anyhow::Result<()> {
    let logger = DebugLogger::new(config)?;
    TARK_DEBUG_LOGGER
        .set(logger)
        .map_err(|_| anyhow::anyhow!("Debug logger already initialized"))?;
    // Set the fast-path flag
    DEBUG_LOGGING_ENABLED.store(true, Ordering::Release);
    Ok(())
}

/// Fast check if debug logging is enabled (zero-cost when disabled)
///
/// Use this for early-bail in hot paths before constructing log entries.
#[inline(always)]
pub fn is_debug_logging_enabled() -> bool {
    DEBUG_LOGGING_ENABLED.load(Ordering::Relaxed)
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

/// Helper macro for logging debug entries with zero-cost when disabled.
///
/// This macro first checks a fast atomic bool before evaluating any arguments,
/// making it truly zero-cost when debug logging is not initialized.
///
/// # Examples
/// ```ignore
/// tark_debug_log!(correlation_id, LogCategory::Service, "event_name");
/// tark_debug_log!(correlation_id, LogCategory::Service, "event_name", json!({"key": "value"}));
/// ```
#[macro_export]
macro_rules! tark_debug_log {
    ($correlation_id:expr, $category:expr, $event:expr) => {
        // Fast path: check atomic bool first (single load instruction)
        if $crate::is_debug_logging_enabled() {
            if let Some(logger) = $crate::debug_logger() {
                logger.log($crate::DebugLogEntry::new(
                    $correlation_id,
                    $category,
                    $event,
                ));
            }
        }
    };
    ($correlation_id:expr, $category:expr, $event:expr, $data:expr) => {
        // Fast path: check atomic bool first (single load instruction)
        if $crate::is_debug_logging_enabled() {
            if let Some(logger) = $crate::debug_logger() {
                logger.log(
                    $crate::DebugLogEntry::new($correlation_id, $category, $event).with_data($data),
                );
            }
        }
    };
}
