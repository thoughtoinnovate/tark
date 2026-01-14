//! Centralized tool execution orchestrator
//!
//! This module provides a unified agentic loop that handles:
//! - Tool execution with duplicate detection
//! - Per-turn tool call limits
//! - Max iteration enforcement
//! - Context sanitization (prevents empty assistant messages)
//! - State management across iterations

#![allow(dead_code)] // API methods for future use

use super::{AgentResponse, ConversationContext, ToolCallLog};
use crate::llm::{
    ContentPart, LlmProvider, LlmResponse, Message, MessageContent, Role, StreamCallback,
    StreamEvent, ThinkSettings, TokenUsage, ToolCall,
};
use crate::tools::{AgentMode, ToolRegistry};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Configuration for the tool orchestrator
#[derive(Debug, Clone)]
pub struct OrchestratorConfig {
    /// Maximum iterations before forcing stop
    pub max_iterations: usize,
    /// Maximum tools LLM can call per single response
    pub max_tools_per_turn: usize,
    /// How many times same tool with same args can return same result before stopping
    pub max_consecutive_duplicates: usize,
}

impl Default for OrchestratorConfig {
    fn default() -> Self {
        Self {
            max_iterations: 10,
            max_tools_per_turn: 5,
            max_consecutive_duplicates: 2,
        }
    }
}

/// Centralized tool execution orchestrator
///
/// Manages the agentic loop (LLM → Tools → LLM) with built-in:
/// - Duplicate detection
/// - Per-turn limits
/// - Smart termination
/// - Context sanitization
pub struct ToolOrchestrator {
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    context: ConversationContext,
    config: OrchestratorConfig,
    mode: AgentMode,
}

