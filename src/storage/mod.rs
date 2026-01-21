//! Persistent storage for tark agent
//!
//! Configuration hierarchy (project overrides global):
//!
//! ~/.config/tark/                    # Global config
//! ├── config.toml                    # Global settings
//! ├── rules/                         # Global rules
//! ├── mcp/                           # Global MCP servers
//! │   └── servers.toml               # MCP server definitions
//! └── plugins/                       # Global plugins
//!     └── {plugin}/
//!
//! .tark/                             # Project-level (overrides global)
//! ├── config.toml                    # Project settings (merges with global)
//! ├── conversations/                 # Saved conversations
//! ├── plans/                         # Saved plans
//! ├── rules/                         # Project-specific rules
//! ├── mcp/                           # Project-specific MCP servers
//! │   └── servers.toml
//! ├── plugins/                       # Project-specific plugins
//! │   └── {plugin}/
//! └── usage.db                       # Usage tracking database

#![allow(dead_code)]

pub mod usage;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Project-level storage directory name
const TARK_DIR: &str = ".tark";

/// Global config directory name
const GLOBAL_CONFIG_DIR: &str = "tark";

/// Global storage (user-level, at ~/.config/tark/)
pub struct GlobalStorage {
    root: PathBuf,
}

impl GlobalStorage {
    /// Initialize global storage
    pub fn new() -> Result<Self> {
        let root = if let Some(config_dir) = dirs::config_dir() {
            config_dir.join(GLOBAL_CONFIG_DIR)
        } else {
            // Fallback to home directory
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".config")
                .join(GLOBAL_CONFIG_DIR)
        };

        // Create directory structure
        std::fs::create_dir_all(&root)?;
        std::fs::create_dir_all(root.join("rules"))?;
        std::fs::create_dir_all(root.join("mcp"))?;
        std::fs::create_dir_all(root.join("plugins"))?;

        Ok(Self { root })
    }

    /// Get the root global config directory
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Load global config
    pub fn load_config(&self) -> Result<WorkspaceConfig> {
        let path = self.root.join("config.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            toml::from_str(&content).context("Failed to parse global config.toml")
        } else {
            Ok(WorkspaceConfig::default())
        }
    }

    /// Save global config
    pub fn save_config(&self, config: &WorkspaceConfig) -> Result<()> {
        let path = self.root.join("config.toml");
        let content = toml::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Load global MCP servers
    pub fn load_mcp_servers(&self) -> Result<McpConfig> {
        let path = self.root.join("mcp").join("servers.toml");
        if path.exists() {
            let content = std::fs::read_to_string(&path)?;
            toml::from_str(&content).context("Failed to parse mcp/servers.toml")
        } else {
            Ok(McpConfig::default())
        }
    }

    /// Load global rules
    pub fn load_rules(&self) -> Result<Vec<Rule>> {
        load_rules_from_dir(&self.root.join("rules"))
    }

    /// List global plugins
    pub fn list_plugins(&self) -> Result<Vec<PluginInfo>> {
        list_plugins_from_dir(&self.root.join("plugins"))
    }
}

/// Workspace-level storage for tark (project-specific)
pub struct TarkStorage {
    /// Project-level .tark directory
    project_root: PathBuf,
    /// Global ~/.config/tark directory
    global: GlobalStorage,
}

impl TarkStorage {
    /// Initialize storage for a workspace (loads both global and project configs)
    pub fn new(workspace_dir: impl AsRef<Path>) -> Result<Self> {
        let project_root = workspace_dir.as_ref().join(TARK_DIR);
        let global = GlobalStorage::new()?;

        // Create project directory structure
        std::fs::create_dir_all(&project_root)?;
        std::fs::create_dir_all(project_root.join("conversations"))?;
        std::fs::create_dir_all(project_root.join("plans"))?;
        std::fs::create_dir_all(project_root.join("rules"))?;
        std::fs::create_dir_all(project_root.join("mcp"))?;
        std::fs::create_dir_all(project_root.join("plugins"))?;
        std::fs::create_dir_all(project_root.join("sessions"))?;
        std::fs::create_dir_all(project_root.join("archives").join("plans"))?;

        let storage = Self {
            project_root,
            global,
        };

        // Migrate existing flat session files to directory structure
        if let Err(e) = storage.migrate_sessions() {
            tracing::warn!("Failed to migrate sessions: {}", e);
        }

        Ok(storage)
    }

    /// Get the project .tark directory
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Get the global config directory
    pub fn global_root(&self) -> &Path {
        self.global.root()
    }

    // ========== Config (merged: global + project) ==========

    /// Load merged config (project overrides global)
    pub fn load_config(&self) -> Result<WorkspaceConfig> {
        // Start with global config
        let mut config = self.global.load_config().unwrap_or_default();

        // Override with project config if it exists
        let project_path = self.project_root.join("config.toml");
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            let project_config: WorkspaceConfig =
                toml::from_str(&content).context("Failed to parse project config.toml")?;
            config.merge(project_config);
        }

