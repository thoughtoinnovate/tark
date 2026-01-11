//! Agent tools for file system and shell operations
//!
//! Tools are organized by risk level:
//! - `readonly/`: Safe read-only tools (RiskLevel::ReadOnly)
//! - `write/`: Write tools within working directory (RiskLevel::Write)
//! - `risky/`: Shell commands and external operations (RiskLevel::Risky)
//! - `dangerous/`: Destructive operations (RiskLevel::Dangerous)

#![allow(dead_code)]

// Tool category modules
pub mod approval;
pub mod dangerous;
pub mod plan;
pub mod readonly;
pub mod risk;
pub mod risky;
pub mod write;

// Legacy modules (to be reorganized)
mod file_ops;
mod file_search;
mod grep;
mod lsp_tools;
mod mode_switch;
pub mod questionnaire;
mod shell;

pub use file_ops::{
    DeleteFileTool, ListDirectoryTool, PatchFileTool, ProposeChangeTool, ReadFileTool,
    ReadFilesTool, WriteFileTool,
};
pub use file_search::{CodebaseOverviewTool, FileSearchTool};
pub use grep::FindReferencesTool;
pub use lsp_tools::{
    set_lsp_proxy_port, CallHierarchyTool, CodeAnalyzer, FindAllReferencesTool, GetSignatureTool,
    GoToDefinitionTool, ListSymbolsTool,
};
pub use mode_switch::ModeSwitchTool;
pub use plan::{
    GetPlanStatusTool, MarkTaskDoneTool, PreviewPlanTool, SavePlanTool, UpdatePlanTool,
};
pub use questionnaire::{
    interaction_channel, ApprovalChoice, ApprovalPattern, ApprovalRequest, ApprovalResponse,
    AskUserTool, InteractionReceiver, InteractionRequest, InteractionSender, SuggestedPattern,
};
pub use readonly::{FilePreviewTool, RipgrepTool, SafeShellTool};
pub use risk::{MatchType, RiskLevel, TrustLevel};
pub use shell::ShellTool;

use crate::llm::ToolDefinition;
use crate::services::PlanService;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Agent mode determines which tools are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Ask mode: read-only tools for exploring and answering questions
    Ask,
    /// Plan mode: read-only tools + propose_change for planning
    Plan,
    /// Build mode: all tools for executing changes
    #[default]
    Build,
}

impl AgentMode {
    /// Get display label for this mode
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Plan => "Plan",
            Self::Build => "Build",
        }
    }

    /// Get icon for this mode
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Ask => "❓",
            Self::Plan => "📋",
            Self::Build => "🔨",
        }
    }

    /// Get description for this mode
    pub fn description(&self) -> &'static str {
        match self {
            Self::Ask => "Read-only exploration and Q&A",
            Self::Plan => "Read + propose changes (no execution)",
            Self::Build => "Full access to read, write, and execute",
        }
    }
}

/// Result of executing a tool
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub success: bool,
    pub output: String,
}

impl ToolResult {
    pub fn success(output: impl Into<String>) -> Self {
        Self {
            success: true,
            output: output.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            success: false,
            output: message.into(),
        }
    }
}

/// Trait for agent tools
#[async_trait]
pub trait Tool: Send + Sync {
    /// Get the tool name
    fn name(&self) -> &str;

    /// Get the tool description
    fn description(&self) -> &str;

    /// Get the JSON schema for parameters
    fn parameters(&self) -> Value;

    /// Execute the tool with given parameters
    async fn execute(&self, params: Value) -> Result<ToolResult>;

    /// Get the risk level for this tool (default: ReadOnly)
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    /// Get the command string for approval (used for pattern matching)
    /// Default implementation returns the first string parameter value.
    fn get_command_string(&self, params: &Value) -> String {
        // Try common parameter names
        for key in &["command", "path", "pattern"] {
            if let Some(val) = params.get(key).and_then(|v| v.as_str()) {
                return val.to_string();
            }
        }
        // Fallback to JSON string
        params.to_string()
    }

    /// Convert to LLM tool definition
    fn to_definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: self.parameters(),
        }
    }
}

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    working_dir: PathBuf,
    mode: AgentMode,
    /// Approval gate for risky operations
    approval_gate: Option<tokio::sync::Mutex<approval::ApprovalGate>>,
}

