//! Ollama LLM provider implementation (local models)

#![allow(dead_code)]

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion, Role,
    StreamCallback, StreamEvent, StreamingResponseBuilder, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";

/// Default wait for the first byte (cold model load can take tens of seconds).
/// Override with `OLLAMA_INITIAL_TIMEOUT` (seconds).
const DEFAULT_INITIAL_TIMEOUT_SECS: u64 = 120;
/// Default wait between subsequent stream chunks. Override with
/// `OLLAMA_CHUNK_TIMEOUT` (seconds).
const DEFAULT_CHUNK_TIMEOUT_SECS: u64 = 60;
/// Warmup is skipped when the model was warmed within this TTL (under
/// Ollama's default 5-minute `keep_alive`).
const WARMUP_TTL_SECS: u64 = 240;
/// Max attempts for transient failures (connect/timeout) with backoff.
const MAX_ATTEMPTS: u32 = 3;

fn initial_timeout() -> Duration {
    Duration::from_secs(
        env::var("OLLAMA_INITIAL_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &u64| *v > 0)
            .unwrap_or(DEFAULT_INITIAL_TIMEOUT_SECS),
    )
}

fn chunk_timeout() -> Duration {
    Duration::from_secs(
        env::var("OLLAMA_CHUNK_TIMEOUT")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v: &u64| *v > 0)
            .unwrap_or(DEFAULT_CHUNK_TIMEOUT_SECS),
    )
}

/// Marker for timeout failures so the retry policy can classify them without
/// parsing message strings.
#[derive(Debug)]
struct OllamaTimeout(&'static str);

impl std::fmt::Display for OllamaTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Ollama operation timed out ({})", self.0)
    }
}

impl std::error::Error for OllamaTimeout {}

/// True for failures worth retrying: timeouts and connection errors.
/// HTTP 4xx (unknown model, bad request) and parse errors are not retried.
fn is_transient(e: &anyhow::Error) -> bool {
    if e.downcast_ref::<OllamaTimeout>().is_some() {
        return true;
    }
    if let Some(req_err) = e.downcast_ref::<reqwest::Error>() {
        return req_err.is_timeout() || req_err.is_connect();
    }
    false
}

