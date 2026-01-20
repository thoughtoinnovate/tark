//! Context Manager - Token counting and context window management
//!
//! Handles:
//! - Token counting for messages using injected Tokenizer
//! - Context window tracking
//! - Auto-compaction decisions
//! - Context budget management (text + images + tool schema tokens)

use super::errors::ContextError;
use super::traits::Tokenizer;
use std::sync::Arc;

/// Result of a compaction operation
#[derive(Debug, Clone)]
pub struct CompactResult {
    /// Number of messages removed
    pub messages_removed: usize,
    /// Number of tokens freed
    pub tokens_freed: usize,
    /// New token count after compaction
    pub new_token_count: usize,
}

/// Compaction strategy for managing context window
#[derive(Debug, Clone, Copy)]
pub enum CompactionStrategy {
    /// Keep last N messages (sliding window)
    SlidingWindow { keep_last: usize },
    /// Keep messages until under threshold
    KeepUntilThreshold { target_percent: f32 },
    /// Hybrid: keep important messages + recent N
    HybridImportance { keep_recent: usize },
}

impl Default for CompactionStrategy {
    fn default() -> Self {
        // Default: keep last 20 messages
        CompactionStrategy::SlidingWindow { keep_last: 20 }
    }
}

/// Manages LLM context window, token counting, and compaction
pub struct ContextManager {
    /// Tokenizer for counting tokens
    tokenizer: Arc<dyn Tokenizer>,
    /// Current estimated token count
    current_tokens: usize,
    /// Threshold at which to trigger compaction (0.0-1.0)
    compaction_threshold: f32,
    /// Reserved tokens for tool schemas
    reserved_for_tools: usize,
    /// Reserved tokens for response
    reserved_for_response: usize,
    /// Current compaction strategy
    strategy: CompactionStrategy,
}

impl ContextManager {
    /// Create a new context manager with injected tokenizer
    pub fn new(tokenizer: Arc<dyn Tokenizer>) -> Self {
        Self {
            tokenizer,
            current_tokens: 0,
            compaction_threshold: 0.85, // Compact at 85% full
            reserved_for_tools: 0,
            reserved_for_response: 2000, // Reserve 2k tokens for response
            strategy: CompactionStrategy::default(),
        }
    }

    /// Get the maximum context tokens from tokenizer
    pub fn max_tokens(&self) -> usize {
        self.tokenizer.max_context_tokens()
    }

    /// Count tokens in a text string
    pub fn count_tokens(&self, text: &str) -> usize {
        self.tokenizer.count_tokens(text)
    }

    /// Count tokens for a message
    pub fn count_message_tokens(&self, role: &str, content: &str) -> usize {
        self.tokenizer.count_message_tokens(role, content)
    }

    /// Update current token count
    pub fn set_current_tokens(&mut self, tokens: usize) {
        self.current_tokens = tokens;
    }

    /// Get current token count
    pub fn current_tokens(&self) -> usize {
        self.current_tokens
    }

    /// Reserve tokens for tool schemas
    pub fn reserve_tool_tokens(&mut self, tool_schema_tokens: usize) {
        self.reserved_for_tools = tool_schema_tokens;
    }

    /// Set reserved tokens for response generation
    pub fn set_response_reserve(&mut self, tokens: usize) {
        self.reserved_for_response = tokens;
    }

    /// Check if context window should be compacted
    pub fn should_compact(&self) -> bool {
        if self.max_tokens() == 0 {
            return false;
        }
        let usage_ratio = self.current_tokens as f32 / self.max_tokens() as f32;
        usage_ratio >= self.compaction_threshold
    }

    /// Get current context usage percentage (0-100)
    pub fn usage_percent(&self) -> f32 {
        if self.max_tokens() == 0 {
            return 0.0;
        }
        (self.current_tokens as f32 / self.max_tokens() as f32) * 100.0
    }

    /// Get available tokens for response
    pub fn available_for_response(&self) -> usize {
        let reserved_total = self.reserved_for_tools + self.reserved_for_response;
        let used_with_reserve = self.current_tokens.saturating_add(reserved_total);
        self.max_tokens().saturating_sub(used_with_reserve)
    }

    /// Set compaction strategy
    pub fn set_strategy(&mut self, strategy: CompactionStrategy) {
        self.strategy = strategy;
    }

    /// Calculate how many messages to keep for compaction
    ///
    /// Returns the number of messages to keep from the end of the message list.
    /// System messages (first message) are always preserved separately.
    pub fn calculate_keep_count(&self, total_messages: usize) -> usize {
        match self.strategy {
            CompactionStrategy::SlidingWindow { keep_last } => {
                // Keep at least keep_last messages, but not more than we have
                keep_last.min(total_messages)
            }
            CompactionStrategy::KeepUntilThreshold { target_percent } => {
                // Calculate target token count
                let target_tokens = (self.max_tokens() as f32 * target_percent) as usize;

                // Estimate: keep enough messages to stay under target
                // Assume average message is current_tokens / total_messages
                if total_messages == 0 {
                    return 0;
                }
                let avg_tokens_per_message = self.current_tokens / total_messages;
                if avg_tokens_per_message == 0 {
                    return total_messages;
                }

                let messages_to_keep = target_tokens / avg_tokens_per_message;
                messages_to_keep.min(total_messages)
            }
            CompactionStrategy::HybridImportance { keep_recent } => {
                // For now, just keep recent messages
                // Future: also keep messages with tool calls or key decisions
                keep_recent.min(total_messages)
            }
        }
    }

