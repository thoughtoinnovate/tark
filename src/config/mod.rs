//! Configuration management for tark

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Main configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub llm: LlmConfig,
    pub server: ServerConfig,
    pub completion: CompletionConfig,
    pub agent: AgentConfig,
    pub tools: ToolsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            server: ServerConfig::default(),
            completion: CompletionConfig::default(),
            agent: AgentConfig::default(),
            tools: ToolsConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub default_provider: String,
    pub claude: ClaudeConfig,
    pub openai: OpenAiConfig,
    pub ollama: OllamaConfig,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            default_provider: "openai".to_string(),
            claude: ClaudeConfig::default(),
            openai: OpenAiConfig::default(),
            ollama: OllamaConfig::default(),
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
            max_iterations: 10,
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
