//! OpenAI LLM provider implementation
//!
//! SECURITY: API keys are ONLY sent to official OpenAI endpoints.
//! The OPENAI_API_KEY is never sent to any third-party services.

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

/// Official OpenAI API endpoint - API key is ONLY sent here
const OPENAI_API_URL: &str = "https://api.openai.com/v1/chat/completions";
/// Official OpenAI models endpoint
const OPENAI_MODELS_URL: &str = "https://api.openai.com/v1/models";

/// Model info returned from OpenAI API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub owned_by: String,
}

/// Response from OpenAI models endpoint
#[derive(Debug, Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

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

    /// Fetch available models from OpenAI API
    /// Returns only chat-capable models (gpt-* and o1-*)
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let response = self
            .client
            .get(OPENAI_MODELS_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to fetch models from OpenAI")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("OpenAI API error {}: {}", status, body);
        }

        let models_response: ModelsResponse = response
            .json()
            .await
            .context("Failed to parse models response")?;

        // Filter to only chat-capable models
        let chat_models: Vec<ModelInfo> = models_response
            .data
            .into_iter()
            .filter(|m| {
                let id = m.id.as_str();
                // Include GPT models and O1 models, exclude embeddings, tts, whisper, dall-e
                (id.starts_with("gpt-") || id.starts_with("o1") || id.starts_with("o3"))
                    && !id.contains("instruct")
                    && !id.contains("realtime")
                    && !id.contains("audio")
            })
            .collect();

        Ok(chat_models)
    }

    /// Sanitize message history to fix orphaned tool calls and tool responses.
    /// OpenAI requires that:
    /// 1. Every assistant message with tool_calls must be followed by tool messages
    /// 2. Every tool message must be preceded by an assistant message with matching tool_call_id
    fn sanitize_messages(&self, messages: &[Message]) -> Vec<Message> {
        use std::collections::{HashMap, HashSet};

        // First pass: find all assistant messages with tool_calls and map their positions
        // Also find which tool_call_ids have corresponding tool responses
        let mut tool_call_positions: HashMap<String, usize> = HashMap::new();
        let mut tool_response_ids: HashSet<String> = HashSet::new();

        for (idx, msg) in messages.iter().enumerate() {
            if msg.role == Role::Assistant {
                if let MessageContent::Parts(parts) = &msg.content {
                    for part in parts {
                        if let ContentPart::ToolUse { id, .. } = part {
                            tool_call_positions.insert(id.clone(), idx);
                        }
                    }
                }
            } else if msg.role == Role::Tool {
                if let Some(ref id) = msg.tool_call_id {
                    tool_response_ids.insert(id.clone());
                }
            }
        }

        // Determine which tool_call_ids are "complete" (have both call and response)
        let complete_tool_calls: HashSet<String> = tool_call_positions
            .keys()
            .filter(|id| tool_response_ids.contains(*id))
            .cloned()
            .collect();

        // Second pass: build result, keeping only valid message sequences
        let mut result: Vec<Message> = Vec::new();
        let mut i = 0;

        while i < messages.len() {
            let msg = &messages[i];

            // Handle tool messages - only keep if they have a matching COMPLETE tool call
            if msg.role == Role::Tool {
                if let Some(ref tool_call_id) = msg.tool_call_id {
                    if complete_tool_calls.contains(tool_call_id) {
                        // Check that we've already added the assistant message with this tool call
                        let assistant_idx = tool_call_positions.get(tool_call_id);
                        let already_added = assistant_idx.is_some_and(|_| {
                            result.iter().any(|m| {
                                if m.role == Role::Assistant {
                                    if let MessageContent::Parts(parts) = &m.content {
                                        parts.iter().any(|p| {
                                            matches!(p, ContentPart::ToolUse { id, .. } if id == tool_call_id)
                                        })
                                    } else {
                                        false
                                    }
                                } else {
                                    false
                                }
                            })
                        });

                        if already_added {
                            result.push(msg.clone());
                        } else {
                            tracing::warn!(
                                "Skipping tool response (assistant message not yet added for id: {})",
                                tool_call_id
                            );
                        }
                    } else {
                        tracing::warn!(
                            "Removing orphaned tool response (incomplete tool call for id: {})",
                            tool_call_id
                        );
                    }
                } else {
                    tracing::warn!("Removing tool message without tool_call_id");
                }
                i += 1;
                continue;
            }

            // Handle assistant messages with tool calls
            if msg.role == Role::Assistant {
                if let MessageContent::Parts(parts) = &msg.content {
                    let tool_ids: Vec<String> = parts
                        .iter()
                        .filter_map(|p| {
                            if let ContentPart::ToolUse { id, .. } = p {
                                Some(id.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    if !tool_ids.is_empty() {
                        // Check if ALL tool calls in this message have responses
                        let all_complete =
                            tool_ids.iter().all(|id| complete_tool_calls.contains(id));

                        if !all_complete {
                            tracing::warn!(
                                "Removing assistant message with incomplete tool calls: {:?}",
                                tool_ids
                            );
                            i += 1;
                            continue;
                        }
                    }
                }
            }

            // Keep this message
            result.push(msg.clone());
            i += 1;
        }

        result
    }

    /// Check if the model supports reasoning (o1/o3/o4 models)
    fn supports_reasoning(&self) -> bool {
        self.model.starts_with("o1") || self.model.starts_with("o3") || self.model.starts_with("o4")
    }

    /// Get reasoning effort for o1/o3/o4 models
    ///
    /// Only returns Some when:
    /// 1. The model supports reasoning
    /// 2. settings.enabled is true (controlled via /think command)
    fn get_reasoning_effort(&self, settings: &super::ThinkSettings) -> Option<String> {
        if settings.enabled && self.supports_reasoning() && !settings.reasoning_effort.is_empty() {
            Some(settings.reasoning_effort.clone())
        } else {
            None
        }
    }

    fn convert_messages(&self, messages: &[Message]) -> Vec<OpenAiMessage> {
        // Log input messages for debugging
        tracing::debug!("convert_messages input: {} messages", messages.len());
        for (i, msg) in messages.iter().enumerate() {
            let role = format!("{:?}", msg.role);
            let has_tool_call_id = msg.tool_call_id.is_some();
            let content_type = match &msg.content {
                MessageContent::Text(_) => "Text",
                MessageContent::Parts(parts) => {
                    if parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolUse { .. }))
                    {
                        "Parts(ToolUse)"
                    } else {
                        "Parts(other)"
                    }
                }
            };
            tracing::debug!(
                "  [{}] role={}, content={}, tool_call_id={}",
                i,
                role,
                content_type,
                has_tool_call_id
            );
        }

        // First sanitize to remove orphaned tool calls
        let sanitized = self.sanitize_messages(messages);

        tracing::debug!("After sanitize: {} messages", sanitized.len());
        for (i, msg) in sanitized.iter().enumerate() {
            let role = format!("{:?}", msg.role);
            tracing::debug!("  [{}] role={}", i, role);
        }

        sanitized
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

    /// Final validation: ensure tool messages follow assistant messages with tool_calls
    /// This is a safety net in case sanitize_messages misses something
    /// Uses a two-pass approach to ensure complete pairs only
    fn validate_tool_sequence(messages: &[OpenAiMessage]) -> Vec<OpenAiMessage> {
        use std::collections::{HashMap, HashSet};

        // First pass: find all assistant messages with tool_calls and their responses
        let mut assistant_tool_calls: HashMap<usize, Vec<String>> = HashMap::new(); // idx -> tool_call_ids
        let mut tool_responses: HashMap<String, usize> = HashMap::new(); // tool_call_id -> idx

        for (idx, msg) in messages.iter().enumerate() {
            if msg.role == "assistant" {
                if let Some(ref tool_calls) = msg.tool_calls {
                    let ids: Vec<String> = tool_calls.iter().map(|tc| tc.id.clone()).collect();
                    if !ids.is_empty() {
                        assistant_tool_calls.insert(idx, ids);
                    }
                }
            } else if msg.role == "tool" {
                if let Some(ref id) = msg.tool_call_id {
                    tool_responses.insert(id.clone(), idx);
                }
            }
        }

        // Find complete assistant messages (ALL their tool_calls have responses)
        let complete_assistants: HashSet<usize> = assistant_tool_calls
            .iter()
            .filter(|(_, ids)| ids.iter().all(|id| tool_responses.contains_key(id)))
            .map(|(idx, _)| *idx)
            .collect();

        // Find valid tool responses (their assistant message is complete)
        let valid_tool_call_ids: HashSet<String> = assistant_tool_calls
            .iter()
            .filter(|(idx, _)| complete_assistants.contains(idx))
            .flat_map(|(_, ids)| ids.clone())
            .collect();

        // Log what we're filtering
        let incomplete_assistants: Vec<usize> = assistant_tool_calls
            .keys()
            .filter(|idx| !complete_assistants.contains(idx))
            .copied()
            .collect();
        if !incomplete_assistants.is_empty() {
            tracing::warn!(
                "validate_tool_sequence: Filtering {} assistant messages with incomplete tool_calls at indices {:?}",
                incomplete_assistants.len(),
                incomplete_assistants
            );
        }

        let orphaned_tools: Vec<String> = tool_responses
            .keys()
            .filter(|id| !valid_tool_call_ids.contains(*id))
            .cloned()
            .collect();
        if !orphaned_tools.is_empty() {
            tracing::warn!(
                "validate_tool_sequence: Filtering {} orphaned tool responses: {:?}",
                orphaned_tools.len(),
                orphaned_tools
            );
        }

        // Second pass: build result with only valid messages
        let mut result: Vec<OpenAiMessage> = Vec::new();
        for (idx, msg) in messages.iter().enumerate() {
            if msg.role == "assistant" {
                if msg.tool_calls.is_some() {
                    // Only include if this assistant message is complete
                    if complete_assistants.contains(&idx) {
                        result.push(msg.clone());
                    }
                } else {
                    // Regular assistant message without tool_calls
                    result.push(msg.clone());
                }
            } else if msg.role == "tool" {
                // Only include if this tool response is for a valid tool_call
                if let Some(ref id) = msg.tool_call_id {
                    if valid_tool_call_ids.contains(id) {
                        result.push(msg.clone());
                    }
                }
            } else {
                // System or user message - always include
                result.push(msg.clone());
            }
        }

        tracing::info!(
            "validate_tool_sequence: {} messages in, {} messages out (filtered {})",
            messages.len(),
            result.len(),
            messages.len() - result.len()
        );

        result
    }

    async fn send_request(&self, request: OpenAiRequest) -> Result<OpenAiResponse> {
        // Final validation: ensure no orphaned tool messages
        let validated_messages = Self::validate_tool_sequence(&request.messages);
        let request = OpenAiRequest {
            messages: validated_messages,
            ..request
        };

        // Log request messages for debugging
        tracing::debug!("Sending {} messages to OpenAI", request.messages.len());
        for (i, msg) in request.messages.iter().enumerate() {
            let has_tool_calls = msg.tool_calls.is_some();
            let has_tool_call_id = msg.tool_call_id.is_some();
            tracing::debug!(
                "  [{}] role={}, tool_calls={}, tool_call_id={}",
                i,
                msg.role,
                has_tool_calls,
                has_tool_call_id
            );
        }

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
            // Log the messages that caused the error
            tracing::error!("OpenAI request failed. Messages sent:");
            for (i, msg) in request.messages.iter().enumerate() {
                tracing::error!(
                    "  [{}] role={}, tool_calls={:?}, tool_call_id={:?}",
                    i,
                    msg.role,
                    msg.tool_calls.is_some(),
                    msg.tool_call_id
                );
            }
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

    fn supports_native_thinking(&self) -> bool {
        self.supports_reasoning()
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        // Default: thinking off
        self.chat_with_thinking(messages, tools, &super::ThinkSettings::off())
            .await
    }

    async fn chat_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        settings: &super::ThinkSettings,
    ) -> Result<LlmResponse> {
        let openai_messages = self.convert_messages(messages);

        let mut request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(self.max_tokens),
            messages: openai_messages,
            tools: None,
            tool_choice: None,
            stream: None, // Non-streaming
            stream_options: None,
            reasoning_effort: self.get_reasoning_effort(settings),
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        let response = self.send_request(request).await?;

        // Convert OpenAI usage to our TokenUsage type
        let usage = response.usage.map(|u| crate::llm::TokenUsage {
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
        true // OpenAI supports native streaming
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        // Default: thinking off
        self.chat_streaming_with_thinking(
            messages,
            tools,
            callback,
            interrupt_check,
            &super::ThinkSettings::off(),
        )
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
        use futures::StreamExt;
        use tokio::time::{timeout, Duration};

        const STREAM_CHUNK_TIMEOUT: Duration = Duration::from_secs(60);
        const INTERRUPT_POLL_INTERVAL: Duration = Duration::from_millis(200);

        let openai_messages = self.convert_messages(messages);

        let mut request = OpenAiRequest {
            model: self.model.clone(),
            max_tokens: Some(self.max_tokens),
            messages: openai_messages,
            tools: None,
            tool_choice: None,
            stream: Some(true), // Enable streaming
            stream_options: Some(StreamOptions {
                include_usage: true,
            }), // Get usage stats at end
            reasoning_effort: self.get_reasoning_effort(settings),
        };

        if let Some(tools) = tools {
            if !tools.is_empty() {
                request.tools = Some(self.convert_tools(tools));
                request.tool_choice = Some("auto".to_string());
            }
        }

        // Final validation: ensure no orphaned tool messages
        let validated_messages = Self::validate_tool_sequence(&request.messages);
        let request = OpenAiRequest {
            messages: validated_messages,
            ..request
        };

        // Send streaming request
        let response = self
            .client
            .post(OPENAI_API_URL)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .context("Failed to send streaming request to OpenAI API")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            callback(StreamEvent::Error(format!(
                "OpenAI API error ({}): {}",
                status, error_text
            )));
            anyhow::bail!("OpenAI API error ({}): {}", status, error_text);
        }

        // Process SSE stream
        let mut builder = StreamingResponseBuilder::new();
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();

        // Track tool calls by index (OpenAI sends tool_calls with index)
        let mut tool_call_map: std::collections::HashMap<usize, (String, String, String)> =
            std::collections::HashMap::new(); // index -> (id, name, accumulated_args)

        let mut last_activity_at = std::time::Instant::now();
        loop {
            // Check for user interrupt frequently so Ctrl+C/Esc+Esc are responsive
            if let Some(check) = interrupt_check {
                if check() {
                    // Return partial response accumulated so far
                    for (id, name, args) in tool_call_map.values() {
                        builder
                            .tool_calls
                            .insert(id.clone(), (name.clone(), args.clone()));
                    }
                    return Ok(builder.build());
                }
            }

            // Enforce per-chunk timeout: if we haven't received any bytes recently, abort.
            if last_activity_at.elapsed() >= STREAM_CHUNK_TIMEOUT {
                anyhow::bail!(
                    "Stream timeout - no response from OpenAI for {} seconds",
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

            // SSE data comes in lines starting with "data: "
            buffer.push_str(&chunk_str);

            // Process complete lines
            while let Some(newline_pos) = buffer.find('\n') {
                let line = buffer[..newline_pos].trim().to_string();
                buffer = buffer[newline_pos + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                if let Some(json_str) = line.strip_prefix("data: ") {
                    if json_str == "[DONE]" {
                        // Stream complete
                        callback(StreamEvent::Done);
                        continue;
                    }

                    // Parse the chunk
                    if let Ok(chunk) = serde_json::from_str::<OpenAiStreamChunk>(json_str) {
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

                                    // Check if this is a new tool call or continuation
                                    if let Some(id) = &tc.id {
                                        // New tool call
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
                                        };
                                        builder.process(&event);
                                        callback(event);
                                    }

                                    // Accumulate arguments
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

                            // Check for finish_reason to emit tool call completion
                            if choice.finish_reason.is_some() {
                                // Emit ToolCallComplete for all tool calls
                                for (id, _, _) in tool_call_map.values() {
                                    let event = StreamEvent::ToolCallComplete { id: id.clone() };
                                    callback(event);
                                }
                            }
                        }

                        // Capture usage if provided (usually at the end)
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

        // Build and return final response
        // If we have tool calls from the map, add them to the builder
        for (_, (id, name, args)) in tool_call_map {
            builder.tool_calls.insert(id, (name, args));
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
            stream: None,
            stream_options: None,
            reasoning_effort: None, // FIM doesn't need reasoning
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

        // Extract usage from OpenAI response
        let usage = response.usage.map(|u| TokenUsage {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        });

        Ok(CompletionResult { text, usage })
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
            stream: None,
            stream_options: None,
            reasoning_effort: None, // Code explanation doesn't need reasoning
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
            stream: None,
            stream_options: None,
            reasoning_effort: None, // Refactoring suggestions don't need reasoning
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
            stream: None,
            reasoning_effort: None, // Code review doesn't need reasoning
            stream_options: None,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning_effort: Option<String>,
}

/// Options for streaming responses
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StreamOptions {
    /// Include usage statistics in the final chunk
    include_usage: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiFunctionCall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    message: OpenAiMessage,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

// Streaming response types

#[derive(Debug, Deserialize)]
struct OpenAiStreamChunk {
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiStreamToolCall>>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamToolCall {
    #[serde(default)]
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiStreamFunction>,
}

#[derive(Debug, Deserialize)]
struct OpenAiStreamFunction {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openai_response_with_usage() {
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

        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage.is_some());
        let usage = response.usage.unwrap();
        assert_eq!(usage.prompt_tokens, 10);
        assert_eq!(usage.completion_tokens, 5);
        assert_eq!(usage.total_tokens, 15);
    }

    #[test]
    fn test_parse_openai_response_without_usage() {
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello"
                }
            }]
        }"#;

        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage.is_none());
    }

    #[test]
    fn test_convert_openai_usage_to_token_usage() {
        let openai_usage = OpenAiUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
        };

        let token_usage = crate::llm::TokenUsage {
            input_tokens: openai_usage.prompt_tokens,
            output_tokens: openai_usage.completion_tokens,
            total_tokens: openai_usage.total_tokens,
        };

        assert_eq!(token_usage.input_tokens, 100);
        assert_eq!(token_usage.output_tokens, 50);
        assert_eq!(token_usage.total_tokens, 150);
    }

    #[test]
    fn test_parse_openai_response_with_tool_calls() {
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
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "total_tokens": 30
            }
        }"#;

        let response: OpenAiResponse = serde_json::from_str(json).unwrap();
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().total_tokens, 30);
    }

    #[test]
    fn test_validate_tool_sequence_filters_orphaned_tool_messages() {
        // Simulates a restored session where assistant messages lost their tool_calls
        let messages = vec![
            OpenAiMessage {
                role: "system".to_string(),
                content: Some("You are a helpful assistant.".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            // Tool message without preceding assistant with tool_calls
            OpenAiMessage {
                role: "tool".to_string(),
                content: Some("tool result".to_string()),
                tool_calls: None,
                tool_call_id: Some("call_orphaned".to_string()),
            },
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Hello".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let result = OpenAiProvider::validate_tool_sequence(&messages);

        // Tool message should be filtered out
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].role, "user");
    }

    #[test]
    fn test_validate_tool_sequence_keeps_valid_tool_calls() {
        let messages = vec![
            OpenAiMessage {
                role: "system".to_string(),
                content: Some("You are a helpful assistant.".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Search for files".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            // Assistant with tool_calls
            OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_123".to_string(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            // Matching tool result
            OpenAiMessage {
                role: "tool".to_string(),
                content: Some("Found 5 files".to_string()),
                tool_calls: None,
                tool_call_id: Some("call_123".to_string()),
            },
        ];

        let result = OpenAiProvider::validate_tool_sequence(&messages);

        // All messages should be kept
        assert_eq!(result.len(), 4);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].role, "user");
        assert_eq!(result[2].role, "assistant");
        assert!(result[2].tool_calls.is_some());
        assert_eq!(result[3].role, "tool");
    }

    #[test]
    fn test_sanitize_messages_filters_orphaned_tool_messages_from_restored_session() {
        // Simulates a restored session where assistant messages have been converted to plain text
        // but tool messages still have their tool_call_id
        let messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::Text("You are a helpful assistant.".to_string()),
                tool_call_id: None,
            },
            // Plain text assistant message (restored from session, originally had tool_calls)
            Message {
                role: Role::Assistant,
                content: MessageContent::Text(
                    "[Tool: grep with {\"pattern\": \"test\"}] Found 5 files".to_string(),
                ),
                tool_call_id: None,
            },
            // Tool message with tool_call_id (orphaned because assistant lost ToolUse parts)
            Message {
                role: Role::Tool,
                content: MessageContent::Text("grep result: 5 matches".to_string()),
                tool_call_id: Some("call_123".to_string()),
            },
            Message {
                role: Role::User,
                content: MessageContent::Text("Thanks!".to_string()),
                tool_call_id: None,
            },
        ];

        let provider = OpenAiProvider {
            client: reqwest::Client::new(),
            api_key: "test".to_string(),
            model: "gpt-4".to_string(),
            max_tokens: 1000,
        };

        let result = provider.sanitize_messages(&messages);

        // Tool message should be filtered out since there's no matching ToolUse in assistant
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].role, Role::System);
        assert_eq!(result[1].role, Role::Assistant);
        assert_eq!(result[2].role, Role::User);
        // Verify no tool messages
        assert!(!result.iter().any(|m| m.role == Role::Tool));
    }

    #[test]
    fn test_validate_tool_sequence_removes_incomplete_assistant_tool_calls() {
        let messages = vec![
            OpenAiMessage {
                role: "system".to_string(),
                content: Some("You are a helpful assistant.".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            // Assistant with tool_calls but NO matching tool result
            OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_unmatched".to_string(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Thanks".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let result = OpenAiProvider::validate_tool_sequence(&messages);

        // Assistant with unmatched tool_calls should be filtered out
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].role, "system");
        assert_eq!(result[1].role, "user");
    }

    #[test]
    fn test_validate_tool_sequence_long_conversation_with_incomplete_tool_calls() {
        // Simulates a long conversation (like the user's case with 68+ messages)
        // where one assistant message has tool_calls without responses
        let mut messages = vec![OpenAiMessage {
            role: "system".to_string(),
            content: Some("You are a helpful assistant.".to_string()),
            tool_calls: None,
            tool_call_id: None,
        }];

        // Add many complete tool call rounds
        for i in 0..30 {
            // User message
            messages.push(OpenAiMessage {
                role: "user".to_string(),
                content: Some(format!("Question {}", i)),
                tool_calls: None,
                tool_call_id: None,
            });

            // Assistant with tool_calls
            messages.push(OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: format!("call_complete_{}", i),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            });

            // Matching tool result
            messages.push(OpenAiMessage {
                role: "tool".to_string(),
                content: Some(format!("Result {}", i)),
                tool_calls: None,
                tool_call_id: Some(format!("call_complete_{}", i)),
            });

            // Assistant response
            messages.push(OpenAiMessage {
                role: "assistant".to_string(),
                content: Some(format!("Here's the result for question {}", i)),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // Add one INCOMPLETE tool call at the end (simulating interrupted execution)
        messages.push(OpenAiMessage {
            role: "user".to_string(),
            content: Some("Final question".to_string()),
            tool_calls: None,
            tool_call_id: None,
        });

        messages.push(OpenAiMessage {
            role: "assistant".to_string(),
            content: None,
            tool_calls: Some(vec![OpenAiToolCall {
                id: "call_incomplete_final".to_string(),
                call_type: "function".to_string(),
                function: OpenAiFunctionCall {
                    name: "grep".to_string(),
                    arguments: "{}".to_string(),
                },
            }]),
            tool_call_id: None,
        });
        // Note: NO tool result for call_incomplete_final

        let original_len = messages.len();
        let result = OpenAiProvider::validate_tool_sequence(&messages);

        // Should have filtered out the incomplete assistant message
        assert_eq!(
            result.len(),
            original_len - 1,
            "Should remove exactly 1 message (the incomplete assistant)"
        );

        // Verify no assistant messages with unmatched tool_calls remain
        for msg in &result {
            if msg.role == "assistant" {
                if let Some(ref tool_calls) = msg.tool_calls {
                    // All assistant messages with tool_calls should have matching responses
                    for tc in tool_calls {
                        assert!(
                            tc.id.starts_with("call_complete_"),
                            "Found incomplete tool_call {} in result",
                            tc.id
                        );
                    }
                }
            }
        }

        // Verify no orphaned tool messages
        for msg in &result {
            if msg.role == "tool" {
                if let Some(ref id) = msg.tool_call_id {
                    assert!(
                        id.starts_with("call_complete_"),
                        "Found orphaned tool message {} in result",
                        id
                    );
                }
            }
        }
    }

    #[test]
    fn test_validate_tool_sequence_multiple_incomplete_throughout() {
        // Test with incomplete tool calls scattered throughout the conversation
        let messages = vec![
            OpenAiMessage {
                role: "system".to_string(),
                content: Some("System prompt".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            // First incomplete tool call
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Q1".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_incomplete_1".to_string(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            // No tool result for call_incomplete_1
            // Complete tool call in the middle
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Q2".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_complete".to_string(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "tool".to_string(),
                content: Some("Result".to_string()),
                tool_calls: None,
                tool_call_id: Some("call_complete".to_string()),
            },
            OpenAiMessage {
                role: "assistant".to_string(),
                content: Some("Done".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            // Second incomplete tool call
            OpenAiMessage {
                role: "user".to_string(),
                content: Some("Q3".to_string()),
                tool_calls: None,
                tool_call_id: None,
            },
            OpenAiMessage {
                role: "assistant".to_string(),
                content: None,
                tool_calls: Some(vec![OpenAiToolCall {
                    id: "call_incomplete_2".to_string(),
                    call_type: "function".to_string(),
                    function: OpenAiFunctionCall {
                        name: "grep".to_string(),
                        arguments: "{}".to_string(),
                    },
                }]),
                tool_call_id: None,
            },
            // No tool result for call_incomplete_2
        ];

        let result = OpenAiProvider::validate_tool_sequence(&messages);

        // Count remaining assistant messages with tool_calls
        let assistant_with_tools: Vec<_> = result
            .iter()
            .filter(|m| m.role == "assistant" && m.tool_calls.is_some())
            .collect();

        // Only the complete one should remain
        assert_eq!(
            assistant_with_tools.len(),
            1,
            "Only 1 complete assistant with tool_calls should remain"
        );
        assert_eq!(
            assistant_with_tools[0].tool_calls.as_ref().unwrap()[0].id,
            "call_complete"
        );

        // Only the matching tool result should remain
        let tool_results: Vec<_> = result.iter().filter(|m| m.role == "tool").collect();
        assert_eq!(tool_results.len(), 1);
        assert_eq!(
            tool_results[0].tool_call_id.as_ref().unwrap(),
            "call_complete"
        );
    }
}
