//! OpenAI LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official OpenAI endpoints.
//! The OPENAI_API_KEY is never sent to any third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, ContentPart, LlmProvider, LlmResponse, Message, MessageContent,
    RefactoringSuggestion, Role, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

/// Official OpenAI API endpoint - API key is ONLY sent here
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";

pub struct OpenAiProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: usize,
}

impl OpenAiProvider {
    pub fn new() -> Result<Self> {
        let api_key =
            env::var("OPENAI_API_KEY").context("OPENAI_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "gpt-4o".to_string(),
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

    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAiMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                };

                // Handle different message content types
                match &msg.content {
                    MessageContent::Text(text) => OpenAiMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                        tool_call_id: msg.tool_call_id.clone(),
                    },
                    MessageContent::Parts(parts) => {
                        // Check if this is an assistant message with tool calls
                        let tool_calls: Vec<OpenAiToolCall> = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolUse { id, name, input } = p {
                                    Some(OpenAiToolCall {
                                        id: id.clone(),
                                        call_type: "function".to_string(),
                                        function: OpenAiFunctionCall {
                                            name: name.clone(),
                                            arguments: serde_json::to_string(input)
                                                .unwrap_or_default(),
                                        },
                                    })
                                } else {
                                    None
                                }
                            })
                            .collect();

                        if !tool_calls.is_empty() && msg.role == Role::Assistant {
                            // Assistant message with tool calls - no content
                            OpenAiMessage {
                                role: role.to_string(),
                                content: None,
                                tool_calls: Some(tool_calls),
                                tool_call_id: None,
                            }
                        } else {
                            // Regular parts message - extract text
                            let text = parts.iter().find_map(|p| {
                                if let ContentPart::Text { text } = p {
                                    Some(text.clone())
                                } else {
                                    None
                                }
                            });
                            OpenAiMessage {
                                role: role.to_string(),
                                content: text,
                                tool_calls: None,
                                tool_call_id: msg.tool_call_id.clone(),
                            }
                        }
                    }
                }
            })
            .collect()
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OpenAiTool> {
        tools
            .iter()
            .map(|t| OpenAiTool {
                tool_type: "function".to_string(),
                function: OpenAiFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    async fn send_request(&self, request: OpenAiRequest) -> Result<OpenAiResponse> {
        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
        }

        response
            .json::<OpenAiResponse>()
            .await
            .context("Failed to parse OpenAI API response")
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn name(&self) -> &str {
        "openai"
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let openai_messages = self.convert_messages(messages);

        let mut request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(self.max_tokens),
            messages: openai_messages,
            tools: None,
            tool_choice: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let response = self.send_request(request).await?;

        if let Some(choice) = response.choices.first() {
            let text = choice.message.content.clone();
            let tool_calls: Vec<ToolCall> = choice
                .message
                .tool_calls
                .as_ref()
                .map(|calls| {
                    calls
                        .iter()
                        .map(|tc| ToolCall {
                            id: tc.id.clone(),
                            name: tc.function.name.clone(),
                            arguments: serde_json::from_str(&tc.function.arguments)
                                .unwrap_or(serde_json::Value::Null),
                        })
                        .collect()
                })
                .unwrap_or_default();

            if tool_calls.is_empty() {
                Ok(LlmResponse::Text(text.unwrap_or_default()))
            } else if text.is_none() || text.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
                Ok(LlmResponse::ToolCalls(tool_calls))
            } else {
                Ok(LlmResponse::Mixed { text, tool_calls })
            }
        } else {
            Ok(LlmResponse::Text(String::new()))
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

        let request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(256),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(system),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            tool_choice: None,
        };

        let response = self.send_request(request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(choice
                .message
                .content
                .clone()
                .unwrap_or_default()
                .trim()
                .to_string())
        } else {
            Ok(String::new())
        }
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let system =
            "You are a helpful code assistant. Explain the provided code clearly and concisely. \
                      Focus on what the code does, its purpose, and any important details.";

        let user_content = format!("Explain this code:\n\n```\n{code}\n```\n\nContext:\n{context}");

        let request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(1024),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(system.to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            tool_choice: None,
        };

        let response = self.send_request(request).await?;

        if let Some(choice) = response.choices.first() {
            Ok(choice
                .message
                .content
                .clone()
                .unwrap_or_else(|| "No explanation available.".to_string()))
        } else {
            Ok("No explanation available.".to_string())
        }
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

        let request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(2048),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(system.to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            tool_choice: None,
        };

        let response = self.send_request(request).await?;

        if let Some(choice) = response.choices.first() {
            if let Some(text) = &choice.message.content {
                if let Ok(suggestions) = serde_json::from_str::<Vec<RefactoringSuggestion>>(text) {
                    return Ok(suggestions);
                }
                // Try to extract JSON from markdown
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

        let request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(2048),
            messages: vec![
                OpenAiMessage {
                    role: "system".to_string(),
                    content: Some(system.to_string()),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenAiMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            tool_choice: None,
        };

        let response = self.send_request(request).await?;

        if let Some(choice) = response.choices.first() {
            if let Some(text) = &choice.message.content {
                if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(text) {
                    return Ok(issues);
                }
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

// OpenAI API types

#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

#[derive(Debug, Serialize)]
struct OpenAiFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
}
