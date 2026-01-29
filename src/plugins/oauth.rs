//! OAuth helper for plugins (provider/channel)

use crate::auth::OAuthHandler;
use crate::plugins::{InstalledPlugin, OAuthConfig, PluginHost};
use anyhow::{Context, Result};
use std::path::PathBuf;

pub struct PluginOAuthResult {
    pub creds_path: PathBuf,
    pub tokens_json: String,
}

pub async fn run_oauth_flow_for_plugin(
    plugin: &InstalledPlugin,
    oauth_config: &OAuthConfig,
) -> Result<PluginOAuthResult> {
    if oauth_config.flow != crate::plugins::OAuthFlowType::Pkce {
        anyhow::bail!("Only PKCE OAuth flow is supported for plugins right now");
    }

    ensure_oauth_env_resolved(oauth_config)?;

    let handler = OAuthHandler::new(oauth_config.clone());
    let tokens = handler.execute_pkce_flow().await?;

    let tokens_json = if oauth_config.process_tokens_callback.is_some() {
        let mut plugin_host = PluginHost::new()?;
        plugin_host.load(plugin)?;

        let tokens_json = serde_json::to_string(&tokens)?;
        if let Some(instance) = plugin_host.get_mut(plugin.id()) {
            if let Some(processed_json) = instance.auth_process_tokens(&tokens_json)? {
                processed_json
            } else {
                tokens_json
            }
        } else {
            tokens_json
        }
    } else {
        let mut token_value = serde_json::to_value(&tokens)?;
        if let Some(expires_in) = tokens.expires_in {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            token_value["expires_at"] = serde_json::json!(now + expires_in);
        }
        serde_json::to_string_pretty(&token_value)?
    };

    let creds_path = if let Some(path_str) = &oauth_config.credentials_path {
        expand_path(path_str)
    } else {
        default_credentials_path(plugin.id())?
    };

    if let Some(parent) = creds_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(&creds_path, &tokens_json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&creds_path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&creds_path, perms)?;
    }

    Ok(PluginOAuthResult {
        creds_path,
        tokens_json,
    })
}

fn ensure_oauth_env_resolved(config: &OAuthConfig) -> Result<()> {
    let mut unresolved = Vec::new();
    if contains_env_placeholder(&config.auth_url) {
        unresolved.push("auth_url");
    }
    if contains_env_placeholder(&config.token_url) {
        unresolved.push("token_url");
    }
    if contains_env_placeholder(&config.client_id) {
        unresolved.push("client_id");
    }
    if config
        .client_secret
        .as_deref()
        .map(contains_env_placeholder)
        .unwrap_or(false)
    {
        unresolved.push("client_secret");
    }
    if config
        .scopes
        .iter()
        .any(|scope| contains_env_placeholder(scope))
    {
        unresolved.push("scopes");
    }
    if contains_env_placeholder(&config.redirect_uri) {
        unresolved.push("redirect_uri");
    }
    if config
        .credentials_path
        .as_deref()
        .map(contains_env_placeholder)
        .unwrap_or(false)
    {
        unresolved.push("credentials_path");
    }
    if config
        .process_tokens_callback
        .as_deref()
        .map(contains_env_placeholder)
        .unwrap_or(false)
    {
        unresolved.push("process_tokens_callback");
    }
    if config
        .extra_params
        .values()
        .any(|value| contains_env_placeholder(value))
    {
        unresolved.push("extra_params");
    }

    if unresolved.is_empty() {
        return Ok(());
    }

    anyhow::bail!(
        "OAuth config has unresolved environment placeholders: {}",
        unresolved.join(", ")
    );
}

fn contains_env_placeholder(value: &str) -> bool {
    let mut chars = value.chars();
    while let Some(ch) = chars.next() {
        if ch == '$' {
            if let Some('{') = chars.next() {
                return true;
            }
        }
    }
    false
}

fn default_credentials_path(plugin_id: &str) -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config_dir
        .join("tark")
        .join(format!("{}_oauth.json", plugin_id)))
}

fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}
