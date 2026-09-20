//! MCP transport implementations.
//!
//! Supports:
//! - STDIO: Spawn a child process and communicate via stdin/stdout (async)
//! - Streamable HTTP: POST JSON-RPC to a remote endpoint (async via reqwest)
//!
//! Legacy SSE (`GET /sse`, `POST /messages`) is explicitly unsupported.
//!
//! Logging policy: debug logs record method names and byte counts only.
//! Header values (notably `Authorization`) and full params that may contain
//! secrets are never logged.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use super::types::{HttpMcpConfig, McpError, MCP_PROTOCOL_REVISION};

/// Default per-request timeout (stdio read and HTTP round-trip).
pub const DEFAULT_REQUEST_TIMEOUT_SECS: u64 = 30;

/// Host environment variables inherited by MCP stdio children (R3).
///
/// Everything else must be provided explicitly via the server's configured
/// `env`. Kept to what runtimes need for resolution and temp files; notably
/// no `*_TOKEN`, `*_KEY`, or `*_SECRET` entries are inherited implicitly.
const MINIMAL_INHERITED_ENV: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "LANG",
    "LC_ALL",
    "TMPDIR",
    "TEMP",
    "TMP",
    "SystemRoot",
    "SYSTEMROOT",
    "PATHEXT",
    "NODE_PATH",
    "PYTHONPATH",
    "RUSTUP_HOME",
    "CARGO_HOME",
];

/// JSON-RPC request
#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

/// Returns true for loopback hosts: `localhost`, `127.0.0.0/8`, `::1`.
///
/// Comparison is case-insensitive; surrounding brackets (`[::1]`) and a
/// trailing dot (`localhost.`) are tolerated.
pub fn is_loopback_host(host: &str) -> bool {
    let h = host.trim().trim_start_matches('[').trim_end_matches(']');
    let h = h.strip_suffix('.').unwrap_or(h);
    if h.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if h == "::1" {
        return true;
    }
    // 127.0.0.0/8
    if let Some(rest) = h.strip_prefix("127.") {
        if !rest.is_empty()
            && rest
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        {
            // Basic numeric check; range check each octet 0-255 when possible.
            let mut ok = true;
            for part in rest.split('.') {
                match part.parse::<u32>() {
                    Ok(n) if n <= 255 => {}
                    _ => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok {
                return true;
            }
        }
    }
    // Bare 127.* without dot handled above; "127" alone is not loopback range notation.
    false
}

/// Returns true when a URL's host is loopback. Unparseable URLs return false.
pub fn is_loopback_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(parsed) => parsed.host_str().map(is_loopback_host).unwrap_or(false),
        Err(_) => false,
    }
}

/// Returns true when a URL looks like a legacy SSE endpoint.
///
/// Legacy SSE used `GET /sse` for the event stream and `POST /messages` for
/// client-to-server messages. Both are unsupported; use Streamable HTTP.
pub fn looks_like_legacy_sse_url(url: &str) -> bool {
    let path = match url::Url::parse(url) {
        Ok(parsed) => parsed.path().to_ascii_lowercase(),
        Err(_) => {
            let lower = url.to_ascii_lowercase();
            // Fall back to substring matching for unparseable inputs.
            return lower.contains("/sse") || lower.contains("/messages");
        }
    };
    path == "/sse"
        || path.starts_with("/sse/")
        || path.contains("/sse/")
        || path.ends_with("/sse")
        || path == "/messages"
        || path.starts_with("/messages/")
        || path.contains("/messages/")
        || path.ends_with("/messages")
        || path.contains("/messages?")
}

/// Validate an HTTP(S) MCP URL.
///
/// - Scheme must be `http` or `https`.
/// - Legacy SSE paths (`/sse`, `/messages`) are rejected as unsupported.
/// - Plain `http://` to a non-loopback host requires `allow_insecure`.
/// - Returns the redacted host for error messages (never credentials).
pub fn validate_http_url(url: &str, allow_insecure: bool) -> std::result::Result<(), McpError> {
    let parsed =
        url::Url::parse(url).map_err(|e| McpError::Config(format!("Invalid MCP URL: {}", e)))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(McpError::Config(format!(
                "Unsupported MCP URL scheme '{}': expected http(s)",
                other
            )));
        }
    }
    if looks_like_legacy_sse_url(url) {
        return Err(McpError::legacy_sse(url));
    }
    if parsed.scheme() == "http" && !allow_insecure {
        let host = parsed.host_str().unwrap_or("<unknown>");
        if !is_loopback_host(host) {
            return Err(McpError::InsecureHttp {
                url: host.to_string(),
                reason: "plain http to non-loopback host".to_string(),
            });
        }
    }
    Ok(())
}

