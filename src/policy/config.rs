use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// User-defined MCP tool policy configuration
#[derive(Debug, Deserialize, Default)]
pub struct McpConfig {
    pub version: Option<u32>,
    #[serde(default)]
    pub tools: Vec<McpToolConfig>,
    #[serde(default)]
    pub patterns: Vec<McpPatternConfig>,
}

#[derive(Debug, Deserialize)]
pub struct McpToolConfig {
    pub server: String,
    pub tool: String,
    pub risk: String,
    pub needs_approval: bool,
    #[serde(default = "default_true")]
    pub allow_save_pattern: bool,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct McpPatternConfig {
    pub server: String,
    pub tool: String,
    pub pattern: String,
    pub match_type: String,
    pub action: String, // "allow" or "deny"
    pub description: Option<String>,
}

fn default_true() -> bool {
    true
}

/// Loads and syncs user MCP configurations
pub struct ConfigLoader {
    user_config_path: PathBuf,
    workspace_config_path: Option<PathBuf>,
}

impl ConfigLoader {
    pub fn new(workspace_dir: Option<&Path>) -> Self {
        let user_config_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tark")
            .join("policy")
            .join("mcp.toml");

        let workspace_config_path = workspace_dir.map(|w| w.join(".tark/policy/mcp.toml"));

        Self {
            user_config_path,
            workspace_config_path,
        }
    }

    /// Load user MCP config
    fn load_user_config(&self) -> Result<Option<McpConfig>> {
        if !self.user_config_path.exists() {
            tracing::debug!("No user MCP config found at {:?}", self.user_config_path);
            return Ok(None);
        }

        let content = std::fs::read_to_string(&self.user_config_path)?;
        let config: McpConfig = toml::from_str(&content)?;
        tracing::info!("Loaded user MCP config with {} tools", config.tools.len());
        Ok(Some(config))
    }

    /// Load workspace MCP config
    fn load_workspace_config(&self) -> Result<Option<McpConfig>> {
        if let Some(path) = &self.workspace_config_path {
            if path.exists() {
                let content = std::fs::read_to_string(path)?;
                let config: McpConfig = toml::from_str(&content)?;
                tracing::info!(
                    "Loaded workspace MCP config with {} tools",
                    config.tools.len()
                );
                return Ok(Some(config));
            }
        }
        Ok(None)
    }

    /// Load and merge configs (workspace overrides user)
    pub fn load(&self) -> Result<McpConfig> {
        let mut merged = McpConfig::default();

        // Load user config
        if let Some(user) = self.load_user_config()? {
            merged.tools.extend(user.tools);
            merged.patterns.extend(user.patterns);
        }

        // Load workspace config (overrides user)
        if let Some(workspace) = self.load_workspace_config()? {
            // Workspace tools override user tools by (server, tool) key
            for ws_tool in workspace.tools {
                // Remove existing user tool if same (server, tool)
                merged
                    .tools
                    .retain(|t| !(t.server == ws_tool.server && t.tool == ws_tool.tool));
                merged.tools.push(ws_tool);
            }

            merged.patterns.extend(workspace.patterns);
        }

        Ok(merged)
    }

    /// Sync config to database
    pub fn sync_to_db(&self, conn: &Connection) -> Result<()> {
        let config = self.load()?;

        conn.execute("BEGIN IMMEDIATE", [])?;

        // Clear existing user/workspace policies (keep session patterns)
        conn.execute(
            "DELETE FROM mcp_tool_policies WHERE source IN ('user', 'workspace')",
            [],
        )?;
        conn.execute(
            "DELETE FROM mcp_approval_patterns WHERE source IN ('user', 'workspace')",
            [],
        )?;

        // Insert tool policies
        for tool in &config.tools {
            let source = if self
                .workspace_config_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false)
            {
                "workspace"
            } else {
                "user"
            };

            conn.execute(
                "INSERT INTO mcp_tool_policies (server_id, tool_name, risk_level, needs_approval, allow_save_pattern, description, source, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))",
                (
                    &tool.server,
                    &tool.tool,
                    &tool.risk,
                    tool.needs_approval,
                    tool.allow_save_pattern,
                    &tool.description,
                    source,
                ),
            )?;
        }
        tracing::debug!("Synced {} MCP tool policies", config.tools.len());

        // Insert patterns
        for pattern in &config.patterns {
            let source = if self
                .workspace_config_path
                .as_ref()
                .map(|p| p.exists())
                .unwrap_or(false)
            {
                "workspace"
            } else {
                "user"
            };

            let is_denial = pattern.action == "deny";

            conn.execute(
                "INSERT INTO mcp_approval_patterns (server_id, tool_name, pattern, match_type, is_denial, source, created_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7)",
                (
                    &pattern.server,
                    &pattern.tool,
                    &pattern.pattern,
                    &pattern.match_type,
                    is_denial,
                    source,
                    &pattern.description,
                ),
            )?;
        }
        tracing::debug!("Synced {} MCP patterns", config.patterns.len());

        conn.execute("COMMIT", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests_config {
    use super::*;

    #[test]
    fn test_parse_mcp_config() {
        let toml = r#"
version = 1

[[tools]]
server = "github"
tool = "list_repos"
risk = "safe"
needs_approval = false

[[tools]]
server = "github"
tool = "create_issue"
risk = "moderate"
needs_approval = true

[[patterns]]
server = "github"
tool = "create_issue"
pattern = "repo:myorg/*"
match_type = "glob"
action = "allow"
"#;

        let config: McpConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.tools.len(), 2);
        assert_eq!(config.patterns.len(), 1);
        assert_eq!(config.tools[0].server, "github");
    }
}
