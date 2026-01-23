use anyhow::Result;
use chrono::Utc;
use rusqlite::Connection;
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

/// User-defined pattern configuration
#[derive(Debug, Deserialize, Default)]
pub struct PatternsConfig {
    pub version: Option<u32>,
    #[serde(default)]
    pub approvals: Vec<ApprovalPatternConfig>,
    #[serde(default)]
    pub denials: Vec<ApprovalPatternConfig>,
    #[serde(default)]
    pub mcp_approvals: Vec<McpPatternConfig>,
    #[serde(default)]
    pub mcp_denials: Vec<McpPatternConfig>,
}

#[derive(Debug, Deserialize)]
pub struct ApprovalPatternConfig {
    pub tool: String,
    pub pattern: String,
    pub match_type: String,
    pub description: Option<String>,
}

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

/// Loads and syncs user pattern configurations
pub struct PatternLoader {
    user_patterns_path: PathBuf,
    workspace_patterns_path: Option<PathBuf>,
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

impl PatternLoader {
    pub fn new(workspace_dir: Option<&Path>) -> Self {
        let user_patterns_path = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("tark")
            .join("policy")
            .join("patterns.toml");

        let workspace_patterns_path = workspace_dir.map(|w| w.join(".tark/policy/patterns.toml"));

        Self {
            user_patterns_path,
            workspace_patterns_path,
        }
    }

    /// Load user patterns config
    fn load_user_patterns(&self) -> Result<Option<PatternsConfig>> {
        if !self.user_patterns_path.exists() {
            info!(
                "No user patterns config found at {:?}",
                self.user_patterns_path
            );
            return Ok(None);
        }

        let content = fs::read_to_string(&self.user_patterns_path)?;
        let config: PatternsConfig = toml::from_str(&content)?;
        info!(
            "Loaded user patterns: {} approvals, {} denials",
            config.approvals.len(),
            config.denials.len()
        );
        Ok(Some(config))
    }

    /// Load workspace patterns config
    fn load_workspace_patterns(&self) -> Result<Option<PatternsConfig>> {
        if let Some(path) = &self.workspace_patterns_path {
            if path.exists() {
                let content = fs::read_to_string(path)?;
                let config: PatternsConfig = toml::from_str(&content)?;
                info!(
                    "Loaded workspace patterns: {} approvals, {} denials",
                    config.approvals.len(),
                    config.denials.len()
                );
                return Ok(Some(config));
            }
        }
        Ok(None)
    }

    /// Load and merge patterns (workspace + user, both applied)
    pub fn load(&self) -> Result<PatternsConfig> {
        let mut merged = PatternsConfig::default();

        // Load user patterns
        if let Some(user) = self.load_user_patterns()? {
            merged.approvals.extend(user.approvals);
            merged.denials.extend(user.denials);
            merged.mcp_approvals.extend(user.mcp_approvals);
            merged.mcp_denials.extend(user.mcp_denials);
        }

        // Load workspace patterns (additive, not override)
        if let Some(workspace) = self.load_workspace_patterns()? {
            merged.approvals.extend(workspace.approvals);
            merged.denials.extend(workspace.denials);
            merged.mcp_approvals.extend(workspace.mcp_approvals);
            merged.mcp_denials.extend(workspace.mcp_denials);
        }

        Ok(merged)
    }

    /// Sync patterns to database
    /// session_id is used for persistent patterns (set to "persistent" for config-loaded patterns)
    pub fn sync_to_db(&self, conn: &Connection, session_id: &str) -> Result<()> {
        let config = self.load()?;
        let now = Utc::now().to_rfc3339();

        conn.execute("BEGIN IMMEDIATE", [])?;

        // Clear existing user/workspace patterns (keep session patterns)
        conn.execute(
            "DELETE FROM approval_patterns WHERE source IN ('user', 'workspace')",
            [],
        )?;

        // Insert approval patterns
        for approval in &config.approvals {
            // Validate pattern first
            let validator = crate::policy::security::PatternValidator::new(PathBuf::from("."));
            if let Err(e) =
                validator.validate(&approval.tool, &approval.pattern, &approval.match_type)
            {
                warn!(
                    "Skipping invalid pattern '{}' for tool '{}': {}",
                    approval.pattern, approval.tool, e
                );
                continue;
            }

            conn.execute(
                "INSERT INTO approval_patterns (tool_type_id, pattern, match_type, is_denial, is_persistent, session_id, created_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    &approval.tool,
                    &approval.pattern,
                    &approval.match_type,
                    false, // is_denial
                    true,  // is_persistent
                    session_id,
                    &now,
                    &approval.description,
                ),
            )?;
        }
        info!("Synced {} approval patterns", config.approvals.len());

        // Insert denial patterns
        for denial in &config.denials {
            // Validate pattern first
            let validator = crate::policy::security::PatternValidator::new(PathBuf::from("."));
            if let Err(e) = validator.validate(&denial.tool, &denial.pattern, &denial.match_type) {
                warn!(
                    "Skipping invalid pattern '{}' for tool '{}': {}",
                    denial.pattern, denial.tool, e
                );
                continue;
            }

            conn.execute(
                "INSERT INTO approval_patterns (tool_type_id, pattern, match_type, is_denial, is_persistent, session_id, created_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    &denial.tool,
                    &denial.pattern,
                    &denial.match_type,
                    true, // is_denial
                    true, // is_persistent
                    session_id,
                    &now,
                    &denial.description,
                ),
            )?;
        }
        info!("Synced {} denial patterns", config.denials.len());

        // Insert MCP approval patterns
        for approval in &config.mcp_approvals {
            conn.execute(
                "INSERT INTO mcp_approval_patterns (server_id, tool_name, pattern, match_type, is_denial, source, created_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    &approval.server,
                    &approval.tool,
                    &approval.pattern,
                    &approval.match_type,
                    false, // is_denial
                    "user",
                    &now,
                    &approval.description,
                ),
            )?;
        }
        info!(
            "Synced {} MCP approval patterns",
            config.mcp_approvals.len()
        );

        // Insert MCP denial patterns
        for denial in &config.mcp_denials {
            conn.execute(
                "INSERT INTO mcp_approval_patterns (server_id, tool_name, pattern, match_type, is_denial, source, created_at, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                (
                    &denial.server,
                    &denial.tool,
                    &denial.pattern,
                    &denial.match_type,
                    true, // is_denial
                    "user",
                    &now,
                    &denial.description,
                ),
            )?;
        }
        info!("Synced {} MCP denial patterns", config.mcp_denials.len());

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
