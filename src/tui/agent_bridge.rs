//! Agent bridge for TUI integration
//!
//! Provides the connection between the TUI and the chat agent backend,
//! handling message sending, response streaming, and tool call display.

// Allow dead code for intentionally unused API methods that are part of the public interface
#![allow(dead_code)]

use crate::agent::{AgentResponse, ChatAgent, ToolCallLog};
use crate::config::Config;
use crate::llm;
use crate::storage::usage::UsageTracker;
use crate::storage::{ChatSession, ModelPreference, TarkStorage};
use crate::tools::{AgentMode as ToolAgentMode, ToolRegistry};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use super::attachments::{AttachmentConfig, AttachmentContent, AttachmentError, MessageAttachment};

/// Events sent from the agent to the TUI
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent started processing
    Started,
    /// Streaming text chunk received
    TextChunk(String),
    /// Thinking content chunk received (Requirements 9.1, 9.3)
    ThinkingChunk(String),
    /// Tool call started
    ToolCallStarted {
        tool: String,
        args: serde_json::Value,
    },
    /// Tool call completed successfully
    ToolCallCompleted {
        tool: String,
        result_preview: String,
    },
    /// Tool call failed with an error (Requirements 7.2)
    ToolCallFailed { tool: String, error: String },
    /// Agent finished with response
    Completed(AgentResponseInfo),
    /// Agent encountered an error
    Error(String),
    /// Agent was interrupted
    Interrupted,
    /// Context was auto-compacted (Requirements 5.6)
    ContextCompacted {
        old_tokens: usize,
        new_tokens: usize,
    },
    /// Context window exceeded - needs compaction (Requirements 7.3)
    ContextWindowExceeded {
        current_tokens: usize,
        max_tokens: usize,
    },
    /// Rate limit hit - retry after delay (Requirements 7.4)
    RateLimited {
        retry_after_secs: u64,
        message: String,
    },
    /// Authentication required (OAuth device flow)
    AuthRequired {
        provider: String,
        verification_url: String,
        user_code: String,
        timeout_secs: u64,
    },
    /// Authentication succeeded
    AuthSuccess { provider: String },
    /// Authentication failed
    AuthFailed { provider: String, error: String },
}

/// Summary info from agent response
#[derive(Debug, Clone)]
pub struct AgentResponseInfo {
    pub text: String,
    pub tool_calls_made: usize,
    pub tool_call_log: Vec<ToolCallLogInfo>,
    pub auto_compacted: bool,
    pub context_usage_percent: usize,
    pub input_tokens: usize,
    pub output_tokens: usize,
}

/// Tool call log info for display
#[derive(Debug, Clone)]
pub struct ToolCallLogInfo {
    pub tool: String,
    pub args: serde_json::Value,
    pub result_preview: String,
}

impl From<&ToolCallLog> for ToolCallLogInfo {
    fn from(log: &ToolCallLog) -> Self {
        Self {
            tool: log.tool.clone(),
            args: log.args.clone(),
            result_preview: log.result_preview.clone(),
        }
    }
}

impl From<AgentResponse> for AgentResponseInfo {
    fn from(response: AgentResponse) -> Self {
        Self {
            text: response.text,
            tool_calls_made: response.tool_calls_made,
            tool_call_log: response.tool_call_log.iter().map(Into::into).collect(),
            auto_compacted: response.auto_compacted,
            context_usage_percent: response.context_usage_percent,
            input_tokens: response
                .usage
                .as_ref()
                .map(|u| u.input_tokens as usize)
                .unwrap_or(0),
            output_tokens: response
                .usage
                .as_ref()
                .map(|u| u.output_tokens as usize)
                .unwrap_or(0),
        }
    }
}

/// Agent mode for the TUI (mirrors tools::AgentMode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Build,
    Plan,
    Review,
}

impl From<AgentMode> for ToolAgentMode {
    fn from(mode: AgentMode) -> Self {
        match mode {
            AgentMode::Build => ToolAgentMode::Build,
            AgentMode::Plan => ToolAgentMode::Plan,
            AgentMode::Review => ToolAgentMode::Review,
        }
    }
}

impl From<ToolAgentMode> for AgentMode {
    fn from(mode: ToolAgentMode) -> Self {
        match mode {
            ToolAgentMode::Build => AgentMode::Build,
            ToolAgentMode::Plan => AgentMode::Plan,
            ToolAgentMode::Review => AgentMode::Review,
        }
    }
}

impl From<&str> for AgentMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "plan" => AgentMode::Plan,
            "review" => AgentMode::Review,
            _ => AgentMode::Build,
        }
    }
}

impl std::fmt::Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentMode::Build => write!(f, "build"),
            AgentMode::Plan => write!(f, "plan"),
            AgentMode::Review => write!(f, "review"),
        }
    }
}

impl AgentMode {
    /// Get the next mode in the cycle (Build → Plan → Review → Build)
    /// Requirements: 13.1
    pub fn next(self) -> Self {
        match self {
            AgentMode::Build => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Review,
            AgentMode::Review => AgentMode::Build,
        }
    }

    /// Get the previous mode in the cycle (Build → Review → Plan → Build)
    /// Requirements: 13.2
    pub fn prev(self) -> Self {
        match self {
            AgentMode::Build => AgentMode::Review,
            AgentMode::Plan => AgentMode::Build,
            AgentMode::Review => AgentMode::Plan,
        }
    }
}

/// Bridge between TUI and chat agent
pub struct AgentBridge {
    /// Chat agent instance
    agent: ChatAgent,
    /// Working directory
    working_dir: PathBuf,
    /// Storage for sessions
    storage: TarkStorage,
    /// Current session
    current_session: ChatSession,
    /// Current provider name
    provider_name: String,
    /// Current model name
    model_name: String,
    /// Current agent mode
    mode: AgentMode,
    /// Interrupt flag
    interrupt_flag: Arc<AtomicBool>,
    /// Whether agent is currently processing
    is_processing: Arc<AtomicBool>,
    /// Config
    config: Config,
    /// Usage tracker for cost/token tracking
    usage_tracker: Option<UsageTracker>,
    /// In-memory usage log for current session (fallback when SQLite fails)
    session_usage: Vec<(String, String, f64)>, // (provider, model, cost)
}

impl AgentBridge {
    /// Create a new agent bridge
    pub fn new(working_dir: PathBuf) -> Result<Self> {
        Self::with_provider(working_dir, None, None)
    }

    /// Create a new agent bridge with optional provider and model overrides
    ///
    /// This allows CLI arguments to override the config defaults before
    /// the provider is created.
    pub fn with_provider(
        working_dir: PathBuf,
        provider_override: Option<String>,
        model_override: Option<String>,
    ) -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let storage = TarkStorage::new(&working_dir)?;

        // Load or create session
        let current_session = storage.load_current_session().unwrap_or_else(|_| {
            let session = ChatSession::new();
            let _ = storage.save_session(&session);
            session
        });

        // Get provider: CLI override > session > config
        let provider_name = if let Some(ref p) = provider_override {
            p.clone()
        } else if !current_session.provider.is_empty() {
            current_session.provider.clone()
        } else {
            config.llm.default_provider.clone()
        };

        // Get model: CLI override > session > config default for provider
        let model_name = if let Some(ref m) = model_override {
            m.clone()
        } else if !current_session.model.is_empty() {
            current_session.model.clone()
        } else {
            // Get default model based on provider
            match provider_name.as_str() {
                "claude" | "anthropic" => config.llm.claude.model.clone(),
                "openai" | "gpt" => config.llm.openai.model.clone(),
                "copilot" | "github" => config.llm.copilot.model.clone(),
                "gemini" | "google" => config.llm.gemini.model.clone(),
                "openrouter" => config.llm.openrouter.model.clone(),
                "ollama" | "local" => config.llm.ollama.model.clone(),
                _ => String::new(),
            }
        };

        // Create LLM provider (silent mode for TUI)
        let provider = llm::create_provider_with_options(&provider_name, true, None)?;
        let provider = Arc::from(provider);

        // Get mode from session
        let mode = AgentMode::from(current_session.mode.as_str());

        // Create tool registry for mode
        let tools =
            ToolRegistry::for_mode(working_dir.clone(), mode.into(), config.tools.shell_enabled);

        // Create agent
        let mut agent = ChatAgent::with_mode(provider, tools, mode.into())
            .with_max_iterations(config.agent.max_iterations);

        // Set thinking configuration
        agent.set_thinking_config(config.thinking.clone());

        // Restore session messages if any
        if !current_session.messages.is_empty() {
            agent.restore_from_session(&current_session);
        }

        // Initialize usage tracker and ensure session exists
        // Usage database is stored in .tark/ directory
        let tark_dir = working_dir.join(".tark");
        let usage_tracker = UsageTracker::new(&tark_dir).ok();

        // Register the current session in the usage tracker so foreign key constraints work
        if let Some(ref tracker) = usage_tracker {
            let host = whoami::fallible::hostname().unwrap_or_else(|_| "unknown".to_string());
            let username = whoami::username();
            // Try to ensure session exists (ignore errors if it already exists)
            let _ = tracker.ensure_session_exists(
                &current_session.id,
                &current_session.name,
                &host,
                &username,
            );
        }

