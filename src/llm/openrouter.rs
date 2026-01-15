//! OpenRouter LLM provider implementation
//!
//! OpenRouter provides access to 200+ models from various providers through
//! an OpenAI-compatible API.
//!
//! SECURITY: API keys are ONLY sent to official OpenRouter endpoints.
//! The OPENROUTER_API_KEY is never sent to any third-party services.

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, ContentPart, LlmProvider, LlmResponse, Message, MessageContent,
    RefactoringSuggestion, Role, StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage,
    ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

/// Official OpenRouter API endpoint
const OPENROUTER_API_URL: &str = "https://openrouter.ai/api/v1/chat/completions";

pub struct OpenRouterProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    max_tokens: usize,
    site_url: Option<String>,
    app_name: Option<String>,
}

impl OpenRouterProvider {
    pub fn new() -> Result<Self> {
        let api_key = env::var("OPENROUTER_API_KEY")
            .context("OPENROUTER_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "anthropic/claude-sonnet-4".to_string(),
            max_tokens: 4096,
            site_url: None,
            app_name: Some("Tark".to_string()),
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

    pub fn with_site_url(mut self, site_url: &str) -> Self {
        self.site_url = Some(site_url.to_string());
        self
    }

    pub fn with_app_name(mut self, app_name: &str) -> Self {
        self.app_name = Some(app_name.to_string());
        self
    }

    /// Convert our messages to OpenAI format (OpenRouter is OpenAI-compatible)
    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenRouterMessage> {
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
                    MessageContent::Text(text) => OpenRouterMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                        tool_call_id: msg.tool_call_id.clone(),
                    },
                    MessageContent::Parts(parts) => {
                        // Check if this is an assistant message with tool calls
                        let tool_calls: Vec<OpenRouterToolCall> = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolUse {
                                    id, name, input, ..
                                } = p
                                {
                                    Some(OpenRouterToolCall {
                                        id: id.clone(),
                                        call_type: "function".to_string(),
                                        function: OpenRouterFunctionCall {
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
                            // Assistant message with tool calls
                            OpenRouterMessage {
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
                            OpenRouterMessage {
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

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<OpenRouterTool> {
        tools
            .iter()
            .map(|t| OpenRouterTool {
                tool_type: "function".to_string(),
                function: OpenRouterFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
            })
            .collect()
    }

    async fn send_request(&self, request: OpenRouterRequest) -> Result<OpenRouterResponse> {
        let mut req_builder = self
            .client
            .post(OPENROUTER_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        // Add optional headers
        if let Some(ref site_url) = self.site_url {
            req_builder = req_builder.header("HTTP-Referer", site_url);
        }
        if let Some(ref app_name) = self.app_name {
            req_builder = req_builder.header("X-Title", app_name);
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send request to OpenRouter API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenRouter API error ({}): {}", status, error_text);
        }

        response
            .json::<OpenRouterResponse>()
            .await
            .context("Failed to parse OpenRouter API response")
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
        // Try models.dev first - model format is usually "provider/model-name"
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
        let openrouter_messages = self.convert_messages(messages);

        let mut request = OpenRouterRequest {
            model: self.model.clone(),
            max_tokens: Some(self.max_tokens),
            messages: openrouter_messages,
            tools: None,
            tool_choice: None,
            stream: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let response = self.send_request(request).await?;

        // Convert OpenRouter usage to our TokenUsage type
        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

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
                            thought_signature: None,
                        })
                        .collect()
                })
                .unwrap_or_default();

            if tool_calls.is_empty() {
                Ok(LlmResponse::Text {
                    text: text.unwrap_or_default(),
                    usage,
                })
            } else if text.is_none() || text.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
                Ok(LlmResponse::ToolCalls {
                    calls: tool_calls,
                    usage,
                })
            } else {
                Ok(LlmResponse::Mixed {
                    text,
                    tool_calls,
                    usage,
                })
            }
        } else {
            Ok(LlmResponse::Text {
                text: String::new(),
                usage,
            })
        }
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
        use futures::StreamExt;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let openrouter_messages = self.convert_messages(messages);

        let mut request = OpenRouterRequest {
            model: self.model.clone(),
            max_tokens: Some(self.max_tokens),
            messages: openrouter_messages,
            tools: None,
            tool_choice: None,
            stream: Some(true),
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let mut req_builder = self
            .client
            .post(OPENROUTER_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json");

        if let Some(ref site_url) = self.site_url {
            req_builder = req_builder.header("HTTP-Referer", site_url);
        }
        if let Some(ref app_name) = self.app_name {
            req_builder = req_builder.header("X-Title", app_name);
        }

        let response = req_builder
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to OpenRouter API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "OpenRouter API error ({}): {}",
                status, error_text
            )));
            anyhow::bail!("OpenRouter API error ({}): {}", status, error_text);
        }

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // Track tool calls by index
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new();

        let mut last_activity_at = std::time::Instant::now();
        loop {
            // Check for user interrupt frequently so Ctrl+C/Esc+Esc are responsive
            if let Some(check) = interrupt_check {
                if check() {
                    for (id, name, args) in tool_call_map.values() {
                        builder
                            .tool_calls
                            .insert(id.clone(), (name.clone(), args.clone(), None));
                    }
                    return Ok(builder.build());
                }
            }

            // Enforce per-chunk timeout: if we haven't received any bytes recently, abort.
            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                anyhow::bail!(
                    "Stream timeout - no response from OpenRouter for {} seconds",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                );
            }

            // Use a short poll interval so interrupts can be observed quickly even when
            // the server is silent, while still enforcing an overall 60s no-data timeout.
            let chunk_result = match timeout(INTERRUPT_POLL_INTERVAL, stream.next()).await {
                Ok(Some(res)) => res,
                Ok(None) => break,  // Stream ended
                Err(_) => continue, // Poll interval elapsed - re-check interrupt/timeout
            };

            last_activity_at = std::time::Instant::now();
            let chunk = chunk_result.context("Error reading stream chunk")?;
            let chunk_str = String::from_utf8_lossy(&chunk);

            buffer.push_str(&chunk_str);

            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str == "[DONE]" {
                        callback(StreamEvent::Done);
                        continue;
                    }

                    if let Ok(chunk) = serde_json::from_str::<OpenRouterStreamChunk>(json_str) {
                        if let Some(choice) = chunk.choices.first() {
                            // Handle text content
                            if let Some(content) = &choice.delta.content {
                                if !content.is_empty() {
                                    let event = StreamEvent::TextDelta(content.clone());
                                    builder.process(&event);
                                    callback(event);
                                }
                            }

                            // Handle tool calls
                            if let Some(tool_calls) = &choice.delta.tool_calls {
                                for tc in tool_calls {
                                    let idx = tc.index;

                                    if let Some(id) = &tc.id {
                                        let name = tc
                                            .function
                                            .as_ref()
                                            .and_then(|f| f.name.clone())
                                            .unwrap_or_default();
                                        tool_call_map
                                            .insert(idx, (id.clone(), name.clone(), String::new()));

                                        let event = StreamEvent::ToolCallStart {
                                            id: id.clone(),
                                            name,
                                            thought_signature: None,
                                        };
                                        builder.process(&event);
                                        callback(event);
                                    }

                                    if let Some(func) = &tc.function {
                                        if let Some(args) = &func.arguments {
                                            if !args.is_empty() {
                                                if let Some((id, _, accumulated)) =
                                                    tool_call_map.get_mut(&idx)
                                                {
                                                    accumulated.push_str(args);
                                                    let event = StreamEvent::ToolCallDelta {
                                                        id: id.clone(),
                                                        arguments_delta: args.clone(),
                                                    };
                                                    builder.process(&event);
                                                    callback(event);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Check for finish_reason
                            if choice.finish_reason.is_some() {
                                for (id, _, _) in tool_call_map.values() {
                                    let event = StreamEvent::ToolCallComplete { id: id.clone() };
                                    callback(event);
                                }
                            }
                        }

                        // Capture usage if provided
                        if let Some(usage) = chunk.usage {
                            builder.usage = Some(TokenUsage {
                                input_tokens: usage.prompt_tokens,
                                output_tokens: usage.completion_tokens,
                                total_tokens: usage.total_tokens,
                            });
                        }
                    }
                }
            }
        }

        // Add tool calls to builder
        for (_, (id, name, args)) in tool_call_map {
            builder.tool_calls.insert(id, (name, args, None));
        }

        Ok(builder.build())
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        let system = format!(
            "You are a code completion engine. Complete the code where <CURSOR> is placed. \
             Output ONLY the completion text. Language: {language}"
        );

        let user_content = format!("{prefix}<CURSOR>{suffix}");

        let request = OpenRouterRequest {
            model: self.model.clone(),
            max_tokens: Some(256),
            messages: vec![
                OpenRouterMessage {
                    role: "system".to_string(),
                    content: Some(system),
                    tool_calls: None,
                    tool_call_id: None,
                },
                OpenRouterMessage {
                    role: "user".to_string(),
                    content: Some(user_content),
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            tools: None,
            tool_choice: None,
            stream: None,
        };

        let response = self.send_request(request).await?;

        let text = if let Some(choice) = response.choices.first() {
            choice
                .message
                .content
                .clone()
                .unwrap_or_default()
                .trim()
                .to_string()
        } else {
            String::new()
        };

        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(CompletionResult { text, usage })
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let messages = vec![
            Message::system("You are a helpful code assistant."),
            Message::user(format!(
                "Explain this code:\n\n```\n{code}\n```\n\nContext:\n{context}"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        Ok(response
            .text()
            .unwrap_or("No explanation available.")
            .to_string())
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        let messages = vec![
            Message::system(
                r#"You are a code refactoring assistant. Return JSON array:
[{"title": "...", "description": "...", "new_code": "..."}]"#,
            ),
            Message::user(format!(
                "Suggest refactorings:\n\n```\n{code}\n```\n\nContext:\n{context}"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        if let Some(text) = response.text() {
            if let Ok(suggestions) = serde_json::from_str::<Vec<RefactoringSuggestion>>(text) {
                return Ok(suggestions);
            }
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
        Ok(Vec::new())
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        let messages = vec![
            Message::system(
                r#"You are a code review assistant. Return JSON array:
[{"severity": "error|warning|info|hint", "message": "...", "line": 1}]"#,
            ),
            Message::user(format!(
                "Review this {language} code:\n\n```{language}\n{code}\n```"
            )),
        ];

        let response = self.chat(&messages, None).await?;
        if let Some(text) = response.text() {
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
        Ok(Vec::new())
    }
}

// OpenRouter API types (OpenAI-compatible)

#[derive(Debug, Serialize)]
struct OpenRouterRequest {
    model: String,
    messages: Vec<OpenRouterMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenRouterTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_choice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenRouterMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenRouterToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenRouterToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenRouterFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenRouterFunctionCall {
    name: String,
    arguments: String,
}

#[derive(Debug, Serialize)]
struct OpenRouterTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenRouterFunction,
}

#[derive(Debug, Serialize)]
struct OpenRouterFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct OpenRouterResponse {
    choices: Vec<OpenRouterChoice>,
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterChoice {
    message: OpenRouterMessage,
}

#[derive(Debug, Deserialize)]
struct OpenRouterUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChunk {
    choices: Vec<OpenRouterStreamChoice>,
    #[serde(default)]
    usage: Option<OpenRouterUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamChoice {
    delta: OpenRouterStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenRouterStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenRouterStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenRouterStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenRouterStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openrouter_response() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello, world!"
                }
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let response: OpenRouterResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_parse_openrouter_tool_calls() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "test_tool",
                            "arguments": "{\"arg\": \"value\"}"
                        }
                    }]
                }
            }]
        }"#;

        let response: OpenRouterResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.choices.len(), 1);
        assert!(response.choices[0].message.tool_calls.is_some());
    }
}
