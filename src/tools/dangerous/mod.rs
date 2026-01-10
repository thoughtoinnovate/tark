//! Dangerous tools with RiskLevel::Dangerous.
//!
//! These tools can permanently delete files or perform other destructive operations.
//! They always require approval (except in Auto mode which we don't have).

use crate::tools::risk::RiskLevel;

/// Get the risk level for all tools in this module.
pub fn risk_level() -> RiskLevel {
    RiskLevel::Dangerous
}

// Note: DeleteFileTool is defined in src/tools/file_ops.rs and will be
// registered directly from ToolRegistry with approval gating.
// This module serves as documentation and future home for these tools.
