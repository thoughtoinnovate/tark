//! Gemini CLI adapter implementation

use super::CliAdapter;
use crate::llm::{LlmResponse, Message};
use anyhow::Result;
use std::path::{Path, PathBuf};

pub struct GeminiCliAdapter {
    cli_path: PathBuf,
}

impl GeminiCliAdapter {
    pub fn new(cli_path: PathBuf) -> Self {
        Self { cli_path }
    }
}

impl CliAdapter for GeminiCliAdapter {
    fn command_path(&self) -> &Path {
        &self.cli_path
    }

    fn name(&self) -> &str {
        "gemini-cli"
    }

    fn convert_messages(&self, messages: &[Message]) -> Result<String> {
        // Simple conversion for now: take the last user message or join all
        // Most CLI tools expect a single prompt string
        let mut prompt = String::new();
        for msg in messages {
            if let Some(text) = msg.content.as_text() {
                prompt.push_str(&format!("{}: {}
", match msg.role {
                    crate::llm::Role::System => "System",
                    crate::llm::Role::User => "User",
                    crate::llm::Role::Assistant => "Assistant",
                    crate::llm::Role::Tool => "Tool",
                }, text));
            }
        }
        Ok(prompt)
    }

    fn parse_response(&self, output: String) -> Result<LlmResponse> {
        Ok(LlmResponse::Text {
            text: output.trim().to_string(),
            usage: None,
        })
    }

    fn get_args(&self, model: &str, prompt: &str) -> Vec<String> {
        vec![
            "chat".to_string(),
            "--model".to_string(),
            model.to_string(),
            "--prompt".to_string(),
            prompt.to_string(),
        ]
    }
}