    /// Compact context by removing older messages
    ///
    /// Returns information about the compaction operation.
    /// The caller is responsible for actually removing messages from their store.
    pub fn compact(
        &mut self,
        total_messages: usize,
        estimated_tokens_per_message: usize,
    ) -> Result<CompactResult, ContextError> {
        if total_messages == 0 {
            return Err(ContextError::CompactionFailed(
                "No messages to compact".to_string(),
            ));
        }

        let keep_count = self.calculate_keep_count(total_messages);

        // Always keep at least 1 message (system prompt or latest)
        let keep_count = keep_count.max(1);

        if keep_count >= total_messages {
            return Err(ContextError::CompactionFailed(
                "No messages can be removed".to_string(),
            ));
        }

        let messages_removed = total_messages - keep_count;
        let tokens_freed = messages_removed * estimated_tokens_per_message;
        let new_token_count = self.current_tokens.saturating_sub(tokens_freed);

        // Update internal state
        self.current_tokens = new_token_count;

        Ok(CompactResult {
            messages_removed,
            tokens_freed,
            new_token_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tokenizer::ApproximateTokenizer;

    fn create_test_manager() -> ContextManager {
        let tokenizer = Arc::new(ApproximateTokenizer::new(8000));
        ContextManager::new(tokenizer)
    }

    #[test]
    fn test_context_manager_creation() {
        let mgr = create_test_manager();
        assert_eq!(mgr.max_tokens(), 8000);
        assert_eq!(mgr.current_tokens(), 0);
    }

    #[test]
    fn test_token_counting() {
        let mgr = create_test_manager();

        // Test text token counting
        let tokens = mgr.count_tokens("Hello, world!"); // 13 chars
        assert!(tokens > 0);

        // Test message token counting (includes overhead)
        let msg_tokens = mgr.count_message_tokens("user", "Hello");
        assert!(msg_tokens > mgr.count_tokens("Hello"));
    }

    #[test]
    fn test_should_compact() {
        let mut mgr = create_test_manager();

        // Under threshold
        mgr.set_current_tokens(6000); // 75%
        assert!(!mgr.should_compact());

        // At threshold
        mgr.set_current_tokens(6800); // 85%
        assert!(mgr.should_compact());

        // Over threshold
        mgr.set_current_tokens(7200); // 90%
        assert!(mgr.should_compact());
    }

    #[test]
    fn test_usage_percent() {
        let mut mgr = create_test_manager();
        mgr.set_current_tokens(4000);
        assert!((mgr.usage_percent() - 50.0).abs() < 0.1);
    }

    #[test]
    fn test_available_for_response() {
        let mut mgr = create_test_manager();
        mgr.set_current_tokens(5000);
        mgr.set_response_reserve(1000);
        mgr.reserve_tool_tokens(500);

        let available = mgr.available_for_response();
        // 8000 - 5000 - 1000 - 500 = 1500
        assert_eq!(available, 1500);
    }

    #[test]
    fn test_sliding_window_strategy() {
        let mut mgr = create_test_manager();
        mgr.set_strategy(CompactionStrategy::SlidingWindow { keep_last: 10 });

        assert_eq!(mgr.calculate_keep_count(20), 10);
        assert_eq!(mgr.calculate_keep_count(5), 5); // Don't over-keep
    }

    #[test]
    fn test_compact_operation() {
        let mut mgr = create_test_manager();
        mgr.set_current_tokens(6000);
        mgr.set_strategy(CompactionStrategy::SlidingWindow { keep_last: 5 });

        let result = mgr.compact(10, 600).unwrap();

        assert_eq!(result.messages_removed, 5);
        assert_eq!(result.tokens_freed, 3000); // 5 * 600
        assert_eq!(result.new_token_count, 3000); // 6000 - 3000
        assert_eq!(mgr.current_tokens(), 3000);
    }

    #[test]
    fn test_compact_no_messages() {
        let mut mgr = create_test_manager();
        let result = mgr.compact(0, 100);
        assert!(result.is_err());
    }

    #[test]
    fn test_compact_preserves_minimum() {
        let mut mgr = create_test_manager();
        mgr.set_strategy(CompactionStrategy::SlidingWindow { keep_last: 100 });

        let result = mgr.compact(5, 100);
        assert!(result.is_err()); // Can't remove if we want to keep more than we have
    }
}
