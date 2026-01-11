//! Plugin manifest parsing and validation
//!
//! Each plugin must have a `plugin.toml` manifest declaring:
//! - Plugin metadata (name, version, author)
//! - Plugin type (auth, tool, provider)
//! - Required capabilities (storage, http, env, etc.)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Plugin type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    /// Authentication plugin (OAuth, API keys)
    Auth,
    /// Tool plugin (adds agent capabilities)
    Tool,
    /// LLM provider plugin
    Provider,
    /// Hook plugin (lifecycle events)
    Hook,
}

impl std::fmt::Display for PluginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginType::Auth => write!(f, "auth"),
            PluginType::Tool => write!(f, "tool"),
            PluginType::Provider => write!(f, "provider"),
            PluginType::Hook => write!(f, "hook"),
        }
    }
}

/// Plugin capabilities - what the plugin is allowed to do
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginCapabilities {
    /// Can use persistent storage
    #[serde(default)]
    pub storage: bool,

    /// Allowed HTTP hosts (empty = no network access)
    #[serde(default)]
    pub http: Vec<String>,

    /// Allowed environment variables to read
    #[serde(default)]
    pub env: Vec<String>,

    /// Can execute shell commands (dangerous!)
    #[serde(default)]
    pub shell: bool,

    /// Allowed filesystem paths (relative to workspace)
    #[serde(default)]
    pub filesystem: Vec<String>,
}

impl PluginCapabilities {
    /// Check if HTTP access to a host is allowed
    pub fn is_http_allowed(&self, host: &str) -> bool {
        self.http.iter().any(|allowed| {
            // Support wildcards like "*.googleapis.com"
            if allowed.starts_with("*.") {
                let suffix = &allowed[1..]; // ".googleapis.com"
                host.ends_with(suffix) || host == &allowed[2..]
            } else {
                host == allowed
            }
        })
    }

    /// Check if environment variable access is allowed
    pub fn is_env_allowed(&self, var: &str) -> bool {
        self.env.iter().any(|allowed| {
            // Support wildcards like "GEMINI_*"
            if allowed.ends_with('*') {
                var.starts_with(&allowed[..allowed.len() - 1])
            } else {
                var == allowed
            }
        })
    }
}

/// Plugin manifest (plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin metadata
    pub plugin: PluginMetadata,

    /// Required capabilities
    #[serde(default)]
    pub capabilities: PluginCapabilities,

    /// Plugin-specific configuration schema (optional)
    #[serde(default)]
    pub config: Option<toml::Value>,
}

/// Plugin metadata section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Unique plugin identifier (e.g., "gemini-auth")
    pub name: String,

    /// Plugin version (semver)
    pub version: String,

    /// Plugin type
    #[serde(rename = "type")]
    pub plugin_type: PluginType,

    /// Human-readable description
    #[serde(default)]
    pub description: String,

    /// Plugin author
    #[serde(default)]
    pub author: String,

    /// Project homepage/repository
    #[serde(default)]
    pub homepage: String,

    /// License identifier
    #[serde(default)]
    pub license: String,

    /// Minimum tark version required
    #[serde(default)]
    pub min_tark_version: Option<String>,

    /// WASM module filename (default: plugin.wasm)
    #[serde(default = "default_wasm_file")]
    pub wasm: String,
}

fn default_wasm_file() -> String {
    "plugin.wasm".to_string()
}

impl PluginManifest {
    /// Load manifest from a plugin.toml file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;

        let manifest: PluginManifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest: {}", path.display()))?;

        manifest.validate()?;

        Ok(manifest)
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<()> {
        // Check required fields
        if self.plugin.name.is_empty() {
            anyhow::bail!("Plugin name is required");
        }

        if self.plugin.version.is_empty() {
            anyhow::bail!("Plugin version is required");
        }

        // Validate version is semver
        if semver::Version::parse(&self.plugin.version).is_err() {
            anyhow::bail!(
                "Plugin version must be valid semver: {}",
                self.plugin.version
            );
        }

        // Validate plugin name (alphanumeric + hyphens)
        if !self
            .plugin
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            anyhow::bail!("Plugin name must be alphanumeric with hyphens/underscores");
        }

        // Warn about dangerous capabilities
        if self.capabilities.shell {
            tracing::warn!(
                "Plugin '{}' requests shell access - this is dangerous!",
                self.plugin.name
            );
        }

        Ok(())
    }

    /// Get the plugin ID (name)
    pub fn id(&self) -> &str {
        &self.plugin.name
    }

    /// Get the plugin type
    pub fn plugin_type(&self) -> PluginType {
        self.plugin.plugin_type
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_manifest() {
        let toml = r#"
[plugin]
name = "test-auth"
version = "1.0.0"
type = "auth"
description = "Test authentication plugin"

[capabilities]
storage = true
http = ["oauth2.googleapis.com", "*.google.com"]
"#;

        let manifest: PluginManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.plugin.name, "test-auth");
        assert_eq!(manifest.plugin.plugin_type, PluginType::Auth);
        assert!(manifest.capabilities.storage);
        assert!(manifest
            .capabilities
            .is_http_allowed("oauth2.googleapis.com"));
        assert!(manifest.capabilities.is_http_allowed("accounts.google.com"));
        assert!(!manifest.capabilities.is_http_allowed("evil.com"));
    }

    #[test]
    fn test_http_wildcards() {
        let caps = PluginCapabilities {
            http: vec!["*.googleapis.com".to_string()],
            ..Default::default()
        };

        assert!(caps.is_http_allowed("oauth2.googleapis.com"));
        assert!(caps.is_http_allowed("generativelanguage.googleapis.com"));
        assert!(!caps.is_http_allowed("googleapis.com.evil.com"));
    }

    #[test]
    fn test_env_wildcards() {
        let caps = PluginCapabilities {
            env: vec!["GEMINI_*".to_string(), "GOOGLE_API_KEY".to_string()],
            ..Default::default()
        };

        assert!(caps.is_env_allowed("GEMINI_API_KEY"));
        assert!(caps.is_env_allowed("GEMINI_PROJECT_ID"));
        assert!(caps.is_env_allowed("GOOGLE_API_KEY"));
        assert!(!caps.is_env_allowed("OPENAI_API_KEY"));
    }
}