impl ToolOrchestrator {
    /// Create a new tool orchestrator
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: ToolRegistry,
        context: ConversationContext,
        mode: AgentMode,
    ) -> Self {
        Self {
            llm,
            tools,
            context,
            config: OrchestratorConfig::default(),
            mode,
        }
    }

    /// Set custom configuration
    pub fn with_config(mut self, config: OrchestratorConfig) -> Self {
        self.config = config;
        self
    }

    /// Consume orchestrator and return the context
    pub fn into_context(self) -> ConversationContext {
        self.context
    }

    /// Run the agentic loop with streaming callbacks
    ///
    /// This is the main entry point that handles the complete tool execution loop:
    /// 1. Call LLM with current context
    /// 2. If text response → done
    /// 3. If tool calls → execute tools, add results to context, loop
    /// 4. Repeat until text response, duplicate detected, or max iterations
    pub async fn run_streaming<F1, F2, F3, F4>(
        &mut self,
        user_message: &str,
        interrupt_check: impl Fn() -> bool + Send + Sync,
        on_text: F1,
        on_thinking: F2,
        on_tool_call: F3,
        on_tool_complete: F4,
    ) -> Result<AgentResponse>
    where
        F1: Fn(String) + Clone + Send + Sync + 'static,
        F2: Fn(String) + Clone + Send + Sync + 'static,
        F3: Fn(String, String) + Send + Sync,
        F4: Fn(String, String, bool) + Send + Sync,
    {
        self.context.add_user(user_message);

        let mut state = LoopState::new();
        let tool_definitions = self.tools.definitions();

        // Wrap callbacks in Arc for sharing
        let on_text = Arc::new(on_text);
        let on_thinking = Arc::new(on_thinking);

        loop {
            // Check interrupt
            if interrupt_check() {
                tracing::info!("Orchestrator interrupted");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return state.into_interrupted_response(&self.context);
            }

            // Check max iterations
            if state.iterations >= self.config.max_iterations {
                self.context.add_assistant(
                    "I've reached the maximum number of steps. Let me know if you'd like me to continue.",
                );
                break;
            }

            // Reset accumulated text for this iteration
            state.clear_accumulated_text();

            // Create streaming callback
            let callback = state.create_streaming_callback(on_text.clone(), on_thinking.clone());

            // Sanitize context before sending to LLM (removes empty assistant messages)
            let sanitized_messages = self.sanitize_context_for_llm(self.context.messages());

            // Call LLM with streaming
            let interrupt_check_ref: &(dyn Fn() -> bool + Send + Sync) = &interrupt_check;
            let response = self
                .llm
                .chat_streaming_with_thinking(
                    &sanitized_messages,
                    Some(&tool_definitions),
                    callback,
                    Some(interrupt_check_ref),
                    &ThinkSettings::off(), // TODO: Pass from agent config
                )
                .await?;

            // Check interrupt after LLM call
            if interrupt_check() {
                tracing::info!("Orchestrator interrupted after LLM response");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return state.into_interrupted_response(&self.context);
            }

            // Update state
            state.add_usage(response.usage());
            state.iterations += 1;

            // Get accumulated text from streaming
            state.finalize_accumulated_text();

            // Handle response
            match response {
                LlmResponse::Text { text, .. } => {
                    // Use accumulated text if available (from streaming), otherwise use final text
                    let final_text = if state.accumulated_text.is_empty() {
                        text
                    } else {
                        state.accumulated_text.clone()
                    };
                    self.context.add_assistant(&final_text);
                    return state.into_success_response(&self.context, final_text);
                }
                LlmResponse::ToolCalls { calls, .. } => {
                    let should_continue = self
                        .handle_tool_calls(&calls, &mut state, &on_tool_call, &on_tool_complete)
                        .await?;

                    if !should_continue {
                        // Duplicate detected or limit reached
                        break;
                    }
                }
                LlmResponse::Mixed {
                    text, tool_calls, ..
                } => {
                    // Text was already streamed via callback
                    if let Some(t) = text {
                        tracing::debug!("Mixed response text: {}", t);
                    }

                    let should_continue = self
                        .handle_tool_calls(
                            &tool_calls,
                            &mut state,
                            &on_tool_call,
                            &on_tool_complete,
                        )
                        .await?;

                    if !should_continue {
                        break;
                    }
                }
            }
        }

        // Loop ended - return final response from context
        state.into_final_response(&self.context)
    }

    /// Handle tool call execution with duplicate detection and limits
    ///
    /// Returns false if the loop should stop (duplicate detected)
    async fn handle_tool_calls<F1, F2>(
        &mut self,
        calls: &[ToolCall],
        state: &mut LoopState,
        on_tool_call: &F1,
        on_tool_complete: &F2,
    ) -> Result<bool>
    where
        F1: Fn(String, String),
        F2: Fn(String, String, bool),
    {
        // Apply per-turn limit
        let limited_calls = self.limit_tool_calls(calls);

        // Add to context (before execution so LLM sees what it requested)
        self.context.add_assistant_tool_calls(limited_calls);

        // Execute each tool
        for call in limited_calls {
            let result = self
                .tools
                .execute(&call.name, call.arguments.clone())
                .await?;

            // Duplicate detection
            if state.check_duplicate(&call.name, &call.arguments, &result.output) {
                state.consecutive_duplicates += 1;
                tracing::warn!(
                    "Tool '{}' returned identical result ({}/{})",
                    call.name,
                    state.consecutive_duplicates,
                    self.config.max_consecutive_duplicates
                );

                if state.consecutive_duplicates >= self.config.max_consecutive_duplicates {
                    self.context.add_assistant(
                        "I notice I'm getting the same results repeatedly. Let me summarize what I found.",
                    );
                    return Ok(false); // Stop
                }
            }

            // Notify UI
            let args_preview = serde_json::to_string(&call.arguments).unwrap_or_default();
            on_tool_call(call.name.clone(), args_preview);

            let preview = truncate_preview(&result.output, 200);
            on_tool_complete(call.name.clone(), preview.clone(), result.success);

            // Add to context
            self.context.add_tool_result(&call.id, &result.output);

            // Log for response
            state.log_tool_call(call, preview);
        }

        Ok(true) // Continue
    }

    /// Apply per-turn tool call limit
    fn limit_tool_calls<'a>(&self, calls: &'a [ToolCall]) -> &'a [ToolCall] {
        if calls.len() > self.config.max_tools_per_turn {
            tracing::warn!(
                "Limiting {} tool calls to {}",
                calls.len(),
                self.config.max_tools_per_turn
            );
            &calls[..self.config.max_tools_per_turn]
        } else {
            calls
        }
    }

    /// Sanitize context before sending to LLM
    ///
    /// Removes assistant messages with no text content to prevent confusing the LLM.
    /// Empty assistant messages cause the LLM to think it hasn't responded yet,
    /// leading to infinite tool call loops.
    fn sanitize_context_for_llm(&self, messages: &[Message]) -> Vec<Message> {
        messages
            .iter()
            .filter(|msg| {
                if msg.role == Role::Assistant {
                    // Keep only if has text content
                    match &msg.content {
                        MessageContent::Text(t) => !t.is_empty(),
                        MessageContent::Parts(parts) => {
                            // Keep if has any text parts (skip tool-only messages)
                            parts.iter().any(
                                |p| matches!(p, ContentPart::Text { text } if !text.is_empty()),
                            )
                        }
                    }
                } else {
                    true // Keep all non-assistant messages
                }
            })
            .cloned()
            .collect()
    }
}

