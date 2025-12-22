//! Conversation context management

use crate::llm::{ContentPart, Message, MessageContent, Role, ToolCall};

/// Default max tokens (conservative estimate for most models)
const DEFAULT_MAX_CONTEXT_TOKENS: usize = 100_000;

/// Max tokens for a single tool result (to prevent one file from filling context)
const MAX_TOOL_RESULT_TOKENS: usize = 8_000;

/// Manages conversation history and context
pub struct ConversationContext {
    messages: Vec<Message>,
    max_messages: usize,
    max_context_tokens: usize,
}

impl ConversationContext {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            max_messages: 100,
            max_context_tokens: DEFAULT_MAX_CONTEXT_TOKENS,
        }
    }

    pub fn with_max_messages(mut self, max: usize) -> Self {
        self.max_messages = max;
        self
    }

    pub fn with_max_context_tokens(mut self, max: usize) -> Self {
        self.max_context_tokens = max;
        self
    }

    /// Add a system message
    pub fn add_system(&mut self, content: impl Into<String>) {
        self.messages.push(Message::system(content));
        self.trim();
    }

    /// Add a user message
    pub fn add_user(&mut self, content: impl Into<String>) {
        self.messages.push(Message::user(content));
        self.trim();
    }

    /// Add an assistant message
    pub fn add_assistant(&mut self, content: impl Into<String>) {
        self.messages.push(Message::assistant(content));
        self.trim();
    }

    /// Add an assistant message with tool calls (required before tool results for OpenAI)
    pub fn add_assistant_tool_calls(&mut self, tool_calls: &[ToolCall]) {
        let parts: Vec<ContentPart> = tool_calls
            .iter()
            .map(|tc| ContentPart::ToolUse {
                id: tc.id.clone(),
                name: tc.name.clone(),
                input: tc.arguments.clone(),
            })
            .collect();

        self.messages.push(Message {
            role: Role::Assistant,
            content: MessageContent::Parts(parts),
            tool_call_id: None,
        });
        self.trim();
    }

    /// Add a tool result (auto-truncates if too large)
    pub fn add_tool_result(&mut self, tool_call_id: impl Into<String>, result: impl Into<String>) {
        let result_str = result.into();
        let truncated = Self::truncate_if_needed(&result_str, MAX_TOOL_RESULT_TOKENS);
        self.messages
            .push(Message::tool_result(tool_call_id, truncated));
        self.trim();
        self.trim_by_tokens();
    }

    /// Truncate text if it exceeds token limit
    fn truncate_if_needed(text: &str, max_tokens: usize) -> String {
        let estimated_tokens = Self::estimate_tokens(text);
        if estimated_tokens <= max_tokens {
            return text.to_string();
        }

        // Truncate to approximately max_tokens
        // ~4 chars per token is a rough estimate
        let max_chars = max_tokens * 4;
        let truncated: String = text.chars().take(max_chars).collect();

        format!(
            "{}\n\n... [TRUNCATED: Content exceeded {} tokens. Use grep or read specific sections for more detail.]",
            truncated,
            max_tokens
        )
    }

    /// Estimate tokens in text (~4 chars per token for English)
    pub fn estimate_tokens(text: &str) -> usize {
        (text.len() + 3) / 4 // Round up
    }

    /// Estimate total tokens in context
    pub fn estimate_total_tokens(&self) -> usize {
        self.messages
            .iter()
            .map(|m| match &m.content {
                MessageContent::Text(t) => Self::estimate_tokens(t),
                MessageContent::Parts(parts) => parts
                    .iter()
                    .map(|p| match p {
                        ContentPart::Text { text } => Self::estimate_tokens(text),
                        ContentPart::ToolUse { input, .. } => {
                            Self::estimate_tokens(&input.to_string())
                        }
                        ContentPart::ToolResult { content, .. } => Self::estimate_tokens(content),
                    })
                    .sum(),
            })
            .sum()
    }

    /// Check if context is near limit
    pub fn is_near_limit(&self) -> bool {
        let used = self.estimate_total_tokens();
        used > (self.max_context_tokens * 80 / 100) // 80% threshold
    }

    /// Get context usage as percentage
    pub fn usage_percentage(&self) -> usize {
        let used = self.estimate_total_tokens();
        (used * 100) / self.max_context_tokens
    }

    /// Get all messages
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
    }

    /// Update the system prompt (replaces the first system message, or adds one if none exists)
    pub fn update_system_prompt(&mut self, new_prompt: &str) {
        // Find and update the first system message
        if let Some(system_msg) = self.messages.iter_mut().find(|m| m.role == Role::System) {
            system_msg.content = MessageContent::Text(new_prompt.to_string());
        } else {
            // No system message found, add one at the beginning
            self.messages.insert(0, Message::system(new_prompt));
        }
    }

    /// Trim messages to max size, keeping system messages
    fn trim(&mut self) {
        if self.messages.len() <= self.max_messages {
            return;
        }

        // Keep system messages and most recent messages
        let system_messages: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| m.role == Role::System)
            .cloned()
            .collect();

        let non_system: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .cloned()
            .collect();

        let keep_count = self.max_messages.saturating_sub(system_messages.len());
        let skip_count = non_system.len().saturating_sub(keep_count);

        self.messages = system_messages;
        self.messages
            .extend(non_system.into_iter().skip(skip_count));
    }

    /// Trim oldest non-system messages if total tokens exceed limit
    fn trim_by_tokens(&mut self) {
        while self.estimate_total_tokens() > self.max_context_tokens && self.messages.len() > 2 {
            // Find first non-system message to remove
            if let Some(idx) = self.messages.iter().position(|m| m.role != Role::System) {
                // Don't remove if it's the only non-system message
                let non_system_count = self
                    .messages
                    .iter()
                    .filter(|m| m.role != Role::System)
                    .count();
                if non_system_count <= 1 {
                    break;
                }
                self.messages.remove(idx);
                tracing::debug!(
                    "Trimmed message at index {} to stay within token limit",
                    idx
                );
            } else {
                break;
            }
        }
    }

    /// Compact context by replacing old messages with a summary
    pub fn compact_with_summary(&mut self, summary: &str, keep_recent: usize) {
        // Extract system message
        let system_msg = self
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .cloned();

        // Get recent non-system messages to keep
        let non_system: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .cloned()
            .collect();

        let recent_start = non_system.len().saturating_sub(keep_recent);
        let recent_messages: Vec<Message> = non_system.into_iter().skip(recent_start).collect();

        // Rebuild context: system + summary + recent
        self.messages.clear();

        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }

        // Add summary as a system-like context message
        let summary_msg = format!(
            "[CONVERSATION SUMMARY - Auto-compacted to save context]\n{}\n[END SUMMARY]",
            summary
        );
        self.messages.push(Message::user(summary_msg));
        self.messages.push(Message::assistant(
            "I understand the context from the summary above. How can I continue helping you?",
        ));

        // Add recent messages
        self.messages.extend(recent_messages);

        tracing::info!(
            "Compacted context: {} messages remaining, ~{} tokens",
            self.messages.len(),
            self.estimate_total_tokens()
        );
    }

    /// Trim to keep only recent messages (fallback when summarization fails)
    pub fn trim_to_recent(&mut self, keep_recent: usize) {
        // Extract system message
        let system_msg = self
            .messages
            .iter()
            .find(|m| m.role == Role::System)
            .cloned();

        // Get recent non-system messages
        let non_system: Vec<Message> = self
            .messages
            .iter()
            .filter(|m| m.role != Role::System)
            .cloned()
            .collect();

        let recent_start = non_system.len().saturating_sub(keep_recent);
        let recent_messages: Vec<Message> = non_system.into_iter().skip(recent_start).collect();

        // Rebuild
        self.messages.clear();

        if let Some(sys) = system_msg {
            self.messages.push(sys);
        }

        // Add a note about lost context
        self.messages.push(Message::user(
            "[Note: Earlier conversation was trimmed to save context space]",
        ));
        self.messages.push(Message::assistant(
            "I understand some earlier context was trimmed. I'll continue with what I know. How can I help?"
        ));

        self.messages.extend(recent_messages);

        tracing::info!("Trimmed context to {} recent messages", self.messages.len());
    }
}

impl Default for ConversationContext {
    fn default() -> Self {
        Self::new()
    }
}
