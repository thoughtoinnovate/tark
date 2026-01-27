//! Conversation Manager - Message history and streaming state
//!
//! Handles:
//! - Message history management
//! - Streaming content accumulation
//! - Streaming state machine (Idle -> Streaming -> ToolCall -> Complete)
//! - Conversion between storage and LLM message formats

use super::errors::ConversationError;
use crate::llm::{Message as LlmMessage, MessageContent, Role};
use crate::storage::{ChatSession, SessionMessage};
use chrono::Utc;
use std::time::Instant;

/// Streaming state machine
#[derive(Debug, Clone, PartialEq)]
pub enum StreamingState {
    /// No streaming in progress
    Idle,
    /// Receiving text content
    ReceivingText { accumulated: String },
    /// Receiving thinking content
    ReceivingThinking { accumulated: String },
    /// Tool call pending (partial JSON received)
    ToolCallPending { tool: String, partial_args: String },
    /// Awaiting tool result
    AwaitingToolResult { tool: String },
    /// Error occurred
    Error { error: String, recoverable: bool },
    /// Streaming completed
    Completed,
}

impl StreamingState {
    /// Get a string representation for debugging
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::ReceivingText { .. } => "ReceivingText",
            Self::ReceivingThinking { .. } => "ReceivingThinking",
            Self::ToolCallPending { .. } => "ToolCallPending",
            Self::AwaitingToolResult { .. } => "AwaitingToolResult",
            Self::Error { .. } => "Error",
            Self::Completed => "Completed",
        }
    }
}

/// Message for conversation history
#[derive(Debug, Clone)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub thinking: Option<String>,
    pub timestamp: chrono::DateTime<Utc>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: "user".to_string(),
            content,
            thinking: None,
            timestamp: Utc::now(),
        }
    }

    pub fn assistant(content: String, thinking: Option<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content,
            thinking,
            timestamp: Utc::now(),
        }
    }

    pub fn system(content: String) -> Self {
        Self {
            role: "system".to_string(),
            content,
            thinking: None,
            timestamp: Utc::now(),
        }
    }
}

/// Manages conversation messages and streaming state
pub struct ConversationManager {
    /// Message history
    messages: Vec<Message>,
    /// Current streaming state
    state: StreamingState,
    /// State transition history (for debugging)
    state_transitions: Vec<(Instant, String)>,
}

