//! Plugin registry - manages installed plugins
//!
//! Handles plugin discovery, installation, and lifecycle.

use super::manifest::{PluginManifest, PluginType};
use super::plugins_dir;
use anyhow::Result;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Information about an installed plugin
#[derive(Debug, Clone)]
pub struct InstalledPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,
    /// Installation directory
    pub path: PathBuf,
    /// Path to WASM module
    pub wasm_path: PathBuf,
    /// Whether the plugin is enabled
    pub enabled: bool,
}

impl InstalledPlugin {
    /// Load an installed plugin from its directory
    pub fn load(plugin_dir: &Path) -> Result<Self> {
        let manifest_path = plugin_dir.join("plugin.toml");
        let manifest = PluginManifest::load(&manifest_path)?;

        let wasm_path = plugin_dir.join(&manifest.plugin.wasm);
        if !wasm_path.exists() {
            anyhow::bail!(
                "WASM module not found: {} (declared in manifest)",
                wasm_path.display()
            );
        }

        // Check for disabled marker
        let enabled = !plugin_dir.join(".disabled").exists();

        Ok(Self {
            manifest,
            path: plugin_dir.to_path_buf(),
            wasm_path,
            enabled,
        })
    }

    /// Get the plugin ID
    pub fn id(&self) -> &str {
        self.manifest.id()
    }

    /// Get the plugin type
    pub fn plugin_type(&self) -> PluginType {
        self.manifest.plugin_type()
    }

    /// Get the plugin data directory
    pub fn data_dir(&self) -> PathBuf {
        self.path.join("data")
    }

    /// Enable the plugin
    pub fn enable(&mut self) -> Result<()> {
        let marker = self.path.join(".disabled");
        if marker.exists() {
            std::fs::remove_file(&marker)?;
        }
        self.enabled = true;
        Ok(())
    }

    /// Disable the plugin
    pub fn disable(&mut self) -> Result<()> {
        let marker = self.path.join(".disabled");
        std::fs::write(&marker, "")?;
        self.enabled = false;
        Ok(())
    }

    /// Check if plugin should activate for an event
    pub fn should_activate_for(&self, event: &str) -> bool {
        // If no activation events specified, activate on startup
        if self.manifest.activation.events.is_empty() {
            return event == "onStartup";
        }

        self.manifest.activation.events.iter().any(|e| {
            if e == event {
                return true;
            }
            // Handle wildcard events like "onProvider:*"
            if let Some(prefix) = e.strip_suffix('*') {
                return event.starts_with(prefix);
            }
            false
        })
    }

    /// Get contributed providers
    pub fn contributed_providers(&self) -> &[super::manifest::ProviderContribution] {
        &self.manifest.contributes.providers
    }
}

/// Registry of all installed plugins
pub struct PluginRegistry {
    /// Installed plugins by ID
    plugins: HashMap<String, InstalledPlugin>,
    /// Plugins directory
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    /// Create a new plugin registry, scanning the plugins directory
    pub fn new() -> Result<Self> {
        let plugins_dir = plugins_dir()?;
        let mut registry = Self {
            plugins: HashMap::new(),
            plugins_dir,
        };

        registry.scan()?;

        Ok(registry)
    }

    /// Scan the plugins directory for installed plugins
    pub fn scan(&mut self) -> Result<()> {
        self.plugins.clear();

        if !self.plugins_dir.exists() {
            std::fs::create_dir_all(&self.plugins_dir)?;
            return Ok(());
        }

        for entry in std::fs::read_dir(&self.plugins_dir)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            // Check for plugin.toml
            let manifest_path = path.join("plugin.toml");
            if !manifest_path.exists() {
                tracing::debug!("Skipping {}: no plugin.toml", path.display());
                continue;
            }

            match InstalledPlugin::load(&path) {
                Ok(plugin) => {
                    tracing::debug!(
                        "Found plugin: {} v{} ({})",
                        plugin.manifest.plugin.name,
                        plugin.manifest.plugin.version,
                        plugin.plugin_type()
                    );
                    self.plugins.insert(plugin.id().to_string(), plugin);
                }
                Err(e) => {
                    tracing::warn!("Failed to load plugin at {}: {}", path.display(), e);
                }
            }
        }