        Ok(config)
    }

    /// Save project-level config
    pub fn save_config(&self, config: &WorkspaceConfig) -> Result<()> {
        let path = self.project_root.join("config.toml");
        let content = toml::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Save global config
    pub fn save_global_config(&self, config: &WorkspaceConfig) -> Result<()> {
        self.global.save_config(config)
    }

    // ========== Conversations (project-level only) ==========

    /// Save a conversation
    pub fn save_conversation(&self, conversation: &SavedConversation) -> Result<PathBuf> {
        let filename = format!("{}.json", conversation.id);
        let path = self.project_root.join("conversations").join(&filename);
        let content = serde_json::to_string_pretty(conversation)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Load a conversation by ID
    pub fn load_conversation(&self, id: &str) -> Result<SavedConversation> {
        let path = self
            .project_root
            .join("conversations")
            .join(format!("{}.json", id));
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse conversation")
    }

    /// List all saved conversations
    pub fn list_conversations(&self) -> Result<Vec<ConversationSummary>> {
        let dir = self.project_root.join("conversations");
        let mut conversations = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Some(ext) = entry.path().extension() {
                    if ext == "json" {
                        if let Ok(content) = std::fs::read_to_string(entry.path()) {
                            if let Ok(conv) = serde_json::from_str::<SavedConversation>(&content) {
                                conversations.push(ConversationSummary {
                                    id: conv.id,
                                    title: conv.title,
                                    created_at: conv.created_at,
                                    message_count: conv.messages.len(),
                                    mode: conv.mode,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Sort by creation date (newest first)
        conversations.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(conversations)
    }

    /// Delete a conversation
    pub fn delete_conversation(&self, id: &str) -> Result<()> {
        let path = self
            .project_root
            .join("conversations")
            .join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    // ========== Chat Sessions ==========

    /// Get sessions directory
    fn sessions_dir(&self) -> PathBuf {
        self.project_root.join("sessions")
    }

    /// Get directory for a specific session
    pub fn session_dir(&self, session_id: &str) -> PathBuf {
        self.sessions_dir().join(session_id)
    }

    /// Get current session ID file path
    fn current_session_file(&self) -> PathBuf {
        self.sessions_dir().join("current")
    }

    /// Migrate flat session files to directory structure
    /// Called on startup to handle existing sessions
    pub fn migrate_sessions(&self) -> Result<()> {
        let sessions_dir = self.sessions_dir();
        if !sessions_dir.exists() {
            return Ok(());
        }

        // Find flat session files (*.json directly in sessions/)
        let entries: Vec<_> = std::fs::read_dir(&sessions_dir)?
            .flatten()
            .filter(|e| {
                let path = e.path();
                path.is_file() && path.extension().map(|ext| ext == "json").unwrap_or(false)
            })
            .collect();

        for entry in entries {
            let path = entry.path();
            let file_name = path.file_stem().and_then(|s| s.to_str());

            if let Some(session_id) = file_name {
                // Skip if it's not a valid session file (e.g., current)
                if session_id == "current" {
                    continue;
                }

                // Read the session
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<ChatSession>(&content) {
                        // Create session directory
                        let session_dir = self.session_dir(&session.id);
                        std::fs::create_dir_all(&session_dir)?;

                        // Write to new location
                        let new_path = session_dir.join("session.json");
                        std::fs::write(&new_path, &content)?;

                        // Create plans directory for this session
                        std::fs::create_dir_all(session_dir.join("plans"))?;

                        // Remove old flat file
                        std::fs::remove_file(&path)?;

                        tracing::info!("Migrated session {} to directory structure", session.id);
                    }
                }
            }
        }

        // Migrate global plans to current session if any exist
        let global_plans_dir = self.sessions_dir().join("plans");
        if global_plans_dir.exists() && global_plans_dir.is_dir() {
            // Get current session to migrate plans to
            if let Some(current_id) = self.get_current_session_id() {
                let target_plans_dir = self.session_dir(&current_id).join("plans");
                std::fs::create_dir_all(&target_plans_dir)?;

                // Move plan files
                if let Ok(entries) = std::fs::read_dir(&global_plans_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            let file_name = path.file_name().unwrap();
                            let target = target_plans_dir.join(file_name);
                            std::fs::rename(&path, &target)?;
                        }
                    }
                }

                // Remove empty global plans dir
                let _ = std::fs::remove_dir(&global_plans_dir);
            }
        }

        Ok(())
    }

    /// Save a chat session
    pub fn save_session(&self, session: &ChatSession) -> Result<PathBuf> {
        // Create session directory
        let session_dir = self.session_dir(&session.id);
        std::fs::create_dir_all(&session_dir)?;

        // Create plans subdirectory
        std::fs::create_dir_all(session_dir.join("plans"))?;

        // Create conversations subdirectory
        let conversations_dir = session_dir.join("conversations");
        std::fs::create_dir_all(&conversations_dir)?;

        // Save session.json in the directory
        let path = session_dir.join("session.json");
        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(&path, content)?;

        // Save conversation messages in session conversations directory
        let conversations_path = conversations_dir.join("conversation.json");
        let messages_content = serde_json::to_string_pretty(&session.messages)?;
        std::fs::write(&conversations_path, messages_content)?;

        // Update current session pointer
        std::fs::write(self.current_session_file(), &session.id)?;

        Ok(path)
    }

    /// Load a chat session by ID
    pub fn load_session(&self, id: &str) -> Result<ChatSession> {
        // Try new directory structure first
        let dir_path = self.session_dir(id).join("session.json");
        if dir_path.exists() {
            let content = std::fs::read_to_string(&dir_path)?;
            let mut session: ChatSession =
                serde_json::from_str(&content).context("Failed to parse session")?;

            let conversations_path = self
                .session_dir(id)
                .join("conversations")
                .join("conversation.json");
            if conversations_path.exists() {
                if let Ok(conversations_content) = std::fs::read_to_string(&conversations_path) {
                    if let Ok(messages) =
                        serde_json::from_str::<Vec<SessionMessage>>(&conversations_content)
                    {
                        session.messages = messages;
                    }
                }
            }

            return Ok(session);
        }

        // Fall back to flat file (for backwards compatibility)
        let flat_path = self.sessions_dir().join(format!("{}.json", id));
        if flat_path.exists() {
            let content = std::fs::read_to_string(&flat_path)?;
            return serde_json::from_str(&content).context("Failed to parse session");
        }

        Err(anyhow::anyhow!("Session not found: {}", id))
    }

    /// Get the current session ID
    pub fn get_current_session_id(&self) -> Option<String> {
        let path = self.current_session_file();
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Set the current session ID
    pub fn set_current_session(&self, id: &str) -> Result<()> {
        std::fs::write(self.current_session_file(), id)?;
        Ok(())
    }

    /// Load the current session (most recently used)
    pub fn load_current_session(&self) -> Result<ChatSession> {
        if let Some(id) = self.get_current_session_id() {
            self.load_session(&id)
        } else {
            // No current session - find most recently updated
            let sessions = self.list_sessions()?;
            if let Some(meta) = sessions.first() {
                self.load_session(&meta.id)
            } else {
                // No sessions exist - create new one
                let session = ChatSession::new();
                self.save_session(&session)?;
                Ok(session)
            }
        }
    }

    /// List all chat sessions
    pub fn list_sessions(&self) -> Result<Vec<SessionMeta>> {
        let dir = self.sessions_dir();
        let mut sessions = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();

                // Check for directory-based sessions (new format)
                if path.is_dir() {
                    let session_file = path.join("session.json");
                    if session_file.exists() {
                        if let Ok(content) = std::fs::read_to_string(&session_file) {
                            if let Ok(session) = serde_json::from_str::<ChatSession>(&content) {
                                sessions.push(SessionMeta::from(&session));
                            }
                        }
                    }
                }
                // Also check for flat files (backwards compatibility)
                else if path.extension().map(|e| e == "json").unwrap_or(false) {
                    // Skip "current" pointer file
                    if path.file_stem().map(|s| s == "current").unwrap_or(false) {
                        continue;
                    }
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(session) = serde_json::from_str::<ChatSession>(&content) {
                            sessions.push(SessionMeta::from(&session));
                        }
                    }
                }
            }
        }

        // Sort by updated date (most recent first)
        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(sessions)
    }

    /// Create a new session and set it as current
    pub fn create_new_session(&self) -> Result<ChatSession> {
        let session = ChatSession::new();
        self.save_session(&session)?;
        Ok(session)
    }

    /// Delete a session
    pub fn delete_session(&self, id: &str) -> Result<()> {
        // Try directory-based session first
        let session_dir = self.session_dir(id);
        if session_dir.exists() {
            std::fs::remove_dir_all(session_dir)?;
        } else {
            // Fall back to flat file
            let path = self.sessions_dir().join(format!("{}.json", id));
            if path.exists() {
                std::fs::remove_file(path)?;
            }
        }

        // If this was the current session, clear the pointer
        if self.get_current_session_id().as_deref() == Some(id) {
            let _ = std::fs::remove_file(self.current_session_file());
        }

        Ok(())
    }

    // ========== Compaction Records (Session-Scoped) ==========

    /// Get compactions directory for a session
    pub fn compactions_dir(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("compactions")
    }

    /// Save a compaction record
    pub fn save_compaction(&self, session_id: &str, record: &CompactionRecord) -> Result<()> {
        let dir = self.compactions_dir(session_id);
        std::fs::create_dir_all(&dir)?;

        let path = dir.join(format!("{}.json", record.id));
        let content = serde_json::to_string_pretty(record)?;
        std::fs::write(path, content)?;

        tracing::debug!(
            "Saved compaction record {} for session {}",
            record.id,
            session_id
        );
        Ok(())
    }

    /// Load a compaction record
    pub fn load_compaction(
        &self,
        session_id: &str,
        compaction_id: &str,
    ) -> Result<CompactionRecord> {
        let path = self
            .compactions_dir(session_id)
            .join(format!("{}.json", compaction_id));
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse compaction record")
    }

    /// List all compaction records for a session (most recent first)
    pub fn list_compactions(&self, session_id: &str) -> Result<Vec<CompactionRecord>> {
        let dir = self.compactions_dir(session_id);
        let mut records = Vec::new();

        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "json").unwrap_or(false) {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            if let Ok(record) = serde_json::from_str::<CompactionRecord>(&content) {
                                records.push(record);
                            }
                        }
                    }
                }
            }
        }

        // Sort by timestamp (most recent first)
        records.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(records)
    }

    // ========== Execution Plans (Session-Scoped) ==========

    /// Get execution plans directory for a session
    pub fn execution_plans_dir(&self, session_id: &str) -> PathBuf {
        self.session_dir(session_id).join("plans")
    }

    /// Get current plan file path for a session
    fn current_plan_file(&self, session_id: &str) -> PathBuf {
        self.execution_plans_dir(session_id).join("current")
    }

    /// Save an execution plan (session-scoped)
    pub fn save_execution_plan(&self, session_id: &str, plan: &ExecutionPlan) -> Result<PathBuf> {
        let plans_dir = self.execution_plans_dir(session_id);
        std::fs::create_dir_all(&plans_dir)?;

        let path = plans_dir.join(format!("{}.md", plan.id));
        let content = plan.to_markdown();
        std::fs::write(&path, content)?;

        // Update current plan pointer if this is an active plan
        if plan.status == PlanStatus::Active || plan.status == PlanStatus::Draft {
            std::fs::write(self.current_plan_file(session_id), &plan.id)?;
        }

        Ok(path)
    }

    /// Load an execution plan by ID (session-scoped)
    pub fn load_execution_plan(&self, session_id: &str, id: &str) -> Result<ExecutionPlan> {
        // Try markdown format first (new)
        let md_path = self
            .execution_plans_dir(session_id)
            .join(format!("{}.md", id));
        if md_path.exists() {
            let content = std::fs::read_to_string(&md_path)?;
            return ExecutionPlan::from_markdown(&content);
        }

        // Fall back to JSON format (legacy)
        let json_path = self
            .execution_plans_dir(session_id)
            .join(format!("{}.json", id));
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)?;
            return serde_json::from_str(&content).context("Failed to parse execution plan");
        }

        Err(anyhow::anyhow!("Plan not found: {}", id))
    }

    /// Get the current plan ID for a session
    pub fn get_current_plan_id(&self, session_id: &str) -> Option<String> {
        let path = self.current_plan_file(session_id);
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Set the current plan ID for a session
    pub fn set_current_plan(&self, session_id: &str, id: &str) -> Result<()> {
        let plans_dir = self.execution_plans_dir(session_id);
        std::fs::create_dir_all(&plans_dir)?;
        std::fs::write(self.current_plan_file(session_id), id)?;
        Ok(())
    }

    /// Clear current plan pointer for a session
    pub fn clear_current_plan(&self, session_id: &str) -> Result<()> {
        let path = self.current_plan_file(session_id);
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Load the current execution plan for a session
    pub fn load_current_execution_plan(&self, session_id: &str) -> Result<ExecutionPlan> {
        if let Some(id) = self.get_current_plan_id(session_id) {
            self.load_execution_plan(session_id, &id)
        } else {
            Err(anyhow::anyhow!("No active plan"))
        }
    }

    /// List all execution plans for a session
    pub fn list_execution_plans(&self, session_id: &str) -> Result<Vec<PlanMeta>> {
        let dir = self.execution_plans_dir(session_id);
        let mut plans = Vec::new();

        if !dir.exists() {
            return Ok(plans);
        }

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());

                // Support both .md (new) and .json (legacy)
                if ext == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(plan) = ExecutionPlan::from_markdown(&content) {
                            plans.push(PlanMeta::from(&plan));
                        }
                    }
                } else if ext == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(plan) = serde_json::from_str::<ExecutionPlan>(&content) {
                            plans.push(PlanMeta::from(&plan));
                        }
                    }
                }
            }
        }

        // Sort by updated date (most recent first)
        plans.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(plans)
    }

    /// Delete an execution plan
    pub fn delete_execution_plan(&self, session_id: &str, id: &str) -> Result<()> {
        // Try both formats
        let md_path = self
            .execution_plans_dir(session_id)
            .join(format!("{}.md", id));
        let json_path = self
            .execution_plans_dir(session_id)
            .join(format!("{}.json", id));

        if md_path.exists() {
            std::fs::remove_file(md_path)?;
        }
        if json_path.exists() {
            std::fs::remove_file(json_path)?;
        }

        // If this was the current plan, clear the pointer
        if self.get_current_plan_id(session_id).as_deref() == Some(id) {
            let _ = std::fs::remove_file(self.current_plan_file(session_id));
        }

        Ok(())
    }

    // ========== Plan Archives ==========

    /// Get the archives/plans directory path
    fn archives_plans_dir(&self) -> PathBuf {
        self.project_root.join("archives").join("plans")
    }

    /// Archive a plan (move to archives, keep max 5)
    ///
    /// Moves the plan from session plans to archives/plans/,
    /// then prunes old archives to keep only the 5 most recent.
    pub fn archive_plan(&self, session_id: &str, id: &str) -> Result<PathBuf> {
        let archive_dir = self.archives_plans_dir();
        std::fs::create_dir_all(&archive_dir)?;

        // Load the plan
        let mut plan = self.load_execution_plan(session_id, id)?;
        plan.status = PlanStatus::Completed;

        // Save to archive location as markdown
        let archive_path = archive_dir.join(format!("{}.md", plan.id));
        let content = plan.to_markdown();
        std::fs::write(&archive_path, content)?;

        // Delete from active plans
        self.delete_execution_plan(session_id, id)?;

        // Prune old archives (keep max 5)
        self.prune_archived_plans(5)?;

        Ok(archive_path)
    }

    /// Prune archived plans to keep only the most recent N
    fn prune_archived_plans(&self, keep_count: usize) -> Result<()> {
        let archive_dir = self.archives_plans_dir();
        if !archive_dir.exists() {
            return Ok(());
        }

        let mut archives: Vec<_> = std::fs::read_dir(&archive_dir)?
            .flatten()
            .filter(|e| {
                let path = e.path();
                let ext = path.extension().and_then(|s| s.to_str());
                ext == Some("md") || ext == Some("json")
            })
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let modified = meta.modified().ok()?;
                Some((e.path(), modified))
            })
            .collect();

        // Sort by modification time (newest first)
        archives.sort_by(|a, b| b.1.cmp(&a.1));

        // Remove excess archives
        for (path, _) in archives.into_iter().skip(keep_count) {
            std::fs::remove_file(path)?;
        }

        Ok(())
    }

    /// List archived plans
    pub fn list_archived_plans(&self) -> Result<Vec<PlanMeta>> {
        let archive_dir = self.archives_plans_dir();
        let mut plans = Vec::new();

        if !archive_dir.exists() {
            return Ok(plans);
        }

        if let Ok(entries) = std::fs::read_dir(&archive_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|e| e.to_str());

                // Support both .md (new) and .json (legacy)
                if ext == Some("md") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(plan) = ExecutionPlan::from_markdown(&content) {
                            plans.push(PlanMeta::from(&plan));
                        }
                    }
                } else if ext == Some("json") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(plan) = serde_json::from_str::<ExecutionPlan>(&content) {
                            plans.push(PlanMeta::from(&plan));
                        }
                    }
                }
            }
        }

        // Sort by updated date (most recent first)
        plans.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(plans)
    }

    /// Load an archived plan by ID
    pub fn load_archived_plan(&self, id: &str) -> Result<ExecutionPlan> {
        // Try markdown first
        let md_path = self.archives_plans_dir().join(format!("{}.md", id));
        if md_path.exists() {
            let content = std::fs::read_to_string(&md_path)?;
            return ExecutionPlan::from_markdown(&content);
        }

        // Fall back to JSON
        let json_path = self.archives_plans_dir().join(format!("{}.json", id));
        if json_path.exists() {
            let content = std::fs::read_to_string(&json_path)?;
            return serde_json::from_str(&content).context("Failed to parse archived plan");
        }

        Err(anyhow::anyhow!("Archived plan not found: {}", id))
    }

    // ========== Plans (project-level only) ==========

    /// Save a plan
    pub fn save_plan(&self, name: &str, content: &str) -> Result<PathBuf> {
        let filename = format!("{}.md", sanitize_filename(name));
        let path = self.project_root.join("plans").join(&filename);
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Load a plan
    pub fn load_plan(&self, name: &str) -> Result<String> {
        let filename = format!("{}.md", sanitize_filename(name));
        let path = self.project_root.join("plans").join(&filename);
        std::fs::read_to_string(&path).context("Failed to read plan")
    }

    /// List all plans
    pub fn list_plans(&self) -> Result<Vec<PlanSummary>> {
        let dir = self.project_root.join("plans");
        let mut plans = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "md").unwrap_or(false) {
                    let name = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let metadata = entry.metadata().ok();
                    let modified = metadata.and_then(|m| m.modified().ok());

                    plans.push(PlanSummary {
                        name,
                        path: path.clone(),
                        modified_at: modified.map(DateTime::from),
                    });
                }
            }
        }

        plans.sort_by(|a, b| b.modified_at.cmp(&a.modified_at));
        Ok(plans)
    }

    // ========== Rules (merged: global + project) ==========

    /// Save a project rule
    pub fn save_rule(&self, name: &str, content: &str) -> Result<PathBuf> {
        let filename = format!("{}.md", sanitize_filename(name));
        let path = self.project_root.join("rules").join(&filename);
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Save a global rule
    pub fn save_global_rule(&self, name: &str, content: &str) -> Result<PathBuf> {
        let filename = format!("{}.md", sanitize_filename(name));
        let path = self.global.root().join("rules").join(&filename);
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Load a rule (checks project first, then global)
    pub fn load_rule(&self, name: &str) -> Result<String> {
        let filename = format!("{}.md", sanitize_filename(name));

        // Check project first
        let project_path = self.project_root.join("rules").join(&filename);
        if project_path.exists() {
            return std::fs::read_to_string(&project_path).context("Failed to read rule");
        }

        // Fall back to global
        let global_path = self.global.root().join("rules").join(&filename);
        std::fs::read_to_string(&global_path).context("Failed to read rule")
    }

    /// Load all rules (global + project, project overrides)
    pub fn load_all_rules(&self) -> Result<Vec<Rule>> {
        let mut rules_map: HashMap<String, Rule> = HashMap::new();

        // Load global rules first
        for rule in load_rules_from_dir(&self.global.root().join("rules"))? {
            rules_map.insert(rule.name.clone(), rule);
        }

        // Project rules override global
        for rule in load_rules_from_dir(&self.project_root.join("rules"))? {
            rules_map.insert(rule.name.clone(), rule);
        }

        let mut rules: Vec<Rule> = rules_map.into_values().collect();
        rules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(rules)
    }

    /// List all rules (both global and project)
    pub fn list_rules(&self) -> Result<Vec<RuleInfo>> {
        let mut rules_map: HashMap<String, RuleInfo> = HashMap::new();

        // Global rules
        for name in list_rule_names(&self.global.root().join("rules"))? {
            rules_map.insert(
                name.clone(),
                RuleInfo {
                    name,
                    scope: ConfigScope::Global,
                },
            );
        }

        // Project rules (override)
        for name in list_rule_names(&self.project_root.join("rules"))? {
            rules_map.insert(
                name.clone(),
                RuleInfo {
                    name,
                    scope: ConfigScope::Project,
                },
            );
        }

        let mut rules: Vec<RuleInfo> = rules_map.into_values().collect();
        rules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(rules)
    }

    // ========== MCP Servers (merged: global + project) ==========

    /// Load merged MCP config
    pub fn load_mcp_config(&self) -> Result<McpConfig> {
        // Start with global
        let mut config = self.global.load_mcp_servers().unwrap_or_default();

        // Merge project MCP config
        let project_path = self.project_root.join("mcp").join("servers.toml");
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            let project_config: McpConfig =
                toml::from_str(&content).context("Failed to parse project mcp/servers.toml")?;
            config.merge(project_config);
        }

        Ok(config)
    }

    /// Save project MCP config
    pub fn save_mcp_config(&self, config: &McpConfig) -> Result<()> {
        let path = self.project_root.join("mcp").join("servers.toml");
        let content = toml::to_string_pretty(config)?;
        std::fs::write(path, content)?;
        Ok(())
    }

    // ========== Plugins (merged: global + project) ==========

    /// Load disabled plugins list from ~/.tark/plugins.json
    pub fn load_disabled_plugins(&self) -> Result<Vec<String>> {
        let path = self.global.root().join("plugins.json");
        if !path.exists() {
            return Ok(vec![]);
        }

        let content = std::fs::read_to_string(&path)?;
        let data: serde_json::Value = serde_json::from_str(&content)?;

        Ok(data
            .get("disabled")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Save disabled plugins list
    pub fn save_disabled_plugins(&self, disabled: &[String]) -> Result<()> {
        let path = self.global.root().join("plugins.json");
        let data = serde_json::json!({ "disabled": disabled });
        std::fs::write(&path, serde_json::to_string_pretty(&data)?)?;
        Ok(())
    }

    /// List all plugins (global + project)
    pub fn list_plugins(&self) -> Result<Vec<PluginInfo>> {
        let mut plugins_map: HashMap<String, PluginInfo> = HashMap::new();

        // Global plugins
        for plugin in list_plugins_from_dir(&self.global.root().join("plugins"))? {
            plugins_map.insert(plugin.name.clone(), plugin);
        }

        // Project plugins (override)
        for mut plugin in list_plugins_from_dir(&self.project_root.join("plugins"))? {
            plugin.scope = ConfigScope::Project;
            plugins_map.insert(plugin.name.clone(), plugin);
        }

        let mut plugins: Vec<PluginInfo> = plugins_map.into_values().collect();
        plugins.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(plugins)
    }

    /// Load a plugin's config
    pub fn load_plugin_config(&self, name: &str) -> Result<PluginConfig> {
        // Check project first
        let project_path = self
            .project_root
            .join("plugins")
            .join(name)
            .join("plugin.toml");
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            return toml::from_str(&content).context("Failed to parse plugin.toml");
        }

        // Fall back to global
        let global_path = self
            .global
            .root()
            .join("plugins")
            .join(name)
            .join("plugin.toml");
        let content = std::fs::read_to_string(&global_path)?;
        toml::from_str(&content).context("Failed to parse plugin.toml")
    }

    // ========== Agents (merged: global + project) ==========

    /// List all available agents (global + project)
    pub fn list_agents(&self) -> Result<Vec<AgentInfo>> {
        let mut agents_map: HashMap<String, AgentInfo> = HashMap::new();

        // Global agents
        for agent in load_agents_from_dir(&self.global.root().join("agents"), ConfigScope::Global)?
        {
            agents_map.insert(agent.id.clone(), agent);
        }

        // Project agents (override)
        for agent in load_agents_from_dir(&self.project_root.join("agents"), ConfigScope::Project)?
        {
            agents_map.insert(agent.id.clone(), agent);
        }

        let mut agents: Vec<AgentInfo> = agents_map.into_values().collect();
        agents.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(agents)
    }

    /// Load an agent config by ID (checks project first, then global)
    pub fn load_agent(&self, id: &str) -> Result<AgentConfig> {
        let filename = format!("{}.toml", sanitize_filename(id));

        // Check project first
        let project_path = self.project_root.join("agents").join(&filename);
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            return toml::from_str(&content).context("Failed to parse agent config");
        }

        // Fall back to global
        let global_path = self.global.root().join("agents").join(&filename);
        let content = std::fs::read_to_string(&global_path)?;
        toml::from_str(&content).context("Failed to parse agent config")
    }

    /// Save an agent config (to project by default)
    pub fn save_agent(&self, id: &str, config: &AgentConfig) -> Result<PathBuf> {
        let filename = format!("{}.toml", sanitize_filename(id));
        let dir = self.project_root.join("agents");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(&filename);
        let content = toml::to_string_pretty(config)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Save an agent config to global location
    pub fn save_global_agent(&self, id: &str, config: &AgentConfig) -> Result<PathBuf> {
        let filename = format!("{}.toml", sanitize_filename(id));
        let dir = self.global.root().join("agents");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(&filename);
        let content = toml::to_string_pretty(config)?;
        std::fs::write(&path, content)?;
        Ok(path)
    }

    /// Find agents that match the given trigger context
    pub fn find_matching_agents(&self, context: &TriggerContext) -> Result<Vec<AgentInfo>> {
        let all_agents = self.list_agents()?;
        let mut matches = Vec::new();

        for agent_info in all_agents {
            if let Ok(agent) = self.load_agent(&agent_info.id) {
                let triggers = &agent.triggers;

                // Check file patterns
                if let Some(ref file_path) = context.file_path {
                    for pattern in &triggers.file_patterns {
                        if glob_match(pattern, file_path) {
                            matches.push(agent_info.clone());
                            continue;
                        }
                    }
                }

                // Check keywords in message
                if let Some(ref message) = context.message {
                    let msg_lower = message.to_lowercase();
                    for keyword in &triggers.keywords {
                        if msg_lower.contains(&keyword.to_lowercase()) {
                            matches.push(agent_info.clone());
                            continue;
                        }
                    }
                }

                // Check git context
                if let Some(ref git_ctx) = context.git_context {
                    if triggers.git_contexts.contains(git_ctx) {
                        matches.push(agent_info.clone());
                    }
                }
            }
        }

        Ok(matches)
    }
}

/// Context for finding matching agents
#[derive(Debug, Default)]
pub struct TriggerContext {
    pub file_path: Option<String>,
    pub message: Option<String>,
    pub git_context: Option<String>,
}

// ========== Data Structures ==========

/// Config scope (where the config comes from)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigScope {
    Global,
    Project,
}