        Ok(Self {
            agent,
            working_dir,
            storage,
            current_session,
            provider_name,
            model_name,
            mode,
            interrupt_flag: Arc::new(AtomicBool::new(false)),
            is_processing: Arc::new(AtomicBool::new(false)),
            config,
            usage_tracker,
            session_usage: Vec::new(),
        })
    }

    /// Get the current provider name
    pub fn provider_name(&self) -> &str {
        &self.provider_name
    }

    /// Get the current model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// List available models for the current provider
    /// Returns a list of (model_id, display_name, description) tuples
    /// Uses models.dev API for dynamic model listing with fallback to hardcoded list
    pub async fn list_available_models(&self) -> Vec<(String, String, String)> {
        // Try to fetch from models.dev first
        let models_db = crate::llm::models_db();
        if let Ok(models) = models_db.list_models(&self.provider_name).await {
            if !models.is_empty() {
                let mut result: Vec<(String, String, String)> = models
                    .into_iter()
                    .map(|m| {
                        let desc = m.capability_summary();
                        (m.id, m.name, desc)
                    })
                    .collect();
                // Sort by model name
                result.sort_by(|a, b| a.1.cmp(&b.1));
                return result;
            }
        }

        // Fallback to provider-specific API or hardcoded list
        match self.provider_name.as_str() {
            "openai" | "gpt" => {
                // Try to fetch from OpenAI API
                if let Ok(provider) = crate::llm::OpenAiProvider::new() {
                    if let Ok(models) = provider.list_models().await {
                        let mut result: Vec<(String, String, String)> = models
                            .into_iter()
                            .map(|m| {
                                let display = format_model_display(&m.id);
                                let desc = format_model_description(&m.id);
                                (m.id, display, desc)
                            })
                            .collect();
                        // Sort by model name, putting newer models first
                        result.sort_by(|a, b| {
                            let a_priority = model_priority(&a.0);
                            let b_priority = model_priority(&b.0);
                            a_priority.cmp(&b_priority).then(a.0.cmp(&b.0))
                        });
                        return result;
                    }
                }
                // Fallback to hardcoded list
                vec![
                    (
                        "gpt-4o".into(),
                        "GPT-4o".into(),
                        "Most capable, multimodal".into(),
                    ),
                    (
                        "gpt-4o-mini".into(),
                        "GPT-4o Mini".into(),
                        "Fast and affordable".into(),
                    ),
                    (
                        "gpt-4-turbo".into(),
                        "GPT-4 Turbo".into(),
                        "High capability, 128k context".into(),
                    ),
                    (
                        "gpt-3.5-turbo".into(),
                        "GPT-3.5 Turbo".into(),
                        "Fast and economical".into(),
                    ),
                ]
            }
            "claude" | "anthropic" => {
                vec![
                    (
                        "claude-sonnet-4-20250514".into(),
                        "Claude Sonnet 4".into(),
                        "Latest, most capable".into(),
                    ),
                    (
                        "claude-3-5-sonnet-20241022".into(),
                        "Claude 3.5 Sonnet".into(),
                        "Best balance of speed and capability".into(),
                    ),
                    (
                        "claude-3-opus-20240229".into(),
                        "Claude 3 Opus".into(),
                        "Most powerful, best for complex tasks".into(),
                    ),
                    (
                        "claude-3-haiku-20240307".into(),
                        "Claude 3 Haiku".into(),
                        "Fastest, most economical".into(),
                    ),
                    (
                        "claude-3-7-sonnet-20250219".into(),
                        "Claude 3.7 Sonnet".into(),
                        "Hybrid reasoning model".into(),
                    ),
                    (
                        "claude-3-5-haiku-20241022".into(),
                        "Claude 3.5 Haiku".into(),
                        "Fast and affordable".into(),
                    ),
                ]
            }
            "copilot" | "github" => {
                // Try to get available models from provider based on subscription
                if let Ok(provider) = crate::llm::CopilotProvider::new() {
                    let models = provider.available_models().await;
                    models
                        .into_iter()
                        .map(|model_id| {
                            let display = format_model_display(&model_id);
                            let desc = format_model_description(&model_id);
                            (model_id, display, desc)
                        })
                        .collect()
                } else {
                    // Fallback if provider creation fails
                    vec![(
                        "gpt-4o".into(),
                        "GPT-4o".into(),
                        "Most capable model via Copilot".into(),
                    )]
                }
            }
            "gemini" | "google" => {
                vec![
                    (
                        "gemini-2.0-flash-exp".into(),
                        "Gemini 2.0 Flash".into(),
                        "Fast and efficient (default)".into(),
                    ),
                    (
                        "gemini-2.0-flash-thinking-exp".into(),
                        "Gemini 2.0 Flash Thinking".into(),
                        "With extended thinking".into(),
                    ),
                    (
                        "gemini-1.5-pro".into(),
                        "Gemini 1.5 Pro".into(),
                        "Larger, more capable".into(),
                    ),
                    (
                        "gemini-1.5-flash".into(),
                        "Gemini 1.5 Flash".into(),
                        "Fast and lightweight".into(),
                    ),
                ]
            }
            "openrouter" => {
                vec![
                    (
                        "anthropic/claude-sonnet-4".into(),
                        "Claude Sonnet 4".into(),
                        "Latest Claude via OpenRouter".into(),
                    ),
                    (
                        "deepseek/deepseek-chat".into(),
                        "DeepSeek Chat".into(),
                        "Very affordable, great quality".into(),
                    ),
                    (
                        "google/gemini-2.0-flash-exp:free".into(),
                        "Gemini 2.0 (Free)".into(),
                        "Free via OpenRouter".into(),
                    ),
                    (
                        "meta-llama/llama-3.1-8b-instruct:free".into(),
                        "Llama 3.1 8B (Free)".into(),
                        "Free open model".into(),
                    ),
                    (
                        "qwen/qwen-2.5-72b-instruct".into(),
                        "Qwen 2.5 72B".into(),
                        "Excellent for coding".into(),
                    ),
                ]
            }
            "ollama" | "local" => {
                // Fallback to hardcoded list (Ollama API doesn't have list_models)
                vec![
                    (
                        "llama3.2".into(),
                        "Llama 3.2".into(),
                        "Meta's latest open model".into(),
                    ),
                    (
                        "qwen2.5-coder".into(),
                        "Qwen 2.5 Coder".into(),
                        "Excellent for coding tasks".into(),
                    ),
                    (
                        "codellama".into(),
                        "Code Llama".into(),
                        "Optimized for code generation".into(),
                    ),
                    (
                        "deepseek-coder-v2".into(),
                        "DeepSeek Coder V2".into(),
                        "Advanced coding model".into(),
                    ),
                    (
                        "mistral".into(),
                        "Mistral".into(),
                        "Fast and capable".into(),
                    ),
                    (
                        "phi3".into(),
                        "Phi-3".into(),
                        "Microsoft's compact model".into(),
                    ),
                ]
            }
            _ => vec![("default".into(), "Default Model".into(), "".into())],
        }
    }

    /// Get the current agent mode
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Get the current session
    pub fn current_session(&self) -> &ChatSession {
        &self.current_session
    }

    /// Get the current session ID
    pub fn session_id(&self) -> &str {
        &self.current_session.id
    }

    /// Get the current session name
    pub fn session_name(&self) -> &str {
        &self.current_session.name
    }

    /// Check if agent is currently processing
    pub fn is_processing(&self) -> bool {
        self.is_processing.load(Ordering::SeqCst)
    }

    /// Get the working directory
    pub fn working_dir(&self) -> &std::path::Path {
        &self.working_dir
    }

    /// Get context usage percentage from the actual conversation context
    pub fn context_usage_percent(&self) -> usize {
        self.agent.context_usage_percentage()
    }

    /// Get estimated tokens in current context (actual conversation size)
    pub fn context_tokens(&self) -> usize {
        self.agent.estimated_context_tokens()
    }

    /// Get max context tokens for the current model
    pub fn max_context_tokens(&self) -> usize {
        self.agent.max_context_tokens()
    }

    /// Update max context tokens (when switching models)
    pub fn set_max_context_tokens(&mut self, max: usize) {
        self.agent.set_max_context_tokens(max);
    }

    /// Set the thinking/reasoning level by name
    ///
    /// Level names are defined in config.thinking.levels
    /// "off" disables thinking
    pub fn set_think_level(&mut self, level_name: String) {
        self.agent.set_think_level(level_name);
    }

    /// Get the current thinking level name
    pub fn think_level(&self) -> &str {
        self.agent.think_level()
    }

    /// Check if thinking is enabled (any level other than "off")
    pub fn is_thinking_enabled(&self) -> bool {
        self.agent.is_thinking_enabled()
    }

    /// Get the thinking configuration
    pub fn thinking_config(&self) -> &crate::config::ThinkingConfig {
        &self.config.thinking
    }

    /// Get total cost from current session
    ///
    /// Queries the usage database for accurate cost tracking
    pub fn total_cost(&self) -> f64 {
        // First try session cost from storage (most accurate, includes all messages)
        if let Some(session_stats) = self.get_session_cost_from_storage() {
            return session_stats;
        }
        // Fallback to cached session cost
        self.current_session.total_cost
    }

    /// Get session cost from usage storage
    fn get_session_cost_from_storage(&self) -> Option<f64> {
        // Query usage tracker if available
        if let Some(ref tracker) = self.usage_tracker {
            if let Ok(logs) = tracker.get_session_logs(&self.current_session.id) {
                if !logs.is_empty() {
                    let total: f64 = logs.iter().fold(0.0, |acc, l| acc + l.cost_usd);
                    return Some(total);
                }
            }
        }
        None
    }

    /// Get cost breakdown by model and provider for current session
    pub fn get_cost_breakdown(&self) -> Vec<crate::tui::widgets::CostBreakdownEntry> {
        use std::collections::HashMap;

        // First try SQLite tracker
        if let Some(ref tracker) = self.usage_tracker {
            if let Ok(logs) = tracker.get_session_logs(&self.current_session.id) {
                if !logs.is_empty() {
                    // Aggregate costs by (provider, model)
                    let mut breakdown: HashMap<(String, String), f64> = HashMap::new();

                    for log in logs {
                        let key = (log.provider.clone(), log.model.clone());
                        *breakdown.entry(key).or_insert(0.0) += log.cost_usd;
                    }

                    // Convert to Vec and sort by cost (highest first)
                    let mut entries: Vec<crate::tui::widgets::CostBreakdownEntry> = breakdown
                        .into_iter()
                        .map(
                            |((provider, model), cost)| crate::tui::widgets::CostBreakdownEntry {
                                provider,
                                model,
                                cost,
                            },
                        )
                        .collect();

                    entries.sort_by(|a, b| {
                        b.cost
                            .partial_cmp(&a.cost)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    return entries;
                }
            }
        }

        // Fallback: use in-memory session usage
        if !self.session_usage.is_empty() {
            let mut breakdown: HashMap<(String, String), f64> = HashMap::new();
            for (provider, model, cost) in &self.session_usage {
                let key = (provider.clone(), model.clone());
                *breakdown.entry(key).or_insert(0.0) += cost;
            }

            let mut entries: Vec<crate::tui::widgets::CostBreakdownEntry> = breakdown
                .into_iter()
                .map(
                    |((provider, model), cost)| crate::tui::widgets::CostBreakdownEntry {
                        provider,
                        model,
                        cost,
                    },
                )
                .collect();

            entries.sort_by(|a, b| {
                b.cost
                    .partial_cmp(&a.cost)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });

            return entries;
        }

        // Return empty if no cost data at all
        Vec::new()
    }

    /// Get total tokens from current session (cumulative input/output)
    pub fn total_tokens(&self) -> (usize, usize) {
        (
            self.current_session.input_tokens,
            self.current_session.output_tokens,
        )
    }

    /// Interrupt the current agent operation
    pub fn interrupt(&self) {
        self.interrupt_flag.store(true, Ordering::SeqCst);
    }

    /// Clear the interrupt flag
    fn clear_interrupt(&self) {
        self.interrupt_flag.store(false, Ordering::SeqCst);
    }

    /// Send a message to the agent and get a response
    pub async fn send_message(&mut self, message: &str) -> Result<AgentResponseInfo> {
        // Set processing flag
        self.is_processing.store(true, Ordering::SeqCst);
        self.clear_interrupt();

        // Set session name from first message if not set
        if self.current_session.name.is_empty() {
            self.current_session.set_name_from_prompt(message);
        }

        // Add user message to session
        self.current_session.add_message("user", message);

        // Create interrupt check closure
        let interrupt_flag = self.interrupt_flag.clone();
        let interrupt_check = move || interrupt_flag.load(Ordering::SeqCst);

        // Send to agent
        let result = self
            .agent
            .chat_with_interrupt(message, interrupt_check)
            .await;

        // Clear processing flag
        self.is_processing.store(false, Ordering::SeqCst);

        match result {
            Ok(response) => {
                // Add assistant response to session
                self.current_session
                    .add_message("assistant", &response.text);

                // Update token counts and log usage
                if let Some(usage) = &response.usage {
                    self.current_session.input_tokens += usage.input_tokens as usize;
                    self.current_session.output_tokens += usage.output_tokens as usize;

                    // Calculate cost and log usage
                    let cost = tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current().block_on(async {
                            crate::llm::models_db()
                                .calculate_cost(
                                    &self.provider_name,
                                    &self.model_name,
                                    usage.input_tokens,
                                    usage.output_tokens,
                                )
                                .await
                        })
                    });
                    self.current_session.total_cost += cost;

                    // Log usage to tracker for cost breakdown display
                    if let Some(ref tracker) = self.usage_tracker {
                        let _ = tracker.log_usage(crate::storage::usage::UsageLog {
                            session_id: self.current_session.id.clone(),
                            provider: self.provider_name.clone(),
                            model: self.model_name.clone(),
                            mode: format!("{:?}", self.agent.mode()),
                            input_tokens: usage.input_tokens,
                            output_tokens: usage.output_tokens,
                            cost_usd: cost,
                            request_type: "chat".to_string(),
                            estimated: false,
                        });
                    }
                    // Also store in-memory for fallback
                    self.session_usage.push((
                        self.provider_name.clone(),
                        self.model_name.clone(),
                        cost,
                    ));
                }

                // Save session
                let _ = self.storage.save_session(&self.current_session);

                Ok(response.into())
            }
            Err(e) => {
                // Save session even on error
                let _ = self.storage.save_session(&self.current_session);
                Err(e)
            }
        }
    }

    /// Send a message with real-time streaming
    ///
    /// Uses the LLM's native streaming API to emit TextChunk and ThinkingChunk
    /// events as they arrive, enabling real-time display in the TUI.
    pub async fn send_message_streaming(
        &mut self,
        message: &str,
        event_tx: mpsc::Sender<AgentEvent>,
    ) -> Result<()> {
        // Set processing flag
        self.is_processing.store(true, Ordering::SeqCst);
        self.clear_interrupt();

        // Notify started
        let _ = event_tx.send(AgentEvent::Started).await;

        // Set session name from first message if not set
        if self.current_session.name.is_empty() {
            self.current_session.set_name_from_prompt(message);
        }

        // Add user message to session
        self.current_session.add_message("user", message);

        // Create interrupt check closure
        let interrupt_flag = self.interrupt_flag.clone();
        let interrupt_check = move || interrupt_flag.load(Ordering::SeqCst);

        // Clone event_tx for use in callbacks
        let text_tx = event_tx.clone();
        let thinking_tx = event_tx.clone();
        let tool_tx = event_tx.clone();

        // Create streaming callbacks
        let on_text = move |chunk: String| {
            let _ = text_tx.try_send(AgentEvent::TextChunk(chunk));
        };

        let on_thinking = move |chunk: String| {
            let _ = thinking_tx.try_send(AgentEvent::ThinkingChunk(chunk));
        };

        let on_tool_call = move |name: String, args: String| {
            let args_value: serde_json::Value =
                serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
            let _ = tool_tx.try_send(AgentEvent::ToolCallStarted {
                tool: name,
                args: args_value,
            });
        };

        // Send to agent with streaming callbacks
        let result = self
            .agent
            .chat_streaming(message, interrupt_check, on_text, on_thinking, on_tool_call)
            .await;

        // Clear processing flag
        self.is_processing.store(false, Ordering::SeqCst);

        match result {
            Ok(response) => {
                // Check if interrupted
                if response.text.contains("interrupted") {
                    let _ = event_tx.send(AgentEvent::Interrupted).await;
                } else {
                    // Add assistant response to session
                    self.current_session
                        .add_message("assistant", &response.text);

                    // Update token counts and calculate cost
                    if let Some(usage) = &response.usage {
                        self.current_session.input_tokens += usage.input_tokens as usize;
                        self.current_session.output_tokens += usage.output_tokens as usize;

                        // Calculate cost using models.dev database
                        let cost = crate::llm::models_db()
                            .calculate_cost(
                                &self.provider_name,
                                &self.model_name,
                                usage.input_tokens,
                                usage.output_tokens,
                            )
                            .await;
                        self.current_session.total_cost += cost;

                        // Log usage to tracker for cost breakdown display
                        if let Some(ref tracker) = self.usage_tracker {
                            let _ = tracker.log_usage(crate::storage::usage::UsageLog {
                                session_id: self.current_session.id.clone(),
                                provider: self.provider_name.clone(),
                                model: self.model_name.clone(),
                                mode: format!("{:?}", self.agent.mode()),
                                input_tokens: usage.input_tokens,
                                output_tokens: usage.output_tokens,
                                cost_usd: cost,
                                request_type: "chat".to_string(),
                                estimated: false,
                            });
                        }
                        // Also store in-memory for fallback
                        self.session_usage.push((
                            self.provider_name.clone(),
                            self.model_name.clone(),
                            cost,
                        ));
                    }

                    // Send completed event
                    let _ = event_tx.send(AgentEvent::Completed(response.into())).await;
                }

                // Save session
                let _ = self.storage.save_session(&self.current_session);

                Ok(())
            }
            Err(e) => {
                // Save session even on error
                let _ = self.storage.save_session(&self.current_session);

                let _ = event_tx.send(AgentEvent::Error(e.to_string())).await;
                Err(e)
            }
        }
    }

    /// Send a message with attachments and event streaming
    ///
    /// This method formats attachments appropriately for the LLM:
    /// - Text/code files are wrapped in code blocks
    /// - Images are encoded as base64 for vision-capable models
    ///
    /// Requirements: 10.1, 10.2, 10.5
    pub async fn send_message_with_attachments(
        &mut self,
        message: &str,
        attachments: Vec<MessageAttachment>,
        event_tx: mpsc::Sender<AgentEvent>,
        config: &AttachmentConfig,
    ) -> Result<()> {
        // Validate attachments (Requirements 10.5)
        self.validate_attachments(&attachments, config)?;

        // Validate image attachments for vision support (Requirements 10.3, 10.4)
        self.validate_image_attachments(&attachments)
            .map_err(|e| anyhow::anyhow!("{}", e))?;

        // Format message with attachments (Requirements 10.1, 10.2)
        let formatted_message = self.format_message_with_attachments(message, &attachments);

        // Send the formatted message
        self.send_message_streaming(&formatted_message, event_tx)
            .await
    }

    /// Validate attachments against size limits
    ///
    /// Requirements: 10.5
    fn validate_attachments(
        &self,
        attachments: &[MessageAttachment],
        config: &AttachmentConfig,
    ) -> Result<()> {
        for attachment in attachments {
            let size = attachment.content.size();
            if size > config.max_attachment_size {
                return Err(anyhow::anyhow!(
                    "Attachment '{}' exceeds size limit ({} > {})",
                    attachment.filename,
                    super::attachments::format_size(size),
                    super::attachments::format_size(config.max_attachment_size)
                ));
            }
        }

        if attachments.len() > config.max_attachments {
            return Err(anyhow::anyhow!(
                "Too many attachments ({} > {})",
                attachments.len(),
                config.max_attachments
            ));
        }

        Ok(())
    }

    /// Format a message with attachments for the LLM
    ///
    /// Requirements: 10.1, 10.2
    fn format_message_with_attachments(
        &self,
        message: &str,
        attachments: &[MessageAttachment],
    ) -> String {
        if attachments.is_empty() {
            return message.to_string();
        }

        let mut formatted = String::new();

        // Add the user's message first
        formatted.push_str(message);

        // Add attachments
        for attachment in attachments {
            formatted.push_str("\n\n");
            formatted.push_str(&self.format_single_attachment(attachment));
        }

        formatted
    }

    /// Format a single attachment for the LLM
    fn format_single_attachment(&self, attachment: &MessageAttachment) -> String {
        let filename = &attachment.filename;

        match &attachment.content {
            AttachmentContent::Text(text) => {
                // Text content - check if it's already wrapped in code blocks
                if text.starts_with("```") {
                    // Already formatted with code blocks
                    format!("**Attached file: {}**\n{}", filename, text)
                } else {
                    // Wrap in code block with filename
                    format!("**Attached file: {}**\n```\n{}\n```", filename, text)
                }
            }
            AttachmentContent::Base64(base64_data) => {
                // Image attachment - format for vision models (Requirements 10.3)
                format!(
                    "**Attached image: {}**\n[Image data: {} encoded as base64, {} bytes]",
                    filename,
                    attachment.mime_type,
                    base64_data.len()
                )
            }
            AttachmentContent::Path(path) => {
                // File path - read content if possible
                if let Ok(content) = std::fs::read_to_string(path) {
                    format!("**Attached file: {}**\n```\n{}\n```", filename, content)
                } else {
                    format!(
                        "**Attached file: {}**\n[File at path: {}]",
                        filename,
                        path.display()
                    )
                }
            }
        }
    }

    /// Check if the current model supports vision (image attachments)
    ///
    /// Requirements: 10.3, 10.4
    pub fn supports_vision(&self) -> bool {
        // Check if the current model supports vision based on known vision-capable models
        let model = self.model_name.to_lowercase();

        // OpenAI vision models
        if model.contains("gpt-4") && (model.contains("vision") || model.contains("turbo")) {
            return true;
        }
        if model.contains("gpt-4o") {
            return true;
        }

        // Claude vision models (Claude 3+)
        if model.contains("claude-3") {
            return true;
        }

        // Ollama vision models
        if model.contains("llava") || model.contains("bakllava") {
            return true;
        }

        false
    }

    /// Validate image attachments for vision support
    ///
    /// Requirements: 10.3, 10.4
    pub fn validate_image_attachments(
        &self,
        attachments: &[MessageAttachment],
    ) -> Result<(), AttachmentError> {
        let has_images = attachments.iter().any(|a| a.is_image());

        if has_images && !self.supports_vision() {
            return Err(AttachmentError::UnsupportedFileType(format!(
                "Model '{}' does not support image attachments. Use a vision-capable model like GPT-4o, Claude 3, or LLaVA.",
                self.model_name
            )));
        }

        Ok(())
    }

    /// Change the agent mode
    ///
    /// Returns `Ok(true)` if the mode was switched successfully and has a saved preference.
    /// Returns `Ok(false)` if the mode was switched but has no saved preference (selection needed).
    /// Requirements: 2.2, 2.6
    pub fn set_mode(&mut self, mode: AgentMode) -> Result<bool> {
        self.mode = mode;

        // Update session
        self.current_session.mode = mode.to_string();

        // Create new tool registry for mode
        let tools = ToolRegistry::for_mode(
            self.working_dir.clone(),
            mode.into(),
            self.config.tools.shell_enabled,
        );

        // Update agent mode
        self.agent.update_mode(tools, mode.into());

        // Check if mode has a saved preference and load it
        let has_preference = self.has_mode_preference(mode);
        if has_preference {
            let pref = self.get_mode_preference(mode);
            // Load the saved provider/model for this mode
            if !pref.provider.is_empty() {
                let _ = self.set_provider(&pref.provider);
            }
            if !pref.model.is_empty() {
                self.set_model(&pref.model);
            }
        }

        // Save session
        let _ = self.storage.save_session(&self.current_session);

        Ok(has_preference)
    }

    /// Change the provider
    ///
    /// Also updates the context limit based on the current model and new provider.
    pub fn set_provider(&mut self, provider_name: &str) -> Result<()> {
        let provider = llm::create_provider_with_options(provider_name, true, None)?;
        let provider = Arc::from(provider);

        self.provider_name = provider_name.to_string();
        self.current_session.provider = provider_name.to_string();

        // Update agent provider
        self.agent.update_provider(provider);

        // Update context limit for the new provider + current model
        let context_limit = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                crate::llm::models_db()
                    .get_context_limit(provider_name, &self.model_name)
                    .await
            })
        });
        self.agent.set_max_context_tokens(context_limit as usize);

        // Save session
        let _ = self.storage.save_session(&self.current_session);

        Ok(())
    }

    /// Set the model name and update context limit
    ///
    /// When switching models, this updates the agent's context limit to match
    /// the new model's context window. If current context exceeds the new limit,
    /// it will be trimmed or auto-compacted on the next message.
    pub fn set_model(&mut self, model_name: &str) {
        self.model_name = model_name.to_string();
        self.current_session.model = model_name.to_string();

        // Fetch the new model's context limit and update the agent
        let context_limit = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                crate::llm::models_db()
                    .get_context_limit(&self.provider_name, model_name)
                    .await
            })
        });

        self.agent.set_max_context_tokens(context_limit as usize);
        tracing::info!(
            "Model switched to {} (context limit: {} tokens)",
            model_name,
            context_limit
        );

        // Save session
        let _ = self.storage.save_session(&self.current_session);
    }

    /// Get the model preference for a specific agent mode
    ///
    /// Returns the saved provider/model preference for the given mode.
    /// Requirements: 2.2, 2.3
    pub fn get_mode_preference(&self, mode: AgentMode) -> ModelPreference {
        self.current_session
            .mode_preferences
            .get(&mode.to_string())
            .clone()
    }

    /// Set the model preference for a specific agent mode
    ///
    /// Saves the provider/model selection for the given mode only,
    /// without affecting other modes' preferences.
    /// Requirements: 2.2, 2.3
    pub fn set_mode_preference(&mut self, mode: AgentMode, pref: ModelPreference) {
        self.current_session
            .mode_preferences
            .set(&mode.to_string(), pref);

        // Save session
        let _ = self.storage.save_session(&self.current_session);
    }

    /// Check if a mode has a model preference configured
    ///
    /// Returns true if the mode has a non-empty provider/model preference.
    /// Requirements: 2.2, 2.3
    pub fn has_mode_preference(&self, mode: AgentMode) -> bool {
        self.current_session
            .mode_preferences
            .has_preference(&mode.to_string())
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.agent.clear_history();
        self.current_session.clear_messages();

        // Save session
        let _ = self.storage.save_session(&self.current_session);
    }

    /// Compact the conversation by summarizing older messages
    /// Returns a formatted result message showing before/after stats
    pub async fn compact(&mut self) -> Result<Option<crate::agent::CompactResult>> {
        self.agent.compact().await
    }

    /// Get available providers
    pub fn available_providers(&self) -> Vec<&'static str> {
        vec![
            "openai",
            "claude",
            "copilot",
            "gemini",
            "openrouter",
            "ollama",
        ]
    }

    /// Get storage reference
    pub fn storage(&self) -> &TarkStorage {
        &self.storage
    }

    /// Get mutable storage reference
    pub fn storage_mut(&mut self) -> &mut TarkStorage {
        &mut self.storage
    }
}

