//! Unified Context Tracker - Single source of truth for context window management
//!
//! This module provides a unified view of all token sources in the context window:
//! - System prompt tokens
//! - Conversation history tokens
//! - Tool schema tokens
//! - Attachment tokens
//!
//! It provides accurate, real-time tracking of context usage.

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Breakdown of context token usage by source
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ContextBreakdown {
    /// Tokens used by the system prompt
    pub system_prompt: usize,
    /// Tokens used by conversation history (user + assistant messages)
    pub conversation_history: usize,
    /// Tokens reserved for tool schemas
    pub tool_schemas: usize,
    /// Tokens used by attached files
    pub attachments: usize,
    /// Total tokens used (sum of all sources)
    pub total: usize,
    /// Maximum tokens allowed for this model
    pub max_tokens: usize,
}

impl ContextBreakdown {
    /// Create a new context breakdown
    pub fn new(
        system_prompt: usize,
        conversation_history: usize,
        tool_schemas: usize,
        attachments: usize,
        max_tokens: usize,
    ) -> Self {
        let total = system_prompt + conversation_history + tool_schemas + attachments;
        Self {
            system_prompt,
            conversation_history,
            tool_schemas,
            attachments,
            total,
            max_tokens,
        }
    }

    /// Get usage as a percentage (0.0 - 100.0)
    pub fn usage_percent(&self) -> f32 {
        if self.max_tokens == 0 {
            return 0.0;
        }
        (self.total as f32 / self.max_tokens as f32) * 100.0
    }

    /// Get available tokens remaining
    pub fn available(&self) -> usize {
        self.max_tokens.saturating_sub(self.total)
    }

    /// Check if context should be compacted (80% threshold)
    pub fn should_compact(&self) -> bool {
        self.usage_percent() >= 80.0
    }

    /// Check if context is critically full (95% threshold)
    pub fn is_critical(&self) -> bool {
        self.usage_percent() >= 95.0
    }

    /// Check if we've exceeded the context limit
    pub fn is_exceeded(&self) -> bool {
        self.total > self.max_tokens
    }

    /// Get the compaction threshold in tokens (80% of max)
    pub fn compaction_threshold(&self) -> usize {
        (self.max_tokens as f32 * 0.80) as usize
    }

    /// Recalculate total from components
    pub fn recalculate_total(&mut self) {
        self.total =
            self.system_prompt + self.conversation_history + self.tool_schemas + self.attachments;
    }
}

/// Unified context tracker - single source of truth for token usage
///
/// This tracker aggregates token counts from all sources and provides
/// a unified interface for context management decisions.
pub struct ContextTracker {
    /// Current context breakdown
    breakdown: RwLock<ContextBreakdown>,
    /// Compaction threshold (0.0-1.0, default 0.80)
    compaction_threshold: f32,
}

impl Default for ContextTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextTracker {
    /// Create a new context tracker
    pub fn new() -> Self {
        Self {
            breakdown: RwLock::new(ContextBreakdown::default()),
            compaction_threshold: 0.80,
        }
    }