/// Workspace-level configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Preferred LLM provider for this workspace
    pub provider: String,
    /// Preferred model (provider/model format)
    pub model: Option<String>,
    /// Default agent mode
    pub default_mode: String,
    /// Enable thinking/verbose mode by default
    pub verbose: bool,
    /// Custom instructions to prepend to system prompt
    pub custom_instructions: Option<String>,
    /// Files/patterns to always ignore
    pub ignore_patterns: Vec<String>,
    /// Auto-save conversations
    pub auto_save_conversations: bool,
    /// Maximum context tokens before auto-compact
    pub max_context_tokens: Option<usize>,
}

impl Default for WorkspaceConfig {
    fn default() -> Self {
        Self {
            provider: "tark_sim".to_string(),
            model: Some("tark_llm".to_string()),
            default_mode: "build".to_string(),
            verbose: false,
            custom_instructions: None,
            ignore_patterns: vec![
                "node_modules".to_string(),
                "target".to_string(),
                ".git".to_string(),
                "*.lock".to_string(),
            ],
            auto_save_conversations: false,
            max_context_tokens: None,
        }
    }
}

impl WorkspaceConfig {
    /// Merge another config into this one (other takes precedence for set values)
    pub fn merge(&mut self, other: WorkspaceConfig) {
        // Only override if explicitly set (non-default)
        if other.provider != "tark_sim" {
            self.provider = other.provider;
        }
        if other.model.is_some() {
            self.model = other.model;
        }
        if other.default_mode != "build" {
            self.default_mode = other.default_mode;
        }
        if other.verbose {
            self.verbose = other.verbose;
        }
        if other.custom_instructions.is_some() {
            self.custom_instructions = other.custom_instructions;
        }
        if !other.ignore_patterns.is_empty() {
            // Append project patterns to global
            self.ignore_patterns.extend(other.ignore_patterns);
        }
        if other.auto_save_conversations {
            self.auto_save_conversations = other.auto_save_conversations;
        }
        if other.max_context_tokens.is_some() {
            self.max_context_tokens = other.max_context_tokens;
        }
    }
}

