//! Tool Execution Service - Manages tool availability and approvals
//!
//! Provides tool introspection, mode-based availability, and approval integration.

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::core::types::AgentMode;
use crate::tools::{
    approval::{ApprovalGate, ApprovalStatus},
    ApprovalPattern, RiskLevel, ToolRegistry, TrustLevel,
};

use super::errors::ToolError;

/// Tool information for UI display
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub risk_level: RiskLevel,
    pub available_in_modes: Vec<AgentMode>,
}

/// Tool Execution Service
///
/// Manages tool introspection, availability by mode, and approval integration.
pub struct ToolExecutionService {
    current_mode: AgentMode,
    approval_gate: Option<Arc<Mutex<ApprovalGate>>>,
}

impl ToolExecutionService {
    /// Create a new tool execution service
    pub fn new(mode: AgentMode, approval_gate: Option<Arc<Mutex<ApprovalGate>>>) -> Self {
        Self {
            current_mode: mode,
            approval_gate,
        }
    }

    // === Introspection ===

    /// List all tools available in a specific mode
    pub fn list_tools(&self, mode: AgentMode) -> Vec<ToolInfo> {
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        let registry = ToolRegistry::for_mode(working_dir, mode, true);

        let definitions = registry.definitions();

        definitions
            .into_iter()
            .map(|def| ToolInfo {
                name: def.name.clone(),
                description: def.description.clone(),
                risk_level: registry
                    .tool_risk_level(&def.name)
                    .unwrap_or(RiskLevel::ReadOnly),
                available_in_modes: vec![mode], // Simplified - would check all modes
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

    // === Approval ===

    /// Get the current trust level
    pub async fn trust_level(&self) -> TrustLevel {
        if let Some(ref gate) = self.approval_gate {
            let gate_guard = gate.lock().await;
            gate_guard.trust_level
        } else {
            TrustLevel::default()
        }
    }

    /// Set the trust level
    pub async fn set_trust_level(&self, level: TrustLevel) {
        if let Some(ref gate) = self.approval_gate {
            let mut gate_guard = gate.lock().await;
            gate_guard.set_trust_level(level);
            tracing::info!("Trust level set to: {} {}", level.icon(), level.label());
        } else {
            tracing::warn!("Cannot set trust level - no approval gate configured");
        }
    }

    /// Check if an operation needs approval
    pub async fn check_approval(
        &self,
        tool: &str,
        command: &str,
        risk: RiskLevel,
    ) -> Result<ApprovalStatus, ToolError> {
        if let Some(ref gate) = self.approval_gate {
            let mut gate_guard = gate.lock().await;
            gate_guard
                .check_and_approve(tool, command, risk)
                .await
                .map_err(ToolError::Other)
        } else {
            // No approval gate - auto-approve
            Ok(ApprovalStatus::Approved)
        }
    }

    // === Pattern Management ===

    /// Get persistent approval patterns
    pub async fn get_persistent_approvals(&self) -> Vec<ApprovalPattern> {
        if let Some(ref gate) = self.approval_gate {
            let gate_guard = gate.lock().await;
            gate_guard.get_persistent_approvals().to_vec()
        } else {
            vec![]
        }
    }

    /// Remove a persistent approval pattern by index
    pub async fn remove_persistent_approval(&self, index: usize) -> Result<(), ToolError> {
        if let Some(ref gate) = self.approval_gate {
            let mut gate_guard = gate.lock().await;
            gate_guard
                .remove_persistent_approval(index)
                .map_err(ToolError::Other)
        } else {
            Err(ToolError::Other(anyhow::anyhow!(
                "No approval gate configured"
            )))
        }
    }

    /// Remove a persistent denial pattern by index
    pub async fn remove_persistent_denial(&self, index: usize) -> Result<(), ToolError> {
        if let Some(ref gate) = self.approval_gate {
            let mut gate_guard = gate.lock().await;
            gate_guard
                .remove_persistent_denial(index)
                .map_err(ToolError::Other)
        } else {
            Err(ToolError::Other(anyhow::anyhow!(
                "No approval gate configured"
            )))
        }
    }

    /// Clear session patterns (non-persisted)
    pub async fn clear_session(&self) {
        if let Some(ref gate) = self.approval_gate {
            let mut gate_guard = gate.lock().await;
            gate_guard.clear_session();
        }
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
