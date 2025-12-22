//! Shell command execution tool

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

/// Dangerous command patterns that should be blocked
const DANGEROUS_PATTERNS: &[&str] = &[
    // Destructive file operations
    "rm -rf /",
    "rm -rf ~",
    "rm -rf /*",
    "rm -rf $HOME",
    "rm -rf .",
    "rm -rf ..",
    "> /dev/sda",
    "dd if=",
    "mkfs",
    "format ",
    // System modification
    "chmod -R 777",
    "chmod 777 /",
    "chown -R",
    // Privilege escalation
    "sudo rm",
    "sudo dd",
    "sudo mkfs",
    "sudo chmod",
    "sudo chown",
    "su -c",
    "su root",
    // Network attacks
    ":(){ :|:& };:", // Fork bomb
    "wget http",     // Downloading arbitrary scripts
    "curl http",     // Downloading arbitrary scripts (but allow https APIs)
    "nc -l",         // Netcat listener
    // Dangerous redirects
    "> /etc/",
    ">> /etc/",
    "> /var/",
    "> /usr/",
    "> /boot/",
    "> /sys/",
    "> /proc/",
    // Environment manipulation
    "export PATH=",
    "unset PATH",
    // Shutdown/reboot
    "shutdown",
    "reboot",
    "halt",
    "poweroff",
    "init 0",
    "init 6",
];

/// Commands that require extra caution but may be allowed
const WARN_PATTERNS: &[&str] = &[
    "rm -rf", // Recursive delete (but not root)
    "rm -r",  // Recursive delete
    "git push --force",
    "git reset --hard",
    "DROP TABLE",
    "DROP DATABASE",
    "DELETE FROM",
    "TRUNCATE",
];

/// Check if a command is dangerous
fn is_dangerous_command(cmd: &str) -> Option<&'static str> {
    let cmd_lower = cmd.to_lowercase();

    for pattern in DANGEROUS_PATTERNS {
        if cmd_lower.contains(&pattern.to_lowercase()) {
            return Some(pattern);
        }
    }

    // Special check for rm -rf with paths that could be dangerous
    if cmd_lower.contains("rm ") && (cmd_lower.contains(" -rf") || cmd_lower.contains(" -fr")) {
        // Check for dangerous path patterns
        let dangerous_paths = ["/", "/*", "~", "$HOME", "..", "../"];
        for path in dangerous_paths {
            if cmd.contains(path) && !cmd.contains(&format!("./{}", path)) {
                return Some("rm -rf with dangerous path");
            }
        }
    }

    None
}

/// Check if command should show a warning
fn should_warn(cmd: &str) -> Option<&'static str> {
    let cmd_lower = cmd.to_lowercase();
    WARN_PATTERNS
        .iter()
        .find(|&pattern| cmd_lower.contains(&pattern.to_lowercase()))
        .copied()
}

/// Tool for executing shell commands
pub struct ShellTool {
    working_dir: PathBuf,
}

impl ShellTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute a shell command and return its output. Use for running tests, builds, git commands, etc."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute"
                },
                "working_dir": {
                    "type": "string",
                    "description": "Optional: Working directory for the command (default: agent working directory)"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Optional: Timeout in seconds (default: 60)"
                }
            },
            "required": ["command"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            command: String,
            working_dir: Option<String>,
            timeout_secs: Option<u64>,
        }

        let params: Params = serde_json::from_value(params)?;

        // SAFETY CHECK: Block dangerous commands
        if let Some(pattern) = is_dangerous_command(&params.command) {
            return Ok(ToolResult::error(format!(
                "🚫 BLOCKED: This command matches a dangerous pattern: '{}'\n\n\
                This command could cause serious harm to the system and has been blocked.\n\
                If you believe this is a legitimate use case, please run it manually.",
                pattern
            )));
        }

        // SAFETY CHECK: Warn about risky commands
        if let Some(pattern) = should_warn(&params.command) {
            tracing::warn!(
                "Executing potentially risky command matching pattern: {}",
                pattern
            );
        }

        let working_dir = params
            .working_dir
            .map(|p| self.working_dir.join(p))
            .unwrap_or_else(|| self.working_dir.clone());
        let timeout = std::time::Duration::from_secs(params.timeout_secs.unwrap_or(60));

        // Determine shell based on OS
        let (shell, shell_arg) = if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        };

        let result = tokio::time::timeout(
            timeout,
            Command::new(shell)
                .arg(shell_arg)
                .arg(&params.command)
                .current_dir(&working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output(),
        )
        .await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                let mut result_text = String::new();

                if !stdout.is_empty() {
                    result_text.push_str(&stdout);
                }

                if !stderr.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push_str("\n--- stderr ---\n");
                    }
                    result_text.push_str(&stderr);
                }

                if output.status.success() {
                    if result_text.is_empty() {
                        result_text = "Command completed successfully (no output)".to_string();
                    }
                    Ok(ToolResult::success(result_text))
                } else {
                    let exit_code = output.status.code().unwrap_or(-1);
                    result_text.push_str(&format!("\nExit code: {}", exit_code));
                    Ok(ToolResult::error(result_text))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!(
                "Failed to execute command: {}",
                e
            ))),
            Err(_) => Ok(ToolResult::error(format!(
                "Command timed out after {} seconds",
                timeout.as_secs()
            ))),
        }
    }
}