// ========== Session Management ==========

impl AgentBridge {
    /// Create a new session
    pub fn new_session(&mut self) -> Result<()> {
        // Create new session
        let session = self.storage.create_new_session()?;

        // Clear agent history
        self.agent.clear_history();

        // Update current session
        self.current_session = session;
        self.current_session.provider = self.provider_name.clone();
        self.current_session.model = self.model_name.clone();
        self.current_session.mode = self.mode.to_string();

        // Save session
        let _ = self.storage.save_session(&self.current_session);

        Ok(())
    }

    /// Switch to a different session
    pub fn switch_session(&mut self, session_id: &str) -> Result<()> {
        // Load the session
        let session = self.storage.load_session(session_id)?;

        // Update provider/model from session
        if !session.provider.is_empty() {
            let _ = self.set_provider(&session.provider);
        }
        if !session.model.is_empty() {
            self.model_name = session.model.clone();
        }

        // Update mode from session
        self.mode = AgentMode::from(session.mode.as_str());
        let tools = ToolRegistry::for_mode(
            self.working_dir.clone(),
            self.mode.into(),
            self.config.tools.shell_enabled,
        );
        self.agent.update_mode(tools, self.mode.into());

        // Restore agent from session
        self.agent.restore_from_session(&session);

        // Update current session
        self.current_session = session;

        // Set as current
        let _ = self.storage.set_current_session(&self.current_session.id);

        Ok(())
    }

