//! WASM plugin host using wasmtime
//!
//! Provides the runtime environment for executing plugins.
//! Host functions allow plugins to access storage, HTTP, environment, and logging.

use super::manifest::PluginCapabilities;
use super::registry::InstalledPlugin;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use wasmtime::*;
use wasmtime_wasi::preview1::WasiP1Ctx;

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

/// Auth status from plugin
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    NotAuthenticated,
    Authenticated,
    Expired,
}

impl PluginInstance {
    // =========================================================================
    // Auth Plugin Interface Methods
    // =========================================================================

    /// Get authentication status from auth plugin
    pub fn auth_status(&mut self) -> Result<AuthStatus> {
        let status_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "status")
            .context("Plugin does not export 'status' function")?;

        let result = status_fn.call(&mut self.store, ())?;
        Ok(match result {
            0 => AuthStatus::NotAuthenticated,
            1 => AuthStatus::Authenticated,
            2 => AuthStatus::Expired,
            _ => AuthStatus::NotAuthenticated,
        })
    }

    /// Get access token from auth plugin
    pub fn auth_get_token(&mut self) -> Result<String> {
        // Allocate buffer in WASM memory for return value
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let buffer_ptr = alloc_fn.call(&mut self.store, 4096)?;

        // Call get_token
        let get_token_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "get_token")
            .context("Plugin does not export 'get_token' function")?;

        let len = get_token_fn.call(&mut self.store, buffer_ptr)?;
        if len < 0 {
            anyhow::bail!("Failed to get token from plugin");
        }

        // Read token from WASM memory
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let token_bytes = &data[buffer_ptr as usize..(buffer_ptr + len) as usize];
        let token = String::from_utf8(token_bytes.to_vec())?;

        Ok(token)
    }

    /// Get API endpoint from auth plugin
    pub fn auth_get_endpoint(&mut self) -> Result<String> {
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let buffer_ptr = alloc_fn.call(&mut self.store, 1024)?;

        let get_endpoint_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "get_endpoint")
            .context("Plugin does not export 'get_endpoint' function")?;

        let len = get_endpoint_fn.call(&mut self.store, buffer_ptr)?;
        if len < 0 {
            anyhow::bail!("Failed to get endpoint from plugin");
        }

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let endpoint_bytes = &data[buffer_ptr as usize..(buffer_ptr + len) as usize];
        let endpoint = String::from_utf8(endpoint_bytes.to_vec())?;

        Ok(endpoint)
    }

    /// Initialize auth plugin with credentials JSON
    pub fn auth_init_with_credentials(&mut self, credentials_json: &str) -> Result<()> {
        // Allocate and copy credentials to WASM memory
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let creds_bytes = credentials_json.as_bytes();
        let creds_ptr = alloc_fn.call(&mut self.store, creds_bytes.len() as i32)?;

        // Write credentials to WASM memory
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        memory.write(&mut self.store, creds_ptr as usize, creds_bytes)?;

        // Call init_with_credentials
        let init_fn = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "init_with_credentials")
            .context("Plugin does not export 'init_with_credentials' function")?;

        let result = init_fn.call(&mut self.store, (creds_ptr, creds_bytes.len() as i32))?;
        if result < 0 {
            anyhow::bail!("Failed to initialize plugin with credentials");
        }

        Ok(())
    }

    /// Logout from auth plugin
    pub fn auth_logout(&mut self) -> Result<()> {
        let logout_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "logout")
            .context("Plugin does not export 'logout' function")?;

        logout_fn.call(&mut self.store, ())?;
        Ok(())
    }

    /// Get display name from auth plugin
    pub fn auth_display_name(&mut self) -> Result<String> {
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let buffer_ptr = alloc_fn.call(&mut self.store, 256)?;

        let display_name_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "display_name")
            .context("Plugin does not export 'display_name' function")?;

        let len = display_name_fn.call(&mut self.store, buffer_ptr)?;
        if len < 0 {
            anyhow::bail!("Failed to get display name from plugin");
        }

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let name_bytes = &data[buffer_ptr as usize..(buffer_ptr + len) as usize];
        let name = String::from_utf8(name_bytes.to_vec())?;

        Ok(name)
    }

    // =========================================================================
    // Provider Plugin Interface Methods
    // =========================================================================

    /// Get provider info (JSON)
    pub fn provider_info(&mut self) -> Result<ProviderInfo> {
        let json = self.call_string_return_fn("provider_info")?;
        serde_json::from_str(&json).context("Failed to parse provider info JSON")
    }

    /// Get available models (JSON array)
    pub fn provider_models(&mut self) -> Result<Vec<ModelInfo>> {
        let json = self.call_string_return_fn("provider_models")?;
        serde_json::from_str(&json).context("Failed to parse models JSON")
    }

    /// Get provider auth status
    pub fn provider_auth_status(&mut self) -> Result<ProviderAuthStatus> {
        let status_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "provider_auth_status")
            .context("Plugin does not export 'provider_auth_status' function")?;

        let result = status_fn.call(&mut self.store, ())?;
        Ok(match result {
            0 => ProviderAuthStatus::NotRequired,
            1 => ProviderAuthStatus::Authenticated,
            2 => ProviderAuthStatus::NotAuthenticated,
            3 => ProviderAuthStatus::Expired,
            _ => ProviderAuthStatus::NotAuthenticated,
        })
    }

    /// Initialize provider with credentials (JSON)
    pub fn provider_auth_init(&mut self, credentials_json: &str) -> Result<()> {
        self.call_string_param_fn("provider_auth_init", credentials_json)
    }

    /// Logout from provider
    pub fn provider_auth_logout(&mut self) -> Result<()> {
        let logout_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "provider_auth_logout")
            .context("Plugin does not export 'provider_auth_logout' function")?;

        let result = logout_fn.call(&mut self.store, ())?;
        if result < 0 {
            anyhow::bail!("Provider logout failed");
        }
        Ok(())
    }

    /// Send chat completion request (non-streaming)
    /// messages_json: JSON array of {role, content} objects
    /// Returns: JSON response with {text, usage}
    pub fn provider_chat(&mut self, messages_json: &str, model: &str) -> Result<ChatResponse> {
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        // Allocate and write messages JSON
        let msgs_bytes = messages_json.as_bytes();
        let msgs_ptr = alloc_fn.call(&mut self.store, msgs_bytes.len() as i32)?;
        {
            let memory = self
                .instance
                .get_memory(&mut self.store, "memory")
                .context("Plugin has no memory export")?;
            memory.write(&mut self.store, msgs_ptr as usize, msgs_bytes)?;
        }

        // Allocate and write model string
        let model_bytes = model.as_bytes();
        let model_ptr = alloc_fn.call(&mut self.store, model_bytes.len() as i32)?;
        {
            let memory = self
                .instance
                .get_memory(&mut self.store, "memory")
                .context("Plugin has no memory export")?;
            memory.write(&mut self.store, model_ptr as usize, model_bytes)?;
        }

        // Allocate return buffer
        let ret_ptr = alloc_fn.call(&mut self.store, 65536)?; // 64KB for response

        // Call provider_chat(msgs_ptr, msgs_len, model_ptr, model_len, ret_ptr) -> len
        let chat_fn = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut self.store, "provider_chat")
            .context("Plugin does not export 'provider_chat' function")?;

        let len = chat_fn.call(
            &mut self.store,
            (
                msgs_ptr,
                msgs_bytes.len() as i32,
                model_ptr,
                model_bytes.len() as i32,
                ret_ptr,
            ),
        )?;

        if len < 0 {
            anyhow::bail!("Provider chat failed with error code: {}", len);
        }

        // Read response JSON
        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let response_bytes = &data[ret_ptr as usize..(ret_ptr + len) as usize];
        let response_json = String::from_utf8(response_bytes.to_vec())?;

        serde_json::from_str(&response_json).context("Failed to parse chat response JSON")
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Call a function that returns a string (allocates buffer, calls fn, reads result)
    fn call_string_return_fn(&mut self, fn_name: &str) -> Result<String> {
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let buffer_ptr = alloc_fn.call(&mut self.store, 8192)?;

        let target_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, fn_name)
            .with_context(|| format!("Plugin does not export '{}' function", fn_name))?;

        let len = target_fn.call(&mut self.store, buffer_ptr)?;
        if len < 0 {
            anyhow::bail!("Function {} returned error code: {}", fn_name, len);
        }

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let result_bytes = &data[buffer_ptr as usize..(buffer_ptr + len) as usize];
        String::from_utf8(result_bytes.to_vec()).context("Invalid UTF-8 in result")
    }

    /// Call a function that takes a string parameter
    fn call_string_param_fn(&mut self, fn_name: &str, param: &str) -> Result<()> {
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut self.store, "alloc")
            .context("Plugin does not export 'alloc' function")?;

        let param_bytes = param.as_bytes();
        let param_ptr = alloc_fn.call(&mut self.store, param_bytes.len() as i32)?;

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        memory.write(&mut self.store, param_ptr as usize, param_bytes)?;

        let target_fn = self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, fn_name)
            .with_context(|| format!("Plugin does not export '{}' function", fn_name))?;

        let result = target_fn.call(&mut self.store, (param_ptr, param_bytes.len() as i32))?;
        if result < 0 {
            anyhow::bail!("Function {} failed with error code: {}", fn_name, result);
        }

        Ok(())
    }
}

