//! MCP client for connecting to external MCP servers (async).

use super::transport::StdioTransport;
use super::types::{ConnectionStatus, McpToolDef, McpToolResult, ServerCapabilities};
use crate::storage::{McpConfig, McpServer};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Information about a connected MCP server
pub struct McpServerConnection {
    /// Server configuration
    pub config: McpServer,
    /// Connection status
    pub status: ConnectionStatus,
    /// Transport (if connected)
    transport: Option<Arc<StdioTransport>>,
    /// Discovered tools
    pub tools: Vec<McpToolDef>,
    /// Server capabilities
    pub capabilities: ServerCapabilities,
}

impl McpServerConnection {
    /// Create a new disconnected connection
    pub fn new(config: McpServer) -> Self {
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            transport: None,
            tools: Vec::new(),
            capabilities: ServerCapabilities::default(),
        }
    }
}

/// Manages connections to multiple MCP servers (async)
pub struct McpServerManager {
    /// Server connections by ID
    connections: RwLock<HashMap<String, McpServerConnection>>,
    /// Data directory for downloads
    data_dir: PathBuf,
    /// Working directory for spawned processes
    working_dir: PathBuf,
}

impl McpServerManager {
    /// Create a new manager
    pub fn new(data_dir: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            data_dir,
            working_dir,
        }
    }

    /// Load server configurations
    pub async fn load_config(&self, config: &McpConfig) {
        let mut connections = self.connections.write().await;
        for (id, server_config) in &config.servers {
            if server_config.enabled {
                connections.insert(id.clone(), McpServerConnection::new(server_config.clone()));
            }
        }
    }

    /// Get list of configured server IDs
    pub async fn server_ids(&self) -> Vec<String> {
        self.connections.read().await.keys().cloned().collect()
    }

    /// Get connection status for a server
    pub async fn status(&self, server_id: &str) -> Option<ConnectionStatus> {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.status.clone())
    }

    /// Get all tools from a connected server
    pub async fn tools(&self, server_id: &str) -> Vec<McpToolDef> {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.tools.clone())
            .unwrap_or_default()
    }

    /// Get all tools from all connected servers
    pub async fn all_tools(&self) -> Vec<(String, McpToolDef)> {
        let connections = self.connections.read().await;
        let mut tools = Vec::new();
        for (server_id, conn) in connections.iter() {
            if conn.status.is_connected() {
                for tool in &conn.tools {
                    tools.push((server_id.clone(), tool.clone()));
                }
            }
        }
        tools
    }

    /// Connect to a server (async)
    pub async fn connect(&self, server_id: &str) -> Result<()> {
        let config = {
            let connections = self.connections.read().await;
            connections
                .get(server_id)
                .map(|c| c.config.clone())
                .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?
        };

        // Update status to connecting
        {
            let mut connections = self.connections.write().await;
            if let Some(conn) = connections.get_mut(server_id) {
                conn.status = ConnectionStatus::Connecting;
            }
        }

        // Spawn the transport (async)
        let transport = match StdioTransport::spawn(
            &config.command,
            &config.args,
            &config.env,
            Some(&self.working_dir),
        )
        .await
        {
            Ok(t) => Arc::new(t),
            Err(e) => {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Initialize the connection (async)
        let init_result = transport
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "tark",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
            )
            .await;

        let capabilities: ServerCapabilities = match init_result {
            Ok(result) => {
                // Send initialized notification (async)
                let _ = transport.notify("notifications/initialized", None).await;

                // Parse capabilities
                result
                    .get("capabilities")
                    .cloned()
                    .and_then(|c| serde_json::from_value(c).ok())
                    .unwrap_or_default()
            }
            Err(e) => {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Discover tools (async)
        let tools = if capabilities.tools.is_some() {
            match transport.request("tools/list", None).await {
                Ok(result) => result
                    .get("tools")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to list tools: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Update connection
        {
            let mut connections = self.connections.write().await;
            if let Some(conn) = connections.get_mut(server_id) {
                conn.transport = Some(transport);
                conn.capabilities = capabilities;
                conn.tools = tools;
                conn.status = ConnectionStatus::Connected;
            }
        }

        tracing::info!("Connected to MCP server: {}", server_id);
        Ok(())
    }

    /// Disconnect from a server (async)
    pub async fn disconnect(&self, server_id: &str) -> Result<()> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_id) {
            if let Some(transport) = conn.transport.take() {
                let _ = transport.kill().await;
            }
            conn.status = ConnectionStatus::Disconnected;
            conn.tools.clear();
            tracing::info!("Disconnected from MCP server: {}", server_id);
        }
        Ok(())
    }

    /// Call a tool on a server (async)
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        // Get transport reference while holding read lock briefly
        let transport = {
            let connections = self.connections.read().await;
            let conn = connections
                .get(server_id)
                .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;

            if !conn.status.is_connected() {
                return Err(anyhow::anyhow!("Server not connected: {}", server_id));
            }

            conn.transport
                .clone()
                .ok_or_else(|| anyhow::anyhow!("No transport for server: {}", server_id))?
        };

        // Make the call without holding the lock
        let result = transport
            .request(
                "tools/call",
                Some(json!({
                    "name": tool_name,
                    "arguments": arguments
                })),
            )
            .await
            .with_context(|| format!("Failed to call tool: {}", tool_name))?;

        serde_json::from_value(result).context("Failed to parse tool result")
    }

    /// Check if a server is connected
    pub async fn is_connected(&self, server_id: &str) -> bool {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.status.is_connected())
            .unwrap_or(false)
    }

    /// Get data directory
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }

    /// Disconnect all servers (async) - for cleanup
    pub async fn disconnect_all(&self) {
        let server_ids: Vec<String> = self.server_ids().await;
        for id in server_ids {
            let _ = self.disconnect(&id).await;
        }
    }
}

// Note: No Drop impl needed - StdioTransport uses kill_on_drop(true)
