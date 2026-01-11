# Plugin Runtime Implementation Plan

**Goal**: Build a plug-and-play plugin system using WASM sandboxing that allows users to install, manage, and run third-party plugins without recompiling tark.

**Status**: Ready for implementation (after 001-gemini-oauth)

**Dependencies**: 
- `001-gemini-oauth.md` should be completed first (establishes auth patterns)

---

## Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         TARK BINARY                             │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                   Plugin Host (wasmtime)                  │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │  │
│  │  │ Auth Plugin │  │ Tool Plugin │  │Provider Plug│       │  │
│  │  │   (.wasm)   │  │   (.wasm)   │  │   (.wasm)   │       │  │
│  │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘       │  │
│  │         │                │                │               │  │
│  │         └────────────────┼────────────────┘               │  │
│  │                          │                                │  │
│  │                    WIT Interface                          │  │
│  │                          │                                │  │
│  │  ┌───────────────────────┴───────────────────────────┐   │  │
│  │  │              Capability Gate                       │   │  │
│  │  │  storage │ http │ env │ shell │ filesystem        │   │  │
│  │  └───────────────────────────────────────────────────┘   │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

---

## Prerequisites

Before starting:
1. Complete `001-gemini-oauth.md` (establishes auth patterns)
2. Ensure Rust toolchain is up to date (`rustup update`)
3. Familiarity with WASM concepts helpful but not required

---

## Phase 1: Add Dependencies and Create Module Structure

### Task 1.1: Add wasmtime dependencies to `Cargo.toml`

**File**: `Cargo.toml`

**Action**: Add to `[dependencies]` section:

```toml
# WASM Plugin Runtime
wasmtime = "27"
wasmtime-wasi = "27"
wit-bindgen = "0.36"
```

**Command after change**:
```bash
cargo check
```

---

### Task 1.2: Create plugin module structure

**Action**: Create the following directory structure:

```bash
mkdir -p src/plugins/{host,manifest,registry,wit}
touch src/plugins/mod.rs
touch src/plugins/host/mod.rs
touch src/plugins/manifest/mod.rs
touch src/plugins/registry/mod.rs
```

---

### Task 1.3: Create `src/plugins/mod.rs`

**File**: `src/plugins/mod.rs`

```rust
//! Plugin system for tark
//!
//! Provides a WASM-based plugin runtime that allows third-party extensions
//! to add authentication methods, tools, and LLM providers.
//!
//! # Architecture
//!
//! Plugins are WebAssembly modules that run in a sandbox. They communicate
//! with tark through WIT (WebAssembly Interface Types) interfaces.
//!
//! # Plugin Types
//!
//! - **Auth Plugins**: Add OAuth/authentication for new providers
//! - **Tool Plugins**: Add new tools the agent can use
//! - **Provider Plugins**: Add new LLM providers
//!
//! # Security
//!
//! Plugins declare required capabilities in `plugin.toml`. The host
//! only grants capabilities that are declared and approved.

mod host;
mod manifest;
mod registry;

pub use host::{PluginHost, PluginInstance};
pub use manifest::{PluginManifest, PluginCapabilities, PluginType};
pub use registry::{PluginRegistry, InstalledPlugin};

use anyhow::Result;
use std::path::PathBuf;

/// Get the plugins directory path
pub fn plugins_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_local_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;
    
    Ok(data_dir.join("tark").join("plugins"))
}

/// Get the plugin data directory for a specific plugin
pub fn plugin_data_dir(plugin_id: &str) -> Result<PathBuf> {
    Ok(plugins_dir()?.join(plugin_id).join("data"))
}
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.4: Create `src/plugins/manifest/mod.rs`

**File**: `src/plugins/manifest/mod.rs`

```rust
//! Plugin manifest parsing and validation
//!
//! Each plugin must have a `plugin.toml` manifest declaring:
//! - Plugin metadata (name, version, author)
//! - Plugin type (auth, tool, provider)
//! - Required capabilities (storage, http, env, etc.)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
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
                var.starts_with(&allowed[..allowed.len()-1])
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
            anyhow::bail!("Plugin version must be valid semver: {}", self.plugin.version);
        }
        
        // Validate plugin name (alphanumeric + hyphens)
        if !self.plugin.name.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
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
        assert!(manifest.capabilities.is_http_allowed("oauth2.googleapis.com"));
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
```

**Add semver dependency to Cargo.toml** if not present:

```toml
semver = "1"
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.5: Create `src/plugins/registry/mod.rs`