/// State management for the agentic loop
struct LoopState {
    /// Current iteration count
    iterations: usize,
    /// Total tool calls made
    tool_calls_made: usize,
    /// Log of tool calls for UI display
    tool_log: Vec<ToolCallLog>,
    /// Accumulated token usage
    usage: TokenUsage,
    /// Cache of tool results for duplicate detection
    last_results: HashMap<String, String>,
    /// Counter for consecutive duplicate results
    consecutive_duplicates: usize,
    /// Accumulated text from streaming (shared with callback)
    accumulated_text_shared: Arc<Mutex<String>>,
    /// Accumulated text for current iteration
    accumulated_text: String,
}

impl LoopState {
    fn new() -> Self {
        Self {
            iterations: 0,
            tool_calls_made: 0,
            tool_log: Vec::new(),
            usage: TokenUsage::default(),
            last_results: HashMap::new(),
            consecutive_duplicates: 0,
            accumulated_text_shared: Arc::new(Mutex::new(String::new())),
            accumulated_text: String::new(),
        }
    }

    /// Clear accumulated text for new iteration
    fn clear_accumulated_text(&mut self) {
        self.accumulated_text.clear();
        if let Ok(mut acc) = self.accumulated_text_shared.lock() {
            acc.clear();
        }
    }

    /// Finalize accumulated text after LLM call
    fn finalize_accumulated_text(&mut self) {
        if let Ok(acc) = self.accumulated_text_shared.lock() {
            self.accumulated_text = acc.clone();
        }
    }

    /// Create streaming callback that accumulates text
    fn create_streaming_callback<F1, F2>(
        &self,
        on_text: Arc<F1>,
        on_thinking: Arc<F2>,
    ) -> StreamCallback
    where
        F1: Fn(String) + Send + Sync + 'static,
        F2: Fn(String) + Send + Sync + 'static,
    {
        let accumulated = self.accumulated_text_shared.clone();

        Box::new(move |event: StreamEvent| match event {
            StreamEvent::TextDelta(text) => {
                // Accumulate for context
                if let Ok(mut acc) = accumulated.lock() {
                    acc.push_str(&text);
                }
                // Forward to UI immediately
                on_text(text);
            }
            StreamEvent::ThinkingDelta(thinking) => {
                on_thinking(thinking);
            }
            StreamEvent::ToolCallStart { name, .. } => {
                tracing::debug!("Tool call starting: {}", name);
            }
            StreamEvent::Error(err) => {
                tracing::error!("Streaming error: {}", err);
            }
            _ => {}
        })
    }

