//! LLM provider implementations

#![allow(dead_code)]

mod claude;
mod models_db;
mod ollama;
mod openai;
mod types;

pub use claude::ClaudeProvider;
pub use models_db::{models_db, ModelCapabilities};
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use types::*;

use anyhow::Result;
use async_trait::async_trait;

/// Trait for LLM providers
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Get the provider name
    fn name(&self) -> &str;

    /// Send a chat completion request
    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse>;

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
    match name.to_lowercase().as_str() {
        "claude" | "anthropic" => Ok(Box::new(ClaudeProvider::new()?)),
        "openai" | "gpt" => Ok(Box::new(OpenAiProvider::new()?)),
        "ollama" | "local" => Ok(Box::new(OllamaProvider::new()?)),
        _ => anyhow::bail!(
            "Unknown LLM provider: {}. Supported: claude, openai, ollama",
            name
        ),
    }
}
