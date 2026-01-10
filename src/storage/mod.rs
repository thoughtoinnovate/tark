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
        std::fs::create_dir_all(project_root.join("sessions").join("plans"))?;

        Ok(Self {
            project_root,
            global,
        })
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

    /// Get current session ID file path
    fn current_session_file(&self) -> PathBuf {
        self.sessions_dir().join("current")
    }

    /// Save a chat session
    pub fn save_session(&self, session: &ChatSession) -> Result<PathBuf> {
        let path = self.sessions_dir().join(format!("{}.json", session.id));
        let content = serde_json::to_string_pretty(session)?;
        std::fs::write(&path, content)?;

        // Update current session pointer
        std::fs::write(self.current_session_file(), &session.id)?;

        Ok(path)
    }

    /// Load a chat session by ID
    pub fn load_session(&self, id: &str) -> Result<ChatSession> {
        let path = self.sessions_dir().join(format!("{}.json", id));
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse session")
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
                if path.extension().map(|e| e == "json").unwrap_or(false) {
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
        let path = self.sessions_dir().join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        // If this was the current session, clear the pointer
        if self.get_current_session_id().as_deref() == Some(id) {
            let _ = std::fs::remove_file(self.current_session_file());
        }

        Ok(())
    }

    // ========== Execution Plans ==========

    /// Get execution plans directory
    fn execution_plans_dir(&self) -> PathBuf {
        self.project_root.join("sessions").join("plans")
    }

    /// Get current plan file path
    fn current_plan_file(&self) -> PathBuf {
        self.execution_plans_dir().join("current")
    }

    /// Save an execution plan
    pub fn save_execution_plan(&self, plan: &ExecutionPlan) -> Result<PathBuf> {
        let path = self.execution_plans_dir().join(format!("{}.json", plan.id));
        let content = serde_json::to_string_pretty(plan)?;
        std::fs::write(&path, content)?;

        // Update current plan pointer if this is an active plan
        if plan.status == PlanStatus::Active || plan.status == PlanStatus::Draft {
            std::fs::write(self.current_plan_file(), &plan.id)?;
        }

        Ok(path)
    }

    /// Load an execution plan by ID
    pub fn load_execution_plan(&self, id: &str) -> Result<ExecutionPlan> {
        let path = self.execution_plans_dir().join(format!("{}.json", id));
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse execution plan")
    }

    /// Get the current plan ID
    pub fn get_current_plan_id(&self) -> Option<String> {
        let path = self.current_plan_file();
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// Set the current plan ID
    pub fn set_current_plan(&self, id: &str) -> Result<()> {
        std::fs::write(self.current_plan_file(), id)?;
        Ok(())
    }

    /// Clear current plan pointer
    pub fn clear_current_plan(&self) -> Result<()> {
        let path = self.current_plan_file();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Load the current execution plan
    pub fn load_current_execution_plan(&self) -> Result<ExecutionPlan> {
        if let Some(id) = self.get_current_plan_id() {
            self.load_execution_plan(&id)
        } else {
            Err(anyhow::anyhow!("No active plan"))
        }
    }

    /// List all execution plans
    pub fn list_execution_plans(&self) -> Result<Vec<PlanMeta>> {
        let dir = self.execution_plans_dir();
        let mut plans = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "json").unwrap_or(false) {
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
    pub fn delete_execution_plan(&self, id: &str) -> Result<()> {
        let path = self.execution_plans_dir().join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }

        // If this was the current plan, clear the pointer
        if self.get_current_plan_id().as_deref() == Some(id) {
            let _ = std::fs::remove_file(self.current_plan_file());
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
    /// Moves the plan from sessions/plans/ to archives/plans/,
    /// then prunes old archives to keep only the 5 most recent.
    pub fn archive_plan(&self, id: &str) -> Result<PathBuf> {
        let archive_dir = self.archives_plans_dir();
        std::fs::create_dir_all(&archive_dir)?;

        // Load the plan
        let mut plan = self.load_execution_plan(id)?;
        plan.status = PlanStatus::Completed;

        // Save to archive location
        let archive_path = archive_dir.join(format!("{}.json", plan.id));
        let content = serde_json::to_string_pretty(&plan)?;
        std::fs::write(&archive_path, content)?;

        // Delete from active plans
        self.delete_execution_plan(id)?;

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
                e.path()
                    .extension()
                    .map(|ext| ext == "json")
                    .unwrap_or(false)
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
                if path.extension().map(|e| e == "json").unwrap_or(false) {
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
        let path = self.archives_plans_dir().join(format!("{}.json", id));
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).context("Failed to parse archived plan")
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
            provider: "openai".to_string(),
            model: None,
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
        if other.provider != "openai" {
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
        });
        self.updated_at = Utc::now();
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

/// A message in a chat session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
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

/// A task in an execution plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanTask {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub subtasks: Vec<PlanSubtask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
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
    /// Original prompt that generated this plan
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
}

/// A refinement/modification to the plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanRefinement {
    pub timestamp: DateTime<Utc>,
    pub description: String,
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

    /// Format plan as markdown checklist
    pub fn to_markdown(&self) -> String {
        let mut md = String::new();

        md.push_str(&format!("# {}\n\n", self.title));
        md.push_str(&format!("**Status:** {:?}\n", self.status));

        let (done, total) = self.progress();
        md.push_str(&format!(
            "**Progress:** {}/{} ({:.0}%)\n\n",
            done,
            total,
            if total > 0 {
                (done as f64 / total as f64) * 100.0
            } else {
                0.0
            }
        ));

        md.push_str("## Tasks\n\n");

        for (i, task) in self.tasks.iter().enumerate() {
            let checkbox = match task.status {
                TaskStatus::Completed | TaskStatus::Skipped => "[x]",
                _ => "[ ]",
            };
            md.push_str(&format!("{}. {} {}\n", i + 1, checkbox, task.description));

            for (j, subtask) in task.subtasks.iter().enumerate() {
                let sub_checkbox = match subtask.status {
                    TaskStatus::Completed | TaskStatus::Skipped => "[x]",
                    _ => "[ ]",
                };
                md.push_str(&format!(
                    "   {}.{}. {} {}\n",
                    i + 1,
                    j + 1,
                    sub_checkbox,
                    subtask.description
                ));
            }
        }

        if !self.refinements.is_empty() {
            md.push_str("\n## Refinements\n\n");
            for r in &self.refinements {
                md.push_str(&format!(
                    "- {} ({})\n",
                    r.description,
                    r.timestamp.format("%Y-%m-%d %H:%M")
                ));
            }
        }

        md
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
}
