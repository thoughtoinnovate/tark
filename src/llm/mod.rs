//! LLM provider implementations

#![allow(dead_code)]

mod claude;
mod copilot;
mod gemini;
mod models_db;
mod ollama;
mod openai;
mod openrouter;
mod types;

pub use claude::ClaudeProvider;
pub use copilot::CopilotProvider;
pub use gemini::GeminiProvider;
pub use models_db::{models_db, ModelCapabilities};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use openrouter::OpenRouterProvider;
pub use types::*;

use anyhow::Result;
use async_trait::async_trait;

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Check if the provider/model supports native extended thinking
    /// Default returns false; providers with native thinking should override
    fn supports_native_thinking(&self) -> bool {
        false
    }

    /// Send a chat completion request (non-streaming)
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse>;

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
        let response = self.chat(messages, tools).await?;

        // Emit the response as a single chunk
        if let Some(text) = response.text() {
            callback(StreamEvent::TextDelta(text.to_string()));
        }

        // Emit tool calls if any
        for tool_call in response.tool_calls() {
            callback(StreamEvent::ToolCallStart {
                id: tool_call.id.clone(),
                name: tool_call.name.clone(),
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

/// Create an LLM provider based on name
pub fn create_provider(name: &str) -> Result<Box<dyn LlmProvider>> {
    create_provider_with_options(name, false)
}

/// Create an LLM provider with options
/// - `silent`: When true, suppress CLI output (for TUI usage)
pub fn create_provider_with_options(name: &str, silent: bool) -> Result<Box<dyn LlmProvider>> {
    match name.to_lowercase().as_str() {
        "claude" | "anthropic" => Ok(Box::new(ClaudeProvider::new()?)),
        "openai" | "gpt" => Ok(Box::new(OpenAiProvider::new()?)),
        "ollama" | "local" => Ok(Box::new(OllamaProvider::new()?)),
        "copilot" | "github" => Ok(Box::new(CopilotProvider::new()?.with_silent(silent))),
        "gemini" | "google" => Ok(Box::new(GeminiProvider::new()?)),
        "openrouter" => Ok(Box::new(OpenRouterProvider::new()?)),
        _ => anyhow::bail!(
            "Unknown LLM provider: {}. Supported: claude, openai, ollama, copilot, gemini, openrouter",
            name
        ),
    }
}
