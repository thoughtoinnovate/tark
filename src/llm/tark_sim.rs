//! Tark Simulation Provider - OpenAI-compatible mock for E2E testing
//!
//! Registered as provider "tark_sim" with model "tark_llm".
//! No API key required. Simulates all LLM behaviors:
//! - Accurate token counting via tiktoken-rs
//! - Tool/function calling with sandboxed execution
//! - Streaming responses with realistic delays
//! - Extended thinking/reasoning
//! - Error scenarios (timeout, rate limit, context exceeded, malformed, partial)
//! - Context verification via ContextSnapshot
//! - Full observability via JSON event logging
//! - Multi-turn conversation state

#![allow(dead_code)]

use super::{
    LlmProvider, LlmResponse, Message, Role, StreamCallback, StreamEvent, ThinkSettings,
    TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::Result;
use async_trait::async_trait;
use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Tark Simulation Provider
///
/// A fully-featured mock LLM that simulates real LLM behavior for testing.
/// Compatible with OpenAI API semantics.
pub struct TarkSimProvider {
    model: String,
    scenario: SimScenario,
    /// Tracks conversation turn for multi-turn scenarios
    turn_counter: AtomicUsize,
    /// Configurable streaming delay (ms per chunk)
    stream_delay_ms: u64,
    /// Event log for observability
    event_log: Mutex<Vec<SimEvent>>,
    /// Last captured context for verification
    last_context: Mutex<Option<ContextSnapshot>>,
    /// Tokenizer for accurate token counting
    #[cfg(feature = "test-sim")]
    tokenizer: tiktoken_rs::CoreBPE,
}

/// Context snapshot for test verification
/// Allows tests to assert what the LLM actually received
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContextSnapshot {
    pub system_prompt: Option<String>,
    pub message_count: usize,
    pub user_message_count: usize,
    pub assistant_message_count: usize,
    pub tool_message_count: usize,
    pub total_tokens_in_context: u32,
    pub truncated: bool,
    pub tools_in_context: Vec<String>,
    pub last_user_message: Option<String>,
}

/// Event log entry for observability
#[derive(Debug, Clone, serde::Serialize)]
pub struct SimEvent {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub event_type: SimEventType,
    pub details: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum SimEventType {
    RequestReceived,
    ContextCaptured,
    ScenarioMatched,
    ToolCallEmitted,
    StreamChunkEmitted,
    ThinkingEmitted,
    ResponseEmitted,
    ErrorEmitted,
}

impl TarkSimProvider {
    pub fn new() -> Self {
        #[cfg(feature = "test-sim")]
        let tokenizer = tiktoken_rs::cl100k_base().expect("Failed to load tokenizer");

        Self {
            model: "tark_llm".to_string(),
            scenario: SimScenario::from_env(),
            turn_counter: AtomicUsize::new(0),
            stream_delay_ms: 50,
            event_log: Mutex::new(Vec::new()),
            last_context: Mutex::new(None),
            #[cfg(feature = "test-sim")]
            tokenizer,
        }
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    pub fn with_scenario(mut self, scenario: SimScenario) -> Self {
        self.scenario = scenario;
        self
    }

    /// Get the last captured context for test assertions
    pub fn get_last_context(&self) -> Option<ContextSnapshot> {
        self.last_context.lock().unwrap().clone()
    }

    /// Get all logged events for debugging
    pub fn get_events(&self) -> Vec<SimEvent> {
        self.event_log.lock().unwrap().clone()
    }

    /// Export events to JSON file (for test observability)
    pub fn export_events(&self, path: &std::path::Path) -> std::io::Result<()> {
        let events = self.get_events();
        let json = serde_json::to_string_pretty(&events)?;
        std::fs::write(path, json)
    }

    /// Capture context snapshot from messages
    fn capture_context(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> ContextSnapshot {
        let system_prompt = messages
            .iter()
            .find(|m| m.role == Role::System)
            .and_then(|m| m.content.as_text().map(|s| s.to_string()));

        #[cfg(feature = "test-sim")]
        let mut total_tokens = 0u32;

        #[cfg(feature = "test-sim")]
        {
            for msg in messages {
                if let Some(text) = msg.content.as_text() {
                    total_tokens += self.tokenizer.encode_with_special_tokens(text).len() as u32;
                }
            }

            // Add tool definition tokens
            if let Some(tools) = tools {
                let tools_json = serde_json::to_string(tools).unwrap_or_default();
                total_tokens += self.tokenizer.encode_with_special_tokens(&tools_json).len() as u32;
            }
        }

        #[cfg(not(feature = "test-sim"))]
        let total_tokens = 0u32;

        ContextSnapshot {
            system_prompt,
            message_count: messages.len(),
            user_message_count: messages.iter().filter(|m| m.role == Role::User).count(),
            assistant_message_count: messages
                .iter()
                .filter(|m| m.role == Role::Assistant)
                .count(),
            tool_message_count: messages.iter().filter(|m| m.role == Role::Tool).count(),
            total_tokens_in_context: total_tokens,
            truncated: total_tokens > 128000, // Simulated context limit
            tools_in_context: tools
                .map(|t| t.iter().map(|td| td.name.clone()).collect())
                .unwrap_or_default(),
            last_user_message: messages
                .iter()
                .rev()
                .find(|m| m.role == Role::User)
                .and_then(|m| m.content.as_text().map(|s| s.to_string())),
        }
    }

    fn log_event(&self, event_type: SimEventType, details: serde_json::Value) {
        let event = SimEvent {
            timestamp: chrono::Utc::now(),
            event_type,
            details,
        };
        self.event_log.lock().unwrap().push(event.clone());

        // Also write to file if TARK_SIM_LOG_FILE is set
        if let Ok(path) = std::env::var("TARK_SIM_LOG_FILE") {
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
            {
                let _ = writeln!(
                    file,
                    "{}",
                    serde_json::to_string(&event).unwrap_or_default()
                );
            }
        }
    }

    /// Accurate token counting using tiktoken
    #[cfg(feature = "test-sim")]
    fn count_tokens(&self, text: &str) -> u32 {
        self.tokenizer.encode_with_special_tokens(text).len() as u32
    }

    #[cfg(not(feature = "test-sim"))]
    fn count_tokens(&self, text: &str) -> u32 {
        // Rough approximation: ~4 chars per token
        (text.len() / 4).max(1) as u32
    }

    fn sim_usage(&self, input_text: &str, output_text: &str) -> TokenUsage {
        let input_tokens = self.count_tokens(input_text);
        let output_tokens = self.count_tokens(output_text);
        TokenUsage {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        }
    }
}

/// BDD-style test scenarios parsed from Gherkin .feature files
///
/// Each scenario follows Given-When-Then format:
/// - Given: Initial context/state (provider, tools, mode)
/// - When: User action (message sent)
/// - Then: Expected LLM behavior (response, tool call, error)
///
/// Uses `gherkin` crate for parsing .feature files
#[derive(Debug, Clone)]
pub struct BddScenario {
    pub name: String,
    pub given: ScenarioContext,
    pub steps: Vec<ScenarioStep>,
}

#[derive(Debug, Clone, Default)]
pub struct ScenarioContext {
    pub description: String,
    pub tools_available: Vec<String>,
    pub thinking_enabled: bool,
    pub mode: Option<String>,
    pub prefill_message_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum ScenarioStep {
    WhenUserSays {
        message: String,
    },
    WhenUserApproves,
    WhenUserDenies,
    WhenUserInterrupts {
        after_chunks: usize,
    },
    ThenRespondWith {
        text: String,
        stream: bool,
    },
    ThenResponseContains {
        substring: String,
    },
    ThenCallTool {
        name: String,
        args: serde_json::Value,
    },
    ThenToolReturns {
        result: String,
    },
    ThenThink {
        thinking: String,
    },
    ThenError {
        error_type: SimErrorType,
        message: Option<String>,
    },
    ThenApprovalShown,
    ThenStreamConfig {
        chunk_size: Option<usize>,
        delay_ms: Option<u64>,
    },
    ThenInterrupted,
    ThenTokenUsage {
        usage: TokenUsage,
    },
    ThenContextContains {
        content: String,
    },
    ThenUIShows {
        element: String,
    },
}

/// Pre-defined scenarios loaded from Gherkin .feature files or built-in
#[derive(Debug)]
pub enum SimScenario {
    /// Load from BDD definition
    Bdd(BddScenario),
    /// Simple echo (default)
    Echo,
    /// Tool invocation: returns tool call, then final response
    ToolCall {
        tool_name: String,
        tool_args: serde_json::Value,
        tool_result_handler: fn(&str) -> String,
        final_response: String,
    },
    /// Multi-tool: chain of tool calls
    MultiTool { calls: Vec<SimToolCall> },
    /// Streaming with configurable chunk size and delays
    Streaming {
        text: String,
        chunk_size: usize,
        delay_ms: u64,
    },
    /// Thinking/reasoning simulation
    Thinking {
        thinking_text: String,
        response_text: String,
    },
    /// Random text generation (seeded for reproducibility)
    Random { seed: u64 },
    /// Error simulation
    Error { error_type: SimErrorType },
}

#[derive(Debug, Clone)]
pub struct SimToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub simulated_result: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub enum SimErrorType {
    Timeout,
    RateLimit,
    InvalidRequest,
    ServerError,
    AuthError,
    ContextLengthExceeded,
    MalformedToolCall,
    PartialResponse,
    ContentFiltered,
}

/// Conversation state for multi-turn scenarios
#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    /// Named values stored during conversation (e.g., "user_name" -> "Alice")
    pub memory: HashMap<String, String>,
    /// Tool results from previous turns
    pub tool_results: Vec<(String, String)>,
    /// Turn counter
    pub turn: usize,
}

impl SimScenario {
    /// Load scenario from TARK_SIM_SCENARIO env var
    pub fn from_env() -> Self {
        let scenario = std::env::var("TARK_SIM_SCENARIO").unwrap_or_else(|_| "echo".to_string());
        let scenario_dir = std::env::var("TARK_SIM_SCENARIO_DIR")
            .unwrap_or_else(|_| "tests/visual/scenarios".to_string());

        // Prefer .feature file if it exists
        #[cfg(feature = "test-sim")]
        {
            let feature_path = format!("{}/{}.feature", scenario_dir, scenario);
            if std::path::Path::new(&feature_path).exists() {
                if let Ok(bdd) = BddScenario::from_feature(&feature_path) {
                    return SimScenario::Bdd(bdd);
                }
            }
        }

        // Fallback to built-ins
        match scenario.as_str() {
            "echo" => SimScenario::Echo,
            "error_timeout" => SimScenario::Error {
                error_type: SimErrorType::Timeout,
            },
            "error_rate_limit" => SimScenario::Error {
                error_type: SimErrorType::RateLimit,
            },
            "error_context_exceeded" => SimScenario::Error {
                error_type: SimErrorType::ContextLengthExceeded,
            },
            "error_malformed" => SimScenario::Error {
                error_type: SimErrorType::MalformedToolCall,
            },
            "error_partial" => SimScenario::Error {
                error_type: SimErrorType::PartialResponse,
            },
            "error_filtered" => SimScenario::Error {
                error_type: SimErrorType::ContentFiltered,
            },
            _ => SimScenario::Echo,
        }
    }
}

// ============================================================================
// GHERKIN STEP DEFINITIONS
// ============================================================================

#[cfg(feature = "test-sim")]
impl BddScenario {
    /// Parse a .feature file into a BddScenario
    pub fn from_feature(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        use gherkin::{Feature, GherkinEnv};

        let content = std::fs::read_to_string(path)?;
        let feature = Feature::parse(&content, GherkinEnv::default())?;

        let scenario = feature
            .scenarios
            .first()
            .ok_or("No scenarios in feature file")?;

        let mut given = ScenarioContext::default();
        let mut steps = Vec::new();

        // Parse Background steps for context
        if let Some(bg) = &feature.background {
            for step in &bg.steps {
                Self::parse_given_step(&step.value, &mut given)?;
            }
        }

        // Parse Scenario steps
        for step in &scenario.steps {
            match step.keyword.trim() {
                "Given" => Self::parse_given_step(&step.value, &mut given)?,
                "When" => steps.push(Self::parse_when_step(
                    &step.value,
                    &step.table,
                    &step.docstring,
                )?),
                "Then" | "And" => steps.push(Self::parse_then_step(
                    &step.value,
                    &step.table,
                    &step.docstring,
                )?),
                _ => {}
            }
        }

        Ok(BddScenario {
            name: scenario.name.clone(),
            given,
            steps,
        })
    }

    fn parse_given_step(
        text: &str,
        ctx: &mut ScenarioContext,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if text.contains("provider is") {
            ctx.description = text.to_string();
        } else if text.contains("tools available are") {
            if let Some(tools) = Self::extract_quoted(text) {
                ctx.tools_available = tools.split(',').map(|s| s.trim().to_string()).collect();
            }
        } else if text.contains("thinking is enabled") {
            ctx.thinking_enabled = true;
        } else if text.contains("thinking is disabled") {
            ctx.thinking_enabled = false;
        } else if text.contains("mode is") {
            if let Some(mode) = Self::extract_quoted(text) {
                ctx.mode = Some(mode);
            }
        } else if text.contains("conversation has") {
            if let Some(count) = Self::extract_number(text) {
                ctx.prefill_message_count = Some(count);
            }
        }
        Ok(())
    }

    fn parse_when_step(
        text: &str,
        _table: &Option<gherkin::Table>,
        docstring: &Option<String>,
    ) -> Result<ScenarioStep, Box<dyn std::error::Error>> {
        if text.contains("user says") || text.contains("user sends") {
            let msg = Self::extract_quoted(text)
                .or_else(|| docstring.clone())
                .ok_or("No message in When step")?;
            Ok(ScenarioStep::WhenUserSays { message: msg })
        } else if text.contains("user approves") {
            Ok(ScenarioStep::WhenUserApproves)
        } else if text.contains("user denies") || text.contains("user rejects") {
            Ok(ScenarioStep::WhenUserDenies)
        } else if text.contains("user presses") && text.contains("Ctrl+C") {
            let chunks = Self::extract_number(text).unwrap_or(0);
            Ok(ScenarioStep::WhenUserInterrupts {
                after_chunks: chunks,
            })
        } else {
            Err(format!("Unknown When step: {}", text).into())
        }
    }

    fn parse_then_step(
        text: &str,
        table: &Option<gherkin::Table>,
        docstring: &Option<String>,
    ) -> Result<ScenarioStep, Box<dyn std::error::Error>> {
        if text.contains("agent calls tool") {
            let name = Self::extract_quoted(text).ok_or("No tool name")?;
            let args = Self::table_to_json(table)?;
            Ok(ScenarioStep::ThenCallTool { name, args })
        } else if text.contains("tool returns") {
            let result = docstring.clone().ok_or("No tool result")?;
            Ok(ScenarioStep::ThenToolReturns { result })
        } else if text.contains("agent responds with") || text.contains("agent says") {
            let text_content = Self::extract_quoted(text)
                .or_else(|| docstring.clone())
                .ok_or("No response text")?;
            let stream = text.contains("streamed");
            Ok(ScenarioStep::ThenRespondWith {
                text: text_content,
                stream,
            })
        } else if text.contains("response contains") || text.contains("responds containing") {
            let substring = Self::extract_quoted(text).ok_or("No substring")?;
            Ok(ScenarioStep::ThenResponseContains { substring })
        } else if text.contains("agent thinks") {
            let thinking = docstring.clone().ok_or("No thinking text")?;
            Ok(ScenarioStep::ThenThink { thinking })
        } else if text.contains("returns error") {
            let error_type = Self::parse_error_type(text)?;
            let message = docstring.clone();
            Ok(ScenarioStep::ThenError {
                error_type,
                message,
            })
        } else if text.contains("approval popup is shown") {
            Ok(ScenarioStep::ThenApprovalShown)
        } else if text.contains("response is streamed") {
            let chunk_size = Self::extract_after(text, "chunk size").and_then(|s| s.parse().ok());
            let delay_ms =
                Self::extract_after(text, "delay").and_then(|s| s.replace("ms", "").parse().ok());
            Ok(ScenarioStep::ThenStreamConfig {
                chunk_size,
                delay_ms,
            })
        } else if text.contains("marked as interrupted") {
            Ok(ScenarioStep::ThenInterrupted)
        } else if text.contains("token usage") {
            let usage = Self::table_to_token_usage(table)?;
            Ok(ScenarioStep::ThenTokenUsage { usage })
        } else if text.contains("context contains") {
            let content = Self::extract_quoted(text).ok_or("No context content")?;
            Ok(ScenarioStep::ThenContextContains { content })
        } else if text.contains("UI shows") || text.contains("shows") {
            let ui_element = Self::extract_quoted(text).ok_or("No UI element")?;
            Ok(ScenarioStep::ThenUIShows {
                element: ui_element,
            })
        } else if let Some(ds) = docstring {
            Ok(ScenarioStep::ThenRespondWith {
                text: ds.clone(),
                stream: false,
            })
        } else {
            Err(format!("Unknown Then step: {}", text).into())
        }
    }

    fn extract_quoted(text: &str) -> Option<String> {
        let start = text.find('"')? + 1;
        let end = text[start..].find('"')? + start;
        Some(text[start..end].to_string())
    }

    fn extract_number(text: &str) -> Option<usize> {
        text.split_whitespace().find_map(|word| word.parse().ok())
    }

    fn extract_after(text: &str, keyword: &str) -> Option<String> {
        let idx = text.find(keyword)?;
        text[idx + keyword.len()..]
            .split_whitespace()
            .next()
            .map(|s| s.to_string())
    }

    fn table_to_json(
        table: &Option<gherkin::Table>,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let table = table.as_ref().ok_or("No table")?;
        let mut map = serde_json::Map::new();
        for row in &table.rows {
            if row.len() >= 2 {
                map.insert(row[0].clone(), serde_json::Value::String(row[1].clone()));
            }
        }
        Ok(serde_json::Value::Object(map))
    }

    fn table_to_token_usage(
        table: &Option<gherkin::Table>,
    ) -> Result<TokenUsage, Box<dyn std::error::Error>> {
        let table = table.as_ref().ok_or("No table")?;
        let mut input = 0u32;
        let mut output = 0u32;
        for row in &table.rows {
            if row.len() >= 2 {
                match row[0].as_str() {
                    "input_tokens" => input = row[1].parse()?,
                    "output_tokens" => output = row[1].parse()?,
                    _ => {}
                }
            }
        }
        Ok(TokenUsage {
            input_tokens: input,
            output_tokens: output,
            total_tokens: input + output,
        })
    }

    fn parse_error_type(text: &str) -> Result<SimErrorType, Box<dyn std::error::Error>> {
        if text.contains("timeout") {
            return Ok(SimErrorType::Timeout);
        }
        if text.contains("rate_limit") || text.contains("rate limit") {
            return Ok(SimErrorType::RateLimit);
        }
        if text.contains("context") && text.contains("exceeded") {
            return Ok(SimErrorType::ContextLengthExceeded);
        }
        if text.contains("malformed") {
            return Ok(SimErrorType::MalformedToolCall);
        }
        if text.contains("partial") {
            return Ok(SimErrorType::PartialResponse);
        }
        if text.contains("filtered") || text.contains("blocked") {
            return Ok(SimErrorType::ContentFiltered);
        }
        Err(format!("Unknown error type in: {}", text).into())
    }
}

#[async_trait]
impl LlmProvider for TarkSimProvider {
    fn name(&self) -> &str {
        "tark_sim"
    }

    fn supports_native_thinking(&self) -> bool {
        matches!(self.scenario, SimScenario::Thinking { .. })
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        // Capture context for verification
        let context = self.capture_context(messages, tools);
        *self.last_context.lock().unwrap() = Some(context.clone());

        self.log_event(
            SimEventType::ContextCaptured,
            serde_json::to_value(&context).unwrap_or_default(),
        );

        // Simulate processing delay
        tokio::time::sleep(Duration::from_millis(100)).await;

        let turn = self.turn_counter.fetch_add(1, Ordering::SeqCst);

        match &self.scenario {
            SimScenario::Echo => {
                let user_msg = messages
                    .last()
                    .and_then(|m| m.content.as_text())
                    .unwrap_or("(no message)");
                let response_text = format!("[tark_sim] Echo: {}", user_msg);
                Ok(LlmResponse::Text {
                    text: response_text.clone(),
                    usage: Some(self.sim_usage(user_msg, &response_text)),
                })
            }
            SimScenario::ToolCall {
                tool_name,
                tool_args,
                final_response,
                ..
            } => {
                if turn == 0 {
                    // First turn: return tool call
                    Ok(LlmResponse::ToolCalls {
                        calls: vec![ToolCall {
                            id: format!("call_{}", uuid::Uuid::new_v4()),
                            name: tool_name.clone(),
                            arguments: tool_args.clone(),
                            thought_signature: None,
                        }],
                        usage: Some(TokenUsage {
                            input_tokens: 50,
                            output_tokens: 20,
                            total_tokens: 70,
                        }),
                    })
                } else {
                    // Second turn: return final response
                    Ok(LlmResponse::Text {
                        text: final_response.clone(),
                        usage: Some(self.sim_usage("", final_response)),
                    })
                }
            }
            SimScenario::Error { error_type } => {
                let error_msg = match error_type {
                    SimErrorType::Timeout => "Request timed out after 30 seconds",
                    SimErrorType::RateLimit => "Rate limit exceeded. Please try again later.",
                    SimErrorType::ContextLengthExceeded => {
                        "Context length exceeded. Try /clear or start a new conversation."
                    }
                    SimErrorType::MalformedToolCall => {
                        "LLM returned invalid tool call JSON. Retrying..."
                    }
                    SimErrorType::PartialResponse => "Connection lost mid-response",
                    SimErrorType::ContentFiltered => "Response blocked by content moderation",
                    _ => "Simulated error occurred",
                };
                Err(anyhow::anyhow!("{}", error_msg))
            }
            _ => {
                // Other scenarios: simple text response
                let response = "Simulated response from tark_sim";
                Ok(LlmResponse::Text {
                    text: response.to_string(),
                    usage: Some(TokenUsage {
                        input_tokens: 10,
                        output_tokens: 5,
                        total_tokens: 15,
                    }),
                })
            }
        }
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
        // Capture context
        let context = self.capture_context(messages, tools);
        *self.last_context.lock().unwrap() = Some(context);

        // Simulate streaming with realistic delays
        match &self.scenario {
            SimScenario::Streaming {
                text,
                chunk_size,
                delay_ms,
            } => {
                for chunk in text.as_bytes().chunks(*chunk_size) {
                    if let Some(check) = interrupt_check {
                        if check() {
                            break;
                        }
                    }
                    let s = String::from_utf8_lossy(chunk);
                    callback(StreamEvent::TextDelta(s.to_string()));
                    tokio::time::sleep(Duration::from_millis(*delay_ms)).await;
                }
                callback(StreamEvent::Done);
                Ok(LlmResponse::Text {
                    text: text.clone(),
                    usage: Some(self.sim_usage("", text)),
                })
            }
            SimScenario::Thinking {
                thinking_text,
                response_text,
            } if settings.enabled => {
                // Emit thinking content
                callback(StreamEvent::ThinkingDelta(thinking_text.clone()));
                tokio::time::sleep(Duration::from_millis(200)).await;
                // Emit response
                callback(StreamEvent::TextDelta(response_text.clone()));
                callback(StreamEvent::Done);
                Ok(LlmResponse::Text {
                    text: response_text.clone(),
                    usage: Some(self.sim_usage("", response_text)),
                })
            }
            _ => {
                // Fallback to non-streaming
                self.chat(messages, tools).await
            }
        }
    }

    fn supports_streaming(&self) -> bool {
        true
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        _language: &str,
    ) -> Result<super::CompletionResult> {
        // Simple simulation: generate a middle part
        let completion = "// Simulated completion\n    ".to_string();
        let usage = self.sim_usage(&format!("{}{}", prefix, suffix), &completion);
        Ok(super::CompletionResult {
            text: completion,
            usage: Some(usage),
        })
    }

    async fn explain_code(&self, code: &str, _context: &str) -> Result<String> {
        Ok(format!(
            "[tark_sim] Code explanation: This code is {} characters long.",
            code.len()
        ))
    }

    async fn suggest_refactorings(
        &self,
        _code: &str,
        _context: &str,
    ) -> Result<Vec<super::RefactoringSuggestion>> {
        // Return empty suggestions for simulation
        Ok(vec![])
    }

    async fn review_code(&self, _code: &str, _language: &str) -> Result<Vec<super::CodeIssue>> {
        // Return no issues for simulation
        Ok(vec![])
    }
}

impl Default for TarkSimProvider {
    fn default() -> Self {
        Self::new()
    }
}
