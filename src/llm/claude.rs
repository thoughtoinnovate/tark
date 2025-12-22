//! Claude (Anthropic) LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official Anthropic endpoints.
//! The ANTHROPIC_API_KEY is never sent to any third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role, ToolCall,
    ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

/// Official Anthropic API endpoint - API key is ONLY sent here
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";
const ANTHROPIC_VERSION: &str = "2023-06-01";

pub struct ClaudeProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl ClaudeProvider {
    pub fn new() -> Result<Self> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
        })
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<ClaudeMessage>) {
        let mut system_prompt = None;
        let mut claude_messages = Vec::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if let Some(text) = msg.content.as_text() {
                        system_prompt = Some(text.to_string());
                    }
                }
                Role::User => {
                    if let Some(text) = msg.content.as_text() {
                        claude_messages.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Text(text.to_string()),
                        });
                    }
                }
                Role::Assistant => {
                    if let Some(text) = msg.content.as_text() {
                        claude_messages.push(ClaudeMessage {
                            role: "assistant".to_string(),
                            content: ClaudeContent::Text(text.to_string()),
                        });
                    }
                }
                Role::Tool => {
                    if let (Some(text), Some(tool_id)) = (msg.content.as_text(), &msg.tool_call_id)
                    {
                        claude_messages.push(ClaudeMessage {
                            role: "user".to_string(),
                            content: ClaudeContent::Blocks(vec![ClaudeContentBlock::ToolResult {
                                tool_use_id: tool_id.clone(),
                                content: text.to_string(),
                            }]),
                        });
                    }
                }
            }
        }

        (system_prompt, claude_messages)
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<ClaudeTool> {
        tools
            .iter()
            .map(|t| ClaudeTool {
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema: t.parameters.clone(),
            })
            .collect()
    }

    async fn send_request(&self, request: ClaudeRequest) -> Result<ClaudeResponse> {
        let response = self
            .client
            .post(ANTHROPIC_API_URL)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .header("content-type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Anthropic API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic API error ({}): {}", status, error_text);
        }

        response
            .json::<ClaudeResponse>()
            .await
            .context("Failed to parse Anthropic API response")
    }
}

#[async_trait]
impl LlmProvider for ClaudeProvider {
    fn name(&self) -> &str {
        "claude"
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let (system, claude_messages) = self.convert_messages(messages);

        let mut request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            system,
            messages: claude_messages,
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
            }
        }

        let response = self.send_request(request).await?;

        // Parse response content
        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in response.content {
            match block {
                ClaudeContentBlock::Text { text } => {
                    text_parts.push(text);
                }
                ClaudeContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id,
                        name,
                        arguments: input,
                    });
                }
                _ => {}
            }
        }

        if tool_calls.is_empty() {
            Ok(LlmResponse::Text(text_parts.join("\n")))
        } else if text_parts.is_empty() {
            Ok(LlmResponse::ToolCalls(tool_calls))
        } else {
            Ok(LlmResponse::Mixed {
                text: Some(text_parts.join("\n")),
                tool_calls,
            })
        }
    }

    async fn complete_fim(&self, prefix: &str, suffix: &str, language: &str) -> Result<String> {
        let system = format!(
            "You are a code completion engine. Complete the code where <CURSOR> is placed. \
             Output ONLY the completion text that should be inserted at the cursor position. \
             Do not include any explanation, markdown formatting, or the surrounding code. \
             Language: {language}"
        );

        let user_content = format!("{prefix}<CURSOR>{suffix}");

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 256,
            system: Some(system),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text(user_content),
            }],
            tools: None,
        };

        let response = self.send_request(request).await?;

        // Extract text from response
        for block in response.content {
            if let ClaudeContentBlock::Text { text } = block {
                return Ok(text.trim().to_string());
            }
        }

        Ok(String::new())
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let system =
            "You are a helpful code assistant. Explain the provided code clearly and concisely. \
                      Focus on what the code does, its purpose, and any important details.";

        let user_content = format!("Explain this code:\n\n```\n{code}\n```\n\nContext:\n{context}");

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 1024,
            system: Some(system.to_string()),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text(user_content),
            }],
            tools: None,
        };

        let response = self.send_request(request).await?;

        for block in response.content {
            if let ClaudeContentBlock::Text { text } = block {
                return Ok(text);
            }
        }

        Ok("No explanation available.".to_string())
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        let system = r#"You are a code refactoring assistant. Analyze the provided code and suggest improvements.
Return your suggestions as a JSON array with this structure:
[{"title": "Brief title", "description": "Why this helps", "new_code": "The refactored code"}]
Only return the JSON array, no other text."#;

        let user_content = format!(
            "Suggest refactorings for this code:\n\n```\n{code}\n```\n\nContext:\n{context}"
        );

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 2048,
            system: Some(system.to_string()),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text(user_content),
            }],
            tools: None,
        };

        let response = self.send_request(request).await?;

        for block in response.content {
            if let ClaudeContentBlock::Text { text } = block {
                // Try to parse as JSON
                if let Ok(suggestions) = serde_json::from_str::<Vec<RefactoringSuggestion>>(&text) {
                    return Ok(suggestions);
                }
                // Try to extract JSON from markdown code blocks
                if let Some(json_start) = text.find('[') {
                    if let Some(json_end) = text.rfind(']') {
                        let json_str = &text[json_start..=json_end];
                        if let Ok(suggestions) =
                            serde_json::from_str::<Vec<RefactoringSuggestion>>(json_str)
                        {
                            return Ok(suggestions);
                        }
                    }
                }
            }
        }

        Ok(Vec::new())
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        let system = r#"You are a code review assistant. Analyze the provided code for potential issues.
Return your findings as a JSON array with this structure:
[{"severity": "error|warning|info|hint", "message": "Description", "line": 1, "end_line": null, "column": null, "end_column": null}]
Line numbers are 1-indexed. Only return the JSON array, no other text.
Focus on: bugs, security issues, performance problems, and code quality."#;

        let user_content = format!("Review this {language} code:\n\n```{language}\n{code}\n```");

        let request = ClaudeRequest {
            model: self.model.clone(),
            max_tokens: 2048,
            system: Some(system.to_string()),
            messages: vec![ClaudeMessage {
                role: "user".to_string(),
                content: ClaudeContent::Text(user_content),
            }],
            tools: None,
        };

        let response = self.send_request(request).await?;

        for block in response.content {
            if let ClaudeContentBlock::Text { text } = block {
                // Try to parse as JSON
                if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(&text) {
                    return Ok(issues);
                }
                // Try to extract JSON from response
                if let Some(json_start) = text.find('[') {
                    if let Some(json_end) = text.rfind(']') {
                        let json_str = &text[json_start..=json_end];
                        if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(json_str) {
                            return Ok(issues);
                        }
                    }
                }
            }
        }

        Ok(Vec::new())
    }
}

// Claude API request/response types

#[derive(Debug, Serialize)]
struct ClaudeRequest {
    model: String,
    max_tokens: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ClaudeTool>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ClaudeMessage {
    role: String,
    content: ClaudeContent,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum ClaudeContent {
    Text(String),
    Blocks(Vec<ClaudeContentBlock>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum ClaudeContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

#[derive(Debug, Serialize)]
struct ClaudeTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContentBlock>,
    #[allow(dead_code)]
    stop_reason: Option<String>,
}