**File**: `src/plugins/registry/mod.rs`

```rust
//! Plugin registry - manages installed plugins
//!
//! Handles plugin discovery, installation, and lifecycle.

use super::manifest::{PluginManifest, PluginType};
use super::plugins_dir;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;

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
    pub fn load(plugin_dir: &PathBuf) -> Result<Self> {
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
            path: plugin_dir.clone(),
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
                    tracing::info!(
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
        
        tracing::info!("Loaded {} plugins", self.plugins.len());
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
    
    /// Get plugins by type
    pub fn by_type(&self, plugin_type: PluginType) -> impl Iterator<Item = &InstalledPlugin> {
        self.plugins.values().filter(move |p| p.plugin_type() == plugin_type)
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
    
    /// Uninstall a plugin
    pub fn uninstall(&mut self, id: &str) -> Result<()> {
        let plugin = self.plugins.remove(id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", id))?;
        
        std::fs::remove_dir_all(&plugin.path)?;
        
        tracing::info!("Uninstalled plugin: {}", id);
        Ok(())
    }
    
    /// Get the plugins directory
    pub fn plugins_dir(&self) -> &PathBuf {
        &self.plugins_dir
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_registry_scan_empty() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("XDG_DATA_HOME", temp_dir.path().join("data"));
        
        let registry = PluginRegistry::new().unwrap();
        assert_eq!(registry.plugins.len(), 0);
    }
}
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.6: Create `src/plugins/host/mod.rs`

**File**: `src/plugins/host/mod.rs`

```rust
//! WASM plugin host using wasmtime
//!
//! Provides the runtime environment for executing plugins.

use super::manifest::PluginCapabilities;
use super::registry::InstalledPlugin;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
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
    /// HTTP client for making requests
    http_client: reqwest::Client,
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
        config.wasm_component_model(true);
        
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
            http_client: reqwest::Client::new(),
        };
        
        let mut store = Store::new(&self.engine, state);
        
        // Create linker with host functions
        let mut linker = Linker::new(&self.engine);
        Self::define_host_functions(&mut linker)?;
        
        // Instantiate module
        let instance = linker.instantiate(&mut store, &module)
            .with_context(|| format!("Failed to instantiate plugin: {}", plugin.id()))?;
        
        let plugin_instance = PluginInstance {
            id: plugin.id().to_string(),
            store,
            instance,
            capabilities: plugin.manifest.capabilities.clone(),
        };
        
        self.instances.insert(plugin.id().to_string(), plugin_instance);
        
        tracing::info!("Loaded plugin: {}", plugin.id());
        Ok(())
    }
    
    /// Unload a plugin
    pub fn unload(&mut self, plugin_id: &str) -> Result<()> {
        self.instances.remove(plugin_id)
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
        linker.func_wrap("tark:storage", "get", |caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32| -> i32 {
            // Implementation would read from plugin's data directory
            // Returns pointer to result in WASM memory
            0 // Placeholder
        })?;
        
        linker.func_wrap("tark:storage", "set", |caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32| -> i32 {
            // Implementation would write to plugin's data directory
            // Returns 0 on success, -1 on error
            0 // Placeholder
        })?;
        
        // HTTP functions (async would need different approach)
        // For now, these are placeholders showing the pattern
        
        // Log function
        linker.func_wrap("tark:log", "info", |caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
            // Read message from WASM memory and log it
            let plugin_id = &caller.data().plugin_id;
            tracing::info!("[plugin:{}] (message)", plugin_id);
        })?;
        
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
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.7: Export plugins module from `src/lib.rs`

**File**: `src/lib.rs`

**Action**: Add plugins module export. Find the module declarations and add:

```rust
pub mod plugins;
```

**Command after change**:
```bash
cargo check --lib
```

---

### Task 1.8: Commit Phase 1

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/plugins/
git add src/lib.rs
git add Cargo.toml
git commit -m "feat(plugins): add plugin system foundation