/// MCP (Model Context Protocol) server configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpConfig {
    /// MCP servers
    #[serde(default)]
    pub servers: HashMap<String, McpServer>,
}

impl McpConfig {
    /// Merge another MCP config (other takes precedence)
    pub fn merge(&mut self, other: McpConfig) {
        for (name, server) in other.servers {
            self.servers.insert(name, server);
        }
    }
}

/// MCP server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Server name/description
    pub name: String,
    /// Command to run the server
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether the server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Server capabilities/tools it provides
    #[serde(default)]
    pub capabilities: Vec<String>,
}

fn default_true() -> bool {
    true
}

/// Plugin information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub name: String,
    pub path: PathBuf,
    pub scope: ConfigScope,
    pub enabled: bool,
    pub description: Option<String>,
}

/// Plugin configuration (plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Plugin type: "tool", "rule", "hook"
    #[serde(default)]
    pub plugin_type: String,
    /// For tool plugins: tool definitions
    #[serde(default)]
    pub tools: Vec<PluginTool>,
    /// For rule plugins: rules to inject
    #[serde(default)]
    pub rules: Vec<String>,
    /// For hook plugins: events to listen for
    #[serde(default)]
    pub hooks: Vec<String>,
}

/// A tool defined by a plugin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginTool {
    pub name: String,
    pub description: String,
    /// Command to execute the tool
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// Custom agent configuration
/// Defines specialized agents with custom roles, tools, and behaviors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// Agent display name
    pub name: String,
    /// Short description
    #[serde(default)]
    pub description: Option<String>,
    /// Icon/emoji for the agent
    #[serde(default)]
    pub icon: Option<String>,
    /// Version of this agent config
    #[serde(default)]
    pub version: Option<String>,
    /// Base mode: "plan", "build", or "review"
    #[serde(default = "default_base_mode")]
    pub base_mode: String,
    /// System prompt for this agent
    #[serde(default)]
    pub system_prompt: Option<String>,
    /// Path to system prompt file (relative to agent config)
    #[serde(default)]
    pub system_prompt_file: Option<String>,
    /// Rules to include (from rules/ directory)
    #[serde(default)]
    pub include_rules: Vec<String>,
    /// Tool configuration
    #[serde(default)]
    pub tools: AgentToolsConfig,
    /// LLM settings override
    #[serde(default)]
    pub llm: AgentLlmConfig,
    /// Auto-activation triggers
    #[serde(default)]
    pub triggers: AgentTriggers,
    /// Output preferences
    #[serde(default)]
    pub output: AgentOutputConfig,
}

fn default_base_mode() -> String {
    "build".to_string()
}

fn default_approval_mode() -> String {
    "ask_risky".to_string()
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            name: "Custom Agent".to_string(),
            description: None,
            icon: None,
            version: None,
            base_mode: "build".to_string(),
            system_prompt: None,
            system_prompt_file: None,
            include_rules: Vec::new(),
            tools: AgentToolsConfig::default(),
            llm: AgentLlmConfig::default(),
            triggers: AgentTriggers::default(),
            output: AgentOutputConfig::default(),
        }
    }
}

/// Tool configuration for an agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentToolsConfig {
    /// Explicitly allowed tools (overrides base_mode if set)
    #[serde(default)]
    pub allowed: Vec<String>,
    /// Explicitly denied tools (takes precedence)
    #[serde(default)]
    pub denied: Vec<String>,
    /// Tool-specific configurations
    #[serde(flatten)]
    pub config: HashMap<String, toml::Value>,
}

/// LLM settings override for an agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentLlmConfig {
    /// Temperature (0.0-1.0)
    #[serde(default)]
    pub temperature: Option<f32>,
    /// Max tokens for responses
    #[serde(default)]
    pub max_tokens: Option<usize>,
    /// Provider override
    #[serde(default)]
    pub provider: Option<String>,
    /// Model override
    #[serde(default)]
    pub model: Option<String>,
}

/// Auto-activation triggers for an agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentTriggers {
    /// File patterns that suggest this agent
    #[serde(default)]
    pub file_patterns: Vec<String>,
    /// Keywords in user message
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Git contexts (e.g., "pull_request", "pre-commit")
    #[serde(default)]
    pub git_contexts: Vec<String>,
}

/// Output preferences for an agent
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentOutputConfig {
    /// Always show verbose tool calls
    #[serde(default)]
    pub verbose: bool,
    /// Output format preference
    #[serde(default)]
    pub format: Option<String>,
    /// Include file references/links
    #[serde(default)]
    pub include_file_links: bool,
}

/// Agent info with scope (for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    /// Agent ID (filename without extension)
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: Option<String>,
    /// Icon
    pub icon: Option<String>,
    /// Base mode
    pub base_mode: String,
    /// Where this agent is defined
    pub scope: ConfigScope,
    /// Path to config file
    pub path: PathBuf,
}

/// Rule info with scope
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleInfo {
    pub name: String,
    pub scope: ConfigScope,
}

/// A saved conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedConversation {
    /// Unique ID (timestamp-based)
    pub id: String,
    /// Optional title
    pub title: Option<String>,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Agent mode used
    pub mode: String,
    /// Provider used
    pub provider: String,
    /// Model used
    pub model: Option<String>,
    /// Messages in the conversation
    pub messages: Vec<SavedMessage>,
    /// Token usage statistics
    pub token_stats: TokenStats,
}

impl SavedConversation {
    /// Create a new conversation
    pub fn new(mode: &str, provider: &str, model: Option<&str>) -> Self {
        let now = Utc::now();
        Self {
            id: now.format("%Y%m%d_%H%M%S").to_string(),
            title: None,
            created_at: now,
            updated_at: now,
            mode: mode.to_string(),
            provider: provider.to_string(),
            model: model.map(String::from),
            messages: Vec::new(),
            token_stats: TokenStats::default(),
        }
    }

    /// Add a message
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(SavedMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
        });
        self.updated_at = Utc::now();
    }
}

/// A message in a saved conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<Vec<SavedToolCall>>,
}

/// A tool call in a saved message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedToolCall {
    pub tool: String,
    pub args: serde_json::Value,
    pub result_preview: Option<String>,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenStats {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub estimated_cost: f64,
}

/// Summary of a conversation (for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub title: Option<String>,
    pub created_at: DateTime<Utc>,
    pub message_count: usize,
    pub mode: String,
}

// ========== Chat Sessions ==========

/// Model preference for a specific agent mode
///
/// Stores the provider and model selection for a single mode (Build/Plan/Ask).
/// Empty strings indicate no preference has been set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelPreference {
    /// Provider ID (e.g., "openai", "claude", "ollama")
    pub provider: String,
    /// Model ID (e.g., "gpt-4o", "claude-sonnet-4")
    pub model: String,
}

impl ModelPreference {
    /// Create a new model preference
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }

    /// Check if this preference is empty (no selection made)
    pub fn is_empty(&self) -> bool {
        self.provider.is_empty() && self.model.is_empty()
    }
}

/// Per-mode model preferences
///
/// Stores separate provider/model preferences for each agent mode,
/// allowing different model configurations for different workflows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModePreferences {
    /// Model preference for Build mode
    pub build: ModelPreference,
    /// Model preference for Plan mode
    pub plan: ModelPreference,
    /// Model preference for Ask mode
    pub ask: ModelPreference,
}

impl ModePreferences {
    /// Get the preference for a specific mode
    pub fn get(&self, mode: &str) -> &ModelPreference {
        match mode.to_lowercase().as_str() {
            "build" => &self.build,
            "plan" => &self.plan,
            "ask" => &self.ask,
            _ => &self.build, // Default to build mode
        }
    }

    /// Set the preference for a specific mode
    pub fn set(&mut self, mode: &str, preference: ModelPreference) {
        match mode.to_lowercase().as_str() {
            "build" => self.build = preference,
            "plan" => self.plan = preference,
            "ask" => self.ask = preference,
            _ => {} // Ignore unknown modes
        }
    }

    /// Check if a mode has a preference set
    pub fn has_preference(&self, mode: &str) -> bool {
        !self.get(mode).is_empty()
    }
}

/// A chat session with conversation history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    /// Unique session ID
    pub id: String,
    /// Session name (derived from first prompt or auto-generated)
    pub name: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last activity timestamp
    pub updated_at: DateTime<Utc>,
    /// Current provider
    pub provider: String,
    /// Current model
    pub model: String,
    /// Current agent mode (plan/build/ask)
    pub mode: String,
    /// Approval mode (ask_risky/only_reads/zero_trust)
    #[serde(default = "default_approval_mode")]
    pub approval_mode: String,
    /// Window style (split/sidepane/popup)
    pub window_style: String,
    /// Window position
    pub window_position: String,
    /// Per-mode model preferences (Build/Plan/Ask)
    #[serde(default)]
    pub mode_preferences: ModePreferences,
    /// Conversation messages
    pub messages: Vec<SessionMessage>,
    /// Total input tokens used
    pub input_tokens: usize,
    /// Total output tokens used
    pub output_tokens: usize,
    /// Total cost
    pub total_cost: f64,
}

impl ChatSession {
    /// Create a new session
    ///
    /// Creates a session with empty provider/model - these will be determined
    /// by the config defaults or user selection when the session is used.
    pub fn new() -> Self {
        let now = Utc::now();
        // Use UUID v4 for cryptographically unique session IDs
        let id = format!("session_{}", uuid::Uuid::new_v4());
        Self {
            id,
            name: String::new(), // Will be set from first prompt
            created_at: now,
            updated_at: now,
            provider: String::new(), // Determined by config/user selection
            model: String::new(),
            mode: "build".to_string(),
            approval_mode: "ask_risky".to_string(),
            window_style: "sidepane".to_string(),
            window_position: "right".to_string(),
            mode_preferences: ModePreferences::default(),
            messages: Vec::new(),
            input_tokens: 0,
            output_tokens: 0,
            total_cost: 0.0,
        }
    }

    /// Set session name (truncated from first prompt)
    pub fn set_name_from_prompt(&mut self, prompt: &str) {
        if self.name.is_empty() {
            // Truncate to 50 chars, clean up whitespace
            let name = prompt
                .lines()
                .next()
                .unwrap_or(prompt)
                .chars()
                .take(50)
                .collect::<String>()
                .trim()
                .to_string();
            self.name = if name.is_empty() {
                format!("Session {}", self.created_at.format("%H:%M"))
            } else if name.len() < prompt.len() {
                format!("{}...", name)
            } else {
                name
            };
        }
    }

