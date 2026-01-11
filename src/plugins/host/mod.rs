//! WASM plugin host using wasmtime
//!
//! Provides the runtime environment for executing plugins.

use super::manifest::PluginCapabilities;
use super::registry::InstalledPlugin;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use wasmtime::*;

/// A loaded plugin instance
pub struct PluginInstance {
    /// Plugin ID
    pub id: String,
    /// WASM store
    store: Store<PluginState>,
    /// WASM instance
    instance: Instance,
    /// Plugin capabilities
    capabilities: PluginCapabilities,
}

/// State passed to plugin host functions
struct PluginState {
    /// Plugin ID
    plugin_id: String,
    /// Plugin data directory
    data_dir: PathBuf,
    /// Capabilities
    capabilities: PluginCapabilities,
}

/// Plugin host - manages WASM runtime and plugin instances
pub struct PluginHost {
    /// WASM engine
    engine: Engine,
    /// Loaded plugin instances
    instances: HashMap<String, PluginInstance>,
}

impl PluginHost {
    /// Create a new plugin host
    pub fn new() -> Result<Self> {
        let mut config = Config::new();
        config.async_support(true);

        let engine = Engine::new(&config)?;

        Ok(Self {
            engine,
            instances: HashMap::new(),
        })
    }

    /// Load a plugin
    pub fn load(&mut self, plugin: &InstalledPlugin) -> Result<()> {
        tracing::info!("Loading plugin: {}", plugin.id());

        // Read WASM module
        let wasm_bytes = std::fs::read(&plugin.wasm_path)
            .with_context(|| format!("Failed to read WASM: {}", plugin.wasm_path.display()))?;

        // Compile module
        let module = Module::new(&self.engine, &wasm_bytes)
            .with_context(|| format!("Failed to compile WASM: {}", plugin.id()))?;

        // Create store with plugin state
        let state = PluginState {
            plugin_id: plugin.id().to_string(),
            data_dir: plugin.data_dir(),
            capabilities: plugin.manifest.capabilities.clone(),
        };

        let mut store = Store::new(&self.engine, state);

        // Create linker with host functions
        let mut linker = Linker::new(&self.engine);
        Self::define_host_functions(&mut linker)?;

        // Instantiate module
        let instance = linker
            .instantiate(&mut store, &module)
            .with_context(|| format!("Failed to instantiate plugin: {}", plugin.id()))?;

        let plugin_instance = PluginInstance {
            id: plugin.id().to_string(),
            store,
            instance,
            capabilities: plugin.manifest.capabilities.clone(),
        };

        self.instances
            .insert(plugin.id().to_string(), plugin_instance);

        tracing::info!("Loaded plugin: {}", plugin.id());
        Ok(())
    }

    /// Unload a plugin
    pub fn unload(&mut self, plugin_id: &str) -> Result<()> {
        self.instances
            .remove(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not loaded: {}", plugin_id))?;

        tracing::info!("Unloaded plugin: {}", plugin_id);
        Ok(())
    }

    /// Get a loaded plugin instance
    pub fn get(&self, plugin_id: &str) -> Option<&PluginInstance> {
        self.instances.get(plugin_id)
    }

    /// Get a mutable reference to a loaded plugin instance
    pub fn get_mut(&mut self, plugin_id: &str) -> Option<&mut PluginInstance> {
        self.instances.get_mut(plugin_id)
    }

    /// Define host functions available to plugins
    fn define_host_functions(linker: &mut Linker<PluginState>) -> Result<()> {
        // Storage functions
        linker.func_wrap(
            "tark:storage",
            "get",
            |_caller: Caller<'_, PluginState>, _key_ptr: i32, _key_len: i32| -> i32 {
                // Implementation would read from plugin's data directory
                // Returns pointer to result in WASM memory
                0 // Placeholder
            },
        )?;

        linker.func_wrap(
            "tark:storage",
            "set",
            |_caller: Caller<'_, PluginState>,
             _key_ptr: i32,
             _key_len: i32,
             _val_ptr: i32,
             _val_len: i32|
             -> i32 {
                // Implementation would write to plugin's data directory
                // Returns 0 on success, -1 on error
                0 // Placeholder
            },
        )?;

        // Log function
        linker.func_wrap(
            "tark:log",
            "info",
            |caller: Caller<'_, PluginState>, _msg_ptr: i32, _msg_len: i32| {
                // Read message from WASM memory and log it
                let plugin_id = &caller.data().plugin_id;
                tracing::info!("[plugin:{}] (message)", plugin_id);
            },
        )?;

        Ok(())
    }

    /// Load all enabled plugins from registry
    pub fn load_all(&mut self, registry: &super::registry::PluginRegistry) -> Result<()> {
        for plugin in registry.enabled() {
            if let Err(e) = self.load(plugin) {
                tracing::error!("Failed to load plugin '{}': {}", plugin.id(), e);
                // Continue loading other plugins
            }
        }
        Ok(())
    }

    /// Get the number of loaded plugins
    pub fn len(&self) -> usize {
        self.instances.len()
    }

    /// Check if no plugins are loaded
    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new().expect("Failed to create plugin host")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_host_creation() {
        let host = PluginHost::new();
        assert!(host.is_ok());
    }
}
