//! CLI commands for plugin management

use crate::plugins::{InstalledPlugin, PluginManifest, PluginRegistry};
use crate::secure_store;
use crate::storage::TarkStorage;
use anyhow::{Context, Result};
use chrono::Utc;
use colored::Colorize;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tabled::{settings::Style, Table, Tabled};

/// List installed plugins
pub async fn run_plugin_list() -> Result<()> {
    let registry = PluginRegistry::new()?;

    println!("{}", "=== Installed Plugins ===".bold().cyan());
    println!();

    #[derive(Tabled)]
    struct PluginRow {
        #[tabled(rename = "ID")]
        id: String,
        #[tabled(rename = "Version")]
        version: String,
        #[tabled(rename = "Type")]
        plugin_type: String,
        #[tabled(rename = "Status")]
        status: String,
        #[tabled(rename = "Description")]
        description: String,
    }

    let rows: Vec<PluginRow> = registry
        .all()
        .map(|p| PluginRow {
            id: p.id().to_string(),
            version: p.manifest.plugin.version.clone(),
            plugin_type: p.plugin_type().to_string(),
            status: if p.enabled {
                "✓ enabled".green().to_string()
            } else {
                "✗ disabled".red().to_string()
            },
            description: p
                .manifest
                .plugin
                .description
                .chars()
                .take(40)
                .collect::<String>(),
        })
        .collect();

    if rows.is_empty() {
        println!("No plugins installed.");
        println!();
        println!("Install a plugin with:");
        println!("  tark plugin add <git-url>");
    } else {
        let mut table = Table::new(rows);
        table.with(Style::rounded());
        println!("{}", table);
    }

    println!();
    println!("Plugins directory: {}", registry.plugins_dir().display());

    Ok(())
}