    /// Delete a session
    pub fn delete_session(&mut self, session_id: &str) -> Result<()> {
        // Don't delete current session
        if session_id == self.current_session.id {
            anyhow::bail!("Cannot delete the current session");
        }

        self.storage.delete_session(session_id)?;
        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<crate::storage::SessionMeta>> {
        let mut sessions = self.storage.list_sessions()?;

        // Mark current session
        for session in &mut sessions {
            session.is_current = session.id == self.current_session.id;
            session.agent_running = session.is_current && self.is_processing();
        }

        Ok(sessions)
    }

    /// Get messages from current session for display
    pub fn get_session_messages(&self) -> Vec<SessionMessageInfo> {
        self.current_session
            .messages
            .iter()
            .map(|m| SessionMessageInfo {
                role: m.role.clone(),
                content: m.content.clone(),
                timestamp: m.timestamp,
            })
            .collect()
    }
}

/// Session message info for display
#[derive(Debug, Clone)]
pub struct SessionMessageInfo {
    pub role: String,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Format model ID to a display name
fn format_model_display(model_id: &str) -> String {
    match model_id {
        "gpt-4o" => "GPT-4o".to_string(),
        "gpt-4o-mini" => "GPT-4o Mini".to_string(),
        "gpt-4-turbo" => "GPT-4 Turbo".to_string(),
        "gpt-4-turbo-preview" => "GPT-4 Turbo Preview".to_string(),
        "gpt-4" => "GPT-4".to_string(),
        "gpt-3.5-turbo" => "GPT-3.5 Turbo".to_string(),
        "o1" => "O1".to_string(),
        "o1-mini" => "O1 Mini".to_string(),
        "o1-preview" => "O1 Preview".to_string(),
        "o3-mini" => "O3 Mini".to_string(),
        _ => {
            // Convert model-id to Model Id format
            model_id
                .split('-')
                .map(|s| {
                    let mut chars = s.chars();
                    match chars.next() {
                        Some(c) => c.to_uppercase().chain(chars).collect(),
                        None => String::new(),
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
    }
}

/// Get description for a model
fn format_model_description(model_id: &str) -> String {
    match model_id {
        "gpt-4o" => "Most capable, multimodal".to_string(),
        "gpt-4o-mini" => "Fast and affordable".to_string(),
        "gpt-4-turbo" | "gpt-4-turbo-preview" => "High capability, 128k context".to_string(),
        "gpt-4" => "Original GPT-4".to_string(),
        "gpt-3.5-turbo" => "Fast and economical".to_string(),
        "o1" => "Advanced reasoning model".to_string(),
        "o1-mini" => "Fast reasoning model".to_string(),
        "o1-preview" => "Reasoning preview".to_string(),
        "o3-mini" => "Latest reasoning model".to_string(),
        _ if model_id.contains("2024") || model_id.contains("2025") => "Dated snapshot".to_string(),
        _ => String::new(),
    }
}

/// Get priority for sorting models (lower = higher priority)
fn model_priority(model_id: &str) -> u8 {
    match model_id {
        "gpt-4o" => 0,
        "gpt-4o-mini" => 1,
        "o3-mini" => 2,
        "o1" => 3,
        "o1-mini" => 4,
        "gpt-4-turbo" => 5,
        "gpt-4" => 6,
        "gpt-3.5-turbo" => 7,
        _ if model_id.starts_with("gpt-4o") => 10,
        _ if model_id.starts_with("o3") => 15,
        _ if model_id.starts_with("o1") => 20,
        _ if model_id.starts_with("gpt-4") => 30,
        _ if model_id.starts_with("gpt-3") => 40,
        _ => 50,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_mode_conversion() {
        assert_eq!(AgentMode::from("build"), AgentMode::Build);
        assert_eq!(AgentMode::from("plan"), AgentMode::Plan);
        assert_eq!(AgentMode::from("review"), AgentMode::Review);
        assert_eq!(AgentMode::from("unknown"), AgentMode::Build);
    }

    #[test]
    fn test_agent_mode_display() {
        assert_eq!(AgentMode::Build.to_string(), "build");
        assert_eq!(AgentMode::Plan.to_string(), "plan");
        assert_eq!(AgentMode::Review.to_string(), "review");
    }

    #[test]
    fn test_agent_mode_to_tool_mode() {
        let tool_mode: ToolAgentMode = AgentMode::Build.into();
        assert_eq!(tool_mode, ToolAgentMode::Build);

        let tool_mode: ToolAgentMode = AgentMode::Plan.into();
        assert_eq!(tool_mode, ToolAgentMode::Plan);

        let tool_mode: ToolAgentMode = AgentMode::Review.into();
        assert_eq!(tool_mode, ToolAgentMode::Review);
    }

    #[test]
    fn test_agent_mode_next_cycle() {
        // Test forward cycling: Build → Plan → Review → Build
        assert_eq!(AgentMode::Build.next(), AgentMode::Plan);
        assert_eq!(AgentMode::Plan.next(), AgentMode::Review);
        assert_eq!(AgentMode::Review.next(), AgentMode::Build);
    }

    #[test]
    fn test_agent_mode_prev_cycle() {
        // Test reverse cycling: Build → Review → Plan → Build
        assert_eq!(AgentMode::Build.prev(), AgentMode::Review);
        assert_eq!(AgentMode::Review.prev(), AgentMode::Plan);
        assert_eq!(AgentMode::Plan.prev(), AgentMode::Build);
    }

    #[test]
    fn test_agent_mode_full_forward_cycle() {
        // Cycling forward 3 times should return to the same mode
        let start = AgentMode::Build;
        let after_one = start.next();
        let after_two = after_one.next();
        let after_three = after_two.next();
        assert_eq!(after_three, start);
    }

    #[test]
    fn test_agent_mode_full_reverse_cycle() {
        // Cycling backward 3 times should return to the same mode
        let start = AgentMode::Build;
        let after_one = start.prev();
        let after_two = after_one.prev();
        let after_three = after_two.prev();
        assert_eq!(after_three, start);
    }

    #[test]
    fn test_agent_mode_next_prev_inverse() {
        // next() followed by prev() should return to the same mode
        for mode in [AgentMode::Build, AgentMode::Plan, AgentMode::Review] {
            assert_eq!(mode.next().prev(), mode);
            assert_eq!(mode.prev().next(), mode);
        }
    }
}

// ========== Message Conversion for Display ==========

impl AgentBridge {
    /// Convert session messages to ChatMessages for display in the TUI
    pub fn get_chat_messages(&self) -> Vec<super::widgets::ChatMessage> {
        self.current_session
            .messages
            .iter()
            .map(|m| {
                let role = match m.role.as_str() {
                    "user" => super::widgets::Role::User,
                    "assistant" => super::widgets::Role::Assistant,
                    "system" => super::widgets::Role::System,
                    "tool" => super::widgets::Role::Tool,
                    _ => super::widgets::Role::System,
                };

                super::widgets::ChatMessage::new(role, m.content.clone())
            })
            .collect()
    }
}

/// Property-based tests for session round-trip
///
/// **Property 8: Session Restore Round-Trip**
/// **Validates: Requirements 7.1, 7.2**
#[cfg(test)]
mod property_tests {
    use crate::storage::{ChatSession, SessionMessage};
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Generate a random session message
    fn arb_session_message() -> impl Strategy<Value = SessionMessage> {
        (
            prop_oneof![
                Just("user".to_string()),
                Just("assistant".to_string()),
                Just("system".to_string()),
            ],
            "[a-zA-Z0-9 .,!?]{1,200}",
        )
            .prop_map(|(role, content)| SessionMessage {
                role,
                content,
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
            })
    }

    /// Generate a random provider name
    fn arb_provider() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("openai".to_string()),
            Just("claude".to_string()),
            Just("ollama".to_string()),
        ]
    }

    /// Generate a random model name
    fn arb_model() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4o".to_string()),
            Just("gpt-4".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("codellama".to_string()),
        ]
    }

    /// Generate a random agent mode
    fn arb_mode() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("build".to_string()),
            Just("plan".to_string()),
            Just("review".to_string()),
        ]
    }

    /// Generate a random session name
    fn arb_session_name() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,50}"
    }

