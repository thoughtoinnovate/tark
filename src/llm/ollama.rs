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
use std::sync::atomic::{AtomicU64, Ordering};

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Generate a unique tool call ID for Ollama tool calls
fn generate_tool_call_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ollama_call_{}", id)
}

/// Splits streamed text into visible text and reasoning segments using
/// prompt-level tags (`<think>...</think>` or `<thinking>...</thinking>`).
#[derive(Debug, Default)]
struct PromptThinkingTagParser {
    in_thinking: bool,
    pending: String,
}

impl PromptThinkingTagParser {
    fn ingest(&mut self, chunk: &str) -> (String, String) {
        self.pending.push_str(chunk);
        let mut text_out = String::new();
        let mut thinking_out = String::new();

        loop {
            if self.in_thinking {
                if let Some((idx, close_len)) = find_closing_tag(&self.pending) {
                    thinking_out.push_str(&self.pending[..idx]);
                    self.pending = self.pending[idx + close_len..].to_string();
                    self.in_thinking = false;
                    continue;
                }

                thinking_out.push_str(&self.pending);
                self.pending.clear();
                break;
            }

            if let Some((idx, open_len)) = find_opening_tag(&self.pending) {
                text_out.push_str(&self.pending[..idx]);
                self.pending = self.pending[idx + open_len..].to_string();
                self.in_thinking = true;
                continue;
            }

            let (emit, rest) = split_emitable_text(&self.pending);
            text_out.push_str(emit);
            self.pending = rest.to_string();
            break;
        }

        (text_out, thinking_out)
    }

    fn flush(&mut self) -> (String, String) {
        let pending = std::mem::take(&mut self.pending);
        if self.in_thinking {
            self.in_thinking = false;
            (String::new(), pending)
        } else {
            (pending, String::new())
        }
    }
}

fn find_opening_tag(s: &str) -> Option<(usize, usize)> {
    let think = s.find("<think>");
    let thinking = s.find("<thinking>");
    match (think, thinking) {
        (Some(a), Some(b)) => Some(if a <= b { (a, 7) } else { (b, 10) }),
        (Some(a), None) => Some((a, 7)),
        (None, Some(b)) => Some((b, 10)),
        (None, None) => None,
    }
}

fn find_closing_tag(s: &str) -> Option<(usize, usize)> {
    let think = s.find("</think>");
    let thinking = s.find("</thinking>");
    match (think, thinking) {
        (Some(a), Some(b)) => Some(if a <= b { (a, 8) } else { (b, 11) }),
        (Some(a), None) => Some((a, 8)),
        (None, Some(b)) => Some((b, 11)),
        (None, None) => None,
    }
}

fn split_emitable_text(s: &str) -> (&str, &str) {
    // Hold a possible partial opening tag suffix so the next chunk can complete it.
    const OPEN_TAGS: [&str; 2] = ["<think>", "<thinking>"];
    let mut hold_len = 0usize;
    for tag in OPEN_TAGS {
        let max_prefix = std::cmp::min(tag.len() - 1, s.len());
        for len in (1..=max_prefix).rev() {
            if s.ends_with(&tag[..len]) {
                hold_len = hold_len.max(len);
                break;
            }
        }
    }
    if hold_len == 0 {
        (s, "")
    } else {
        (&s[..s.len() - hold_len], &s[s.len() - hold_len..])
    }
}

pub struct OllamaProvider {
    client: reqwest::Client,
    base_url: String,
    model: String,
}

/// Model info returned from Ollama's /api/tags endpoint
#[derive(Debug, Clone, Deserialize)]
pub struct OllamaModelInfo {
    pub name: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub modified_at: String,
}

