//! Catalog Service - Manages providers, models, and authentication
//!
//! Provides async access to provider/model discovery and capabilities.

use anyhow::Result;

use crate::llm::{models_db, ModelCapabilities};

use super::errors::CatalogError;
use super::types::{ModelInfo, ProviderInfo, ProviderSource};

/// Authentication status for a provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthStatus {
    /// Not authenticated, no stored credentials
    NotAuthenticated,
    /// Using API key from environment
    ApiKey,
    /// Using OAuth token
    OAuth,
    /// Using Application Default Credentials
    #[allow(clippy::upper_case_acronyms)]
    ADC,
}

/// Device flow information for OAuth
#[derive(Debug, Clone)]
pub struct DeviceFlowInfo {
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Device flow poll result
#[derive(Debug, Clone)]
pub enum AuthPollResult {
    Pending,
    Success(String), // Access token
    Expired,
    Error(String),
}

/// Provider capabilities
#[derive(Debug, Clone)]
pub struct ProviderCapabilities {
    pub supports_streaming: bool,
    pub supports_thinking: bool,
    pub supports_vision: bool,
    pub supports_tools: bool,
    pub max_context_tokens: usize,
}

/// Catalog Service
///
/// Manages provider/model discovery, capabilities, and authentication.
#[derive(Clone, Copy)]
pub struct CatalogService;

impl CatalogService {
    /// Create a new catalog service
    pub fn new() -> Self {
        Self
    }

    // === Providers ===

    /// List all available providers (async, no blocking)
    pub async fn list_providers(&self) -> Vec<ProviderInfo> {
        // Load config to check enabled_providers filter
        let config = crate::config::Config::load().unwrap_or_default();
        let enabled_providers = if config.llm.enabled_providers.is_empty() {
            None
        } else {
            Some(config.llm.enabled_providers.clone())
        };

        // Get providers from models.dev
        let models_db_manager = models_db();
        let all_provider_ids: Vec<String> = models_db_manager
            .list_providers()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!("Failed to load providers from models.dev: {}", e);
                enabled_providers
                    .clone()
                    .unwrap_or_else(|| vec!["openai".to_string(), "google".to_string()])
            });

        // Filter by enabled_providers if configured
        let provider_ids: Vec<String> = if let Some(ref enabled) = enabled_providers {
            all_provider_ids
                .into_iter()
                .filter(|id| enabled.contains(id))
                .collect()
        } else {
            all_provider_ids
        };

        tracing::info!(
            "Loading {} providers: {:?}",
            provider_ids.len(),
            provider_ids
        );

        // Collect provider info
        let mut providers = Vec::new();
        let models_db_manager = models_db();
        for id in &provider_ids {
            let provider_info = models_db_manager.get_provider(id).await.ok().flatten();

            let Some(info) = provider_info else {
                continue;
            };

            // Check if configured
            let configured = if info.env.is_empty() {
                true
            } else {
                info.env
                    .iter()
                    .any(|env_var| std::env::var(env_var).is_ok())
            };

            let icon = Self::provider_icon(id);
            let description = if info.models.is_empty() {
                "AI models".to_string()
            } else {
                format!("{} models available", info.models.len())
            };

            providers.push(ProviderInfo {
                id: id.clone(),
                name: info.name.clone(),
                description,
                configured,
                icon,
                source: ProviderSource::Native,
            });
        }

        // Add plugin providers (load in blocking task to avoid blocking in async context)
        let plugin_providers = tokio::task::spawn_blocking(crate::llm::list_plugin_providers)
            .await
            .unwrap_or_default();
        for (plugin_id, display_name) in plugin_providers {
            // Skip if already in list (avoid duplicates)
            if providers.iter().any(|p| p.id == plugin_id) {
                continue;
            }

            providers.push(ProviderInfo {
                id: plugin_id.clone(),
                name: display_name,
                description: "Plugin provider".to_string(),
                configured: true, // Plugin is installed = configured
                icon: "🔌".to_string(),
                source: ProviderSource::Plugin,
            });
        }

