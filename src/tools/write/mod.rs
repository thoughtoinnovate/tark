//! Write tools with RiskLevel::Write.
//!
//! These tools can modify files within the working directory.
//! They are available in Build mode (and propose_change in Plan mode).

use crate::tools::risk::RiskLevel;

/// Get the risk level for all tools in this module.
pub fn risk_level() -> RiskLevel {
    RiskLevel::Write
}

// Note: WriteFileTool, PatchFileTool, and ProposeChangeTool are defined in
// src/tools/file_ops.rs and will be registered directly from ToolRegistry.
// This module serves as documentation and future home for these tools.
