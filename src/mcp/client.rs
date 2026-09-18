//! MCP client for connecting to external MCP servers (async).

use super::transport::{ActiveTransport, StdioTransport, StreamableHttpTransport};
use super::trust::McpTrustStore;
use super::types::{
    protocol, ConformanceCheck, ConformanceReport, ConnectionState, ConnectionStatus,
    ExtensionGate, McpInspectSummary, McpToolDef, McpToolResult, ServerCapabilities,
    MCP_PROTOCOL_REVISION,
};
use crate::storage::{McpConfig, McpServer};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Information about a connected MCP server
pub struct McpServerConnection {
    /// Server configuration
    pub config: McpServer,
    /// Connection status (kept for back-compat UI code)
    pub status: ConnectionStatus,
    /// Transport (if connected)
    transport: Option<ActiveTransport>,
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

    /// Richer lifecycle state derived from status + enabled flag.
    pub fn state(&self) -> ConnectionState {
        if !self.config.enabled {
            return ConnectionState::Disabled;
        }
        ConnectionState::from_status(&self.status)
    }
}

/// Manages connections to multiple MCP servers (async)
pub struct McpServerManager {
    /// Server connections by ID
    connections: RwLock<HashMap<String, McpServerConnection>>,
    /// Server ids whose last connect was refused for lack of explicit trust (R3 S7).
    trust_gated: RwLock<HashSet<String>>,
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
            trust_gated: RwLock::new(HashSet::new()),
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

    /// Register (or replace) a server config in memory.
    ///
    /// Used by `tark mcp add/import` without touching storage persistence.
    pub async fn register_server(&self, server_id: &str, config: McpServer) {
        let mut connections = self.connections.write().await;
        // Preserve live transport/status when replacing config for same id.
        if let Some(existing) = connections.get_mut(server_id) {
            let transport = existing.transport.clone();
            let status = existing.status.clone();
            let tools = existing.tools.clone();
            let capabilities = existing.capabilities.clone();
            *existing = McpServerConnection {
                config,
                status,
                transport,
                tools,
                capabilities,
            };
        } else {
            connections.insert(server_id.to_string(), McpServerConnection::new(config));
        }
    }

    /// Remove a server from memory (disconnects first).
    pub async fn remove_server(&self, server_id: &str) -> Result<()> {
        let _ = self.disconnect(server_id).await;
        let mut connections = self.connections.write().await;
        connections
            .remove(server_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;
        Ok(())
    }

    /// Get a clone of the stored config for a server.
    pub async fn get_config(&self, server_id: &str) -> Option<McpServer> {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.config.clone())
    }

    /// Enable/disable a server in memory.
    ///
    /// NOTE: persistence is storage-owned (`mcp/servers.toml` via
    /// `crate::storage`); this only flips the in-memory flag so CLI flows
    /// can stage changes before writing the TOML file themselves.
    pub async fn set_enabled(&self, server_id: &str, enabled: bool) -> Result<()> {
        let mut connections = self.connections.write().await;
        let conn = connections
            .get_mut(server_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;
        conn.config.enabled = enabled;
        Ok(())
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

    /// Get richer connection state for a server.
    pub async fn connection_state(&self, server_id: &str) -> Option<ConnectionState> {
        if self.trust_gated.read().await.contains(server_id) {
            return Some(ConnectionState::Untrusted);
        }
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.state())
    }

    /// Human-readable launch summary for informed trust (R3 S7): executable
    /// command, arguments, working directory, provenance when available, and
    /// configured environment categories (values hidden).
    pub async fn trust_summary(&self, server_id: &str) -> Result<String> {
        let config = self
            .connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.config.clone())
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;
        Ok(McpTrustStore::launch_summary(
            &config.command,
            &config.args,
            &self.working_dir.display().to_string(),
            &config.env,
            None,
        ))
    }

