//! WASM plugin host using wasmtime
//!
//! Provides the runtime environment for executing plugins.
//! Host functions allow plugins to access storage, HTTP, environment, and logging.
//!
//! # Security Architecture
//!
//! Plugins are sandboxed using WASM isolation, but we add additional protections:
//! - **Epoch interruption**: Prevents infinite loops (configurable timeout)
//! - **HTTP timeouts**: All HTTP requests have configurable timeouts
//! - **Panic catching**: Plugin failures don't crash the host application
//! - **Capability restrictions**: Plugins declare required capabilities in manifest

use super::manifest::PluginCapabilities;
use super::registry::InstalledPlugin;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use wasmtime::*;
use wasmtime_wasi::preview1::WasiP1Ctx;

/// Default timeout for plugin HTTP requests (30 seconds)
const HTTP_TIMEOUT_SECS: u64 = 30;

/// Default epoch deadline ticks (roughly 30 seconds at default interval)
const DEFAULT_EPOCH_DEADLINE: u64 = 30_000;

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
    // Safety Wrappers
    // =========================================================================

    /// Safely execute a plugin operation, catching any panics
    ///
    /// This ensures that a misbehaving plugin cannot crash the host application.
    /// Panics are converted to errors and logged.
    fn safe_call<F, T>(&mut self, op_name: &str, f: F) -> Result<T>
    where
        F: FnOnce(&mut Self) -> Result<T>,
    {
        // Reset epoch deadline before each call to ensure fresh timeout
        self.store.set_epoch_deadline(DEFAULT_EPOCH_DEADLINE);

        // Wrap in catch_unwind to prevent panics from crashing the host
        let result = catch_unwind(AssertUnwindSafe(|| f(self)));

        match result {
            Ok(inner_result) => inner_result,
            Err(panic_info) => {
                // Extract panic message if possible
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };

                tracing::error!(
                    "[plugin:{}] Plugin panicked during '{}': {}",
                    self.id,
                    op_name,
                    panic_msg
                );

                anyhow::bail!(
                    "Plugin '{}' crashed during '{}': {}. \
                    This is a plugin bug, not a tark bug.",
                    self.id,
                    op_name,
                    panic_msg
                )
            }
        }
    }

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

    /// Get auth credentials from auth-only plugin
    ///
    /// Auth-only plugins export this function instead of provider_chat.
    /// The returned credentials are used to create a native provider instance
    /// (e.g., GeminiProvider with Cloud Code Assist mode).
    ///
    /// Returns Err if the plugin doesn't export this function.
    ///
    /// # Safety
    /// This call is wrapped with panic protection - plugin crashes won't crash tark.
    pub fn provider_auth_credentials(&mut self) -> Result<AuthCredentials> {
        self.safe_call("provider_auth_credentials", |this| {
            let json = this.call_string_return_fn("provider_auth_credentials")?;
            serde_json::from_str(&json).context("Failed to parse auth credentials JSON")
        })
    }

    /// Check if plugin exports the auth-only interface
    ///
    /// Returns true if the plugin exports provider_auth_credentials,
    /// indicating it's an auth-only plugin that delegates to native providers.
    pub fn has_auth_credentials_interface(&mut self) -> bool {
        self.instance
            .get_typed_func::<i32, i32>(&mut self.store, "provider_auth_credentials")
            .is_ok()
    }

    /// Process OAuth tokens through plugin callback (optional)
    ///
    /// If the plugin exports `auth_process_tokens`, this calls it with the tokens JSON
    /// and returns the processed result. This allows plugins to:
    /// - Extract additional data (e.g., account_id from JWT)
    /// - Add metadata or computed fields
    /// - Validate tokens before storage
    ///
    /// Returns None if plugin doesn't export auth_process_tokens function.
    pub fn auth_process_tokens(&mut self, tokens_json: &str) -> Result<Option<String>> {
        // Check if function exists
        if self
            .instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "auth_process_tokens")
            .is_err()
        {
            return Ok(None);
        }

        self.safe_call("auth_process_tokens", |this| {
            // Allocate buffer for input tokens
            let alloc_fn = this
                .instance
                .get_typed_func::<i32, i32>(&mut this.store, "alloc")
                .context("Plugin does not export 'alloc' function")?;

            let tokens_bytes = tokens_json.as_bytes();
            let tokens_ptr = alloc_fn.call(&mut this.store, tokens_bytes.len() as i32)?;

            // Write tokens to WASM memory
            let memory = this
                .instance
                .get_memory(&mut this.store, "memory")
                .context("Plugin has no memory export")?;

            let data = memory.data_mut(&mut this.store);
            let dest = &mut data[tokens_ptr as usize..(tokens_ptr as usize + tokens_bytes.len())];
            dest.copy_from_slice(tokens_bytes);

            // Allocate buffer for output
            let output_ptr = alloc_fn.call(&mut this.store, 8192)?; // 8KB buffer

            // Call auth_process_tokens(tokens_ptr, tokens_len, output_ptr) -> output_len
            let process_fn = this
                .instance
                .get_typed_func::<(i32, i32, i32), i32>(&mut this.store, "auth_process_tokens")?;

            let output_len = process_fn.call(
                &mut this.store,
                (tokens_ptr, tokens_bytes.len() as i32, output_ptr),
            )?;

            if output_len < 0 {
                anyhow::bail!("Plugin auth_process_tokens returned error");
            }

            // Read processed tokens from WASM memory
            let memory = this
                .instance
                .get_memory(&mut this.store, "memory")
                .context("Plugin has no memory export")?;

            let data = memory.data(&this.store);
            let processed_bytes = &data[output_ptr as usize..(output_ptr + output_len) as usize];
            let processed_json = String::from_utf8(processed_bytes.to_vec())?;

            Ok(Some(processed_json))
        })
    }

    /// Send chat completion request (non-streaming)
    /// messages_json: JSON array of {role, content} objects
    /// Returns: JSON response with {text, usage}
    ///
    /// # Safety
    /// This call is wrapped with panic protection - plugin crashes won't crash tark.
    pub fn provider_chat(&mut self, messages_json: &str, model: &str) -> Result<ChatResponse> {
        let messages_json = messages_json.to_string();
        let model = model.to_string();

        self.safe_call("provider_chat", move |this| {
            let alloc_fn = this
                .instance
                .get_typed_func::<i32, i32>(&mut this.store, "alloc")
                .context("Plugin does not export 'alloc' function")?;

            // Allocate and write messages JSON
            let msgs_bytes = messages_json.as_bytes();
            let msgs_ptr = alloc_fn.call(&mut this.store, msgs_bytes.len() as i32)?;
            {
                let memory = this
                    .instance
                    .get_memory(&mut this.store, "memory")
                    .context("Plugin has no memory export")?;
                memory.write(&mut this.store, msgs_ptr as usize, msgs_bytes)?;
            }

            // Allocate and write model string
            let model_bytes = model.as_bytes();
            let model_ptr = alloc_fn.call(&mut this.store, model_bytes.len() as i32)?;
            {
                let memory = this
                    .instance
                    .get_memory(&mut this.store, "memory")
                    .context("Plugin has no memory export")?;
                memory.write(&mut this.store, model_ptr as usize, model_bytes)?;
            }

            // Allocate return buffer
            let ret_ptr = alloc_fn.call(&mut this.store, 65536)?; // 64KB for response

            // Call provider_chat(msgs_ptr, msgs_len, model_ptr, model_len, ret_ptr) -> len
            let chat_fn = this
                .instance
                .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut this.store, "provider_chat")
                .context("Plugin does not export 'provider_chat' function")?;

            let len = chat_fn.call(
                &mut this.store,
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
            let memory = this
                .instance
                .get_memory(&mut this.store, "memory")
                .context("Plugin has no memory export")?;

            let data = memory.data(&this.store);
            let response_bytes = &data[ret_ptr as usize..(ret_ptr + len) as usize];
            let response_json = String::from_utf8(response_bytes.to_vec())?;

            serde_json::from_str(&response_json).context("Failed to parse chat response JSON")
        })
    }

    // =========================================================================
    // Channel Plugin Interface Methods
    // =========================================================================

    /// Get channel info (JSON)
    pub fn channel_info(&mut self) -> Result<ChannelInfo> {
        let json = self.call_string_return_fn("channel_info")?;
        serde_json::from_str(&json).context("Failed to parse channel info JSON")
    }

    /// Start channel plugin
    pub fn channel_start(&mut self) -> Result<()> {
        let start_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "channel_start")
            .context("Plugin does not export 'channel_start' function")?;

        let result = start_fn.call(&mut self.store, ())?;
        if result < 0 {
            anyhow::bail!("Channel start failed");
        }
        Ok(())
    }

    /// Stop channel plugin
    pub fn channel_stop(&mut self) -> Result<()> {
        let stop_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "channel_stop")
            .context("Plugin does not export 'channel_stop' function")?;

        let result = stop_fn.call(&mut self.store, ())?;
        if result < 0 {
            anyhow::bail!("Channel stop failed");
        }
        Ok(())
    }

    /// Get channel auth status
    pub fn channel_auth_status(&mut self) -> Result<ChannelAuthStatus> {
        let status_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "channel_auth_status")
            .context("Plugin does not export 'channel_auth_status' function")?;

        let result = status_fn.call(&mut self.store, ())?;
        Ok(match result {
            0 => ChannelAuthStatus::NotRequired,
            1 => ChannelAuthStatus::Authenticated,
            2 => ChannelAuthStatus::NotAuthenticated,
            3 => ChannelAuthStatus::Expired,
            _ => ChannelAuthStatus::NotAuthenticated,
        })
    }

    /// Initialize channel with credentials (JSON)
    pub fn channel_auth_init(&mut self, credentials_json: &str) -> Result<()> {
        self.call_string_param_fn("channel_auth_init", credentials_json)
    }

    /// Logout from channel
    pub fn channel_auth_logout(&mut self) -> Result<()> {
        let logout_fn = self
            .instance
            .get_typed_func::<(), i32>(&mut self.store, "channel_auth_logout")
            .context("Plugin does not export 'channel_auth_logout' function")?;

        let result = logout_fn.call(&mut self.store, ())?;
        if result < 0 {
            anyhow::bail!("Channel logout failed");
        }
        Ok(())
    }

    /// Get channel widget state (JSON)
    pub fn channel_widget_state(&mut self) -> Result<String> {
        self.call_string_return_fn("channel_widget_state")
    }

    /// Handle webhook request (JSON)
    pub fn channel_handle_webhook(
        &mut self,
        request: &ChannelWebhookRequest,
    ) -> Result<ChannelWebhookResponse> {
        let json = serde_json::to_string(request)?;
        let response_json = self.call_string_param_return_fn("channel_handle_webhook", &json)?;
        serde_json::from_str(&response_json).context("Failed to parse webhook response JSON")
    }

    /// Handle gateway event payload (JSON)
    pub fn channel_handle_gateway_event(
        &mut self,
        payload_json: &str,
    ) -> Result<Vec<ChannelInboundMessage>> {
        let response_json =
            self.call_string_param_return_fn("channel_handle_gateway_event", payload_json)?;
        serde_json::from_str(&response_json)
            .context("Failed to parse gateway inbound messages JSON")
    }

    /// Poll channel for inbound messages (JSON)
    pub fn channel_poll(&mut self) -> Result<Vec<ChannelInboundMessage>> {
        let response_json = self.call_string_return_fn("channel_poll")?;
        serde_json::from_str(&response_json).context("Failed to parse channel poll JSON")
    }

    /// Send outbound message (JSON)
    pub fn channel_send(&mut self, request: &ChannelSendRequest) -> Result<ChannelSendResult> {
        let json = serde_json::to_string(request)?;
        let response_json = self.call_string_param_return_fn("channel_send", &json)?;
        serde_json::from_str(&response_json).context("Failed to parse send result JSON")
    }

    /// Check if channel auth init is supported
    pub fn has_channel_auth_init(&mut self) -> bool {
        self.instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "channel_auth_init")
            .is_ok()
    }

    /// Check if channel widget state is supported
    pub fn has_channel_widget_state(&mut self) -> bool {
        self.instance
            .get_typed_func::<i32, i32>(&mut self.store, "channel_widget_state")
            .is_ok()
    }

    /// Check if gateway event handler is supported
    pub fn has_channel_gateway_handler(&mut self) -> bool {
        self.instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, "channel_handle_gateway_event")
            .is_ok()
    }

    /// Check if channel poll is supported
    pub fn has_channel_poll(&mut self) -> bool {
        self.instance
            .get_typed_func::<i32, i32>(&mut self.store, "channel_poll")
            .is_ok()
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

    /// Call a function that takes a string param and returns a string
    fn call_string_param_return_fn(&mut self, fn_name: &str, param: &str) -> Result<String> {
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

        let output_ptr = alloc_fn.call(&mut self.store, 64 * 1024)?;

        let target_fn = self
            .instance
            .get_typed_func::<(i32, i32, i32), i32>(&mut self.store, fn_name)
            .with_context(|| format!("Plugin does not export '{}' function", fn_name))?;

        let output_len = target_fn.call(
            &mut self.store,
            (param_ptr, param_bytes.len() as i32, output_ptr),
        )?;
        if output_len < 0 {
            anyhow::bail!(
                "Function {} failed with error code: {}",
                fn_name,
                output_len
            );
        }

        let memory = self
            .instance
            .get_memory(&mut self.store, "memory")
            .context("Plugin has no memory export")?;

        let data = memory.data(&self.store);
        let result_bytes = &data[output_ptr as usize..(output_ptr + output_len) as usize];
        String::from_utf8(result_bytes.to_vec()).context("Invalid UTF-8 in result")
    }
}

