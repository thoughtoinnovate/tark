//! Wrapper that adapts MCP tools to tark's Tool trait.

use super::client::McpServerManager;
use super::types::McpToolDef;
use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Default timeout for MCP tool calls (30 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Wraps an MCP tool to implement tark's Tool trait
pub struct McpToolWrapper {
    /// Server ID this tool belongs to
    server_id: String,
    /// Tool definition from MCP server
    tool_def: McpToolDef,
    /// Reference to the manager for making calls
    manager: Arc<McpServerManager>,
    /// Risk level for this tool
    risk_level: RiskLevel,
    /// Timeout for tool calls
    #[allow(dead_code)]
    timeout_secs: u64,
    /// Optional namespace prefix for tool name
    #[allow(dead_code)]
    namespace: Option<String>,
}

impl McpToolWrapper {
    /// Create a new wrapper
    pub fn new(server_id: String, tool_def: McpToolDef, manager: Arc<McpServerManager>) -> Self {
        Self {
            server_id,
            tool_def,
            manager,
            risk_level: RiskLevel::Risky, // Default to risky for external tools
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            namespace: None,
        }
    }

    /// Set the risk level
    pub fn with_risk_level(mut self, level: RiskLevel) -> Self {
        self.risk_level = level;
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set a namespace prefix for the tool name
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = Some(namespace);
        self
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.tool_def.name
    }

    fn description(&self) -> &str {
        &self.tool_def.description
    }

    fn parameters(&self) -> Value {
        self.tool_def.input_schema.clone()
    }

    fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Check if server is connected
        if !self.manager.is_connected(&self.server_id) {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not connected. Use /mcp connect {} first.",
                self.server_id, self.server_id
            )));
        }

        // Call the tool
        match self
            .manager
            .call_tool(&self.server_id, &self.tool_def.name, params)
        {
            Ok(result) => {
                if result.is_error {
                    Ok(ToolResult::error(result.to_text()))
                } else {
                    Ok(ToolResult::success(result.to_text()))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("MCP call failed: {}", e))),
        }
    }

    fn get_command_string(&self, params: &Value) -> String {
        format!("mcp:{}:{} {}", self.server_id, self.tool_def.name, params)
    }
}

/// Create tool wrappers for all tools from a connected server
pub fn wrap_server_tools(
    server_id: &str,
    manager: Arc<McpServerManager>,
    risk_level: Option<RiskLevel>,
    timeout_secs: Option<u64>,
    namespace: Option<String>,
) -> Vec<Arc<dyn Tool>> {
    let tools = manager.tools(server_id);

    tools
        .into_iter()
        .map(|tool_def| {
            let mut wrapper = McpToolWrapper::new(server_id.to_string(), tool_def, manager.clone());

            if let Some(level) = risk_level {
                wrapper = wrapper.with_risk_level(level);
            }
            if let Some(secs) = timeout_secs {
                wrapper = wrapper.with_timeout(secs);
            }
            if let Some(ref ns) = namespace {
                wrapper = wrapper.with_namespace(ns.clone());
            }

            Arc::new(wrapper) as Arc<dyn Tool>
        })
        .collect()
}
