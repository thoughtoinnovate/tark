//! Agent tools for file system and shell operations
//!
//! Tools are organized by risk level:
//! - `readonly/`: Safe read-only tools (RiskLevel::ReadOnly)
//! - `write/`: Write tools within working directory (RiskLevel::Write)
//! - `risky/`: Shell commands and external operations (RiskLevel::Risky)
//! - `dangerous/`: Destructive operations (RiskLevel::Dangerous)

#![allow(dead_code)]

// Tool category modules
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
use crate::policy::{ModeId, PolicyEngine, ToolPolicyMetadata, TrustId};
use crate::services::PlanService;
use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time::timeout;

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

    /// Get policy metadata for this tool (self-declaration pattern)
    /// Tools can override this to declare their own risk level, operations, and mode availability.
    /// This provides a more declarative approach to policy configuration.
    fn policy_metadata(&self) -> Option<ToolPolicyMetadata> {
        // Default: tools don't declare metadata, rely on external config
        // Individual tools can override this to self-declare their policies
        None
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
    tool_timeout_secs: u64,
    /// Policy engine for approval decisions
    policy_engine: Option<Arc<PolicyEngine>>,
    /// Current session ID for tracking patterns
    session_id: String,
    /// Current trust level for approval decisions
    trust_level: crate::policy::types::TrustId,
    /// Interaction channel for approval requests
    interaction_tx: Option<InteractionSender>,
}