    /// Add a message to the session
    pub fn add_message(&mut self, role: &str, content: &str) {
        self.messages.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls: Vec::new(),
            thinking_content: None,
            segments: Vec::new(),
        });
        self.updated_at = Utc::now();
    }

    /// Add a message with tool calls (for assistant messages with tool history)
    pub fn add_message_with_tools(
        &mut self,
        role: &str,
        content: &str,
        tool_calls: Vec<ToolCallRecord>,
    ) {
        self.messages.push(SessionMessage {
            role: role.to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls,
            thinking_content: None,
            segments: Vec::new(),
        });
        self.updated_at = Utc::now();
    }

    /// Add a complete assistant message with all metadata
    pub fn add_assistant_message_full(
        &mut self,
        content: &str,
        tool_calls: Vec<ToolCallRecord>,
        thinking_content: Option<String>,
        segments: Vec<SegmentRecord>,
    ) {
        self.messages.push(SessionMessage {
            role: "assistant".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: None,
            tool_calls,
            thinking_content,
            segments,
        });
        self.updated_at = Utc::now();
    }

    /// Add a tool message
    pub fn add_tool_message(&mut self, tool_call_id: &str, content: &str) {
        self.messages.push(SessionMessage {
            role: "tool".to_string(),
            content: content.to_string(),
            timestamp: Utc::now(),
            tool_call_id: Some(tool_call_id.to_string()),
            tool_calls: Vec::new(),
            thinking_content: None,
            segments: Vec::new(),
        });
        self.updated_at = Utc::now();
    }

    /// Update the last assistant message with full metadata
    ///
    /// Called after TUI has built the complete message with segments and thinking
    pub fn update_last_assistant_metadata(
        &mut self,
        thinking_content: Option<String>,
        segments: Vec<SegmentRecord>,
    ) {
        if let Some(last) = self.messages.last_mut() {
            if last.role == "assistant" {
                last.thinking_content = thinking_content;
                last.segments = segments;
                self.updated_at = Utc::now();
            }
        }
    }

    /// Clear messages but keep settings and accumulated cost
    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.input_tokens = 0;
        self.output_tokens = 0;
        // Note: total_cost is intentionally NOT reset - it accumulates across the session
        self.updated_at = Utc::now();
    }
}

impl Default for ChatSession {
    fn default() -> Self {
        Self::new()
    }
}

/// A tool call record for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallRecord {
    /// Tool name (e.g., "ripgrep", "read_file")
    pub tool: String,
    /// Tool arguments as JSON
    pub args: serde_json::Value,
    /// Preview of the tool result
    pub result_preview: String,
    /// Error message if the tool failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Segment record for preserving message structure order
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SegmentRecord {
    /// Text content segment with its actual content
    Text(String),
    /// Tool reference by index into tool_calls
    Tool(usize),
}

/// A message in a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// Tool calls made during this message (for assistant messages)
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tool_calls: Vec<ToolCallRecord>,
    /// Thinking/reasoning content (for assistant messages)
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub thinking_content: Option<String>,
    /// Segment order to preserve interleaved display
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub segments: Vec<SegmentRecord>,
}

/// Session metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub provider: String,
    pub model: String,
    pub mode: String,
    pub message_count: usize,
    /// Is this the current active session
    pub is_current: bool,
    /// Is agent currently processing in this session
    pub agent_running: bool,
}

impl From<&ChatSession> for SessionMeta {
    fn from(session: &ChatSession) -> Self {
        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            provider: session.provider.clone(),
            model: session.model.clone(),
            mode: session.mode.clone(),
            message_count: session.messages.len(),
            is_current: false,    // Will be set by caller
            agent_running: false, // Will be set by caller
        }
    }
}

// ========== Compaction Records ==========

/// Record of a context compaction operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionRecord {
    /// Unique compaction ID
    pub id: String,
    /// When compaction occurred
    pub timestamp: DateTime<Utc>,
    /// Token count before compaction
    pub old_tokens: usize,
    /// Token count after compaction
    pub new_tokens: usize,
    /// Number of messages removed
    pub messages_removed: usize,
    /// Summary generated by compaction (if LLM summarization was used)
    pub summary: Option<String>,
}

impl CompactionRecord {
    /// Create a new compaction record
    pub fn new(
        old_tokens: usize,
        new_tokens: usize,
        messages_removed: usize,
        summary: Option<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Utc::now(),
            old_tokens,
            new_tokens,
            messages_removed,
            summary,
        }
    }
}

// ========== Execution Plans ==========

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Skipped,
    Failed,
}

impl std::fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "⬜ pending"),
            TaskStatus::InProgress => write!(f, "🔄 in_progress"),
            TaskStatus::Completed => write!(f, "✅ completed"),
            TaskStatus::Skipped => write!(f, "⏭️ skipped"),
            TaskStatus::Failed => write!(f, "❌ failed"),
        }
    }
}

/// A subtask within a task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanSubtask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl PlanSubtask {
    /// Check if subtask is complete
    pub fn is_complete(&self) -> bool {
        self.status == TaskStatus::Completed || self.status == TaskStatus::Skipped
    }
}

/// A task in an execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub subtasks: Vec<PlanSubtask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Files that will be modified by this task
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

impl PlanTask {
    /// Check if all subtasks are completed
    pub fn is_complete(&self) -> bool {
        if self.subtasks.is_empty() {
            self.status == TaskStatus::Completed || self.status == TaskStatus::Skipped
        } else {
            self.subtasks
                .iter()
                .all(|s| s.status == TaskStatus::Completed || s.status == TaskStatus::Skipped)
        }
    }

    /// Get progress as (completed, total)
    pub fn progress(&self) -> (usize, usize) {
        if self.subtasks.is_empty() {
            let done = if self.status == TaskStatus::Completed || self.status == TaskStatus::Skipped
            {
                1
            } else {
                0
            };
            (done, 1)
        } else {
            let completed = self
                .subtasks
                .iter()
                .filter(|s| s.status == TaskStatus::Completed || s.status == TaskStatus::Skipped)
                .count();
            (completed, self.subtasks.len())
        }
    }
}

/// Execution plan status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlanStatus {
    #[default]
    Draft,
    Active,
    Paused,
    Completed,
    Abandoned,
}

/// An execution plan with tasks and subtasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPlan {
    /// Unique plan ID (derived from title, max 10 words)
    pub id: String,
    /// Plan title/name
    pub title: String,
    /// Original prompt that generated this plan (deprecated, use overview)
    #[serde(default)]
    pub original_prompt: String,
    /// Plan status
    pub status: PlanStatus,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Last updated timestamp
    pub updated_at: DateTime<Utc>,
    /// Tasks in this plan
    pub tasks: Vec<PlanTask>,
    /// Index of current task being executed (for resume)
    pub current_task_index: usize,
    /// Index of current subtask within current task
    pub current_subtask_index: usize,
    /// Session ID this plan is associated with
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Notes/refinements added to the plan
    #[serde(default)]
    pub refinements: Vec<PlanRefinement>,

    // ===== V2 Schema Fields =====
    /// High-level overview of what this plan accomplishes
    #[serde(default)]
    pub overview: String,
    /// Architecture notes (components, data flow, dependencies)
    #[serde(default)]
    pub architecture: String,
    /// Proposed changes summary (what will be modified and how)
    #[serde(default)]
    pub proposed_changes: String,
    /// Acceptance criteria for plan completion
    #[serde(default)]
    pub acceptance_criteria: Vec<AcceptanceCriterion>,
    /// Reference to the agent/model working on this plan
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<AgentRef>,
    /// Detected tech stack of the codebase
    #[serde(default)]
    pub stack: TechStack,
}

/// A refinement/modification to the plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRefinement {
    pub timestamp: DateTime<Utc>,
    pub description: String,
}

/// Detected tech stack of the codebase
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TechStack {
    /// Primary language (rust, python, typescript, go, java, etc.)
    pub language: String,
    /// Framework if detected (axum, fastapi, express, gin, spring, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub framework: Option<String>,
    /// UI library if detected (react, vue, svelte, tailwind, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_library: Option<String>,
    /// Test command (cargo test, pytest, npm test, go test, bun test)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_command: Option<String>,
    /// Build command (cargo build, npm run build, go build)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_command: Option<String>,
}

/// Reference to the agent working on this plan
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRef {
    /// Agent/model ID (e.g., "claude-sonnet-4-20250514")
    pub id: String,
    /// Agent display name (e.g., "Claude Sonnet 4")
    pub name: String,
}

/// Acceptance criterion for plan completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptanceCriterion {
    pub description: String,
    pub met: bool,
}

impl ExecutionPlan {
    /// Create a new plan from a prompt
    pub fn new(title: &str, original_prompt: &str) -> Self {
        let now = Utc::now();
        // Generate ID from title (max 10 words, sanitized)
        let id = Self::generate_id(title);

        Self {
            id,
            title: title.to_string(),
            original_prompt: original_prompt.to_string(),
            status: PlanStatus::Draft,
            created_at: now,
            updated_at: now,
            tasks: Vec::new(),
            current_task_index: 0,
            current_subtask_index: 0,
            session_id: None,
            refinements: Vec::new(),
            // V2 fields
            overview: String::new(),
            architecture: String::new(),
            proposed_changes: String::new(),
            acceptance_criteria: Vec::new(),
            agent: None,
            stack: TechStack::default(),
        }
    }

    /// Generate plan ID from title (max 10 words, sanitized)
    fn generate_id(title: &str) -> String {
        let words: Vec<&str> = title.split_whitespace().take(10).collect();

        let base = words
            .join("_")
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect::<String>();

        // Add timestamp suffix for uniqueness
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        format!("{}_{}", base, timestamp)
    }

    /// Add a task to the plan
    pub fn add_task(&mut self, description: &str) -> &mut PlanTask {
        let task_id = format!("task_{}", self.tasks.len() + 1);
        self.tasks.push(PlanTask {
            id: task_id,
            description: description.to_string(),
            status: TaskStatus::Pending,
            subtasks: Vec::new(),
            notes: None,
            files: Vec::new(),
        });
        self.updated_at = Utc::now();
        self.tasks.last_mut().unwrap()
    }

    /// Add a task with associated files
    pub fn add_task_with_files(&mut self, description: &str, files: Vec<String>) -> &mut PlanTask {
        let task_id = format!("task_{}", self.tasks.len() + 1);
        self.tasks.push(PlanTask {
            id: task_id,
            description: description.to_string(),
            status: TaskStatus::Pending,
            subtasks: Vec::new(),
            notes: None,
            files,
        });
        self.updated_at = Utc::now();
        self.tasks.last_mut().unwrap()
    }

    /// Add a subtask to a task
    pub fn add_subtask(
        &mut self,
        task_index: usize,
        description: &str,
    ) -> Option<&mut PlanSubtask> {
        if let Some(task) = self.tasks.get_mut(task_index) {
            let subtask_id = format!("{}_sub_{}", task.id, task.subtasks.len() + 1);
            task.subtasks.push(PlanSubtask {
                id: subtask_id,
                description: description.to_string(),
                status: TaskStatus::Pending,
                notes: None,
            });
            self.updated_at = Utc::now();
            task.subtasks.last_mut()
        } else {
            None
        }
    }

