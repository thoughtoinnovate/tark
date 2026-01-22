//! Core traits for the domain layer
//!
//! These traits define the interfaces that domain components depend on,
//! allowing infrastructure to be injected and tests to use mocks.

use crate::storage::{ChatSession, SessionMeta};
use anyhow::Result;

/// Tokenizer for counting tokens in text and messages
///
/// Different LLM providers use different tokenizers:
/// - OpenAI uses tiktoken (cl100k_base for GPT-4)
/// - Claude has its own tokenizer
/// - Gemini uses a different tokenizer
/// - Local models vary
///
/// This trait allows context tracking to be provider-agnostic.
pub trait Tokenizer: Send + Sync {
    /// Count tokens in a text string
    fn count_tokens(&self, text: &str) -> usize;

    /// Count tokens for a message (includes role and formatting overhead)
    fn count_message_tokens(&self, _role: &str, content: &str) -> usize {
        // Default implementation adds overhead for message structure
        // {"role": "user", "content": "..."}
        let overhead = 20; // Approximate JSON structure overhead
        overhead + self.count_tokens(content)
    }

    /// Get the maximum context window size for this tokenizer
    fn max_context_tokens(&self) -> usize;
}

/// Session storage abstraction
///
/// Allows domain layer to persist sessions without depending on TarkStorage directly.
pub trait SessionStore: Send + Sync {
    /// Save a session to storage
    fn save(&self, session: &ChatSession) -> Result<()>;

    /// Load a session by ID
    fn load(&self, id: &str) -> Result<ChatSession>;

    /// List all available sessions
    fn list(&self) -> Result<Vec<SessionMeta>>;

    /// Delete a session by ID
    fn delete(&self, id: &str) -> Result<()>;

    /// Create a new empty session
    fn create_new(&self) -> Result<ChatSession>;

    /// Set the current session as active
    fn set_current(&self, session_id: &str) -> Result<()>;
}
