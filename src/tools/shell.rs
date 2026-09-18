//! Shell command execution tool

use super::workspace::WorkspaceCap;
use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
#[cfg(not(unix))]
use tokio::process::Command;

/// Maximum bytes retained from a child process (stdout+stderr combined).
/// Exceeding this kills the process and reports truncation (R2: output bounds).
pub const MAX_CHILD_OUTPUT_BYTES: usize = 256 * 1024;
/// Maximum lines retained from a child process.
pub const MAX_CHILD_OUTPUT_LINES: usize = 2000;

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
    cap: WorkspaceCap,
}

impl ShellTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            cap: WorkspaceCap::new(working_dir),
        }
    }
}

#[allow(clippy::manual_is_multiple_of)]
fn is_multiple_of_50(value: usize) -> bool {
    value % 50 == 0
}

/// Guard that kills a spawned process *group* when dropped.
///
/// The child is started as a process-group leader (`setsid`); dropping this
/// guard (timeout, user interrupt via the registry watcher, runtime shutdown)
/// sends `SIGKILL` to the whole group so descendants cannot outlive the tool
/// call (R2 S5). Firing is best-effort and idempotent. Disarm promptly once
/// the child is reaped so a recycled group id is never signalled.
struct ProcessGroupGuard {
    #[cfg(unix)]
    pgid: i32,
    disarmed: bool,
}

impl ProcessGroupGuard {
    fn new(#[cfg(unix)] pgid: i32, #[cfg(not(unix))] _pgid: i32) -> Self {
        Self {
            #[cfg(unix)]
            pgid,
            disarmed: false,
        }
    }

    /// The child exited on its own; do not kill.
    fn disarm(&mut self) {
        self.disarmed = true;
    }

    /// Kill the group now (e.g. output bound exceeded). Best-effort.
    fn kill_group(&self) {
        #[cfg(unix)]
        {
            if !self.disarmed && self.pgid > 0 {
                // Negative pid targets the process group.
                unsafe {
                    libc::killpg(self.pgid, libc::SIGKILL);
                }
            }
        }
    }
}

impl Drop for ProcessGroupGuard {
    fn drop(&mut self) {
        self.kill_group();
    }
}

/// Spawn `shell -c command` as a process-group leader (Unix `setsid`) so the
/// whole tree can be reaped on timeout/cancel via [`ProcessGroupGuard`].
fn spawn_grouped(
    shell: &str,
    shell_args: &[&str],
    command: &str,
    working_dir: &std::path::Path,
) -> std::io::Result<std::process::Child> {
    let mut std_cmd = std::process::Command::new(shell);
    std_cmd.args(shell_args);
    std_cmd.arg(command);
    std_cmd.current_dir(working_dir);
    std_cmd.stdout(Stdio::piped());
    std_cmd.stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setsid() is async-signal-safe; the closure does nothing else.
        unsafe {
            std_cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    std_cmd.spawn()
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

        let working_dir = match params.working_dir {
            Some(ref p) => match self.cap.resolve(p) {
                Ok(path) => path,
                Err(denied) => return Ok(ToolResult::error(denied.to_string())),
            },
            None => self.cap.primary().to_path_buf(),
        };
        let timeout = std::time::Duration::from_secs(
            params
                .timeout_secs
                .unwrap_or(60)
                .clamp(1, super::MAX_TOOL_TIMEOUT_SECS),
        );

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
            let mut std_child = spawn_grouped(shell, shell_args, &params.command, &working_dir)?;
            // Dropping `group_guard` kills the whole process group (R2 S5).
            // It fires on timeout, registry interrupt, or runtime abort.
            // Disarm promptly once the child is reaped (see below).
            let mut group_guard = ProcessGroupGuard::new(std_child.id() as i32);

            let stdout = std_child.stdout.take().expect("stdout was piped");
            let stderr = std_child.stderr.take().expect("stderr was piped");
            let mut stdout_reader =
                BufReader::new(tokio::process::ChildStdout::from_std(stdout)?).lines();
            let mut stderr_reader =
                BufReader::new(tokio::process::ChildStderr::from_std(stderr)?).lines();

            let mut stdout_lines: Vec<String> = Vec::new();
            let mut stderr_lines: Vec<String> = Vec::new();
            let mut output_bytes: usize = 0;
            let mut truncated = false;

            // Push a line unless a bound is hit; returns false when full.
            macro_rules! push_line {
                ($lines:expr, $line:expr) => {{
                    output_bytes += $line.len() + 1;
                    if $lines.len() >= MAX_CHILD_OUTPUT_LINES
                        || output_bytes > MAX_CHILD_OUTPUT_BYTES
                    {
                        truncated = true;
                        false
                    } else {
                        $lines.push($line);
                        true
                    }
                }};
            }

            // Read stdout and stderr concurrently using select
            loop {
                tokio::select! {
                    biased;

                    line = stdout_reader.next_line() => {
                        match line {
                            Ok(Some(l)) => {
                                if !push_line!(stdout_lines, l) {
                                    break;
                                }
                            }
                            Ok(None) => {
                                // stdout closed, drain stderr and wait for process
                                while let Ok(Some(l)) = stderr_reader.next_line().await {
                                    if !push_line!(stderr_lines, l) {
                                        break;
                                    }
                                    // Yield periodically to keep TUI responsive
                        if is_multiple_of_50(stderr_lines.len()) {
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
                            Ok(Some(l)) => {
                                if !push_line!(stderr_lines, l) {
                                    break;
                                }
                            }
                            Ok(None) => {} // stderr closed, continue reading stdout
                            Err(e) => {
                                tracing::warn!("Error reading stderr: {}", e);
                            }
                        }
                    }
                }

                // Yield periodically to keep TUI responsive during long output
                if is_multiple_of_50(stdout_lines.len() + stderr_lines.len()) {
                    tokio::task::yield_now().await;
                }
            }

            if truncated {
                // Stop the flood at the source instead of retaining it.
                group_guard.kill_group();
                let _ = std_child.kill();
            }
            // Reap the child without blocking the executor.
            let status = loop {
                if let Some(status) = std_child.try_wait()? {
                    // Reaped: disarm before the guard drops so a recycled
                    // group id is never signalled.
                    group_guard.disarm();
                    break status;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            };
            Ok::<_, std::io::Error>((status, stdout_lines, stderr_lines, truncated))
        })
        .await;

        match result {
            Ok(Ok((status, stdout_lines, stderr_lines, truncated))) => {
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

                if truncated {
                    result_text.push_str(&format!(
                        "\n... [output truncated at {} lines / {} bytes; process stopped]",
                        MAX_CHILD_OUTPUT_LINES, MAX_CHILD_OUTPUT_BYTES
                    ));
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
                "Command timed out after {} seconds (process and descendants stopped)",
                timeout.as_secs()
            ))),
        }
    }
}
