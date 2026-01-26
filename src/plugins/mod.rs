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
//! - **Channel Plugins**: Add messaging channels (Slack, Discord, Signal)
//!
//! # Security
//!
//! Plugins declare required capabilities in `plugin.toml`. The host
//! only grants capabilities that are declared and approved.

#![allow(dead_code)]
#![allow(unused_imports)]

pub mod host;
mod manifest;
mod oauth;
mod registry;

pub use host::{
    AuthCredentials, AuthStatus, ChannelAuthStatus, ChannelInboundMessage, ChannelInfo,
    ChannelSendRequest, ChannelSendResult, ChannelWebhookRequest, ChannelWebhookResponse,
    ChatResponse, ChatUsage, ModelInfo, PluginHost, PluginInstance, ProviderAuthStatus,
    ProviderInfo,
};
pub use manifest::{
    ChannelContribution, OAuthConfig, OAuthFlowType, PluginCapabilities, PluginContributions,
    PluginManifest, PluginType, ProviderContribution,
};
pub use oauth::{run_oauth_flow_for_plugin, PluginOAuthResult};
pub use registry::{InstalledPlugin, PluginRegistry};

/// Current plugin API version
pub const PLUGIN_API_VERSION: &str = "0.2.0";

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