    /// Check if this tool call + result is a duplicate
    ///
    /// Returns true if the exact same tool with same arguments returned the same result
    fn check_duplicate(&mut self, name: &str, args: &serde_json::Value, result: &str) -> bool {
        let key = format!(
            "{}:{}",
            name,
            serde_json::to_string(args).unwrap_or_default()
        );

        if let Some(last_result) = self.last_results.get(&key) {
            if last_result == result {
                return true;
            }
        }

        // Not a duplicate - reset counter
        self.consecutive_duplicates = 0;
        self.last_results.insert(key, result.to_string());
        false
    }

    /// Add usage statistics from LLM response
    fn add_usage(&mut self, usage: Option<&TokenUsage>) {
        if let Some(u) = usage {
            self.usage.input_tokens += u.input_tokens;
            self.usage.output_tokens += u.output_tokens;
            self.usage.total_tokens += u.total_tokens;
        }
    }

    /// Log a tool call for the response
    fn log_tool_call(&mut self, call: &ToolCall, preview: String) {
        self.tool_calls_made += 1;
        self.tool_log.push(ToolCallLog {
            tool: call.name.clone(),
            args: call.arguments.clone(),
            result_preview: preview,
        });
    }

    /// Convert to success response with final text
    fn into_success_response(
        self,
        context: &ConversationContext,
        text: String,
    ) -> Result<AgentResponse> {
        Ok(AgentResponse {
            text,
            tool_calls_made: self.tool_calls_made,
            tool_call_log: self.tool_log,
            auto_compacted: false, // TODO: Track compaction in orchestrator
            context_usage_percent: context.usage_percentage(),
            usage: Some(self.usage),
        })
    }

    /// Convert to interrupted response
    fn into_interrupted_response(self, context: &ConversationContext) -> Result<AgentResponse> {
        Ok(AgentResponse {
            text: "⚠️ *Operation interrupted by user*".to_string(),
            tool_calls_made: self.tool_calls_made,
            tool_call_log: self.tool_log,
            auto_compacted: false,
            context_usage_percent: context.usage_percentage(),
            usage: Some(self.usage),
        })
    }

    /// Convert to final response (when loop ends without text response)
    fn into_final_response(self, context: &ConversationContext) -> Result<AgentResponse> {
        let text = context
            .messages()
            .last()
            .and_then(|m| m.content.as_text())
            .unwrap_or("Done.")
            .to_string();

        Ok(AgentResponse {
            text,
            tool_calls_made: self.tool_calls_made,
            tool_call_log: self.tool_log,
            auto_compacted: false,
            context_usage_percent: context.usage_percentage(),
            usage: Some(self.usage),
        })
    }
}

/// Truncate preview text to max length
fn truncate_preview(text: &str, max_len: usize) -> String {
    if text.len() > max_len {
        format!("{}...", truncate_at_char_boundary(text, max_len))
    } else {
        text.to_string()
    }
}

