//! MCP protocol types and data structures.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Current MCP protocol revision spoken by this client.
///
/// Handshake sends this value as `protocolVersion` and requires the server
/// to offer the same revision. Mismatches are reported as an explicit
/// [`McpError::Incompatible`] error and never silently accepted via fallback.
pub const MCP_PROTOCOL_REVISION: &str = "2026-07-28";

/// Status of an MCP server connection
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionStatus {
    /// Not connected
    #[default]
    Disconnected,
    /// Currently attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Connection failed with error message
    Failed(String),
}

impl ConnectionStatus {
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Get display string
    pub fn display(&self) -> &str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting...",
            Self::Connected => "Connected",
            Self::Failed(_) => "Failed",
        }
    }

    /// Get icon for TUI
    pub fn icon(&self) -> &str {
        match self {
            Self::Disconnected => "○",
            Self::Connecting => "◐",
            Self::Connected => "●",
            Self::Failed(_) => "✗",
        }
    }
}

/// Richer lifecycle state for an MCP server (R5).
///
/// [`ConnectionStatus`] is kept for back-compat with existing UI code.
/// New code should prefer `ConnectionState` which distinguishes disabled,
/// untrusted, degraded, incompatible and stopped states.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ConnectionState {
    /// Server is disabled in configuration.
    Disabled,
    /// Server is configured but not trusted to auto-connect.
    Untrusted,
    /// Currently attempting to connect.
    Starting,
    /// Successfully connected.
    Connected,
    /// Connected but degraded (e.g. tools/list failed).
    Degraded(String),
    /// Protocol version incompatible (required vs offered mismatch).
    Incompatible(String),
    /// Connection failed with error message.
    Failed(String),
    /// Stopped after a clean disconnect.
    #[default]
    Stopped,
}

impl std::fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Disabled => write!(f, "Disabled"),
            Self::Untrusted => write!(f, "Untrusted"),
            Self::Starting => write!(f, "Starting"),
            Self::Connected => write!(f, "Connected"),
            Self::Degraded(d) => write!(f, "Degraded: {}", d),
            Self::Incompatible(d) => write!(f, "Incompatible: {}", d),
            Self::Failed(e) => write!(f, "Failed: {}", e),
            Self::Stopped => write!(f, "Stopped"),
        }
    }
}

impl ConnectionState {
    /// Check if connected (Connected only; Degraded counts as usable but not fully connected).
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Convert from the legacy [`ConnectionStatus`].
    pub fn from_status(status: &ConnectionStatus) -> Self {
        match status {
            ConnectionStatus::Disconnected => Self::Stopped,
            ConnectionStatus::Connecting => Self::Starting,
            ConnectionStatus::Connected => Self::Connected,
            ConnectionStatus::Failed(e) => Self::Failed(e.clone()),
        }
    }

    /// Convert into the legacy [`ConnectionStatus`] for UI code.
    pub fn as_status(&self) -> ConnectionStatus {
        match self {
            Self::Connected => ConnectionStatus::Connected,
            Self::Starting => ConnectionStatus::Connecting,
            Self::Stopped | Self::Disabled | Self::Untrusted => ConnectionStatus::Disconnected,
            Self::Degraded(d) | Self::Incompatible(d) | Self::Failed(d) => {
                ConnectionStatus::Failed(d.clone())
            }
        }
    }
}

/// Transport used to reach an MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum McpServerTransport {
    /// Spawn a child process and speak JSON-RPC over stdio.
    Stdio,
    /// Speak Streamable HTTP (POST JSON-RPC) to a remote URL.
    StreamableHttp,
}

impl std::fmt::Display for McpServerTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Stdio => write!(f, "stdio"),
            Self::StreamableHttp => write!(f, "streamable-http"),
        }
    }
}

/// Configuration for Streamable HTTP transport.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpMcpConfig {
    /// Target URL (must be http(s)).
    pub url: String,
    /// Extra headers to send (Authorization is injected separately).
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// Name of an environment variable holding a bearer token.
    #[serde(default)]
    pub bearer_env: Option<String>,
    /// Path to a file holding a bearer token (e.g. a Tark-issued loopback
    /// credential). Read at request time so rotation needs no restart; the
    /// file must be a regular non-symlink file with owner-only permissions
    /// (unix), otherwise it is refused fail-closed. Takes precedence over
    /// `bearer_env` when both are set.
    #[serde(default)]
    pub bearer_file: Option<PathBuf>,
    /// Allow plain http:// to non-loopback hosts (default false).
    #[serde(default)]
    pub allow_insecure: bool,
}