    /// Create a new context tracker wrapped in Arc for sharing
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }

    /// Update the context breakdown from all sources
    pub async fn update(
        &self,
        system_prompt_tokens: usize,
        conversation_tokens: usize,
        tool_schema_tokens: usize,
        attachment_tokens: usize,
        max_tokens: usize,
    ) {
        let mut breakdown = self.breakdown.write().await;
        breakdown.system_prompt = system_prompt_tokens;
        breakdown.conversation_history = conversation_tokens;
        breakdown.tool_schemas = tool_schema_tokens;
        breakdown.attachments = attachment_tokens;
        breakdown.max_tokens = max_tokens;
        breakdown.recalculate_total();
    }

    /// Update only the attachment tokens (for when attachments change)
    pub async fn update_attachments(&self, attachment_tokens: usize) {
        let mut breakdown = self.breakdown.write().await;
        breakdown.attachments = attachment_tokens;
        breakdown.recalculate_total();
    }

    /// Update only the conversation history tokens
    pub async fn update_conversation(&self, conversation_tokens: usize) {
        let mut breakdown = self.breakdown.write().await;
        breakdown.conversation_history = conversation_tokens;
        breakdown.recalculate_total();
    }

    /// Update the max tokens (e.g., on model switch)
    pub async fn update_max_tokens(&self, max_tokens: usize) {
        let mut breakdown = self.breakdown.write().await;
        breakdown.max_tokens = max_tokens;
    }

    /// Get a copy of the current context breakdown
    pub async fn get_breakdown(&self) -> ContextBreakdown {
        self.breakdown.read().await.clone()
    }

    /// Check if compaction should be triggered
    pub async fn should_compact(&self) -> bool {
        let breakdown = self.breakdown.read().await;
        let usage_ratio = if breakdown.max_tokens == 0 {
            0.0
        } else {
            breakdown.total as f32 / breakdown.max_tokens as f32
        };
        usage_ratio >= self.compaction_threshold
    }

    /// Check if context limit would be exceeded after adding tokens
    pub async fn would_exceed(&self, additional_tokens: usize) -> bool {
        let breakdown = self.breakdown.read().await;
        breakdown.total + additional_tokens > breakdown.max_tokens
    }

    /// Get current usage percentage
    pub async fn usage_percent(&self) -> f32 {
        self.breakdown.read().await.usage_percent()
    }

    /// Get available tokens
    pub async fn available(&self) -> usize {
        self.breakdown.read().await.available()
    }

    /// Set the compaction threshold (0.0-1.0)
    pub fn set_compaction_threshold(&mut self, threshold: f32) {
        self.compaction_threshold = threshold.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_breakdown_new() {
        let breakdown = ContextBreakdown::new(500, 3000, 200, 100, 8000);
        assert_eq!(breakdown.system_prompt, 500);
        assert_eq!(breakdown.conversation_history, 3000);
        assert_eq!(breakdown.tool_schemas, 200);
        assert_eq!(breakdown.attachments, 100);
        assert_eq!(breakdown.total, 3800);
        assert_eq!(breakdown.max_tokens, 8000);
    }

    #[tokio::test]
    async fn test_usage_percent() {
        let breakdown = ContextBreakdown::new(500, 3500, 0, 0, 8000);
        assert!((breakdown.usage_percent() - 50.0).abs() < 0.1);
    }

    #[tokio::test]
    async fn test_should_compact() {
        // Under 80%
        let under = ContextBreakdown::new(500, 5500, 0, 0, 8000); // 75%
        assert!(!under.should_compact());

        // At 80%
        let at = ContextBreakdown::new(500, 5900, 0, 0, 8000); // 80%
        assert!(at.should_compact());

        // Over 80%
        let over = ContextBreakdown::new(500, 6500, 0, 0, 8000); // 87.5%
        assert!(over.should_compact());
    }

    #[tokio::test]
    async fn test_context_tracker_update() {
        let tracker = ContextTracker::new();
        tracker.update(500, 3000, 200, 100, 8000).await;

        let breakdown = tracker.get_breakdown().await;
        assert_eq!(breakdown.total, 3800);
        assert_eq!(breakdown.max_tokens, 8000);
    }

    #[tokio::test]
    async fn test_tracker_should_compact() {
        let tracker = ContextTracker::new();

        // Under threshold
        tracker.update(500, 5500, 0, 0, 8000).await; // 75%
        assert!(!tracker.should_compact().await);

        // Over threshold
        tracker.update(500, 6500, 0, 0, 8000).await; // 87.5%
        assert!(tracker.should_compact().await);
    }

    #[tokio::test]
    async fn test_tracker_update_attachments() {
        let tracker = ContextTracker::new();
        tracker.update(500, 3000, 200, 0, 8000).await;

        assert_eq!(tracker.get_breakdown().await.total, 3700);

        tracker.update_attachments(500).await;
        assert_eq!(tracker.get_breakdown().await.total, 4200);
        assert_eq!(tracker.get_breakdown().await.attachments, 500);
    }

    #[tokio::test]
    async fn test_would_exceed() {
        let tracker = ContextTracker::new();
        tracker.update(500, 7000, 200, 0, 8000).await; // 7700 used

        assert!(!tracker.would_exceed(200).await); // 7900 < 8000
        assert!(tracker.would_exceed(500).await); // 8200 > 8000
    }

    #[tokio::test]
    async fn test_available() {
        let tracker = ContextTracker::new();
        tracker.update(500, 3000, 200, 300, 8000).await;

        assert_eq!(tracker.available().await, 4000); // 8000 - 4000
    }

    #[test]
    fn test_breakdown_default() {
        let breakdown = ContextBreakdown::default();
        assert_eq!(breakdown.total, 0);
        assert_eq!(breakdown.max_tokens, 0);
        assert!(!breakdown.should_compact()); // 0/0 = 0%
    }
}