    /// Generate a random model preference
    fn arb_model_preference() -> impl Strategy<Value = crate::storage::ModelPreference> {
        (arb_provider(), arb_model())
            .prop_map(|(provider, model)| crate::storage::ModelPreference::new(provider, model))
    }

    /// Generate a random mode preferences (per-mode model selections)
    fn arb_mode_preferences() -> impl Strategy<Value = crate::storage::ModePreferences> {
        (
            arb_model_preference(),
            arb_model_preference(),
            arb_model_preference(),
        )
            .prop_map(|(build, plan, ask)| crate::storage::ModePreferences {
                build,
                plan,
                ask,
            })
    }

    /// Generate a random chat session
    fn arb_chat_session() -> impl Strategy<Value = ChatSession> {
        (
            arb_session_name(),
            arb_provider(),
            arb_model(),
            arb_mode(),
            arb_mode_preferences(),
            prop::collection::vec(arb_session_message(), 0..10),
            0usize..10000usize,
            0usize..10000usize,
        )
            .prop_map(
                |(
                    name,
                    provider,
                    model,
                    mode,
                    mode_preferences,
                    messages,
                    input_tokens,
                    output_tokens,
                )| {
                    let mut session = ChatSession::new();
                    session.name = name;
                    session.provider = provider;
                    session.model = model;
                    session.mode = mode;
                    session.mode_preferences = mode_preferences;
                    session.messages = messages;
                    session.input_tokens = input_tokens;
                    session.output_tokens = output_tokens;
                    session
                },
            )
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// **Feature: tui-llm-integration, Property 6: Session Persistence Round-Trip**
        /// **Validates: Requirements 6.2, 6.5, 6.6**
        ///
        /// For any valid session with messages, provider, model, and mode,
        /// saving and restoring SHALL produce an equivalent session state
        /// including all messages, tool calls, and metadata.
        #[test]
        fn prop_session_round_trip(session in arb_chat_session()) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = crate::storage::TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Save the session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save session: {:?}", save_result.err());

            // Load the session back
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load session: {:?}", loaded.err());

            let loaded = loaded.unwrap();

            // Verify all fields are preserved
            prop_assert_eq!(&loaded.id, &session.id, "Session ID mismatch");
            prop_assert_eq!(&loaded.name, &session.name, "Session name mismatch");
            prop_assert_eq!(&loaded.provider, &session.provider, "Provider mismatch");
            prop_assert_eq!(&loaded.model, &session.model, "Model mismatch");
            prop_assert_eq!(&loaded.mode, &session.mode, "Mode mismatch");
            prop_assert_eq!(loaded.input_tokens, session.input_tokens, "Input tokens mismatch");
            prop_assert_eq!(loaded.output_tokens, session.output_tokens, "Output tokens mismatch");

            // Verify mode_preferences are preserved
            prop_assert_eq!(&loaded.mode_preferences, &session.mode_preferences, "Mode preferences mismatch");

            // Verify messages are preserved
            prop_assert_eq!(loaded.messages.len(), session.messages.len(), "Message count mismatch");

            for (i, (loaded_msg, original_msg)) in loaded.messages.iter().zip(session.messages.iter()).enumerate() {
                prop_assert_eq!(&loaded_msg.role, &original_msg.role,
                    "Message {} role mismatch", i);
                prop_assert_eq!(&loaded_msg.content, &original_msg.content,
                    "Message {} content mismatch", i);
                prop_assert_eq!(&loaded_msg.tool_call_id, &original_msg.tool_call_id,
                    "Message {} tool_call_id mismatch", i);
            }
        }