impl ConversationManager {
    /// Create a new conversation manager
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            state: StreamingState::Idle,
            state_transitions: Vec::new(),
        }
    }

    /// Get current streaming state
    pub fn state(&self) -> &StreamingState {
        &self.state
    }

    /// Get all messages
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Get message count
    pub fn message_count(&self) -> usize {
        self.messages.len()
    }

    /// Add a user message
    pub fn add_user_message(&mut self, content: String) {
        self.messages.push(Message::user(content));
    }

    /// Add an assistant message
    pub fn add_assistant_message(&mut self, content: String, thinking: Option<String>) {
        self.messages.push(Message::assistant(content, thinking));
    }

    /// Add a system message
    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(Message::system(content));
    }

    /// Clear all messages
    pub fn clear_messages(&mut self) {
        self.messages.clear();
    }

    /// Start streaming (transition to ReceivingText state)
    pub fn start_streaming(&mut self) -> Result<(), ConversationError> {
        self.transition_state(StreamingState::ReceivingText {
            accumulated: String::new(),
        })
    }

    /// Append content to streaming text
    pub fn append_streaming_content(&mut self, chunk: &str) -> Result<(), ConversationError> {
        match &mut self.state {
            StreamingState::ReceivingText { accumulated } => {
                accumulated.push_str(chunk);
                Ok(())
            }
            _ => Err(ConversationError::InvalidStateTransition {
                from: self.state.as_str().to_string(),
                to: "append_content".to_string(),
            }),
        }
    }

    /// Start streaming thinking content
    pub fn start_streaming_thinking(&mut self) -> Result<(), ConversationError> {
        self.transition_state(StreamingState::ReceivingThinking {
            accumulated: String::new(),
        })
    }

    /// Append content to streaming thinking
    pub fn append_streaming_thinking(&mut self, chunk: &str) -> Result<(), ConversationError> {
        match &mut self.state {
            StreamingState::ReceivingThinking { accumulated } => {
                accumulated.push_str(chunk);
                Ok(())
            }
            _ => Err(ConversationError::InvalidStateTransition {
                from: self.state.as_str().to_string(),
                to: "append_thinking".to_string(),
            }),
        }
    }

    /// Get accumulated streaming content
    pub fn get_streaming_content(&self) -> Option<String> {
        match &self.state {
            StreamingState::ReceivingText { accumulated } => Some(accumulated.clone()),
            _ => None,
        }
    }

    /// Get accumulated thinking content
    pub fn get_streaming_thinking(&self) -> Option<String> {
        match &self.state {
            StreamingState::ReceivingThinking { accumulated } => Some(accumulated.clone()),
            _ => None,
        }
    }

    /// Finalize streaming and create assistant message
    ///
    /// This transitions to Completed and adds the accumulated content as an assistant message.
    pub fn finalize_streaming(&mut self) -> Result<Message, ConversationError> {
        let (content, thinking) = match &self.state {
            StreamingState::ReceivingText { accumulated } => (accumulated.clone(), None),
            StreamingState::ReceivingThinking { accumulated } => {
                // If we ended in thinking state, we need to get both text and thinking
                (String::new(), Some(accumulated.clone()))
            }
            StreamingState::Completed => {
                return Err(ConversationError::NoStreamingInProgress);
            }
            _ => (String::new(), None),
        };

        let message = Message::assistant(content, thinking);
        self.messages.push(message.clone());
        self.transition_state(StreamingState::Completed)?;

        Ok(message)
    }

    /// Clear streaming state and return to Idle
    pub fn clear_streaming(&mut self) {
        self.state = StreamingState::Idle;
        self.record_transition("Idle (cleared)");
    }

    /// Handle streaming error
    pub fn handle_error(&mut self, error: String, recoverable: bool) {
        let _ = self.transition_state(StreamingState::Error { error, recoverable });
    }

    /// Convert messages to LLM format
    pub fn to_llm_messages(&self) -> Vec<LlmMessage> {
        self.messages
            .iter()
            .map(|msg| {
                let role = match msg.role.as_str() {
                    "user" => Role::User,
                    "assistant" => Role::Assistant,
                    "system" => Role::System,
                    _ => Role::User,
                };

                LlmMessage {
                    role,
                    content: MessageContent::Text(msg.content.clone()),
                    tool_call_id: None,
                }
            })
            .collect()
    }

    /// Restore from a chat session
    pub fn restore_from_session(&mut self, session: &ChatSession) {
        self.messages.clear();

        for session_msg in &session.messages {
            let message = Message {
                role: session_msg.role.clone(),
                content: session_msg.content.clone(),
                thinking: session_msg.thinking_content.clone(),
                timestamp: session_msg.timestamp,
            };
            self.messages.push(message);
        }

        self.clear_streaming();
    }

    /// Convert to session messages for persistence
    pub fn to_session_messages(&self) -> Vec<SessionMessage> {
        self.messages
            .iter()
            .map(|msg| SessionMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                timestamp: msg.timestamp,
                remote: false,
                provider: None,
                model: None,
                context_transient: false,
                thinking_content: msg.thinking.clone(),
                segments: Vec::new(),
                tool_call_id: None,
                tool_calls: Vec::new(),
            })
            .collect()
    }

    // Private helper methods

    fn transition_state(&mut self, new_state: StreamingState) -> Result<(), ConversationError> {
        // Validate transition
        let valid = match (&self.state, &new_state) {
            // From Idle
            (StreamingState::Idle, StreamingState::ReceivingText { .. }) => true,
            (StreamingState::Idle, StreamingState::ReceivingThinking { .. }) => true,

            // From ReceivingText
            (StreamingState::ReceivingText { .. }, StreamingState::ReceivingThinking { .. }) => {
                true
            }
            (StreamingState::ReceivingText { .. }, StreamingState::ToolCallPending { .. }) => true,
            (StreamingState::ReceivingText { .. }, StreamingState::Completed) => true,
            (StreamingState::ReceivingText { .. }, StreamingState::Error { .. }) => true,

            // From ReceivingThinking
            (StreamingState::ReceivingThinking { .. }, StreamingState::ReceivingText { .. }) => {
                true
            }
            (StreamingState::ReceivingThinking { .. }, StreamingState::Completed) => true,
            (StreamingState::ReceivingThinking { .. }, StreamingState::Error { .. }) => true,

            // From ToolCallPending
            (StreamingState::ToolCallPending { .. }, StreamingState::AwaitingToolResult { .. }) => {
                true
            }
            (StreamingState::ToolCallPending { .. }, StreamingState::Error { .. }) => true,

            // From AwaitingToolResult
            (StreamingState::AwaitingToolResult { .. }, StreamingState::ReceivingText { .. }) => {
                true
            }
            (StreamingState::AwaitingToolResult { .. }, StreamingState::Error { .. }) => true,

            // From Completed or Error back to Idle
            (StreamingState::Completed, StreamingState::ReceivingText { .. }) => true,
            (StreamingState::Error { .. }, StreamingState::ReceivingText { .. }) => true,

            // Invalid transitions
            _ => false,
        };

        if !valid {
            return Err(ConversationError::InvalidStateTransition {
                from: self.state.as_str().to_string(),
                to: new_state.as_str().to_string(),
            });
        }

        self.record_transition(new_state.as_str());
        self.state = new_state;
        Ok(())
    }

    fn record_transition(&mut self, to: &str) {
        self.state_transitions
            .push((Instant::now(), to.to_string()));

        // Keep only last 20 transitions for debugging
        if self.state_transitions.len() > 20 {
            self.state_transitions.drain(0..1);
        }
    }
}