/// Resolved endpoint for an MCP server.
///
/// Because `crate::storage::McpServer` (owned by storage) cannot gain new
/// fields here, the HTTP endpoint is carried out-of-band via well-known keys
/// in `server.env`:
///
/// - `MCP_URL`: http(s) URL of the Streamable HTTP endpoint. Absent => stdio.
/// - `MCP_BEARER_ENV`: name of an env var holding the bearer token.
/// - `MCP_BEARER_FILE`: path to a file holding the bearer token (takes
///   precedence over `MCP_BEARER_ENV`; see [`HttpMcpConfig::bearer_file`]).
/// - `MCP_ALLOW_INSECURE=1`: allow plain `http://` to non-loopback hosts.
/// - `MCP_HEADER_<NAME>`: extra headers (`<NAME>` with `_` converted to `-`).
///
/// See [`resolve_endpoint`] for the mapping.
#[derive(Debug, Clone)]
pub struct McpServerEndpoint {
    /// Selected transport.
    pub transport: McpServerTransport,
    /// HTTP config when transport is StreamableHttp.
    pub http: Option<HttpMcpConfig>,
}

impl McpServerEndpoint {
    /// Stdio endpoint.
    pub fn stdio() -> Self {
        Self {
            transport: McpServerTransport::Stdio,
            http: None,
        }
    }

    /// Human-readable transport label.
    pub fn transport_label(&self) -> &'static str {
        match self.transport {
            McpServerTransport::Stdio => "stdio",
            McpServerTransport::StreamableHttp => "streamable-http",
        }
    }
}

/// Resolve which transport to use for a stored server config.
///
/// Mapping (documented on [`McpServerEndpoint`]):
/// - If `server.env["MCP_URL"]` is present and non-empty (after trimming) and
///   parses as an `http(s)` URL, select [`McpServerTransport::StreamableHttp`].
/// - Otherwise select [`McpServerTransport::Stdio`].
/// - `MCP_BEARER_ENV` names the env var holding the bearer token.
/// - `MCP_BEARER_FILE` names a file holding the bearer token.
/// - `MCP_ALLOW_INSECURE=1` permits non-loopback plain http.
/// - `MCP_HEADER_<NAME>` entries become extra headers.
pub fn resolve_endpoint(server: &crate::storage::McpServer) -> McpServerEndpoint {
    let raw_url = server
        .env
        .get("MCP_URL")
        .map(|s| s.trim())
        .unwrap_or_default();
    if raw_url.is_empty() {
        return McpServerEndpoint::stdio();
    }
    // Only treat http(s) values as HTTP endpoints; anything else falls back
    // to stdio so a typo never silently selects the wrong transport.
    let lower = raw_url.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return McpServerEndpoint::stdio();
    }
    let bearer_env = server
        .env
        .get("MCP_BEARER_ENV")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let bearer_file = server
        .env
        .get("MCP_BEARER_FILE")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    let allow_insecure = server
        .env
        .get("MCP_ALLOW_INSECURE")
        .map(|s| s.trim() == "1")
        .unwrap_or(false);
    let mut headers = HashMap::new();
    for (k, v) in &server.env {
        if let Some(suffix) = k.strip_prefix("MCP_HEADER_") {
            if suffix.is_empty() {
                continue;
            }
            // `MCP_HEADER_X_CUSTOM` -> `X-CUSTOM`.
            let name = suffix.replace('_', "-");
            headers.insert(name, v.clone());
        }
    }
    McpServerEndpoint {
        transport: McpServerTransport::StreamableHttp,
        http: Some(HttpMcpConfig {
            url: raw_url.to_string(),
            headers,
            bearer_env,
            bearer_file,
            allow_insecure,
        }),
    }
}

/// Protocol version helpers.
pub mod protocol {
    use super::{McpError, MCP_PROTOCOL_REVISION};

    /// Returns true when the server-offered version matches our revision.
    pub fn is_supported(offered: &str) -> bool {
        offered == MCP_PROTOCOL_REVISION
    }

    /// Human-readable mismatch message naming required vs offered revision.
    pub fn version_mismatch_message(required: &str, offered: &str) -> String {
        format!(
            "MCP protocol version incompatible: required '{}' but server offered '{}'. \
             Upgrade the server or client so both speak '{}'; refusing to fall back silently.",
            required, offered, required
        )
    }