- Add wasmtime runtime for WASM plugins
- Add PluginManifest for plugin.toml parsing
- Add PluginRegistry for plugin discovery and management
- Add PluginHost for WASM execution
- Support for auth, tool, provider, and hook plugin types
- Capability-based security model"
```

---

## Phase 2: Define WIT Interfaces

### Task 2.1: Create WIT directory structure

**Action**:
```bash
mkdir -p src/plugins/wit
```

---

### Task 2.2: Create `src/plugins/wit/tark.wit`

**File**: `src/plugins/wit/tark.wit`

```wit
// tark.wit - WebAssembly Interface Types for tark plugins
//
// This file defines the contract between tark (host) and plugins (guest).
// Plugins import interfaces they need and export interfaces they implement.

package tark:plugin;

// ============================================================================
// Host-provided interfaces (plugins import these)
// ============================================================================

/// Persistent storage for plugin data
interface storage {
    /// Get a value by key
    /// Returns None if key doesn't exist
    get: func(key: string) -> option<string>;
    
    /// Set a value for a key
    /// Overwrites existing value
    set: func(key: string, value: string) -> result<_, string>;
    
    /// Delete a key
    delete: func(key: string) -> result<_, string>;
    
    /// List all keys
    list-keys: func() -> list<string>;
}

/// HTTP client for making requests
interface http {
    /// HTTP response
    record http-response {
        status: u16,
        headers: list<tuple<string, string>>,
        body: string,
    }
    
    /// HTTP error
    record http-error {
        message: string,
        status: option<u16>,
    }
    
    /// Make a GET request
    get: func(url: string, headers: list<tuple<string, string>>) 
         -> result<http-response, http-error>;
    
    /// Make a POST request
    post: func(url: string, body: string, headers: list<tuple<string, string>>) 
          -> result<http-response, http-error>;
}

/// Environment variable access
interface env {
    /// Get an environment variable
    /// Returns None if not set or not allowed
    get: func(name: string) -> option<string>;
}

/// Logging
interface log {
    /// Log at debug level
    debug: func(message: string);
    
    /// Log at info level
    info: func(message: string);
    
    /// Log at warn level
    warn: func(message: string);
    
    /// Log at error level
    error: func(message: string);
}

// ============================================================================
// Plugin-provided interfaces (plugins export these)
// ============================================================================

/// Authentication plugin interface
interface auth-plugin {
    /// Device code response for OAuth device flow
    record device-code {
        device-code: string,
        user-code: string,
        verification-url: string,
        expires-in: u64,
        interval: u64,
    }
    
    /// Authentication status
    enum auth-status {
        not-authenticated,
        authenticated,
        expired,
    }
    
    /// Poll result
    variant poll-result {
        pending,
        success(string),  // access token
        expired,
        error(string),
    }
    
    /// Get plugin display name
    display-name: func() -> string;
    
    /// Get current authentication status
    status: func() -> auth-status;
    
    /// Start device flow authentication
    start-device-flow: func() -> result<device-code, string>;
    
    /// Poll for token (call after user authorizes)
    poll: func(device-code: string) -> poll-result;
    
    /// Get current access token (if authenticated)
    get-token: func() -> result<string, string>;
    
    /// Logout (clear stored credentials)
    logout: func() -> result<_, string>;
}

/// Tool plugin interface
interface tool-plugin {
    /// Tool definition
    record tool-def {
        name: string,
        description: string,
        parameters-schema: string,  // JSON schema
    }
    
    /// Tool execution result
    record tool-result {
        success: bool,
        output: string,
        error: option<string>,
    }
    
    /// Risk level for the tool
    enum risk-level {
        read-only,
        write,
        risky,
        dangerous,
    }
    
    /// Get tool definitions
    get-tools: func() -> list<tool-def>;
    
    /// Get risk level for a tool
    get-risk-level: func(tool-name: string) -> risk-level;
    
    /// Execute a tool
    execute: func(tool-name: string, arguments: string) -> tool-result;
}

/// LLM provider plugin interface  
interface provider-plugin {
    /// Message role
    enum role {
        system,
        user,
        assistant,
        tool,
    }
    
    /// Chat message
    record message {
        role: role,
        content: string,
    }
    
    /// Token usage
    record usage {
        input-tokens: u32,
        output-tokens: u32,
    }
    
    /// Chat response
    record chat-response {
        text: string,
        usage: option<usage>,
    }
    
    /// Get provider name
    name: func() -> string;
    
    /// Get available models
    models: func() -> list<string>;
    
    /// Send chat completion (non-streaming)
    chat: func(messages: list<message>, model: string) -> result<chat-response, string>;
}