/// Byte length of an optional JSON value when serialized (0 when None).
fn params_byte_len(params: &Option<Value>) -> usize {
    match params {
        Some(v) => serde_json::to_string(v).map(|s| s.len()).unwrap_or(0),
        None => 0,
    }
}

/// STDIO transport for MCP servers (async)
pub struct StdioTransport {
    /// Child process
    child: Arc<Mutex<Child>>,
    /// Request ID counter
    next_id: AtomicU64,
    /// Stdin writer
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    /// Stdout reader
    stdout: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
}

impl StdioTransport {
    /// Spawn a new MCP server process (async).
    ///
    /// The child is untrusted third-party code (R3 S7): it inherits only an
    /// explicit allowlist of host variables plus the server's configured
    /// environment — never the full parent environment.
    pub async fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&PathBuf>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()) // Pass stderr through for debugging
            .kill_on_drop(true); // Auto-cleanup on drop

        // Minimal inheritance: start from a clean environment (R3).
        cmd.env_clear();
        for key in MINIMAL_INHERITED_ENV {
            if let Some(value) = std::env::var_os(key) {
                cmd.env(key, value);
            }
        }

        // Set environment variables (expand ${VAR} references)
        for (key, value) in env {
            let expanded = expand_env_vars(value);
            cmd.env(key, expanded);
        }

        // Set working directory if specified
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {}", command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            next_id: AtomicU64::new(1),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
        })
    }

    /// Send a request and wait for response (async, 30s timeout).
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let params_bytes = params_byte_len(&params);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // Serialize request
        let request_str = serde_json::to_string(&request)?;
        // Redacted: method + byte counts only, never full params.
        tracing::debug!(
            "MCP stdio request method={} id={} params_bytes={} request_bytes={}",
            method,
            id,
            params_bytes,
            request_str.len()
        );

        // Async write
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(request_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Async read with per-request timeout (previously blocked indefinitely).
        let response: JsonRpcResponse = {
            let mut stdout = self.stdout.lock().await;
            let mut line = String::new();
            let read = tokio::time::timeout(
                Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
                stdout.read_line(&mut line),
            )
            .await
            .map_err(|_| McpError::Timeout {
                method: method.to_string(),
                secs: DEFAULT_REQUEST_TIMEOUT_SECS,
            })?
            .with_context(|| format!("Failed to read MCP response for '{}'", method))?;
            let _ = read;
            // Redacted: byte count only, never full response payload.
            tracing::debug!(
                "MCP stdio response method={} id={} response_bytes={}",
                method,
                id,
                line.len()
            );
            serde_json::from_str(&line)
                .with_context(|| format!("Failed to parse MCP response for '{}'", method))?
        };

        // Handle response
        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error {}: {}",
                error.code,
                error.message
            ));
        }

        response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))
    }

    /// Send a notification (no response expected) (async)
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        #[derive(Serialize)]
        struct JsonRpcNotification {
            jsonrpc: &'static str,
            method: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            params: Option<Value>,
        }

        let params_bytes = params_byte_len(&params);
        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let notification_str = serde_json::to_string(&notification)?;
        // Redacted: method + byte counts only.
        tracing::debug!(
            "MCP stdio notify method={} params_bytes={} request_bytes={}",
            method,
            params_bytes,
            notification_str.len()
        );

        let mut stdin = self.stdin.lock().await;
        stdin.write_all(notification_str.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;

        Ok(())
    }

    /// Check if the child process is still running (async)
    pub async fn is_alive(&self) -> bool {
        let mut child = self.child.lock().await;
        match child.try_wait() {
            Ok(None) => true,     // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,      // Error checking
        }
    }

    /// Kill the child process (async)
    pub async fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().await.context("Failed to kill MCP server")?;
        Ok(())
    }
}

// Note: No manual Drop needed - kill_on_drop(true) handles cleanup