    /// Assert the offered version matches; missing version is also incompatible.
    pub fn assert_protocol_version(offered: Option<&str>) -> Result<String, McpError> {
        match offered {
            Some(v) if is_supported(v) => Ok(v.to_string()),
            Some(v) => Err(McpError::Incompatible {
                required: MCP_PROTOCOL_REVISION.to_string(),
                offered: v.to_string(),
            }),
            None => Err(McpError::Incompatible {
                required: MCP_PROTOCOL_REVISION.to_string(),
                offered: "<missing>".to_string(),
            }),
        }
    }
}

/// Plain MCP error enum (codebase standard is anyhow; no new deps).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum McpError {
    /// Protocol version mismatch. Never silently fall back.
    Incompatible {
        /// Revision required by this client.
        required: String,
        /// Revision offered by the server.
        offered: String,
    },
    /// Plain http:// to a non-loopback host without explicit opt-in.
    InsecureHttp {
        /// Redacted URL (host only, no credentials).
        url: String,
        /// Why it was rejected.
        reason: String,
    },
    /// Legacy SSE transport (`GET /sse`, `POST /messages`) is unsupported.
    LegacySseUnsupported {
        /// URL that looked like a legacy SSE endpoint.
        url: String,
    },
    /// Request timed out.
    Timeout {
        /// JSON-RPC method.
        method: String,
        /// Timeout in seconds.
        secs: u64,
    },
    /// Transport-level failure (stdio spawn, HTTP error, ...).
    Transport(String),
    /// JSON-RPC error or malformed response.
    RequestFailed(String),
    /// Invalid configuration.
    Config(String),
    /// Unknown server id.
    UnknownServer(String),
}

impl McpError {
    /// Build an incompatible-version error.
    pub fn incompatible(required: &str, offered: &str) -> Self {
        Self::Incompatible {
            required: required.to_string(),
            offered: offered.to_string(),
        }
    }

    /// Rendered message for [`McpError::Incompatible`].
    pub fn mismatch_text(required: &str, offered: &str) -> String {
        protocol::version_mismatch_message(required, offered)
    }

    /// Error naming legacy SSE as unsupported.
    pub fn legacy_sse(url: &str) -> Self {
        Self::LegacySseUnsupported {
            url: url.to_string(),
        }
    }
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incompatible { required, offered } => {
                write!(
                    f,
                    "{}",
                    protocol::version_mismatch_message(required, offered)
                )
            }
            Self::InsecureHttp { url, reason } => {
                write!(
                    f,
                    "Refusing insecure MCP HTTP endpoint '{}': {}. \
                     Use https://, a loopback host (localhost/127.0.0.0/8/::1), \
                     or set MCP_ALLOW_INSECURE=1 explicitly.",
                    url, reason
                )
            }
            Self::LegacySseUnsupported { url } => {
                write!(
                    f,
                    "Legacy SSE transport is unsupported for '{}': \
                     GET /sse and POST /messages were removed; \
                     use a Streamable HTTP endpoint instead.",
                    url
                )
            }
            Self::Timeout { method, secs } => {
                write!(f, "MCP request '{}' timed out after {}s", method, secs)
            }
            Self::Transport(msg) => write!(f, "MCP transport error: {}", msg),
            Self::RequestFailed(msg) => write!(f, "MCP request failed: {}", msg),
            Self::Config(msg) => write!(f, "MCP config error: {}", msg),
            Self::UnknownServer(id) => write!(f, "Unknown MCP server: {}", id),
        }
    }
}

impl std::error::Error for McpError {}

/// Gates for experimental MCP extensions.
///
/// - Apps (`TARK_MCP_APPS=1`) is opt-in and defaults to false.
/// - Tasks (`TARK_MCP_TASKS_EXPERIMENTAL=1`) is an unstable draft and is
///   never presented as stable: the client only advertises the `tasks`
///   capability when this flag is set, and all task-related UI must label
///   it experimental.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ExtensionGate {
    /// Whether MCP Apps extension is enabled. Default false.
    #[serde(default)]
    pub apps_enabled: bool,
    /// Whether draft Tasks is enabled. Default false, never stable.
    #[serde(default)]
    pub tasks_experimental: bool,
}

/// Parse a boolean env flag value. Only `"1"` enables (exact match after trim).
pub fn parse_flag_value(raw: Option<&str>) -> bool {
    matches!(raw.map(str::trim), Some("1"))
}

