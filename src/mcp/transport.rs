//! MCP transport implementations.
//!
//! Supports:
//! - STDIO: Spawn a child process and communicate via stdin/stdout (async)
//! - HTTP/SSE: Connect to HTTP endpoints (future)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

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
    /// Spawn a new MCP server process (async)
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

    /// Send a request and wait for response (async)
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // Serialize request
        let request_str = serde_json::to_string(&request)?;
        tracing::debug!("MCP request: {}", request_str);

        // Async write
        {
            let mut stdin = self.stdin.lock().await;
            stdin.write_all(request_str.as_bytes()).await?;
            stdin.write_all(b"\n").await?;
            stdin.flush().await?;
        }

        // Async read
        let response: JsonRpcResponse = {
            let mut stdout = self.stdout.lock().await;
            let mut line = String::new();
            stdout.read_line(&mut line).await?;
            tracing::debug!("MCP response: {}", line.trim());
            serde_json::from_str(&line)?
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

        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let notification_str = serde_json::to_string(&notification)?;
        tracing::debug!("MCP notification: {}", notification_str);

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
}