/// Streamable HTTP transport for MCP servers (async via reqwest).
///
/// POSTs JSON-RPC to the configured URL with:
/// - `Accept: application/json, text/event-stream`
/// - `Content-Type: application/json`
/// - `MCP-Protocol-Version: 2026-07-28`
/// - Optional `Authorization: Bearer <token>` where the token is read at
///   request time from the file named by `bearer_file` (preferred, e.g. a
///   Tark-issued loopback credential) or the env var named by `bearer_env`
///   (never logged).
pub struct StreamableHttpTransport {
    /// HTTP client with request timeout.
    client: reqwest::Client,
    /// Endpoint config.
    config: HttpMcpConfig,
    /// Request ID counter.
    next_id: AtomicU64,
    /// Timeout per request.
    timeout: Duration,
}

impl StreamableHttpTransport {
    /// Create a new transport, validating URL + legacy SSE + insecure rules.
    pub fn new(config: HttpMcpConfig) -> std::result::Result<Self, McpError> {
        validate_http_url(&config.url, config.allow_insecure)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| McpError::Transport(format!("Failed to build HTTP client: {}", e)))?;
        Ok(Self {
            client,
            config,
            next_id: AtomicU64::new(1),
            timeout: Duration::from_secs(DEFAULT_REQUEST_TIMEOUT_SECS),
        })
    }

    /// Endpoint URL (for labels; may contain no credentials by construction).
    pub fn url(&self) -> &str {
        &self.config.url
    }

    fn bearer_token(&self) -> Option<String> {
        // Explicit file binding wins: it supports rotation without restart.
        // Refusals (symlink, lax permissions, missing file) fail closed to
        // unauthenticated, which the server rejects; the token value itself
        // is never logged.
        if let Some(path) = &self.config.bearer_file {
            match super::loopback_cred::read_token_file(path) {
                Ok(token) => return Some(token),
                Err(e) => {
                    tracing::debug!(
                        "MCP http bearer file unreadable host={} path={} error={}",
                        self.host_label(),
                        path.display(),
                        e
                    );
                    return None;
                }
            }
        }
        match &self.config.bearer_env {
            Some(name) => std::env::var(name).ok().filter(|v| !v.is_empty()),
            None => None,
        }
    }

    fn has_auth(&self) -> bool {
        self.config.bearer_env.is_some() || self.config.bearer_file.is_some()
    }

    fn host_label(&self) -> String {
        url::Url::parse(&self.config.url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_string))
            .unwrap_or_else(|| "<unknown-host>".to_string())
    }

    /// Send a request and wait for response (async, 30s timeout).
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let params_bytes = params_byte_len(&params);
        let has_auth = self.has_auth();
        // Redacted: method + sizes + host only; never header values or params.
        tracing::debug!(
            "MCP http request method={} id={} host={} params_bytes={} has_auth={}",
            method,
            id,
            self.host_label(),
            params_bytes,
            has_auth
        );

        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        // `params: None` serializes as null; strip it to match stdio shape.
        let mut body = body;
        if body.get("params").map(Value::is_null).unwrap_or(false) {
            if let Some(obj) = body.as_object_mut() {
                obj.remove("params");
            }
        }
        let request_bytes = serde_json::to_string(&body).map(|s| s.len()).unwrap_or(0);

        let mut req = self
            .client
            .post(&self.config.url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", MCP_PROTOCOL_REVISION)
            .json(&body);
        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(token) = self.bearer_token() {
            req = req.bearer_auth(token);
        }

        let resp = tokio::time::timeout(self.timeout, req.send())
            .await
            .map_err(|_| McpError::Timeout {
                method: method.to_string(),
                secs: DEFAULT_REQUEST_TIMEOUT_SECS,
            })?
            .map_err(|e| McpError::Transport(format!("HTTP request failed: {}", e)))?;

        let status = resp.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "MCP HTTP error {} for '{}' ({} bytes sent)",
                status,
                method,
                request_bytes
            ));
        }
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        let text = resp
            .text()
            .await
            .map_err(|e| McpError::Transport(format!("Failed to read HTTP body: {}", e)))?;
        tracing::debug!(
            "MCP http response method={} id={} host={} response_bytes={} content_type_len={}",
            method,
            id,
            self.host_label(),
            text.len(),
            content_type.len()
        );

        let response: JsonRpcResponse = if content_type.contains("text/event-stream") {
            parse_sse_jsonrpc(&text).map_err(|e| {
                McpError::RequestFailed(format!("Failed to parse SSE response: {}", e))
            })?
        } else {
            serde_json::from_str(&text).map_err(|e| {
                McpError::RequestFailed(format!("Failed to parse JSON response: {}", e))
            })?
        };

        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error {}: {}",
                error.code,
                error.message
            ));
        }
        response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))
    }

    /// Send a notification (no response expected) (async).
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        let params_bytes = params_byte_len(&params);
        tracing::debug!(
            "MCP http notify method={} host={} params_bytes={} has_auth={}",
            method,
            self.host_label(),
            params_bytes,
            self.has_auth()
        );
        let mut body = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        if body.get("params").map(Value::is_null).unwrap_or(false) {
            if let Some(obj) = body.as_object_mut() {
                obj.remove("params");
            }
        }
        let mut req = self
            .client
            .post(&self.config.url)
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .header("MCP-Protocol-Version", MCP_PROTOCOL_REVISION)
            .json(&body);
        for (k, v) in &self.config.headers {
            req = req.header(k.as_str(), v.as_str());
        }
        if let Some(token) = self.bearer_token() {
            req = req.bearer_auth(token);
        }
        let resp = tokio::time::timeout(self.timeout, req.send())
            .await
            .map_err(|_| McpError::Timeout {
                method: method.to_string(),
                secs: DEFAULT_REQUEST_TIMEOUT_SECS,
            })?
            .map_err(|e| McpError::Transport(format!("HTTP notify failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(anyhow::anyhow!(
                "MCP HTTP notify error {} for '{}'",
                resp.status(),
                method
            ));
        }
        Ok(())
    }

    /// HTTP endpoints have no child process; always report alive.
    pub async fn is_alive(&self) -> bool {
        true
    }
}

