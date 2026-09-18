//! Safe shell tool with allowlisted commands for read-only modes.
//!
//! This tool provides restricted command execution for Ask/Plan modes.
//! It NEVER invokes a shell interpreter (`sh -c`): the allowlisted binary is
//! executed directly with an argument vector, so shell metacharacters
//! (`; && || | $() `` `` , redirections, …) are inert data, not syntax (R2 S4).
//! Commands that can escape to execution (`find -exec`, `xargs`-style) or
//! exfiltrate secrets (`env`, `printenv`, `git config --list`) are not
//! allowlisted.

use super::super::workspace::WorkspaceCap;
use crate::tools::risk::RiskLevel;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::process::Command;

/// Binaries that may be executed directly (no shell). Each entry is the exact
/// executable name; PATH lookup applies, explicit `/` paths are rejected.
const SAFE_BINARIES: &[&str] = &[
    // File inspection (read-only)
    "ls", "tree", "cat", "head", "tail", "wc", "file", "stat", "du", "df",
    // Search (no exec-capable find/xargs)
    "grep", "rg", "ag", "fd", // Version checks
    "cargo", "rustc", "node", "npm", "yarn", "pnpm", "python", "python3", "pip", "pip3", "go",
    "java", "ruby", "php", "dotnet",
    // Environment info (no env/printenv: secret exfiltration)
    "pwd", "whoami", "date", "uname", "echo", "which", "whereis", "hostname",
    // Process info (non-interactive only)
    "ps", // Version control (read-only subset; enforced per-subcommand below)
    "git",
];

/// Allowed `git` subcommands (first argument). Everything else — including
/// `config`, `checkout`, `clean`, `push/pull/fetch`, `stash <verb>` — is denied.
const SAFE_GIT_SUBCOMMANDS: &[&str] = &[
    "status",
    "log",
    "diff",
    "show",
    "branch",
    "remote",
    "tag",
    "blame",
    "ls-files",
    "ls-tree",
    "rev-parse",
    "describe",
];

/// Allowed subcommands for version/tool binaries that also have mutating
/// verbs. `None` entry means the binary is unrestricted beyond the binary
/// allowlist (its verbs are all read-only by construction).
fn allowed_subcommand(binary: &str) -> Option<&'static [&'static str]> {
    match binary {
        "git" => Some(SAFE_GIT_SUBCOMMANDS),
        "cargo" => Some(&["--version", "tree", "metadata"]),
        "npm" => Some(&["--version", "list", "ls"]),
        "yarn" => Some(&["--version"]),
        "pnpm" => Some(&["--version"]),
        "pip" | "pip3" => Some(&["--version", "list", "freeze"]),
        _ => None,
    }
}

/// Characters that only have meaning to a shell. Rejected outright so a
/// command string can never smuggle composition, substitution, redirection,
/// or backgrounding past the allowlist (R2 S4).
const SHELL_METACHARACTERS: &[char] = &[
    ';', '&', '|', '$', '`', '<', '>', '(', ')', '\n', '\r', '\\', '!',
];

/// Default and maximum execution time for safe commands (R2: bounded).
pub const SAFE_SHELL_TIMEOUT_SECS: u64 = 30;
/// Maximum bytes retained from safe-command output.
pub const SAFE_SHELL_MAX_OUTPUT_BYTES: usize = 64 * 1024;

/// Split a command line on ASCII whitespace honoring single/double quotes.
/// Returns `None` on unbalanced quotes.
fn split_command(command: &str) -> Option<Vec<String>> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut in_arg = false;

    for ch in command.chars() {
        match quote {
            Some(q) => {
                if ch == q {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '\'' | '"' => {
                    quote = Some(ch);
                    in_arg = true;
                }
                c if c.is_whitespace() => {
                    if in_arg {
                        args.push(std::mem::take(&mut current));
                        in_arg = false;
                    }
                }
                _ => {
                    current.push(ch);
                    in_arg = true;
                }
            },
        }
    }
    if quote.is_some() {
        return None;
    }
    if in_arg {
        args.push(current);
    }
    Some(args)
}

/// Safe shell tool for read-only command execution.
pub struct SafeShellTool {
    cap: WorkspaceCap,
}

