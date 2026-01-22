//! MCP client for connecting to external MCP servers.

use super::transport::StdioTransport;
use super::types::{ConnectionStatus, McpToolDef, McpToolResult, ServerCapabilities};
use crate::storage::{McpConfig, McpServer};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

/// Information about a connected MCP server
pub struct McpServerConnection {
    /// Server configuration
    pub config: McpServer,
    /// Connection status
    pub status: ConnectionStatus,
    /// Transport (if connected)
    transport: Option<StdioTransport>,
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

/// Manages connections to multiple MCP servers
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
    pub fn load_config(&self, config: &McpConfig) {
        let mut connections = self.connections.write().unwrap();
        for (id, server_config) in &config.servers {
            if server_config.enabled {
                connections.insert(id.clone(), McpServerConnection::new(server_config.clone()));
            }
        }
    }

    /// Get list of configured server IDs
    pub fn server_ids(&self) -> Vec<String> {
        self.connections.read().unwrap().keys().cloned().collect()
    }

    /// Get connection status for a server
    pub fn status(&self, server_id: &str) -> Option<ConnectionStatus> {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.status.clone())
    }

    /// Get all tools from a connected server
    pub fn tools(&self, server_id: &str) -> Vec<McpToolDef> {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.tools.clone())
            .unwrap_or_default()
    }

    /// Get all tools from all connected servers
    pub fn all_tools(&self) -> Vec<(String, McpToolDef)> {
        let connections = self.connections.read().unwrap();
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

    /// Connect to a server
    pub fn connect(&self, server_id: &str) -> Result<()> {
        let config = {
            let connections = self.connections.read().unwrap();
            connections
                .get(server_id)
                .map(|c| c.config.clone())
                .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?
        };

        // Update status to connecting
        {
            let mut connections = self.connections.write().unwrap();
            if let Some(conn) = connections.get_mut(server_id) {
                conn.status = ConnectionStatus::Connecting;
            }
        }

        // Spawn the transport
        let transport = match StdioTransport::spawn(
            &config.command,
            &config.args,
            &config.env,
            Some(&self.working_dir),
        ) {
            Ok(t) => t,
            Err(e) => {
                let mut connections = self.connections.write().unwrap();
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Initialize the connection
        let init_result = transport.request(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "tark",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let capabilities: ServerCapabilities = match init_result {
            Ok(result) => {
                // Send initialized notification
                let _ = transport.notify("notifications/initialized", None);

                // Parse capabilities
                result
                    .get("capabilities")
                    .cloned()
                    .and_then(|c| serde_json::from_value(c).ok())
                    .unwrap_or_default()
            }
            Err(e) => {
                let mut connections = self.connections.write().unwrap();
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Discover tools
        let tools = if capabilities.tools.is_some() {
            match transport.request("tools/list", None) {
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
            let mut connections = self.connections.write().unwrap();
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

    /// Disconnect from a server
    pub fn disconnect(&self, server_id: &str) -> Result<()> {
        let mut connections = self.connections.write().unwrap();
        if let Some(conn) = connections.get_mut(server_id) {
            if let Some(transport) = conn.transport.take() {
                let _ = transport.kill();
            }
            conn.status = ConnectionStatus::Disconnected;
            conn.tools.clear();
            tracing::info!("Disconnected from MCP server: {}", server_id);
        }
        Ok(())
    }

    /// Call a tool on a server
    pub fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        let connections = self.connections.read().unwrap();
        let conn = connections
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;

        if !conn.status.is_connected() {
            return Err(anyhow::anyhow!("Server not connected: {}", server_id));
        }

        let transport = conn
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No transport for server: {}", server_id))?;

        let result = transport
            .request(
                "tools/call",
                Some(json!({
                    "name": tool_name,
                    "arguments": arguments
                })),
            )
            .with_context(|| format!("Failed to call tool: {}", tool_name))?;

        serde_json::from_value(result).context("Failed to parse tool result")
    }

    /// Check if a server is connected
    pub fn is_connected(&self, server_id: &str) -> bool {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.status.is_connected())
            .unwrap_or(false)
    }

    /// Get data directory
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

impl Drop for McpServerManager {
    fn drop(&mut self) {
        // Disconnect all servers
        let server_ids: Vec<String> = self.server_ids();
        for id in server_ids {
            let _ = self.disconnect(&id);
        }
    }
}