// =============================================================================
// Provider Plugin Types
// =============================================================================

/// Provider metadata from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requires_auth: bool,
}

/// Model info from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub context_window: u32,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_tools: bool,
}

/// Provider auth status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderAuthStatus {
    NotRequired,
    Authenticated,
    NotAuthenticated,
    Expired,
}

/// Chat response from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatResponse {
    pub text: String,
    #[serde(default)]
    pub usage: Option<ChatUsage>,
    #[serde(default)]
    pub finish_reason: Option<String>,
}

/// Token usage from chat response
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChatUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// State passed to plugin host functions
pub struct PluginState {
    /// Plugin ID
    pub plugin_id: String,
    /// Plugin data directory for storage
    pub data_dir: PathBuf,
    /// Capabilities declared by plugin
    pub capabilities: PluginCapabilities,
    /// HTTP client for making requests
    pub http_client: reqwest::blocking::Client,
    /// In-memory key-value storage (persisted to disk on write)
    pub storage: HashMap<String, String>,
    /// Allowed environment variables (from capabilities)
    pub allowed_env_vars: Vec<String>,
    /// Allowed HTTP domains (from capabilities)
    pub allowed_http_domains: Vec<String>,
    /// WASI context for standard library functions
    pub wasi_ctx: WasiP1Ctx,
}

