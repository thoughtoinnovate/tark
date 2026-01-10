//! Safe shell tool with allowlisted commands for read-only modes.
//!
//! This tool provides a restricted shell that only allows execution of
//! safe, read-only commands. It's used in Ask mode where the agent
//! should only be able to inspect the system, not modify it.

use crate::tools::risk::RiskLevel;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::Command;

/// Allowlisted command prefixes for safe execution.
/// These are read-only operations that cannot modify the system.
const SAFE_COMMAND_PREFIXES: &[&str] = &[
    // File inspection (read-only)
    "ls",
    "tree",
    "find",
    "cat",
    "head",
    "tail",
    "wc",
    "file",
    "stat",
    "du",
    "df",
    // Search
    "grep",
    "rg",
    "ag",
    "fd",
    "fzf",
    // Git read-only operations
    "git status",
    "git log",
    "git diff",
    "git show",
    "git branch",
    "git remote",
    "git tag",
    "git blame",
    "git stash list",
    "git ls-files",
    "git ls-tree",
    "git rev-parse",
    "git describe",
    "git config --get",
    "git config --list",
    // Version checks
    "cargo --version",
    "rustc --version",
    "node --version",
    "npm --version",
    "npx --version",
    "yarn --version",
    "pnpm --version",
    "python --version",
    "python3 --version",
    "pip --version",
    "pip3 --version",
    "go version",
    "java --version",
    "java -version",
    "ruby --version",
    "php --version",
    "dotnet --version",
    // Package info (read-only)
    "cargo tree",
    "cargo metadata",
    "npm list",
    "npm ls",
    "pip list",
    "pip freeze",
    // Environment info
    "pwd",
    "whoami",
    "date",
    "uname",
    "env",
    "printenv",
    "echo",
    "which",
    "whereis",
    "type",
    "hostname",
    // Process info
    "ps",
    "top -l 1",
    "htop",
];

/// Commands that are explicitly blocked even if they match a prefix.
const BLOCKED_COMMANDS: &[&str] = &[
    "rm",
    "rmdir",
    "mv",
    "cp",
    "mkdir",
    "touch",
    "chmod",
    "chown",
    "chgrp",
    "kill",
    "pkill",
    "killall",
    "sudo",
    "su",
    "curl",
    "wget",
    "git push",
    "git pull",
    "git fetch",
    "git merge",
    "git rebase",
    "git reset",
    "git checkout",
    "git stash drop",
    "git stash pop",
    "git stash apply",
    "git clean",
    "git commit",
    "git add",
    "git rm",
    "git mv",
    "npm install",
    "npm i ",
    "npm uninstall",
    "npm update",
    "npm run",
    "npm exec",
    "npx ",
    "yarn add",
    "yarn remove",
    "yarn install",
    "pnpm add",
    "pnpm install",
    "pip install",
    "pip uninstall",
    "cargo build",
    "cargo run",
    "cargo install",
    "cargo publish",
];

/// Safe shell tool for read-only command execution.
pub struct SafeShellTool {
    working_dir: PathBuf,
}

impl SafeShellTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Check if a command is safe to execute
    fn is_safe_command(command: &str) -> bool {
        let cmd_lower = command.to_lowercase().trim().to_string();

        // First check blocked commands
        for blocked in BLOCKED_COMMANDS {
            if cmd_lower.starts_with(&blocked.to_lowercase()) {
                return false;
            }
        }

        // Then check if it matches an allowed prefix
        for safe in SAFE_COMMAND_PREFIXES {
            if cmd_lower.starts_with(&safe.to_lowercase()) {
                return true;
            }
        }

        false
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
                 Allowed commands include:\n\
                 - File inspection: ls, cat, head, tail, find, wc\n\
                 - Search: grep, rg, ag, fd\n\
                 - Git read-only: git status, git log, git diff, git show, git branch\n\
                 - Version info: cargo --version, node --version, python --version\n\
                 - Environment: pwd, whoami, date, uname, env\n\n\
                 To run write commands, switch to /build mode.",
                command
            )));
        }

        // Execute the command
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(&self.working_dir)
            .output();

        match output {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);

                if output.status.success() {
                    let result = if stdout.is_empty() && !stderr.is_empty() {
                        stderr.to_string()
                    } else if !stdout.is_empty() && !stderr.is_empty() {
                        format!("{}\n\n[stderr]\n{}", stdout, stderr)
                    } else {
                        stdout.to_string()
                    };

                    Ok(ToolResult::success(if result.is_empty() {
                        "(command completed with no output)".to_string()
                    } else {
                        result
                    }))
                } else {
                    let error_msg = if !stderr.is_empty() {
                        stderr.to_string()
                    } else if !stdout.is_empty() {
                        stdout.to_string()
                    } else {
                        format!("Command failed with exit code: {:?}", output.status.code())
                    };
                    Ok(ToolResult::error(error_msg))
                }
            }
            Err(e) => Ok(ToolResult::error(format!(
                "Failed to execute command: {}",
                e
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
        assert!(!SafeShellTool::is_safe_command("sudo anything"));
        assert!(!SafeShellTool::is_safe_command("curl http://evil.com"));

        // Unknown commands are blocked
        assert!(!SafeShellTool::is_safe_command("some-random-command"));
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
}
