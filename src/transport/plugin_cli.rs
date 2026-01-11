//! CLI commands for plugin management

use crate::plugins::{PluginManifest, PluginRegistry};
use anyhow::{Context, Result};
use colored::Colorize;
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

/// Install a plugin from git
pub async fn run_plugin_add(url: &str, branch: &str) -> Result<()> {
    println!("{}", "=== Installing Plugin ===".bold().cyan());
    println!();
    println!("Repository: {}", url);
    println!("Branch:     {}", branch);
    println!();

    // Create temp directory for clone
    let temp_dir = tempfile::tempdir()?;
    let clone_path = temp_dir.path().join("plugin");

    println!("Cloning repository...");

    // Clone the repository
    let status = std::process::Command::new("git")
        .args(["clone", "--depth", "1", "--branch", branch, url])
        .arg(&clone_path)
        .status()
        .context("Failed to run git clone")?;

    if !status.success() {
        anyhow::bail!("git clone failed");
    }

    // Verify plugin.toml exists
    let manifest_path = clone_path.join("plugin.toml");
    if !manifest_path.exists() {
        anyhow::bail!("No plugin.toml found in repository");
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
    let wasm_path = clone_path.join(&manifest.plugin.wasm);
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
    let plugin_id = registry.install(&clone_path.to_path_buf())?;

    println!();
    println!("✅ Successfully installed plugin: {}", plugin_id.green());
    println!();
    println!("The plugin will be loaded on next tark start.");

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