impl PluginState {
    /// Create new plugin state
    pub fn new(plugin_id: &str, data_dir: PathBuf, capabilities: PluginCapabilities) -> Self {
        // Load existing storage from disk
        let storage = Self::load_storage(&data_dir).unwrap_or_default();

        // Extract allowed domains and env vars from capabilities
        let allowed_http_domains = capabilities.http.clone();
        let allowed_env_vars = capabilities.env.clone();

        // Create WASI context with limited capabilities
        let wasi_ctx = wasmtime_wasi::WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build_p1();

        Self {
            plugin_id: plugin_id.to_string(),
            data_dir,
            capabilities,
            http_client: reqwest::blocking::Client::new(),
            storage,
            allowed_env_vars,
            allowed_http_domains,
            wasi_ctx,
        }
    }

    /// Load storage from disk
    fn load_storage(data_dir: &std::path::Path) -> Result<HashMap<String, String>> {
        let storage_file = data_dir.join("storage.json");
        if storage_file.exists() {
            let content = std::fs::read_to_string(&storage_file)?;
            let storage: HashMap<String, String> = serde_json::from_str(&content)?;
            Ok(storage)
        } else {
            Ok(HashMap::new())
        }
    }

    /// Save storage to disk
    pub fn save_storage(&self) -> Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        let storage_file = self.data_dir.join("storage.json");
        let content = serde_json::to_string_pretty(&self.storage)?;
        std::fs::write(&storage_file, content)?;
        Ok(())
    }

    /// Check if HTTP domain is allowed
    pub fn is_http_allowed(&self, url: &str) -> bool {
        if self.allowed_http_domains.is_empty() {
            return false; // No network access if no domains declared
        }

        // Parse URL to get domain
        if let Ok(parsed) = url::Url::parse(url) {
            if let Some(host) = parsed.host_str() {
                return self
                    .allowed_http_domains
                    .iter()
                    .any(|allowed| host == allowed || host.ends_with(&format!(".{}", allowed)));
            }
        }
        false
    }

    /// Check if env var access is allowed
    pub fn is_env_allowed(&self, name: &str) -> bool {
        self.allowed_env_vars.iter().any(|v| v == name || v == "*")
    }
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
        let config = Config::new();
        // Note: We use sync mode for simplicity. Async mode requires async instantiation.

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
        let state = PluginState::new(
            plugin.id(),
            plugin.data_dir(),
            plugin.manifest.capabilities.clone(),
        );

        let mut store = Store::new(&self.engine, state);

        // Create linker with host functions
        let mut linker = Linker::new(&self.engine);

        // Add WASI functions first (required by wasm32-wasip1 target)
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state: &mut PluginState| {
            &mut state.wasi_ctx
        })?;

        // Add our custom host functions
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
        // =========================================================================
        // Storage Functions
        // =========================================================================

        // storage.get(key: string) -> option<string>
        // Uses JSON encoding: returns "" for None, JSON string for Some
        linker.func_wrap(
            "tark:storage",
            "get",
            |mut caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32, ret_ptr: i32| {
                let key = match read_string(&mut caller, key_ptr, key_len) {
                    Ok(k) => k,
                    Err(_) => return -1i32,
                };

                let result = caller.data().storage.get(&key).cloned();

                // Write result back to WASM memory
                let response: String = result.unwrap_or_default();

                write_string(&mut caller, ret_ptr, &response).unwrap_or(-1)
            },
        )?;

        // storage.set(key: string, value: string) -> result<_, string>
        linker.func_wrap(
            "tark:storage",
            "set",
            |mut caller: Caller<'_, PluginState>,
             key_ptr: i32,
             key_len: i32,
             val_ptr: i32,
             val_len: i32|
             -> i32 {
                let key = match read_string(&mut caller, key_ptr, key_len) {
                    Ok(k) => k,
                    Err(_) => return -1,
                };

                let value = match read_string(&mut caller, val_ptr, val_len) {
                    Ok(v) => v,
                    Err(_) => return -1,
                };

                let plugin_id = caller.data().plugin_id.clone();
                tracing::debug!("[plugin:{}] storage.set({}, ...)", plugin_id, key);

                caller.data_mut().storage.insert(key, value);

                // Persist to disk
                if let Err(e) = caller.data().save_storage() {
                    tracing::error!("[plugin:{}] Failed to save storage: {}", plugin_id, e);
                    return -1;
                }

                0 // Success
            },
        )?;

        // storage.delete(key: string) -> result<_, string>
        linker.func_wrap(
            "tark:storage",
            "delete",
            |mut caller: Caller<'_, PluginState>, key_ptr: i32, key_len: i32| -> i32 {
                let key = match read_string(&mut caller, key_ptr, key_len) {
                    Ok(k) => k,
                    Err(_) => return -1,
                };

                let plugin_id = caller.data().plugin_id.clone();
                tracing::debug!("[plugin:{}] storage.delete({})", plugin_id, key);

                caller.data_mut().storage.remove(&key);

                if let Err(e) = caller.data().save_storage() {
                    tracing::error!("[plugin:{}] Failed to save storage: {}", plugin_id, e);
                    return -1;
                }

                0
            },
        )?;

        // =========================================================================
        // HTTP Functions
        // =========================================================================

        // http.get(url: string, headers: string) -> i32 (bytes written or error)
        // Response body written to ret_ptr
        linker.func_wrap(
            "tark:http",
            "get",
            |mut caller: Caller<'_, PluginState>,
             url_ptr: i32,
             url_len: i32,
             headers_ptr: i32,
             headers_len: i32,
             ret_ptr: i32|
             -> i32 {
                let url = match read_string(&mut caller, url_ptr, url_len) {
                    Ok(u) => u,
                    Err(_) => return -1,
                };

                let headers_json = match read_string(&mut caller, headers_ptr, headers_len) {
                    Ok(h) => h,
                    Err(_) => return -1,
                };

                let plugin_id = caller.data().plugin_id.clone();

                // Check if domain is allowed
                if !caller.data().is_http_allowed(&url) {
                    tracing::warn!(
                        "[plugin:{}] HTTP GET blocked - domain not in allowed list: {}",
                        plugin_id,
                        url
                    );
                    return -2; // Permission denied
                }

                tracing::debug!("[plugin:{}] http.get({})", plugin_id, url);

                // Clone client to release immutable borrow
                let client = caller.data().http_client.clone();
                let response_str = http_get_impl(&client, &url, &headers_json);

                write_string(&mut caller, ret_ptr, &response_str).unwrap_or(-1)
            },
        )?;

        // http.post(url: string, body: string, headers: string) -> i32
        // Response body written to ret_ptr
        linker.func_wrap(
            "tark:http",
            "post",
            |mut caller: Caller<'_, PluginState>,
             url_ptr: i32,
             url_len: i32,
             body_ptr: i32,
             body_len: i32,
             headers_ptr: i32,
             headers_len: i32,
             ret_ptr: i32|
             -> i32 {
                let url = match read_string(&mut caller, url_ptr, url_len) {
                    Ok(u) => u,
                    Err(_) => return -1,
                };

                let body = match read_string(&mut caller, body_ptr, body_len) {
                    Ok(b) => b,
                    Err(_) => return -1,
                };

                let headers_json = match read_string(&mut caller, headers_ptr, headers_len) {
                    Ok(h) => h,
                    Err(_) => return -1,
                };

                let plugin_id = caller.data().plugin_id.clone();

                // Check if domain is allowed
                if !caller.data().is_http_allowed(&url) {
                    tracing::warn!(
                        "[plugin:{}] HTTP POST blocked - domain not in allowed list: {}",
                        plugin_id,
                        url
                    );
                    return -2;
                }

                tracing::debug!("[plugin:{}] http.post({})", plugin_id, url);

                let client = caller.data().http_client.clone();
                let response_str = http_post_impl(&client, &url, body, &headers_json);

                write_string(&mut caller, ret_ptr, &response_str).unwrap_or(-1)
            },
        )?;

        // =========================================================================
        // Environment Functions
        // =========================================================================

        // env.get(name: string) -> option<string>
        linker.func_wrap(
            "tark:env",
            "get",
            |mut caller: Caller<'_, PluginState>,
             name_ptr: i32,
             name_len: i32,
             ret_ptr: i32|
             -> i32 {
                let name = match read_string(&mut caller, name_ptr, name_len) {
                    Ok(n) => n,
                    Err(_) => return -1,
                };

                let plugin_id = caller.data().plugin_id.clone();

                // Check if env var access is allowed
                if !caller.data().is_env_allowed(&name) {
                    tracing::warn!(
                        "[plugin:{}] env.get blocked - {} not in allowed list",
                        plugin_id,
                        name
                    );
                    return -2;
                }

                let value = std::env::var(&name).unwrap_or_default();
                write_string(&mut caller, ret_ptr, &value).unwrap_or(-1)
            },
        )?;

        // =========================================================================
        // Logging Functions
        // =========================================================================

        linker.func_wrap(
            "tark:log",
            "debug",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                if let Ok(msg) = read_string(&mut caller, msg_ptr, msg_len) {
                    let plugin_id = &caller.data().plugin_id;
                    tracing::debug!("[plugin:{}] {}", plugin_id, msg);
                }
            },
        )?;

        linker.func_wrap(
            "tark:log",
            "info",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                if let Ok(msg) = read_string(&mut caller, msg_ptr, msg_len) {
                    let plugin_id = &caller.data().plugin_id;
                    tracing::info!("[plugin:{}] {}", plugin_id, msg);
                }
            },
        )?;

        linker.func_wrap(
            "tark:log",
            "warn",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                if let Ok(msg) = read_string(&mut caller, msg_ptr, msg_len) {
                    let plugin_id = &caller.data().plugin_id;
                    tracing::warn!("[plugin:{}] {}", plugin_id, msg);
                }
            },
        )?;

        linker.func_wrap(
            "tark:log",
            "error",
            |mut caller: Caller<'_, PluginState>, msg_ptr: i32, msg_len: i32| {
                if let Ok(msg) = read_string(&mut caller, msg_ptr, msg_len) {
                    let plugin_id = &caller.data().plugin_id;
                    tracing::error!("[plugin:{}] {}", plugin_id, msg);
                }
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

// =============================================================================
// Helper functions for WASM memory access
// =============================================================================

/// Read a string from WASM memory
fn read_string(caller: &mut Caller<'_, PluginState>, ptr: i32, len: i32) -> Result<String> {
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow::anyhow!("Plugin has no memory export"))?;

    let data = memory.data(&*caller);
    let start = ptr as usize;
    let end = start + len as usize;

    if end > data.len() {
        anyhow::bail!("Memory access out of bounds");
    }

    let bytes = &data[start..end];
    String::from_utf8(bytes.to_vec()).context("Invalid UTF-8 in string")
}