        /// **Feature: unified-model-selection, Property 5: Session Preference Round-Trip**
        /// **Validates: Requirements 2.5**
        ///
        /// For any session with mode preferences, saving and then loading the session
        /// SHALL restore identical mode preferences for all modes.
        #[test]
        fn prop_session_preference_round_trip(
            build_pref in arb_model_preference(),
            plan_pref in arb_model_preference(),
            ask_pref in arb_model_preference(),
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = crate::storage::TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Create a session with specific mode preferences
            let mut session = ChatSession::new();
            session.mode_preferences.build = build_pref.clone();
            session.mode_preferences.plan = plan_pref.clone();
            session.mode_preferences.ask = ask_pref.clone();

            // Save the session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save session: {:?}", save_result.err());

            // Load the session back
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load session: {:?}", loaded.err());

            let loaded = loaded.unwrap();

            // Verify each mode's preference is preserved independently
            prop_assert_eq!(&loaded.mode_preferences.build.provider, &build_pref.provider,
                "Build mode provider mismatch");
            prop_assert_eq!(&loaded.mode_preferences.build.model, &build_pref.model,
                "Build mode model mismatch");

            prop_assert_eq!(&loaded.mode_preferences.plan.provider, &plan_pref.provider,
                "Plan mode provider mismatch");
            prop_assert_eq!(&loaded.mode_preferences.plan.model, &plan_pref.model,
                "Plan mode model mismatch");

            prop_assert_eq!(&loaded.mode_preferences.ask.provider, &ask_pref.provider,
                "Ask mode provider mismatch");
            prop_assert_eq!(&loaded.mode_preferences.ask.model, &ask_pref.model,
                "Ask mode model mismatch");
        }

        /// **Feature: tui-llm-integration, Property 6: Session Persistence Round-Trip**
        /// **Validates: Requirements 6.2, 6.5, 6.6**
        ///
        /// For any session, setting it as current and loading current session
        /// SHALL return the same session.
        #[test]
        fn prop_current_session_round_trip(session in arb_chat_session()) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = crate::storage::TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Save the session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save session");

            // Set as current
            let set_result = storage.set_current_session(&session.id);
            prop_assert!(set_result.is_ok(), "Failed to set current session");

            // Load current session
            let loaded = storage.load_current_session();
            prop_assert!(loaded.is_ok(), "Failed to load current session");

            let loaded = loaded.unwrap();
            prop_assert_eq!(&loaded.id, &session.id, "Current session ID mismatch");
        }

        /// **Feature: tui-llm-integration, Property 6: Session Persistence Round-Trip**
        /// **Validates: Requirements 6.2, 6.5, 6.6**
        ///
        /// For any list of sessions, listing sessions SHALL return all saved sessions.
        #[test]
        fn prop_list_sessions_complete(
            session_count in 1usize..5usize
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = crate::storage::TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Create sessions with unique IDs
            let mut sessions = Vec::new();
            for i in 0..session_count {
                let mut session = ChatSession::new();
                // Ensure unique ID by appending index
                session.id = format!("{}_{}", session.id, i);
                session.name = format!("Test Session {}", i);
                sessions.push(session);
            }

            // Save all sessions
            for session in &sessions {
                let save_result = storage.save_session(session);
                prop_assert!(save_result.is_ok(), "Failed to save session");
            }

            // List sessions
            let listed = storage.list_sessions();
            prop_assert!(listed.is_ok(), "Failed to list sessions");

            let listed = listed.unwrap();

            // All sessions should be listed
            prop_assert_eq!(listed.len(), sessions.len(),
                "Listed session count mismatch: expected {}, got {}",
                sessions.len(), listed.len());

            // Each session should be in the list
            for session in &sessions {
                let found = listed.iter().any(|meta| meta.id == session.id);
                prop_assert!(found, "Session {} not found in list", session.id);
            }
        }

        /// **Feature: tui-llm-integration, Property 13: Mode Cycling**
        /// **Validates: Requirements 13.1, 13.2, 13.4, 13.5, 13.6**
        ///
        /// For any starting mode and any number of forward cycles,
        /// the mode SHALL cycle through Build → Plan → Review → Build.
        /// After 3 forward cycles, the mode SHALL return to the starting mode.
        #[test]
        fn prop_mode_cycling_forward(
            start_mode_idx in 0usize..3usize,
            cycles in 0usize..20usize,
        ) {
            use super::AgentMode;

            let modes = [AgentMode::Build, AgentMode::Plan, AgentMode::Review];
            let start_mode = modes[start_mode_idx];

            // Apply forward cycles
            let mut current = start_mode;
            for _ in 0..cycles {
                current = current.next();
            }

            // Calculate expected mode
            let expected_idx = (start_mode_idx + cycles) % 3;
            let expected = modes[expected_idx];

            prop_assert_eq!(current, expected,
                "After {} forward cycles from {:?}, expected {:?} but got {:?}",
                cycles, start_mode, expected, current);
        }

        /// **Feature: tui-llm-integration, Property 13: Mode Cycling**
        /// **Validates: Requirements 13.1, 13.2, 13.4, 13.5, 13.6**
        ///
        /// For any starting mode and any number of backward cycles,
        /// the mode SHALL cycle through Build → Review → Plan → Build.
        /// After 3 backward cycles, the mode SHALL return to the starting mode.
        #[test]
        fn prop_mode_cycling_backward(
            start_mode_idx in 0usize..3usize,
            cycles in 0usize..20usize,
        ) {
            use super::AgentMode;

            let modes = [AgentMode::Build, AgentMode::Plan, AgentMode::Review];
            let start_mode = modes[start_mode_idx];

            // Apply backward cycles
            let mut current = start_mode;
            for _ in 0..cycles {
                current = current.prev();
            }

            // Calculate expected mode (going backward)
            // Build(0) -> Review(2) -> Plan(1) -> Build(0)
            // So prev() subtracts 1 mod 3, but we need to handle negative modulo
            let expected_idx = (start_mode_idx + 3 * cycles - cycles) % 3;
            let expected = modes[expected_idx];

            prop_assert_eq!(current, expected,
                "After {} backward cycles from {:?}, expected {:?} but got {:?}",
                cycles, start_mode, expected, current);
        }

        /// **Feature: tui-llm-integration, Property 13: Mode Cycling**
        /// **Validates: Requirements 13.1, 13.2, 13.4, 13.5, 13.6**
        ///
        /// For any starting mode, cycling forward then backward the same number
        /// of times SHALL return to the starting mode (round-trip property).
        #[test]
        fn prop_mode_cycling_round_trip(
            start_mode_idx in 0usize..3usize,
            cycles in 0usize..20usize,
        ) {
            use super::AgentMode;

            let modes = [AgentMode::Build, AgentMode::Plan, AgentMode::Review];
            let start_mode = modes[start_mode_idx];

            // Apply forward cycles
            let mut current = start_mode;
            for _ in 0..cycles {
                current = current.next();
            }

            // Apply backward cycles (same number)
            for _ in 0..cycles {
                current = current.prev();
            }

            prop_assert_eq!(current, start_mode,
                "After {} forward and {} backward cycles from {:?}, expected to return to {:?} but got {:?}",
                cycles, cycles, start_mode, start_mode, current);
        }

        /// **Feature: unified-model-selection, Property 4: Per-Mode Preferences Independence**
        /// **Validates: Requirements 2.1, 2.3**
        ///
        /// For any model change in one agent mode, the model preferences for other modes
        /// SHALL remain unchanged. Each mode maintains its own independent provider/model configuration.
        #[test]
        fn prop_per_mode_preferences_independence(
            build_pref in arb_model_preference(),
            plan_pref in arb_model_preference(),
            ask_pref in arb_model_preference(),
            new_pref in arb_model_preference(),
            target_mode_idx in 0usize..3usize,
        ) {
            use crate::storage::ModePreferences;

            // Create initial mode preferences
            let mut prefs = ModePreferences {
                build: build_pref.clone(),
                plan: plan_pref.clone(),
                ask: ask_pref.clone(),
            };

            // Determine which mode to modify and which to check remain unchanged
            let modes = ["build", "plan", "ask"];
            let target_mode = modes[target_mode_idx];

            // Store original values for non-target modes
            let original_build = prefs.build.clone();
            let original_plan = prefs.plan.clone();
            let original_ask = prefs.ask.clone();

            // Modify only the target mode's preference
            prefs.set(target_mode, new_pref.clone());

            // Verify the target mode was updated
            let updated_pref = prefs.get(target_mode);
            prop_assert_eq!(&updated_pref.provider, &new_pref.provider,
                "Target mode {} provider should be updated", target_mode);
            prop_assert_eq!(&updated_pref.model, &new_pref.model,
                "Target mode {} model should be updated", target_mode);

            // Verify other modes remain unchanged
            match target_mode {
                "build" => {
                    prop_assert_eq!(&prefs.plan, &original_plan,
                        "Plan mode should remain unchanged when Build is modified");
                    prop_assert_eq!(&prefs.ask, &original_ask,
                        "Ask mode should remain unchanged when Build is modified");
                }
                "plan" => {
                    prop_assert_eq!(&prefs.build, &original_build,
                        "Build mode should remain unchanged when Plan is modified");
                    prop_assert_eq!(&prefs.ask, &original_ask,
                        "Ask mode should remain unchanged when Plan is modified");
                }
                "ask" => {
                    prop_assert_eq!(&prefs.build, &original_build,
                        "Build mode should remain unchanged when Ask is modified");
                    prop_assert_eq!(&prefs.plan, &original_plan,
                        "Plan mode should remain unchanged when Ask is modified");
                }
                _ => {}
            }
        }
    }
}

