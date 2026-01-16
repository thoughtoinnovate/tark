//! Generic OpenAI-compatible LLM provider
//!
//! This module provides a reusable provider implementation for any API that follows
//! the OpenAI chat completions format. It supports:
//! - Gemini (via OpenAI-compatible endpoint)
//! - OpenRouter
//! - GitHub Copilot
//! - Any other OpenAI-compatible API
//!
//! SECURITY: Credentials are only sent to the configured endpoint.

#![allow(dead_code)]

use super::{
    streaming::SseDecoder, CodeIssue, CompletionResult, ContentPart, LlmError, LlmProvider,
    LlmResponse, Message, MessageContent, RefactoringSuggestion, Role, StreamCallback, StreamEvent,
    StreamingResponseBuilder, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// Configuration Types
// ============================================================================

/// Authentication method for the API
#[derive(Debug, Clone)]
pub enum AuthMethod {
    /// Bearer token in Authorization header
    BearerToken(String),
    /// API key in a custom header
    ApiKeyHeader { header: String, key: String },
}

/// Configuration for an OpenAI-compatible provider
#[derive(Debug, Clone)]
pub struct OpenAiCompatConfig {
    /// Provider name (e.g., "gemini", "openrouter", "copilot")
    pub name: String,
    /// Base URL for chat completions endpoint
    pub base_url: String,
    /// Authentication method
    pub auth: AuthMethod,
    /// Default model to use
    pub default_model: String,
    /// Maximum output tokens
    pub max_tokens: usize,
    /// Custom headers to send with requests
    pub custom_headers: Vec<(String, String)>,
    /// Whether the provider supports streaming
    pub supports_streaming: bool,
    /// Whether the provider supports tool/function calling
    pub supports_tools: bool,
}

impl OpenAiCompatConfig {
    /// Create a new configuration with minimal required fields
    pub fn new(name: impl Into<String>, base_url: impl Into<String>, auth: AuthMethod) -> Self {
        Self {
            name: name.into(),
            base_url: base_url.into(),
            auth,
            default_model: String::new(),
            max_tokens: 4096, // Fallback default; config overrides this
            custom_headers: Vec::new(),
            supports_streaming: true,
            supports_tools: true,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.default_model = model.into();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.custom_headers.push((name.into(), value.into()));
        self
    }

    pub fn with_streaming(mut self, supported: bool) -> Self {
        self.supports_streaming = supported;
        self
    }

    pub fn with_tools(mut self, supported: bool) -> Self {
        self.supports_tools = supported;
        self
    }
}

// ============================================================================
// Provider Implementation
// ============================================================================

/// Generic OpenAI-compatible provider
pub struct OpenAiCompatProvider {
    client: reqwest::Client,
    config: OpenAiCompatConfig,
    model: String,
    max_tokens: usize,
    /// Reasoning effort for thinking models ("low", "medium", "high")
    reasoning_effort: Option<String>,
}

impl OpenAiCompatProvider {
    /// Create a new provider with the given configuration
    pub fn new(config: OpenAiCompatConfig) -> Self {
        let model = config.default_model.clone();
        let max_tokens = config.max_tokens;
        Self {
            client: reqwest::Client::new(),
            config,
            model,
            max_tokens,
            reasoning_effort: None,
        }
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set max output tokens
    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set reasoning effort for thinking models
    pub fn with_reasoning_effort(mut self, effort: impl Into<String>) -> Self {
        self.reasoning_effort = Some(effort.into());
        self
    }

    /// Get the current model
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Get provider name
    pub fn provider_name(&self) -> &str {
        &self.config.name
    }

    /// Update the auth token (useful for token refresh in Copilot)
    pub fn set_auth(&mut self, auth: AuthMethod) {
        self.config.auth = auth;
    }

    /// Set reasoning effort dynamically
    pub fn set_reasoning_effort(&mut self, effort: Option<String>) {
        self.reasoning_effort = effort;
    }

    // ========================================================================
    // Message Conversion
    // ========================================================================

    /// Convert internal messages to OpenAI format
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

                match &msg.content {
                    MessageContent::Text(text) => OpenAiMessage {
                        role: role.to_string(),
                        content: Some(text.clone()),
                        tool_calls: None,
                        tool_call_id: msg.tool_call_id.clone(),
                    },
                    MessageContent::Parts(parts) => {
                        // Extract tool calls from parts
                        let tool_calls: Vec<OpenAiToolCall> = parts
                            .iter()
                            .filter_map(|p| {
                                if let ContentPart::ToolUse {
                                    id, name, input, ..
                                } = p
                                {
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

                        // Extract text content
                        let text_content = parts.iter().find_map(|p| {
                            if let ContentPart::Text { text } = p {
                                Some(text.clone())
                            } else {
                                None
                            }
                        });

                        if !tool_calls.is_empty() && msg.role == Role::Assistant {
                            // Assistant message with tool calls
                            OpenAiMessage {
                                role: role.to_string(),
                                content: text_content, // May have both text and tool calls
                                tool_calls: Some(tool_calls),
                                tool_call_id: None,
                            }
                        } else {
                            // Regular message
                            OpenAiMessage {
                                role: role.to_string(),
                                content: text_content,
                                tool_calls: None,
                                tool_call_id: msg.tool_call_id.clone(),
                            }
                        }
                    }
                }
            })
            .collect()
    }

    /// Convert internal tool definitions to OpenAI format
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

    // ========================================================================
    // Request Building
    // ========================================================================

    /// Build the request body
    fn build_request(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        stream: bool,
    ) -> OpenAiRequest {
        let mut request = OpenAiRequest {
            model: self.model.clone(),
            messages: self.convert_messages(messages),
            max_tokens: Some(self.max_tokens),
            tools: None,
            tool_choice: None,
            stream: if stream { Some(true) } else { None },
            reasoning_effort: self.reasoning_effort.clone(),
        };

        if let Some(tools) = tools {
            if !tools.is_empty() && self.config.supports_tools {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        request
    }

    /// Build request with authorization headers
    fn build_http_request(&self, body: &OpenAiRequest) -> reqwest::RequestBuilder {
        let mut req = self
            .client
            .post(&self.config.base_url)
            .header("Content-Type", "application/json");

        // Add auth
        match &self.config.auth {
            AuthMethod::BearerToken(token) => {
                req = req.header("Authorization", format!("Bearer {}", token));
            }
            AuthMethod::ApiKeyHeader { header, key } => {
                req = req.header(header, key);
            }
        }

        // Add custom headers
        for (name, value) in &self.config.custom_headers {
            req = req.header(name, value);
        }

        req.json(body)
    }

    // ========================================================================
    // Response Parsing
    // ========================================================================

    /// Parse a non-streaming response
    fn parse_response(&self, response: OpenAiResponse) -> LlmResponse {
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
                LlmResponse::Text {
                    text: text.unwrap_or_default(),
                    usage,
                }
            } else if text.is_none() || text.as_ref().map(|t| t.is_empty()).unwrap_or(true) {
                LlmResponse::ToolCalls {
                    calls: tool_calls,
                    usage,
                }
            } else {
                LlmResponse::Mixed {
                    text,
                    tool_calls,
                    usage,
                }
            }
        } else {
            LlmResponse::Text {
                text: String::new(),
                usage,
            }
        }
    }

    // ========================================================================
    // API Methods
    // ========================================================================

    /// Send a non-streaming chat request
    async fn chat_impl(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let request = self.build_request(messages, tools, false);
        let http_req = self.build_http_request(&request);

        // Log request in debug mode
        tracing::debug!(
            target: "llm",
            provider = self.config.name,
            model = self.model,
            messages = messages.len(),
            "Sending chat request"
        );

        let response = http_req
            .send()
            .await
            .with_context(|| format!("Failed to send request to {} API", self.config.name))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(LlmError::from_http_status(status, error_text).into());
        }

        let api_response: OpenAiResponse = response
            .json()
            .await
            .with_context(|| format!("Failed to parse {} API response", self.config.name))?;

        Ok(self.parse_response(api_response))
    }

    /// Send a streaming chat request
    async fn chat_streaming_impl(
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

        let request = self.build_request(messages, tools, true);
        let http_req = self.build_http_request(&request);

        // Log request in debug mode
        tracing::debug!(
            target: "llm",
            provider = self.config.name,
            model = self.model,
            messages = messages.len(),
            "Sending streaming request"
        );

        let response = http_req
            .send()
            .await
            .with_context(|| format!("Failed to send request to {} API", self.config.name))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "{} API error ({}): {}",
                self.config.name, status, error_text
            )));
            return Err(LlmError::from_http_status(status, error_text).into());
        }

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::new();

        // Track tool calls by index (for streaming deltas)
        let mut tool_call_map: HashMap<usize, (String, String, String)> = HashMap::new();

        let mut last_activity_at = std::time::Instant::now();
        loop {
            // Check for user interrupt
            if let Some(check) = interrupt_check {
                if check() {
                    // Finalize any pending tool calls
                    for (id, name, args) in tool_call_map.values() {
                        builder
                            .tool_calls
                            .insert(id.clone(), (name.clone(), args.clone(), None));
                    }
                    return Ok(builder.build());
                }
            }

            // Check for timeout
            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                return Err(LlmError::Network(format!(
                    "Stream timeout - no response from {} for {} seconds",
                    self.config.name,
                    STREAM_CHUNK_TIMEOUT.as_secs()
                ))
                .into());
            }

            // Read next chunk with short timeout for interrupt responsiveness
            let chunk_result = match timeout(INTERRUPT_POLL_INTERVAL, stream.next()).await {
                Ok(Some(res)) => res,
                Ok(None) => break,  // Stream ended
                Err(_) => continue, // Timeout - check interrupt again
            };

            last_activity_at = std::time::Instant::now();
            let chunk = chunk_result.context("Error reading stream chunk")?;

            // Process SSE events
            for payload_json in decoder.push(&chunk) {
                // Log raw response in debug mode
                crate::llm::append_llm_raw_line(&payload_json);

                // Handle [DONE] marker
                if payload_json.trim() == "[DONE]" {
                    callback(StreamEvent::Done);
                    continue;
                }

                if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(&payload_json) {
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

                                // New tool call started
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

                                // Tool call arguments delta
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

                        // Handle finish reason
                        if choice.finish_reason.is_some() {
                            for (id, _, _) in tool_call_map.values() {
                                callback(StreamEvent::ToolCallComplete { id: id.clone() });
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

        // Flush remaining events
        for payload_json in decoder.finish() {
            if payload_json.trim() == "[DONE]" {
                continue;
            }

            if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(&payload_json) {
                if let Some(choice) = chunk.choices.first() {
                    if let Some(content) = &choice.delta.content {
                        if !content.is_empty() {
                            let event = StreamEvent::TextDelta(content.clone());
                            builder.process(&event);
                            callback(event);
                        }
                    }
                }

                if let Some(usage) = chunk.usage {
                    builder.usage = Some(TokenUsage {
                        input_tokens: usage.prompt_tokens,
                        output_tokens: usage.completion_tokens,
                        total_tokens: usage.total_tokens,
                    });
                }
            }
        }

        // Add accumulated tool calls to builder
        for (_, (id, name, args)) in tool_call_map {
            builder.tool_calls.insert(id, (name, args, None));
        }

        callback(StreamEvent::Done);
        Ok(builder.build())
    }
}

// ============================================================================
// LlmProvider Implementation
// ============================================================================

#[async_trait]
impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn supports_native_thinking(&self) -> bool {
        // Check for common thinking-capable model patterns
        self.model.contains("o1")
            || self.model.contains("o3")
            || self.model.contains("o4")
            || self.model.contains("thinking")
            || self.model.contains("gemini-2.5")
            || self.model.contains("gemini-3")
            || self.model.contains("deepseek-r1")
    }

    async fn supports_native_thinking_async(&self) -> bool {
        // Try models.dev first for accurate detection
        let db = super::models_db();
        if db.supports_reasoning(&self.config.name, &self.model).await {
            return true;
        }

        // For OpenRouter-style models (provider/model), try extracting
        if let Some(slash_idx) = self.model.find('/') {
            let provider = &self.model[..slash_idx];
            let model_name = &self.model[slash_idx + 1..];
            if db.supports_reasoning(provider, model_name).await {
                return true;
            }
        }

        // Fallback to sync check
        self.supports_native_thinking()
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        self.chat_impl(messages, tools).await
    }

    fn supports_streaming(&self) -> bool {
        self.config.supports_streaming
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        self.chat_streaming_impl(messages, tools, callback, interrupt_check)
            .await
    }

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        settings: &super::ThinkSettings,
    ) -> Result<LlmResponse> {
        // Set reasoning effort based on settings
        let provider = Self {
            client: self.client.clone(),
            config: self.config.clone(),
            model: self.model.clone(),
            max_tokens: self.max_tokens,
            reasoning_effort: if settings.enabled && !settings.reasoning_effort.is_empty() {
                Some(settings.reasoning_effort.clone())
            } else {
                None
            },
        };

        provider
            .chat_streaming_impl(messages, tools, callback, interrupt_check)
            .await
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

        let messages = vec![
            Message::system(system),
            Message::user(format!("{prefix}<CURSOR>{suffix}")),
        ];

        let response = self.chat_impl(&messages, None).await?;

        let text = response.text().unwrap_or("").trim().to_string();
        let usage = response.usage().cloned();

        Ok(CompletionResult { text, usage })
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let messages = vec![
            Message::system("You are a helpful code assistant."),
            Message::user(format!(
                "Explain this code:\n\n```\n{code}\n```\n\nContext:\n{context}"
            )),
        ];

        let response = self.chat_impl(&messages, None).await?;
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

        let response = self.chat_impl(&messages, None).await?;
        if let Some(text) = response.text() {
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

        let response = self.chat_impl(&messages, None).await?;
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

// ============================================================================
// API Types (OpenAI Format)
// ============================================================================

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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
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
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChoice {
    message: OpenAiResponseMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiResponseMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Streaming types
#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiDelta {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiFunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct OpenAiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

// ============================================================================
// Factory Functions for Common Providers
// ============================================================================

/// Create a Gemini provider using the OpenAI-compatible endpoint
pub fn create_gemini_openai_compat(api_key: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(
        OpenAiCompatConfig::new(
            "gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions",
            AuthMethod::BearerToken(api_key),
        )
        .with_model("gemini-2.5-flash")
        .with_max_tokens(8192),
    )
}

/// Create an OpenRouter provider
pub fn create_openrouter(api_key: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(
        OpenAiCompatConfig::new(
            "openrouter",
            "https://openrouter.ai/api/v1/chat/completions",
            AuthMethod::BearerToken(api_key),
        )
        .with_model("anthropic/claude-sonnet-4")
        .with_max_tokens(4096)
        .with_header("HTTP-Referer", "https://github.com/tark-ai/tark")
        .with_header("X-Title", "Tark"),
    )
}

/// Create a GitHub Copilot provider
pub fn create_copilot(access_token: String) -> OpenAiCompatProvider {
    OpenAiCompatProvider::new(
        OpenAiCompatConfig::new(
            "copilot",
            "https://api.githubcopilot.com/chat/completions",
            AuthMethod::BearerToken(access_token),
        )
        .with_model("gpt-4o")
        .with_max_tokens(4096)
        .with_header("Editor-Version", "Neovim/0.9.0")
        .with_header("Copilot-Integration-Id", "vscode-chat"),
    )
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = OpenAiCompatConfig::new(
            "test",
            "https://api.example.com/v1/chat/completions",
            AuthMethod::BearerToken("test-key".into()),
        )
        .with_model("gpt-4")
        .with_max_tokens(2048)
        .with_header("X-Custom", "value");

        assert_eq!(config.name, "test");
        assert_eq!(config.default_model, "gpt-4");
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.custom_headers.len(), 1);
    }

    #[test]
    fn test_message_conversion() {
        let provider = OpenAiCompatProvider::new(
            OpenAiCompatConfig::new(
                "test",
                "https://api.example.com",
                AuthMethod::BearerToken("key".into()),
            )
            .with_model("gpt-4"),
        );

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi there!"),
        ];

        let converted = provider.convert_messages(&messages);
        assert_eq!(converted.len(), 3);
        assert_eq!(converted[0].role, "system");
        assert_eq!(converted[1].role, "user");
        assert_eq!(converted[2].role, "assistant");
    }

    #[test]
    fn test_tool_conversion() {
        let provider = OpenAiCompatProvider::new(
            OpenAiCompatConfig::new(
                "test",
                "https://api.example.com",
                AuthMethod::BearerToken("key".into()),
            )
            .with_model("gpt-4"),
        );

        let tools = vec![ToolDefinition {
            name: "search".into(),
            description: "Search the web".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
        }];

        let converted = provider.convert_tools(&tools);
        assert_eq!(converted.len(), 1);
        assert_eq!(converted[0].function.name, "search");
        assert_eq!(converted[0].tool_type, "function");
    }

    #[test]
    fn test_thinking_detection() {
        let cases = vec![
            ("gpt-4", false),
            ("o1-preview", true),
            ("o3-mini", true),
            ("gemini-2.5-flash", true),
            ("gemini-3-pro", true),
            ("deepseek-r1", true),
            ("claude-sonnet-4", false),
        ];

        for (model, expected) in cases {
            let provider = OpenAiCompatProvider::new(
                OpenAiCompatConfig::new(
                    "test",
                    "https://api.example.com",
                    AuthMethod::BearerToken("key".into()),
                )
                .with_model(model),
            );
            assert_eq!(
                provider.supports_native_thinking(),
                expected,
                "Model {} should {} support thinking",
                model,
                if expected { "" } else { "not" }
            );
        }
    }
}