// ============================================================================
// World definitions (what plugins export)
// ============================================================================

/// Auth plugin world
world auth-world {
    import storage;
    import http;
    import env;
    import log;
    
    export auth-plugin;
}

/// Tool plugin world
world tool-world {
    import storage;
    import http;
    import env;
    import log;
    
    export tool-plugin;
}

/// Provider plugin world
world provider-world {
    import storage;
    import http;
    import env;
    import log;
    
    export provider-plugin;
}
```

---

### Task 2.3: Commit Phase 2

**Commands**:
```bash
git add src/plugins/wit/
git commit -m "feat(plugins): add WIT interface definitions

- Define storage, http, env, log host interfaces
- Define auth-plugin interface for OAuth plugins
- Define tool-plugin interface for agent tools
- Define provider-plugin interface for LLM providers
- Define world exports for each plugin type"
```

---

## Phase 3: Add CLI Commands

### Task 3.1: Update `src/main.rs` with plugin subcommands

**File**: `src/main.rs`

**Action**: Add plugin subcommands to the CLI. The exact changes depend on how clap is structured in main.rs.

Add these subcommands:

```rust
/// Plugin management commands
#[derive(Subcommand)]
enum PluginCommands {
    /// List installed plugins
    List,
    
    /// Show plugin details
    Info {
        /// Plugin ID
        plugin_id: String,
    },
    
    /// Install a plugin from a git repository
    Add {
        /// Git repository URL
        url: String,
        
        /// Branch or tag (default: main)
        #[arg(short, long, default_value = "main")]
        branch: String,
    },
    
    /// Uninstall a plugin
    Remove {
        /// Plugin ID
        plugin_id: String,
    },
    
    /// Enable a disabled plugin
    Enable {
        /// Plugin ID
        plugin_id: String,
    },
    
    /// Disable a plugin
    Disable {
        /// Plugin ID
        plugin_id: String,
    },
}
```

---

### Task 3.2: Create `src/transport/plugin_cli.rs`

**File**: `src/transport/plugin_cli.rs`

```rust
//! CLI commands for plugin management

