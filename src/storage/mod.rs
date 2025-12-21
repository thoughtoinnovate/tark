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
//! └── plugins/                       # Project-specific plugins
//!     └── {plugin}/

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

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
        
        Ok(Self { project_root, global })
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
            let project_config: WorkspaceConfig = toml::from_str(&content)
                .context("Failed to parse project config.toml")?;
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
        let path = self.project_root.join("conversations").join(format!("{}.json", id));
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
        let path = self.project_root.join("conversations").join(format!("{}.json", id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
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
                    let name = path.file_stem()
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
            rules_map.insert(name.clone(), RuleInfo {
                name,
                scope: ConfigScope::Global,
            });
        }
        
        // Project rules (override)
        for name in list_rule_names(&self.project_root.join("rules"))? {
            rules_map.insert(name.clone(), RuleInfo {
                name,
                scope: ConfigScope::Project,
            });
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
            let project_config: McpConfig = toml::from_str(&content)
                .context("Failed to parse project mcp/servers.toml")?;
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
        let project_path = self.project_root.join("plugins").join(name).join("plugin.toml");
        if project_path.exists() {
            let content = std::fs::read_to_string(&project_path)?;
            return toml::from_str(&content).context("Failed to parse plugin.toml");
        }
        
        // Fall back to global
        let global_path = self.global.root().join("plugins").join(name).join("plugin.toml");
        let content = std::fs::read_to_string(&global_path)?;
        toml::from_str(&content).context("Failed to parse plugin.toml")
    }
    
    // ========== Agents (merged: global + project) ==========
    
    /// List all available agents (global + project)
    pub fn list_agents(&self) -> Result<Vec<AgentInfo>> {
        let mut agents_map: HashMap<String, AgentInfo> = HashMap::new();
        
        // Global agents
        for agent in load_agents_from_dir(&self.global.root().join("agents"), ConfigScope::Global)? {
            agents_map.insert(agent.id.clone(), agent);
        }
        
        // Project agents (override)
        for agent in load_agents_from_dir(&self.project_root.join("agents"), ConfigScope::Project)? {
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
                let name = path.file_stem()
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
                    let name = path.file_name()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    
                    // Try to load plugin config for details
                    let (enabled, description) = if let Ok(content) = std::fs::read_to_string(&plugin_toml) {
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
                let id = path.file_stem()
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
        
        assert!(storage.root().exists());
        assert!(storage.root().join("conversations").exists());
        assert!(storage.root().join("plans").exists());
        assert!(storage.root().join("rules").exists());
    }
    
    #[test]
    fn test_config_roundtrip() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();
        
        let mut config = WorkspaceConfig::default();
        config.provider = "claude".to_string();
        config.verbose = true;
        
        storage.save_config(&config).unwrap();
        let loaded = storage.load_config().unwrap();
        
        assert_eq!(loaded.provider, "claude");
        assert!(loaded.verbose);
    }
}