impl ExtensionGate {
    /// Read gates from the process environment.
    pub fn from_env() -> Self {
        Self {
            apps_enabled: parse_flag_value(std::env::var("TARK_MCP_APPS").ok().as_deref()),
            tasks_experimental: parse_flag_value(
                std::env::var("TARK_MCP_TASKS_EXPERIMENTAL").ok().as_deref(),
            ),
        }
    }

    /// Build from explicit values (useful for tests).
    pub fn new(apps_enabled: bool, tasks_experimental: bool) -> Self {
        Self {
            apps_enabled,
            tasks_experimental,
        }
    }
}

/// Tool definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// JSON Schema for input parameters
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

/// Resource definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceDef {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Resource description
    #[serde(default)]
    pub description: String,
    /// MIME type
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Content returned by the tool
    pub content: Vec<McpContent>,
    /// Whether the call resulted in an error
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// Content item in MCP responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64)
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String },
}

impl McpToolResult {
    /// Convert to string representation
    pub fn to_text(&self) -> String {
        self.content
            .iter()
            .map(|c| match c {
                McpContent::Text { text } => text.clone(),
                McpContent::Image { .. } => "[Image]".to_string(),
                McpContent::Resource { uri } => format!("[Resource: {}]", uri),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Server capabilities returned during initialization
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Whether server supports tools
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    /// Whether server supports resources
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
    /// Whether server supports prompts
    #[serde(default)]
    pub prompts: Option<PromptsCapability>,
    /// Whether server supports MCP Apps extension. Default false, back-compat.
    #[serde(default)]
    pub apps: bool,
    /// Whether server supports draft Tasks. Default false, never stable.
    #[serde(default)]
    pub tasks: bool,
}

/// Tools capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether tool list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

/// Resources capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Whether resource list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
    /// Whether server supports subscriptions
    #[serde(default)]
    pub subscribe: bool,
}

/// Prompts capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsCapability {
    /// Whether prompt list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

/// Summary returned by `McpServerManager::inspect`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpInspectSummary {
    /// Server id.
    pub server_id: String,
    /// Whether the server is enabled in memory.
    pub enabled: bool,
    /// Transport label (`stdio` / `streamable-http`).
    pub transport: String,
    /// Connection status display string.
    pub status: String,
    /// Server capabilities (empty when not connected).
    pub capabilities: ServerCapabilities,
    /// Number of discovered tools.
    pub tool_count: usize,
    /// Names of discovered tools.
    pub tool_names: Vec<String>,
    /// Extension gates in effect at inspect time.
    pub apps_enabled: bool,
    /// Extension gates in effect at inspect time (draft, never stable).
    pub tasks_experimental: bool,
}

/// Single conformance check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceCheck {
    /// Check name.
    pub name: String,
    /// Whether the check passed.
    pub passed: bool,
    /// Human-readable detail.
    pub detail: String,
}

/// Serializable conformance report for `conformance_check`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConformanceReport {
    /// Server id (and name when available: `id (name)`).
    pub server_id: String,
    /// Protocol revision asserted.
    pub revision: String,
    /// Transport label.
    pub transport: String,
    /// Individual checks.
    pub checks: Vec<ConformanceCheck>,
}

impl ConformanceReport {
    /// True when every check passed.
    pub fn all_passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_const_is_current() {
        assert_eq!(MCP_PROTOCOL_REVISION, "2026-07-28");
    }

    #[test]
    fn version_supported_only_exact() {
        assert!(protocol::is_supported("2026-07-28"));
        assert!(!protocol::is_supported("2024-11-05"));
        assert!(!protocol::is_supported(""));
    }

    #[test]
    fn version_mismatch_text_names_both_revisions() {
        let msg = protocol::version_mismatch_message("2026-07-28", "2024-11-05");
        assert!(msg.contains("2026-07-28"), "must name required: {}", msg);
        assert!(msg.contains("2024-11-05"), "must name offered: {}", msg);
        let err = McpError::incompatible("2026-07-28", "2024-11-05");
        let text = err.to_string();
        assert!(text.contains("2026-07-28"));
        assert!(text.contains("2024-11-05"));
    }

    #[test]
    fn assert_version_rejects_missing_and_mismatch() {
        assert!(protocol::assert_protocol_version(Some("2026-07-28")).is_ok());
        let err = protocol::assert_protocol_version(Some("2024-11-05")).unwrap_err();
        assert!(matches!(err, McpError::Incompatible { .. }));
        let missing = protocol::assert_protocol_version(None).unwrap_err();
        assert!(matches!(missing, McpError::Incompatible { .. }));
        assert!(missing.to_string().contains("2026-07-28"));
    }

