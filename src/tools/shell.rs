//! Shell command execution tool

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
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

    fn risk_level(&self) -> super::RiskLevel {
        super::RiskLevel::Risky
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

        // Determine shell based on OS (prefer bash/Powershell, fallback to sh/cmd)
        // Note: We use -c instead of -lc to avoid slow login shell initialization
        let (shell, shell_args): (&str, &[&str]) = if cfg!(windows) {
            // Check for PowerShell; fallback to cmd if unavailable
            let ps_available = std::process::Command::new("powershell")
                .arg("-NoProfile")
                .arg("-NonInteractive")
                .arg("-Command")
                .arg("Write-Output ok")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .is_ok();

            if ps_available {
                (
                    "powershell",
                    &["-NoProfile", "-NonInteractive", "-Command"] as &[&str],
                )
            } else {
                ("cmd", &["/C"] as &[&str])
            }
        } else {
            // Prefer bash; fallback to sh if unavailable
            // Use -c (non-login) instead of -lc to avoid slow shell initialization
            let bash_available = std::process::Command::new("bash")
                .arg("-c")
                .arg("echo ok")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .is_ok();

            if bash_available {
                ("bash", &["-c"] as &[&str])
            } else {
                ("sh", &["-c"] as &[&str])
            }
        };

        // Use spawn() with incremental reading to allow the TUI event loop to remain responsive
        let result = tokio::time::timeout(timeout, async {
            let mut child = Command::new(shell)
                .args(shell_args)
                .arg(&params.command)
                .current_dir(&working_dir)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?;

            let stdout = child.stdout.take().expect("stdout was piped");
            let stderr = child.stderr.take().expect("stderr was piped");

            let mut stdout_reader = BufReader::new(stdout).lines();
            let mut stderr_reader = BufReader::new(stderr).lines();

            let mut stdout_lines: Vec<String> = Vec::new();
            let mut stderr_lines: Vec<String> = Vec::new();

            // Read stdout and stderr concurrently using select
            loop {
                tokio::select! {
                    biased;

                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => stdout_lines.push(l),
                            Ok(None) => {
                                // stdout closed, drain stderr and wait for process
                                while let Ok(Some(l)) = stderr_reader.next_line().await {
                                    stderr_lines.push(l);
                                    // Yield periodically to keep TUI responsive
                        if stderr_lines.len() % 50 == 0 {
                                        tokio::task::yield_now().await;
                                    }
                                }
                                break;
                            }
                            Err(e) => {
                                tracing::warn!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }
                    line = stderr_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => stderr_lines.push(l),
                            Ok(None) => {} // stderr closed, continue reading stdout
                            Err(e) => {
                                tracing::warn!("Error reading stderr: {}", e);
                            }
                        }
                    }
                }

                // Yield periodically to keep TUI responsive during long output
                if (stdout_lines.len() + stderr_lines.len()) % 50 == 0 {
                    tokio::task::yield_now().await;
                }
            }

            let status = child.wait().await?;
            Ok::<_, std::io::Error>((status, stdout_lines, stderr_lines))
        })
        .await;

        match result {
            Ok(Ok((status, stdout_lines, stderr_lines))) => {
                let mut result_text = String::new();

                if !stdout_lines.is_empty() {
                    result_text.push_str(&stdout_lines.join("\n"));
                }

                if !stderr_lines.is_empty() {
                    if !result_text.is_empty() {
                        result_text.push_str("\n--- stderr ---\n");
                    }
                    result_text.push_str(&stderr_lines.join("\n"));
                }

                if status.success() {
                    if result_text.is_empty() {
                        result_text = "Command completed successfully (no output)".to_string();
                    }
                    Ok(ToolResult::success(result_text))
                } else {
                    let exit_code = status.code().unwrap_or(-1);
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