/// Run `f` up to `max_attempts` times with 1s/2s/4s backoff on transient
/// failures. Non-transient errors return immediately.
async fn with_retry<T, F, Fut>(op: &str, max_attempts: u32, f: F) -> Result<T>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T>>,
{
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        match f().await {
            Ok(value) => return Ok(value),
            Err(e) if attempt < max_attempts && is_transient(&e) => {
                let backoff_secs = 1u64 << (attempt - 1).min(3);
                tracing::warn!(
                    "Ollama {op} attempt {attempt}/{max_attempts} failed ({e}); retrying in {backoff_secs}s"
                );
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Process-global warmup timestamps per `base_url#model`.
static WARMED: std::sync::OnceLock<std::sync::Mutex<HashMap<String, std::time::Instant>>> =
    std::sync::OnceLock::new();

fn warmed_at(key: &str) -> Option<std::time::Instant> {
    WARMED
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
        .ok()?
        .get(key)
        .copied()
}

fn mark_warmed(key: &str) {
    if let Ok(mut map) = WARMED
        .get_or_init(|| std::sync::Mutex::new(HashMap::new()))
        .lock()
    {
        map.insert(key.to_string(), std::time::Instant::now());
    }
}

/// True when `installed` (e.g. `llama3.2:latest` from `/api/tags`) satisfies
/// `wanted` (e.g. `llama3.2`), ignoring a trailing `:latest` on either side.
fn model_matches(installed: &str, wanted: &str) -> bool {
    fn strip(s: &str) -> &str {
        s.strip_suffix(":latest").unwrap_or(s)
    }
    strip(installed) == strip(wanted)
}

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

    /// Ensure the server is reachable, the model is installed, and the model
    /// is loaded (cold-start warmup). Fails fast with actionable errors
    /// instead of hanging the chat flow.
    async fn ensure_model_ready(&self) -> Result<()> {
        let key = format!("{}#{}", self.base_url, self.model);
        if warmed_at(&key).is_some_and(|at| at.elapsed().as_secs() < WARMUP_TTL_SECS) {
            return Ok(());
        }

        // 1. Server reachable?
        let tags_url = format!("{}/api/tags", self.base_url);
        let probe =
            tokio::time::timeout(Duration::from_secs(10), self.client.get(&tags_url).send()).await;
        let tags_resp = match probe {
            Err(_) => anyhow::bail!(
                "Ollama is not reachable at {} within 10s — is it running? Try: ollama serve",
                self.base_url
            ),
            Ok(Err(e)) => anyhow::bail!(
                "Cannot reach Ollama at {} ({e}) — is it running? Try: ollama serve",
                self.base_url
            ),
            Ok(Ok(resp)) => resp,
        };

        if !tags_resp.status().is_success() {
            anyhow::bail!(
                "Ollama API error ({}) at {} — is it running? Try: ollama serve",
                tags_resp.status(),
                self.base_url
            );
        }
        #[derive(Deserialize)]
        struct TagsResponse {
            #[serde(default)]
            models: Vec<OllamaModelInfo>,
        }
        let tags: TagsResponse = tags_resp
            .json()
            .await
            .context("Failed to parse Ollama /api/tags response")?;

        // 2. Model installed?
        if !tags
            .models
            .iter()
            .any(|m| model_matches(&m.name, &self.model))
        {
            let installed: Vec<&str> = tags.models.iter().map(|m| m.name.as_str()).collect();
            let hint = if installed.is_empty() {
                "no models are installed yet".to_string()
            } else {
                format!("installed: {}", installed.join(", "))
            };
            anyhow::bail!(
                "Ollama model '{}' is not installed ({hint}) — pull it with: ollama pull {}",
                self.model,
                self.model
            );
        }

        // 3. Warm the model (cold load can take tens of seconds on first use).
        let warmup = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: "ok".to_string(),
            stream: false,
            options: Some(OllamaOptions {
                num_predict: Some(1),
            }),
            keep_alive: Some("5m".to_string()),
        };
        let url = format!("{}/api/generate", self.base_url);
        let limit = initial_timeout();
        tokio::time::timeout(limit, self.client.post(&url).json(&warmup).send())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Ollama model '{}' did not load within {}s (cold start on first use can be slow) — \
                     increase via OLLAMA_INITIAL_TIMEOUT, or pre-load with: ollama run {} ''",
                    self.model,
                    limit.as_secs(),
                    self.model
                )
            })?
            .map_err(|e| {
                anyhow::anyhow!(
                    "Cannot reach Ollama at {} during warmup ({e}) — is it running? Try: ollama serve",
                    self.base_url
                )
            })?;
        // Warmup response body is intentionally ignored; loading is the goal.

        mark_warmed(&key);
        Ok(())
    }

    async fn send_request(&self, request: OllamaRequest) -> Result<OllamaResponse> {
        let url = format!("{}/api/chat", self.base_url);
        let limit = initial_timeout();

        with_retry("chat", MAX_ATTEMPTS, || async {
            let response = tokio::time::timeout(limit, self.client.post(&url).json(&request).send())
                .await
                .map_err(|_| {
                    anyhow::anyhow!(OllamaTimeout("chat response")).context(format!(
                        "Ollama model '{}' produced no response within {}s — the model may still be loading; \
                         increase via OLLAMA_INITIAL_TIMEOUT",
                        self.model,
                        limit.as_secs()
                    ))
                })?
                .context("Failed to send request to Ollama")?;

            if !response.status().is_success() {
                let status = response.status();
                let error_text = response.text().await.unwrap_or_default();
                if status.as_u16() == 404 {
                    anyhow::bail!(
                        "Ollama model '{}' not found ({}) — pull it with: ollama pull {} ({})",
                        self.model,
                        status,
                        self.model,
                        error_text
                    );
                }
                anyhow::bail!("Ollama API error ({}): {}", status, error_text);
            }

            response
                .json::<OllamaResponse>()
                .await
                .context("Failed to parse Ollama response")
        })
        .await
    }

    async fn generate(&self, prompt: &str) -> Result<String> {
        let url = format!("{}/api/generate", self.base_url);

        let request = OllamaGenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options: None,
            keep_alive: None,
        };

        let limit = initial_timeout();
        let response = tokio::time::timeout(limit, self.client.post(&url).json(&request).send())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "Ollama model '{}' produced no response within {}s — the model may still be loading; \
                     increase via OLLAMA_INITIAL_TIMEOUT",
                    self.model,
                    limit.as_secs()
                )
            })?
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
        // Fail fast on unreachable server / missing model / cold load
        // instead of hanging the chat flow.
        self.ensure_model_ready().await?;

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
        use tokio::time::timeout;

        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        // Fail fast on unreachable server / missing model / cold load
        // instead of spinning the TUI loading state forever.
        self.ensure_model_ready().await?;

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
        let setup_limit = initial_timeout();

        // Retry transient setup failures (connect resets under load); the
        // first byte may take a full cold-load window to arrive.
        let response = with_retry("chat stream setup", MAX_ATTEMPTS, || async {
            tokio::time::timeout(setup_limit, self.client.post(&url).json(&request).send())
                .await
                .map_err(|_| {
                    anyhow::anyhow!(OllamaTimeout("stream setup")).context(format!(
                        "Ollama model '{}' sent no data within {}s — the model may still be loading; \
                         increase via OLLAMA_INITIAL_TIMEOUT",
                        self.model,
                        setup_limit.as_secs()
                    ))
                })?
                .context("Failed to send streaming request to Ollama")
        })
        .await?;

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
        let mut received_any = false;
        loop {
            // Check for user interrupt frequently so Ctrl+C/Esc+Esc are responsive
            if let Some(check) = interrupt_check {
                if check() {
                    return Ok(builder.build());
                }
            }

            // First byte gets the cold-load window; subsequent chunks get the
            // steady-state window. This is the "keeps loading forever" fix:
            // a silent server now fails loudly with a remediation hint.
            let limit = if received_any {
                chunk_timeout()
            } else {
                initial_timeout()
            };
            if last_activity_at.elapsed() >= limit {
                if received_any {
                    anyhow::bail!(
                        "Stream timeout - no response from Ollama for {} seconds",
                        limit.as_secs()
                    );
                }
                anyhow::bail!(
                    "Ollama model '{}' sent no data within {}s — the model may still be loading; \
                     increase via OLLAMA_INITIAL_TIMEOUT, or pre-load with: ollama run {} ''",
                    self.model,
                    limit.as_secs(),
                    self.model
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
            received_any = true;
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

#[derive(Debug, Clone, Serialize)]
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

#[derive(Debug, Clone, Serialize)]
struct OllamaGenerateRequest {
    model: String,
    prompt: String,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_alive: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
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
    use super::{initial_timeout, is_transient, mark_warmed, model_matches, warmed_at, with_retry};
    use super::{OllamaTimeout, DEFAULT_CHUNK_TIMEOUT_SECS, DEFAULT_INITIAL_TIMEOUT_SECS};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

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

    #[test]
    fn timeout_env_parsing_falls_back_to_defaults() {
        // Single sequential test: env mutation is process-global.
        std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "5");
        std::env::set_var("OLLAMA_CHUNK_TIMEOUT", "7");
        assert_eq!(initial_timeout(), std::time::Duration::from_secs(5));
        assert_eq!(super::chunk_timeout(), std::time::Duration::from_secs(7));

        for bad in ["bogus", "0", "-3", ""] {
            std::env::set_var("OLLAMA_INITIAL_TIMEOUT", bad);
            std::env::set_var("OLLAMA_CHUNK_TIMEOUT", bad);
            assert_eq!(
                initial_timeout(),
                std::time::Duration::from_secs(DEFAULT_INITIAL_TIMEOUT_SECS)
            );
            assert_eq!(
                super::chunk_timeout(),
                std::time::Duration::from_secs(DEFAULT_CHUNK_TIMEOUT_SECS)
            );
        }
        std::env::remove_var("OLLAMA_INITIAL_TIMEOUT");
        std::env::remove_var("OLLAMA_CHUNK_TIMEOUT");
        assert_eq!(
            initial_timeout(),
            std::time::Duration::from_secs(DEFAULT_INITIAL_TIMEOUT_SECS)
        );
    }

    #[test]
    fn transient_classification() {
        assert!(is_transient(&anyhow::anyhow!(OllamaTimeout("x"))));
        assert!(!is_transient(&anyhow::anyhow!(
            "Ollama model 'x' not found"
        )));
        assert!(!is_transient(&anyhow::anyhow!(
            "Failed to parse Ollama response"
        )));
    }

    #[test]
    fn model_name_matching_ignores_latest_tag() {
        assert!(model_matches("llama3.2:latest", "llama3.2"));
        assert!(model_matches("llama3.2", "llama3.2:latest"));
        assert!(model_matches("llama3.2:latest", "llama3.2:latest"));
        assert!(model_matches("deepseek-r1:8b", "deepseek-r1:8b"));
        assert!(!model_matches("llama3.2:latest", "llama3.1"));
        assert!(!model_matches("llama3.2:8b", "llama3.2:70b"));
    }

    #[test]
    fn warmup_cache_marks_and_expires() {
        let key = "test://warmup-cache-probe#model";
        mark_warmed(key);
        assert!(warmed_at(key).is_some());
        // Backdate past the TTL: treated as cold.
        if let Ok(mut map) = super::WARMED
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
            .lock()
        {
            map.insert(
                key.to_string(),
                std::time::Instant::now()
                    - std::time::Duration::from_secs(super::WARMUP_TTL_SECS + 1),
            );
        }
        let at = warmed_at(key).expect("entry");
        assert!(at.elapsed().as_secs() >= super::WARMUP_TTL_SECS);
    }

    #[tokio::test]
    async fn retry_succeeds_after_transient_failures() {
        let attempts = Arc::new(AtomicU32::new(0));
        let probe = attempts.clone();
        let result = with_retry("probe", 3, || {
            let probe = probe.clone();
            async move {
                let n = probe.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err::<u32, _>(anyhow::anyhow!(OllamaTimeout("probe")))
                } else {
                    Ok(n)
                }
            }
        })
        .await
        .expect("retry");
        assert_eq!(result, 2);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_does_not_retry_permanent_errors() {
        let attempts = Arc::new(AtomicU32::new(0));
        let probe = attempts.clone();
        let err = with_retry("probe", 3, || {
            let probe = probe.clone();
            async move {
                probe.fetch_add(1, Ordering::SeqCst);
                Err::<u32, _>(anyhow::anyhow!("Ollama model 'x' not found"))
            }
        })
        .await
        .expect_err("permanent");
        assert!(err.to_string().contains("not found"));
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    #[ignore]
    async fn live_warmup_against_local_ollama() {
        // Runs only with OLLAMA_TEST_URL + OLLAMA_TEST_MODEL set, e.g.:
        // OLLAMA_TEST_URL=http://localhost:11434 OLLAMA_TEST_MODEL=llama3.2 \
        //   cargo test --all-features --lib ollama -- --ignored
        let (Some(base_url), Some(model)) = (
            std::env::var("OLLAMA_TEST_URL").ok(),
            std::env::var("OLLAMA_TEST_MODEL").ok(),
        ) else {
            return;
        };
        let provider = super::OllamaProvider::new()
            .expect("provider")
            .with_base_url(&base_url)
            .with_model(&model);
        provider.ensure_model_ready().await.expect("warmup");
    }
}
