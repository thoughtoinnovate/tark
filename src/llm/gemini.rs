//! Google Gemini LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official Google endpoints.
//! The GEMINI_API_KEY is never sent to any third-party services.
//!
//! Supports two API modes:
//! - Standard API - Uses OpenAI-compatible endpoint for reliability
//! - Cloud Code Assist API (cloudcode-pa.googleapis.com) - OAuth Bearer auth, native format

#![allow(dead_code)]

use super::{
    openai_compat::{AuthMethod, OpenAiCompatConfig, OpenAiCompatProvider},
    streaming::SseDecoder,
    CodeIssue, CompletionResult, ContentPart, LlmError, LlmProvider, LlmResponse, Message,
    MessageContent, RefactoringSuggestion, Role, StreamCallback, StreamEvent,
    StreamingResponseBuilder, ThinkSettings, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::env;

/// OpenAI-compatible endpoint for Gemini
const GEMINI_OPENAI_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

/// Cloud Code Assist API endpoint (OAuth mode) - uses native Gemini format
const CLOUD_CODE_ASSIST_BASE: &str = "https://cloudcode-pa.googleapis.com/v1internal";

/// API mode for Gemini provider
#[derive(Debug, Clone)]
pub enum GeminiApiMode {
    /// Standard API using OpenAI-compatible endpoint
    Standard,
    /// Cloud Code Assist API (cloudcode-pa.googleapis.com) - OAuth Bearer token
    /// Includes project_id for request wrapping
    CloudCodeAssist { project_id: String },
}

/// Authentication method for Gemini
#[derive(Debug, Clone)]
pub enum GeminiAuth {
    /// API key
    ApiKey(String),
    /// OAuth Bearer token (passed in Authorization header)
    Bearer(String),
}

/// Gemini provider that supports both Standard (OpenAI-compat) and CloudCodeAssist modes
pub struct GeminiProvider {
    /// Inner provider - either OpenAI-compat or CloudCodeAssist native
    inner: GeminiInner,
    model: String,
}

enum GeminiInner {
    /// Standard mode uses OpenAI-compatible endpoint
    OpenAiCompat(OpenAiCompatProvider),
    /// CloudCodeAssist uses native Gemini format
    CloudCodeAssist(CloudCodeAssistProvider),
}

impl GeminiProvider {
    /// Create a new GeminiProvider using Standard API mode (OpenAI-compatible)
    ///
    /// Credentials are resolved in this priority:
    /// 1. GEMINI_API_KEY or GOOGLE_API_KEY environment variable
    /// 2. OAuth token from `~/.local/share/tark/tokens/gemini.json`
    ///
    /// If no credentials are found, returns an error with setup instructions.
    pub fn new() -> Result<Self> {
        // Try API key first (highest priority)
        if let Ok(api_key) = env::var("GEMINI_API_KEY").or_else(|_| env::var("GOOGLE_API_KEY")) {
            let provider = OpenAiCompatProvider::new(
                OpenAiCompatConfig::new(
                    "gemini",
                    GEMINI_OPENAI_URL,
                    AuthMethod::BearerToken(api_key),
                )
                .with_model("gemini-2.5-flash")
                .with_max_tokens(8192),
            );
            return Ok(Self {
                inner: GeminiInner::OpenAiCompat(provider),
                model: "gemini-2.5-flash".to_string(),
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
                            let provider = OpenAiCompatProvider::new(
                                OpenAiCompatConfig::new(
                                    "gemini",
                                    GEMINI_OPENAI_URL,
                                    AuthMethod::BearerToken(access_token.to_string()),
                                )
                                .with_model("gemini-2.5-flash")
                                .with_max_tokens(8192),
                            );
                            return Ok(Self {
                                inner: GeminiInner::OpenAiCompat(provider),
                                model: "gemini-2.5-flash".to_string(),
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
        let provider = CloudCodeAssistProvider::new(access_token.clone(), project_id);
        Self {
            inner: GeminiInner::CloudCodeAssist(provider),
            model: "gemini-2.5-flash".to_string(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        match &mut self.inner {
            GeminiInner::OpenAiCompat(p) => {
                // Update model on the existing provider
                *p = p.clone().with_model(model);
            }
            GeminiInner::CloudCodeAssist(p) => {
                p.model = model.to_string();
            }
        }
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        match &mut self.inner {
            GeminiInner::OpenAiCompat(p) => {
                *p = p.clone().with_max_tokens(max_tokens);
            }
            GeminiInner::CloudCodeAssist(p) => {
                p.max_tokens = max_tokens;
            }
        }
        self
    }

    /// Get the current API mode
    pub fn api_mode(&self) -> GeminiApiMode {
        match &self.inner {
            GeminiInner::OpenAiCompat(_) => GeminiApiMode::Standard,
            GeminiInner::CloudCodeAssist(p) => GeminiApiMode::CloudCodeAssist {
                project_id: p.project_id.clone(),
            },
        }
    }
}

impl Clone for OpenAiCompatProvider {
    fn clone(&self) -> Self {
        // This is a workaround since OpenAiCompatProvider doesn't derive Clone
        // We need to reconstruct it - this is only used in with_model
        Self::new(
            OpenAiCompatConfig::new(
                self.provider_name(),
                GEMINI_OPENAI_URL,
                AuthMethod::BearerToken(String::new()),
            )
            .with_model(self.model())
            .with_max_tokens(8192),
        )
    }
}

#[async_trait]
impl LlmProvider for GeminiProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supports_native_thinking(&self) -> bool {
        self.model.contains("thinking")
            || self.model.contains("gemini-2.5")
            || self.model.contains("gemini-3")
    }

    async fn supports_native_thinking_async(&self) -> bool {
        // Try models.dev first for future-proof detection
        let db = super::models_db();
        if db.supports_reasoning("google", &self.model).await {
            return true;
        }
        // Fallback to hardcoded check
        self.supports_native_thinking()
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => p.chat(messages, tools).await,
            GeminiInner::CloudCodeAssist(p) => p.chat(messages, tools).await,
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
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => {
                p.chat_streaming(messages, tools, callback, interrupt_check)
                    .await
            }
            GeminiInner::CloudCodeAssist(p) => {
                p.chat_streaming(messages, tools, callback, interrupt_check)
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
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => {
                p.chat_streaming_with_thinking(messages, tools, callback, interrupt_check, settings)
                    .await
            }
            GeminiInner::CloudCodeAssist(p) => {
                p.chat_streaming_with_thinking(messages, tools, callback, interrupt_check, settings)
                    .await
            }
        }
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => p.complete_fim(prefix, suffix, language).await,
            GeminiInner::CloudCodeAssist(p) => p.complete_fim(prefix, suffix, language).await,
        }
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => p.explain_code(code, context).await,
            GeminiInner::CloudCodeAssist(p) => p.explain_code(code, context).await,
        }
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => p.suggest_refactorings(code, context).await,
            GeminiInner::CloudCodeAssist(p) => p.suggest_refactorings(code, context).await,
        }
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        match &self.inner {
            GeminiInner::OpenAiCompat(p) => p.review_code(code, language).await,
            GeminiInner::CloudCodeAssist(p) => p.review_code(code, language).await,
        }
    }
}

// ============================================================================
// CloudCodeAssist Provider (Native Gemini Format)
// ============================================================================

/// Provider for Cloud Code Assist API using native Gemini format
///
/// This is kept separate because CloudCodeAssist uses a different endpoint
/// and response format that doesn't follow OpenAI conventions.
struct CloudCodeAssistProvider {
    client: reqwest::Client,
    access_token: String,
    project_id: String,
    model: String,
    max_tokens: usize,
}

impl CloudCodeAssistProvider {
    fn new(access_token: String, project_id: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            access_token,
            project_id,
            model: "gemini-2.5-flash".to_string(),
            max_tokens: 8192,
        }
    }

    /// Convert messages to native Gemini format
    ///
    /// IMPORTANT: This version correctly handles thoughtSignature as a SIBLING
    /// of functionCall, not nested inside it.
    fn convert_messages(&self, messages: &[Message]) -> (Option<String>, Vec<GeminiContent>) {
        let mut system_instruction = None;
        let mut contents = Vec::new();

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
                            parts: vec![GeminiPartRaw {
                                text: Some(text.to_string()),
                                thought: None,
                                thought_signature: None,
                                function_call: None,
                                function_response: None,
                            }],
                        });
                    }
                }
                Role::Assistant => {
                    let mut parts = Vec::new();

                    match &msg.content {
                        MessageContent::Text(text) => {
                            if !text.is_empty() {
                                parts.push(GeminiPartRaw {
                                    text: Some(text.clone()),
                                    thought: None,
                                    thought_signature: None,
                                    function_call: None,
                                    function_response: None,
                                });
                            }
                        }
                        MessageContent::Parts(content_parts) => {
                            for part in content_parts {
                                match part {
                                    ContentPart::Text { text } => {
                                        if !text.is_empty() {
                                            parts.push(GeminiPartRaw {
                                                text: Some(text.clone()),
                                                thought: None,
                                                thought_signature: None,
                                                function_call: None,
                                                function_response: None,
                                            });
                                        }
                                    }
                                    ContentPart::ToolUse {
                                        name,
                                        input,
                                        thought_signature,
                                        ..
                                    } => {
                                        // FIXED: thoughtSignature is a SIBLING of functionCall
                                        parts.push(GeminiPartRaw {
                                            text: None,
                                            thought: None,
                                            thought_signature: thought_signature.clone(),
                                            function_call: Some(GeminiFunctionCallInner {
                                                name: name.clone(),
                                                args: input.clone(),
                                            }),
                                            function_response: None,
                                        });
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
                            parts: vec![GeminiPartRaw {
                                text: None,
                                thought: None,
                                thought_signature: None,
                                function_call: None,
                                function_response: Some(GeminiFunctionResponse {
                                    name: function_name,
                                    response: response_value,
                                }),
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

    /// Parse retry-after time from error message
    fn parse_retry_after(error_text: &str) -> Option<u64> {
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
impl LlmProvider for CloudCodeAssistProvider {
    fn name(&self) -> &str {
        "gemini"
    }

    fn supports_native_thinking(&self) -> bool {
        self.model.contains("thinking")
            || self.model.contains("gemini-2.5")
            || self.model.contains("gemini-3")
    }

    async fn supports_native_thinking_async(&self) -> bool {
        let db = super::models_db();
        if db.supports_reasoning("google", &self.model).await {
            return true;
        }
        self.supports_native_thinking()
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
                parts: vec![GeminiPartRaw {
                    text: Some(text),
                    thought: None,
                    thought_signature: None,
                    function_call: None,
                    function_response: None,
                }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
            thinking_config: None,
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        let url = format!("{}:generateContent", CLOUD_CODE_ASSIST_BASE);

        let wrapped_request = serde_json::json!({
            "project": self.project_id,
            "model": self.model,
            "request": request
        });

        let response = self
            .client
            .post(&url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.access_token))
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

        let wrapper: CloudCodeAssistResponse = response
            .json()
            .await
            .context("Failed to parse Cloud Code Assist API response")?;

        let api_response = wrapper.response;

        let usage = api_response.usage_metadata.map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
        });

        if let Some(candidate) = api_response.candidates.first() {
            let mut text_parts = Vec::new();
            let mut tool_calls = Vec::new();

            if let Some(content) = &candidate.content {
                for part in &content.parts {
                    if let Some(text) = &part.text {
                        // Skip thought content for now (marked with thought: true)
                        if part.thought != Some(true) {
                            text_parts.push(text.clone());
                        }
                    }
                    if let Some(fc) = &part.function_call {
                        tool_calls.push(ToolCall {
                            id: format!("gemini_{}", fc.name),
                            name: fc.name.clone(),
                            arguments: fc.args.clone(),
                            // FIXED: Get thought_signature from sibling field
                            thought_signature: part.thought_signature.clone(),
                        });
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
        self.chat_streaming_with_thinking(
            messages,
            tools,
            callback,
            interrupt_check,
            &ThinkSettings::off(),
        )
        .await
    }

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let (system_instruction, contents) = self.convert_messages(messages);

        let mut request = GeminiRequest {
            contents,
            system_instruction: system_instruction.map(|text| GeminiSystemInstruction {
                parts: vec![GeminiPartRaw {
                    text: Some(text),
                    thought: None,
                    thought_signature: None,
                    function_call: None,
                    function_response: None,
                }],
            }),
            generation_config: Some(GeminiGenerationConfig {
                max_output_tokens: Some(self.max_tokens),
                temperature: Some(1.0),
            }),
            tools: None,
            thinking_config: None,
        };

        // Note: CloudCodeAssist endpoint does NOT support thinkingConfig parameter
        // The API returns "Unknown name" error if we include it
        // Thinking/reasoning is handled internally by compatible models (gemini-2.5+)
        // but we cannot explicitly enable it via this endpoint
        if settings.enabled && settings.budget_tokens > 0 {
            tracing::debug!(
                "CloudCodeAssist: thinking config requested but not supported by endpoint. \
                 Model {} may still use internal reasoning.",
                self.model
            );
        }

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(vec![GeminiTools {
                    function_declarations: self.convert_tools(tools),
                }]);
            }
        }

        // Use SSE format for proper streaming
        let url = format!("{}:streamGenerateContent?alt=sse", CLOUD_CODE_ASSIST_BASE);

        let wrapped_request = serde_json::json!({
            "project": self.project_id,
            "model": self.model,
            "request": request
        });

        // Retry logic with exponential backoff for 429 rate limit errors
        let retry_delays = [1, 5, 10, 20, 30, 60];
        let mut last_error = String::new();

        let response = 'retry: {
            for (attempt, &delay) in std::iter::once(&0).chain(retry_delays.iter()).enumerate() {
                if delay > 0 {
                    callback(StreamEvent::TextDelta(format!(
                        "\n⏳ Rate limited. Retrying in {} seconds (attempt {}/{})...\n",
                        delay,
                        attempt,
                        retry_delays.len() + 1
                    )));
                    tokio::time::sleep(Duration::from_secs(delay)).await;

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
                    .header("Authorization", format!("Bearer {}", self.access_token))
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

            for payload_json in decoder.push(&chunk) {
                // Debug logging
                crate::llm::append_llm_raw_line(&payload_json);

                // Cloud Code Assist wraps response in {"response": {...}}
                if let Ok(chunk) = serde_json::from_str::<CloudCodeAssistStreamChunk>(&payload_json)
                {
                    if let Some(candidates) = &chunk.response.candidates {
                        if let Some(candidate) = candidates.first() {
                            if let Some(content) = &candidate.content {
                                for part in &content.parts {
                                    // Handle text content
                                    if let Some(text) = &part.text {
                                        if !text.is_empty() {
                                            // Check if this is thinking content
                                            if part.thought == Some(true) {
                                                // Emit as thinking delta
                                                let event =
                                                    StreamEvent::ThinkingDelta(text.clone());
                                                builder.process(&event);
                                                callback(event);
                                            } else {
                                                let event = StreamEvent::TextDelta(text.clone());
                                                builder.process(&event);
                                                callback(event);
                                            }
                                        }
                                    }

                                    // Handle function calls
                                    if let Some(fc) = &part.function_call {
                                        let id = format!("gemini_{}", fc.name);
                                        // FIXED: Get thought_signature from sibling field
                                        let event = StreamEvent::ToolCallStart {
                                            id: id.clone(),
                                            name: fc.name.clone(),
                                            thought_signature: part.thought_signature.clone(),
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let args_str =
                                            serde_json::to_string(&fc.args).unwrap_or_default();
                                        let event = StreamEvent::ToolCallDelta {
                                            id: id.clone(),
                                            arguments_delta: args_str,
                                        };
                                        builder.process(&event);
                                        callback(event);

                                        let event = StreamEvent::ToolCallComplete { id };
                                        callback(event);
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
            }
        }

        // Flush remaining events
        for payload_json in decoder.finish() {
            if let Ok(chunk) = serde_json::from_str::<CloudCodeAssistStreamChunk>(&payload_json) {
                if let Some(candidates) = &chunk.response.candidates {
                    if let Some(candidate) = candidates.first() {
                        if let Some(content) = &candidate.content {
                            for part in &content.parts {
                                if let Some(text) = &part.text {
                                    if !text.is_empty() {
                                        if part.thought == Some(true) {
                                            let event = StreamEvent::ThinkingDelta(text.clone());
                                            builder.process(&event);
                                            callback(event);
                                        } else {
                                            let event = StreamEvent::TextDelta(text.clone());
                                            builder.process(&event);
                                            callback(event);
                                        }
                                    }
                                }

                                if let Some(fc) = &part.function_call {
                                    let id = format!("gemini_{}", fc.name);
                                    callback(StreamEvent::ToolCallStart {
                                        id: id.clone(),
                                        name: fc.name.clone(),
                                        thought_signature: part.thought_signature.clone(),
                                    });

                                    let args_str =
                                        serde_json::to_string(&fc.args).unwrap_or_default();
                                    callback(StreamEvent::ToolCallDelta {
                                        id: id.clone(),
                                        arguments_delta: args_str,
                                    });

                                    callback(StreamEvent::ToolCallComplete { id });
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
        }

        callback(StreamEvent::Done);
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

        let messages = vec![
            Message::system(system),
            Message::user(format!("{prefix}<CURSOR>{suffix}")),
        ];

        let response = self.chat(&messages, None).await?;

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

// ============================================================================
// Native Gemini API Types (for CloudCodeAssist)
// ============================================================================

#[derive(Debug, Serialize)]
struct GeminiRequest {
    contents: Vec<GeminiContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_instruction: Option<GeminiSystemInstruction>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generation_config: Option<GeminiGenerationConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<GeminiTools>>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "thinkingConfig")]
    thinking_config: Option<GeminiThinkingConfig>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeminiThinkingConfig {
    thinking_budget: i32,
    include_thoughts: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiSystemInstruction {
    parts: Vec<GeminiPartRaw>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiContent {
    role: String,
    parts: Vec<GeminiPartRaw>,
}

/// Raw Gemini part structure with thoughtSignature as SIBLING of functionCall
///
/// This matches the actual API response format where thoughtSignature
/// is at the same level as functionCall, not nested inside it.
#[derive(Debug, Serialize, Deserialize)]
struct GeminiPartRaw {
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thought: Option<bool>,
    #[serde(rename = "thoughtSignature", skip_serializing_if = "Option::is_none")]
    thought_signature: Option<String>,
    #[serde(rename = "functionCall", skip_serializing_if = "Option::is_none")]
    function_call: Option<GeminiFunctionCallInner>,
    #[serde(rename = "functionResponse", skip_serializing_if = "Option::is_none")]
    function_response: Option<GeminiFunctionResponse>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionCallInner {
    name: String,
    args: serde_json::Value,
}

#[derive(Debug, Serialize, Deserialize)]
struct GeminiFunctionResponse {
    name: String,
    response: serde_json::Value,
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

#[derive(Debug, Deserialize)]
struct CloudCodeAssistResponse {
    response: GeminiResponse,
}

#[derive(Debug, Deserialize)]
struct CloudCodeAssistStreamChunk {
    response: GeminiStreamChunkInner,
}

#[derive(Debug, Deserialize)]
struct GeminiStreamChunkInner {
    candidates: Option<Vec<GeminiCandidate>>,
    #[serde(rename = "usageMetadata")]
    usage_metadata: Option<GeminiUsageMetadata>,
}

#[derive(Debug, Deserialize)]
struct GeminiCandidate {
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gemini_response_with_thought_signature_sibling() {
        // This test verifies the FIXED parsing where thoughtSignature is a sibling
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [{
                        "thoughtSignature": "sig_123",
                        "functionCall": {
                            "name": "test_function",
                            "args": {"key": "value"}
                        }
                    }]
                }
            }]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.candidates.len(), 1);
        let content = response.candidates[0].content.as_ref().unwrap();
        let part = &content.parts[0];

        // Verify thoughtSignature is captured at the sibling level
        assert_eq!(part.thought_signature.as_deref(), Some("sig_123"));
        assert!(part.function_call.is_some());
        let fc = part.function_call.as_ref().unwrap();
        assert_eq!(fc.name, "test_function");
    }

    #[test]
    fn test_parse_thought_content() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "role": "model",
                    "parts": [
                        {"text": "Let me think about this...", "thought": true},
                        {"text": "Here is my answer."}
                    ]
                }
            }]
        }"#;

        let response: GeminiResponse = serde_json::from_str(json).unwrap();
        let content = response.candidates[0].content.as_ref().unwrap();

        assert_eq!(content.parts.len(), 2);
        assert_eq!(content.parts[0].thought, Some(true));
        assert_eq!(content.parts[1].thought, None);
    }

    #[test]
    fn test_parse_text_response() {
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
}
