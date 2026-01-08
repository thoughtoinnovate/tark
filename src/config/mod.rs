//! Configuration management for tark

#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub completion: CompletionConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
    pub thinking: ThinkingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub default_provider: String,
    pub claude: ClaudeConfig,
    pub openai: OpenAiConfig,
    pub ollama: OllamaConfig,
    pub copilot: CopilotConfig,
    pub gemini: GeminiConfig,
    pub openrouter: OpenRouterConfig,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_provider: "openai".to_string(),
            claude: ClaudeConfig::default(),
            openai: OpenAiConfig::default(),
            ollama: OllamaConfig::default(),
            copilot: CopilotConfig::default(),
            gemini: GeminiConfig::default(),
            openrouter: OpenRouterConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeConfig {
    pub model: String,
    pub max_tokens: usize,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
    pub model: String,
    pub max_tokens: usize,
}

impl Default for OpenAiConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OllamaConfig {
    pub base_url: String,
    pub model: String,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434".to_string(),
            model: "codellama".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CopilotConfig {
    pub model: String,
    pub max_tokens: usize,
    /// Timeout in seconds for Device Flow authentication (default: 1800 = 30 minutes)
    pub auth_timeout_secs: u64,
}

impl Default for CopilotConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o".to_string(),
            max_tokens: 4096,
            auth_timeout_secs: 1800, // 30 minutes
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeminiConfig {
    pub model: String,
    pub max_tokens: usize,
}

impl Default for GeminiConfig {
    fn default() -> Self {
        Self {
            model: "gemini-2.0-flash-exp".to_string(),
            max_tokens: 8192,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OpenRouterConfig {
    pub model: String,
    pub max_tokens: usize,
    pub site_url: Option<String>,
    pub app_name: Option<String>,
}

impl Default for OpenRouterConfig {
    fn default() -> Self {
        Self {
            model: "anthropic/claude-sonnet-4".to_string(),
            max_tokens: 4096,
            site_url: None,
            app_name: Some("Tark".to_string()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 8765,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CompletionConfig {
    pub enabled: bool,
    pub debounce_ms: u64,
    pub max_completion_tokens: usize,
    pub cache_size: usize,
    pub context_lines_before: usize,
    pub context_lines_after: usize,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 150,
            max_completion_tokens: 256,
            cache_size: 100,
            context_lines_before: 50,
            context_lines_after: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub max_iterations: usize,
    pub working_directory: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 25,
            working_directory: ".".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolsConfig {
    pub shell_enabled: bool,
    pub allowed_paths: Vec<String>,
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            allowed_paths: vec![".".to_string()],
        }
    }
}

/// Configuration for thinking/reasoning features
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThinkingConfig {
    /// Default level to use (must match a key in `levels`)
    pub default_level: String,
    /// Maximum visible lines for thinking block in UI
    pub max_visible_lines: usize,
    /// Automatically collapse thinking block after response complete
    pub auto_collapse: bool,
    /// Configurable thinking levels (e.g., "low", "medium", "high", "ultra")
    #[serde(default = "ThinkingConfig::default_levels")]
    pub levels: HashMap<String, ThinkLevel>,
}

impl ThinkingConfig {
    /// Get default thinking levels
    fn default_levels() -> HashMap<String, ThinkLevel> {
        let mut levels = HashMap::new();
        levels.insert(
            "low".to_string(),
            ThinkLevel {
                description: "Quick reasoning, fast responses".to_string(),
                budget_tokens: 2_000,
                reasoning_effort: "low".to_string(),
            },
        );
        levels.insert(
            "medium".to_string(),
            ThinkLevel {
                description: "Balanced reasoning".to_string(),
                budget_tokens: 10_000,
                reasoning_effort: "medium".to_string(),
            },
        );
        levels.insert(
            "high".to_string(),
            ThinkLevel {
                description: "Deep reasoning for complex tasks".to_string(),
                budget_tokens: 50_000,
                reasoning_effort: "high".to_string(),
            },
        );
        levels
    }

    /// Get a level by name (case-insensitive)
    pub fn get_level(&self, name: &str) -> Option<&ThinkLevel> {
        self.levels.get(&name.to_lowercase())
    }

    /// Get all level names sorted alphabetically
    pub fn level_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.levels.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Get levels with descriptions for intellisense
    pub fn levels_for_intellisense(&self) -> Vec<(String, String)> {
        let mut items: Vec<(String, String)> = self
            .levels
            .iter()
            .map(|(name, level)| (name.clone(), level.description.clone()))
            .collect();
        items.sort_by(|a, b| a.0.cmp(&b.0));
        // Add "off" as first option
        items.insert(0, ("off".to_string(), "Disable thinking".to_string()));
        items
    }
}

impl Default for ThinkingConfig {
    fn default() -> Self {
        Self {
            default_level: "off".to_string(), // Off by default (cost protection)
            max_visible_lines: 6,
            auto_collapse: false,
            levels: Self::default_levels(),
        }
    }
}

/// A configurable thinking level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkLevel {
    /// Human-readable description for intellisense
    pub description: String,
    /// Token budget for Claude/Gemini (thinking tokens)
    pub budget_tokens: u32,
    /// Reasoning effort for OpenAI o-series: "low", "medium", "high"
    pub reasoning_effort: String,
}

impl Config {
    /// Load configuration from default location or create default
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
            let config_dir = proj_dirs.config_dir();
            std::fs::create_dir_all(config_dir)?;
            Ok(config_dir.join("config.toml"))
        } else {
            Ok(PathBuf::from("config.toml"))
        }
    }

    /// Save configuration to default location
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(config_path, content)?;
        Ok(())
    }
}
