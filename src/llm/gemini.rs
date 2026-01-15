//! Google Gemini LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official Google endpoints.
//! The GEMINI_API_KEY is never sent to any third-party services.
//!
//! Supports two API modes:
//! - Standard API (generativelanguage.googleapis.com) - API key auth
//! - Cloud Code Assist API (cloudcode-pa.googleapis.com) - OAuth Bearer auth

#![allow(dead_code)]

use super::{
    streaming::SseDecoder, CodeIssue, CompletionResult, ContentPart, LlmError, LlmProvider,
    LlmResponse, Message, MessageContent, RefactoringSuggestion, Role, StreamCallback, StreamEvent,
    StreamingResponseBuilder, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::env;

/// Official Google Gemini API endpoint (Standard mode)
const GEMINI_API_BASE: &str = "https://generativelanguage.googleapis.com/v1beta/models";

/// Cloud Code Assist API endpoint (OAuth mode)
const CLOUD_CODE_ASSIST_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

/// API mode for Gemini provider
#[derive(Debug, Clone)]
pub enum GeminiApiMode {
    /// Standard API (generativelanguage.googleapis.com) - API key in URL
    Standard,
    /// Cloud Code Assist API (cloudcode-pa.googleapis.com) - OAuth Bearer token
    /// Includes project_id for request wrapping
    CloudCodeAssist { project_id: String },
}

/// Authentication method for Gemini
#[derive(Debug, Clone)]
pub enum GeminiAuth {
    /// API key (passed in URL query parameter)
    ApiKey(String),
    /// OAuth Bearer token (passed in Authorization header)
    Bearer(String),
}

pub struct GeminiProvider {
    client: reqwest::Client,
    auth: GeminiAuth,
    model: String,
    max_tokens: usize,
    api_mode: GeminiApiMode,
}

impl GeminiProvider {
    /// Create a new GeminiProvider using Standard API mode
    ///
    /// Credentials are resolved in this priority:
    /// 1. GEMINI_API_KEY or GOOGLE_API_KEY environment variable
    /// 2. OAuth token from `~/.local/share/tark/tokens/gemini.json`
    /// 3. Application Default Credentials (GOOGLE_APPLICATION_CREDENTIALS)
    ///
    /// If no credentials are found, returns an error with setup instructions.
    pub fn new() -> Result<Self> {
        // Try API key first (highest priority)
        if let Ok(api_key) = env::var("GEMINI_API_KEY").or_else(|_| env::var("GOOGLE_API_KEY")) {
            return Ok(Self {
                client: reqwest::Client::new(),
                auth: GeminiAuth::ApiKey(api_key),
                model: "gemini-2.0-flash-exp".to_string(),
                max_tokens: 8192,
                api_mode: GeminiApiMode::Standard,
            });
        }

        // Try OAuth token
        let token_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("tark")
            .join("tokens")
            .join("gemini.json");

        if token_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&token_path) {
                if let Ok(stored) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(access_token) = stored["access_token"].as_str() {
                        // Check if expired
                        let expires_at = stored["expires_at"].as_u64().unwrap_or(0);
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        if expires_at > now + 300 {
                            // Token valid for at least 5 more minutes
                            return Ok(Self {
                                client: reqwest::Client::new(),
                                auth: GeminiAuth::ApiKey(access_token.to_string()),
                                model: "gemini-2.0-flash-exp".to_string(),
                                max_tokens: 8192,
                                api_mode: GeminiApiMode::Standard,
                            });
                        }
                    }
                }
            }
        }

        // No valid credentials found
        anyhow::bail!(
            "Gemini authentication required.\n\n\
            Option 1 - OAuth (recommended):\n  \
            tark auth gemini\n\n\
            Option 2 - API Key:\n  \
            export GEMINI_API_KEY=\"your-api-key\"\n  \
            Get key: https://aistudio.google.com/apikey"
        )
    }

    /// Create a new GeminiProvider using Cloud Code Assist API mode
    ///
    /// This mode uses OAuth Bearer token authentication and the Cloud Code Assist
    /// API endpoint (cloudcode-pa.googleapis.com). This is the same API used by
    /// the Gemini CLI and provides access to preview models.
    ///
    /// # Arguments
    /// * `access_token` - Valid OAuth access token
    /// * `project_id` - Google Cloud project ID (discovered via loadCodeAssist)
    pub fn with_cloud_code_assist(access_token: String, project_id: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            auth: GeminiAuth::Bearer(access_token),
            model: "gemini-2.0-flash".to_string(),
            max_tokens: 8192,
            api_mode: GeminiApiMode::CloudCodeAssist { project_id },
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Get the current API mode
    pub fn api_mode(&self) -> &GeminiApiMode {
        &self.api_mode
    }

    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();
        let mut allowed_tool_call_ids: HashSet<String> = HashSet::new();

        for msg in messages {
            match msg.role {
                Role::System => {
                    if let Some(text) = msg.content.as_text() {
                        system_instruction = Some(text.to_string());
                    }
                }
                Role::User => {
                    if let Some(text) = msg.content.as_text() {
                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts: vec![GeminiPart::Text {
                                text: text.to_string(),
                            }],
                        });
                    }
                }
                Role::Assistant => {
                    // Handle assistant messages.
                    //
                    // IMPORTANT: We must include prior tool calls (functionCall) in the
                    // conversation history; otherwise Gemini/Cloud Code Assist may ignore
                    // subsequent functionResponse messages and produce empty outputs.
                    let mut parts = Vec::new();

                    match &msg.content {
                        MessageContent::Text(text) => {
                            if !text.is_empty() {
                                parts.push(GeminiPart::Text { text: text.clone() });
                            }
                        }
                        MessageContent::Parts(content_parts) => {
                            for part in content_parts {
                                match part {
                                    ContentPart::Text { text } => {
                                        if !text.is_empty() {
                                            parts.push(GeminiPart::Text { text: text.clone() });
                                        }
                                    }
                                    ContentPart::ToolUse {
                                        id,
                                        name,
                                        input,
                                        thought_signature,
                                    } => {
                                        // Include tool calls as functionCall parts so that
                                        // subsequent tool results (functionResponse) are
                                        // properly attributed by the model.
                                        if thought_signature.is_some() {
                                            allowed_tool_call_ids.insert(id.clone());
                                            parts.push(GeminiPart::FunctionCall {
                                                function_call: GeminiFunctionCall {
                                                    name: name.clone(),
                                                    args: input.clone(),
                                                    thought_signature: thought_signature.clone(),
                                                },
                                            });
                                        } else {
                                            tracing::warn!(
                                                "Skipping Gemini tool call '{}' without thought_signature",
                                                name
                                            );
                                        }
                                    }
                                    ContentPart::ToolResult { .. } => {
                                        // Tool results are handled in Role::Tool
                                    }
                                }
                            }
                        }
                    }

                    if !parts.is_empty() {
                        contents.push(GeminiContent {
                            role: "model".to_string(),
                            parts,
                        });
                    }
                }
                Role::Tool => {
                    // Gemini expects function responses as user role with functionResponse part
                    if let (Some(tool_call_id), Some(text)) =
                        (&msg.tool_call_id, msg.content.as_text())
                    {
                        if !allowed_tool_call_ids.contains(tool_call_id) {
                            tracing::warn!(
                                "Skipping Gemini tool response for '{}' without matching thought_signature",
                                tool_call_id
                            );
                            continue;
                        }

                        // Extract function name from tool_call_id (format: "gemini_{name}")
                        let function_name = tool_call_id
                            .strip_prefix("gemini_")
                            .unwrap_or(tool_call_id)
                            .to_string();

                        // Parse the result as JSON if possible, otherwise wrap as string
                        let response_value = serde_json::from_str(text)
                            .unwrap_or_else(|_| serde_json::json!({ "result": text }));

                        contents.push(GeminiContent {
                            role: "user".to_string(),
                            parts: vec![GeminiPart::FunctionResponse {
                                function_response: GeminiFunctionResponse {
                                    name: function_name,
                                    response: response_value,
                                },
                            }],
                        });
                    }
                }
            }
        }

        (system_instruction, contents)
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<GeminiFunctionDeclaration> {
        tools
            .iter()
            .map(|t| GeminiFunctionDeclaration {
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.parameters.clone(),
            })
            .collect()
    }

    async fn send_request(&self, request: GeminiRequest) -> Result<GeminiResponse> {
        match &self.api_mode {
            GeminiApiMode::Standard => {
                // Standard API: key in URL, direct request body
                let api_key = match &self.auth {
                    GeminiAuth::ApiKey(key) => key,
                    GeminiAuth::Bearer(token) => token, // Use as API key for backwards compat
                };

                let url = format!(
                    "{}/{}:generateContent?key={}",
                    GEMINI_API_BASE, self.model, api_key
                );

                let response = self
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .json(&request)
                    .send()
                    .await
                    .context("Failed to send request to Gemini API")?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("Gemini API error ({}): {}", status, error_text);
                }

                response
                    .json::<GeminiResponse>()
                    .await
                    .context("Failed to parse Gemini API response")
            }
            GeminiApiMode::CloudCodeAssist { project_id } => {
                // Cloud Code Assist API: Bearer token in header, wrapped request body
                let token = match &self.auth {
                    GeminiAuth::Bearer(token) => token,
                    GeminiAuth::ApiKey(key) => key,
                };

                let url = format!("{}:generateContent", CLOUD_CODE_ASSIST_BASE);

                // Wrap request in Cloud Code Assist format
                let wrapped_request = serde_json::json!({
                    "project": project_id,
                    "model": self.model,
                    "request": request
                });

                let response = self
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("User-Agent", "google-api-nodejs-client/9.15.1")
                    .header("X-Goog-Api-Client", "gl-node/22.17.0")
                    .header(
                        "Client-Metadata",
                        "ideType=IDE_UNSPECIFIED,platform=PLATFORM_UNSPECIFIED,pluginType=GEMINI",
                    )
                    .json(&wrapped_request)
                    .send()
                    .await
                    .context("Failed to send request to Cloud Code Assist API")?;

                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    anyhow::bail!("Cloud Code Assist API error ({}): {}", status, error_text);
                }

                // Cloud Code Assist wraps response in { "response": { ... } }
                let wrapper: CloudCodeAssistResponse = response
                    .json()
                    .await
                    .context("Failed to parse Cloud Code Assist API response")?;

                Ok(wrapper.response)
            }
        }
    }

    /// Streaming for Standard API (SSE format)
    async fn chat_streaming_standard(
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

        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let api_key = match &self.auth {
            GeminiAuth::ApiKey(key) => key,
            GeminiAuth::Bearer(token) => token,
        };

        let url = format!(
            "{}/{}:streamGenerateContent?key={}&alt=sse",
            GEMINI_API_BASE, self.model, api_key
        );

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to Gemini API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "Gemini API error ({}): {}",
                status, error_text
            )));
            return Err(LlmError::from_http_status(status, error_text).into());
        }

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::new();

        let mut last_activity_at = std::time::Instant::now();
        loop {
            if let Some(check) = interrupt_check {
                if check() {
                    return Ok(builder.build());
                }
            }

            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                return Err(LlmError::Network(format!(
                    "Stream timeout - no response from Gemini for {} seconds",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                ))
                .into());
            }

            let chunk_result = match timeout(INTERRUPT_POLL_INTERVAL, stream.next()).await {
                Ok(Some(res)) => res,
                Ok(None) => break,
                Err(_) => continue,
            };

            last_activity_at = std::time::Instant::now();
            let chunk = chunk_result.context("Error reading stream chunk")?;

            // Push bytes into SSE decoder
            for payload_json in decoder.push(&chunk) {
                if let Ok(chunk) = serde_json::from_str::<GeminiStreamChunk>(&payload_json) {
                    if let Some(candidate) = chunk.candidates.first() {
                        if let Some(content) = &candidate.content {
                            for part in &content.parts {
                                match part {
                                    GeminiPart::Text { text } => {
                                        if !text.is_empty() {
                                            let event = StreamEvent::TextDelta(text.clone());
                                            builder.process(&event);
                                            callback(event);
                                        }
                                    }
                                    GeminiPart::FunctionCall { function_call } => {
                                        let id = format!("gemini_{}", function_call.name);
                                        let event = StreamEvent::ToolCallStart {
                                            id: id.clone(),
                                            name: function_call.name.clone(),
                                            thought_signature: function_call
                                                .thought_signature
                                                .clone(),
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let args_str = serde_json::to_string(&function_call.args)
                                            .unwrap_or_default();
                                        let event = StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_delta: args_str,
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let event = StreamEvent::ToolCallComplete { id };
                                        callback(event);
                                    }
                                    GeminiPart::FunctionResponse { .. } => {
                                        // Function responses are what we sent, not model output
                                    }
                                }
                            }
                        }
                    }

                    if let Some(usage) = chunk.usage_metadata {
                        builder.usage = Some(TokenUsage {
                            input_tokens: usage.prompt_token_count,
                            output_tokens: usage.candidates_token_count,
                            total_tokens: usage.total_token_count,
                        });
                    }
                }
            }
        }

        // Flush any remaining buffered events (handles final event without trailing \n)
        for payload_json in decoder.finish() {
            if let Ok(chunk) = serde_json::from_str::<GeminiStreamChunk>(&payload_json) {
                if let Some(candidate) = chunk.candidates.first() {
                    if let Some(content) = &candidate.content {
                        for part in &content.parts {
                            match part {
                                GeminiPart::Text { text } => {
                                    if !text.is_empty() {
                                        let event = StreamEvent::TextDelta(text.clone());
                                        builder.process(&event);
                                        callback(event);
                                    }
                                }
                                GeminiPart::FunctionCall { function_call } => {
                                    let id = format!("gemini_{}", function_call.name);
                                    callback(StreamEvent::ToolCallStart {
                                        id: id.clone(),
                                        name: function_call.name.clone(),
                                        thought_signature: function_call.thought_signature.clone(),
                                    });

                                    let args_str = serde_json::to_string(&function_call.args)
                                        .unwrap_or_default();
                                    callback(StreamEvent::ToolCallDelta {
                                        id: id.clone(),
                                        arguments_delta: args_str,
                                    });

                                    callback(StreamEvent::ToolCallComplete { id });
                                }
                                GeminiPart::FunctionResponse { .. } => {}
                            }
                        }
                    }
                }

                if let Some(usage) = chunk.usage_metadata {
                    builder.usage = Some(TokenUsage {
                        input_tokens: usage.prompt_token_count,
                        output_tokens: usage.candidates_token_count,
                        total_tokens: usage.total_token_count,
                    });
                }
            }
        }

        callback(StreamEvent::Done);
        Ok(builder.build())
    }

    /// Streaming for Cloud Code Assist API (JSON array format)
    async fn chat_streaming_cloud_code_assist(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        project_id: &str,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let token = match &self.auth {
            GeminiAuth::Bearer(token) => token,
            GeminiAuth::ApiKey(key) => key,
        };

        // Use SSE format for proper streaming (same as standard Gemini API)
        let url = format!("{}:streamGenerateContent?alt=sse", CLOUD_CODE_ASSIST_BASE);

        let wrapped_request = serde_json::json!({
            "project": project_id,
            "model": self.model,
            "request": request
        });

        // Retry logic with exponential backoff for 429 rate limit errors
        let retry_delays = [1, 5, 10, 20, 30, 60]; // seconds
        let mut last_error = String::new();

        let response = 'retry: {
            for (attempt, &delay) in std::iter::once(&0).chain(retry_delays.iter()).enumerate() {
                // Wait before retry (skip on first attempt)
                if delay > 0 {
                    callback(StreamEvent::TextDelta(format!(
                        "\n⏳ Rate limited. Retrying in {} seconds (attempt {}/{})...\n",
                        delay,
                        attempt,
                        retry_delays.len() + 1
                    )));
                    tokio::time::sleep(Duration::from_secs(delay)).await;

                    // Check for interrupt during wait
                    if let Some(check) = interrupt_check {
                        if check() {
                            return Err(anyhow::anyhow!("Interrupted during rate limit retry"));
                        }
                    }
                }

                let result = self
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .header("Authorization", format!("Bearer {}", token))
                    .header("User-Agent", "google-api-nodejs-client/9.15.1")
                    .header("X-Goog-Api-Client", "gl-node/22.17.0")
                    .header(
                        "Client-Metadata",
                        "ideType=IDE_UNSPECIFIED,platform=PLATFORM_UNSPECIFIED,pluginType=GEMINI",
                    )
                    .json(&wrapped_request)
                    .send()
                    .await;

                match result {
                    Ok(resp) if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
                        let error_text = resp.text().await.unwrap_or_default();
                        // Parse retry time from error if available
                        let wait_msg = if let Some(secs) = Self::parse_retry_after(&error_text) {
                            format!("Rate limit exceeded. Quota resets in {}s.", secs)
                        } else {
                            "Rate limit exceeded.".to_string()
                        };
                        last_error = format!("429 Too Many Requests: {}", wait_msg);
                        tracing::warn!("Rate limited (attempt {}): {}", attempt + 1, last_error);
                        continue;
                    }
                    Ok(resp) if !resp.status().is_success() => {
                        let status = resp.status();
                        let error_text = resp.text().await.unwrap_or_default();
                        callback(StreamEvent::Error(format!(
                            "Cloud Code Assist API error ({}): {}",
                            status, error_text
                        )));
                        return Err(LlmError::from_http_status(status, error_text).into());
                    }
                    Ok(resp) => break 'retry resp,
                    Err(e) => {
                        return Err(LlmError::from_network_error(e).into());
                    }
                }
            }

            // All retries exhausted
            let err_msg = format!(
                "Rate limit exceeded after {} retries. {}",
                retry_delays.len(),
                last_error
            );
            callback(StreamEvent::Error(err_msg.clone()));
            return Err(LlmError::RateLimited(err_msg).into());
        };

        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut decoder = SseDecoder::new();

        let mut last_activity_at = std::time::Instant::now();
        loop {
            if let Some(check) = interrupt_check {
                if check() {
                    return Ok(builder.build());
                }
            }

            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                return Err(LlmError::Network(format!(
                    "Stream timeout - no response from Cloud Code Assist for {} seconds",
                    STREAM_CHUNK_TIMEOUT.as_secs()
                ))
                .into());
            }

            let chunk_result = match timeout(INTERRUPT_POLL_INTERVAL, stream.next()).await {
                Ok(Some(res)) => res,
                Ok(None) => break,
                Err(_) => continue,
            };

            last_activity_at = std::time::Instant::now();
            let chunk = chunk_result.context("Error reading stream chunk")?;

            // Push bytes into SSE decoder
            for payload_json in decoder.push(&chunk) {
                // Cloud Code Assist wraps response in {"response": {...}}
                match serde_json::from_str::<CloudCodeAssistStreamChunk>(&payload_json) {
                    Ok(chunk) => {
                        if let Some(candidates) = &chunk.response.candidates {
                            if let Some(candidate) = candidates.first() {
                                // Content may be missing in metadata-only chunks
                                if let Some(content) = &candidate.content {
                                    for part in &content.parts {
                                        match part {
                                            GeminiPart::Text { text } => {
                                                if !text.is_empty() {
                                                    let event =
                                                        StreamEvent::TextDelta(text.clone());
                                                    builder.process(&event);
                                                    callback(event);
                                                }
                                            }
                                            GeminiPart::FunctionCall { function_call } => {
                                                let id = format!("gemini_{}", function_call.name);
                                                let event = StreamEvent::ToolCallStart {
                                                    id: id.clone(),
                                                    name: function_call.name.clone(),
                                                    thought_signature: function_call
                                                        .thought_signature
                                                        .clone(),
                                                };
                                                builder.process(&event);
                                                callback(event);

                                                let args_str =
                                                    serde_json::to_string(&function_call.args)
                                                        .unwrap_or_default();
                                                let event = StreamEvent::ToolCallDelta {
                                                    id: id.clone(),
                                                    arguments_delta: args_str,
                                                };
                                                builder.process(&event);
                                                callback(event);

                                                let event = StreamEvent::ToolCallComplete { id };
                                                callback(event);
                                            }
                                            GeminiPart::FunctionResponse { .. } => {
                                                // Function responses are what we sent, not model output
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        // Extract usage metadata if present
                        if let Some(usage) = chunk.response.usage_metadata {
                            builder.usage = Some(TokenUsage {
                                input_tokens: usage.prompt_token_count,
                                output_tokens: usage.candidates_token_count,
                                total_tokens: usage.total_token_count,
                            });
                        }
                    }
                    Err(e) => {
                        // Log parse failures to help debug response format issues
                        tracing::debug!(
                            "Failed to parse Cloud Code Assist chunk: {} - payload: {}",
                            e,
                            &payload_json[..payload_json.len().min(200)]
                        );
                    }
                }
            }
        }

        // Flush any remaining buffered events (handles final event without trailing \n)
        for payload_json in decoder.finish() {
            match serde_json::from_str::<CloudCodeAssistStreamChunk>(&payload_json) {
                Ok(chunk) => {
                    if let Some(candidates) = &chunk.response.candidates {
                        if let Some(candidate) = candidates.first() {
                            // Content may be missing in metadata-only chunks
                            if let Some(content) = &candidate.content {
                                for part in &content.parts {
                                    match part {
                                        GeminiPart::Text { text } => {
                                            if !text.is_empty() {
                                                let event = StreamEvent::TextDelta(text.clone());
                                                builder.process(&event);
                                                callback(event);
                                            }
                                        }
                                        GeminiPart::FunctionCall { function_call } => {
                                            let id = format!("gemini_{}", function_call.name);
                                            callback(StreamEvent::ToolCallStart {
                                                id: id.clone(),
                                                name: function_call.name.clone(),
                                                thought_signature: function_call
                                                    .thought_signature
                                                    .clone(),
                                            });

                                            let args_str =
                                                serde_json::to_string(&function_call.args)
                                                    .unwrap_or_default();
                                            callback(StreamEvent::ToolCallDelta {
                                                id: id.clone(),
                                                arguments_delta: args_str,
                                            });

                                            callback(StreamEvent::ToolCallComplete { id });
                                        }
                                        GeminiPart::FunctionResponse { .. } => {}
                                    }
                                }
                            }
                        }
                    }

                    if let Some(usage) = chunk.response.usage_metadata {
                        builder.usage = Some(TokenUsage {
                            input_tokens: usage.prompt_token_count,
                            output_tokens: usage.candidates_token_count,
                            total_tokens: usage.total_token_count,
                        });
                    }
                }
                Err(e) => {
                    tracing::debug!(
                        "Failed to parse Cloud Code Assist final chunk: {} - payload: {}",
                        e,
                        &payload_json[..payload_json.len().min(200)]
                    );
                }
            }
        }

        callback(StreamEvent::Done);
        Ok(builder.build())
    }

    /// Parse retry-after time from error message (e.g., "reset after 55s")
    fn parse_retry_after(error_text: &str) -> Option<u64> {
        // Look for patterns like "reset after 55s" or "retry in 30 seconds"
        let patterns = [
            r"reset after (\d+)s",
            r"retry in (\d+)",
            r"wait (\d+) second",
            r"(\d+)s\.",
        ];

        for pattern in patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                if let Some(caps) = re.captures(error_text) {
                    if let Some(m) = caps.get(1) {
                        if let Ok(secs) = m.as_str().parse::<u64>() {
                            return Some(secs);
                        }
                    }
                }
            }
        }
        None
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supports_native_thinking(&self) -> bool {
        self.model.contains("thinking")
    }

    async fn supports_native_thinking_async(&self) -> bool {
        // Try models.dev first for future-proof detection
        let db = super::models_db();
        if db.supports_reasoning("google", &self.model).await {
            return true;
        }
        // Fallback to hardcoded check
        self.model.contains("thinking")
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let response = self.send_request(request).await?;

        let usage = response.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        });

        if let Some(candidate) = response.candidates.first() {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    match part {
                        GeminiPart::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        GeminiPart::FunctionCall { function_call } => {
                            tool_calls.push(ToolCall {
                                id: format!("gemini_{}", function_call.name), // Gemini doesn't provide IDs
                                name: function_call.name.clone(),
                                arguments: function_call.args.clone(),
                                thought_signature: function_call.thought_signature.clone(),
                            });
                        }
                        GeminiPart::FunctionResponse { .. } => {
                            // Function responses are what we sent, not model output
                        }
                    }
                }
            }

            if tool_calls.is_empty() {
                Ok(LlmResponse::Text {
                    text: text_parts.join("\n"),
                    usage,
                })
            } else if text_parts.is_empty() {
                Ok(LlmResponse::ToolCalls {
                    calls: tool_calls,
                    usage,
                })
            } else {
                Ok(LlmResponse::Mixed {
                    text: Some(text_parts.join("\n")),
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
        match &self.api_mode {
            GeminiApiMode::Standard => {
                self.chat_streaming_standard(messages, tools, callback, interrupt_check)
                    .await
            }
            GeminiApiMode::CloudCodeAssist { project_id } => {
                self.chat_streaming_cloud_code_assist(
                    messages,
                    tools,
                    callback,
                    interrupt_check,
                    project_id,
                )
                .await
            }
        }
    }

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        _settings: &super::ThinkSettings,
    ) -> Result<LlmResponse> {
        // Gemini doesn't have separate thinking mode yet, just use regular streaming
        self.chat_streaming(messages, tools, callback, interrupt_check)
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

        let user_content = format!("{prefix}<CURSOR>{suffix}");

        let request = GeminiRequest {
            contents: vec![GeminiContent {
                role: "user".to_string(),
                parts: vec![GeminiPart::Text { text: user_content }],
            }],
            system_instruction: Some(GeminiSystemInstruction {
                parts: vec![GeminiPart::Text { text: system }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(256),
                temperature: Some(0.2),
            }),
            tools: None,
        };

        let response = self.send_request(request).await?;

        let text = if let Some(candidate) = response.candidates.first() {
            candidate
                .content
                .as_ref()
                .map(|content| {
                    content
                        .parts
                        .iter()
                        .filter_map(|p| {
                            if let GeminiPart::Text { text } = p {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("")
                        .trim()
                        .to_string()
                })
                .unwrap_or_default()
        } else {
            String::new()
        };

        let usage = response.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
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

// Gemini API types

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTools>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPart>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum GeminiPart {
    Text {
        text: String,
    },
    FunctionCall {
        #[serde(rename = "functionCall")]
        function_call: GeminiFunctionCall,
    },
    FunctionResponse {
        #[serde(rename = "functionResponse")]
        function_response: GeminiFunctionResponse,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCall {
    name: String,
    args: serde_json::Value,
    #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
}

#[derive(Debug, Serialize)]
struct GeminiGenerationConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    max_output_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
}

#[derive(Debug, Serialize)]
struct GeminiTools {
    #[serde(rename = "functionDeclarations")]
    function_declarations: Vec<GeminiFunctionDeclaration>,
}

#[derive(Debug, Serialize)]
struct GeminiFunctionDeclaration {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct GeminiResponse {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

/// Cloud Code Assist API response wrapper
#[derive(Debug, Deserialize)]
struct CloudCodeAssistResponse {
    response: GeminiResponse,
}

/// Cloud Code Assist streaming chunk wrapper
#[derive(Debug, Deserialize)]
struct CloudCodeAssistStreamChunk {
    response: GeminiStreamChunkInner,
}

/// Inner streaming chunk for Cloud Code Assist
#[derive(Debug, Deserialize)]
struct GeminiStreamChunkInner {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
    // Content may be missing in some streaming chunks (e.g., metadata-only chunks)
    content: Option<GeminiContent>,
}

#[derive(Debug, Deserialize)]
struct GeminiUsageMetadata {
    #[serde(rename = "promptTokenCount")]
    prompt_token_count: u32,
    #[serde(rename = "candidatesTokenCount")]
    candidates_token_count: u32,
    #[serde(rename = "totalTokenCount")]
    total_token_count: u32,
}

#[derive(Debug, Deserialize)]
struct GeminiStreamChunk {
    candidates: Vec<GeminiCandidate>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemini_response() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{"text": "Hello, world!"}]
                }
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        assert!(response.usage_metadata.is_some());
        let usage = response.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, 10);
        assert_eq!(usage.candidates_token_count, 5);
        assert_eq!(usage.total_token_count, 15);
    }

    #[test]
    fn test_parse_gemini_function_call() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "functionCall": {
                            "name": "test_function",
                            "args": {"key": "value"},
                            "thoughtSignature": "sig_123"
                        }
                    }]
                }
            }]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        let content = response.candidates[0].content.as_ref().unwrap();
        let part = &content.parts[0];
        match part {
            GeminiPart::FunctionCall { function_call } => {
                assert_eq!(function_call.name, "test_function");
                assert_eq!(function_call.thought_signature.as_deref(), Some("sig_123"));
            }
            _ => panic!("Expected FunctionCall"),
        }
    }

    #[test]
    fn test_convert_messages_skips_missing_thought_signature() {
        let provider = GeminiProvider {
            client: reqwest::Client::new(),
            auth: GeminiAuth::ApiKey("test".to_string()),
            model: "gemini-2.0-flash-exp".to_string(),
            max_tokens: 16,
            api_mode: GeminiApiMode::Standard,
        };

        let messages = vec![
            Message::user("Hello"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "gemini_list".to_string(),
                    name: "list_directory".to_string(),
                    input: serde_json::json!({"path": "."}),
                    thought_signature: None,
                }]),
                tool_call_id: None,
            },
            Message::tool_result("gemini_list", "file.txt"),
        ];

        let (_, contents) = provider.convert_messages(&messages);
        assert_eq!(contents.len(), 1);
        assert_eq!(contents[0].role, "user");
    }
}