use crate::plugins::{PluginRegistry, InstalledPlugin, PluginType};
use anyhow::{Context, Result};
use colored::Colorize;
use std::path::PathBuf;
use tabled::{Table, Tabled, settings::Style};

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
    
    let rows: Vec<PluginRow> = registry.all()
        .map(|p| PluginRow {
            id: p.id().to_string(),
            version: p.manifest.plugin.version.clone(),
            plugin_type: p.plugin_type().to_string(),
            status: if p.enabled { "✓ enabled".green().to_string() } else { "✗ disabled".red().to_string() },
            description: p.manifest.plugin.description.chars().take(40).collect::<String>(),
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
    
    let plugin = registry.get(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;
    
    println!("{}", format!("=== Plugin: {} ===", plugin_id).bold().cyan());
    println!();
    
    println!("{}:    {}", "Name".bold(), plugin.manifest.plugin.name);
    println!("{}:  {}", "Version".bold(), plugin.manifest.plugin.version);
    println!("{}:    {}", "Type".bold(), plugin.plugin_type());
    println!("{}:  {}", "Status".bold(), if plugin.enabled { "enabled".green() } else { "disabled".red() });
    
    if !plugin.manifest.plugin.description.is_empty() {
        println!("{}: {}", "Description".bold(), plugin.manifest.plugin.description);
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
    println!("  HTTP:    {}", if caps.http.is_empty() { "✗".to_string() } else { caps.http.join(", ") });
    println!("  Env:     {}", if caps.env.is_empty() { "✗".to_string() } else { caps.env.join(", ") });
    println!("  Shell:   {}", if caps.shell { "⚠️ yes (dangerous!)".red().to_string() } else { "✗".to_string() });
    
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
    let manifest = crate::plugins::PluginManifest::load(&manifest_path)?;
    
    println!();
    println!("Found plugin: {} v{}", manifest.plugin.name.green(), manifest.plugin.version);
    println!("Type: {}", manifest.plugin_type());
    println!("Capabilities:");
    let caps = &manifest.capabilities;
    if caps.storage { println!("  • Storage access"); }
    if !caps.http.is_empty() { println!("  • HTTP to: {}", caps.http.join(", ")); }
    if !caps.env.is_empty() { println!("  • Env vars: {}", caps.env.join(", ")); }
    if caps.shell { println!("  • ⚠️  Shell access (dangerous!)"); }
    
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
    let plugin_id = registry.install(&clone_path)?;
    
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
    
    let plugin = registry.get_mut(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;
    
    plugin.enable()?;
    
    println!("✅ Enabled plugin: {}", plugin_id.green());
    
    Ok(())
}

/// Disable a plugin
pub async fn run_plugin_disable(plugin_id: &str) -> Result<()> {
    let mut registry = PluginRegistry::new()?;
    
    let plugin = registry.get_mut(plugin_id)
        .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;
    
    plugin.disable()?;
    
    println!("✅ Disabled plugin: {}", plugin_id.yellow());
    
    Ok(())
}
```

---

### Task 3.3: Export plugin_cli from transport module

**File**: `src/transport/mod.rs`

**Action**: Add module export:

```rust
pub mod plugin_cli;
```

---

### Task 3.4: Commit Phase 3

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/transport/plugin_cli.rs
git add src/transport/mod.rs
git add src/main.rs
git commit -m "feat(cli): add plugin management commands

- tark plugin list: show installed plugins
- tark plugin info <id>: show plugin details
- tark plugin add <url>: install from git repo
- tark plugin remove <id>: uninstall a plugin
- tark plugin enable/disable <id>: toggle plugin state"
```

---

## Phase 4: Plugin SDK Foundation

### Task 4.1: Create SDK crate structure (separate crate)

**Note**: This would be a separate crate, potentially in a workspace or published to crates.io.

For now, create documentation for plugin developers:

**File**: `docs/PLUGIN_SDK.md`

```markdown
# Tark Plugin SDK

This guide explains how to build plugins for tark.

## Quick Start

### 1. Create a new Rust project

```bash
cargo new my-tark-plugin --lib
cd my-tark-plugin
```

### 2. Add dependencies

```toml
[package]
name = "my-tark-plugin"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
wit-bindgen = "0.36"
```

### 3. Create plugin.toml

```toml
[plugin]
name = "my-plugin"
version = "0.1.0"
type = "auth"  # or "tool" or "provider"
description = "My awesome plugin"
author = "Your Name"

[capabilities]
storage = true
http = ["api.example.com"]
```

### 4. Implement the plugin

```rust
// src/lib.rs
wit_bindgen::generate!({
    world: "auth-world",
    path: "tark.wit",
});

struct MyAuthPlugin;

impl exports::tark::plugin::auth_plugin::Guest for MyAuthPlugin {
    fn display_name() -> String {
        "My Auth".to_string()
    }
    
    fn status() -> AuthStatus {
        // Check for stored credentials
        if let Some(_token) = tark::plugin::storage::get("token") {
            AuthStatus::Authenticated
        } else {
            AuthStatus::NotAuthenticated
        }
    }
    
    // ... implement other methods
}

export!(MyAuthPlugin);
```

### 5. Build the plugin

```bash
cargo build --release --target wasm32-wasip1
cp target/wasm32-wasip1/release/my_tark_plugin.wasm plugin.wasm
```

### 6. Test locally

```bash
# Install to tark
tark plugin add ./

# Or copy manually
cp -r . ~/.local/share/tark/plugins/my-plugin/
```

## Plugin Types

### Auth Plugin

Add authentication methods (OAuth, API keys).

```rust
impl exports::tark::plugin::auth_plugin::Guest for MyPlugin {
    fn display_name() -> String;
    fn status() -> AuthStatus;
    fn start_device_flow() -> Result<DeviceCode, String>;
    fn poll(device_code: String) -> PollResult;
    fn get_token() -> Result<String, String>;
    fn logout() -> Result<(), String>;
}
```

### Tool Plugin

Add tools the agent can use.

```rust
impl exports::tark::plugin::tool_plugin::Guest for MyPlugin {
    fn get_tools() -> Vec<ToolDef>;
    fn get_risk_level(tool_name: String) -> RiskLevel;
    fn execute(tool_name: String, arguments: String) -> ToolResult;
}
```

### Provider Plugin

Add LLM providers.

```rust
impl exports::tark::plugin::provider_plugin::Guest for MyPlugin {
    fn name() -> String;
    fn models() -> Vec<String>;
    fn chat(messages: Vec<Message>, model: String) -> Result<ChatResponse, String>;
}
```

## Host Capabilities

Plugins can import these capabilities (if declared in plugin.toml):

### Storage

```rust
tark::plugin::storage::get("key") -> Option<String>
tark::plugin::storage::set("key", "value") -> Result<(), String>
tark::plugin::storage::delete("key") -> Result<(), String>
```

### HTTP

```rust
tark::plugin::http::get(url, headers) -> Result<HttpResponse, HttpError>
tark::plugin::http::post(url, body, headers) -> Result<HttpResponse, HttpError>
```

### Environment

```rust
tark::plugin::env::get("VAR_NAME") -> Option<String>
```

### Logging

```rust
tark::plugin::log::info("message")
tark::plugin::log::warn("message")
tark::plugin::log::error("message")
```

## Security

- Plugins run in a WASM sandbox
- Network access is restricted to declared hosts
- Env var access is restricted to declared variables
- Storage is namespaced per plugin
- Shell access requires explicit approval
```

---

### Task 4.2: Commit Phase 4

**Commands**:
```bash
git add docs/PLUGIN_SDK.md
git commit -m "docs: add plugin SDK documentation

- Document plugin types (auth, tool, provider)
- Document host capabilities (storage, http, env, log)
- Document build process for WASM plugins
- Document security model"
```

---

## Phase 5: Final Integration and Testing

### Task 5.1: Update README.md with plugin section

**Action**: Add a Plugins section to README.md documenting:
- Plugin installation
- Plugin management commands
- Security model

---

### Task 5.2: Add integration tests

**File**: `tests/plugin_system.rs`

```rust
//! Integration tests for the plugin system

use tark::plugins::{PluginManifest, PluginRegistry, PluginType};
use tempfile::TempDir;

#[test]
fn test_manifest_parsing() {
    let toml = r#"
[plugin]
name = "test-plugin"
version = "1.0.0"
type = "auth"
description = "Test plugin"

[capabilities]
storage = true
http = ["api.example.com"]
"#;
    
    let manifest: PluginManifest = toml::from_str(toml).unwrap();
    assert_eq!(manifest.plugin.name, "test-plugin");
    assert_eq!(manifest.plugin_type(), PluginType::Auth);
    assert!(manifest.capabilities.storage);
}

#[test]
fn test_registry_empty() {
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("XDG_DATA_HOME", temp_dir.path());
    
    let registry = PluginRegistry::new().unwrap();
    assert_eq!(registry.all().count(), 0);
}
```

---

### Task 5.3: Commit Phase 5

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add tests/plugin_system.rs
git add README.md
git commit -m "feat(plugins): complete plugin system integration

- Add integration tests for plugin system
- Update README with plugin documentation"
```

---

### Task 5.4: Final Validation and Push

**Commands**:
```bash
# Full build
cargo build --release

# All checks
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

# Push
git push origin main
```

---

## Summary

### Files Created

| File | Purpose |
|------|---------|
| `src/plugins/mod.rs` | Plugin module root |
| `src/plugins/manifest/mod.rs` | plugin.toml parsing |
| `src/plugins/registry/mod.rs` | Plugin discovery and management |
| `src/plugins/host/mod.rs` | WASM runtime |
| `src/plugins/wit/tark.wit` | WIT interface definitions |
| `src/transport/plugin_cli.rs` | CLI commands |
| `docs/PLUGIN_SDK.md` | Developer documentation |
| `tests/plugin_system.rs` | Integration tests |

### Commit Summary

```
feat(plugins): add plugin system foundation
feat(plugins): add WIT interface definitions
feat(cli): add plugin management commands
docs: add plugin SDK documentation
feat(plugins): complete plugin system integration
```

---

## User Experience After Implementation

```bash
# Install a plugin from GitHub
tark plugin add https://github.com/someone/tark-chatgpt-auth

# List installed plugins
tark plugin list
# → chatgpt-auth v1.0.0 (auth) ✓ enabled

# Show plugin details
tark plugin info chatgpt-auth
# → Shows capabilities, paths, etc.

# Disable a plugin
tark plugin disable chatgpt-auth

# Remove a plugin
tark plugin remove chatgpt-auth
```

---

## Next Steps After This Plan

1. **Example plugin**: Build a reference auth plugin
2. **Plugin SDK crate**: Publish tark-plugin-sdk to crates.io
3. **Community index**: Create awesome-tark-plugins repo
4. **Signature verification**: Add plugin signing
