//! Adapters for external CLI-based LLM providers

pub mod gemini_cli;

pub use gemini_cli::GeminiCliAdapter;

use crate::llm::{LlmProvider, LlmResponse, Message, ToolDefinition, CompletionResult, RefactoringSuggestion, CodeIssue};
use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Trait for external CLI-based LLM providers
pub trait CliAdapter: Send + Sync {
    /// Get the path to the CLI executable
    fn command_path(&self) -> &Path;
    
    /// Get the provider name
    fn name(&self) -> &str;

    /// Convert Tark messages to CLI-specific input format (usually a prompt string or JSON)
    fn convert_messages(&self, messages: &[Message]) -> Result<String>;
    
    /// Parse CLI output into a Tark LlmResponse
    fn parse_response(&self, output: String) -> Result<LlmResponse>;
    
    /// Optional: Get arguments for the CLI command
    fn get_args(&self, model: &str, prompt: &str) -> Vec<String>;
}

/// A generic LLM provider that wraps an external CLI via a CliAdapter
pub struct GenericCliProvider<A: CliAdapter> {
    adapter: A,
    model: String,
}

impl<A: CliAdapter> GenericCliProvider<A> {
    pub fn new(adapter: A, model: String) -> Self {
        Self { adapter, model }
    }
}

#[async_trait]
impl<A: CliAdapter> LlmProvider for GenericCliProvider<A> {
    fn name(&self) -> &str {
        self.adapter.name()
    }

    async fn chat(
        &self,
        messages: &[Message],
        _tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let prompt = self.adapter.convert_messages(messages)?;
        let args = self.adapter.get_args(&self.model, &prompt);
        
        let output = tokio::process::Command::new(self.adapter.command_path())
            .args(&args)
            .output()
            .await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("CLI provider {} failed: {}", self.adapter.name(), stderr);
        }
        
        self.adapter.parse_response(String::from_utf8(output.stdout)?)
    }

    async fn complete_fim(
        &self,
        _prefix: &str,
        _suffix: &str,
        _language: &str,
    ) -> Result<CompletionResult> {
        anyhow::bail!("CLI provider {} does not support complete_fim", self.adapter.name())
    }

    async fn explain_code(&self, _code: &str, _context: &str) -> Result<String> {
        anyhow::bail!("CLI provider {} does not support explain_code", self.adapter.name())
    }

    async fn suggest_refactorings(
        &self,
        _code: &str,
        _context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        anyhow::bail!("CLI provider {} does not support suggest_refactorings", self.adapter.name())
    }

    async fn review_code(&self, _code: &str, _language: &str) -> Result<Vec<CodeIssue>> {
        anyhow::bail!("CLI provider {} does not support review_code", self.adapter.name())
    }
}