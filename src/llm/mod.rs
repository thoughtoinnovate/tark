//! LLM provider implementations

#![allow(dead_code)]

use crate::config::Config;

pub mod auth;
mod claude;
mod copilot;
mod debug_wrapper;
mod error;
mod gemini;
mod models_db;
mod ollama;
mod openai;
mod openai_compat;
mod openrouter;
mod plugin_provider;
mod raw_log;
pub mod streaming;
mod types;

// Test simulation provider (feature-gated)
#[cfg(feature = "test-sim")]
pub mod tark_sim;
#[cfg(feature = "test-sim")]
pub use tark_sim::TarkSimProvider;

pub use claude::ClaudeProvider;
pub use copilot::CopilotProvider;
pub use debug_wrapper::DebugProviderWrapper;
pub use error::LlmError;
pub use gemini::GeminiProvider;
pub use models_db::{init_models_db, models_db, ModelCapabilities};
pub use ollama::{list_local_ollama_models, OllamaProvider};
pub use openai::OpenAiProvider;
// OpenAI-compatible provider components - public API for plugin providers
#[allow(unused_imports)]
pub use openai_compat::{AuthMethod, OpenAiCompatConfig, OpenAiCompatProvider};
pub use openrouter::OpenRouterProvider;
pub use plugin_provider::{list_plugin_providers, try_create_plugin_provider};
pub use types::*;

pub use raw_log::append_raw_line as append_llm_raw_line;
pub(crate) fn normalize_gemini_oauth_model(model: &str) -> String {
    gemini::normalize_cloud_code_assist_model(model)
}

use anyhow::Result;
use async_trait::async_trait;

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Check if the provider/model supports native extended thinking (sync version)
    /// Default returns false; providers with native thinking should override
    /// This is used as a fallback when async check is not possible
    fn supports_native_thinking(&self) -> bool {
        false
    }

    /// Check if the provider/model supports native extended thinking (async version)
    /// This queries models.dev for capability detection, falling back to sync check
    /// Override this in providers to add models.dev lookup
    async fn supports_native_thinking_async(&self) -> bool {
        // Default: fall back to sync check
        self.supports_native_thinking()
    }

    /// Send a chat completion request (non-streaming)
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse>;

    /// Send a chat completion request with thinking settings (non-streaming)
    ///
    /// When `settings.enabled` is true and the model supports it,
    /// extended thinking/reasoning will be used with the specified effort.
    async fn chat_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        // Default: ignore thinking settings and call regular chat
        let _ = settings;
        self.chat(messages, tools).await
    }

    /// Send a streaming chat completion request
    ///
    /// The callback is invoked for each chunk as it arrives from the LLM.
    /// This enables real-time display of responses in the UI.
    ///
    /// Default implementation falls back to non-streaming `chat()` and
    /// emits a single TextDelta with the complete response.
    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        self.chat_streaming_with_thinking(
            messages,
            tools,
            callback,
            interrupt_check,
            &ThinkSettings::off(),
        )
        .await
    }

    /// Send a streaming chat completion request with thinking settings
    ///
    /// When `settings.enabled` is true and the model supports it,
    /// extended thinking/reasoning will be used with the specified effort.
    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        // Allow callers (TUI/agent) to interrupt even when using the fallback
        // non-streaming implementation.
        if let Some(check) = interrupt_check {
            if check() {
                return Ok(LlmResponse::Text {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    usage: None,
                });
            }
        }

        // Default: fall back to non-streaming and emit complete response
        let response = self.chat_with_thinking(messages, tools, settings).await?;

        // Emit the response as a single chunk
        if let Some(text) = response.text() {
            callback(StreamEvent::TextDelta(text.to_string()));
        }

        // Emit tool calls if any
        for tool_call in response.tool_calls() {
            callback(StreamEvent::ToolCallStart {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
                thought_signature: tool_call.thought_signature.clone(),
            });
            callback(StreamEvent::ToolCallDelta {
                id: tool_call.id.clone(),
                arguments_delta: tool_call.arguments.to_string(),
            });
            callback(StreamEvent::ToolCallComplete {
                id: tool_call.id.clone(),
            });
        }

        callback(StreamEvent::Done);
        Ok(response)
    }

    /// Check if this provider supports true streaming
    ///
    /// Returns true if the provider implements native streaming,
    /// false if it uses the default fallback implementation.
    fn supports_streaming(&self) -> bool {
        false // Default: no native streaming
    }

    /// Fill-in-middle completion for code
    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult>;

    /// Explain code at a specific location
    async fn explain_code(&self, code: &str, context: &str) -> Result<String>;

    /// Suggest refactorings for selected code
    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>>;

    /// Review code and return potential issues
    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>>;
}

