//! Risky tools with RiskLevel::Risky.
//!
//! These tools can execute arbitrary shell commands and interact with external systems.
//! They require approval in Ask Risky and Ask All modes.

use crate::tools::risk::RiskLevel;

/// Get the risk level for all tools in this module.
pub fn risk_level() -> RiskLevel {
    RiskLevel::Risky
}

// Note: ShellTool is defined in src/tools/shell.rs and will be
// registered directly from ToolRegistry with approval gating.
// This module serves as documentation and future home for these tools.
