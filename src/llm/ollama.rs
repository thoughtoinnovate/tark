//! Ollama LLM provider implementation (local models)

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role,
    StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

impl OllamaProvider {
    pub fn new() -> Result<Self> {
        let base_url =
            env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
        let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| "codellama".to_string());

        Ok(Self {
            client: reqwest::Client::new(),
            base_url,
            model,
        })
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "user", // Ollama doesn't have tool role, use user
                };

                OllamaMessage {
                    role: role.to_string(),
                    content: msg.content.as_text().unwrap_or("").to_string(),
                }
            })
            .collect()
    }

    async fn send_request(&self, request: OllamaRequest) -> Result<OllamaResponse> {
        let url = format!("{}/api/chat", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error ({}): {}", status, error_text);
        }

        response
            .json::<OllamaResponse>()
            .await
            .context("Failed to parse Ollama response")
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);

        let request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Ollama API error ({}): {}", status, error_text);
        }

        let resp: OllamaGenerateResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        Ok(resp.response)
    }
}

#[async_trait]
impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let ollama_messages = self.convert_messages(messages);

        // Note: Most Ollama models don't support tool calling natively
        // We'll handle tools via prompting if needed
        if let Some(tools) = tools {
            if !tools.is_empty() {
                // Add tool descriptions to system prompt
                let tool_desc = tools
                    .iter()
                    .map(|t| format!("- {}: {}", t.name, t.description))
                    .collect::<Vec<_>>()
                    .join("\n");

                let mut modified_messages = ollama_messages.clone();

                // Find or create system message
                if let Some(sys_msg) = modified_messages.iter_mut().find(|m| m.role == "system") {
                    sys_msg.content = format!(
                        "{}\n\nYou have access to these tools:\n{}\n\nTo use a tool, respond with JSON: {{\"tool\": \"name\", \"args\": {{...}}}}",
                        sys_msg.content, tool_desc
                    );
                } else {
                    modified_messages.insert(0, OllamaMessage {
                        role: "system".to_string(),
                        content: format!(
                            "You have access to these tools:\n{}\n\nTo use a tool, respond with JSON: {{\"tool\": \"name\", \"args\": {{...}}}}",
                            tool_desc
                        ),
                    });
                }

                let request = OllamaRequest {
                    model: self.model.clone(),
                    messages: modified_messages,
                    stream: false,
                };

                let response = self.send_request(request).await?;
                let content = response.message.content;

                // Try to parse tool call from response
                if let Some(tool_call) = self.parse_tool_call(&content) {
                    return Ok(LlmResponse::ToolCalls {
                        calls: vec![tool_call],
                        usage: None, // Ollama doesn't provide usage info
                    });
                }

                return Ok(LlmResponse::Text {
                    text: content,
                    usage: None, // Ollama doesn't provide usage info
                });
            }
        }

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: false,
        };

        let response = self.send_request(request).await?;
        Ok(LlmResponse::Text {
            text: response.message.content,
            usage: None, // Ollama doesn't provide usage info
        })
    }

    fn supports_streaming(&self) -> bool {
        true // Ollama supports native streaming
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;

        let ollama_messages = self.convert_messages(messages);

        // Handle tools via prompting (same as non-streaming)
        let (messages_to_send, has_tools) = if let Some(tools) = tools {
            if !tools.is_empty() {
                let tool_desc = tools
                    .iter()
                    .map(|t| format!("- {}: {}", t.name, t.description))
                    .collect::<Vec<_>>()
                    .join("\n");

                let mut modified_messages = ollama_messages.clone();

                if let Some(sys_msg) = modified_messages.iter_mut().find(|m| m.role == "system") {
                    sys_msg.content = format!(
                        "{}\n\nYou have access to these tools:\n{}\n\nTo use a tool, respond with JSON: {{\"tool\": \"name\", \"args\": {{...}}}}",
                        sys_msg.content, tool_desc
                    );
                } else {
                    modified_messages.insert(0, OllamaMessage {
                        role: "system".to_string(),
                        content: format!(
                            "You have access to these tools:\n{}\n\nTo use a tool, respond with JSON: {{\"tool\": \"name\", \"args\": {{...}}}}",
                            tool_desc
                        ),
                    });
                }

                (modified_messages, true)
            } else {
                (ollama_messages, false)
            }
        } else {
            (ollama_messages, false)
        };

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: messages_to_send,
            stream: true, // Enable streaming
        };

        let url = format!("{}/api/chat", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Ollama")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "Ollama API error ({}): {}",
                status, error_text
            )));
            anyhow::bail!("Ollama API error ({}): {}", status, error_text);
        }

        // Process newline-delimited JSON stream
        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error reading stream chunk")?;
            let chunk_str = String::from_utf8_lossy(&chunk);

            buffer.push_str(&chunk_str);

            // Process complete lines (newline-delimited JSON)
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                // Parse the JSON chunk
                if let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(&line) {
                    if chunk.done {
                        callback(StreamEvent::Done);
                        continue;
                    }

                    if let Some(message) = chunk.message {
                        if !message.content.is_empty() {
                            let event = StreamEvent::TextDelta(message.content);
                            builder.process(&event);
                            callback(event);
                        }
                    }
                }
            }
        }

        // Try to parse tool call from accumulated text (if tools were provided)
        let final_text = builder.text.clone();
        if has_tools {
            if let Some(tool_call) = self.parse_tool_call(&final_text) {
                return Ok(LlmResponse::ToolCalls {
                    calls: vec![tool_call],
                    usage: None,
                });
            }
        }

        Ok(builder.build())
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        // Use generate API for FIM-style completion
        let prompt = format!(
            "Complete the following {} code. Only output the code that goes between the prefix and suffix, nothing else.\n\nPrefix:\n```\n{}\n```\n\nSuffix:\n```\n{}\n```\n\nCompletion:",
            language, prefix, suffix
        );

        let response = self.generate(&prompt).await?;

        // Clean up the response
        let cleaned = response
            .trim()
            .trim_start_matches("```")
            .trim_start_matches(language)
            .trim_end_matches("```")
            .trim()
            .to_string();

        // Estimate tokens (Ollama doesn't always return usage in generate API)
        let input_tokens = (prefix.len() + suffix.len() + prompt.len()) / 4;
        let output_tokens = cleaned.len() / 4;

        Ok(CompletionResult {
            text: cleaned,
            usage: Some(TokenUsage {
                input_tokens: input_tokens as u32,
                output_tokens: output_tokens as u32,
                total_tokens: (input_tokens + output_tokens) as u32,
            }),
        })
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let messages = vec![
            OllamaMessage {
                role: "system".to_string(),
                content: "You are a helpful code assistant. Explain code clearly and concisely."
                    .to_string(),
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!(
                    "Explain this code:\n\n```\n{}\n```\n\nContext:\n{}",
                    code, context
                ),
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
        };

        let response = self.send_request(request).await?;
        Ok(response.message.content)
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        let messages = vec![
            OllamaMessage {
                role: "system".to_string(),
                content: r#"You are a code refactoring assistant. Suggest improvements and return them as a JSON array:
[{"title": "Brief title", "description": "Why this helps", "new_code": "The refactored code"}]
Only return valid JSON, no other text."#.to_string(),
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!("Suggest refactorings for:\n\n```\n{}\n```\n\nContext:\n{}", code, context),
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
        };

        let response = self.send_request(request).await?;
        let text = &response.message.content;

        // Try to parse JSON
        if let Ok(suggestions) = serde_json::from_str::<Vec<RefactoringSuggestion>>(text) {
            return Ok(suggestions);
        }

        // Try to extract JSON from response
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                let json_str = &text[start..=end];
                if let Ok(suggestions) =
                    serde_json::from_str::<Vec<RefactoringSuggestion>>(json_str)
                {
                    return Ok(suggestions);
                }
            }
        }

        Ok(Vec::new())
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        let messages = vec![
            OllamaMessage {
                role: "system".to_string(),
                content: r#"You are a code review assistant. Find potential issues and return them as a JSON array:
[{"severity": "error|warning|info|hint", "message": "Description", "line": 1, "end_line": null, "column": null, "end_column": null}]
Line numbers are 1-indexed. Only return valid JSON, no other text."#.to_string(),
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!("Review this {} code:\n\n```{}\n{}\n```", language, language, code),
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
        };

        let response = self.send_request(request).await?;
        let text = &response.message.content;

        // Try to parse JSON
        if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(text) {
            return Ok(issues);
        }

        // Try to extract JSON
        if let Some(start) = text.find('[') {
            if let Some(end) = text.rfind(']') {
                let json_str = &text[start..=end];
                if let Ok(issues) = serde_json::from_str::<Vec<CodeIssue>>(json_str) {
                    return Ok(issues);
                }
            }
        }

        Ok(Vec::new())
    }
}

impl OllamaProvider {
    fn parse_tool_call(&self, content: &str) -> Option<ToolCall> {
        // Try to find JSON tool call in response
        let content = content.trim();

        // Look for JSON object
        let start = content.find('{')?;
        let end = content.rfind('}')?;
        let json_str = &content[start..=end];

        #[derive(Deserialize)]
        struct ToolCallJson {
            tool: String,
            args: serde_json::Value,
        }

        if let Ok(tc) = serde_json::from_str::<ToolCallJson>(json_str) {
            return Some(ToolCall {
                id: format!("ollama_{}", uuid_simple()),
                name: tc.tool,
                arguments: tc.args,
            });
        }

        None
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before UNIX epoch");
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

// Ollama API types

#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    messages: Vec<OllamaMessage>,
    stream: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

#[derive(Debug, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: String,
}

// Streaming response type
#[derive(Debug, Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    message: Option<OllamaMessage>,
    #[serde(default)]
    done: bool,
}