/// Parse a `text/event-stream` body, returning the last `data:` JSON block
/// that deserializes as a JSON-RPC response.
fn parse_sse_jsonrpc(text: &str) -> Result<JsonRpcResponse> {
    let mut last: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim();
            if !data.is_empty() && data != "[DONE]" {
                last = Some(data.to_string());
            }
        }
    }
    // Some servers return plain JSON even with an SSE content-type.
    let payload = last.unwrap_or_else(|| text.trim().to_string());
    serde_json::from_str(&payload).context("Failed to parse SSE JSON-RPC payload")
}

/// Unified handle over stdio and Streamable HTTP transports.
#[derive(Clone)]
pub enum ActiveTransport {
    /// Child-process stdio transport.
    Stdio(Arc<StdioTransport>),
    /// Remote Streamable HTTP transport.
    Http(Arc<StreamableHttpTransport>),
}

impl ActiveTransport {
    /// Transport label for reports.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stdio(_) => "stdio",
            Self::Http(_) => "streamable-http",
        }
    }

    /// Send a request via the active transport.
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        match self {
            Self::Stdio(t) => t.request(method, params).await,
            Self::Http(t) => t.request(method, params).await,
        }
    }

    /// Send a notification via the active transport.
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        match self {
            Self::Stdio(t) => t.notify(method, params).await,
            Self::Http(t) => t.notify(method, params).await,
        }
    }

    /// Shut down the transport (kills child process; no-op for HTTP).
    pub async fn shutdown(&self) -> Result<()> {
        match self {
            Self::Stdio(t) => t.kill().await,
            Self::Http(_) => Ok(()),
        }
    }

    /// Check liveness.
    pub async fn is_alive(&self) -> bool {
        match self {
            Self::Stdio(t) => t.is_alive().await,
            Self::Http(t) => t.is_alive().await,
        }
    }
}