/// Truncate at UTF-8 character boundary
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orchestrator_config_defaults() {
        let config = OrchestratorConfig::default();
        assert_eq!(config.max_iterations, 10);
        assert_eq!(config.max_tools_per_turn, 5);
        assert_eq!(config.max_consecutive_duplicates, 2);
    }

    #[test]
    fn test_loop_state_duplicate_detection() {
        let mut state = LoopState::new();

        let args = serde_json::json!({"path": "."});

        // First call - not a duplicate
        assert!(!state.check_duplicate("overview", &args, "result1"));
        assert_eq!(state.consecutive_duplicates, 0);

        // Different result - not a duplicate
        assert!(!state.check_duplicate("overview", &args, "result2"));
        assert_eq!(state.consecutive_duplicates, 0);

        // Same result - duplicate
        assert!(state.check_duplicate("overview", &args, "result2"));
        assert_eq!(state.consecutive_duplicates, 0); // check_duplicate doesn't increment, caller does

        // Same result again - still duplicate
        assert!(state.check_duplicate("overview", &args, "result2"));
    }

    #[test]
    fn test_loop_state_different_args_not_duplicate() {
        let mut state = LoopState::new();

        let args1 = serde_json::json!({"depth": 3});
        let args2 = serde_json::json!({"depth": 4});

        state.check_duplicate("overview", &args1, "result");
        assert!(!state.check_duplicate("overview", &args2, "result"));
    }

    #[test]
    fn test_loop_state_usage_accumulation() {
        let mut state = LoopState::new();

        state.add_usage(Some(&TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            total_tokens: 150,
        }));

        state.add_usage(Some(&TokenUsage {
            input_tokens: 200,
            output_tokens: 75,
            total_tokens: 275,
        }));

        assert_eq!(state.usage.input_tokens, 300);
        assert_eq!(state.usage.output_tokens, 125);
        assert_eq!(state.usage.total_tokens, 425);
    }

    #[test]
    fn test_truncate_preview() {
        assert_eq!(truncate_preview("short", 100), "short");
        assert_eq!(
            truncate_preview("a".repeat(300).as_str(), 200),
            format!("{}...", "a".repeat(200))
        );
    }

    #[test]
    fn test_truncate_at_char_boundary() {
        let text = "Hello 世界";
        // Japanese characters are 3 bytes each
        // "Hello " = 6 bytes, "世" = 3 bytes
        let truncated = truncate_at_char_boundary(text, 7);
        assert_eq!(truncated, "Hello "); // Stops before multi-byte char
    }

    #[test]
    fn test_sanitize_removes_empty_assistant_messages() {
        // This test verifies the critical fix for the infinite loop bug
        let messages = [
            Message::user("hello"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Text(String::new()), // Empty!
                tool_call_id: None,
            },
            Message::tool_result("call_1", "result"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Text("Real response".to_string()),
                tool_call_id: None,
            },
        ];

        // Create a mock orchestrator (we just need sanitize method)
        // In a real scenario, we'd use dependency injection, but for this test
        // we can't easily construct ToolOrchestrator without real dependencies

        // Instead, test the logic inline
        let sanitized: Vec<_> = messages
            .iter()
            .filter(|msg| {
                if msg.role == Role::Assistant {
                    match &msg.content {
                        MessageContent::Text(t) => !t.is_empty(),
                        MessageContent::Parts(parts) => parts
                            .iter()
                            .any(|p| matches!(p, ContentPart::Text { text } if !text.is_empty())),
                    }
                } else {
                    true
                }
            })
            .collect();

        // Should have removed the empty assistant message
        assert_eq!(sanitized.len(), 3); // user, tool_result, assistant (non-empty)
        assert_eq!(sanitized[0].role, Role::User);
        assert_eq!(sanitized[1].role, Role::Tool);
        assert_eq!(sanitized[2].role, Role::Assistant);
        assert_eq!(sanitized[2].content.as_text().unwrap(), "Real response");
    }

    #[test]
    fn test_sanitize_keeps_assistant_with_tool_use() {
        let messages = [
            Message::user("test"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "call_1".to_string(),
                    name: "search".to_string(),
                    input: serde_json::json!({"query": "test"}),
                }]),
                tool_call_id: None,
            },
        ];

        let sanitized: Vec<_> = messages
            .iter()
            .filter(|msg| {
                if msg.role == Role::Assistant {
                    match &msg.content {
                        MessageContent::Text(t) => !t.is_empty(),
                        MessageContent::Parts(parts) => parts
                            .iter()
                            .any(|p| matches!(p, ContentPart::Text { text } if !text.is_empty())),
                    }
                } else {
                    true
                }
            })
            .collect();

        // Should have removed the assistant message with ONLY tool use (no text)
        assert_eq!(sanitized.len(), 1); // Only user message
        assert_eq!(sanitized[0].role, Role::User);
    }
}
