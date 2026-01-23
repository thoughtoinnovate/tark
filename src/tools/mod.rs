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
pub mod builtin;
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
    interaction_channel, ApprovalPattern, AskUserTool, InteractionReceiver, InteractionSender,
};
#[allow(unused_imports)]
pub use questionnaire::{
    ApprovalChoice, ApprovalRequest, ApprovalResponse, InteractionRequest, SuggestedPattern,
};
pub use readonly::{FilePreviewTool, RipgrepTool, SafeShellTool};
pub use risk::{MatchType, RiskLevel, TrustLevel};
pub use shell::ShellTool;

// Built-in extra tools
#[allow(unused_imports)]
pub use builtin::TodoItem;
pub use builtin::{
    MemoryDeleteTool, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory, ThinkTool,
    ThinkingTracker, TodoTool, TodoTracker,
};

use crate::llm::ToolDefinition;
use crate::policy::PolicyEngine;
use crate::services::PlanService;
use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use serde_json::Value;
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// Re-export canonical AgentMode from core::types
pub use crate::core::types::AgentMode;

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

/// Tool category for UI organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolCategory {
    /// Core tools: file_read, grep, shell, etc.
    #[default]
    Core,
    /// Built-in extra tools: think, memory_*
    Builtin,
    /// External MCP server tools
    External,
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

    /// Get the category for this tool (default: Core)
    fn category(&self) -> ToolCategory {
        ToolCategory::Core
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
    /// **DEPRECATED**: Use `policy_engine` instead. Kept for backward compatibility.
    #[allow(deprecated)]
    approval_gate: Option<tokio::sync::Mutex<approval::ApprovalGate>>,
    /// Policy engine for approval decisions (NEW)
    policy_engine: Option<Arc<PolicyEngine>>,
    /// Current session ID for tracking patterns
    session_id: String,
}