/// Expand environment variable references like ${VAR} in a string
fn expand_env_vars(input: &str) -> String {
    let mut result = input.to_string();

    // Find all ${VAR} patterns
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();

    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        if let Ok(value) = std::env::var(var_name) {
            result = result.replace(&cap[0], &value);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "hello");
        assert_eq!(expand_env_vars("${TEST_VAR} world"), "hello world");
        assert_eq!(expand_env_vars("no vars here"), "no vars here");
        assert_eq!(expand_env_vars("${NONEXISTENT}"), "${NONEXISTENT}");
    }

    fn http_transport_with_bearer_file(path: std::path::PathBuf) -> StreamableHttpTransport {
        StreamableHttpTransport::new(HttpMcpConfig {
            url: "http://localhost:3000/mcp".to_string(),
            headers: std::collections::HashMap::new(),
            bearer_env: None,
            bearer_file: Some(path),
            allow_insecure: false,
        })
        .expect("transport")
    }

    #[test]
    fn bearer_file_token_read_at_request_time() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cred =
            crate::mcp::loopback_cred::LoopbackCredential::issue_in(dir.path()).expect("issue");
        let token =
            crate::mcp::loopback_cred::read_token_file(cred.path()).expect("read token file");
        let transport = http_transport_with_bearer_file(cred.path().to_path_buf());
        assert_eq!(transport.bearer_token().as_deref(), Some(token.as_str()));
        assert!(transport.has_auth());
    }
    #[test]
    fn bearer_file_takes_precedence_over_bearer_env() {
        let dir = tempfile::tempdir().expect("tempdir");
        let cred =
            crate::mcp::loopback_cred::LoopbackCredential::issue_in(dir.path()).expect("issue");
        std::env::set_var("TARK_TEST_BEARER_PRECEDENCE", "env-token");
        let transport = StreamableHttpTransport::new(HttpMcpConfig {
            url: "http://localhost:3000/mcp".to_string(),
            headers: std::collections::HashMap::new(),
            bearer_env: Some("TARK_TEST_BEARER_PRECEDENCE".to_string()),
            bearer_file: Some(cred.path().to_path_buf()),
            allow_insecure: false,
        })
        .expect("transport");
        let expected =
            crate::mcp::loopback_cred::read_token_file(cred.path()).expect("read token file");
        assert_eq!(transport.bearer_token().as_deref(), Some(expected.as_str()));
        std::env::remove_var("TARK_TEST_BEARER_PRECEDENCE");
    }

    #[test]
    fn bearer_file_refusal_fails_closed_to_unauthenticated() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("missing.token");
        let transport = http_transport_with_bearer_file(missing);
        assert!(transport.bearer_token().is_none());
        // Configured auth that cannot be read still advertises intent, but
        // no token value is ever attached to the request.
        assert!(transport.has_auth());
    }

    #[test]
    fn loopback_hosts_accepted() {
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LOCALHOST"));
        assert!(is_loopback_host("localhost."));
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.1.2.3"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));
        assert!(!is_loopback_host("example.com"));
        assert!(!is_loopback_host("128.0.0.1"));
        assert!(!is_loopback_host(""));
    }

    #[test]
    fn loopback_urls_detected() {
        assert!(is_loopback_url("http://localhost:3000/mcp"));
        assert!(is_loopback_url("http://127.0.0.1/mcp"));
        assert!(is_loopback_url("http://[::1]/mcp"));
        assert!(!is_loopback_url("http://example.com/mcp"));
        assert!(!is_loopback_url("not a url"));
    }

    #[test]
    fn insecure_http_rejected_unless_allowed_or_loopback() {
        // Non-loopback plain http requires opt-in.
        let err = validate_http_url("http://example.com/mcp", false).unwrap_err();
        assert!(matches!(err, McpError::InsecureHttp { .. }));
        assert!(err.to_string().contains("example.com"));
        // Opt-in allows it.
        assert!(validate_http_url("http://example.com/mcp", true).is_ok());
        // Loopback http is fine without opt-in.
        assert!(validate_http_url("http://localhost:3000/mcp", false).is_ok());
        assert!(validate_http_url("http://127.0.0.1/mcp", false).is_ok());
        // https is fine anywhere.
        assert!(validate_http_url("https://example.com/mcp", false).is_ok());
    }

    #[test]
    fn legacy_sse_rejected_explicitly() {
        for url in [
            "https://example.com/sse",
            "https://example.com/foo/sse/bar",
            "https://example.com/messages",
            "https://example.com/mcp/messages",
        ] {
            let err = validate_http_url(url, true).unwrap_err();
            assert!(
                matches!(err, McpError::LegacySseUnsupported { .. }),
                "expected legacy SSE error for {}",
                url
            );
            let text = err.to_string();
            assert!(text.contains("GET /sse"), "missing GET /sse: {}", text);
            assert!(text.contains("POST /messages"), "missing POST: {}", text);
        }
    }

    #[test]
    fn sse_parsing_extracts_last_data_block() {
        let body = "event: message\ndata: {\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"ok\":1}}\n\n";
        let resp = parse_sse_jsonrpc(body).unwrap();
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }
}