        providers
    }

    /// Get provider capabilities
    pub async fn provider_capabilities(&self, id: &str) -> Option<ProviderCapabilities> {
        let models_db_manager = models_db();
        let _info = models_db_manager.get_provider(id).await.ok().flatten()?;

        Some(ProviderCapabilities {
            supports_streaming: true,   // Most providers support streaming
            supports_thinking: false,   // Will check per model
            supports_vision: false,     // Will check per model
            supports_tools: true,       // Most providers support tools
            max_context_tokens: 128000, // Default, will vary by model
        })
    }

    /// Check if a provider is configured
    pub fn is_provider_configured(&self, provider_id: &str) -> bool {
        match provider_id {
            "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
            "google" | "gemini" => {
                std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok()
            }
            "openrouter" => std::env::var("OPENROUTER_API_KEY").is_ok(),
            "ollama" => true,   // Local, always available
            "tark_sim" => true, // Simulation provider always available
            _ => false,
        }
    }

    // === Models ===

    /// List models for a provider
    pub async fn list_models(&self, provider: &str) -> Vec<ModelInfo> {
        // Check if this is a plugin provider with base_provider delegation
        let lookup_provider = self.resolve_base_provider(provider);

        let models_db_manager = models_db();
        match models_db_manager.list_models(&lookup_provider).await {
            Ok(models) if !models.is_empty() => models
                .into_iter()
                .map(|m| ModelInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    description: m.capability_summary(),
                    provider: provider.to_string(), // Keep original provider for display
                    context_window: m.limit.context as usize,
                    max_tokens: m.limit.output as usize,
                })
                .collect(),
            _ => {
                tracing::warn!(
                    "Failed to load models for provider {} (lookup: {})",
                    provider,
                    lookup_provider
                );
                vec![]
            }
        }
    }

    /// Resolve base_provider for plugin providers
    ///
    /// If the provider is a plugin with base_provider set, return the base_provider
    /// for models.dev lookup. Otherwise return the provider as-is.
    fn resolve_base_provider(&self, provider: &str) -> String {
        // Check if this is a plugin provider
        if let Ok(registry) = crate::plugins::PluginRegistry::new() {
            for plugin in registry.provider_plugins() {
                for contrib in plugin.contributed_providers() {
                    if contrib.id == provider {
                        // Found the plugin provider - check for base_provider
                        if let Some(ref base) = contrib.base_provider {
                            tracing::debug!(
                                "Provider {} delegates to base_provider: {}",
                                provider,
                                base
                            );
                            return base.clone();
                        }
                    }
                }
            }
        }
        // Not a plugin or no base_provider - use as-is
        provider.to_string()
    }

    /// Get model capabilities
    pub async fn model_capabilities(
        &self,
        provider: &str,
        model: &str,
    ) -> Option<ModelCapabilities> {
        let models_db_manager = models_db();
        let model_info = models_db_manager
            .get_model(provider, model)
            .await
            .ok()
            .flatten()?;

        // Convert ModelInfo to ModelCapabilities
        Some(ModelCapabilities {
            tool_call: model_info.tool_call,
            reasoning: model_info.reasoning,
            vision: model_info.supports_vision(),
            audio_input: model_info.supports_audio_input(),
            video_input: model_info.supports_video_input(),
            pdf: model_info.supports_pdf(),
            image_output: model_info.supports_image_output(),
            audio_output: model_info.supports_audio_output(),
            structured_output: model_info.structured_output.unwrap_or(false),
            temperature: model_info.temperature,
            context_limit: model_info.limit.context,
            output_limit: model_info.limit.output,
            input_cost: model_info.cost.input,
            output_cost: model_info.cost.output,
            supports_caching: model_info.cost.cache_read.is_some(),
            reasoning_cost: model_info.cost.reasoning_cost_per_million(),
        })
    }

    /// Get context limit for a model
    pub fn context_limit(&self, provider: &str, _model: &str) -> usize {
        // This would require async, but for now return defaults
        // Will be properly implemented when integrated with models.dev
        match provider {
            "openai" => 128000,
            "anthropic" => 200000,
            "google" | "gemini" => 1_000_000,
            "tark_sim" => 8192,
            _ => 100_000,
        }
    }

    /// Check if a model supports thinking
    pub fn supports_thinking(&self, provider: &str, model: &str) -> bool {
        // Check model name for thinking indicators
        model.contains("thinking")
            || model.contains("o1")
            || model.contains("o3")
            || (provider == "google" && model.contains("2.0-flash-thinking"))
            || (provider == "tark_sim" && model == "tark_llm") // Sim provider supports thinking
    }

    /// Check if a model supports vision
    pub fn supports_vision(&self, _provider: &str, model: &str) -> bool {
        // Most modern models support vision
        !model.contains("audio") && !model.contains("embedding") && !model.contains("moderation")
    }

    // === Authentication ===

    /// Get authentication status for a provider
    pub fn auth_status(&self, provider: &str) -> AuthStatus {
        if self.is_provider_configured(provider) {
            AuthStatus::ApiKey
        } else {
            AuthStatus::NotAuthenticated
        }
    }

    /// Start device flow authentication
    ///
    /// Note: Full device flow implementation pending. This requires providers to implement
    /// the DeviceFlowAuth trait. Currently the copilot provider has internal device flow
    /// methods but doesn't expose the trait interface. This will be completed when providers
    /// are refactored to implement the trait defined in llm/auth/mod.rs.
    pub async fn start_device_flow(
        &self,
        provider: &str,
        _state: &crate::ui_backend::SharedState,
    ) -> Result<DeviceFlowInfo, CatalogError> {
        // TODO: Once providers implement DeviceFlowAuth trait, this should:
        // 1. Get provider-specific DeviceFlowAuth implementation
        // 2. Call start_device_flow() on the trait
        // 3. Store session in SharedState
        // 4. Return DeviceFlowInfo

        // For now, return not supported until trait is implemented on providers
        Err(CatalogError::DeviceFlowNotSupported(provider.to_string()))
    }

    /// Poll device flow for completion
    ///
    /// Note: Full implementation pending - requires DeviceFlowAuth trait on providers
    pub async fn poll_device_flow(
        &self,
        state: &crate::ui_backend::SharedState,
    ) -> Result<AuthPollResult, CatalogError> {
        let session = state
            .device_flow_session()
            .ok_or(CatalogError::NoActiveDeviceFlow)?;

        // Check expiry
        if std::time::Instant::now() >= session.expires_at {
            state.set_device_flow_session(None);
            return Ok(AuthPollResult::Expired);
        }

        // TODO: Once providers implement DeviceFlowAuth trait:
        // 1. Get provider-specific implementation
        // 2. Call poll() method with device_code
        // 3. Convert PollResult to AuthPollResult
        // 4. Clear session on success/expiry
        // 5. Return result

        // For now, return Pending until trait is implemented
        Ok(AuthPollResult::Pending)
    }

    /// Logout from a provider
    ///
    /// Note: Full implementation pending - requires TokenStore integration
    pub async fn logout(&self, provider: &str) -> Result<(), CatalogError> {
        // TODO: Once TokenStore is integrated:
        // 1. Load TokenStore for provider
        // 2. Call clear() or remove credentials
        // 3. Clear any cached tokens in memory
        tracing::info!("Logout requested for provider: {}", provider);
        Ok(())
    }

    /// Set API key for a provider
    ///
    /// Note: Full implementation pending - requires secure storage or config integration
    pub fn set_api_key(&self, provider: &str, _key: &str) -> Result<(), CatalogError> {
        // TODO: Implement secure storage for API keys:
        // Option 1: Write to config file (less secure)
        // Option 2: Use system keychain (more secure)
        // Option 3: Environment variable (current method - just log)
        tracing::info!("Set API key requested for provider: {}", provider);
        Ok(())
    }

    // === Helper Methods ===

    /// Get icon for provider
    fn provider_icon(provider_id: &str) -> String {
        match provider_id {
            "openai" => "🔑",
            "anthropic" => "🤖",
            "google" | "gemini" => "💎",
            "openrouter" => "🔀",
            "ollama" => "🦙",
            "tark_sim" => "🧪",
            _ => "📦",
        }
        .to_string()
    }
}

impl Default for CatalogService {
    fn default() -> Self {
        Self::new()
    }
}