/// List available models from local Ollama instance (standalone function)
///
/// This function queries the local Ollama server for installed models.
/// Returns an empty list if Ollama is not running or unreachable.
pub async fn list_local_ollama_models() -> Result<Vec<OllamaModelInfo>> {
    let base_url = env::var("OLLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_OLLAMA_URL.to_string());
    let client = reqwest::Client::new();
    let url = format!("{}/api/tags", base_url);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .context("Failed to connect to Ollama - is it running? Try: ollama serve")?;

    if !response.status().is_success() {
        let status = response.status();
        anyhow::bail!("Ollama API error ({})", status);
    }

    #[derive(Deserialize)]
    struct TagsResponse {
        models: Vec<OllamaModelInfo>,
    }

    let resp: TagsResponse = response
        .json()
        .await
        .context("Failed to parse Ollama response")?;

    Ok(resp.models)
}

impl OllamaProvider {
    /// Create a new Ollama provider
    ///
    /// Auto-detects Ollama on localhost:11434 or uses OLLAMA_BASE_URL/OLLAMA_MODEL env vars.
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

    /// Check if Ollama is running and reachable
    pub async fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// List available models from Ollama's /api/tags endpoint
    pub async fn list_models(&self) -> Result<Vec<OllamaModelInfo>> {
        let url = format!("{}/api/tags", self.base_url);
        let response = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .context("Failed to connect to Ollama - is it running? Try: ollama serve")?;

        if !response.status().is_success() {
            let status = response.status();
            anyhow::bail!("Ollama API error ({})", status);
        }

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<OllamaModelInfo>,
        }

        let resp: TagsResponse = response
            .json()
            .await
            .context("Failed to parse Ollama response")?;

        Ok(resp.models)
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OllamaMessage> {
        messages
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    Role::System => "system",
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool", // Native tool role for tool responses
                };

                OllamaMessage {
                    role: role.to_string(),
                    content: msg.content.as_text().unwrap_or("").to_string(),
                    tool_calls: None,
                }
            })
            .collect()
    }

    /// Convert ToolDefinition to native Ollama tool format
    fn convert_tools(tools: &[ToolDefinition]) -> Vec<OllamaTool> {
        tools
            .iter()
            .map(|t| OllamaTool {
                type_field: "function".to_string(),
                function: OllamaFunction {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.parameters.clone(),
                },
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

    fn supports_native_thinking(&self) -> bool {
        // Ollama models generally don't have native thinking/reasoning APIs
        // Some models like deepseek-r1 may output thinking in <think> tags
        // but this is model-specific, not API-level support
        false
    }

    async fn supports_native_thinking_async(&self) -> bool {
        // Check models.dev for ollama model capabilities
        let db = super::models_db();
        if db.supports_reasoning("ollama", &self.model).await {
            return true;
        }
        // Ollama models generally use prompt-based thinking, not API-level
        false
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let ollama_messages = self.convert_messages(messages);

        // Convert tools to native Ollama format
        let ollama_tools = tools.filter(|t| !t.is_empty()).map(Self::convert_tools);

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: false,
            tools: ollama_tools,
        };

        let response = self.send_request(request).await?;

        // Check if model returned tool calls (native tool calling)
        if let Some(tool_calls) = response.message.tool_calls {
            if !tool_calls.is_empty() {
                let calls = tool_calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        id: generate_tool_call_id(),
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                        thought_signature: None,
                    })
                    .collect();

                return Ok(LlmResponse::ToolCalls {
                    calls,
                    usage: None, // Ollama doesn't provide usage info in standard response
                });
            }
        }

        // Fallback: try to parse tool call from text (for models that output JSON)
        let content = &response.message.content;
        if let Some(tool_call) = self.parse_tool_call(content) {
            return Ok(LlmResponse::ToolCalls {
                calls: vec![tool_call],
                usage: None,
            });
        }

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
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        use futures::StreamExt;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let ollama_messages = self.convert_messages(messages);

        // Convert tools to native Ollama format
        let ollama_tools = tools.filter(|t| !t.is_empty()).map(Self::convert_tools);
        let has_tools = ollama_tools.is_some();

        let request = OllamaRequest {
            model: self.model.clone(),
            messages: ollama_messages,
            stream: true, // Enable streaming
            tools: ollama_tools,
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
        let mut thinking_parser = PromptThinkingTagParser::default();

        let mut last_activity_at = std::time::Instant::now();
        loop {
            // Check for user interrupt frequently so Ctrl+C/Esc+Esc are responsive
            if let Some(check) = interrupt_check {
                if check() {
                    return Ok(builder.build());
                }
            }

            // Enforce per-chunk timeout: if we haven't received any bytes recently, abort.
            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                anyhow::bail!(
                    "Stream timeout - no response from Ollama for {} seconds",
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

                        // Check for tool calls in the final message
                        if let Some(ref message) = chunk.message {
                            if let Some(ref tool_calls) = message.tool_calls {
                                if !tool_calls.is_empty() {
                                    let calls = tool_calls
                                        .iter()
                                        .map(|tc| ToolCall {
                                            id: generate_tool_call_id(),
                                            name: tc.function.name.clone(),
                                            arguments: tc.function.arguments.clone(),
                                            thought_signature: None,
                                        })
                                        .collect();

                                    return Ok(LlmResponse::ToolCalls { calls, usage: None });
                                }
                            }
                        }
                        continue;
                    }

                    if let Some(message) = chunk.message {
                        // Check for tool calls in streaming chunks
                        if let Some(ref tool_calls) = message.tool_calls {
                            if !tool_calls.is_empty() {
                                let calls = tool_calls
                                    .iter()
                                    .map(|tc| ToolCall {
                                        id: generate_tool_call_id(),
                                        name: tc.function.name.clone(),
                                        arguments: tc.function.arguments.clone(),
                                        thought_signature: None,
                                    })
                                    .collect();

                                return Ok(LlmResponse::ToolCalls { calls, usage: None });
                            }
                        }

                        if !message.content.is_empty() {
                            let (text_delta, thinking_delta) =
                                thinking_parser.ingest(&message.content);

                            if !thinking_delta.is_empty() {
                                let event = StreamEvent::ThinkingDelta(thinking_delta);
                                builder.process(&event);
                                callback(event);
                            }

                            if !text_delta.is_empty() {
                                let event = StreamEvent::TextDelta(text_delta);
                                builder.process(&event);
                                callback(event);
                            }
                        }
                    }
                }
            }
        }

        // Flush any partial tag buffer at end-of-stream.
        let (text_tail, thinking_tail) = thinking_parser.flush();
        if !thinking_tail.is_empty() {
            let event = StreamEvent::ThinkingDelta(thinking_tail);
            builder.process(&event);
            callback(event);
        }
        if !text_tail.is_empty() {
            let event = StreamEvent::TextDelta(text_tail);
            builder.process(&event);
            callback(event);
        }

        // Fallback: try to parse tool call from accumulated text
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

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        _settings: &super::ThinkSettings,
    ) -> Result<LlmResponse> {
        // Ollama does not expose a native thinking parameter in /api/chat.
        // Keep native streaming behavior and parse prompt-level think tags.
        self.chat_streaming(messages, tools, callback, interrupt_check)
            .await
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
                tool_calls: None,
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!(
                    "Explain this code:\n\n```\n{}\n```\n\nContext:\n{}",
                    code, context
                ),
                tool_calls: None,
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            tools: None,
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
                tool_calls: None,
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!("Suggest refactorings for:\n\n```\n{}\n```\n\nContext:\n{}", code, context),
                tool_calls: None,
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            tools: None,
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
                tool_calls: None,
            },
            OllamaMessage {
                role: "user".to_string(),
                content: format!("Review this {} code:\n\n```{}\n{}\n```", language, language, code),
                tool_calls: None,
            },
        ];

        let request = OllamaRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            tools: None,
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
                thought_signature: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OllamaTool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OllamaToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OllamaResponse {
    message: OllamaMessage,
}

