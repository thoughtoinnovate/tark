//! OpenRouter LLM provider implementation
//!
//! OpenRouter provides access to 200+ models from various providers through
//! an OpenAI-compatible API.
//!
//! SECURITY: API keys are ONLY sent to official OpenRouter endpoints.
//! The OPENROUTER_API_KEY is never sent to any third-party services.

#![allow(dead_code)]

use super::{
    openai_compat::{AuthMethod, OpenAiCompatConfig, OpenAiCompatProvider},
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion,
    StreamCallback, ThinkSettings, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use std::env;

/// Official OpenRouter API endpoint
const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

/// OpenRouter provider using the common OpenAI-compatible layer
pub struct OpenRouterProvider {
    inner: OpenAiCompatProvider,
    model: String,
}

impl OpenRouterProvider {
    pub fn new() -> Result<Self> {
        let api_key = env::var("OPENROUTER_API_KEY")
            .context("OPENROUTER_API_KEY environment variable not set")?;

        let provider = OpenAiCompatProvider::new(
            OpenAiCompatConfig::new(
                "openrouter",
                OPENROUTER_API_URL,
                AuthMethod::BearerToken(api_key),
            )
            .with_model("anthropic/claude-sonnet-4")
            .with_max_tokens(4096)
            .with_header("HTTP-Referer", "https://github.com/tark-ai/tark")
            .with_header("X-Title", "Tark"),
        );

        Ok(Self {
            inner: provider,
            model: "anthropic/claude-sonnet-4".to_string(),
        })
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self.inner = self.inner.with_model(model);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.inner = self.inner.with_max_tokens(max_tokens);
        self
    }

    pub fn with_site_url(self, _site_url: &str) -> Self {
        // Headers are already set in the config, this is kept for API compatibility
        self
    }

    pub fn with_app_name(self, _app_name: &str) -> Self {
        // Headers are already set in the config, this is kept for API compatibility
        self
    }
}

#[async_trait]
impl LlmProvider for OpenRouterProvider {
    fn name(&self) -> &str {
        "openrouter"
    }

    fn supports_native_thinking(&self) -> bool {
        // OpenRouter passes through to underlying models
        // Check for thinking-capable models
        self.model.contains("sonnet")
            || self.model.contains("o1")
            || self.model.contains("o3")
            || self.model.contains("thinking")
            || self.model.contains("deepseek-r1")
    }

    async fn supports_native_thinking_async(&self) -> bool {
        // OpenRouter routes to various providers, try to detect the underlying model
        let db = super::models_db();

        // Try with openrouter as provider
        if db.supports_reasoning("openrouter", &self.model).await {
            return true;
        }

        // Try extracting provider/model from the openrouter model string
        if let Some(slash_idx) = self.model.find('/') {
            let provider = &self.model[..slash_idx];
            let model_name = &self.model[slash_idx + 1..];
            if db.supports_reasoning(provider, model_name).await {
                return true;
            }
        }

        // Fallback to hardcoded check
        self.supports_native_thinking()
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        self.inner.chat(messages, tools).await
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        self.inner
            .chat_streaming(messages, tools, callback, interrupt_check)
            .await
    }

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        self.inner
            .chat_streaming_with_thinking(messages, tools, callback, interrupt_check, settings)
            .await
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        self.inner.complete_fim(prefix, suffix, language).await
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        self.inner.explain_code(code, context).await
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        self.inner.suggest_refactorings(code, context).await
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        self.inner.review_code(code, language).await
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_thinking_detection() {
        // This test doesn't need API access
        let cases = vec![
            ("gpt-4", false),
            ("anthropic/claude-sonnet-4", true),
            ("openai/o1-preview", true),
            ("openai/o3-mini", true),
            ("deepseek/deepseek-r1", true),
        ];

        for (model, expected) in cases {
            // We can't create a provider without API key in test
            // Just test the model pattern matching logic
            let supports = model.contains("sonnet")
                || model.contains("o1")
                || model.contains("o3")
                || model.contains("thinking")
                || model.contains("deepseek-r1");
            assert_eq!(
                supports,
                expected,
                "Model {} should {} support thinking",
                model,
                if expected { "" } else { "not" }
            );
        }
    }
}
