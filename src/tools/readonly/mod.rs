//! Read-only tools with RiskLevel::ReadOnly.
//!
//! These tools only read from the filesystem and cannot make any modifications.
//! They are available in all agent modes (Ask, Plan, Build).

mod file_preview;
mod ripgrep;
mod safe_shell;

// Re-export tools
pub use file_preview::FilePreviewTool;
pub use ripgrep::RipgrepTool;
pub use safe_shell::SafeShellTool;

// Re-export from parent module (existing tools that are read-only)
// These will be moved here in a future refactor

use crate::tools::risk::RiskLevel;
use crate::tools::ToolRegistry;
use std::path::Path;
use std::sync::Arc;

/// Register all read-only tools with the registry.
pub fn register_all(registry: &mut ToolRegistry, working_dir: &Path) {
    // New tools
    registry.register(Arc::new(RipgrepTool::new(working_dir.to_path_buf())));
    registry.register(Arc::new(FilePreviewTool::new(working_dir.to_path_buf())));
    // SafeShellTool is only registered in Ask mode (handled in ToolRegistry::for_mode)
}

/// Get the risk level for all tools in this module.
pub fn risk_level() -> RiskLevel {
    RiskLevel::ReadOnly
}