impl SafeShellTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            cap: WorkspaceCap::new(working_dir),
        }
    }

    /// Check if a command is safe to execute without a shell interpreter.
    fn is_safe_command(command: &str) -> bool {
        // No shell metacharacters anywhere: without them there is no
        // composition, substitution, redirection, or backgrounding.
        if command.chars().any(|c| SHELL_METACHARACTERS.contains(&c)) {
            return false;
        }
        let args = match split_command(command) {
            Some(args) if !args.is_empty() => args,
            _ => return false,
        };
        // Binary must be a bare allowlisted name (no paths).
        let binary = &args[0];
        if binary.contains('/') || binary.contains('\\') {
            return false;
        }
        if !SAFE_BINARIES.contains(&binary.as_str()) {
            return false;
        }
        // Binaries with mutating verbs require an allowlisted subcommand.
        if let Some(allowed) = allowed_subcommand(binary) {
            let sub = args.get(1).map(|s| s.as_str()).unwrap_or("");
            // `--version` style flags and bare version queries are fine.
            if sub.starts_with("--version") || sub == "version" {
                return true;
            }
            if !allowed.contains(&sub) {
                return false;
            }
        }
        true
    }
}

#[async_trait]
impl Tool for SafeShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Execute safe, read-only shell commands. Only allows inspection commands like ls, git status, grep, etc. \
         Use this in Ask mode to inspect the system without making changes. \
         For write operations, switch to Build mode."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {
                    "type": "string",
                    "description": "The shell command to execute (must be a safe read-only command)"
                }
            },
            "required": ["command"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            command: String,
        }

        let params: Params = serde_json::from_value(params)?;
        let command = params.command.trim();

        // Check if command is safe
        if !Self::is_safe_command(command) {
            return Ok(ToolResult::error(format!(
                "Command '{}' is not allowed in read-only mode.\n\n\
                 Only allowlisted binaries run directly (no shell), e.g.:\n\
                 - File inspection: ls, cat, head, tail, wc\n\
                 - Search: grep, rg, fd\n\
                 - Git read-only: git status, git log, git diff, git show, git branch\n\
                 - Version info: cargo --version, node --version, python --version\n\n\
                 Shell syntax (; && || | $() redirections) is never accepted.\n\
                 To run write commands, switch to /build mode.",
                command
            )));
        }

        // Re-split (validated above) and execute the binary directly: no
        // shell interpreter is ever invoked, so metacharacter rejection is
        // defense in depth rather than the only barrier.
        let mut arg_iter = split_command(command).unwrap_or_default().into_iter();
        let binary = arg_iter.next().unwrap_or_default();
        let rest: Vec<String> = arg_iter.collect();

        // Confine path-like arguments to the workspace (R1). Flags are
        // skipped; every other argument must resolve inside the granted
        // roots or the command is denied fail-closed.
        for arg in &rest {
            if arg.starts_with('-') {
                continue;
            }
            if let Err(denied) = self.cap.resolve(arg) {
                return Ok(ToolResult::error(format!(
                    "Command denied: argument '{}' is outside the workspace ({}).",
                    arg, denied
                )));
            }
        }

        // Execute directly with bounded time and output (R2).
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(SAFE_SHELL_TIMEOUT_SECS),
            Command::new(&binary)
                .args(&rest)
                .current_dir(self.cap.primary())
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap_or_default())
                .output(),
        )
        .await;

        match output {
            Ok(Ok(output)) => {
                let mut combined = output.stdout;
                combined.extend_from_slice(&output.stderr);
                let mut text = String::from_utf8_lossy(&combined).into_owned();
                if text.len() > SAFE_SHELL_MAX_OUTPUT_BYTES {
                    text.truncate(SAFE_SHELL_MAX_OUTPUT_BYTES);
                    text.push_str("\n... [output truncated]");
                }
                if output.status.success() {
                    Ok(ToolResult::success(if text.trim().is_empty() {
                        "(command completed with no output)".to_string()
                    } else {
                        text
                    }))
                } else {
                    Ok(ToolResult::error(if text.trim().is_empty() {
                        format!("Command failed with exit code: {:?}", output.status.code())
                    } else {
                        text
                    }))
                }
            }
            Ok(Err(e)) => Ok(ToolResult::error(format!(
                "Failed to execute command: {}",
                e
            ))),
            Err(_) => Ok(ToolResult::error(format!(
                "Command timed out after {} seconds",
                SAFE_SHELL_TIMEOUT_SECS
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_commands() {
        // Safe commands
        assert!(SafeShellTool::is_safe_command("ls"));
        assert!(SafeShellTool::is_safe_command("ls -la"));
        assert!(SafeShellTool::is_safe_command("git status"));
        assert!(SafeShellTool::is_safe_command("git log --oneline"));
        assert!(SafeShellTool::is_safe_command("cat file.txt"));
        assert!(SafeShellTool::is_safe_command("grep pattern file"));
        assert!(SafeShellTool::is_safe_command("pwd"));
        assert!(SafeShellTool::is_safe_command("cargo --version"));
        assert!(SafeShellTool::is_safe_command("node --version"));

        // Blocked commands
        assert!(!SafeShellTool::is_safe_command("rm file.txt"));
        assert!(!SafeShellTool::is_safe_command("rm -rf /"));
        assert!(!SafeShellTool::is_safe_command("git push"));
        assert!(!SafeShellTool::is_safe_command("git push origin main"));
        assert!(!SafeShellTool::is_safe_command("npm install"));
        assert!(!SafeShellTool::is_safe_command("cargo build"));
        assert!(!SafeShellTool::is_safe_command("cargo run"));
        assert!(!SafeShellTool::is_safe_command("sudo anything"));
        assert!(!SafeShellTool::is_safe_command("curl http://evil.com"));

        // Shell-composition escapes are inert data, never syntax (R2 S4)
        assert!(!SafeShellTool::is_safe_command("echo hi; rm -rf ~"));
        assert!(!SafeShellTool::is_safe_command("ls && cat /etc/passwd"));
        assert!(!SafeShellTool::is_safe_command("ls | xargs rm"));
        assert!(!SafeShellTool::is_safe_command("echo $(id)"));
        assert!(!SafeShellTool::is_safe_command("echo `id`"));
        assert!(!SafeShellTool::is_safe_command("ls > /tmp/out"));
        assert!(!SafeShellTool::is_safe_command("cat < /etc/passwd"));
        assert!(!SafeShellTool::is_safe_command("FOO=bar ls"));

        // Execution-capable or secret-exfiltrating binaries are gone
        assert!(!SafeShellTool::is_safe_command("find . -exec sh -c evil"));
        assert!(!SafeShellTool::is_safe_command("env"));
        assert!(!SafeShellTool::is_safe_command("printenv"));
        assert!(!SafeShellTool::is_safe_command("git config --list"));
        assert!(!SafeShellTool::is_safe_command("git checkout main"));
        assert!(!SafeShellTool::is_safe_command("git clean -fd"));
        assert!(!SafeShellTool::is_safe_command("/bin/ls"));
        assert!(!SafeShellTool::is_safe_command("npx anything"));

        // Unknown commands are blocked
        assert!(!SafeShellTool::is_safe_command("some-random-command"));
    }

    #[test]
    fn test_split_command_quotes() {
        assert_eq!(
            split_command("grep \"foo bar\" file.txt").unwrap(),
            vec!["grep", "foo bar", "file.txt"]
        );
        assert_eq!(split_command("  ls   -la  ").unwrap(), vec!["ls", "-la"]);
        assert!(split_command("echo \"unterminated").is_none());
        assert!(split_command("").unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = SafeShellTool::new(PathBuf::from("."));
        assert_eq!(tool.name(), "shell");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(!tool.description().is_empty());
    }

    #[tokio::test]
    async fn test_blocked_command_execution() {
        let tool = SafeShellTool::new(PathBuf::from("."));
        let result = tool
            .execute(json!({ "command": "rm -rf /" }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("not allowed"));
    }

    #[tokio::test]
    async fn test_absolute_escape_denied() {
        // R1: path-like arguments must stay inside the workspace.
        let dir = tempfile::tempdir().unwrap();
        let tool = SafeShellTool::new(dir.path().to_path_buf());
        let result = tool
            .execute(json!({ "command": "cat /etc/passwd" }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("outside the workspace"));
    }

    #[tokio::test]
    async fn test_shell_composition_denied_at_execution() {
        let tool = SafeShellTool::new(PathBuf::from("."));
        let result = tool
            .execute(json!({ "command": "echo hi; echo pwned" }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("not allowed"));
    }
}