    /// Mark a task as complete
    pub fn complete_task(&mut self, task_index: usize) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.status = TaskStatus::Completed;
            // Also mark all subtasks as complete
            for subtask in &mut task.subtasks {
                if subtask.status == TaskStatus::Pending || subtask.status == TaskStatus::InProgress
                {
                    subtask.status = TaskStatus::Completed;
                }
            }
            self.updated_at = Utc::now();
        }
    }

    /// Mark a subtask as complete
    pub fn complete_subtask(&mut self, task_index: usize, subtask_index: usize) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            if let Some(subtask) = task.subtasks.get_mut(subtask_index) {
                subtask.status = TaskStatus::Completed;
                self.updated_at = Utc::now();

                // Check if all subtasks are done, mark task complete
                if task.is_complete() {
                    task.status = TaskStatus::Completed;
                }
            }
        }
    }

    /// Set task status
    pub fn set_task_status(&mut self, task_index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.status = status;
            self.updated_at = Utc::now();
        }
    }

    /// Set subtask status
    pub fn set_subtask_status(
        &mut self,
        task_index: usize,
        subtask_index: usize,
        status: TaskStatus,
    ) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            if let Some(subtask) = task.subtasks.get_mut(subtask_index) {
                subtask.status = status;
                self.updated_at = Utc::now();
            }
        }
    }

    /// Get next pending task/subtask for execution
    pub fn get_next_pending(&self) -> Option<(usize, Option<usize>)> {
        for (task_idx, task) in self.tasks.iter().enumerate() {
            if task.status == TaskStatus::Pending || task.status == TaskStatus::InProgress {
                // Check subtasks first
                for (sub_idx, subtask) in task.subtasks.iter().enumerate() {
                    if subtask.status == TaskStatus::Pending {
                        return Some((task_idx, Some(sub_idx)));
                    }
                }
                // No pending subtasks, return task itself if no subtasks
                if task.subtasks.is_empty() && task.status == TaskStatus::Pending {
                    return Some((task_idx, None));
                }
            }
        }
        None
    }

    /// Check if plan is complete
    pub fn is_complete(&self) -> bool {
        self.tasks.iter().all(|t| t.is_complete())
    }

    /// Get overall progress as (completed, total)
    pub fn progress(&self) -> (usize, usize) {
        let mut completed = 0;
        let mut total = 0;

        for task in &self.tasks {
            let (c, t) = task.progress();
            completed += c;
            total += t;
        }

        (completed, total)
    }

    /// Add a refinement note
    pub fn add_refinement(&mut self, description: &str) {
        self.refinements.push(PlanRefinement {
            timestamp: Utc::now(),
            description: description.to_string(),
        });
        self.updated_at = Utc::now();
    }

    /// Format plan as markdown with YAML frontmatter
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        // YAML frontmatter
        md.push_str("---\n");
        md.push_str(&format!("id: {}\n", self.id));
        md.push_str(&format!("title: \"{}\"\n", self.title.replace('"', "\\\"")));
        md.push_str(&format!(
            "status: {}\n",
            format!("{:?}", self.status).to_lowercase()
        ));
        md.push_str(&format!("created: {}\n", self.created_at.to_rfc3339()));
        md.push_str(&format!("updated: {}\n", self.updated_at.to_rfc3339()));
        if let Some(ref session_id) = self.session_id {
            md.push_str(&format!("session_id: {}\n", session_id));
        }
        md.push_str(&format!("current_task: {}\n", self.current_task_index));
        md.push_str(&format!(
            "current_subtask: {}\n",
            self.current_subtask_index
        ));
        // V2: Agent reference
        if let Some(ref agent) = self.agent {
            md.push_str(&format!("agent_id: {}\n", agent.id));
            md.push_str(&format!(
                "agent_name: \"{}\"\n",
                agent.name.replace('"', "\\\"")
            ));
        }
        // V2: Tech stack
        if !self.stack.language.is_empty() {
            md.push_str(&format!("stack_language: {}\n", self.stack.language));
            if let Some(ref fw) = self.stack.framework {
                md.push_str(&format!("stack_framework: {}\n", fw));
            }
            if let Some(ref ui) = self.stack.ui_library {
                md.push_str(&format!("stack_ui: {}\n", ui));
            }
            if let Some(ref tc) = self.stack.test_command {
                md.push_str(&format!("stack_test: {}\n", tc));
            }
            if let Some(ref bc) = self.stack.build_command {
                md.push_str(&format!("stack_build: {}\n", bc));
            }
        }
        md.push_str("---\n\n");

        // Title
        md.push_str(&format!("# {}\n\n", self.title));

        // V2: Overview section (preferred over original_prompt)
        if !self.overview.is_empty() {
            md.push_str("## Overview\n\n");
            md.push_str(&self.overview);
            md.push_str("\n\n");
        } else if !self.original_prompt.is_empty() {
            // Fallback to original_prompt for backwards compatibility
            md.push_str(&format!("{}\n\n", self.original_prompt));
        }

        // V2: Architecture section
        if !self.architecture.is_empty() {
            md.push_str("## Architecture\n\n");
            md.push_str(&self.architecture);
            md.push_str("\n\n");
        }

        // V2: Proposed Changes section
        if !self.proposed_changes.is_empty() {
            md.push_str("## Proposed Changes\n\n");
            md.push_str(&self.proposed_changes);
            md.push_str("\n\n");
        }

        // Tasks section
        md.push_str("## Tasks\n\n");

        for (i, task) in self.tasks.iter().enumerate() {
            let checkbox = match task.status {
                TaskStatus::Completed | TaskStatus::Skipped => "[x]",
                TaskStatus::InProgress => "[-]",
                _ => "[ ]",
            };
            md.push_str(&format!(
                "- {} **{}.** {}\n",
                checkbox,
                i + 1,
                task.description
            ));

            // V2: Files associated with task
            if !task.files.is_empty() {
                md.push_str(&format!("  - Files: {}\n", task.files.join(", ")));
            }

            for (j, subtask) in task.subtasks.iter().enumerate() {
                let sub_checkbox = match subtask.status {
                    TaskStatus::Completed | TaskStatus::Skipped => "[x]",
                    TaskStatus::InProgress => "[-]",
                    _ => "[ ]",
                };
                md.push_str(&format!(
                    "  - {} {}.{} {}\n",
                    sub_checkbox,
                    i + 1,
                    j + 1,
                    subtask.description
                ));
            }

            // Task notes (audit trail for mark_task_done)
            if let Some(ref notes) = task.notes {
                md.push_str(&format!("  - Notes: {}\n", notes));
            }
        }

        // V2: Acceptance Criteria section
        if !self.acceptance_criteria.is_empty() {
            md.push_str("\n## Acceptance Criteria\n\n");
            for criterion in &self.acceptance_criteria {
                let checkbox = if criterion.met { "[x]" } else { "[ ]" };
                md.push_str(&format!("- {} {}\n", checkbox, criterion.description));
            }
        }

        // Notes/Refinements section
        if !self.refinements.is_empty() {
            md.push_str("\n## Notes\n\n");
            for r in &self.refinements {
                md.push_str(&format!(
                    "- {} _({})\n",
                    r.description,
                    r.timestamp.format("%Y-%m-%d %H:%M")
                ));
            }
        }

        md
    }

    /// Parse plan from markdown with YAML frontmatter
    pub fn from_markdown(content: &str) -> Result<Self> {
        // Split frontmatter and body
        let content = content.trim();
        if !content.starts_with("---") {
            return Err(anyhow::anyhow!("Missing YAML frontmatter"));
        }

        let parts: Vec<&str> = content.splitn(3, "---").collect();
        if parts.len() < 3 {
            return Err(anyhow::anyhow!("Invalid frontmatter format"));
        }

        let frontmatter = parts[1].trim();
        let body = parts[2].trim();

        // Parse frontmatter
        let mut id = String::new();
        let mut title = String::new();
        let mut status = PlanStatus::Draft;
        let mut created_at = Utc::now();
        let mut updated_at = Utc::now();
        let mut session_id: Option<String> = None;
        let mut current_task_index = 0usize;
        let mut current_subtask_index = 0usize;
        // V2 frontmatter fields
        let mut agent = AgentRef::default();
        let mut stack = TechStack::default();

        for line in frontmatter.lines() {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let key = key.trim();
                let value = value.trim().trim_matches('"');
                match key {
                    "id" => id = value.to_string(),
                    "title" => title = value.to_string(),
                    "status" => {
                        status = match value.to_lowercase().as_str() {
                            "draft" => PlanStatus::Draft,
                            "active" => PlanStatus::Active,
                            "paused" => PlanStatus::Paused,
                            "completed" => PlanStatus::Completed,
                            "abandoned" => PlanStatus::Abandoned,
                            _ => PlanStatus::Draft,
                        };
                    }
                    "created" => {
                        created_at = DateTime::parse_from_rfc3339(value)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                    }
                    "updated" => {
                        updated_at = DateTime::parse_from_rfc3339(value)
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_else(|_| Utc::now());
                    }
                    "session_id" => session_id = Some(value.to_string()),
                    "current_task" => current_task_index = value.parse().unwrap_or(0),
                    "current_subtask" => current_subtask_index = value.parse().unwrap_or(0),
                    // V2 frontmatter
                    "agent_id" => agent.id = value.to_string(),
                    "agent_name" => agent.name = value.to_string(),
                    "stack_language" => stack.language = value.to_string(),
                    "stack_framework" => stack.framework = Some(value.to_string()),
                    "stack_ui" => stack.ui_library = Some(value.to_string()),
                    "stack_test" => stack.test_command = Some(value.to_string()),
                    "stack_build" => stack.build_command = Some(value.to_string()),
                    _ => {}
                }
            }
        }

        // Parse body for sections
        let mut tasks = Vec::new();
        let mut refinements = Vec::new();
        let mut acceptance_criteria = Vec::new();
        let mut original_prompt = String::new();
        let mut overview = String::new();
        let mut architecture = String::new();
        let mut proposed_changes = String::new();

        // Section tracking
        #[derive(PartialEq)]
        enum Section {
            Preamble,
            Overview,
            Architecture,
            ProposedChanges,
            Tasks,
            AcceptanceCriteria,
            Notes,
        }
        let mut current_section = Section::Preamble;
        let mut current_task: Option<PlanTask> = None;

        for line in body.lines() {
            let line_trimmed = line.trim();

            // Skip title line (starts with single #)
            if line_trimmed.starts_with("# ") && !line_trimmed.starts_with("## ") {
                continue;
            }

            // Detect section headers
            if line_trimmed.starts_with("## ") {
                // Save pending task when leaving Tasks section
                if current_section == Section::Tasks {
                    if let Some(task) = current_task.take() {
                        tasks.push(task);
                    }
                }

                current_section = match line_trimmed {
                    "## Overview" => Section::Overview,
                    "## Architecture" => Section::Architecture,
                    "## Proposed Changes" => Section::ProposedChanges,
                    "## Tasks" => Section::Tasks,
                    "## Acceptance Criteria" => Section::AcceptanceCriteria,
                    "## Notes" => Section::Notes,
                    _ => current_section, // Unknown section, keep current
                };
                continue;
            }

            match current_section {
                Section::Preamble => {
                    // Text before any ## section is original_prompt (backwards compat)
                    if !line_trimmed.is_empty() {
                        if !original_prompt.is_empty() {
                            original_prompt.push('\n');
                        }
                        original_prompt.push_str(line_trimmed);
                    }
                }
                Section::Overview => {
                    if !line_trimmed.is_empty() {
                        if !overview.is_empty() {
                            overview.push('\n');
                        }
                        overview.push_str(line_trimmed);
                    }
                }
                Section::Architecture => {
                    if !line_trimmed.is_empty() {
                        if !architecture.is_empty() {
                            architecture.push('\n');
                        }
                        architecture.push_str(line_trimmed);
                    }
                }
                Section::ProposedChanges => {
                    if !line_trimmed.is_empty() {
                        if !proposed_changes.is_empty() {
                            proposed_changes.push('\n');
                        }
                        proposed_changes.push_str(line_trimmed);
                    }
                }
                Section::Tasks => {
                    // Files line: "  - Files: file1, file2"
                    if line.starts_with("  - Files:") && current_task.is_some() {
                        let files_str = line.trim_start_matches("  - Files:").trim();
                        if let Some(ref mut task) = current_task {
                            task.files = files_str
                                .split(", ")
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .collect();
                        }
                    }
                    // Notes line: "  - Notes: ..."
                    else if line.starts_with("  - Notes:") && current_task.is_some() {
                        let notes_str = line.trim_start_matches("  - Notes:").trim();
                        if let Some(ref mut task) = current_task {
                            task.notes = Some(notes_str.to_string());
                        }
                    }
                    // Subtask: "  - [x] 1.1 Description"
                    else if (line.starts_with("  - [") || line.starts_with("    - ["))
                        && current_task.is_some()
                    {
                        let (status, desc) = Self::parse_checkbox_line(line_trimmed);
                        // Strip number prefix like "1.1 " if present
                        let desc = Self::strip_number_prefix(&desc);
                        if let Some(ref mut task) = current_task {
                            task.subtasks.push(PlanSubtask {
                                id: format!("{}_sub_{}", task.id, task.subtasks.len() + 1),
                                description: desc,
                                status,
                                notes: None,
                            });
                        }
                    }
                    // Main task: "- [x] **1.** Description" or "- [x] Description"
                    else if line.starts_with("- [") {
                        // Save previous task
                        if let Some(task) = current_task.take() {
                            tasks.push(task);
                        }

                        let (status, desc) = Self::parse_checkbox_line(line_trimmed);
                        // Strip bold number prefix like "**1.** " if present
                        let desc = Self::strip_bold_number_prefix(&desc);
                        current_task = Some(PlanTask {
                            id: format!("task_{}", tasks.len() + 1),
                            description: desc,
                            status,
                            subtasks: Vec::new(),
                            notes: None,
                            files: Vec::new(),
                        });
                    }
                }
                Section::AcceptanceCriteria => {
                    // Parse: "- [x] Description" or "- [ ] Description"
                    if line_trimmed.starts_with("- [") {
                        let met =
                            line_trimmed.starts_with("- [x]") || line_trimmed.starts_with("- [X]");
                        let desc = line_trimmed
                            .trim_start_matches("- [x]")
                            .trim_start_matches("- [X]")
                            .trim_start_matches("- [ ]")
                            .trim();
                        acceptance_criteria.push(AcceptanceCriterion {
                            description: desc.to_string(),
                            met,
                        });
                    }
                }
                Section::Notes => {
                    // Parse refinement: "- Description _(timestamp)"
                    if line_trimmed.starts_with("- ") {
                        let desc = line_trimmed.trim_start_matches("- ");
                        // Try to extract timestamp from end
                        let (desc_part, timestamp) = if let Some(idx) = desc.rfind(" _(") {
                            let ts_str = desc[idx + 3..].trim_end_matches(')');
                            let ts =
                                chrono::NaiveDateTime::parse_from_str(ts_str, "%Y-%m-%d %H:%M")
                                    .map(|dt| dt.and_utc())
                                    .unwrap_or_else(|_| Utc::now());
                            (desc[..idx].to_string(), ts)
                        } else {
                            (desc.to_string(), Utc::now())
                        };
                        refinements.push(PlanRefinement {
                            timestamp,
                            description: desc_part,
                        });
                    }
                }
            }
        }

        // Don't forget the last task
        if let Some(task) = current_task {
            tasks.push(task);
        }

        Ok(Self {
            id,
            title,
            original_prompt,
            status,
            created_at,
            updated_at,
            tasks,
            current_task_index,
            current_subtask_index,
            session_id,
            refinements,
            // V2 fields
            overview,
            architecture,
            proposed_changes,
            acceptance_criteria,
            agent: if agent.id.is_empty() && agent.name.is_empty() {
                None
            } else {
                Some(agent)
            },
            stack,
        })
    }

    /// Strip bold number prefix like "**1.** " from task descriptions
    fn strip_bold_number_prefix(desc: &str) -> String {
        let desc = desc.trim();
        if desc.starts_with("**") {
            if let Some(end_idx) = desc.find(".**") {
                let after = &desc[end_idx + 3..];
                return after.trim().to_string();
            }
        }
        desc.to_string()
    }

    /// Strip number prefix like "1.1 " from subtask descriptions
    fn strip_number_prefix(desc: &str) -> String {
        let desc = desc.trim();
        // Match patterns like "1.1 ", "2.3 ", etc.
        let mut chars = desc.chars().peekable();
        let mut has_number = false;

        // Skip leading digits
        while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            chars.next();
            has_number = true;
        }

        // Check for dot
        if has_number && chars.peek() == Some(&'.') {
            chars.next();
            // Skip more digits after dot
            while chars.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                chars.next();
            }
            // Skip space
            if chars.peek() == Some(&' ') {
                chars.next();
            }
            return chars.collect::<String>().trim().to_string();
        }

        desc.to_string()
    }

    /// Parse a checkbox line like "- [x] Description" or "- [ ] Description"
    fn parse_checkbox_line(line: &str) -> (TaskStatus, String) {
        let line = line.trim().trim_start_matches("- ");
        if let Some(rest) = line
            .strip_prefix("[x]")
            .or_else(|| line.strip_prefix("[X]"))
        {
            (TaskStatus::Completed, rest.trim().to_string())
        } else if let Some(rest) = line.strip_prefix("[-]") {
            (TaskStatus::InProgress, rest.trim().to_string())
        } else if let Some(rest) = line.strip_prefix("[ ]") {
            (TaskStatus::Pending, rest.trim().to_string())
        } else {
            (TaskStatus::Pending, line.to_string())
        }
    }

    /// Format plan as clean markdown for display (no YAML frontmatter)
    ///
    /// Use this for previewing plans to users or showing in chat.
    pub fn to_preview(&self) -> String {
        let mut md = String::new();

        // Title
        md.push_str(&format!("# {}\n\n", self.title));

        // Overview (preferred) or original_prompt (fallback)
        if !self.overview.is_empty() {
            md.push_str("## Overview\n\n");
            md.push_str(&self.overview);
            md.push_str("\n\n");
        } else if !self.original_prompt.is_empty() {
            md.push_str(&format!("{}\n\n", self.original_prompt));
        }

        // Architecture (if present)
        if !self.architecture.is_empty() {
            md.push_str("## Architecture\n\n");
            md.push_str(&self.architecture);
            md.push_str("\n\n");
        }

        // Proposed Changes (if present)
        if !self.proposed_changes.is_empty() {
            md.push_str("## Proposed Changes\n\n");
            md.push_str(&self.proposed_changes);
            md.push_str("\n\n");
        }

        // Tasks - numbered with checkboxes
        md.push_str("## Tasks\n\n");
        for (i, task) in self.tasks.iter().enumerate() {
            let checkbox = if task.is_complete() { "[x]" } else { "[ ]" };
            md.push_str(&format!(
                "- {} **{}.** {}\n",
                checkbox,
                i + 1,
                task.description
            ));

            if !task.files.is_empty() {
                md.push_str(&format!("  - Files: {}\n", task.files.join(", ")));
            }

            for (j, subtask) in task.subtasks.iter().enumerate() {
                let sub_cb = if subtask.is_complete() { "[x]" } else { "[ ]" };
                md.push_str(&format!(
                    "  - {} {}.{} {}\n",
                    sub_cb,
                    i + 1,
                    j + 1,
                    subtask.description
                ));
            }
        }

        // Acceptance Criteria (if present)
        if !self.acceptance_criteria.is_empty() {
            md.push_str("\n## Acceptance Criteria\n\n");
            for criterion in &self.acceptance_criteria {
                let cb = if criterion.met { "[x]" } else { "[ ]" };
                md.push_str(&format!("- {} {}\n", cb, criterion.description));
            }
        }

        md
    }

    /// Set the overview text
    pub fn set_overview(&mut self, overview: &str) {
        self.overview = overview.to_string();
        self.updated_at = Utc::now();
    }

    /// Set the architecture text
    pub fn set_architecture(&mut self, architecture: &str) {
        self.architecture = architecture.to_string();
        self.updated_at = Utc::now();
    }

    /// Set the proposed changes text
    pub fn set_proposed_changes(&mut self, proposed_changes: &str) {
        self.proposed_changes = proposed_changes.to_string();
        self.updated_at = Utc::now();
    }

    /// Add an acceptance criterion
    pub fn add_acceptance_criterion(&mut self, description: &str) {
        self.acceptance_criteria.push(AcceptanceCriterion {
            description: description.to_string(),
            met: false,
        });
        self.updated_at = Utc::now();
    }

    /// Mark an acceptance criterion as met
    pub fn mark_criterion_met(&mut self, index: usize) {
        if let Some(criterion) = self.acceptance_criteria.get_mut(index) {
            criterion.met = true;
            self.updated_at = Utc::now();
        }
    }

    /// Set the agent reference
    pub fn set_agent(&mut self, id: &str, name: &str) {
        self.agent = Some(AgentRef {
            id: id.to_string(),
            name: name.to_string(),
        });
        self.updated_at = Utc::now();
    }

    /// Set the tech stack
    pub fn set_stack(&mut self, stack: TechStack) {
        self.stack = stack;
        self.updated_at = Utc::now();
    }

    /// Add files to a task
    pub fn set_task_files(&mut self, task_index: usize, files: Vec<String>) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.files = files;
            self.updated_at = Utc::now();
        }
    }

    /// Set notes on a task (for audit trail)
    pub fn set_task_notes(&mut self, task_index: usize, notes: &str) {
        if let Some(task) = self.tasks.get_mut(task_index) {
            task.notes = Some(notes.to_string());
            self.updated_at = Utc::now();
        }
    }
}