// =============================================================================
// Channel Plugin Types
// =============================================================================

/// Channel metadata from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChannelInfo {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub supports_streaming: bool,
    #[serde(default)]
    pub supports_edits: bool,
}

/// Auth status for channel plugins
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelAuthStatus {
    NotRequired,
    Authenticated,
    NotAuthenticated,
    Expired,
}

/// Inbound message from a channel
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChannelInboundMessage {
    pub conversation_id: String,
    pub user_id: String,
    pub text: String,
    #[serde(default)]
    pub metadata_json: String,
}

/// Webhook request from host
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelWebhookRequest {
    pub method: String,
    pub path: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
}

/// Webhook response from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChannelWebhookResponse {
    pub status: u16,
    #[serde(default)]
    pub headers: Vec<(String, String)>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub messages: Vec<ChannelInboundMessage>,
}

/// Outbound message request
#[derive(Debug, Clone, serde::Serialize)]
pub struct ChannelSendRequest {
    pub conversation_id: String,
    pub text: String,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub is_final: bool,
    #[serde(default)]
    pub metadata_json: String,
}

/// Send result from plugin
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ChannelSendResult {
    pub success: bool,
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
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
    /// Provider key for models.dev lookup (e.g., "google" for gemini plugins)
    /// If not set, tark will try to infer from the plugin id
    #[serde(default)]
    pub provider: Option<String>,
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

/// Auth credentials from auth-only plugin
///
/// Auth-only plugins provide credentials to tark's built-in providers
/// instead of implementing the full provider_chat interface. This enables
/// streaming, tools, and all native provider features.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct AuthCredentials {
    /// Valid OAuth access token (refreshed by plugin if needed)
    pub access_token: String,
    /// Google Cloud project ID (for Cloud Code Assist API)
    #[serde(default)]
    pub project_id: Option<String>,
    /// API mode: "standard", "cloud_code_assist", or "openai_compat"
    #[serde(default = "default_api_mode")]
    pub api_mode: String,
    /// Custom API endpoint (for openai_compat mode)
    #[serde(default)]
    pub endpoint: Option<String>,
    /// Custom headers to send with requests (for openai_compat mode)
    #[serde(default)]
    pub custom_headers: Option<std::collections::HashMap<String, String>>,
}