/// Write a string to WASM memory, returns bytes written
fn write_string(caller: &mut Caller<'_, PluginState>, ptr: i32, s: &str) -> Result<i32> {
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or_else(|| anyhow::anyhow!("Plugin has no memory export"))?;

    let bytes = s.as_bytes();
    let data = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + bytes.len();

    if end > data.len() {
        anyhow::bail!("Memory access out of bounds");
    }

    data[start..end].copy_from_slice(bytes);
    Ok(bytes.len() as i32)
}

// =============================================================================
// HTTP helper functions (outside wasmtime closures for better type inference)
// =============================================================================

/// Perform HTTP GET request and return JSON response string
fn http_get_impl(client: &reqwest::blocking::Client, url: &str, headers_json: &str) -> String {
    let headers: Vec<(String, String)> = serde_json::from_str(headers_json).unwrap_or_default();

    let mut request = client.get(url);
    for (name, value) in headers {
        request = request.header(&name, &value);
    }

    match request.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut resp_headers: Vec<(String, String)> = Vec::new();
            for (k, v) in resp.headers().iter() {
                resp_headers.push((
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                ));
            }
            let body = resp.text().unwrap_or_default();

            // Use proper JSON serialization for all fields
            serde_json::json!({
                "status": status,
                "headers": resp_headers,
                "body": body
            }).to_string()
        }
        Err(e) => {
            serde_json::json!({"error": e.to_string()}).to_string()
        }
    }
}