impl ToolRegistry {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            tools: HashMap::new(),
            working_dir,
            mode: AgentMode::Build,
            tool_timeout_secs: 60,
            policy_engine: None,
            session_id: uuid::Uuid::new_v4().to_string(),
            trust_level: crate::policy::types::TrustId::default(),
            interaction_tx: None,
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
    ///
    /// When `thinking_tracker` is provided, the think tool will use that shared tracker.
    /// Otherwise, a new tracker is created.
    pub fn for_mode_with_services(
        working_dir: PathBuf,
        mode: AgentMode,
        shell_enabled: bool,
        interaction_tx: Option<InteractionSender>,
        plan_service: Option<Arc<PlanService>>,
        todo_tracker: Option<Arc<Mutex<TodoTracker>>>,
        thinking_tracker: Option<Arc<Mutex<ThinkingTracker>>>,
    ) -> Self {
        // Initialize PolicyEngine
        let policy_engine = {
            let db_path = working_dir.join(".tark").join("policy.db");
            match PolicyEngine::open(&db_path, &working_dir) {
                Ok(engine) => {
                    tracing::info!("PolicyEngine initialized at {:?}", db_path);
                    Some(Arc::new(engine))
                }
                Err(e) => {
                    tracing::error!("Failed to initialize PolicyEngine: {}", e);
                    None
                }
            }
        };

        let mut registry = Self {
            tools: HashMap::new(),
            working_dir: working_dir.clone(),
            mode,
            tool_timeout_secs: 60,
            policy_engine,
            session_id: uuid::Uuid::new_v4().to_string(),
            trust_level: crate::policy::types::TrustId::default(),
            interaction_tx: interaction_tx.clone(),
        };

        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // ===== Built-in extra tools (available in ALL modes) =====

        // Thinking tool - always available, helps with reasoning
        // Use the provided tracker or create a new one
        let thinking_tracker =
            thinking_tracker.unwrap_or_else(|| Arc::new(Mutex::new(ThinkingTracker::new())));
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
                // Ask mode: safe shell + propose_change (read-only preview)
                registry.register(Arc::new(SafeShellTool::new(working_dir.clone())));
                registry.register(Arc::new(ProposeChangeTool::new(working_dir)));
                tracing::debug!("Ask mode: registered safe_shell, propose_change");
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

    /// Set the default tool timeout (seconds)
    pub fn set_tool_timeout_secs(&mut self, secs: u64) {
        self.tool_timeout_secs = secs;
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

        let timeout_secs = self
            .tool_timeout_override(&params)
            .unwrap_or(self.tool_timeout_secs);
        let timeout_duration = Duration::from_secs(timeout_secs);

        // Build command string for display/pattern matching
        let command = self.build_command_string(name, &params);

        // Check approval with PolicyEngine
        if let Some(ref engine) = self.policy_engine {
            let mode_id = match self.mode {
                AgentMode::Ask => ModeId::Ask,
                AgentMode::Plan => ModeId::Plan,
                AgentMode::Build => ModeId::Build,
            };

            let trust_id = self.get_trust_level_id();

            #[cfg(debug_assertions)]
            tracing::debug!(
                "ToolRegistry.execute: tool={}, trust_level={:?}, trust_id={}",
                name,
                self.trust_level,
                trust_id
            );

            match engine.check_approval(
                name,
                &command,
                mode_id.as_str(),
                trust_id.as_str(),
                &self.session_id,
            ) {
                Ok(decision) => {
                    // Log approval decision at info level for security audit
                    tracing::info!(
                        tool = %name,
                        trust = %trust_id.as_str(),
                        needs_approval = %decision.needs_approval,
                        classification = %decision.classification.classification_id,
                        in_workdir = %decision.classification.in_workdir,
                        operation = %decision.classification.operation,
                        "PolicyEngine approval decision"
                    );

                    if decision.needs_approval {
                        // Approval required - block and wait for user response
                        if let Some(ref tx) = self.interaction_tx {
                            tracing::info!(
                                "Tool '{}' requires approval, sending request to user",
                                name
                            );

                            // Build suggested patterns
                            let suggested_patterns = self.build_suggested_patterns(name, &command);

                            // Create approval request
                            let (responder, receiver) = tokio::sync::oneshot::channel();
                            let request = ApprovalRequest {
                                tool: name.to_string(),
                                command: command.clone(),
                                risk_level: tool.risk_level(),
                                suggested_patterns,
                            };

                            // Send request to UI
                            if let Err(e) = tx
                                .send(InteractionRequest::Approval { request, responder })
                                .await
                            {
                                tracing::error!("Failed to send approval request: {}", e);
                                return Ok(ToolResult::error(
                                    "Failed to request approval from user",
                                ));
                            }

                            // Wait for user response
                            match receiver.await {
                                Ok(response) => {
                                    tracing::debug!(
                                        "Received approval response: {:?}",
                                        response.choice
                                    );

                                    // Handle denial
                                    if matches!(
                                        response.choice,
                                        ApprovalChoice::Deny | ApprovalChoice::DenyAlways
                                    ) {
                                        tracing::info!("Tool '{}' denied by user", name);
                                        return Ok(ToolResult::error("Operation denied by user"));
                                    }

                                    // Handle pattern saving for session/always
                                    if let Some(pattern) = response.selected_pattern {
                                        if matches!(
                                            response.choice,
                                            ApprovalChoice::ApproveSession
                                                | ApprovalChoice::ApproveAlways
                                        ) {
                                            let is_persistent = matches!(
                                                response.choice,
                                                ApprovalChoice::ApproveAlways
                                            );
                                            if let Err(e) = self.save_approval_pattern(
                                                name,
                                                &pattern,
                                                is_persistent,
                                            ) {
                                                tracing::warn!(
                                                    "Failed to save approval pattern: {}",
                                                    e
                                                );
                                            } else {
                                                tracing::info!("Saved approval pattern for '{}': {} (persistent: {})", 
                                                    name, pattern.pattern, is_persistent);
                                            }
                                        }
                                    }

                                    tracing::info!("Tool '{}' approved by user", name);
                                    // Continue to execution below
                                }
                                Err(_) => {
                                    tracing::warn!("Approval request cancelled for '{}'", name);
                                    return Ok(ToolResult::error("Approval request cancelled"));
                                }
                            }
                        } else {
                            // No interaction channel - fail safe
                            tracing::error!(
                                "Approval required for '{}' but no interaction channel available",
                                name
                            );
                            return Ok(ToolResult::error(
                                "Approval required but no interaction channel available",
                            ));
                        }
                    } else {
                        // Auto-approved - proceed with execution
                        tracing::debug!("Tool '{}' auto-approved by PolicyEngine", name);
                    }
                }
                Err(e) => {
                    tracing::warn!("PolicyEngine check failed for '{}': {}", name, e);
                }
            }
        }

        // Wrap tool execution with timeout + panic recovery to prevent crashes
        match timeout(
            timeout_duration,
            AssertUnwindSafe(tool.execute(params)).catch_unwind(),
        )
        .await
        {
            Ok(Ok(result)) => result,
            Ok(Err(panic_info)) => {
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
            Err(_) => Ok(ToolResult::error(format!(
                "Tool '{}' timed out after {} seconds",
                name, timeout_secs
            ))),
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
        self.tools
            .values()
            .map(|t| Self::augment_tool_definition(t.to_definition()))
            .collect()
    }

    /// Get tool definitions filtered by current mode using PolicyEngine
    ///
    /// This queries the PolicyEngine's tool_mode_availability table to determine
    /// which tools are available in the current mode, ensuring the TOML config
    /// is the single source of truth for tool availability.
    pub fn definitions_for_mode(&self) -> Vec<ToolDefinition> {
        let available_tool_ids: HashSet<String> = if let Some(ref engine) = self.policy_engine {
            match engine.get_available_tools(&self.mode.to_string()) {
                Ok(tools) => tools.into_iter().map(|t| t.id).collect(),
                Err(e) => {
                    tracing::warn!(
                        "Failed to get available tools from PolicyEngine: {}. Falling back to all tools.",
                        e
                    );
                    return self.definitions();
                }
            }
        } else {
            // Fallback: return all tools if no policy engine
            tracing::debug!("No PolicyEngine available, returning all tool definitions");
            return self.definitions();
        };

        self.tools
            .values()
            .filter(|t| available_tool_ids.contains(t.name()))
            .map(|t| Self::augment_tool_definition(t.to_definition()))
            .collect()
    }

    /// Get working directory
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    /// Get the current agent mode
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Set the trust level for approval decisions
    pub fn set_trust_level(&mut self, level: TrustLevel) {
        self.trust_level = crate::policy::types::TrustId::from(level);
        tracing::debug!(
            "ToolRegistry trust level updated to: {:?}",
            self.trust_level
        );
    }

    /// Set the session ID for pattern tracking
    ///
    /// This should be called when switching sessions to ensure patterns
    /// are saved and retrieved using the correct session identifier.
    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = session_id;
        tracing::debug!("ToolRegistry session_id updated to: {}", self.session_id);
    }

    /// Get the current session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Check if a tool requires approval based on its risk level
    pub fn tool_risk_level(&self, name: &str) -> Option<RiskLevel> {
        self.tools.get(name).map(|t| t.risk_level())
    }

    fn tool_timeout_override(&self, params: &Value) -> Option<u64> {
        params.get("timeout_secs").and_then(|v| v.as_u64())
    }

    fn augment_tool_definition(mut def: ToolDefinition) -> ToolDefinition {
        let params = def.parameters.as_object_mut();
        if let Some(params) = params {
            if params
                .get("type")
                .and_then(|v| v.as_str())
                .map(|t| t == "object")
                .unwrap_or(false)
            {
                let properties = params.entry("properties").or_insert_with(|| json!({}));
                if let Some(props) = properties.as_object_mut() {
                    props.entry("timeout_secs".to_string()).or_insert_with(|| {
                        json!({
                            "type": "integer",
                            "description": "Optional tool timeout in seconds (overrides global default)"
                        })
                    });
                }
            }
        }
        def
    }

    /// Get trust level ID for PolicyEngine
    fn get_trust_level_id(&self) -> TrustId {
        self.trust_level
    }

    /// Build suggested approval patterns for a command
    fn build_suggested_patterns(&self, tool_name: &str, command: &str) -> Vec<SuggestedPattern> {
        vec![
            SuggestedPattern {
                pattern: command.to_string(),
                match_type: MatchType::Exact,
                description: format!("{} on exact: {}", tool_name, command),
            },
            SuggestedPattern {
                pattern: command
                    .split_whitespace()
                    .next()
                    .unwrap_or(command)
                    .to_string(),
                match_type: MatchType::Prefix,
                description: format!(
                    "{} starting with: {}",
                    tool_name,
                    command.split_whitespace().next().unwrap_or(command)
                ),
            },
            SuggestedPattern {
                pattern: "*".to_string(),
                match_type: MatchType::Glob,
                description: format!("{} (any file)", tool_name),
            },
        ]
    }

    /// Save an approval pattern to the policy database
    fn save_approval_pattern(
        &self,
        tool: &str,
        pattern: &ApprovalPattern,
        is_persistent: bool,
    ) -> Result<()> {
        if let Some(ref engine) = self.policy_engine {
            // Convert MatchType from tools::risk to policy::types
            let policy_match_type = match pattern.match_type {
                MatchType::Exact => crate::policy::types::MatchType::Exact,
                MatchType::Prefix => crate::policy::types::MatchType::Prefix,
                MatchType::Glob => crate::policy::types::MatchType::Glob,
            };

            // Convert tools::ApprovalPattern to policy::ApprovalPattern
            let policy_pattern = crate::policy::types::ApprovalPattern {
                id: None,
                tool: tool.to_string(),
                pattern: pattern.pattern.clone(),
                match_type: policy_match_type,
                is_denial: false,
                source: if is_persistent {
                    crate::policy::types::PatternSource::User
                } else {
                    crate::policy::types::PatternSource::Session
                },
                description: pattern.description.clone(),
                session_id: if is_persistent {
                    None
                } else {
                    Some(self.session_id.clone())
                },
            };
            engine.save_pattern(policy_pattern)?;
        }
        Ok(())
    }

    /// List session approval patterns (for UI display)
    pub fn list_session_patterns(
        &self,
        session_id: &str,
    ) -> Result<(
        Vec<crate::policy::ApprovalPatternEntry>,
        Vec<crate::policy::ApprovalPatternEntry>,
    )> {
        if let Some(ref engine) = self.policy_engine {
            engine.list_session_patterns(session_id)
        } else {
            Ok((Vec::new(), Vec::new()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::time;

    struct SleepTool {
        name: &'static str,
        duration: Duration,
    }

    #[async_trait]
    impl Tool for SleepTool {
        fn name(&self) -> &str {
            self.name
        }

        fn description(&self) -> &str {
            "sleep tool"
        }

        fn parameters(&self) -> Value {
            json!({
                "type": "object",
                "properties": {}
            })
        }

        async fn execute(&self, _params: Value) -> Result<ToolResult> {
            time::sleep(self.duration).await;
            Ok(ToolResult::success("done"))
        }
    }

    #[tokio::test]
    async fn tool_registry_enforces_timeout() {
        let mut registry = ToolRegistry::new(PathBuf::from("."));
        registry.register(Arc::new(SleepTool {
            name: "sleep",
            duration: Duration::from_secs(5),
        }));
        registry.set_tool_timeout_secs(1);

        let result = registry.execute("sleep", json!({})).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("timed out"));
    }

    #[tokio::test]
    async fn tool_registry_timeout_override_respected() {
        let mut registry = ToolRegistry::new(PathBuf::from("."));
        registry.register(Arc::new(SleepTool {
            name: "sleep",
            duration: Duration::from_secs(2),
        }));
        registry.set_tool_timeout_secs(1);

        let result = registry
            .execute("sleep", json!({ "timeout_secs": 3 }))
            .await
            .unwrap();
        assert!(result.success);
    }
}
