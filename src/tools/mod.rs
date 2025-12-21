//! Agent tools for file system and shell operations

mod file_ops;
mod file_search;
mod grep;
mod shell;
mod lsp_tools;

pub use file_ops::{ReadFileTool, ReadFilesTool, WriteFileTool, PatchFileTool, DeleteFileTool, ListDirectoryTool, ProposeChangeTool};
pub use file_search::{FileSearchTool, CodebaseOverviewTool};
pub use grep::{GrepTool, FindReferencesTool};
pub use shell::ShellTool;
pub use lsp_tools::{ListSymbolsTool, GoToDefinitionTool, FindAllReferencesTool, CallHierarchyTool, GetSignatureTool};

use crate::llm::ToolDefinition;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// Agent mode determines which tools are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Plan mode: read-only tools for exploring and planning
    Plan,
    /// Build mode: all tools for executing changes
    Build,
    /// Review mode: all tools but requires approval (handled by frontend)
    Review,
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
}

impl ToolRegistry {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            tools: HashMap::new(),
            working_dir,
        }
    }

    /// Create a registry with all default tools
    pub fn with_defaults(working_dir: PathBuf, shell_enabled: bool) -> Self {
        Self::for_mode(working_dir, AgentMode::Build, shell_enabled)
    }

    /// Create a registry for a specific agent mode
    pub fn for_mode(working_dir: PathBuf, mode: AgentMode, shell_enabled: bool) -> Self {
        let mut registry = Self::new(working_dir.clone());

        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // Read-only tools (available in all modes)
        registry.register(Arc::new(CodebaseOverviewTool::new(working_dir.clone())));  // Overview (use first!)
        registry.register(Arc::new(FindReferencesTool::new(working_dir.clone())));   // Code flow tracing
        registry.register(Arc::new(ReadFileTool::new(working_dir.clone())));
        registry.register(Arc::new(ReadFilesTool::new(working_dir.clone())));  // Batch read
        registry.register(Arc::new(ListDirectoryTool::new(working_dir.clone())));  // Directory listing
        registry.register(Arc::new(FileSearchTool::new(working_dir.clone())));
        registry.register(Arc::new(GrepTool::new(working_dir.clone())));
        
        // LSP-powered tools for code understanding (available in all modes)
        registry.register(Arc::new(ListSymbolsTool::new(working_dir.clone())));        // List symbols in file/dir
        registry.register(Arc::new(GoToDefinitionTool::new(working_dir.clone())));     // Jump to definition
        registry.register(Arc::new(FindAllReferencesTool::new(working_dir.clone())));  // Find all usages
        registry.register(Arc::new(CallHierarchyTool::new(working_dir.clone())));      // Trace call flow
        registry.register(Arc::new(GetSignatureTool::new(working_dir.clone())));       // Get function signature

        // Write tools (only in Build and Review modes)
        if mode != AgentMode::Plan {
            tracing::debug!("Mode is NOT Plan, registering write tools");
            registry.register(Arc::new(WriteFileTool::new(working_dir.clone())));
            registry.register(Arc::new(PatchFileTool::new(working_dir.clone())));
            registry.register(Arc::new(DeleteFileTool::new(working_dir.clone())));

            if shell_enabled {
                registry.register(Arc::new(ShellTool::new(working_dir)));
                tracing::debug!("Registered: write_file, patch_file, delete_file, shell");
            } else {
                tracing::debug!("Registered: write_file, patch_file, delete_file (shell disabled)");
            }
        } else {
            // Plan mode: register propose_change to show diffs without applying
            registry.register(Arc::new(ProposeChangeTool::new(working_dir.clone())));
            tracing::debug!("Mode IS Plan - registered propose_change for diff preview!");
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
    pub async fn execute(&self, name: &str, params: Value) -> Result<ToolResult> {
        if let Some(tool) = self.tools.get(name) {
            tool.execute(params).await
        } else {
            Ok(ToolResult::error(format!("Unknown tool: {}", name)))
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
}

