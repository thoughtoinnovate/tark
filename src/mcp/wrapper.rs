//! Wrapper that adapts MCP tools to tark's Tool trait (async with timeout).

use super::client::McpServerManager;
use super::types::McpToolDef;
use crate::policy::PolicyEngine;
use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

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
    /// Timeout for tool calls in seconds
    timeout_secs: u64,
    /// Optional namespace prefix for tool name
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

    /// Set the timeout in seconds
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set a namespace prefix for the tool name
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = Some(namespace);
        self
    }

    /// Get the full tool name (with namespace if set)
    pub fn full_name(&self) -> String {
        match &self.namespace {
            Some(ns) => format!("{}:{}", ns, self.tool_def.name),
            None => self.tool_def.name.clone(),
        }
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

    fn category(&self) -> crate::tools::ToolCategory {
        crate::tools::ToolCategory::External
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Check if server is connected
        if !self.manager.is_connected(&self.server_id).await {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not connected. Use /mcp connect {} first.",
                self.server_id, self.server_id
            )));
        }

        // Call the tool with timeout
        let timeout_duration = Duration::from_secs(self.timeout_secs);

        match tokio::time::timeout(
            timeout_duration,
            self.manager
                .call_tool(&self.server_id, &self.tool_def.name, params),
        )
        .await
        {
            // Success within timeout
            Ok(Ok(result)) => {
                if result.is_error {
                    Ok(ToolResult::error(result.to_text()))
                } else {
                    Ok(ToolResult::success(result.to_text()))
                }
            }
            // Error within timeout
            Ok(Err(e)) => Ok(ToolResult::error(format!("MCP call failed: {}", e))),
            // Timeout exceeded
            Err(_) => Ok(ToolResult::error(format!(
                "MCP call to '{}' on server '{}' timed out after {}s. \
                 The external server may be unresponsive.",
                self.tool_def.name, self.server_id, self.timeout_secs
            ))),
        }
    }

    fn get_command_string(&self, params: &Value) -> String {
        format!("mcp:{}:{} {}", self.server_id, self.tool_def.name, params)
    }
}

/// Create tool wrappers for all tools from a connected server
pub async fn wrap_server_tools(
    server_id: &str,
    manager: Arc<McpServerManager>,
    risk_level: Option<RiskLevel>,
    timeout_secs: Option<u64>,
    namespace: Option<String>,
) -> Vec<Arc<dyn Tool>> {
    let tools = manager.tools(server_id).await;

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

/// Create tool wrappers with PolicyEngine-driven risk levels (NEW)
///
/// Queries the PolicyEngine for each tool's risk level. Falls back to
/// provided defaults or Risky if no policy is defined.
pub async fn wrap_server_tools_with_policy(
    server_id: &str,
    manager: Arc<McpServerManager>,
    policy_engine: Option<Arc<PolicyEngine>>,
    default_risk: Option<RiskLevel>,
    default_timeout: Option<u64>,
    namespace: Option<String>,
) -> Vec<Arc<dyn Tool>> {
    let tools = manager.tools(server_id).await;

    tools
        .into_iter()
        .map(|tool_def| {
            let mut wrapper =
                McpToolWrapper::new(server_id.to_string(), tool_def.clone(), manager.clone());

            // Query PolicyEngine for risk level
            let risk_level = if let Some(ref engine) = policy_engine {
                query_mcp_risk_level(engine, server_id, &tool_def.name)
                    .unwrap_or_else(|_| default_risk.unwrap_or(RiskLevel::Risky))
            } else {
                default_risk.unwrap_or(RiskLevel::Risky)
            };

            wrapper = wrapper.with_risk_level(risk_level);

            if let Some(secs) = default_timeout {
                wrapper = wrapper.with_timeout(secs);
            }
            if let Some(ref ns) = namespace {
                wrapper = wrapper.with_namespace(ns.clone());
            }

            Arc::new(wrapper) as Arc<dyn Tool>
        })
        .collect()
}

/// Query PolicyEngine for MCP tool risk level
fn query_mcp_risk_level(
    _engine: &PolicyEngine,
    server_id: &str,
    tool_name: &str,
) -> Result<RiskLevel> {
    // The PolicyEngine's mcp module checks approval, but we just need the risk level here
    // For now, we'll use a simple query - this could be extended to query the DB directly
    // TODO: Query mcp_tool_policies table to get actual risk level
    // Default to Risky for external MCP tools as a safe default
    tracing::debug!(
        "Querying PolicyEngine for MCP tool {}:{} - using Risky default",
        server_id,
        tool_name
    );
    Ok(RiskLevel::Risky)
}
