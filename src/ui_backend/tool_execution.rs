//! Tool Execution Service - Manages tool availability and approvals
//!
//! Provides tool introspection, mode-based availability, and approval integration.

use crate::core::types::AgentMode;
use crate::tools::{RiskLevel, ToolRegistry};

/// Tool information for UI display
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub category: crate::tools::ToolCategory,
    pub available_in_modes: Vec<AgentMode>,
}

/// Tool Execution Service
///
/// Manages tool introspection, availability by mode, and approval integration.
pub struct ToolExecutionService {
    current_mode: AgentMode,
}

impl ToolExecutionService {
    /// Create a new tool execution service
    pub fn new(mode: AgentMode) -> Self {
        Self { current_mode: mode }
    }

    // === Introspection ===

    /// List all tools available in a specific mode
    pub fn list_tools(&self, mode: AgentMode) -> Vec<ToolInfo> {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let registry = ToolRegistry::for_mode(working_dir, mode, true);

        let definitions = registry.definitions();

        definitions
            .into_iter()
            .map(|def| {
                let tool = registry.get(&def.name);
                ToolInfo {
                    name: def.name.clone(),
                    description: def.description.clone(),
                    risk_level: registry
                        .tool_risk_level(&def.name)
                        .unwrap_or(RiskLevel::ReadOnly),
                    category: tool
                        .map(|t| t.category())
                        .unwrap_or(crate::tools::ToolCategory::Core),
                    available_in_modes: vec![mode], // Simplified - would check all modes
                }
            })
            .collect()
    }

    /// Get risk level for a specific tool
    pub fn tool_risk_level(&self, name: &str) -> Option<RiskLevel> {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let registry = ToolRegistry::for_mode(working_dir, self.current_mode, true);
        registry.tool_risk_level(name)
    }

    /// Check if a tool is available in a specific mode
    pub fn is_available(&self, name: &str, mode: AgentMode) -> bool {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let registry = ToolRegistry::for_mode(working_dir, mode, true);
        registry.get(name).is_some()
    }

    /// Get tool description
    pub fn tool_description(&self, name: &str) -> Option<String> {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let registry = ToolRegistry::for_mode(working_dir, self.current_mode, true);
        registry.get(name).map(|t| t.description().to_string())
    }

    /// Update the current mode
    pub fn set_mode(&mut self, mode: AgentMode) {
        self.current_mode = mode;
    }

    /// Get the current mode
    pub fn mode(&self) -> AgentMode {
        self.current_mode
    }
}