/// Plan metadata for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanMeta {
    pub id: String,
    pub title: String,
    pub status: PlanStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub task_count: usize,
    pub progress: (usize, usize),
}

impl From<&ExecutionPlan> for PlanMeta {
    fn from(plan: &ExecutionPlan) -> Self {
        Self {
            id: plan.id.clone(),
            title: plan.title.clone(),
            status: plan.status,
            created_at: plan.created_at,
            updated_at: plan.updated_at,
            task_count: plan.tasks.len(),
            progress: plan.progress(),
        }
    }
}

/// Summary of a plan
#[derive(Debug, Clone)]
pub struct PlanSummary {
    pub name: String,
    pub path: PathBuf,
    pub modified_at: Option<DateTime<Utc>>,
}

/// A custom rule
#[derive(Debug, Clone)]
pub struct Rule {
    pub name: String,
    pub content: String,
}

// ========== Helpers ==========

/// Sanitize a filename (remove unsafe characters)
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Load rules from a directory
fn load_rules_from_dir(dir: &Path) -> Result<Vec<Rule>> {
    let mut rules = Vec::new();

    if !dir.exists() {
        return Ok(rules);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                if let Ok(content) = std::fs::read_to_string(&path) {
                    rules.push(Rule { name, content });
                }
            }
        }
    }

    Ok(rules)
}

