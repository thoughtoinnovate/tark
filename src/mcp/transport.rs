//! MCP transport implementations.
//!
//! Supports:
//! - STDIO: Spawn a child process and communicate via stdin/stdout
//! - HTTP/SSE: Connect to HTTP endpoints (future)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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

/// STDIO transport for MCP servers
pub struct StdioTransport {
    /// Child process
    child: Arc<Mutex<Child>>,
    /// Request ID counter
    next_id: AtomicU64,
    /// Stdin writer
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    /// Stdout reader  
    stdout: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
}

impl StdioTransport {
    /// Spawn a new MCP server process
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&PathBuf>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Pass stderr through for debugging

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

    /// Send a request and wait for response
    pub fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // Serialize and send
        let request_str = serde_json::to_string(&request)?;
        tracing::debug!("MCP request: {}", request_str);

        {
            let mut stdin = self.stdin.lock().unwrap();
            writeln!(stdin, "{}", request_str)?;
            stdin.flush()?;
        }

        // Read response
        let response: JsonRpcResponse = {
            let mut stdout = self.stdout.lock().unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line)?;
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

    /// Send a notification (no response expected)
    pub fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
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

        let mut stdin = self.stdin.lock().unwrap();
        writeln!(stdin, "{}", notification_str)?;
        stdin.flush()?;

        Ok(())
    }

    /// Check if the child process is still running
    pub fn is_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        match child.try_wait() {
            Ok(None) => true,     // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,      // Error checking
        }
    }

    /// Kill the child process
    pub fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().unwrap();
        child.kill().context("Failed to kill MCP server")?;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Try to gracefully terminate
        let _ = self.kill();
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
}