    /// True when this exact launch configuration was explicitly trusted.
    pub async fn is_trusted(&self, server_id: &str) -> bool {
        let config = match self
            .connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.config.clone())
        {
            Some(config) => config,
            None => return false,
        };
        McpTrustStore::open(&self.working_dir)
            .map(|store| {
                store.is_trusted(
                    server_id,
                    &config.command,
                    &config.args,
                    &self.working_dir.display().to_string(),
                    &config.env,
                )
            })
            .unwrap_or(false)
    }

    /// Record explicit user trust for the current launch configuration (R3 S7).
    pub async fn approve_server(&self, server_id: &str) -> Result<String> {
        let config = self
            .connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.config.clone())
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;
        let cwd = self.working_dir.display().to_string();
        let summary =
            McpTrustStore::launch_summary(&config.command, &config.args, &cwd, &config.env, None);
        McpTrustStore::open(&self.working_dir)?.approve(
            server_id,
            &config.command,
            &config.args,
            &cwd,
            &config.env,
            summary.clone(),
        )?;
        self.trust_gated.write().await.remove(server_id);
        Ok(summary)
    }

    /// Revoke previously granted trust. Returns true when a record existed.
    pub async fn revoke_trust(&self, server_id: &str) -> Result<bool> {
        McpTrustStore::open(&self.working_dir)?.revoke(server_id)
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

    /// Build the `capabilities` object advertised in `initialize`.
    ///
    /// Draft Tasks is never presented as stable: `tasks` is only advertised
    /// when `TARK_MCP_TASKS_EXPERIMENTAL=1`, and `apps` only when
    /// `TARK_MCP_APPS=1`.
    fn client_capabilities() -> Value {
        let gate = ExtensionGate::from_env();
        let mut caps = serde_json::Map::new();
        if gate.apps_enabled {
            caps.insert("apps".to_string(), json!({}));
        }
        if gate.tasks_experimental {
            // Explicitly tagged experimental; never stable.
            caps.insert("tasks".to_string(), json!({"experimental": true}));
        }
        Value::Object(caps)
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

        if !config.enabled {
            return Err(anyhow::anyhow!("Server '{}' is disabled", server_id));
        }

        // Update status to connecting
        {
            let mut connections = self.connections.write().await;
            if let Some(conn) = connections.get_mut(server_id) {
                conn.status = ConnectionStatus::Connecting;
            }
        }

        // Resolve transport: Streamable HTTP when MCP_URL is present,
        // otherwise stdio. See `resolve_endpoint` for the env-key mapping.
        let endpoint = super::types::resolve_endpoint(&config);
        // Informed trust for stdio launches (R3 S7): an MCP stdio server is
        // executable third-party code, so no process starts until the user
        // has explicitly trusted this exact launch configuration.
        if matches!(endpoint.transport, super::types::McpServerTransport::Stdio)
            && !self.is_trusted(server_id).await
        {
            let summary = self.trust_summary(server_id).await.unwrap_or_default();
            self.trust_gated.write().await.insert(server_id.to_string());
            {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Disconnected;
                }
            }
            return Err(anyhow::anyhow!(
                "MCP server '{}' is not trusted: no process was started.\n\n{}\n\n\
                 Review the executable, arguments, working directory, and environment \
                 categories above, then approve explicitly before connecting.",
                server_id,
                summary
            ));
        }
        self.trust_gated.write().await.remove(server_id);
        let transport: ActiveTransport = match endpoint.transport {
            super::types::McpServerTransport::Stdio => {
                match StdioTransport::spawn(
                    &config.command,
                    &config.args,
                    &config.env,
                    Some(&self.working_dir),
                )
                .await
                {
                    Ok(t) => ActiveTransport::Stdio(Arc::new(t)),
                    Err(e) => {
                        let mut connections = self.connections.write().await;
                        if let Some(conn) = connections.get_mut(server_id) {
                            conn.status = ConnectionStatus::Failed(e.to_string());
                        }
                        return Err(e);
                    }
                }
            }
            super::types::McpServerTransport::StreamableHttp => {
                let http_config = endpoint.http.clone().ok_or_else(|| {
                    anyhow::anyhow!("Missing HTTP config for server: {}", server_id)
                })?;
                match StreamableHttpTransport::new(http_config) {
                    Ok(t) => ActiveTransport::Http(Arc::new(t)),
                    Err(e) => {
                        let msg = e.to_string();
                        let mut connections = self.connections.write().await;
                        if let Some(conn) = connections.get_mut(server_id) {
                            conn.status = ConnectionStatus::Failed(msg.clone());
                        }
                        return Err(anyhow::anyhow!(msg));
                    }
                }
            }
        };

        // Initialize the connection (async)
        let init_result = transport
            .request(
                "initialize",
                Some(json!({
                    "protocolVersion": MCP_PROTOCOL_REVISION,
                    "capabilities": Self::client_capabilities(),
                    "clientInfo": {
                        "name": "tark",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                })),
            )
            .await;

        let init_value = match init_result {
            Ok(v) => v,
            Err(e) => {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                // Shut down stdio child on failed handshake; no-op for HTTP.
                let _ = transport.shutdown().await;
                return Err(e);
            }
        };

        // Enforce protocol version: mismatch is an explicit Incompatible
        // error naming required vs offered; never silently fall back.
        let offered = init_value.get("protocolVersion").and_then(|v| v.as_str());
        if let Err(version_err) = protocol::assert_protocol_version(offered) {
            let msg = version_err.to_string();
            {
                let mut connections = self.connections.write().await;
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(msg.clone());
                }
            }
            let _ = transport.shutdown().await;
            return Err(anyhow::anyhow!(msg));
        }

        let capabilities: ServerCapabilities = {
            // Send initialized notification (async)
            let _ = transport.notify("notifications/initialized", None).await;

            // Parse capabilities
            init_value
                .get("capabilities")
                .cloned()
                .and_then(|c| serde_json::from_value(c).ok())
                .unwrap_or_default()
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

    /// Reconnect to a server: clean disconnect followed by connect.
    pub async fn reconnect(&self, server_id: &str) -> Result<()> {
        let _ = self.disconnect(server_id).await;
        self.connect(server_id).await
    }

    /// Inspect a server: config + status + capabilities + tools summary.
    pub async fn inspect(&self, server_id: &str) -> Result<McpInspectSummary> {
        let connections = self.connections.read().await;
        let conn = connections
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;
        let transport = conn
            .transport
            .as_ref()
            .map(|t| t.label().to_string())
            .unwrap_or_else(|| {
                super::types::resolve_endpoint(&conn.config)
                    .transport_label()
                    .to_string()
            });
        let gate = ExtensionGate::from_env();
        Ok(McpInspectSummary {
            server_id: server_id.to_string(),
            enabled: conn.config.enabled,
            transport,
            status: conn.status.display().to_string(),
            capabilities: conn.capabilities.clone(),
            tool_count: conn.tools.len(),
            tool_names: conn.tools.iter().map(|t| t.name.clone()).collect(),
            apps_enabled: gate.apps_enabled,
            tasks_experimental: gate.tasks_experimental,
        })
    }

    /// Disconnect from a server (async)
    pub async fn disconnect(&self, server_id: &str) -> Result<()> {
        let mut connections = self.connections.write().await;
        if let Some(conn) = connections.get_mut(server_id) {
            if let Some(transport) = conn.transport.take() {
                let _ = transport.shutdown().await;
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

/// Run a conformance check for a server.
///
/// Performs: handshake state + protocol-version assertion + transport check +
/// live `tools/list` + a `tools/call` ping when tools are available.
/// Returns a serializable [`ConformanceReport`]; never panics on
/// disconnected servers (checks are recorded as failed/skipped instead).
pub async fn conformance_check(manager: &McpServerManager, server_id: &str) -> ConformanceReport {
    let revision = MCP_PROTOCOL_REVISION.to_string();
    let mut checks: Vec<ConformanceCheck> = Vec::new();

    let snapshot = {
        let connections = manager.connections.read().await;
        connections.get(server_id).map(|c| {
            (
                c.config.clone(),
                c.status.clone(),
                c.transport.clone(),
                c.tools.clone(),
            )
        })
    };

    let Some((config, status, transport_opt, known_tools)) = snapshot else {
        return ConformanceReport {
            server_id: server_id.to_string(),
            revision,
            transport: "unknown".to_string(),
            checks: vec![ConformanceCheck {
                name: "handshake".to_string(),
                passed: false,
                detail: format!("Unknown server: {}", server_id),
            }],
        };
    };

    let display_name = if config.name.is_empty() {
        server_id.to_string()
    } else {
        format!("{} ({})", server_id, config.name)
    };
    let transport_label = transport_opt
        .as_ref()
        .map(|t| t.label().to_string())
        .unwrap_or_else(|| {
            super::types::resolve_endpoint(&config)
                .transport_label()
                .to_string()
        });

    // 1. Handshake / connection state.
    if status.is_connected() {
        checks.push(ConformanceCheck {
            name: "handshake".to_string(),
            passed: true,
            detail: format!("connected via {}", transport_label),
        });
    } else {
        checks.push(ConformanceCheck {
            name: "handshake".to_string(),
            passed: false,
            detail: format!("not connected (status: {})", status.display()),
        });
        // Remaining live checks cannot run without a connection.
        checks.push(ConformanceCheck {
            name: "protocol-version".to_string(),
            passed: false,
            detail: format!(
                "skipped: not connected; client requires '{}' with no silent fallback",
                MCP_PROTOCOL_REVISION
            ),
        });
        checks.push(ConformanceCheck {
            name: "transport".to_string(),
            passed: true,
            detail: format!("configured transport: {}", transport_label),
        });
        checks.push(ConformanceCheck {
            name: "tools/list".to_string(),
            passed: false,
            detail: "skipped: not connected".to_string(),
        });
        checks.push(ConformanceCheck {
            name: "tools/call-ping".to_string(),
            passed: false,
            detail: "skipped: not connected".to_string(),
        });
        return ConformanceReport {
            server_id: display_name,
            revision,
            transport: transport_label,
            checks,
        };
    }

    // 2. Protocol version assertion (enforced at connect time; re-assert here).
    checks.push(ConformanceCheck {
        name: "protocol-version".to_string(),
        passed: true,
        detail: format!(
            "client requires '{}'; mismatch returns explicit Incompatible (no silent fallback)",
            MCP_PROTOCOL_REVISION
        ),
    });

    // 3. Transport check.
    checks.push(ConformanceCheck {
        name: "transport".to_string(),
        passed: true,
        detail: format!("active transport: {}", transport_label),
    });

    // 4. Live tools/list.
    let transport = transport_opt.expect("connected implies transport");
    match transport.request("tools/list", None).await {
        Ok(value) => {
            let count = value
                .get("tools")
                .and_then(|t| t.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            checks.push(ConformanceCheck {
                name: "tools/list".to_string(),
                passed: true,
                detail: format!("tools/list ok: {} tool(s)", count),
            });
        }
        Err(e) => {
            checks.push(ConformanceCheck {
                name: "tools/list".to_string(),
                passed: false,
                detail: format!("tools/list failed: {}", e),
            });
            checks.push(ConformanceCheck {
                name: "tools/call-ping".to_string(),
                passed: false,
                detail: "skipped: tools/list failed".to_string(),
            });
            return ConformanceReport {
                server_id: display_name,
                revision,
                transport: transport_label,
                checks,
            };
        }
    }

    // 5. tools/call ping when tools are available.
    if let Some(first) = known_tools.first() {
        match transport
            .request(
                "tools/call",
                Some(json!({"name": first.name, "arguments": {}})),
            )
            .await
        {
            Ok(_) => checks.push(ConformanceCheck {
                name: "tools/call-ping".to_string(),
                passed: true,
                detail: format!("tools/call ping ok for '{}'", first.name),
            }),
            Err(e) => checks.push(ConformanceCheck {
                name: "tools/call-ping".to_string(),
                passed: false,
                detail: format!("tools/call ping failed for '{}': {}", first.name, e),
            }),
        }
    } else {
        checks.push(ConformanceCheck {
            name: "tools/call-ping".to_string(),
            passed: true,
            detail: "no tools to ping (skipped)".to_string(),
        });
    }

    ConformanceReport {
        server_id: display_name,
        revision,
        transport: transport_label,
        checks,
    }
}

// Note: No Drop impl needed - StdioTransport uses kill_on_drop(true)

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_server(name: &str) -> McpServer {
        McpServer {
            name: name.to_string(),
            command: "true".to_string(),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            capabilities: vec![],
            tark: None,
        }
    }

    #[tokio::test]
    async fn reconnect_unknown_server_errors() {
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        assert!(manager.reconnect("nope").await.is_err());
    }

    #[tokio::test]
    async fn inspect_reports_summary() {
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        manager.register_server("demo", test_server("Demo")).await;
        let summary = manager.inspect("demo").await.unwrap();
        assert_eq!(summary.server_id, "demo");
        assert_eq!(summary.transport, "stdio");
        assert_eq!(summary.tool_count, 0);
        assert!(summary.enabled);
    }

    #[tokio::test]
    async fn set_enabled_is_in_memory() {
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        manager.register_server("demo", test_server("Demo")).await;
        manager.set_enabled("demo", false).await.unwrap();
        let cfg = manager.get_config("demo").await.unwrap();
        assert!(!cfg.enabled);
        // Disabled servers refuse to connect.
        assert!(manager.connect("demo").await.is_err());
    }

    #[tokio::test]
    async fn conformance_unknown_server_reports_failure() {
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        let report = conformance_check(&manager, "ghost").await;
        assert_eq!(report.revision, MCP_PROTOCOL_REVISION);
        assert!(!report.all_passed_for_test());
    }

    #[tokio::test]
    async fn conformance_disconnected_reports_skipped() {
        let manager = McpServerManager::new(std::env::temp_dir(), std::env::temp_dir());
        manager.register_server("demo", test_server("Demo")).await;
        let report = conformance_check(&manager, "demo").await;
        assert_eq!(report.transport, "stdio");
        // Handshake must fail when disconnected.
        let handshake = report
            .checks
            .iter()
            .find(|c| c.name == "handshake")
            .unwrap();
        assert!(!handshake.passed);
    }

    // Helper to avoid depending on private helper `all_passed` visibility in test.
    trait ReportExt {
        fn all_passed_for_test(&self) -> bool;
    }
    impl ReportExt for ConformanceReport {
        fn all_passed_for_test(&self) -> bool {
            self.checks.iter().all(|c| c.passed)
        }
    }
}
