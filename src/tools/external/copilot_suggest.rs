//! GitHub Copilot code suggestion tool

use crate::tools::{Tool, ToolResult, RiskLevel, ToolCategory};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopilotSuggestTool;

#[async_trait]
impl Tool for CopilotSuggestTool {
    fn name(&self) -> &str {
        "copilot_suggest"
    }

    fn description(&self) -> &str {
        "Get GitHub Copilot code suggestions for a given context"
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the file"
                },
                "cursor_line": {
                    "type": "integer",
                    "description": "Optional: Cursor line number (0-indexed)"
                },
                "cursor_col": {
                    "type": "integer",
                    "description": "Optional: Cursor column number"
                }
            },
            "required": ["file_path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn category(&self) -> ToolCategory {
        ToolCategory::External
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let file_path = args["file_path"].as_str()
            .ok_or_else(|| anyhow::anyhow!("file_path required"))?;
        
        // In a real implementation, we would call the GitHub Copilot CLI or API.
        // For now, we'll implement the basic structure as described in the RFC.
        
        let output = tokio::process::Command::new("gh")
            .args(["copilot", "suggest", "-t", "shell", file_path])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Ok(ToolResult::error(format!("GitHub Copilot CLI failed: {}", stderr)));
        }
        
        let suggestions = String::from_utf8(output.stdout)?;
        
        Ok(ToolResult::success(suggestions))
    }
}