impl Default for ConversationManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_conversation() {
        let mgr = ConversationManager::new();
        assert_eq!(mgr.message_count(), 0);
        assert!(matches!(mgr.state(), StreamingState::Idle));
    }

    #[test]
    fn test_add_messages() {
        let mut mgr = ConversationManager::new();

        mgr.add_user_message("Hello".to_string());
        mgr.add_assistant_message("Hi there".to_string(), None);
        mgr.add_system_message("System note".to_string());

        assert_eq!(mgr.message_count(), 3);
        assert_eq!(mgr.messages()[0].role, "user");
        assert_eq!(mgr.messages()[1].role, "assistant");
        assert_eq!(mgr.messages()[2].role, "system");
    }

    #[test]
    fn test_streaming_text() {
        let mut mgr = ConversationManager::new();

        mgr.start_streaming().unwrap();
        mgr.append_streaming_content("Hello").unwrap();
        mgr.append_streaming_content(" world").unwrap();

        assert_eq!(mgr.get_streaming_content(), Some("Hello world".to_string()));
    }

    #[test]
    fn test_finalize_streaming() {
        let mut mgr = ConversationManager::new();

        mgr.start_streaming().unwrap();
        mgr.append_streaming_content("Test message").unwrap();
        let msg = mgr.finalize_streaming().unwrap();

        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "Test message");
        assert_eq!(mgr.message_count(), 1);
    }

    #[test]
    fn test_clear_messages() {
        let mut mgr = ConversationManager::new();
        mgr.add_user_message("Test".to_string());
        assert_eq!(mgr.message_count(), 1);

        mgr.clear_messages();
        assert_eq!(mgr.message_count(), 0);
    }

    #[test]
    fn test_invalid_state_transition() {
        let mut mgr = ConversationManager::new();

        // Can't append content when not streaming
        let result = mgr.append_streaming_content("test");
        assert!(result.is_err());
    }

    #[test]
    fn test_to_llm_messages() {
        let mut mgr = ConversationManager::new();
        mgr.add_user_message("Hello".to_string());
        mgr.add_assistant_message("Hi".to_string(), None);

        let llm_messages = mgr.to_llm_messages();
        assert_eq!(llm_messages.len(), 2);
        assert!(matches!(llm_messages[0].role, Role::User));
        assert!(matches!(llm_messages[1].role, Role::Assistant));
    }
}