    #[test]
    fn extension_gate_defaults_off_and_parses_env() {
        let g = ExtensionGate::default();
        assert!(!g.apps_enabled);
        assert!(!g.tasks_experimental);
        assert!(parse_flag_value(Some("1")));
        assert!(!parse_flag_value(Some("0")));
        assert!(!parse_flag_value(Some("true")));
        assert!(!parse_flag_value(None));
        assert!(!parse_flag_value(Some("  ")));
    }

    #[test]
    fn server_capabilities_back_compat_and_new_flags() {
        // Old payload without apps/tasks must default to false.
        let caps: ServerCapabilities = serde_json::from_value(serde_json::json!({
            "tools": {"listChanged": true}
        }))
        .unwrap();
        assert!(caps.tools.is_some());
        assert!(!caps.apps);
        assert!(!caps.tasks);
        let caps2: ServerCapabilities = serde_json::from_value(serde_json::json!({
            "apps": true, "tasks": true
        }))
        .unwrap();
        assert!(caps2.apps);
        assert!(caps2.tasks);
    }

    #[test]
    fn connection_state_display_and_mapping() {
        assert_eq!(ConnectionState::Disabled.to_string(), "Disabled");
        assert_eq!(ConnectionState::Connected.to_string(), "Connected");
        assert!(ConnectionState::Incompatible("x".into())
            .to_string()
            .contains("Incompatible"));
        assert_eq!(
            ConnectionState::from_status(&ConnectionStatus::Connected),
            ConnectionState::Connected
        );
        assert!(ConnectionState::Connected.is_connected());
        assert!(!ConnectionState::Degraded("d".into()).is_connected());
    }

    #[test]
    fn resolve_endpoint_stdio_by_default() {
        let server = crate::storage::McpServer {
            name: "x".into(),
            command: "npx".into(),
            args: vec![],
            env: HashMap::new(),
            enabled: true,
            capabilities: vec![],
            tark: None,
        };
        let ep = resolve_endpoint(&server);
        assert_eq!(ep.transport, McpServerTransport::Stdio);
        assert!(ep.http.is_none());
    }

    #[test]
    fn resolve_endpoint_reads_mcp_env_keys() {
        let mut env = HashMap::new();
        env.insert("MCP_URL".into(), "https://example.com/mcp".into());
        env.insert("MCP_BEARER_ENV".into(), "MY_TOKEN".into());
        env.insert("MCP_ALLOW_INSECURE".into(), "1".into());
        env.insert("MCP_HEADER_X_CUSTOM".into(), "v".into());
        let server = crate::storage::McpServer {
            name: "x".into(),
            command: "npx".into(),
            args: vec![],
            env,
            enabled: true,
            capabilities: vec![],
            tark: None,
        };
        let ep = resolve_endpoint(&server);
        assert_eq!(ep.transport, McpServerTransport::StreamableHttp);
        let http = ep.http.unwrap();
        assert_eq!(http.url, "https://example.com/mcp");
        assert_eq!(http.bearer_env.as_deref(), Some("MY_TOKEN"));
        assert!(http.allow_insecure);
        assert_eq!(http.headers.get("X-CUSTOM").map(String::as_str), Some("v"));
    }

    #[test]
    fn resolve_endpoint_reads_bearer_file_key() {
        let mut env = HashMap::new();
        env.insert("MCP_URL".into(), "http://localhost:3000/mcp".into());
        env.insert("MCP_BEARER_FILE".into(), "/run/tark/loopback.token".into());
        let server = crate::storage::McpServer {
            name: "x".into(),
            command: "npx".into(),
            args: vec![],
            env,
            enabled: true,
            capabilities: vec![],
            tark: None,
        };
        let ep = resolve_endpoint(&server);
        let http = ep.http.expect("http endpoint");
        assert_eq!(
            http.bearer_file.as_deref(),
            Some(std::path::Path::new("/run/tark/loopback.token"))
        );
        assert!(http.bearer_env.is_none());
    }

    #[test]
    fn resolve_endpoint_ignores_non_http_url() {
        let mut env = HashMap::new();
        env.insert("MCP_URL".into(), "ftp://example.com/x".into());
        let server = crate::storage::McpServer {
            name: "x".into(),
            command: "npx".into(),
            args: vec![],
            env,
            enabled: true,
            capabilities: vec![],
            tark: None,
        };
        assert_eq!(
            resolve_endpoint(&server).transport,
            McpServerTransport::Stdio
        );
    }
}