        tracing::debug!("Loaded {} plugins", self.plugins.len());
        Ok(())
    }

    /// Get all installed plugins
    pub fn all(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins.values()
    }

    /// Get enabled plugins
    pub fn enabled(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins.values().filter(|p| p.enabled)
    }

    /// Get enabled plugins as a Vec (for when you need ownership or multiple iterations)
    pub fn list_enabled(&self) -> Vec<&InstalledPlugin> {
        self.plugins.values().filter(|p| p.enabled).collect()
    }

    /// Get enabled provider plugins
    pub fn provider_plugins(&self) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins
            .values()
            .filter(|p| p.enabled && p.plugin_type() == PluginType::Provider)
    }

    /// Get plugins by type
    pub fn by_type(&self, plugin_type: PluginType) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins
            .values()
            .filter(move |p| p.plugin_type() == plugin_type)
    }

    /// Get a specific plugin by ID
    pub fn get(&self, id: &str) -> Option<&InstalledPlugin> {
        self.plugins.get(id)
    }

    /// Get a mutable reference to a plugin
    pub fn get_mut(&mut self, id: &str) -> Option<&mut InstalledPlugin> {
        self.plugins.get_mut(id)
    }

    /// Install a plugin from a directory (copies to plugins dir)
    pub fn install(&mut self, source_dir: &PathBuf) -> Result<String> {
        // Load manifest to get plugin ID
        let manifest_path = source_dir.join("plugin.toml");
        let manifest = PluginManifest::load(&manifest_path)?;
        let plugin_id = manifest.id().to_string();

        // Check if already installed
        if self.plugins.contains_key(&plugin_id) {
            anyhow::bail!("Plugin '{}' is already installed", plugin_id);
        }

        // Copy to plugins directory
        let dest_dir = self.plugins_dir.join(&plugin_id);
        if dest_dir.exists() {
            std::fs::remove_dir_all(&dest_dir)?;
        }

        copy_dir_recursive(source_dir, &dest_dir)?;

        // Load the installed plugin
        let plugin = InstalledPlugin::load(&dest_dir)?;
        self.plugins.insert(plugin_id.clone(), plugin);

        tracing::info!("Installed plugin: {}", plugin_id);
        Ok(plugin_id)
    }

    /// Update an installed plugin from a directory (preserves data and disabled state)
    pub fn update(&mut self, id: &str, source_dir: &PathBuf) -> Result<()> {
        let manifest_path = source_dir.join("plugin.toml");
        let manifest = PluginManifest::load(&manifest_path)?;
        let source_id = manifest.id();
        if source_id != id {
            anyhow::bail!(
                "Plugin ID mismatch: expected '{}', found '{}'",
                id,
                source_id
            );
        }

        let dest_dir = self.plugins_dir.join(id);
        if !dest_dir.exists() {
            anyhow::bail!("Plugin '{}' is not installed", id);
        }

        let preserve = ["data", ".disabled", ".install.json"];
        clear_dir_except(&dest_dir, &preserve)?;
        copy_dir_recursive_excluding(source_dir, &dest_dir, &preserve)?;

        let plugin = InstalledPlugin::load(&dest_dir)?;
        self.plugins.insert(id.to_string(), plugin);
        Ok(())
    }

    /// Uninstall a plugin
    pub fn uninstall(&mut self, id: &str) -> Result<()> {
        let plugin = self
            .plugins
            .remove(id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;

        std::fs::remove_dir_all(&plugin.path)?;

        tracing::info!("Uninstalled plugin: {}", id);
        Ok(())
    }

    /// Get the plugins directory
    pub fn plugins_dir(&self) -> &PathBuf {
        &self.plugins_dir
    }

    /// Get the number of installed plugins
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Check if registry is empty
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn copy_dir_recursive_excluding(src: &PathBuf, dst: &PathBuf, skip: &[&str]) -> Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if skip.iter().any(|s| s == &name_str) {
            continue;
        }
        let src_path = entry.path();
        let dst_path = dst.join(name);

        if src_path.is_dir() {
            copy_dir_recursive_excluding(&src_path, &dst_path, skip)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn clear_dir_except(dir: &PathBuf, keep: &[&str]) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if keep.iter().any(|s| s == &name_str) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        // Just verify the registry can be created
        // It will create the plugins directory if it doesn't exist
        let result = PluginRegistry::new();
        assert!(result.is_ok());
    }
}
