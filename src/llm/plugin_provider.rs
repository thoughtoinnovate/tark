//! Plugin-based LLM provider adapter
//!
//! Wraps a WASM provider plugin as an `LlmProvider` implementation,
//! allowing plugins to be used seamlessly alongside built-in providers.

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role,
    TokenUsage, ToolDefinition,
};
use crate::plugins::{
    ChatResponse, ModelInfo, PluginHost, PluginRegistry, PluginType, ProviderAuthStatus,
    ProviderInfo,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;

/// Adapter that implements `LlmProvider` by calling a WASM provider plugin
pub struct PluginProviderAdapter {
    /// Plugin ID
    plugin_id: String,
    /// Provider display name
    display_name: String,
    /// Selected model
    model: String,
    /// Available models (cached)
    models: Vec<ModelInfo>,
    /// Provider info (cached)
    info: ProviderInfo,
}

impl PluginProviderAdapter {
    /// Create adapter for a provider plugin
    pub fn new(plugin_id: &str) -> Result<Self> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found: {}", plugin_id))?;

        if plugin.manifest.plugin_type() != PluginType::Provider {
            anyhow::bail!("Plugin {} is not a provider plugin", plugin_id);
        }

        if !plugin.enabled {
            anyhow::bail!("Plugin {} is disabled", plugin_id);
        }

        // Load plugin to get metadata
        let mut host = PluginHost::new()?;
        host.load(plugin)?;

        let instance = host
            .get_mut(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Failed to get plugin instance"))?;

        // Get provider info and models
        let info = instance.provider_info()?;
        let models = instance.provider_models()?;

        let default_model = models
            .first()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "default".to_string());

        Ok(Self {
            plugin_id: plugin_id.to_string(),
            display_name: info.display_name.clone(),
            model: default_model,
            models,
            info,
        })
    }

    /// Set the model to use
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Get available models
    pub fn available_models(&self) -> &[ModelInfo] {
        &self.models
    }

    /// Get provider info
    pub fn info(&self) -> &ProviderInfo {
        &self.info
    }

    /// Check authentication status
    pub fn auth_status(&self) -> Result<ProviderAuthStatus> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found"))?;

        let mut host = PluginHost::new()?;
        host.load(plugin)?;

        let instance = host
            .get_mut(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Failed to get plugin instance"))?;

        instance.provider_auth_status()
    }

    /// Initialize with credentials
    pub fn auth_init(&self, credentials_json: &str) -> Result<()> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found"))?;

        let mut host = PluginHost::new()?;
        host.load(plugin)?;

        let instance = host
            .get_mut(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Failed to get plugin instance"))?;

        instance.provider_auth_init(credentials_json)
    }

    /// Get a fresh plugin instance for making requests
    fn get_instance(&self) -> Result<(PluginHost, String)> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(&self.plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin not found"))?;

        let mut host = PluginHost::new()?;
        host.load(plugin)?;

        Ok((host, self.plugin_id.clone()))
    }
}

/// Message format for plugin communication
#[derive(Debug, Serialize)]
struct PluginMessage {
    role: String,
    content: String,
}

#[async_trait]
impl LlmProvider for PluginProviderAdapter {
    fn name(&self) -> &str {
        &self.display_name
    }

    fn supports_streaming(&self) -> bool {
        // Check if selected model supports streaming
        self.models
            .iter()
            .find(|m| m.id == self.model)
            .map(|m| m.supports_streaming)
            .unwrap_or(false)
    }

    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let (mut host, plugin_id) = self.get_instance()?;
        let instance = host
            .get_mut(&plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin instance not found"))?;

        // Convert messages to plugin format (JSON)
        let plugin_messages: Vec<PluginMessage> = messages
            .iter()
            .map(|m| PluginMessage {
                role: match m.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                }
                .to_string(),
                content: m.content.as_text().unwrap_or("").to_string(),
            })
            .collect();

        let messages_json = serde_json::to_string(&plugin_messages)?;

        // Call plugin
        let response: ChatResponse = instance.provider_chat(&messages_json, &self.model)?;