/// Perform HTTP POST request and return JSON response string
fn http_post_impl(
    client: &reqwest::blocking::Client,
    url: &str,
    body: String,
    headers_json: &str,
) -> String {
    let headers: Vec<(String, String)> = serde_json::from_str(headers_json).unwrap_or_default();

    let mut request = client.post(url).body(body);
    for (name, value) in headers {
        request = request.header(&name, &value);
    }

    match request.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let mut resp_headers: Vec<(String, String)> = Vec::new();
            for (k, v) in resp.headers().iter() {
                resp_headers.push((
                    k.as_str().to_string(),
                    v.to_str().unwrap_or_default().to_string(),
                ));
            }
            let body = resp.text().unwrap_or_default();

            // Use proper JSON serialization for all fields
            serde_json::json!({
                "status": status,
                "headers": resp_headers,
                "body": body
            }).to_string()
        }
        Err(e) => {
            serde_json::json!({"error": e.to_string()}).to_string()
        }
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

    #[test]
    fn test_http_domain_check() {
        let caps = PluginCapabilities {
            http: vec!["googleapis.com".to_string(), "example.com".to_string()],
            ..Default::default()
        };

        let state = PluginState::new("test", PathBuf::from("/tmp"), caps);

        assert!(state.is_http_allowed("https://oauth2.googleapis.com/token"));
        assert!(state.is_http_allowed("https://cloudcode-pa.googleapis.com/v1"));
        assert!(state.is_http_allowed("https://example.com/api"));
        assert!(!state.is_http_allowed("https://evil.com/steal"));
        assert!(!state.is_http_allowed("https://notgoogleapis.com/fake"));
    }

    #[test]
    fn test_env_var_check() {
        let caps = PluginCapabilities {
            env: vec!["GEMINI_API_KEY".to_string(), "HOME".to_string()],
            ..Default::default()
        };

        let state = PluginState::new("test", PathBuf::from("/tmp"), caps);

        assert!(state.is_env_allowed("GEMINI_API_KEY"));
        assert!(state.is_env_allowed("HOME"));
        assert!(!state.is_env_allowed("SECRET_KEY"));
        assert!(!state.is_env_allowed("PATH"));
    }
}
