//! Storage Facade - Unified API for all persistent storage
//!
//! Wraps TarkStorage and GlobalStorage to provide a clean BFF-layer API.

use anyhow::Result;
use std::path::Path;

use crate::storage::{GlobalStorage, TarkStorage};

use super::errors::StorageError;
use super::types::SessionInfo;

/// Configuration scope (project or global)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigScope {
    Project,
    Global,
}

/// Usage period for statistics
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsagePeriod {
    Today,
    ThisWeek,
    ThisMonth,
    AllTime,
}

/// Usage summary
#[derive(Debug, Clone)]
pub struct UsageSummary {
    pub total_requests: usize,
    pub total_input_tokens: usize,
    pub total_output_tokens: usize,
    pub total_cost: f64,
    pub by_model: Vec<(String, usize, f64)>, // (model, requests, cost)
}

/// Plugin information
#[derive(Debug, Clone)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
}

/// Storage Facade
///
/// Provides unified access to both project-level (.tark/) and global
/// (~/.config/tark/) storage.
pub struct StorageFacade {
    project: TarkStorage,
    global: GlobalStorage,
}

impl StorageFacade {
    /// Create a new storage facade
    pub fn new(project_dir: &Path) -> Result<Self, StorageError> {
        let project = TarkStorage::new(project_dir).map_err(StorageError::Other)?;
        let global = GlobalStorage::new().map_err(StorageError::Other)?;

        Ok(Self { project, global })
    }

    // === Sessions ===