/// Show plugin details
pub async fn run_plugin_info(plugin_id: &str) -> Result<()> {
    let registry = PluginRegistry::new()?;

    let plugin = registry
        .get(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;

    println!("{}", format!("=== Plugin: {} ===", plugin_id).bold().cyan());
    println!();

    println!("{}:    {}", "Name".bold(), plugin.manifest.plugin.name);
    println!("{}:  {}", "Version".bold(), plugin.manifest.plugin.version);
    println!("{}:    {}", "Type".bold(), plugin.plugin_type());
    println!(
        "{}:  {}",
        "Status".bold(),
        if plugin.enabled {
            "enabled".green()
        } else {
            "disabled".red()
        }
    );

    if !plugin.manifest.plugin.description.is_empty() {
        println!(
            "{}: {}",
            "Description".bold(),
            plugin.manifest.plugin.description
        );
    }
    if !plugin.manifest.plugin.author.is_empty() {
        println!("{}:  {}", "Author".bold(), plugin.manifest.plugin.author);
    }
    if !plugin.manifest.plugin.homepage.is_empty() {
        println!("{}: {}", "Homepage".bold(), plugin.manifest.plugin.homepage);
    }
    if !plugin.manifest.plugin.license.is_empty() {
        println!("{}: {}", "License".bold(), plugin.manifest.plugin.license);
    }

    println!();
    println!("{}:", "Capabilities".bold());
    let caps = &plugin.manifest.capabilities;
    println!("  Storage: {}", if caps.storage { "✓" } else { "✗" });
    println!(
        "  HTTP:    {}",
        if caps.http.is_empty() {
            "✗".to_string()
        } else {
            caps.http.join(", ")
        }
    );
    println!(
        "  Env:     {}",
        if caps.env.is_empty() {
            "✗".to_string()
        } else {
            caps.env.join(", ")
        }
    );
    println!(
        "  Shell:   {}",
        if caps.shell {
            "⚠️ yes (dangerous!)".red().to_string()
        } else {
            "✗".to_string()
        }
    );

    println!();
    println!("{}:", "Paths".bold());
    println!("  Install: {}", plugin.path.display());
    println!("  WASM:    {}", plugin.wasm_path.display());
    println!("  Data:    {}", plugin.data_dir().display());

    Ok(())
}

/// Run OAuth authentication for a plugin
pub async fn run_plugin_auth(plugin_id: &str) -> Result<()> {
    let registry = PluginRegistry::new()?;
    let plugin = registry
        .get(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?
        .clone();

    let oauth_config = plugin
        .manifest
        .oauth
        .clone()
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' has no OAuth configuration", plugin_id))?;

    println!(
        "{}",
        format!("=== Plugin OAuth: {} ===", plugin_id).bold().cyan()
    );
    println!();

    let storage_scope = prompt_storage_scope()?;
    let mut oauth_config = oauth_config.clone();
    if let Some(local_root) = storage_scope.project_root.as_ref() {
        let creds_path = local_root
            .join("plugins")
            .join(plugin.id())
            .join("oauth.json");
        oauth_config.credentials_path = Some(creds_path.to_string_lossy().to_string());
    }

    let mut config_payload = Value::Null;
    let mut skip_oauth = false;
    if plugin.plugin_type() == crate::plugins::PluginType::Channel {
        config_payload = build_channel_config_payload(plugin.id())?;
        if plugin.id() == "discord" {
            if let Some(cfg) = config_payload.get("config").and_then(Value::as_object) {
                if let Some(app_id) = cfg.get("application_id").and_then(Value::as_str) {
                    oauth_config.client_id = app_id.to_string();
                }
            }
            let client_secret = prompt_optional("Discord Client Secret")?;
            oauth_config.client_secret = client_secret;
            skip_oauth = !prompt_yes_no_default("Run Discord OAuth flow", false)?;
        }
    }

    if skip_oauth {
        let creds_path = oauth_config
            .credentials_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| default_oauth_credentials_path(plugin.id()).unwrap());
        ensure_parent_dir(&creds_path)?;
        let payload = if config_payload.is_null() {
            "{}".to_string()
        } else {
            serde_json::to_string(&config_payload)?
        };
        std::fs::write(&creds_path, &payload)?;
        encrypt_credentials(&creds_path)?;
        let mut host = crate::plugins::PluginHost::new()?;
        if let Some(local_root) = storage_scope.project_root.as_ref() {
            let data_dir = local_root.join("plugins").join(plugin.id()).join("data");
            host.load_with_data_dir(&plugin, data_dir)?;
        } else {
            host.load(&plugin)?;
        }
        if let Some(instance) = host.get_mut(plugin.id()) {
            if instance.has_channel_auth_init() {
                if let Err(err) = instance.channel_auth_init(&payload) {
                    tracing::warn!("Failed to initialize channel auth: {}", err);
                }
            }
        }
        println!("✅ Saved credentials to {}", creds_path.display());
        return Ok(());
    }

    let result = crate::plugins::run_oauth_flow_for_plugin(&plugin, &oauth_config).await?;

    if plugin.plugin_type() == crate::plugins::PluginType::Channel {
        ensure_parent_dir(&result.creds_path)?;
        let merged_payload = merge_tokens_and_config(&result.tokens_json, config_payload)?;
        std::fs::write(&result.creds_path, &merged_payload)?;
        encrypt_credentials(&result.creds_path)?;
        let mut host = crate::plugins::PluginHost::new()?;
        if let Some(local_root) = storage_scope.project_root.as_ref() {
            let data_dir = local_root.join("plugins").join(plugin.id()).join("data");
            host.load_with_data_dir(&plugin, data_dir)?;
        } else {
            host.load(&plugin)?;
        }
        if let Some(instance) = host.get_mut(plugin.id()) {
            if instance.has_channel_auth_init() {
                if let Err(err) = instance.channel_auth_init(&merged_payload) {
                    tracing::warn!("Failed to initialize channel auth: {}", err);
                }
            }
        }
    }

    println!("✅ Saved credentials to {}", result.creds_path.display());
    Ok(())
}

/// Install a plugin from git repository or local path
///
/// # Arguments
/// * `url` - Git repository URL or local path
/// * `branch` - Branch or tag to clone (default: main)
/// * `subpath` - Optional subdirectory path within the repository (for monorepos)
pub async fn run_plugin_add(url: &str, branch: &str, subpath: Option<&str>) -> Result<()> {
    println!("{}", "=== Installing Plugin ===".bold().cyan());
    println!();

    // Check if it's a local path
    let is_local = url.starts_with('/')
        || url.starts_with("./")
        || url.starts_with("../")
        || url.starts_with('~')
        || std::path::Path::new(url).exists();

    let source_path: std::path::PathBuf;
    let _temp_dir: Option<tempfile::TempDir>;

    let source_meta;
    if is_local {
        // Local path - expand ~ and use directly
        let expanded = if url.starts_with('~') {
            dirs::home_dir()
                .map(|h| h.join(&url[2..]))
                .unwrap_or_else(|| std::path::PathBuf::from(url))
        } else {
            std::path::PathBuf::from(url)
        };

        // Apply subpath if provided
        let final_path = if let Some(sub) = subpath {
            expanded.join(sub)
        } else {
            expanded
        };

        println!("Source: {} (local)", final_path.display());
        println!();

        if !final_path.exists() {
            anyhow::bail!("Path does not exist: {}", final_path.display());
        }

        source_path = final_path.clone();
        _temp_dir = None;
        source_meta = PluginInstallSource::local(final_path);
    } else {
        // Git URL - clone to temp directory
        println!("Repository: {}", url);
        println!("Branch:     {}", branch);
        if let Some(sub) = subpath {
            println!("Path:       {}", sub);
        }
        println!();

        let temp = tempfile::tempdir()?;
        let clone_path = temp.path().join("plugin");

        println!("Cloning repository...");

        let status = std::process::Command::new("git")
            .args(["clone", "--depth", "1", "--branch", branch, url])
            .arg(&clone_path)
            .status()
            .context("Failed to run git clone")?;

        if !status.success() {
            anyhow::bail!("git clone failed");
        }

        // Apply subpath if provided
        let final_path = if let Some(sub) = subpath {
            clone_path.join(sub)
        } else {
            clone_path
        };

        if !final_path.exists() {
            anyhow::bail!(
                "Subdirectory '{}' not found in repository",
                subpath.unwrap_or("")
            );
        }

        source_path = final_path.clone();
        _temp_dir = Some(temp);
        source_meta = PluginInstallSource::git(url, branch, subpath.map(str::to_string));
    }

    // Verify plugin.toml exists
    let manifest_path = source_path.join("plugin.toml");
    if !manifest_path.exists() {
        anyhow::bail!(
            "No plugin.toml found in {}\nMake sure the path contains a valid plugin.",
            source_path.display()
        );
    }

    // Load manifest to show info
    let manifest = PluginManifest::load(&manifest_path)?;

    println!();
    println!(
        "Found plugin: {} v{}",
        manifest.plugin.name.green(),
        manifest.plugin.version
    );
    println!("Type: {}", manifest.plugin_type());
    println!("Capabilities:");
    let caps = &manifest.capabilities;
    if caps.storage {
        println!("  • Storage access");
    }
    if !caps.http.is_empty() {
        println!("  • HTTP to: {}", caps.http.join(", "));
    }
    if !caps.env.is_empty() {
        println!("  • Env vars: {}", caps.env.join(", "));
    }
    if caps.shell {
        println!("  • ⚠️  Shell access (dangerous!)");
    }

    // Verify WASM exists
    let wasm_path = source_path.join(&manifest.plugin.wasm);
    if !wasm_path.exists() {
        anyhow::bail!(
            "WASM module not found: {}\nDid you forget to build the plugin?",
            manifest.plugin.wasm
        );
    }

    println!();
    println!("Installing...");

    // Install via registry
    let mut registry = PluginRegistry::new()?;
    let plugin_id = registry.install(&source_path)?;
    write_install_source(&registry, &plugin_id, &source_meta)?;

    println!();
    println!("✅ Successfully installed plugin: {}", plugin_id.green());
    println!();
    println!("The plugin will be loaded on next tark start.");

    Ok(())
}

/// Update a plugin from its recorded source
pub async fn run_plugin_update(plugin_id: &str) -> Result<()> {
    println!("{}", "=== Updating Plugin ===".bold().cyan());
    println!();

    let mut registry = PluginRegistry::new()?;
    let plugin = registry
        .get(plugin_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;

    let source = read_install_source(&plugin)?;
    let update_source = resolve_update_source(&source).await?;
    let source_path = update_source.path();

    println!("Updating {} from {}", plugin_id.green(), source.display());
    registry.update(plugin_id, source_path)?;
    write_install_source(&registry, plugin_id, &source)?;

    println!("✅ Updated plugin: {}", plugin_id.green());
    Ok(())
}

/// Update all plugins that have a recorded source
pub async fn run_plugin_update_all() -> Result<()> {
    println!("{}", "=== Updating Plugins ===".bold().cyan());
    println!();

    let mut registry = PluginRegistry::new()?;
    let plugins: Vec<InstalledPlugin> = registry.all().cloned().collect();
    if plugins.is_empty() {
        println!("No plugins installed.");
        return Ok(());
    }

    let mut updated = 0usize;
    let mut skipped = 0usize;
    let mut failed = 0usize;

    for plugin in plugins {
        let source = match read_install_source(&plugin) {
            Ok(source) => source,
            Err(_) => {
                skipped += 1;
                continue;
            }
        };
        let update_source = match resolve_update_source(&source).await {
            Ok(source) => source,
            Err(err) => {
                failed += 1;
                tracing::warn!("Failed to update {}: {}", plugin.id(), err);
                continue;
            }
        };

        if let Err(err) = registry.update(plugin.id(), update_source.path()) {
            failed += 1;
            tracing::warn!("Failed to update {}: {}", plugin.id(), err);
            continue;
        }
        let _ = write_install_source(&registry, plugin.id(), &source);
        updated += 1;
        println!("✅ Updated {}", plugin.id().green());
    }

    println!();
    println!(
        "Done. updated={}, skipped={}, failed={}",
        updated, skipped, failed
    );
    Ok(())
}

/// Uninstall a plugin
pub async fn run_plugin_remove(plugin_id: &str) -> Result<()> {
    println!("{}", "=== Uninstalling Plugin ===".bold().cyan());
    println!();

    let mut registry = PluginRegistry::new()?;

    // Check if plugin exists
    if registry.get(plugin_id).is_none() {
        anyhow::bail!("Plugin '{}' not found", plugin_id);
    }

    registry.uninstall(plugin_id)?;

    println!("✅ Successfully uninstalled plugin: {}", plugin_id.green());

    Ok(())
}

/// Enable a plugin
pub async fn run_plugin_enable(plugin_id: &str) -> Result<()> {
    let mut registry = PluginRegistry::new()?;

    let plugin = registry
        .get_mut(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;

    plugin.enable()?;

    println!("✅ Enabled plugin: {}", plugin_id.green());

    Ok(())
}

/// Disable a plugin
pub async fn run_plugin_disable(plugin_id: &str) -> Result<()> {
    let mut registry = PluginRegistry::new()?;

    let plugin = registry
        .get_mut(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;

    plugin.disable()?;

    println!("✅ Disabled plugin: {}", plugin_id.yellow());

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PluginInstallSource {
    is_git: bool,
    url: String,
    branch: Option<String>,
    path: Option<String>,
    installed_at: String,
}

impl PluginInstallSource {
    fn git(url: &str, branch: &str, path: Option<String>) -> Self {
        Self {
            is_git: true,
            url: url.to_string(),
            branch: Some(branch.to_string()),
            path,
            installed_at: Utc::now().to_rfc3339(),
        }
    }

    fn local(path: PathBuf) -> Self {
        Self {
            is_git: false,
            url: path.to_string_lossy().to_string(),
            branch: None,
            path: None,
            installed_at: Utc::now().to_rfc3339(),
        }
    }

    fn display(&self) -> String {
        if self.is_git {
            if let Some(path) = &self.path {
                format!(
                    "{} ({}:{})",
                    self.url,
                    self.branch.as_deref().unwrap_or("main"),
                    path
                )
            } else {
                format!(
                    "{} ({})",
                    self.url,
                    self.branch.as_deref().unwrap_or("main")
                )
            }
        } else {
            self.url.clone()
        }
    }
}

struct StorageScope {
    project_root: Option<PathBuf>,
}

fn prompt_storage_scope() -> Result<StorageScope> {
    println!("Store plugin credentials in:");
    println!("  1) Global (default)");
    println!("  2) Project .tark (current directory)");
    print!("Choose [1-2]: ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    if choice == "2" {
        let cwd = std::env::current_dir()?;
        let storage = TarkStorage::new(&cwd).context("Failed to initialize project storage")?;
        return Ok(StorageScope {
            project_root: Some(storage.project_root().to_path_buf()),
        });
    }

    Ok(StorageScope { project_root: None })
}

fn prompt_required(label: &str) -> Result<String> {
    loop {
        print!("{}: ", label);
        io::stdout().flush()?;
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let value = input.trim().to_string();
        if !value.is_empty() {
            return Ok(value);
        }
        println!("Value required.");
    }
}

fn prompt_optional(label: &str) -> Result<Option<String>> {
    print!("{} (optional): ", label);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim().to_string();
    if value.is_empty() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

fn prompt_yes_no_default(label: &str, default_yes: bool) -> Result<bool> {
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("{} [{}]: ", label, hint);
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let value = input.trim().to_lowercase();
    if value.is_empty() {
        return Ok(default_yes);
    }
    Ok(matches!(value.as_str(), "y" | "yes"))
}

fn build_channel_config_payload(plugin_id: &str) -> Result<Value> {
    if plugin_id != "discord" {
        return Ok(Value::Null);
    }

    println!();
    println!("Discord configuration:");
    let app_id = prompt_required("Discord Application ID")?;
    let public_key = prompt_required("Discord Public Key")?;
    let bot_token = prompt_optional("Discord Bot Token")?;

    let mut config = serde_json::Map::new();
    config.insert("application_id".to_string(), Value::String(app_id));
    config.insert("public_key".to_string(), Value::String(public_key));
    if let Some(token) = bot_token {
        config.insert("bot_token".to_string(), Value::String(token));
    }

    Ok(Value::Object(
        [("config".to_string(), Value::Object(config))]
            .into_iter()
            .collect(),
    ))
}

fn merge_tokens_and_config(tokens_json: &str, config_payload: Value) -> Result<String> {
    let tokens_value: Value = serde_json::from_str(tokens_json)
        .unwrap_or_else(|_| Value::String(tokens_json.to_string()));

    if config_payload.is_null() {
        return Ok(serde_json::to_string(&tokens_value)?);
    }

    let mut merged = serde_json::Map::new();
    merged.insert("tokens".to_string(), tokens_value);
    if let Some(config) = config_payload.get("config") {
        merged.insert("config".to_string(), config.clone());
    }
    Ok(serde_json::to_string(&Value::Object(merged))?)
}

fn encrypt_credentials(path: &PathBuf) -> Result<()> {
    println!();
    println!("Encrypting credentials with a passphrase.");
    let passphrase = secure_store::prompt_new_passphrase()?;
    secure_store::encrypt_file_in_place(path, &passphrase)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn default_oauth_credentials_path(plugin_id: &str) -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config_dir
        .join("tark")
        .join(format!("{}_oauth.json", plugin_id)))
}

fn source_file_path(registry: &PluginRegistry, plugin_id: &str) -> PathBuf {
    registry.plugins_dir().join(plugin_id).join(".install.json")
}

fn write_install_source(
    registry: &PluginRegistry,
    plugin_id: &str,
    source: &PluginInstallSource,
) -> Result<()> {
    let path = source_file_path(registry, plugin_id);
    let payload = serde_json::to_string_pretty(source)?;
    std::fs::write(&path, payload)?;
    Ok(())
}

fn read_install_source(plugin: &InstalledPlugin) -> Result<PluginInstallSource> {
    let path = plugin.path.join(".install.json");
    let payload = std::fs::read_to_string(&path)
        .with_context(|| format!("Missing install metadata: {}", path.display()))?;
    let source = serde_json::from_str(&payload)?;
    Ok(source)
}

struct UpdateSource {
    _temp: Option<tempfile::TempDir>,
    path: PathBuf,
}

impl UpdateSource {
    fn path(&self) -> &PathBuf {
        &self.path
    }
}

async fn resolve_update_source(source: &PluginInstallSource) -> Result<UpdateSource> {
    if !source.is_git {
        let path = PathBuf::from(&source.url);
        if !path.exists() {
            anyhow::bail!("Local plugin source path not found: {}", path.display());
        }
        return Ok(UpdateSource { _temp: None, path });
    }

    let temp = tempfile::tempdir()?;
    let clone_path = temp.path().join("plugin");
    let branch = source.branch.as_deref().unwrap_or("main");

    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--branch", branch, &source.url])
        .arg(&clone_path)
        .status()
        .context("Failed to run git clone")?;
    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    let final_path = if let Some(sub) = &source.path {
        clone_path.join(sub)
    } else {
        clone_path
    };

    if !final_path.exists() {
        anyhow::bail!("Subdirectory not found in repository");
    }

    Ok(UpdateSource {
        _temp: Some(temp),
        path: final_path,
    })
}