/// Property-based tests for attachment handling
///
/// **Property 10: Attachment Handling**
/// **Validates: Requirements 10.1, 10.2, 10.5, 10.6**
#[cfg(test)]
mod attachment_property_tests {
    use super::*;
    use crate::tui::attachments::{
        AttachmentConfig, AttachmentContent, AttachmentType, ImageFormat, MessageAttachment,
    };
    use proptest::prelude::*;

    /// Generate a random text attachment
    fn arb_text_attachment() -> impl Strategy<Value = MessageAttachment> {
        (
            "[a-zA-Z0-9_-]{1,20}\\.[a-z]{1,4}",
            "[a-zA-Z0-9 .,!?\\n]{1,500}",
        )
            .prop_map(|(filename, content)| MessageAttachment {
                filename,
                mime_type: "text/plain".to_string(),
                content: AttachmentContent::Text(content),
            })
    }

    /// Generate a random image attachment (base64 encoded)
    fn arb_image_attachment() -> impl Strategy<Value = MessageAttachment> {
        (
            "[a-zA-Z0-9_-]{1,20}\\.png",
            prop::collection::vec(any::<u8>(), 100..1000),
        )
            .prop_map(|(filename, bytes)| {
                let encoded = crate::tui::attachments::base64_encode(&bytes);
                MessageAttachment {
                    filename,
                    mime_type: "image/png".to_string(),
                    content: AttachmentContent::Base64(encoded),
                }
            })
    }

    /// Generate a random attachment (text or image)
    fn arb_attachment() -> impl Strategy<Value = MessageAttachment> {
        prop_oneof![arb_text_attachment(), arb_image_attachment(),]
    }

    /// Generate a random message
    fn arb_message() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?]{1,200}"
    }

    /// Generate a random model name
    fn arb_model_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4o".to_string()),
            Just("gpt-4-turbo".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("claude-3-opus".to_string()),
            Just("llava".to_string()),
            Just("gpt-3.5-turbo".to_string()),
            Just("codellama".to_string()),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: tui-llm-integration, Property 10: Attachment Handling**
        /// **Validates: Requirements 10.1, 10.2**
        ///
        /// For any attached file, the AgentBridge SHALL include it in the message
        /// to the LLM with appropriate formatting.
        #[test]
        fn prop_attachment_formatting(
            message in arb_message(),
            attachment in arb_text_attachment(),
        ) {
            // Create a mock AgentBridge-like formatter
            let formatted = format_message_with_attachments_test(&message, std::slice::from_ref(&attachment));

            // The formatted message should contain the original message
            prop_assert!(formatted.contains(&message),
                "Formatted message should contain original message");

            // The formatted message should contain the filename
            prop_assert!(formatted.contains(&attachment.filename),
                "Formatted message should contain filename '{}'", attachment.filename);

            // Text attachments should be wrapped appropriately
            if let AttachmentContent::Text(content) = &attachment.content {
                // Either the content is included directly or wrapped in code blocks
                let has_content = formatted.contains(content) ||
                    formatted.contains("```");
                prop_assert!(has_content,
                    "Formatted message should contain attachment content or code blocks");
            }
        }

        /// **Feature: tui-llm-integration, Property 10: Attachment Handling**
        /// **Validates: Requirements 10.5**
        ///
        /// For any attachment exceeding size limits, validation SHALL reject it.
        #[test]
        fn prop_attachment_size_validation(
            max_size in 100u64..10000u64,
            attachment_size in 100u64..20000u64,
        ) {
            let config = AttachmentConfig {
                max_attachment_size: max_size,
                max_attachments: 10,
                temp_dir: std::env::temp_dir().join("tark-test"),
            };

            let attachment = MessageAttachment {
                filename: "test.txt".to_string(),
                mime_type: "text/plain".to_string(),
                content: AttachmentContent::Text("x".repeat(attachment_size as usize)),
            };

            let result = validate_attachments_test(&[attachment], &config);

            if attachment_size > max_size {
                prop_assert!(result.is_err(),
                    "Attachment of size {} should be rejected when max is {}",
                    attachment_size, max_size);
            } else {
                prop_assert!(result.is_ok(),
                    "Attachment of size {} should be accepted when max is {}",
                    attachment_size, max_size);
            }
        }

        /// **Feature: tui-llm-integration, Property 10: Attachment Handling**
        /// **Validates: Requirements 10.5**
        ///
        /// For any number of attachments exceeding the limit, validation SHALL reject.
        #[test]
        fn prop_attachment_count_validation(
            max_count in 1usize..10usize,
            attachment_count in 1usize..15usize,
        ) {
            let config = AttachmentConfig {
                max_attachment_size: 10 * 1024 * 1024,
                max_attachments: max_count,
                temp_dir: std::env::temp_dir().join("tark-test"),
            };

            let attachments: Vec<MessageAttachment> = (0..attachment_count)
                .map(|i| MessageAttachment {
                    filename: format!("file{}.txt", i),
                    mime_type: "text/plain".to_string(),
                    content: AttachmentContent::Text("test".to_string()),
                })
                .collect();

            let result = validate_attachments_test(&attachments, &config);

            if attachment_count > max_count {
                prop_assert!(result.is_err(),
                    "Should reject {} attachments when max is {}",
                    attachment_count, max_count);
            } else {
                prop_assert!(result.is_ok(),
                    "Should accept {} attachments when max is {}",
                    attachment_count, max_count);
            }
        }

        /// **Feature: tui-llm-integration, Property 10: Attachment Handling**
        /// **Validates: Requirements 10.6**
        ///
        /// For any list of attachments, after formatting they should all be included
        /// in the message and the original message should be preserved.
        #[test]
        fn prop_all_attachments_included(
            message in arb_message(),
            attachments in prop::collection::vec(arb_text_attachment(), 1..5),
        ) {
            let formatted = format_message_with_attachments_test(&message, &attachments);

            // Original message should be at the start
            prop_assert!(formatted.starts_with(&message),
                "Formatted message should start with original message");

            // All attachment filenames should be present
            for attachment in &attachments {
                prop_assert!(formatted.contains(&attachment.filename),
                    "Formatted message should contain filename '{}'", attachment.filename);
            }
        }

        /// **Feature: tui-llm-integration, Property 10: Attachment Handling**
        /// **Validates: Requirements 10.3, 10.4**
        ///
        /// For any vision-capable model, image attachments should be accepted.
        /// For non-vision models, image attachments should be rejected.
        #[test]
        fn prop_vision_model_validation(
            model_name in arb_model_name(),
        ) {
            let supports_vision = check_vision_support(&model_name);

            let image_attachment = MessageAttachment {
                filename: "test.png".to_string(),
                mime_type: "image/png".to_string(),
                content: AttachmentContent::Base64("test".to_string()),
            };

            let result = validate_image_attachments_test(&[image_attachment], &model_name);

            if supports_vision {
                prop_assert!(result.is_ok(),
                    "Vision model '{}' should accept image attachments", model_name);
            } else {
                prop_assert!(result.is_err(),
                    "Non-vision model '{}' should reject image attachments", model_name);
            }
        }
    }

    // Helper functions for testing (mirrors AgentBridge methods)

    fn format_message_with_attachments_test(
        message: &str,
        attachments: &[MessageAttachment],
    ) -> String {
        if attachments.is_empty() {
            return message.to_string();
        }

        let mut formatted = String::new();
        formatted.push_str(message);

        for attachment in attachments {
            formatted.push_str("\n\n");
            formatted.push_str(&format_single_attachment_test(attachment));
        }

        formatted
    }

    fn format_single_attachment_test(attachment: &MessageAttachment) -> String {
        let filename = &attachment.filename;

        match &attachment.content {
            AttachmentContent::Text(text) => {
                if text.starts_with("```") {
                    format!("**Attached file: {}**\n{}", filename, text)
                } else {
                    format!("**Attached file: {}**\n```\n{}\n```", filename, text)
                }
            }
            AttachmentContent::Base64(base64_data) => {
                format!(
                    "**Attached image: {}**\n[Image data: {} encoded as base64, {} bytes]",
                    filename,
                    attachment.mime_type,
                    base64_data.len()
                )
            }
            AttachmentContent::Path(path) => {
                format!(
                    "**Attached file: {}**\n[File at path: {}]",
                    filename,
                    path.display()
                )
            }
        }
    }

    fn validate_attachments_test(
        attachments: &[MessageAttachment],
        config: &AttachmentConfig,
    ) -> anyhow::Result<()> {
        for attachment in attachments {
            let size = attachment.content.size();
            if size > config.max_attachment_size {
                return Err(anyhow::anyhow!(
                    "Attachment '{}' exceeds size limit",
                    attachment.filename
                ));
            }
        }

        if attachments.len() > config.max_attachments {
            return Err(anyhow::anyhow!("Too many attachments"));
        }

        Ok(())
    }

    fn check_vision_support(model_name: &str) -> bool {
        let model = model_name.to_lowercase();

        if model.contains("gpt-4") && (model.contains("vision") || model.contains("turbo")) {
            return true;
        }
        if model.contains("gpt-4o") {
            return true;
        }
        if model.contains("claude-3") {
            return true;
        }
        if model.contains("llava") || model.contains("bakllava") {
            return true;
        }

        false
    }

    fn validate_image_attachments_test(
        attachments: &[MessageAttachment],
        model_name: &str,
    ) -> Result<(), AttachmentError> {
        let has_images = attachments.iter().any(|a| a.is_image());

        if has_images && !check_vision_support(model_name) {
            return Err(AttachmentError::UnsupportedFileType(format!(
                "Model '{}' does not support image attachments",
                model_name
            )));
        }

        Ok(())
    }
}

