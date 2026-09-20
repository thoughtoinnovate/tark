//! MCP client for connecting to external MCP servers (async).

use super::transport::{ActiveTransport, StdioTransport, StreamableHttpTransport};
use super::trust::McpTrustStore;
use super::types::{
    protocol, ConformanceCheck, ConformanceReport, ConnectionState, ConnectionStatus,
    ExtensionGate, McpInspectSummary, McpPromptDef, McpPromptResult, McpResourceDef,
    McpResourceResult, McpToolDef, McpToolResult, ServerCapabilities, ServerNotification,
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
    /// Discovered resources (R5)
    pub resources: Vec<McpResourceDef>,
    /// Discovered prompts (R5)
    pub prompts: Vec<McpPromptDef>,
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
            resources: Vec::new(),
            prompts: Vec::new(),
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
    /// Lazily opened credential store for managed auth tokens (R6 S17).
    /// Failures degrade to no store (env/file auth still works).
    credential_store: once_cell::sync::OnceCell<Arc<super::credential_store::EncryptedFileStore>>,
}

impl McpServerManager {
    /// Create a new manager
    pub fn new(data_dir: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            trust_gated: RwLock::new(HashSet::new()),
            data_dir,
            working_dir,
            credential_store: once_cell::sync::OnceCell::new(),
        }
    }

    /// Credential store for managed authorization tokens, opened best-effort.
    ///
    /// Cached after the first attempt; a failed open is retried on the next
    /// call. `None` degrades to env/file auth without blocking connects.
    fn credential_store(&self) -> Option<Arc<super::credential_store::EncryptedFileStore>> {
        match self.credential_store.get_or_try_init(|| {
            let path = self.data_dir.join("credentials.json");
            super::credential_store::EncryptedFileStore::open(&path).map(Arc::new)
        }) {
            Ok(store) => Some(store.clone()),
            Err(e) => {
                tracing::debug!("MCP credential store unavailable: {}", e);
                None
            }
        }
    }

    /// Refresh an OAuth-managed access token when expired (R6 S17).
    ///
    /// Applies when the endpoint's `credential_key` names the OAuth service
    /// (`tark-mcp-oauth/<issuer>`): the refresh record is loaded, rotated
    /// when expired, and both the raw token entry (read at request time by
    /// the transport) and the record are updated. Best-effort: failures warn
    /// and the connect proceeds (the server then rejects with a clear 401).
    /// No-ops when no store, no key, or no record exists yet.
    async fn ensure_oauth_token(&self, credential_key: Option<&str>) {
        use super::credential_store::{CredentialKey, CredentialStore};
        use super::oauth::{
            refresh_access_token, OAuthTokenRecord, OAUTH_RECORD_SUFFIX, OAUTH_TOKEN_SERVICE,
        };

        let Some(raw) = credential_key else {
            return;
        };
        let Ok(key) = CredentialKey::parse(raw) else {
            return;
        };
        if key.service != OAUTH_TOKEN_SERVICE {
            return;
        }
        let Some(store) = self.credential_store() else {
            return;
        };
        let record_key = match CredentialKey::new(
            OAUTH_TOKEN_SERVICE,
            &format!("{}{}", key.account, OAUTH_RECORD_SUFFIX),
        ) {
            Ok(key) => key,
            Err(_) => return,
        };
        let record_json = match store.load_token(&record_key) {
            Ok(Some(json)) => json,
            Ok(None) => return, // Initial authorization not completed yet.
            Err(e) => {
                tracing::warn!("OAuth record unreadable for '{}': {}", key.account, e);
                return;
            }
        };
        let record: OAuthTokenRecord = match serde_json::from_str(&record_json) {
            Ok(record) => record,
            Err(e) => {
                tracing::warn!("OAuth record invalid for '{}': {}", key.account, e);
                return;
            }
        };
        if !record.is_expired() {
            return;
        }
        match refresh_access_token(&record).await {
            Ok((access_token, updated)) => {
                if store.store_token(&key, &access_token).is_err() {
                    tracing::warn!("OAuth token store failed for '{}'", key.account);
                    return;
                }
                let record_json = serde_json::to_string(&updated).unwrap_or_default();
                if !record_json.is_empty() {
                    let _ = store.store_token(&record_key, &record_json);
                }
                tracing::info!("OAuth token refreshed for '{}'", key.account);
            }
            Err(e) => {
                tracing::warn!(
                    "OAuth refresh failed for '{}': {} (re-authorization may be required)",
                    key.account,
                    e
                );
            }
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
            let resources = existing.resources.clone();
            let prompts = existing.prompts.clone();
            let capabilities = existing.capabilities.clone();
            *existing = McpServerConnection {
                config,
                status,
                transport,
                tools,
                resources,
                prompts,
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

    /// Clone the live transport for a connected server.
    async fn live_transport(&self, server_id: &str) -> Result<ActiveTransport> {
        self.connections
            .read()
            .await
            .get(server_id)
            .filter(|conn| conn.status.is_connected())
            .and_then(|conn| conn.transport.clone())
            .ok_or_else(|| anyhow::anyhow!("Server '{}' is not connected", server_id))
    }

    /// List discovered resources for a server (R5).
    pub async fn resources(&self, server_id: &str) -> Vec<McpResourceDef> {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.resources.clone())
            .unwrap_or_default()
    }

    /// List discovered prompts for a server (R5).
    pub async fn prompts(&self, server_id: &str) -> Vec<McpPromptDef> {
        self.connections
            .read()
            .await
            .get(server_id)
            .map(|c| c.prompts.clone())
            .unwrap_or_default()
    }

    /// Read a resource by URI (R5).
    pub async fn read_resource(&self, server_id: &str, uri: &str) -> Result<McpResourceResult> {
        let transport = self.live_transport(server_id).await?;
        let value = transport
            .request("resources/read", Some(json!({"uri": uri})))
            .await?;
        serde_json::from_value(value).context("Failed to parse resources/read result")
    }

    /// Subscribe to resource updates (R5).
    ///
    /// The server must advertise the subscribe capability; otherwise this
    /// fails closed instead of sending a request the server cannot honor.
    pub async fn subscribe_resource(&self, server_id: &str, uri: &str) -> Result<()> {
        let advertised = self
            .connections
            .read()
            .await
            .get(server_id)
            .and_then(|c| c.capabilities.resources.clone())
            .is_some_and(|r| r.subscribe);
        if !advertised {
            anyhow::bail!(
                "Server '{}' does not advertise resources/subscribe",
                server_id
            );
        }
        let transport = self.live_transport(server_id).await?;
        transport
            .request("resources/subscribe", Some(json!({"uri": uri})))
            .await?;
        Ok(())
    }

    /// Unsubscribe from resource updates (R5).
    pub async fn unsubscribe_resource(&self, server_id: &str, uri: &str) -> Result<()> {
        let transport = self.live_transport(server_id).await?;
        transport
            .request("resources/unsubscribe", Some(json!({"uri": uri})))
            .await?;
        Ok(())
    }

    /// Render a prompt with arguments (R5).
    pub async fn get_prompt(
        &self,
        server_id: &str,
        name: &str,
        arguments: Option<Value>,
    ) -> Result<McpPromptResult> {
        let transport = self.live_transport(server_id).await?;
        let mut params = serde_json::Map::new();
        params.insert("name".to_string(), Value::String(name.to_string()));
        if let Some(args) = arguments {
            params.insert("arguments".to_string(), args);
        }
        let value = transport
            .request("prompts/get", Some(Value::Object(params)))
            .await?;
        serde_json::from_value(value).context("Failed to parse prompts/get result")
    }

    /// Re-discover tools/resources/prompts per advertised capabilities (R5).
    ///
    /// Used after `*/list_changed` notifications and on demand (`tark mcp
    /// sync`). Never clears capabilities, only refreshes listings.
    pub async fn refresh_server(&self, server_id: &str) -> Result<()> {
        let (transport, capabilities) = {
            let connections = self.connections.read().await;
            let conn = connections
                .get(server_id)
                .filter(|c| c.status.is_connected())
                .ok_or_else(|| anyhow::anyhow!("Server '{}' is not connected", server_id))?;
            (conn.transport.clone(), conn.capabilities.clone())
        };
        let transport =
            transport.ok_or_else(|| anyhow::anyhow!("Server '{}' is not connected", server_id))?;

        if capabilities.tools.is_some() {
            if let Ok(result) = transport.request("tools/list", None).await {
                if let Some(tools) = result
                    .get("tools")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                {
                    if let Some(conn) = self.connections.write().await.get_mut(server_id) {
                        conn.tools = tools;
                    }
                }
            }
        }
        if capabilities.resources.is_some() {
            if let Ok(result) = transport.request("resources/list", None).await {
                if let Some(resources) = result
                    .get("resources")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                {
                    if let Some(conn) = self.connections.write().await.get_mut(server_id) {
                        conn.resources = resources;
                    }
                }
            }
        }
        if capabilities.prompts.is_some() {
            if let Ok(result) = transport.request("prompts/list", None).await {
                if let Some(prompts) = result
                    .get("prompts")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                {
                    if let Some(conn) = self.connections.write().await.get_mut(server_id) {
                        conn.prompts = prompts;
                    }
                }
            }
        }
        Ok(())
    }

    /// Drain buffered server notifications, applying `*/list_changed`
    /// refreshes (R5).
    ///
    /// Returns the drained notifications so callers can observe resource
    /// updates (`notifications/resources/updated` carries the URI).
    pub async fn drain_notifications(&self, server_id: &str) -> Vec<ServerNotification> {
        let Ok(transport) = self.live_transport(server_id).await else {
            return Vec::new();
        };
        let notifications = transport.drain_notifications().await;
        if notifications.iter().any(|n| n.is_list_changed().is_some()) {
            let _ = self.refresh_server(server_id).await;
        }
        notifications
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
                // Rotate OAuth-managed tokens before connecting (R6 S17).
                self.ensure_oauth_token(http_config.credential_key.as_deref())
                    .await;
                // Attach the managed credential store when the endpoint
                // references it (MCP_BEARER_CREDENTIAL); env/file auth is
                // unaffected when the store is unavailable.
                let transport = StreamableHttpTransport::new(http_config);
                let transport = match (transport, self.credential_store()) {
                    (Ok(t), Some(store)) => Ok(t.with_credential_store(store)),
                    (transport, _) => transport,
                };
                match transport {
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

        // Discover resources (async, R5)
        let resources = if capabilities.resources.is_some() {
            match transport.request("resources/list", None).await {
                Ok(result) => result
                    .get("resources")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to list resources: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Discover prompts (async, R5)
        let prompts = if capabilities.prompts.is_some() {
            match transport.request("prompts/list", None).await {
                Ok(result) => result
                    .get("prompts")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to list prompts: {}", e);
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
                conn.resources = resources;
                conn.prompts = prompts;
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
            resource_count: conn.resources.len(),
            resource_uris: conn.resources.iter().map(|r| r.uri.clone()).collect(),
            prompt_count: conn.prompts.len(),
            prompt_names: conn.prompts.iter().map(|p| p.name.clone()).collect(),
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
            conn.resources.clear();
            conn.prompts.clear();
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
/// live `ping` + `notifications/initialized` acceptance + live `tools/list` +
/// a `tools/call` ping when tools are available.
/// Returns a serializable [`ConformanceReport`]; never panics on
/// disconnected servers (checks are recorded as failed/skipped instead).
///
/// This is Tark's dedicated conformance entry point (R13 S30): it exercises
/// the client contract against any configured server, including the official
/// Everything/Filesystem reference servers. Running the upstream
/// modelcontextprotocol/conformance suite itself remains a CI follow-up
/// requiring network access to the reference servers.
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
                c.capabilities.clone(),
            )
        })
    };

    let Some((config, status, transport_opt, known_tools, capabilities)) = snapshot else {
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
        for name in ["ping", "notifications/initialized", "tools/list"] {
            checks.push(ConformanceCheck {
                name: name.to_string(),
                passed: false,
                detail: "skipped: not connected".to_string(),
            });
        }
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

    let transport = transport_opt.expect("connected implies transport");

    // 4. Live ping (protocol liveness, independent of capabilities).
    match transport.request("ping", None).await {
        Ok(_) => checks.push(ConformanceCheck {
            name: "ping".to_string(),
            passed: true,
            detail: "ping ok".to_string(),
        }),
        Err(e) => checks.push(ConformanceCheck {
            name: "ping".to_string(),
            passed: false,
            detail: format!("ping failed: {}", e),
        }),
    }

    // 5. notifications/initialized acceptance (send path; servers ack nothing).
    match transport.notify("notifications/initialized", None).await {
        Ok(_) => checks.push(ConformanceCheck {
            name: "notifications/initialized".to_string(),
            passed: true,
            detail: "initialized notification accepted".to_string(),
        }),
        Err(e) => checks.push(ConformanceCheck {
            name: "notifications/initialized".to_string(),
            passed: false,
            detail: format!("initialized notification failed: {}", e),
        }),
    }

    // 6. Live tools/list.
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

    // 7. resources/list when the server advertises resources.
    if capabilities.resources.is_some() {
        match transport.request("resources/list", None).await {
            Ok(value) => {
                let count = value
                    .get("resources")
                    .and_then(|t| t.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                checks.push(ConformanceCheck {
                    name: "resources/list".to_string(),
                    passed: true,
                    detail: format!("resources/list ok: {} resource(s)", count),
                });
            }
            Err(e) => checks.push(ConformanceCheck {
                name: "resources/list".to_string(),
                passed: false,
                detail: format!("resources/list failed: {}", e),
            }),
        }
    } else {
        checks.push(ConformanceCheck {
            name: "resources/list".to_string(),
            passed: true,
            detail: "not advertised (skipped)".to_string(),
        });
    }

    // 8. prompts/list when the server advertises prompts.
    if capabilities.prompts.is_some() {
        match transport.request("prompts/list", None).await {
            Ok(value) => {
                let count = value
                    .get("prompts")
                    .and_then(|t| t.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                checks.push(ConformanceCheck {
                    name: "prompts/list".to_string(),
                    passed: true,
                    detail: format!("prompts/list ok: {} prompt(s)", count),
                });
            }
            Err(e) => checks.push(ConformanceCheck {
                name: "prompts/list".to_string(),
                passed: false,
                detail: format!("prompts/list failed: {}", e),
            }),
        }
    } else {
        checks.push(ConformanceCheck {
            name: "prompts/list".to_string(),
            passed: true,
            detail: "not advertised (skipped)".to_string(),
        });
    }

    // 9. tools/call ping when tools are available.
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

    /// Minimal in-process MCP server speaking newline-delimited JSON-RPC
    /// (S12/S30): answers `initialize`, `tools/list`, `tools/call`, `ping`,
    /// `resources/list`, `resources/read`, `resources/subscribe`,
    /// `resources/unsubscribe`, `prompts/list`, and `prompts/get`; ignores
    /// notifications. Protocol version comes from argv[1]; when argv[2] is
    /// `notify-first`, a `notifications/tools/list_changed` line is emitted
    /// before the first response to prove server push never desynchronizes
    /// requests.
    const MOCK_MCP_SERVER_PY: &str = r#"
import sys, json
version = sys.argv[1] if len(sys.argv) > 1 else "2026-07-28"
notify_first = len(sys.argv) > 2 and sys.argv[2] == "notify-first"
notified = False
def emit(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()
def respond(req_id, result):
    emit({"jsonrpc": "2.0", "id": req_id, "result": result})
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        msg = json.loads(line)
    except Exception:
        continue
    if "id" not in msg:
        continue
    if notify_first and not notified:
        notified = True
        emit({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"})
    method = msg.get("method")
    params = msg.get("params") or {}
    if method == "initialize":
        respond(msg["id"], {
            "protocolVersion": version,
            "capabilities": {"tools": {}, "resources": {"subscribe": True}, "prompts": {}},
            "serverInfo": {"name": "mock", "version": "1.0"},
        })
    elif method == "tools/list":
        respond(msg["id"], {"tools": [
            {"name": "ping", "description": "Ping tool",
             "inputSchema": {"type": "object"}},
        ]})
    elif method == "tools/call":
        respond(msg["id"], {"content": [{"type": "text", "text": "pong"}]})
    elif method == "ping":
        respond(msg["id"], {})
    elif method == "resources/list":
        respond(msg["id"], {"resources": [
            {"uri": "mock://greeting", "name": "greeting",
             "description": "A greeting", "mimeType": "text/plain"},
        ]})
    elif method == "resources/read":
        respond(msg["id"], {"contents": [
            {"uri": params.get("uri", ""), "mimeType": "text/plain", "text": "hello"},
        ]})
    elif method == "resources/subscribe":
        respond(msg["id"], {})
    elif method == "resources/unsubscribe":
        respond(msg["id"], {})
    elif method == "prompts/list":
        respond(msg["id"], {"prompts": [
            {"name": "greet", "description": "Greet someone",
             "arguments": [{"name": "who", "required": True}]},
        ]})
    elif method == "prompts/get":
        respond(msg["id"], {"description": "greeting", "messages": [
            {"role": "user", "content": {"type": "text", "text": "hi"}},
        ]})
    else:
        emit({"jsonrpc": "2.0", "id": msg["id"],
            "error": {"code": -32601, "message": "unknown method"}})
"#;

    async fn mock_manager(version: &str) -> (tempfile::TempDir, McpServerManager) {
        mock_manager_with_extra_arg(version, None).await
    }

    /// Python executable for the mock MCP server: `python3` with a `python`
    /// fallback (Windows runners may only provide the latter).
    fn mock_python_exe() -> &'static str {
        static EXE: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
        EXE.get_or_init(|| {
            for candidate in ["python3", "python"] {
                if std::process::Command::new(candidate)
                    .arg("--version")
                    .output()
                    .is_ok()
                {
                    return Box::leak(candidate.to_string().into_boxed_str());
                }
            }
            "python3"
        })
    }

    async fn mock_manager_with_extra_arg(
        version: &str,
        extra_arg: Option<&str>,
    ) -> (tempfile::TempDir, McpServerManager) {
        let work = tempfile::tempdir().expect("workdir");
        let script = work.path().join("mock_mcp.py");
        std::fs::write(&script, MOCK_MCP_SERVER_PY).expect("write mock");
        let manager = McpServerManager::new(work.path().to_path_buf(), work.path().to_path_buf());
        let mut args = vec![script.display().to_string(), version.to_string()];
        if let Some(extra) = extra_arg {
            args.push(extra.to_string());
        }
        manager
            .register_server(
                "mock",
                McpServer {
                    name: "Mock".to_string(),
                    command: mock_python_exe().to_string(),
                    args,
                    env: HashMap::new(),
                    enabled: true,
                    capabilities: vec![],
                    tark: None,
                },
            )
            .await;
        // Explicit informed trust for the mock launch (R3 S7).
        manager.approve_server("mock").await.expect("approve");
        (work, manager)
    }

    fn check_passed(report: &ConformanceReport, name: &str) {
        let check = report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing check {name}"));
        assert!(check.passed, "check {name} failed: {}", check.detail);
    }

    #[tokio::test]
    async fn conformance_live_mock_stdio_server_passes() {
        let (_work, manager) = mock_manager(MCP_PROTOCOL_REVISION).await;
        manager.connect("mock").await.expect("connect");
        let report = conformance_check(&manager, "mock").await;
        assert_eq!(report.revision, MCP_PROTOCOL_REVISION);
        assert_eq!(report.transport, "stdio");
        for name in [
            "handshake",
            "protocol-version",
            "transport",
            "ping",
            "notifications/initialized",
            "tools/list",
            "resources/list",
            "prompts/list",
            "tools/call-ping",
        ] {
            check_passed(&report, name);
        }
        assert!(report.all_passed_for_test());
        manager.disconnect("mock").await.expect("disconnect");
    }

    #[tokio::test]
    async fn mock_server_discovers_resources_and_prompts() {
        let (_work, manager) = mock_manager(MCP_PROTOCOL_REVISION).await;
        manager.connect("mock").await.expect("connect");

        let resources = manager.resources("mock").await;
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].uri, "mock://greeting");

        let prompts = manager.prompts("mock").await;
        assert_eq!(prompts.len(), 1);
        assert_eq!(prompts[0].name, "greet");

        let read = manager
            .read_resource("mock", "mock://greeting")
            .await
            .expect("read");
        assert_eq!(read.to_text(), "hello");

        manager
            .subscribe_resource("mock", "mock://greeting")
            .await
            .expect("subscribe");
        manager
            .unsubscribe_resource("mock", "mock://greeting")
            .await
            .expect("unsubscribe");

        let prompt = manager
            .get_prompt("mock", "greet", Some(json!({"who": "tark"})))
            .await
            .expect("get prompt");
        assert_eq!(prompt.messages.len(), 1);
        assert_eq!(prompt.messages[0].role, "user");

        manager.disconnect("mock").await.expect("disconnect");
    }

    #[tokio::test]
    async fn server_push_notification_never_desynchronizes_requests() {
        // The mock emits a list_changed notification before its first
        // response; id-matched demux must still deliver that response.
        let (_work, manager) =
            mock_manager_with_extra_arg(MCP_PROTOCOL_REVISION, Some("notify-first")).await;
        manager.connect("mock").await.expect("connect");
        assert!(!manager.tools("mock").await.is_empty());

        let notifications = manager.drain_notifications("mock").await;
        assert!(
            notifications
                .iter()
                .any(|n| n.method == "notifications/tools/list_changed"),
            "expected buffered list_changed, got {notifications:?}"
        );
        // Draining applied the refresh: tools still listed afterwards.
        assert!(!manager.tools("mock").await.is_empty());
        manager.disconnect("mock").await.expect("disconnect");
    }

    #[tokio::test]
    async fn conformance_mock_wrong_protocol_version_rejected() {
        let (_work, manager) = mock_manager("2020-11-05").await;
        let err = manager.connect("mock").await.expect_err("must reject");
        assert!(
            err.to_string().contains("2020-11-05"),
            "unexpected error: {err}"
        );
        // Failed handshake surfaces as a failed (not skipped) handshake check.
        let report = conformance_check(&manager, "mock").await;
        assert!(!report.all_passed_for_test());
        let handshake = report
            .checks
            .iter()
            .find(|c| c.name == "handshake")
            .unwrap();
        assert!(!handshake.passed);
    }
}