/// List rule names from a directory
fn list_rule_names(dir: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();

    if !dir.exists() {
        return Ok(names);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "md").unwrap_or(false) {
                if let Some(name) = path.file_stem() {
                    names.push(name.to_string_lossy().to_string());
                }
            }
        }
    }

    Ok(names)
}

/// List plugins from a directory
fn list_plugins_from_dir(dir: &Path) -> Result<Vec<PluginInfo>> {
    let mut plugins = Vec::new();

    if !dir.exists() {
        return Ok(plugins);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let plugin_toml = path.join("plugin.toml");
                if plugin_toml.exists() {
                    let name = path
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();

                    // Try to load plugin config for details
                    let (enabled, description) =
                        if let Ok(content) = std::fs::read_to_string(&plugin_toml) {
                            if let Ok(config) = toml::from_str::<PluginConfig>(&content) {
                                (config.enabled, config.description)
                            } else {
                                (true, None)
                            }
                        } else {
                            (true, None)
                        };

                    plugins.push(PluginInfo {
                        name,
                        path: path.clone(),
                        scope: ConfigScope::Global, // Will be overridden for project plugins
                        enabled,
                        description,
                    });
                }
            }
        }
    }

    Ok(plugins)
}

/// Load agents from a directory
fn load_agents_from_dir(dir: &Path, scope: ConfigScope) -> Result<Vec<AgentInfo>> {
    let mut agents = Vec::new();

    if !dir.exists() {
        return Ok(agents);
    }

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                let id = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();

                // Try to load agent config for details
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(config) = toml::from_str::<AgentConfig>(&content) {
                        agents.push(AgentInfo {
                            id,
                            name: config.name,
                            description: config.description,
                            icon: config.icon,
                            base_mode: config.base_mode,
                            scope,
                            path: path.clone(),
                        });
                    }
                }
            }
        }
    }

    Ok(agents)
}

/// Simple glob matching (supports * and ?)
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();

    // Simple implementation - handle * as wildcard
    if pattern.contains('*') {
        let parts: Vec<&str> = pattern.split('*').collect();
        if parts.len() == 2 {
            // Pattern like "*.test.*" or "test_*"
            let starts = parts[0].is_empty() || text.starts_with(parts[0]);
            let ends = parts[1].is_empty() || text.ends_with(parts[1]);
            return starts && ends;
        }
    }

    // Exact match fallback
    pattern == text || text.contains(&pattern)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_init() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();

        // Check project-level directories (in .tark/)
        assert!(storage.project_root().exists());
        assert!(storage.project_root().join("conversations").exists());
        assert!(storage.project_root().join("plans").exists());
        assert!(storage.project_root().join("rules").exists());

        // Check global directories exist
        assert!(storage.global.root().exists());
        assert!(storage.global.root().join("rules").exists());
    }

    #[test]
    fn test_config_roundtrip() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();

        let config = WorkspaceConfig {
            provider: "claude".to_string(),
            verbose: true,
            ..Default::default()
        };

        storage.save_config(&config).unwrap();
        let loaded = storage.load_config().unwrap();

        assert_eq!(loaded.provider, "claude");
        assert!(loaded.verbose);
    }

    #[test]
    fn test_clear_messages_preserves_cost() {
        let mut session = ChatSession::new();

        // Simulate some usage
        session.add_message("user", "Hello");
        session.add_message("assistant", "Hi there!");
        session.input_tokens = 100;
        session.output_tokens = 50;
        session.total_cost = 0.0025; // Accumulated cost

        // Clear messages
        session.clear_messages();

        // Verify messages and tokens are cleared
        assert!(session.messages.is_empty());
        assert_eq!(session.input_tokens, 0);
        assert_eq!(session.output_tokens, 0);

        // Verify cost is PRESERVED (not reset)
        assert_eq!(session.total_cost, 0.0025);
    }

    #[test]
    fn test_clear_messages_accumulates_cost() {
        let mut session = ChatSession::new();

        // First conversation
        session.add_message("user", "First question");
        session.add_message("assistant", "First answer");
        session.input_tokens = 50;
        session.output_tokens = 100;
        session.total_cost = 0.001;

        // Clear and start new conversation
        session.clear_messages();

        // Second conversation adds more cost
        session.add_message("user", "Second question");
        session.add_message("assistant", "Second answer");
        session.input_tokens = 75;
        session.output_tokens = 150;
        session.total_cost += 0.002; // Add to existing cost

        // Total cost should be sum of both conversations
        assert_eq!(session.total_cost, 0.003);
        // But tokens should only reflect current context
        assert_eq!(session.input_tokens, 75);
        assert_eq!(session.output_tokens, 150);
    }

    #[test]
    fn test_session_conversations_persisted_in_session_dir() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();

        let mut session = ChatSession::new();
        session.add_message("user", "Hello");
        session.add_message("assistant", "Hi");

        storage.save_session(&session).unwrap();

        let conversations_path = storage
            .session_dir(&session.id)
            .join("conversations")
            .join("conversation.json");
        assert!(conversations_path.exists());

        let loaded = storage.load_session(&session.id).unwrap();
        assert_eq!(loaded.messages.len(), 2);
        assert_eq!(loaded.messages[0].content, "Hello");
        assert_eq!(loaded.messages[1].content, "Hi");
    }

    #[test]
    fn test_execution_plan_v2_markdown_roundtrip() {
        // Create a V2 plan with all new fields
        let mut plan = ExecutionPlan::new("Test Plan", "");
        plan.set_overview("This is a comprehensive test plan for the new schema.");
        plan.set_architecture("- Component A: handles X\n- Component B: handles Y");
        plan.set_proposed_changes("1. Modify file A\n2. Add new file B");
        plan.add_acceptance_criterion("All tests pass");
        plan.add_acceptance_criterion("Documentation updated");
        plan.set_agent("claude-sonnet-4", "Claude Sonnet 4");
        plan.set_stack(TechStack {
            language: "rust".to_string(),
            framework: Some("axum".to_string()),
            ui_library: None,
            test_command: Some("cargo test".to_string()),
            build_command: Some("cargo build --release".to_string()),
        });

        // Add tasks with files
        plan.add_task_with_files(
            "Implement feature X",
            vec!["src/lib.rs".to_string(), "src/feature.rs".to_string()],
        );
        plan.add_subtask(0, "Create module structure");
        plan.add_subtask(0, "Write unit tests");
        plan.set_task_notes(0, "Completed with 100% coverage");
        plan.complete_task(0);

        plan.add_task("Update documentation");
        plan.add_subtask(1, "Add README section");

        // Serialize to markdown
        let md = plan.to_markdown();

        // Verify key sections are present
        assert!(md.contains("## Overview"));
        assert!(md.contains("comprehensive test plan"));
        assert!(md.contains("## Architecture"));
        assert!(md.contains("Component A"));
        assert!(md.contains("## Proposed Changes"));
        assert!(md.contains("Modify file A"));
        assert!(md.contains("## Tasks"));
        assert!(md.contains("Files: src/lib.rs, src/feature.rs"));
        assert!(md.contains("Notes: Completed with 100% coverage"));
        assert!(md.contains("## Acceptance Criteria"));
        assert!(md.contains("All tests pass"));
        assert!(md.contains("stack_language: rust"));
        assert!(md.contains("agent_id: claude-sonnet-4"));

        // Parse back
        let parsed = ExecutionPlan::from_markdown(&md).unwrap();

        // Verify roundtrip
        assert_eq!(parsed.title, "Test Plan");
        assert_eq!(parsed.overview, plan.overview);
        assert_eq!(parsed.architecture, plan.architecture);
        assert_eq!(parsed.proposed_changes, plan.proposed_changes);
        assert_eq!(parsed.acceptance_criteria.len(), 2);
        assert_eq!(parsed.acceptance_criteria[0].description, "All tests pass");
        assert!(!parsed.acceptance_criteria[0].met);

        // Agent
        assert!(parsed.agent.is_some());
        let agent = parsed.agent.unwrap();
        assert_eq!(agent.id, "claude-sonnet-4");
        assert_eq!(agent.name, "Claude Sonnet 4");

        // Stack
        assert_eq!(parsed.stack.language, "rust");
        assert_eq!(parsed.stack.framework, Some("axum".to_string()));
        assert_eq!(parsed.stack.test_command, Some("cargo test".to_string()));

        // Tasks
        assert_eq!(parsed.tasks.len(), 2);
        assert_eq!(parsed.tasks[0].description, "Implement feature X");
        assert_eq!(parsed.tasks[0].files, vec!["src/lib.rs", "src/feature.rs"]);
        assert_eq!(
            parsed.tasks[0].notes,
            Some("Completed with 100% coverage".to_string())
        );
        assert_eq!(parsed.tasks[0].status, TaskStatus::Completed);
        assert_eq!(parsed.tasks[0].subtasks.len(), 2);
        assert_eq!(
            parsed.tasks[0].subtasks[0].description,
            "Create module structure"
        );
    }

    #[test]
    fn test_execution_plan_to_preview() {
        let mut plan = ExecutionPlan::new("Preview Test", "");
        plan.set_overview("Testing the preview output.");
        plan.add_task("Task one");
        plan.add_task("Task two");
        plan.add_subtask(1, "Subtask 2.1");
        plan.complete_task(0);
        plan.add_acceptance_criterion("Works correctly");

        let preview = plan.to_preview();

        // Preview should NOT contain frontmatter
        assert!(!preview.contains("---"));
        assert!(!preview.contains("id:"));
        assert!(!preview.contains("status:"));

        // Should contain clean markdown
        assert!(preview.contains("# Preview Test"));
        assert!(preview.contains("## Overview"));
        assert!(preview.contains("Testing the preview output"));
        assert!(preview.contains("## Tasks"));
        assert!(preview.contains("[x] **1.** Task one")); // Completed
        assert!(preview.contains("[ ] **2.** Task two")); // Pending
        assert!(preview.contains("2.1 Subtask 2.1"));
        assert!(preview.contains("## Acceptance Criteria"));
        assert!(preview.contains("Works correctly"));
    }

    #[test]
    fn test_execution_plan_backwards_compat() {
        // Test that old-style plans (with original_prompt, no Overview section) still work
        let old_md = r#"---
id: old_plan_123
title: "Old Style Plan"
status: draft
created: 2025-01-01T00:00:00Z
updated: 2025-01-01T00:00:00Z
current_task: 0
current_subtask: 0
---

# Old Style Plan

This is the original prompt text without a section header.

## Tasks

- [ ] Do something
- [ ] Do something else
"#;

        let plan = ExecutionPlan::from_markdown(old_md).unwrap();

        assert_eq!(plan.title, "Old Style Plan");
        assert_eq!(
            plan.original_prompt,
            "This is the original prompt text without a section header."
        );
        assert!(plan.overview.is_empty()); // No ## Overview section
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].description, "Do something");
    }
}
