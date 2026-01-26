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
    /// Messaging channel plugin (Slack, Discord, Signal, etc.)
    Channel,
    /// Hook plugin (lifecycle events)
    Hook,
}

impl std::fmt::Display for PluginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginType::Auth => write!(f, "auth"),
            PluginType::Tool => write!(f, "tool"),
            PluginType::Provider => write!(f, "provider"),
            PluginType::Channel => write!(f, "channel"),
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

    /// Allowed filesystem read paths (absolute, supports ~ for home)
    /// Used for reading system files like Gemini CLI installation
    #[serde(default)]
    pub fs_read: Vec<String>,
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

    /// Check if filesystem read access to a path is allowed
    pub fn is_fs_read_allowed(&self, requested_path: &str) -> bool {
        if self.fs_read.is_empty() {
            return false;
        }

        // Expand ~ in requested path
        let expanded_requested = expand_home_path(requested_path);

        // Try to canonicalize (resolve symlinks, normalize)
        let canonical_requested = match std::fs::canonicalize(&expanded_requested) {
            Ok(p) => p,
            Err(_) => {
                // Path doesn't exist yet, use the expanded path directly
                std::path::PathBuf::from(&expanded_requested)
            }
        };

        for allowed in &self.fs_read {
            let expanded_allowed = expand_home_path(allowed);

            // Check for glob pattern (contains *)
            if allowed.contains('*') {
                if glob_match_path(&expanded_allowed, &canonical_requested.to_string_lossy()) {
                    return true;
                }
                continue;
            }

            // Exact path match
            let canonical_allowed = match std::fs::canonicalize(&expanded_allowed) {
                Ok(p) => p,
                Err(_) => std::path::PathBuf::from(&expanded_allowed),
            };

            if canonical_requested == canonical_allowed {
                return true;
            }

            // Also check if requested path starts with allowed directory
            if canonical_requested.starts_with(&canonical_allowed) {
                return true;
            }
        }

        false
    }
}

/// Expand ~ to home directory in a path
fn expand_home_path(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped).to_string_lossy().to_string();
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().to_string();
        }
    }
    path.to_string()
}

fn expand_env_placeholders(value: &str) -> Result<String> {
    if !value.contains("${") {
        return Ok(value.to_string());
    }
    let re = regex::Regex::new(r"\$\{([A-Za-z0-9_]+)\}")?;
    let mut result = String::with_capacity(value.len());
    let mut last = 0;
    for caps in re.captures_iter(value) {
        let mat = caps.get(0).unwrap();
        let var = caps.get(1).unwrap().as_str();
        result.push_str(&value[last..mat.start()]);
        if let Ok(env_value) = std::env::var(var) {
            result.push_str(&env_value);
        } else {
            result.push_str(mat.as_str());
        }
        last = mat.end();
    }
    result.push_str(&value[last..]);
    Ok(result)
}

/// Simple glob matching for paths (supports * and **)
fn glob_match_path(pattern: &str, path: &str) -> bool {
    // Convert glob pattern to regex
    let regex_pattern = pattern
        .replace('.', r"\.")
        .replace("**", "<<<DOUBLESTAR>>>")
        .replace('*', "[^/]*")
        .replace("<<<DOUBLESTAR>>>", ".*");

    if let Ok(re) = regex::Regex::new(&format!("^{}$", regex_pattern)) {
        re.is_match(path)
    } else {
        false
    }
}

/// Plugin contributions (VS Code-style)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginContributions {
    /// LLM providers contributed
    #[serde(default)]
    pub providers: Vec<ProviderContribution>,

    /// Messaging channels contributed
    #[serde(default)]
    pub channels: Vec<ChannelContribution>,

    /// Commands contributed
    #[serde(default)]
    pub commands: Vec<CommandContribution>,

    /// Configuration schema
    #[serde(default)]
    pub configuration: Vec<ConfigContribution>,
}

/// Provider contribution declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Base provider for models.dev lookup (e.g., "google", "openai", "anthropic")
    /// When set, tark loads models from models.dev using this provider key
    #[serde(default)]
    pub base_provider: Option<String>,
}

/// Channel contribution declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelContribution {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

/// Command contribution declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub category: Option<String>,
}

/// Configuration contribution declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigContribution {
    pub key: String,
    #[serde(rename = "type")]
    pub value_type: String,
    #[serde(default)]
    pub default: Option<toml::Value>,
    #[serde(default)]
    pub description: String,
}

/// Plugin activation events
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginActivation {
    /// Events that trigger plugin activation
    /// Supported: "onStartup", "onProvider:<id>", "onCommand:<id>"
    #[serde(default)]
    pub events: Vec<String>,
}

/// OAuth2 flow type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OAuthFlowType {
    /// Authorization Code with PKCE (Proof Key for Code Exchange)
    Pkce,
    /// Device Flow (for devices without browsers)
    DeviceFlow,
}

/// OAuth2 authentication configuration for plugins
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// OAuth flow type
    pub flow: OAuthFlowType,
    /// Authorization endpoint URL
    pub auth_url: String,
    /// Token endpoint URL
    pub token_url: String,
    /// OAuth client ID
    pub client_id: String,
    /// Optional client secret (for confidential clients, not recommended for CLI tools)
    #[serde(default)]
    pub client_secret: Option<String>,
    /// OAuth scopes to request
    pub scopes: Vec<String>,
    /// Redirect URI for PKCE flow (e.g., "http://localhost:8888/callback")
    pub redirect_uri: String,
    /// Where to save credentials (supports ~ expansion, e.g., "~/.config/tark/provider_oauth.json")
    #[serde(default)]
    pub credentials_path: Option<String>,
    /// Optional: WASM function name to call for token post-processing
    /// If specified, the plugin's exported function will be called with tokens JSON
    /// and can return modified tokens (e.g., extract account_id, add metadata)
    #[serde(default)]
    pub process_tokens_callback: Option<String>,
    /// Extra parameters to include in authorization URL (provider-specific)
    /// Example: { originator = "opencode", id_token_add_organizations = "true" }
    #[serde(default)]
    pub extra_params: std::collections::HashMap<String, String>,
    /// Whether to include a state parameter for CSRF protection (default: true)
    #[serde(default = "default_use_state")]
    pub use_state: bool,
}

