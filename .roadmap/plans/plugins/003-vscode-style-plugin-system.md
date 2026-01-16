# Plan 003: VS Code-Style Plugin System Architecture

## Overview

Upgrade tark's plugin system from "safe embedded modules" to a "mature extension platform" like VS Code, supporting:
- **Provider plugins** as first-class LLM providers (not just auth token sources)
- **Contribution points** (declare what you add)
- **Activation events** (lazy loading)
- **Stable host API with versioning**

## Current State

✅ WASM sandboxing + capability-based permissions  
✅ WIT contracts for host/guest ABI  
✅ Basic host API (storage, http, env, log)  
✅ Plugin manifest (plugin.toml)  
✅ Git/local install  

❌ Provider plugins not first-class (only auth returns token, core misuses it)  
❌ No contribution points (plugins don't declare what they add)  
❌ No activation events (eager loading)  
❌ No API versioning  
❌ No IDE hooks (LSP, TUI, model picker)  

## Phase 1: Provider Plugins as First-Class Citizens

**Goal**: Plugin-provided LLM providers appear in model picker alongside built-in providers.

### Task 1.1: Extend WIT provider-plugin interface

**File**: `src/plugins/wit/tark.wit`

```wit
/// LLM provider plugin interface (extended)
interface provider-plugin {
    /// Provider metadata
    record provider-info {
        /// Provider ID (e.g., "gemini-oauth")
        id: string,
        /// Display name (e.g., "Gemini (OAuth)")
        display-name: string,
        /// Description
        description: string,
        /// Whether provider requires authentication
        requires-auth: bool,
    }
    
    /// Model info
    record model-info {
        id: string,
        display-name: string,
        context-window: u32,
        supports-streaming: bool,
        supports-tools: bool,
    }
    
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
        finish-reason: option<string>,
    }
    
    /// Stream chunk
    record stream-chunk {
        text: string,
        done: bool,
        usage: option<usage>,
    }
    
    /// Auth status for provider
    enum auth-status {
        not-required,
        authenticated,
        not-authenticated,
        expired,
    }
    
    // === Provider Info ===
    
    /// Get provider metadata
    info: func() -> provider-info;
    
    /// Get available models
    models: func() -> list<model-info>;
    
    // === Authentication ===
    
    /// Get authentication status
    auth-status: func() -> auth-status;
    
    /// Initialize with credentials (JSON)
    auth-init: func(credentials-json: string) -> result<_, string>;
    
    /// Logout
    auth-logout: func() -> result<_, string>;
    
    // === Chat ===
    
    /// Send chat completion (non-streaming)
    chat: func(messages: list<message>, model: string) -> result<chat-response, string>;
    
    /// Send chat completion (streaming) - returns stream ID
    chat-stream-start: func(messages: list<message>, model: string) -> result<u64, string>;
    
    /// Poll stream for next chunk
    chat-stream-poll: func(stream-id: u64) -> result<stream-chunk, string>;
}
```

**Verification**: `cargo build --release` compiles

---

### Task 1.2: Create PluginProviderAdapter

**File**: `src/llm/plugin_provider.rs` (new)

```rust
//! Adapter that wraps a WASM provider plugin as an LlmProvider

use crate::llm::{
    CompletionResult, LlmProvider, LlmResponse, Message, Role,
    StreamCallback, StreamEvent, TokenUsage, ToolDefinition,
    RefactoringSuggestion, CodeIssue,
};
use crate::plugins::{PluginHost, PluginRegistry, PluginType};
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Adapter that implements LlmProvider by calling a WASM plugin
pub struct PluginProviderAdapter {
    plugin_id: String,
    display_name: String,
    model: String,
}

impl PluginProviderAdapter {
    /// Create adapter for a provider plugin
    pub fn new(plugin_id: &str) -> Result<Self> {
        let registry = PluginRegistry::new()?;
        let plugin = registry.get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;
        
        if plugin.manifest.plugin_type() != PluginType::Provider {
            anyhow::bail!("Plugin {} is not a provider plugin", plugin_id);
        }
        
        // Load plugin to get metadata
        let mut host = PluginHost::new()?;
        host.load(plugin)?;
        
        let instance = host.get_mut(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Failed to get plugin instance"))?;
        
        let info = instance.provider_info()?;
        let models = instance.provider_models()?;
        let default_model = models.first()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "default".to_string());
        
        Ok(Self {
            plugin_id: plugin_id.to_string(),
            display_name: info.display_name,
            model: default_model,
        })
    }
    
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }
    
    /// Get fresh plugin instance (plugins are stateless between calls)
    fn get_instance(&self) -> Result<(PluginHost, String)> {
        let registry = PluginRegistry::new()?;
        let plugin = registry.get(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found"))?;
        
        let mut host = PluginHost::new()?;
        host.load(plugin)?;
        
        Ok((host, self.plugin_id.clone()))
    }
}

#[async_trait]
impl LlmProvider for PluginProviderAdapter {
    fn name(&self) -> &str {
        &self.display_name
    }
    
    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let (mut host, plugin_id) = self.get_instance()?;
        let instance = host.get_mut(&plugin_id).unwrap();
        
        // Convert messages to plugin format
        let plugin_messages = messages.iter()
            .map(|m| PluginMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user", 
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }.to_string(),
                content: m.content.as_text().unwrap_or("").to_string(),
            })
            .collect();
        
        let response = instance.provider_chat(plugin_messages, &self.model)?;
        
        Ok(LlmResponse::text(response.text)
            .with_usage(response.usage.map(|u| TokenUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
            })))
    }
    
    // ... implement other required methods with sensible defaults
}
```

**Verification**: 
- `cargo build --release` compiles
- `cargo test plugin_provider` passes

---

### Task 1.3: Register plugin providers in create_provider()

**File**: `src/llm/mod.rs`

Update `create_provider_with_options()` to check for provider plugins:

```rust
pub fn create_provider_with_options(
    name: &str,
    silent: bool,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider>> {
    // First check if it's a plugin provider
    if let Some(provider) = try_plugin_provider(name, model) {
        return Ok(provider);
    }
    
    // Then check built-in providers
    match name.to_lowercase().as_str() {
        "claude" | "anthropic" => { /* ... */ }
        "gemini" | "google" => { /* ... */ }
        // ...
    }
}

/// Try to create a provider from an installed plugin
fn try_plugin_provider(name: &str, model: Option<&str>) -> Option<Box<dyn LlmProvider>> {
    use crate::plugins::{PluginRegistry, PluginType};
    
    let registry = PluginRegistry::new().ok()?;
    
    // Check if there's a provider plugin with this name
    let plugin = registry.get(name)?;
    if plugin.manifest.plugin_type() != PluginType::Provider {
        return None;
    }
    if !plugin.enabled {
        return None;
    }
    
    let mut adapter = PluginProviderAdapter::new(name).ok()?;
    if let Some(m) = model {
        adapter = adapter.with_model(m);
    }
    
    Some(Box::new(adapter))
}
```

**Verification**:
- `cargo test create_provider` passes
- `tark chat -p gemini-oauth` uses plugin (if installed)

---

### Task 1.4: Show plugin providers in model picker

**File**: `src/tui/widgets/model_picker.rs`

Add plugin providers to the provider list:

```rust
fn get_available_providers() -> Vec<ProviderInfo> {
    let mut providers = vec![
        // Built-in providers
        ProviderInfo { id: "claude", name: "Claude", /* ... */ },
        ProviderInfo { id: "openai", name: "OpenAI", /* ... */ },
        // ...
    ];
    
    // Add plugin providers
    if let Ok(registry) = PluginRegistry::new() {
        for plugin in registry.list_enabled() {
            if plugin.manifest.plugin_type() == PluginType::Provider {
                providers.push(ProviderInfo {
                    id: plugin.id().to_string(),
                    name: plugin.manifest.plugin.description.clone(),
                    is_plugin: true,
                });
            }
        }
    }
    
    providers
}
```

**Verification**: Model picker shows plugin providers

---

## Phase 2: Contribution Points

**Goal**: Plugins declare what they contribute via `plugin.toml`.

### Task 2.1: Extend manifest with contributions

**File**: `src/plugins/manifest/mod.rs`

```rust
/// Plugin contributions (VS Code-style)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginContributions {
    /// LLM providers contributed
    #[serde(default)]
    pub providers: Vec<ProviderContribution>,
    
    /// Commands contributed
    #[serde(default)]
    pub commands: Vec<CommandContribution>,
    
    /// Configuration schema
    #[serde(default)]
    pub configuration: Vec<ConfigContribution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderContribution {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub id: String,
    pub title: String,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigContribution {
    pub key: String,
    #[serde(rename = "type")]
    pub value_type: String,
    pub default: Option<toml::Value>,
    pub description: String,
}
```

**Example plugin.toml**:

```toml
[plugin]
name = "gemini-oauth"
version = "0.1.0"
type = "provider"
description = "Gemini with OAuth via Cloud Code Assist API"

[capabilities]
storage = true
http = ["oauth2.googleapis.com", "cloudcode-pa.googleapis.com"]

[contributes]
providers = [
    { id = "gemini-oauth", name = "Gemini (OAuth)", description = "Uses Gemini CLI credentials" }
]
commands = [
    { id = "gemini-oauth.login", title = "Login to Gemini", category = "Authentication" }
]
configuration = [
    { key = "gemini-oauth.model", type = "string", default = "gemini-2.0-flash-exp", description = "Default model" }
]
```

---

## Phase 3: Activation Events

**Goal**: Plugins load lazily based on events.

### Task 3.1: Add activation events to manifest

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginActivation {
    /// Events that trigger plugin activation
    #[serde(default)]
    pub events: Vec<String>,
}
```

**Supported events**:
- `onStartup` - Load immediately
- `onProvider:<id>` - Load when provider is selected
- `onCommand:<id>` - Load when command is invoked
- `onConfiguration:<key>` - Load when config key is accessed

**Example**:

```toml
[activation]
events = ["onProvider:gemini-oauth", "onCommand:gemini-oauth.login"]
```

### Task 3.2: Implement lazy loading in PluginHost

```rust
impl PluginHost {
    /// Load plugin only if needed for event
    pub fn ensure_loaded_for_event(&mut self, event: &str) -> Result<Option<&str>> {
        let registry = PluginRegistry::new()?;
        
        for plugin in registry.list_enabled() {
            if plugin.should_activate_for(event) && !self.is_loaded(&plugin.id()) {
                self.load(&plugin)?;
                return Ok(Some(plugin.id()));
            }
        }
        
        Ok(None)
    }
}
```

---

## Phase 4: API Versioning

**Goal**: Plugins declare required API version; host rejects incompatible plugins.

### Task 4.1: Add API version to manifest

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    // ...existing fields...
    
    /// Required tark plugin API version (semver range)
    #[serde(default = "default_api_version")]
    pub api_version: String,
}

fn default_api_version() -> String {
    "0.1".to_string()
}
```

### Task 4.2: Version check on load

```rust
const PLUGIN_API_VERSION: &str = "0.1.0";

impl PluginHost {
    pub fn load(&mut self, plugin: &InstalledPlugin) -> Result<()> {
        // Check API version compatibility
        let required = &plugin.manifest.plugin.api_version;
        if !is_api_compatible(required, PLUGIN_API_VERSION) {
            anyhow::bail!(
                "Plugin {} requires API version {}, but tark provides {}",
                plugin.id(), required, PLUGIN_API_VERSION
            );
        }
        
        // ... rest of loading
    }
}
```

---

## Execution Order

1. **Phase 1** (Provider Plugins) - Immediate priority
   - Task 1.1: Extend WIT
   - Task 1.2: PluginProviderAdapter  
   - Task 1.3: Register in create_provider()
   - Task 1.4: Model picker integration

2. **Phase 2** (Contributions) - After Phase 1 works
   - Task 2.1: Extend manifest

3. **Phase 3** (Activation) - After Phase 2
   - Task 3.1: Activation events in manifest
   - Task 3.2: Lazy loading

4. **Phase 4** (Versioning) - Can be done in parallel
   - Task 4.1: API version in manifest
   - Task 4.2: Version check

---

## Verification Checklist

After each phase:

- [ ] `cargo build --release` passes
- [ ] `cargo fmt --all -- --check` passes  
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --all-features` passes
- [ ] Manual test: `tark chat -p <plugin-provider>` works

---

## Success Criteria

- [ ] `gemini-oauth` plugin appears in model picker
- [ ] Selecting `gemini-oauth` uses plugin's chat implementation
- [ ] Plugin handles its own auth (Cloud Code Assist API)
- [ ] Core `gemini` provider remains API-key only
- [ ] New provider plugins can be added without changing tark core