impl ToolRegistry {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            tools: HashMap::new(),
            working_dir,
            mode: AgentMode::Build,
            approval_gate: None,
        }
    }

    /// Create a registry with all default tools
    pub fn with_defaults(working_dir: PathBuf, shell_enabled: bool) -> Self {
        Self::for_mode(working_dir, AgentMode::Build, shell_enabled)
    }

    /// Create a registry for a specific agent mode
    pub fn for_mode(working_dir: PathBuf, mode: AgentMode, shell_enabled: bool) -> Self {
        Self::for_mode_with_interaction(working_dir, mode, shell_enabled, None)
    }

    /// Create a registry for a specific agent mode with optional interaction channel
    ///
    /// When `interaction_tx` is provided, the `ask_user` tool is registered and can
    /// be used by the agent to ask structured questions to the user via TUI popup.
    /// The approval gate is also created to handle approval requests for risky operations.
    pub fn for_mode_with_interaction(
        working_dir: PathBuf,
        mode: AgentMode,
        shell_enabled: bool,
        interaction_tx: Option<InteractionSender>,
    ) -> Self {
        Self::for_mode_with_services(working_dir, mode, shell_enabled, interaction_tx, None)
    }

    /// Create a registry with full service support including plan tools
    ///
    /// When `plan_service` is provided, plan management tools are registered:
    /// - `save_plan`: Available in Plan mode only
    /// - `mark_task_done`: Available in Plan and Build modes
    pub fn for_mode_with_services(
        working_dir: PathBuf,
        mode: AgentMode,
        shell_enabled: bool,
        interaction_tx: Option<InteractionSender>,
        plan_service: Option<Arc<PlanService>>,
    ) -> Self {
        // Create approval gate if we have an interaction channel
        let approval_gate = interaction_tx.as_ref().map(|tx| {
            let tark_dir = working_dir.join(".tark");
            tokio::sync::Mutex::new(approval::ApprovalGate::new(tark_dir, Some(tx.clone())))
        });

        let mut registry = Self {
            tools: HashMap::new(),
            working_dir: working_dir.clone(),
            mode,
            approval_gate,
        };

        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // ===== Read-only tools (available in ALL modes) =====

        // Code exploration
        registry.register(Arc::new(CodebaseOverviewTool::new(working_dir.clone()))); // Overview (use first!)
        registry.register(Arc::new(FindReferencesTool::new(working_dir.clone()))); // Code flow tracing
        registry.register(Arc::new(ReadFileTool::new(working_dir.clone())));
        registry.register(Arc::new(ReadFilesTool::new(working_dir.clone()))); // Batch read
        registry.register(Arc::new(ListDirectoryTool::new(working_dir.clone()))); // Directory listing
        registry.register(Arc::new(FileSearchTool::new(working_dir.clone())));

        // Search tools
        registry.register(Arc::new(RipgrepTool::new(working_dir.clone()))); // Fast grep replacement
        registry.register(Arc::new(FilePreviewTool::new(working_dir.clone()))); // Large file preview

        // LSP-powered tools for code understanding
        registry.register(Arc::new(ListSymbolsTool::new(working_dir.clone()))); // List symbols in file/dir
        registry.register(Arc::new(GoToDefinitionTool::new(working_dir.clone()))); // Jump to definition
        registry.register(Arc::new(FindAllReferencesTool::new(working_dir.clone()))); // Find all usages
        registry.register(Arc::new(CallHierarchyTool::new(working_dir.clone()))); // Trace call flow
        registry.register(Arc::new(GetSignatureTool::new(working_dir.clone()))); // Get function signature

        // Ask user tool (available in all modes when interaction channel is provided)
        if let Some(ref tx) = interaction_tx {
            registry.register(Arc::new(AskUserTool::new(Some(tx.clone()))));
            // Mode switch tool - allows agent to request mode changes with user confirmation
            registry.register(Arc::new(ModeSwitchTool::new(Some(tx.clone()))));
            tracing::debug!("Registered: ask_user, switch_mode (interaction channel provided)");
        }

        // ===== Mode-specific tools =====
        match mode {
            AgentMode::Ask => {
                // Ask mode: only safe shell (allowlisted commands)
                registry.register(Arc::new(SafeShellTool::new(working_dir)));
                tracing::debug!("Ask mode: registered safe_shell only");
            }
            AgentMode::Plan => {
                // Plan mode: safe shell + propose_change + all plan tools
                registry.register(Arc::new(SafeShellTool::new(working_dir.clone())));
                registry.register(Arc::new(ProposeChangeTool::new(working_dir.clone())));

                // Register plan tools if service is available
                if let Some(ref service) = plan_service {
                    // Preview plan (draft without saving)
                    registry.register(Arc::new(PreviewPlanTool::new()));
                    // Save plan (persist to storage)
                    registry.register(Arc::new(SavePlanTool::new(service.clone())));
                    // Update existing plan
                    registry.register(Arc::new(UpdatePlanTool::new(service.clone())));
                    // Get current plan status
                    registry.register(Arc::new(GetPlanStatusTool::new(service.clone())));
                    // Mark tasks complete
                    registry.register(Arc::new(MarkTaskDoneTool::new(
                        service.clone(),
                        interaction_tx.as_ref().cloned(),
                    )));
                    tracing::debug!(
                        "Plan mode: registered safe_shell, propose_change, preview_plan, \
                        save_plan, update_plan, get_plan_status, mark_task_done"
                    );
                } else {
                    tracing::debug!(
                        "Plan mode: registered safe_shell and propose_change (no plan service)"
                    );
                }
            }
            AgentMode::Build => {
                // Build mode: all write tools + plan tracking tools
                registry.register(Arc::new(WriteFileTool::new(working_dir.clone())));
                registry.register(Arc::new(PatchFileTool::new(working_dir.clone())));
                registry.register(Arc::new(DeleteFileTool::new(working_dir.clone())));

                // Register plan tracking tools if service is available
                if let Some(ref service) = plan_service {
                    // Get plan status (to check current task)
                    registry.register(Arc::new(GetPlanStatusTool::new(service.clone())));
                    // Mark tasks complete
                    registry.register(Arc::new(MarkTaskDoneTool::new(
                        service.clone(),
                        interaction_tx.as_ref().cloned(),
                    )));
                    tracing::debug!(
                        "Build mode: registered get_plan_status, mark_task_done for plan tracking"
                    );
                }

                if shell_enabled {
                    registry.register(Arc::new(ShellTool::new(working_dir)));
                    tracing::debug!(
                        "Build mode: registered write_file, patch_file, delete_file, shell"
                    );
                } else {
                    tracing::debug!("Build mode: registered write_file, patch_file, delete_file (shell disabled)");
                }
            }
        }

        let tool_names: Vec<_> = registry.tools.keys().cloned().collect();
        tracing::debug!("Tool registry created with tools: {:?}", tool_names);

        registry
    }

    /// Register a tool
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// Get a tool by name
    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    /// Execute a tool by name with given parameters
    ///
    /// If an approval gate is configured and the tool's risk level requires approval,
    /// the user will be prompted before execution.
    pub async fn execute(&self, name: &str, params: Value) -> Result<ToolResult> {
        let Some(tool) = self.tools.get(name) else {
            return Ok(ToolResult::error(format!("Unknown tool: {}", name)));
        };

        // Check approval if we have an approval gate
        if let Some(ref gate_mutex) = self.approval_gate {
            let risk_level = tool.risk_level();

            // Build a command string for display/pattern matching
            let command = self.build_command_string(name, &params);

            tracing::debug!(
                "Checking approval for tool '{}' (risk: {:?}), command: {}",
                name,
                risk_level,
                command
            );

            // Check with approval gate
            let mut gate = gate_mutex.lock().await;
            tracing::debug!(
                "Trust level: {} {:?}",
                gate.trust_level.icon(),
                gate.trust_level
            );

            match gate.check_and_approve(name, &command, risk_level).await {
                Ok(approval::ApprovalStatus::Approved) => {
                    tracing::debug!("Tool '{}' approved", name);
                    // Proceed with execution
                }
                Ok(approval::ApprovalStatus::Denied) => {
                    tracing::info!("Tool '{}' denied by user", name);
                    return Ok(ToolResult::error(format!(
                        "Operation denied by user: {} {}",
                        name, command
                    )));
                }
                Ok(approval::ApprovalStatus::Blocked(reason)) => {
                    tracing::info!("Tool '{}' blocked: {}", name, reason);
                    return Ok(ToolResult::error(format!("Operation blocked: {}", reason)));
                }
                Err(e) => {
                    tracing::warn!("Approval check failed, auto-approving: {}", e);
                    // Continue with execution on error (fail-open for usability)
                }
            }
        } else {
            tracing::debug!("No approval gate configured for tool '{}'", name);
        }

        tool.execute(params).await
    }

    /// Build a human-readable command string from tool name and params
    fn build_command_string(&self, name: &str, params: &Value) -> String {
        match name {
            "shell" => params
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string(),
            "write_file" | "read_file" | "delete_file" => params
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("(unknown)")
                .to_string(),
            "patch_file" => {
                let path = params
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                format!("patch {}", path)
            }
            "grep" => {
                let pattern = params
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                format!("grep '{}'", pattern)
            }
            _ => format!("{} {:?}", name, params),
        }
    }

    /// Get all tool definitions for LLM
    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().map(|t| t.to_definition()).collect()
    }

    /// Get working directory
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Get the current agent mode
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Check if a tool requires approval based on its risk level
    pub fn tool_risk_level(&self, name: &str) -> Option<RiskLevel> {
        self.tools.get(name).map(|t| t.risk_level())
    }

    /// Set the trust level for the registry's approval gate
    pub async fn set_trust_level(&self, level: TrustLevel) {
        if let Some(ref gate_mutex) = self.approval_gate {
            let mut gate = gate_mutex.lock().await;
            gate.set_trust_level(level);
            tracing::info!(
                "Trust level set to: {} {} (gate present)",
                level.icon(),
                level.label()
            );
        } else {
            tracing::warn!(
                "Cannot set trust level to {} - no approval gate configured",
                level.label()
            );
        }
    }

    /// Get the current trust level
    pub async fn trust_level(&self) -> TrustLevel {
        if let Some(ref gate_mutex) = self.approval_gate {
            let gate = gate_mutex.lock().await;
            gate.trust_level
        } else {
            TrustLevel::default()
        }
    }
}