/// Try to get Gemini token from the gemini-auth plugin
fn try_gemini_auth_plugin() -> Option<String> {
    use crate::plugins::{PluginHost, PluginRegistry};

    // Check if gemini-auth plugin is installed
    let registry = PluginRegistry::new().ok()?;
    let plugin = registry.get("gemini-auth")?;

    if !plugin.enabled {
        return None;
    }

    // Load and initialize plugin
    let mut host = PluginHost::new().ok()?;
    host.load(plugin).ok()?;

    let instance = host.get_mut("gemini-auth")?;

    // Check if plugin has credentials - try loading from Gemini CLI
    let gemini_cli_path = dirs::home_dir()?.join(".gemini").join("oauth_creds.json");
    if gemini_cli_path.exists() {
        if let Ok(creds_json) = std::fs::read_to_string(&gemini_cli_path) {
            // Initialize plugin with Gemini CLI credentials
            if instance.auth_init_with_credentials(&creds_json).is_ok() {
                tracing::debug!("Initialized gemini-auth plugin with Gemini CLI credentials");
            }
        }
    }

    // Try to get token from plugin
    match instance.auth_get_token() {
        Ok(token) if !token.is_empty() => Some(token),
        _ => None,
    }
}

/// Create an LLM provider based on name
pub fn create_provider(name: &str) -> Result<Box<dyn LlmProvider>> {
    create_provider_with_options(name, false, None)
}

/// Create an LLM provider with options
/// - `silent`: When true, suppress CLI output (for TUI usage)
/// - Loads max_tokens from config file for each provider
pub fn create_provider_with_options(
    name: &str,
    silent: bool,
    model: Option<&str>,
) -> Result<Box<dyn LlmProvider>> {
    // First, try plugin providers
    if let Some(provider) = try_create_plugin_provider(name, model) {
        tracing::info!("Using plugin provider: {}", name);
        return Ok(provider);
    }

    // Load config to get max_tokens settings
    let config = Config::load().unwrap_or_default();

    // Then try built-in providers
    match name.to_lowercase().as_str() {
        "claude" | "anthropic" => {
            let mut p = ClaudeProvider::new()?.with_max_tokens(config.llm.claude.max_tokens);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        "openai" | "gpt" => {
            let mut p = OpenAiProvider::new()?.with_max_tokens(config.llm.openai.max_tokens);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        "ollama" | "local" => {
            let mut p = OllamaProvider::new()?;
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        "copilot" | "github" => {
            let mut p = CopilotProvider::new()?
                .with_silent(silent)
                .with_max_tokens(config.llm.copilot.max_tokens);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        "gemini" | "google" => {
            let mut p = GeminiProvider::new()?.with_max_tokens(config.llm.gemini.max_tokens);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        "openrouter" => {
            let mut p =
                OpenRouterProvider::new()?.with_max_tokens(config.llm.openrouter.max_tokens);
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        #[cfg(feature = "test-sim")]
        "tark_sim" | "sim" | "test" => {
            let mut p = TarkSimProvider::new();
            if let Some(m) = model {
                p = p.with_model(m);
            }
            Ok(Box::new(p))
        }
        _ => {
            // List available plugin providers in error message
            let plugin_providers = list_plugin_providers();
            let plugin_list = if plugin_providers.is_empty() {
                String::new()
            } else {
                format!(
                    "\nPlugin providers: {}",
                    plugin_providers
                        .iter()
                        .map(|(id, _)| id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };

            anyhow::bail!(
                "Unknown LLM provider: {}. Supported: claude, openai, ollama, copilot, gemini, openrouter{}",
                name,
                plugin_list
            )
        }
    }
}

/// Create provider with optional debug wrapper
pub fn create_provider_with_debug(
    name: &str,
    silent: bool,
    model: Option<&str>,
    debug_log_path: Option<&std::path::Path>,
) -> Result<Box<dyn LlmProvider>> {
    let provider = create_provider_with_options(name, silent, model)?;

    if let Some(log_path) = debug_log_path {
        Ok(Box::new(DebugProviderWrapper::new(provider, log_path)?))
    } else {
        Ok(provider)
    }
}