/// Property-based tests for provider/model switching
///
/// **Property 12: Provider/Model Switching**
/// **Validates: Requirements 12.3, 12.4, 12.5, 12.6**
#[cfg(test)]
mod provider_model_property_tests {
    use crate::storage::{ChatSession, SessionMessage, TarkStorage};
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Generate a random provider name
    fn arb_provider() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("openai".to_string()),
            Just("claude".to_string()),
            Just("ollama".to_string()),
        ]
    }

    /// Generate a random model name for a given provider
    fn arb_model_for_provider(provider: &str) -> Vec<String> {
        match provider {
            "openai" => vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4-turbo".to_string(),
                "gpt-3.5-turbo".to_string(),
            ],
            "claude" => vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-opus-20240229".to_string(),
                "claude-3-haiku-20240307".to_string(),
            ],
            "ollama" => vec![
                "llama3.2".to_string(),
                "codellama".to_string(),
                "mistral".to_string(),
                "deepseek-coder".to_string(),
            ],
            _ => vec!["default".to_string()],
        }
    }

    /// Generate a random model name
    fn arb_model() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4o".to_string()),
            Just("gpt-4o-mini".to_string()),
            Just("claude-3-5-sonnet-20241022".to_string()),
            Just("codellama".to_string()),
        ]
    }

    /// Generate a random session message
    fn arb_session_message() -> impl Strategy<Value = SessionMessage> {
        (
            prop_oneof![Just("user".to_string()), Just("assistant".to_string()),],
            "[a-zA-Z0-9 .,!?]{1,100}",
        )
            .prop_map(|(role, content)| SessionMessage {
                role,
                content,
                timestamp: chrono::Utc::now(),
                tool_call_id: None,
            })
    }

    /// Generate a random chat session with messages
    fn arb_session_with_messages() -> impl Strategy<Value = ChatSession> {
        (
            "[a-zA-Z0-9 ]{1,30}",
            arb_provider(),
            arb_model(),
            prop::collection::vec(arb_session_message(), 1..10),
        )
            .prop_map(|(name, provider, model, messages)| {
                let mut session = ChatSession::new();
                session.name = name;
                session.provider = provider;
                session.model = model;
                session.messages = messages;
                session
            })
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: tui-llm-integration, Property 12: Provider/Model Switching**
        /// **Validates: Requirements 12.3, 12.5**
        ///
        /// For any provider change, the session SHALL update the provider field
        /// while preserving all existing messages (conversation history).
        #[test]
        fn prop_provider_switch_preserves_messages(
            session in arb_session_with_messages(),
            new_provider in arb_provider(),
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Save the initial session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save initial session");

            // Simulate provider switch by updating session and saving
            let mut updated_session = session.clone();
            updated_session.provider = new_provider.clone();
            let save_result = storage.save_session(&updated_session);
            prop_assert!(save_result.is_ok(), "Failed to save updated session");

            // Load the session back
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load session");

            let loaded = loaded.unwrap();

            // Verify provider was updated (Requirements 12.3)
            prop_assert_eq!(&loaded.provider, &new_provider,
                "Provider should be updated to '{}'", new_provider);

            // Verify messages are preserved (Requirements 12.5)
            prop_assert_eq!(loaded.messages.len(), session.messages.len(),
                "Message count should be preserved after provider switch");

            for (i, (loaded_msg, original_msg)) in loaded.messages.iter().zip(session.messages.iter()).enumerate() {
                prop_assert_eq!(&loaded_msg.role, &original_msg.role,
                    "Message {} role should be preserved", i);
                prop_assert_eq!(&loaded_msg.content, &original_msg.content,
                    "Message {} content should be preserved", i);
            }
        }

        /// **Feature: tui-llm-integration, Property 12: Provider/Model Switching**
        /// **Validates: Requirements 12.4, 12.5**
        ///
        /// For any model change, the session SHALL update the model field
        /// while preserving all existing messages (conversation history).
        #[test]
        fn prop_model_switch_preserves_messages(
            session in arb_session_with_messages(),
            new_model in arb_model(),
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Save the initial session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save initial session");

            // Simulate model switch by updating session and saving
            let mut updated_session = session.clone();
            updated_session.model = new_model.clone();
            let save_result = storage.save_session(&updated_session);
            prop_assert!(save_result.is_ok(), "Failed to save updated session");

            // Load the session back
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load session");

            let loaded = loaded.unwrap();

            // Verify model was updated (Requirements 12.4)
            prop_assert_eq!(&loaded.model, &new_model,
                "Model should be updated to '{}'", new_model);

            // Verify messages are preserved (Requirements 12.5)
            prop_assert_eq!(loaded.messages.len(), session.messages.len(),
                "Message count should be preserved after model switch");

            for (i, (loaded_msg, original_msg)) in loaded.messages.iter().zip(session.messages.iter()).enumerate() {
                prop_assert_eq!(&loaded_msg.role, &original_msg.role,
                    "Message {} role should be preserved", i);
                prop_assert_eq!(&loaded_msg.content, &original_msg.content,
                    "Message {} content should be preserved", i);
            }
        }

        /// **Feature: tui-llm-integration, Property 12: Provider/Model Switching**
        /// **Validates: Requirements 12.5, 12.6**
        ///
        /// For any sequence of provider and model changes, the conversation history
        /// SHALL be preserved throughout all switches.
        #[test]
        fn prop_multiple_switches_preserve_history(
            session in arb_session_with_messages(),
            providers in prop::collection::vec(arb_provider(), 1..5),
            models in prop::collection::vec(arb_model(), 1..5),
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Save the initial session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save initial session");

            let original_messages = session.messages.clone();
            let mut current_session = session.clone();

            // Perform multiple provider switches
            for provider in &providers {
                current_session.provider = provider.clone();
                let save_result = storage.save_session(&current_session);
                prop_assert!(save_result.is_ok(), "Failed to save after provider switch");
            }

            // Perform multiple model switches
            for model in &models {
                current_session.model = model.clone();
                let save_result = storage.save_session(&current_session);
                prop_assert!(save_result.is_ok(), "Failed to save after model switch");
            }

            // Load the final session
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load final session");

            let loaded = loaded.unwrap();

            // Verify final provider and model are correct
            prop_assert_eq!(&loaded.provider, providers.last().unwrap(),
                "Final provider should match last switch");
            prop_assert_eq!(&loaded.model, models.last().unwrap(),
                "Final model should match last switch");

            // Verify all messages are still preserved (Requirements 12.5, 12.6)
            prop_assert_eq!(loaded.messages.len(), original_messages.len(),
                "Message count should be preserved after multiple switches");

            for (i, (loaded_msg, original_msg)) in loaded.messages.iter().zip(original_messages.iter()).enumerate() {
                prop_assert_eq!(&loaded_msg.role, &original_msg.role,
                    "Message {} role should be preserved after multiple switches", i);
                prop_assert_eq!(&loaded_msg.content, &original_msg.content,
                    "Message {} content should be preserved after multiple switches", i);
            }
        }

        /// **Feature: tui-llm-integration, Property 12: Provider/Model Switching**
        /// **Validates: Requirements 12.3, 12.4**
        ///
        /// For any provider/model combination, the session SHALL correctly store
        /// both values and they SHALL be retrievable after reload.
        #[test]
        fn prop_provider_model_combination_persists(
            session_name in "[a-zA-Z0-9 ]{1,30}",
            provider in arb_provider(),
            model in arb_model(),
        ) {
            // Create a temporary directory for storage
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage = TarkStorage::new(temp_dir.path())
                .expect("Failed to create storage");

            // Create a session with specific provider and model
            let mut session = ChatSession::new();
            session.name = session_name;
            session.provider = provider.clone();
            session.model = model.clone();

            // Save the session
            let save_result = storage.save_session(&session);
            prop_assert!(save_result.is_ok(), "Failed to save session");

            // Load the session back
            let loaded = storage.load_session(&session.id);
            prop_assert!(loaded.is_ok(), "Failed to load session");

            let loaded = loaded.unwrap();

            // Verify both provider and model are correctly persisted
            prop_assert_eq!(&loaded.provider, &provider,
                "Provider should be persisted correctly");
            prop_assert_eq!(&loaded.model, &model,
                "Model should be persisted correctly");
        }
    }
}
