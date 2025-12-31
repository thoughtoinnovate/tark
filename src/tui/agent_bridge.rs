//! Agent bridge for TUI integration
//!
//! Provides the connection between the TUI and the chat agent backend,
//! handling message sending, response streaming, and tool call display.

// Allow dead code for intentionally unused API methods that are part of the public interface
#![allow(dead_code)]

use crate::agent::{AgentResponse, ChatAgent, ToolCallLog};
use crate::config::Config;
use crate::llm;
use crate::storage::{ChatSession, TarkStorage};
use crate::tools::{AgentMode as ToolAgentMode, ToolRegistry};
use anyhow::Result;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Events sent from the agent to the TUI
#[derive(Debug, Clone)]
pub enum AgentEvent {
    /// Agent started processing
    Started,
    /// Streaming text chunk received
    TextChunk(String),
    /// Tool call started
    ToolCallStarted {
        tool: String,
        args: serde_json::Value,
    },
    /// Tool call completed
    ToolCallCompleted {
        tool: String,
        result_preview: String,
    },
    /// Agent finished with response
    Completed(AgentResponseInfo),
    /// Agent encountered an error
    Error(String),
    /// Agent was interrupted
    Interrupted,
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
}

impl AgentBridge {
    /// Create a new agent bridge
    pub fn new(working_dir: PathBuf) -> Result<Self> {
        let config = Config::load().unwrap_or_default();
        let storage = TarkStorage::new(&working_dir)?;

        // Load or create session
        let current_session = storage.load_current_session().unwrap_or_else(|_| {
            let session = ChatSession::new();
            let _ = storage.save_session(&session);
            session
        });

        // Get provider and model from session or config
        let provider_name = if current_session.provider.is_empty() {
            config.llm.default_provider.clone()
        } else {
            current_session.provider.clone()
        };

        let model_name = if current_session.model.is_empty() {
            // Get default model based on provider
            match provider_name.as_str() {
                "claude" | "anthropic" => config.llm.claude.model.clone(),
                "openai" | "gpt" => config.llm.openai.model.clone(),
                "ollama" | "local" => config.llm.ollama.model.clone(),
                _ => String::new(),
            }
        } else {
            current_session.model.clone()
        };

        // Create LLM provider
        let provider = llm::create_provider(&provider_name)?;
        let provider = Arc::from(provider);

        // Get mode from session
        let mode = AgentMode::from(current_session.mode.as_str());

        // Create tool registry for mode
        let tools =
            ToolRegistry::for_mode(working_dir.clone(), mode.into(), config.tools.shell_enabled);

        // Create agent
        let mut agent = ChatAgent::with_mode(provider, tools, mode.into())
            .with_max_iterations(config.agent.max_iterations);

        // Restore session messages if any
        if !current_session.messages.is_empty() {
            agent.restore_from_session(&current_session);
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

    /// Get context usage percentage
    pub fn context_usage_percent(&self) -> usize {
        // This would need to be tracked from the last response
        // For now, return 0 as a placeholder
        0
    }

    /// Get total cost from current session
    pub fn total_cost(&self) -> f64 {
        self.current_session.total_cost
    }

    /// Get total tokens from current session
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

                // Update token counts
                if let Some(usage) = &response.usage {
                    self.current_session.input_tokens += usage.input_tokens as usize;
                    self.current_session.output_tokens += usage.output_tokens as usize;
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

    /// Send a message with event streaming
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

        // Send to agent
        let result = self
            .agent
            .chat_with_interrupt(message, interrupt_check)
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

                    // Update token counts
                    if let Some(usage) = &response.usage {
                        self.current_session.input_tokens += usage.input_tokens as usize;
                        self.current_session.output_tokens += usage.output_tokens as usize;
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

    /// Change the agent mode
    pub fn set_mode(&mut self, mode: AgentMode) -> Result<()> {
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

        // Save session
        let _ = self.storage.save_session(&self.current_session);

        Ok(())
    }

    /// Change the provider
    pub fn set_provider(&mut self, provider_name: &str) -> Result<()> {
        let provider = llm::create_provider(provider_name)?;
        let provider = Arc::from(provider);

        self.provider_name = provider_name.to_string();
        self.current_session.provider = provider_name.to_string();

        // Update agent provider
        self.agent.update_provider(provider);

        // Save session
        let _ = self.storage.save_session(&self.current_session);

        Ok(())
    }

    /// Set the model name
    pub fn set_model(&mut self, model_name: &str) {
        self.model_name = model_name.to_string();
        self.current_session.model = model_name.to_string();

        // Save session
        let _ = self.storage.save_session(&self.current_session);
    }

    /// Clear conversation history
    pub fn clear_history(&mut self) {
        self.agent.clear_history();
        self.current_session.clear_messages();

        // Save session
        let _ = self.storage.save_session(&self.current_session);
    }

    /// Compact the conversation
    pub async fn compact(&mut self) -> Result<String> {
        // For now, just clear and return a message
        // TODO: Implement actual compaction with summary
        self.clear_history();
        Ok("Conversation compacted".to_string())
    }

    /// Get available providers
    pub fn available_providers(&self) -> Vec<&'static str> {
        vec!["openai", "claude", "ollama"]
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

    /// Generate a random chat session
    fn arb_chat_session() -> impl Strategy<Value = ChatSession> {
        (
            arb_session_name(),
            arb_provider(),
            arb_model(),
            arb_mode(),
            prop::collection::vec(arb_session_message(), 0..10),
            0usize..10000usize,
            0usize..10000usize,
        )
            .prop_map(
                |(name, provider, model, mode, messages, input_tokens, output_tokens)| {
                    let mut session = ChatSession::new();
                    session.name = name;
                    session.provider = provider;
                    session.model = model;
                    session.mode = mode;
                    session.messages = messages;
                    session.input_tokens = input_tokens;
                    session.output_tokens = output_tokens;
                    session
                },
            )
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 8: Session Restore Round-Trip**
        /// **Validates: Requirements 7.1, 7.2**
        ///
        /// For any valid session with messages, provider, model, and mode,
        /// saving and restoring SHALL produce an equivalent session state.
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

        /// **Feature: terminal-tui-chat, Property 8: Session Restore Round-Trip**
        /// **Validates: Requirements 7.1, 7.2**
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

        /// **Feature: terminal-tui-chat, Property 8: Session Restore Round-Trip**
        /// **Validates: Requirements 7.1, 7.2**
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
    }
}