impl ToolRegistry {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            tools: HashMap::new(),
            working_dir,
            mode: AgentMode::Build,
            approval_gate: None,
            policy_engine: None,
            session_id: uuid::Uuid::new_v4().to_string(),
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
        Self::for_mode_with_services(
            working_dir,
            mode,
            shell_enabled,
            interaction_tx,
            None,
            None,
            None,
        )
    }

    /// Create a registry with full service support including plan tools
    ///
    /// When `plan_service` is provided, plan management tools are registered:
    /// - `save_plan`: Available in Plan mode only
    /// - `mark_task_done`: Available in Plan and Build modes
    ///
    /// When `todo_tracker` is provided, the todo tool will use that shared tracker.
    /// Otherwise, a new tracker is created.
    pub fn for_mode_with_services(
        working_dir: PathBuf,
        mode: AgentMode,
        shell_enabled: bool,
        interaction_tx: Option<InteractionSender>,
        plan_service: Option<Arc<PlanService>>,
        approvals_path: Option<PathBuf>,
        todo_tracker: Option<Arc<Mutex<TodoTracker>>>,
    ) -> Self {
        // Create approval gate if we have an interaction channel
        // DEPRECATED: ApprovalGate is deprecated in favor of PolicyEngine
        #[allow(deprecated)]
        let approval_gate = interaction_tx.as_ref().map(|tx| {
            let storage_path = approvals_path
                .clone()
                .unwrap_or_else(|| working_dir.join(".tark").join("approvals.json"));
            tokio::sync::Mutex::new(approval::ApprovalGate::new(storage_path, Some(tx.clone())))
        });

        // Initialize PolicyEngine (NEW)
        let policy_engine = {
            let db_path = working_dir.join(".tark").join("policy.db");
            match PolicyEngine::open(&db_path, &working_dir) {
                Ok(engine) => {
                    tracing::info!("PolicyEngine initialized at {:?}", db_path);
                    Some(Arc::new(engine))
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to initialize PolicyEngine: {}, falling back to ApprovalGate",
                        e
                    );
                    None
                }
            }
        };

        let mut registry = Self {
            tools: HashMap::new(),
            working_dir: working_dir.clone(),
            mode,
            approval_gate,
            policy_engine,
            session_id: uuid::Uuid::new_v4().to_string(),
        };

        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // ===== Built-in extra tools (available in ALL modes) =====

        // Thinking tool - always available, helps with reasoning
        let thinking_tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        registry.register(Arc::new(ThinkTool::new(thinking_tracker)));

        // Memory tools - persistent storage across sessions
        let memory_db_path = working_dir.join(".tark").join("memory.db");
        match TarkMemory::open(&memory_db_path) {
            Ok(memory) => {
                let memory = Arc::new(memory);
                registry.register(Arc::new(MemoryStoreTool::new(memory.clone())));
                registry.register(Arc::new(MemoryQueryTool::new(memory.clone())));
                registry.register(Arc::new(MemoryListTool::new(memory.clone())));
                registry.register(Arc::new(MemoryDeleteTool::new(memory)));
                tracing::debug!("Registered memory tools (db: {})", memory_db_path.display());
            }
            Err(e) => {
                tracing::warn!("Failed to initialize memory tools: {}", e);
            }
        }

        // Todo tool - session-scoped task tracking
        let todo_tracker = todo_tracker.unwrap_or_else(|| Arc::new(Mutex::new(TodoTracker::new())));
        registry.register(Arc::new(TodoTool::new(todo_tracker)));
        tracing::debug!("Registered todo tool");

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
    /// If an approval gate or policy engine is configured and the tool's risk level requires approval,
    /// the user will be prompted before execution.
    pub async fn execute(&self, name: &str, params: Value) -> Result<ToolResult> {
        let Some(tool) = self.tools.get(name) else {
            return Ok(ToolResult::error(format!("Unknown tool: {}", name)));
        };

        // Build command string for display/pattern matching
        let command = self.build_command_string(name, &params);

        // NEW: Try PolicyEngine first
        if let Some(ref engine) = self.policy_engine {
            let mode_id = match self.mode {
                AgentMode::Ask => "ask",
                AgentMode::Plan => "plan",
                AgentMode::Build => "build",
            };

            let trust_id = self.get_trust_level_id();

            match engine.check_approval(name, &command, mode_id, &trust_id, &self.session_id) {
                Ok(decision) => {
                    tracing::debug!(
                        "PolicyEngine decision for '{}': needs_approval={}, allow_save={}",
                        name,
                        decision.needs_approval,
                        decision.allow_save_pattern
                    );

                    if !decision.needs_approval {
                        // Auto-approved - proceed with execution
                        tracing::debug!("Tool '{}' auto-approved by PolicyEngine", name);
                    } else {
                        // Need approval - should be handled by caller via interaction channel
                        // For now, we fall through to old approval gate
                        tracing::warn!("PolicyEngine requires approval but interaction flow not yet integrated for '{}'", name);
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "PolicyEngine check failed for '{}': {}, falling back to approval gate",
                        name,
                        e
                    );
                }
            }
        }

        // FALLBACK: Check approval if we have an approval gate (deprecated path)
        // This fallback exists for backward compatibility only
        #[allow(deprecated)]
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
                    let request = gate.create_approval_request(name, &command, risk_level);
                    let suggestions = serde_json::to_string_pretty(&request.suggested_patterns)
                        .unwrap_or_else(|_| "[]".to_string());
                    return Ok(ToolResult::error(format!(
                        "Operation denied by user: {} {}\nSuggested patterns: {}",
                        name, command, suggestions
                    )));
                }
                Ok(approval::ApprovalStatus::Blocked(reason)) => {
                    tracing::info!("Tool '{}' blocked: {}", name, reason);
                    let request = gate.create_approval_request(name, &command, risk_level);
                    let suggestions = serde_json::to_string_pretty(&request.suggested_patterns)
                        .unwrap_or_else(|_| "[]".to_string());
                    return Ok(ToolResult::error(format!(
                        "Operation blocked: {}\nSuggested patterns: {}",
                        reason, suggestions
                    )));
                }
                Err(e) => {
                    tracing::warn!("Approval check failed, auto-approving: {}", e);
                    // Continue with execution on error (fail-open for usability)
                }
            }
        } else {
            tracing::debug!("No approval gate configured for tool '{}'", name);
        }

        // Wrap tool execution with panic recovery to prevent crashes
        match AssertUnwindSafe(tool.execute(params)).catch_unwind().await {
            Ok(result) => result,
            Err(panic_info) => {
                // Extract panic message
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                tracing::error!("Tool '{}' panicked: {}", name, panic_msg);
                Ok(ToolResult::error(format!(
                    "Tool '{}' crashed: {}. Please report this bug.",
                    name, panic_msg
                )))
            }
        }
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
    /// **DEPRECATED**: Use PolicyEngine trust levels instead
    #[deprecated(since = "0.8.0", note = "Use PolicyEngine trust levels instead")]
    pub async fn set_trust_level(&self, level: TrustLevel) {
        #[allow(deprecated)]
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

    /// Update approval storage path (per session)
    /// **DEPRECATED**: Use PolicyEngine pattern storage instead
    #[deprecated(since = "0.8.0", note = "Use PolicyEngine pattern storage instead")]
    pub async fn set_approval_storage_path(&self, storage_path: PathBuf) {
        #[allow(deprecated)]
        if let Some(ref gate_mutex) = self.approval_gate {
            let mut gate = gate_mutex.lock().await;
            gate.set_storage_path(storage_path);
        }
    }

    /// Get trust level ID as string for PolicyEngine
    fn get_trust_level_id(&self) -> String {
        // For now, default to "balanced" - will be properly managed via set_trust_level
        // TODO: Store trust_level in ToolRegistry directly and sync with approval_gate
        "balanced".to_string()
    }
}