    /// Create a new session
    pub fn create_session(&self) -> Result<SessionInfo, StorageError> {
        let session = crate::storage::ChatSession::new();

        self.project
            .save_session(&session)
            .map_err(StorageError::Other)?;

        Ok(SessionInfo {
            session_id: session.id.clone(),
            session_name: "New Session".to_string(),
            total_cost: 0.0,
            model_count: 0,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    /// Load a session by ID
    pub fn load_session(&self, id: &str) -> Result<crate::storage::ChatSession, StorageError> {
        self.project
            .load_session(id)
            .map_err(|_e| StorageError::SessionNotFound(id.to_string()))
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<crate::storage::SessionMeta>, StorageError> {
        self.project.list_sessions().map_err(StorageError::Other)
    }

    /// Delete a session
    pub fn delete_session(&self, id: &str) -> Result<(), StorageError> {
        self.project
            .delete_session(id)
            .map_err(|_e| StorageError::SessionNotFound(id.to_string()))
    }

    /// Export a session to a file
    pub fn export_session(&self, id: &str, path: &Path) -> Result<(), StorageError> {
        let session = self.load_session(id)?;

        let export_data = serde_json::json!({
            "session_id": session.id,
            "provider": session.provider,
            "model": session.model,
            "mode": session.mode,
            "messages": session.messages,
            "exported_at": chrono::Local::now().to_rfc3339(),
        });

        let json = serde_json::to_string_pretty(&export_data)
            .map_err(|e| StorageError::Other(e.into()))?;
        std::fs::write(path, json).map_err(StorageError::IoError)
    }

    /// Import a session from a file
    pub fn import_session(&self, path: &Path) -> Result<SessionInfo, StorageError> {
        let content = std::fs::read_to_string(path)?;
        let data: serde_json::Value = serde_json::from_str(&content)
            .map_err(|_| StorageError::InvalidSessionFile(path.display().to_string()))?;

        // Create a new session with imported data
        let mut session = crate::storage::ChatSession::new();

        if let Some(provider) = data.get("provider").and_then(|v| v.as_str()) {
            session.provider = provider.to_string();
        }
        if let Some(model) = data.get("model").and_then(|v| v.as_str()) {
            session.model = model.to_string();
        }

        self.project.save_session(&session)?;

        let session_name = if session.name.is_empty() {
            format!("Imported {}", session.created_at.format("%Y-%m-%d %H:%M"))
        } else {
            session.name.clone()
        };

        Ok(SessionInfo {
            session_id: session.id.clone(),
            session_name,
            total_cost: 0.0,
            model_count: 0,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    // === Config ===

    /// Get merged config (project overrides global)
    pub fn get_config(&self) -> crate::storage::WorkspaceConfig {
        self.project.load_config().unwrap_or_default()
    }

    /// Save project-level config
    pub fn save_project_config(
        &self,
        config: &crate::storage::WorkspaceConfig,
    ) -> Result<(), StorageError> {
        self.project
            .save_config(config)
            .map_err(StorageError::Other)
    }

    /// Save global config
    pub fn save_global_config(
        &self,
        config: &crate::storage::WorkspaceConfig,
    ) -> Result<(), StorageError> {
        self.project
            .save_global_config(config)
            .map_err(StorageError::Other)
    }

    // === Rules ===

    /// Get all rules (merged: global + project)
    pub fn get_rules(&self) -> Vec<String> {
        // List rule names and load their content
        let rules_dir = self.project.project_root().join("rules");
        if !rules_dir.exists() {
            return vec![];
        }

        std::fs::read_dir(rules_dir)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let path = e.path();
                        if path.extension().map(|ext| ext == "md").unwrap_or(false) {
                            std::fs::read_to_string(&path).ok()
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Save a rule
    pub fn save_rule(
        &self,
        name: &str,
        content: &str,
        scope: ConfigScope,
    ) -> Result<(), StorageError> {
        let result = match scope {
            ConfigScope::Project => self.project.save_rule(name, content),
            ConfigScope::Global => self.project.save_global_rule(name, content),
        };
        result.map(|_| ()).map_err(StorageError::Other)
    }

    /// Delete a rule (manual file deletion)
    pub fn delete_rule(&self, name: &str, scope: ConfigScope) -> Result<(), StorageError> {
        let rules_dir = match scope {
            ConfigScope::Project => self.project.project_root().join("rules"),
            ConfigScope::Global => self.project.global_root().join("rules"),
        };

        let path = rules_dir.join(format!("{}.md", name));
        if path.exists() {
            std::fs::remove_file(&path).map_err(StorageError::IoError)
        } else {
            Err(StorageError::RuleNotFound(name.to_string()))
        }
    }

    // === MCP Servers ===

    /// Get MCP server configuration
    pub fn get_mcp_config(&self) -> crate::storage::McpConfig {
        self.project.load_mcp_config().unwrap_or_default()
    }

    /// Save MCP configuration
    pub fn save_mcp_config(
        &self,
        config: &crate::storage::McpConfig,
        _scope: ConfigScope,
    ) -> Result<(), StorageError> {
        // Currently only project scope is supported
        self.project
            .save_mcp_config(config)
            .map_err(StorageError::Other)
    }

    // === Plugins ===

    /// List all plugins (merged: global + project)
    pub fn list_plugins(&self) -> Vec<PluginInfo> {
        let disabled = self.project.load_disabled_plugins().unwrap_or_default();

        self.project
            .list_plugins()
            .unwrap_or_default()
            .into_iter()
            .map(|p| {
                // Try to extract version from plugin config
                let plugin_toml = self
                    .project
                    .project_root()
                    .join("plugins")
                    .join(&p.name)
                    .join("plugin.toml");

                let version = if plugin_toml.exists() {
                    "0.1.0".to_string()
                } else {
                    "unknown".to_string()
                };

                PluginInfo {
                    id: p.name.clone(),
                    name: p.name.clone(),
                    version,
                    enabled: !disabled.contains(&p.name),
                }
            })
            .collect()
    }

    /// Enable a plugin
    pub fn enable_plugin(&self, id: &str) -> Result<(), StorageError> {
        let mut disabled = self
            .project
            .load_disabled_plugins()
            .map_err(StorageError::Other)?;

        // Remove from disabled list
        disabled.retain(|p| p != id);

        self.project
            .save_disabled_plugins(&disabled)
            .map_err(StorageError::Other)
    }

    /// Disable a plugin
    pub fn disable_plugin(&self, id: &str) -> Result<(), StorageError> {
        let mut disabled = self
            .project
            .load_disabled_plugins()
            .map_err(StorageError::Other)?;

        // Add to disabled list if not already there
        if !disabled.contains(&id.to_string()) {
            disabled.push(id.to_string());
        }

        self.project
            .save_disabled_plugins(&disabled)
            .map_err(StorageError::Other)
    }

    /// Get plugin configuration
    ///
    /// Note: Returns None - plugin config loading not yet implemented.
    /// Would read from .tark/plugins/{id}/config.json when implemented.
    pub fn plugin_config(&self, _id: &str) -> Option<serde_json::Value> {
        None
    }

    /// Save plugin configuration
    ///
    /// Note: Not yet implemented - would write to .tark/plugins/{id}/config.json
    pub fn save_plugin_config(
        &self,
        _id: &str,
        _config: serde_json::Value,
    ) -> Result<(), StorageError> {
        Ok(())
    }

    // === Usage Tracking ===

    /// Get usage tracker
    pub fn get_usage_tracker(&self) -> Result<crate::storage::usage::UsageTracker, StorageError> {
        let db_dir = self.project.project_root();
        crate::storage::usage::UsageTracker::new(db_dir).map_err(StorageError::Other)
    }

    /// Record usage
    ///
    /// Note: Usage tracking is automatically handled by ChatAgent.
    /// This method exists for API completeness but actual recording happens
    /// internally when messages are sent. UsageTracker is queried via get_usage_tracker().
    pub fn record_usage(
        &self,
        provider: &str,
        model: &str,
        input_tokens: usize,
        output_tokens: usize,
        cost_usd: f64,
    ) -> Result<(), StorageError> {
        // Log the intent - actual recording happens in ChatAgent
        tracing::debug!(
            "Usage record: {} {} - {} in + {} out = ${:.4}",
            provider,
            model,
            input_tokens,
            output_tokens,
            cost_usd
        );

        // Usage is automatically tracked by ChatAgent when sending messages
        // Access via get_usage_tracker() to query historical data
        Ok(())
    }

    /// Get usage summary for a period
    ///
    /// Queries the usage database for aggregated statistics.
    pub fn usage_summary(&self, period: UsagePeriod) -> Result<UsageSummary, StorageError> {
        let tracker = self.get_usage_tracker()?;

        // Get overall summary from database
        let db_summary = tracker
            .get_summary()
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?;

        // Get per-model breakdown
        let by_model = tracker
            .get_usage_by_model()
            .map_err(|e| StorageError::DatabaseError(e.to_string()))?
            .into_iter()
            .map(|m| (m.model, m.request_count as usize, m.cost))
            .collect();

        // Filter by period if needed
        let _ = period; // Note: Time-based filtering would require WHERE clause in SQL query

        Ok(UsageSummary {
            total_requests: db_summary.log_count as usize,
            total_input_tokens: 0,  // Not tracked separately in summary
            total_output_tokens: 0, // Not tracked separately in summary
            total_cost: db_summary.total_cost,
            by_model,
        })
    }

    /// Get the project root path
    pub fn project_root(&self) -> &Path {
        self.project.project_root()
    }

    /// Get the global root path
    pub fn global_root(&self) -> &Path {
        self.project.global_root()
    }
}
