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

#![allow(dead_code)]
#![allow(unused_imports)]

mod host;
mod manifest;
mod registry;

pub use host::PluginHost;
pub use manifest::{PluginCapabilities, PluginManifest, PluginType};
pub use registry::{InstalledPlugin, PluginRegistry};

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