fn default_use_state() -> bool {
    true
}

/// Plugin manifest (plugin.toml)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin metadata
    pub plugin: PluginMetadata,

    /// Required capabilities
    #[serde(default)]
    pub capabilities: PluginCapabilities,

    /// Plugin contributions (what the plugin adds)
    #[serde(default)]
    pub contributes: PluginContributions,

    /// Activation events (when to load)
    #[serde(default)]
    pub activation: PluginActivation,

    /// OAuth configuration (if plugin needs authentication via OAuth)
    #[serde(default)]
    pub oauth: Option<OAuthConfig>,

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

    /// Required tark plugin API version (semver range)
    #[serde(default = "default_api_version")]
    pub api_version: String,

    /// WASM module filename (default: plugin.wasm)
    #[serde(default = "default_wasm_file")]
    pub wasm: String,
}

fn default_api_version() -> String {
    "0.1".to_string()
}

fn default_wasm_file() -> String {
    "plugin.wasm".to_string()
}

impl PluginManifest {
    /// Load manifest from a plugin.toml file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read manifest: {}", path.display()))?;

        let mut manifest: PluginManifest = toml::from_str(&content)
            .with_context(|| format!("Failed to parse manifest: {}", path.display()))?;

        manifest.expand_env()?;
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

    fn expand_env(&mut self) -> Result<()> {
        if let Some(oauth) = &mut self.oauth {
            oauth.auth_url = expand_env_placeholders(&oauth.auth_url)?;
            oauth.token_url = expand_env_placeholders(&oauth.token_url)?;
            oauth.client_id = expand_env_placeholders(&oauth.client_id)?;
            if let Some(secret) = oauth.client_secret.as_mut() {
                *secret = expand_env_placeholders(secret)?;
            }
            oauth.scopes = oauth
                .scopes
                .iter()
                .map(|scope| expand_env_placeholders(scope))
                .collect::<Result<Vec<_>>>()?;
            oauth.redirect_uri = expand_env_placeholders(&oauth.redirect_uri)?;
            if let Some(path) = oauth.credentials_path.as_mut() {
                *path = expand_env_placeholders(path)?;
            }
            if let Some(callback) = oauth.process_tokens_callback.as_mut() {
                *callback = expand_env_placeholders(callback)?;
            }
            for value in oauth.extra_params.values_mut() {
                *value = expand_env_placeholders(value)?;
            }
        }
        Ok(())
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
    fn test_parse_channel_manifest() {
        let toml = r#"
[plugin]
name = "test-channel"
version = "0.1.0"
type = "channel"

[contributes]
channels = [{ id = "slack", name = "Slack" }]
"#;

        let manifest: PluginManifest = toml::from_str(toml).unwrap();
        assert_eq!(manifest.plugin.name, "test-channel");
        assert_eq!(manifest.plugin.plugin_type, PluginType::Channel);
        assert_eq!(manifest.contributes.channels.len(), 1);
        assert_eq!(manifest.contributes.channels[0].id, "slack");
    }

    #[test]
    fn test_manifest_env_expansion() {
        std::env::set_var("TEST_CLIENT_ID", "client-123");
        std::env::set_var("TEST_REDIRECT", "http://localhost/callback");

        let toml = r#"
[plugin]
name = "test-channel"
version = "0.1.0"
type = "channel"

[oauth]
flow = "pkce"
auth_url = "https://example.com/auth"
token_url = "https://example.com/token"
client_id = "${TEST_CLIENT_ID}"
scopes = ["bot"]
redirect_uri = "${TEST_REDIRECT}"
"#;

        let path = std::env::temp_dir().join("tark-plugin-manifest-test.toml");
        std::fs::write(&path, toml).unwrap();

        let manifest = PluginManifest::load(&path).unwrap();
        let oauth = manifest.oauth.unwrap();
        assert_eq!(oauth.client_id, "client-123");
        assert_eq!(oauth.redirect_uri, "http://localhost/callback");

        let _ = std::fs::remove_file(path);
        std::env::remove_var("TEST_CLIENT_ID");
        std::env::remove_var("TEST_REDIRECT");
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

    #[test]
    fn test_expand_home_path() {
        let home = dirs::home_dir().unwrap();
        let home_str = home.to_string_lossy();

        assert_eq!(
            expand_home_path("~/.gemini/oauth_creds.json"),
            format!("{}/.gemini/oauth_creds.json", home_str)
        );
        assert_eq!(expand_home_path("/absolute/path"), "/absolute/path");
        assert_eq!(expand_home_path("relative/path"), "relative/path");
    }

    #[test]
    fn test_fs_read_empty() {
        let caps = PluginCapabilities::default();
        assert!(!caps.is_fs_read_allowed("/any/path"));
    }

    #[test]
    fn test_glob_match_path() {
        // Single star matches within directory
        assert!(glob_match_path("/usr/*/file.txt", "/usr/local/file.txt"));
        assert!(!glob_match_path(
            "/usr/*/file.txt",
            "/usr/local/bin/file.txt"
        ));

        // Double star matches across directories
        assert!(glob_match_path(
            "/usr/**/file.txt",
            "/usr/local/bin/file.txt"
        ));
        assert!(glob_match_path("/usr/**/*.js", "/usr/local/lib/test.js"));
    }
}