        // Convert response
        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
        });

        Ok(LlmResponse::Text {
            text: response.text,
            usage,
        })
    }

    // Note: chat_streaming has a default implementation that calls chat_streaming_with_thinking
    // which has a default implementation that falls back to chat().
    // So we don't need to implement it - the default will work.

    async fn complete_fim(
        &self,
        _prefix: &str,
        _suffix: &str,
        _language: &str,
    ) -> Result<CompletionResult> {
        // FIM not supported by plugin providers yet
        anyhow::bail!(
            "FIM completion not supported by plugin provider: {}",
            self.plugin_id
        )
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        // Use chat for code explanation
        let messages = vec![Message::user(format!(
            "Explain this code:\n\nContext: {}\n\n```\n{}\n```",
            context, code
        ))];

        let response = self.chat(&messages, None).await?;
        Ok(response.text().unwrap_or("").to_string())
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        // Use chat for refactoring suggestions
        let messages = vec![Message::user(format!(
            "Suggest refactorings for this code. Return as JSON array with 'description' and 'code' fields:\n\nContext: {}\n\n```\n{}\n```",
            context, code
        ))];

        let response = self.chat(&messages, None).await?;
        let text = response.text().unwrap_or("");

        // Try to parse as JSON, fall back to empty if parsing fails
        match serde_json::from_str::<Vec<RefactoringSuggestion>>(text) {
            Ok(suggestions) => Ok(suggestions),
            Err(_) => Ok(Vec::new()),
        }
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        // Use chat for code review
        let messages = vec![Message::user(format!(
            "Review this {} code for issues. Return as JSON array with 'line', 'severity', 'message', and 'suggestion' fields:\n\n```{}\n{}\n```",
            language, language, code
        ))];

        let response = self.chat(&messages, None).await?;
        let text = response.text().unwrap_or("");

        // Try to parse as JSON, fall back to empty if parsing fails
        match serde_json::from_str::<Vec<CodeIssue>>(text) {
            Ok(issues) => Ok(issues),
            Err(_) => Ok(Vec::new()),
        }
    }
}

/// Get list of available plugin providers
pub fn list_plugin_providers() -> Vec<(String, String)> {
    let registry = match PluginRegistry::new() {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };

    let mut providers = Vec::new();

    for plugin in registry.provider_plugins() {
        // Try to load and get info
        let mut host = match PluginHost::new() {
            Ok(h) => h,
            Err(_) => continue,
        };

        if host.load(plugin).is_err() {
            continue;
        }

        if let Some(instance) = host.get_mut(plugin.id()) {
            if let Ok(info) = instance.provider_info() {
                providers.push((plugin.id().to_string(), info.display_name));
            }
        }
    }

    providers
}

/// Try to create a provider from a plugin
pub fn try_create_plugin_provider(name: &str, model: Option<&str>) -> Option<Box<dyn LlmProvider>> {
    let registry = PluginRegistry::new().ok()?;

    // Check if there's a provider plugin with this name
    let plugin = registry.get(name)?;

    if plugin.manifest.plugin_type() != PluginType::Provider {
        return None;
    }

    if !plugin.enabled {
        tracing::debug!("Plugin {} is disabled", name);
        return None;
    }

    let mut adapter = match PluginProviderAdapter::new(name) {
        Ok(a) => a,
        Err(e) => {
            tracing::debug!("Failed to create plugin provider {}: {}", name, e);
            return None;
        }
    };

    if let Some(m) = model {
        adapter = adapter.with_model(m);
    }

    // Try to auto-initialize with Gemini CLI credentials if this is gemini-oauth
    if name == "gemini-oauth" || name.contains("gemini") {
        if let Some(creds_path) =
            dirs::home_dir().map(|h| h.join(".gemini").join("oauth_creds.json"))
        {
            if creds_path.exists() {
                if let Ok(creds_json) = std::fs::read_to_string(&creds_path) {
                    let _ = adapter.auth_init(&creds_json);
                }
            }
        }
    }

    Some(Box::new(adapter))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_plugin_providers() {
        // Should not panic even if no plugins installed
        let providers = list_plugin_providers();
        println!("Found {} plugin providers", providers.len());
        for (id, name) in &providers {
            println!("  - {} ({})", name, id);
        }
    }
}