fn default_api_mode() -> String {
    "cloud_code_assist".to_string()
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
            // IMPORTANT: plugin HTTP is implemented with a blocking reqwest client.
            // Without timeouts, a network stall can hang provider calls indefinitely,
            // which shows up in the TUI as "no response".
            http_client: reqwest::blocking::Client::builder()
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
                .build()
                .unwrap_or_else(|_| reqwest::blocking::Client::new()),
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

    /// Check if filesystem read access to a path is allowed
    pub fn is_fs_read_allowed(&self, path: &str) -> bool {
        self.capabilities.is_fs_read_allowed(path)
    }
}

/// Plugin host - manages WASM runtime and plugin instances
pub struct PluginHost {
    /// WASM engine (shared for epoch increment thread)
    engine: Arc<Engine>,
    /// Loaded plugin instances
    instances: HashMap<String, PluginInstance>,
}

impl PluginHost {
    /// Create a new plugin host with security features enabled
    pub fn new() -> Result<Self> {
        let mut config = Config::new();

        // Enable epoch-based interruption to prevent infinite loops
        // This allows us to interrupt long-running WASM code
        config.epoch_interruption(true);

        // Note: We use sync mode for simplicity. Async mode requires async instantiation.
        let engine = Arc::new(Engine::new(&config)?);

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

        // Set epoch deadline to prevent infinite loops
        // The deadline is set to a reasonable number of ticks
        store.set_epoch_deadline(DEFAULT_EPOCH_DEADLINE);

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

    /// Load a plugin using a custom data directory (e.g., project-local storage)
    pub fn load_with_data_dir(
        &mut self,
        plugin: &InstalledPlugin,
        data_dir: std::path::PathBuf,
    ) -> Result<()> {
        tracing::info!(
            "Loading plugin: {} (data dir: {})",
            plugin.id(),
            data_dir.display()
        );

        let wasm_bytes = std::fs::read(&plugin.wasm_path)
            .with_context(|| format!("Failed to read WASM: {}", plugin.wasm_path.display()))?;

        let module = Module::new(&self.engine, &wasm_bytes)
            .with_context(|| format!("Failed to compile WASM: {}", plugin.id()))?;

        let state = PluginState::new(plugin.id(), data_dir, plugin.manifest.capabilities.clone());

        let mut store = Store::new(&self.engine, state);
        store.set_epoch_deadline(DEFAULT_EPOCH_DEADLINE);

        let mut linker = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state: &mut PluginState| {
            &mut state.wasi_ctx
        })?;
        Self::define_host_functions(&mut linker)?;

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
                // Direct call - http_get_impl uses reqwest::blocking which is safe
                // Note: block_in_place() was removed as it panics on single-threaded runtimes
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
                // Direct call - http_post_impl uses reqwest::blocking which is safe
                // Note: block_in_place() was removed as it panics on single-threaded runtimes
                let response_str = http_post_impl(&client, &url, body, &headers_json);

                write_string(&mut caller, ret_ptr, &response_str).unwrap_or(-1)
            },
        )?;

        // =========================================================================
        // WebSocket Functions
        // =========================================================================

        // ws.connect(url: string, headers_json: string) -> i32 (JSON response)
        linker.func_wrap(
            "tark:ws",
            "connect",
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
                if !caller.data().is_http_allowed(&url) {
                    tracing::warn!(
                        "[plugin:{}] WS connect blocked - domain not in allowed list: {}",
                        plugin_id,
                        url
                    );
                    return -2;
                }

                let headers: Vec<(String, String)> =
                    serde_json::from_str(&headers_json).unwrap_or_default();

                let result =
                    crate::plugins::ws_connect(&url, &headers, &caller.data().allowed_http_domains);
                let payload = match result {
                    Ok(handle) => serde_json::json!({ "ok": true, "handle": handle }),
                    Err(err) => serde_json::json!({ "ok": false, "error": err.to_string() }),
                };
                write_string(&mut caller, ret_ptr, &payload.to_string()).unwrap_or(-1)
            },
        )?;

        // ws.send(handle: u64, data: string) -> i32 (JSON response)
        linker.func_wrap(
            "tark:ws",
            "send",
            |mut caller: Caller<'_, PluginState>,
             handle: i64,
             data_ptr: i32,
             data_len: i32,
             ret_ptr: i32|
             -> i32 {
                let data = match read_string(&mut caller, data_ptr, data_len) {
                    Ok(d) => d,
                    Err(_) => return -1,
                };
                let result = crate::plugins::ws_send(handle as u64, &data);
                let payload = match result {
                    Ok(_) => serde_json::json!({ "ok": true }),
                    Err(err) => serde_json::json!({ "ok": false, "error": err.to_string() }),
                };
                write_string(&mut caller, ret_ptr, &payload.to_string()).unwrap_or(-1)
            },
        )?;

        // ws.recv(handle: u64, timeout_ms: u64, max_bytes: u64) -> i32 (JSON response)
        linker.func_wrap(
            "tark:ws",
            "recv",
            |mut caller: Caller<'_, PluginState>,
             handle: i64,
             timeout_ms: i64,
             max_bytes: i64,
             ret_ptr: i32|
             -> i32 {
                let result = crate::plugins::ws_recv(
                    handle as u64,
                    timeout_ms.max(0) as u64,
                    max_bytes.max(0) as usize,
                );
                let payload = match result {
                    Ok(res) => serde_json::json!({
                        "ok": true,
                        "message": res.message,
                        "closed": res.closed,
                        "error": res.error
                    }),
                    Err(err) => serde_json::json!({ "ok": false, "error": err.to_string() }),
                };
                write_string(&mut caller, ret_ptr, &payload.to_string()).unwrap_or(-1)
            },
        )?;

        // ws.close(handle: u64) -> i32 (JSON response)
        linker.func_wrap(
            "tark:ws",
            "close",
            |mut caller: Caller<'_, PluginState>, handle: i64, ret_ptr: i32| -> i32 {
                let result = crate::plugins::ws_close(handle as u64);
                let payload = match result {
                    Ok(_) => serde_json::json!({ "ok": true }),
                    Err(err) => serde_json::json!({ "ok": false, "error": err.to_string() }),
                };
                write_string(&mut caller, ret_ptr, &payload.to_string()).unwrap_or(-1)
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

        // =========================================================================
        // Filesystem Functions (read-only, capability-controlled)
        // =========================================================================

        // fs.read(path: string) -> i32 (bytes written or error)
        // Error codes: -1 = invalid path string, -2 = permission denied, -3 = read error, -4 = write error
        linker.func_wrap(
            "tark:fs",
            "read",
            |mut caller: Caller<'_, PluginState>,
             path_ptr: i32,
             path_len: i32,
             ret_ptr: i32|
             -> i32 {
                let path = match read_string(&mut caller, path_ptr, path_len) {
                    Ok(p) => p,
                    Err(_) => return -1, // Invalid path string
                };

                let plugin_id = caller.data().plugin_id.clone();

                // Security check: is this path allowed?
                if !caller.data().is_fs_read_allowed(&path) {
                    tracing::warn!(
                        "[plugin:{}] fs.read blocked - path not in allowed list: {}",
                        plugin_id,
                        path
                    );
                    return -2; // Permission denied
                }

                tracing::debug!("[plugin:{}] fs.read({})", plugin_id, path);

                // Expand ~ to home directory
                let expanded = expand_home_for_fs(&path);

                // Read file contents
                let contents = match std::fs::read_to_string(&expanded) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::debug!("[plugin:{}] fs.read failed: {}", plugin_id, e);
                        return -3; // Read error
                    }
                };

                write_string(&mut caller, ret_ptr, &contents).unwrap_or(-4)
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
// Filesystem helper functions
// =============================================================================

/// Expand ~ to home directory in a path
fn expand_home_for_fs(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped).to_string_lossy().to_string();
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.to_string_lossy().to_string();
        }
    }
    path.to_string()
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
            })
            .to_string()
        }
        Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
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
            })
            .to_string()
        }
        Err(e) => serde_json::json!({"error": e.to_string()}).to_string(),
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