// Native tool calling types
#[derive(Debug, Clone, Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    type_field: String,
    function: OllamaFunction,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaToolCall {
    function: OllamaToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OllamaToolCallFunction {
    name: String,
    #[serde(default)]
    arguments: serde_json::Value,
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

#[cfg(test)]
mod tests {
    use super::PromptThinkingTagParser;

    #[test]
    fn parser_splits_prompt_thinking_tags() {
        let mut parser = PromptThinkingTagParser::default();
        let (t1, k1) = parser.ingest("hello <think>reason");
        assert_eq!(t1, "hello ");
        assert_eq!(k1, "reason");

        let (t2, k2) = parser.ingest("ing</think> world");
        assert_eq!(t2, " world");
        assert_eq!(k2, "ing");

        let (t3, k3) = parser.flush();
        assert_eq!(t3, "");
        assert_eq!(k3, "");
    }

    #[test]
    fn parser_handles_split_open_tag() {
        let mut parser = PromptThinkingTagParser::default();
        let (t1, k1) = parser.ingest("hello <thi");
        assert_eq!(t1, "hello ");
        assert_eq!(k1, "");

        let (t2, k2) = parser.ingest("nk>r</think>");
        assert_eq!(t2, "");
        assert_eq!(k2, "r");
    }
}
