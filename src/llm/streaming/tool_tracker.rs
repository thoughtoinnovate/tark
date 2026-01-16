//! Tool call tracking for streaming LLM responses
//!
//! This module provides a shared abstraction for tracking tool/function calls
//! across different provider streaming formats.

use std::collections::HashMap;

use crate::llm::StreamEvent;

/// Tracks in-progress tool calls across streaming events
///
/// Handles the complexity of mapping provider-specific IDs to canonical call IDs.
/// For example:
/// - OpenAI uses both `item_id` (e.g., "fc_xxx") and `call_id` (e.g., "call_xxx")
/// - Claude uses numeric indices for content blocks
/// - This tracker normalizes these to consistent call IDs for the rest of the system
#[derive(Debug, Default)]
pub struct ToolCallTracker {
    /// Canonical call_id -> (name, accumulated_arguments)
    calls: HashMap<String, (String, String)>,
    /// Provider-specific ID -> canonical call_id mapping
    /// (e.g., OpenAI item_id -> call_id, Claude index -> call_id)
    id_mapping: HashMap<String, String>,
}

impl ToolCallTracker {
    /// Create a new tool call tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new tool call and return a ToolCallStart event
    ///
    /// # Arguments
    /// - `call_id`: The canonical call ID (e.g., "call_abc123")
    /// - `name`: The function/tool name
    /// - `provider_id`: Optional provider-specific ID that will be mapped to call_id
    ///   (e.g., OpenAI's item_id like "fc_xxx", or Claude's index)
    ///
    /// # Returns
    /// `StreamEvent::ToolCallStart` event to emit to callbacks
    pub fn start_call(
        &mut self,
        call_id: &str,
        name: &str,
        provider_id: Option<&str>,
    ) -> StreamEvent {
        // Register the call
        self.calls
            .insert(call_id.to_string(), (name.to_string(), String::new()));

        // If provider_id is different from call_id, create mapping
        if let Some(pid) = provider_id {
            if pid != call_id {
                self.id_mapping.insert(pid.to_string(), call_id.to_string());
            }
        }

        StreamEvent::ToolCallStart {
            id: call_id.to_string(),
            name: name.to_string(),
            thought_signature: None,
        }
    }

    /// Append arguments delta and return a ToolCallDelta event
    ///
    /// Accepts either the canonical call_id or a provider_id that was registered
    /// via `start_call`.
    ///
    /// # Returns
    /// - `Some(StreamEvent::ToolCallDelta)` if the call exists
    /// - `None` if the ID is not recognized (call not started)
    pub fn append_args(&mut self, id: &str, delta: &str) -> Option<StreamEvent> {
        // Resolve provider_id to call_id if needed
        let call_id = self.id_mapping.get(id).map(|s| s.as_str()).unwrap_or(id);

        // Append to the call's arguments
        if let Some((_, args)) = self.calls.get_mut(call_id) {
            args.push_str(delta);
            Some(StreamEvent::ToolCallDelta {
                id: call_id.to_string(),
                arguments_delta: delta.to_string(),
            })
        } else {
            tracing::warn!("Received tool call delta for unknown ID: {}", id);
            None
        }
    }

    /// Mark a tool call as complete and return a ToolCallComplete event
    ///
    /// Accepts either the canonical call_id or a provider_id that was registered
    /// via `start_call`.
    ///
    /// # Returns
    /// - `Some(StreamEvent::ToolCallComplete)` if the call exists
    /// - `None` if the ID is not recognized
    pub fn complete_call(&mut self, id: &str) -> Option<StreamEvent> {
        // Resolve provider_id to call_id if needed
        let call_id = self.id_mapping.get(id).map(|s| s.as_str()).unwrap_or(id);

        if self.calls.contains_key(call_id) {
            Some(StreamEvent::ToolCallComplete {
                id: call_id.to_string(),
            })
        } else {
            tracing::warn!("Received tool call complete for unknown ID: {}", id);
            None
        }
    }

    /// Get all tracked calls (for building final response)
    ///
    /// Consumes the tracker and returns a map of call_id -> (name, accumulated_args)
    pub fn into_calls(self) -> HashMap<String, (String, String)> {
        self.calls
    }

    /// Check if a call exists (by canonical or provider ID)
    pub fn contains(&self, id: &str) -> bool {
        let call_id = self.id_mapping.get(id).map(|s| s.as_str()).unwrap_or(id);
        self.calls.contains_key(call_id)
    }

    /// Get the number of tracked calls
    pub fn len(&self) -> usize {
        self.calls.len()
    }

    /// Check if the tracker is empty
    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_flow() {
        let mut tracker = ToolCallTracker::new();

        // Start a call
        let event = tracker.start_call("call_123", "search", None);
        match event {
            StreamEvent::ToolCallStart { id, name, .. } => {
                assert_eq!(id, "call_123");
                assert_eq!(name, "search");
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Append args
        let event = tracker.append_args("call_123", "{\"query\":").unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call_123");
                assert_eq!(arguments_delta, "{\"query\":");
            }
            _ => panic!("Expected ToolCallDelta"),
        }

        let event = tracker.append_args("call_123", "\"test\"}").unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call_123");
                assert_eq!(arguments_delta, "\"test\"}");
            }
            _ => panic!("Expected ToolCallDelta"),
        }

        // Complete
        let event = tracker.complete_call("call_123").unwrap();
        match event {
            StreamEvent::ToolCallComplete { id } => {
                assert_eq!(id, "call_123");
            }
            _ => panic!("Expected ToolCallComplete"),
        }

        // Verify accumulated args
        let calls = tracker.into_calls();
        assert_eq!(calls.len(), 1);
        let (name, args) = calls.get("call_123").unwrap();
        assert_eq!(name, "search");
        assert_eq!(args, "{\"query\":\"test\"}");
    }

    #[test]
    fn test_provider_id_mapping() {
        let mut tracker = ToolCallTracker::new();

        // OpenAI-style: item_id "fc_xxx" maps to call_id "call_xxx"
        let event = tracker.start_call("call_456", "get_weather", Some("fc_456"));
        match event {
            StreamEvent::ToolCallStart { id, name, .. } => {
                assert_eq!(id, "call_456");
                assert_eq!(name, "get_weather");
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Use provider_id in subsequent calls
        let event = tracker.append_args("fc_456", "{\"city\":").unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call_456"); // Resolved to canonical call_id
                assert_eq!(arguments_delta, "{\"city\":");
            }
            _ => panic!("Expected ToolCallDelta"),
        }

        let event = tracker.complete_call("fc_456").unwrap();
        match event {
            StreamEvent::ToolCallComplete { id } => {
                assert_eq!(id, "call_456");
            }
            _ => panic!("Expected ToolCallComplete"),
        }
    }

    #[test]
    fn test_multiple_calls() {
        let mut tracker = ToolCallTracker::new();

        // Start two calls
        tracker.start_call("call_1", "tool_a", None);
        tracker.start_call("call_2", "tool_b", None);

        // Append to both
        tracker.append_args("call_1", "args1");
        tracker.append_args("call_2", "args2");

        // Verify both tracked
        let calls = tracker.into_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls.get("call_1").unwrap().1, "args1");
        assert_eq!(calls.get("call_2").unwrap().1, "args2");
    }

    #[test]
    fn test_unknown_id_returns_none() {
        let mut tracker = ToolCallTracker::new();

        // Append to non-existent call
        assert!(tracker.append_args("unknown_id", "data").is_none());

        // Complete non-existent call
        assert!(tracker.complete_call("unknown_id").is_none());
    }

    #[test]
    fn test_contains() {
        let mut tracker = ToolCallTracker::new();

        tracker.start_call("call_123", "test", Some("provider_id"));

        // Check by canonical ID
        assert!(tracker.contains("call_123"));

        // Check by provider ID
        assert!(tracker.contains("provider_id"));

        // Check unknown ID
        assert!(!tracker.contains("unknown"));
    }

    #[test]
    fn test_len_and_empty() {
        let mut tracker = ToolCallTracker::new();

        assert!(tracker.is_empty());
        assert_eq!(tracker.len(), 0);

        tracker.start_call("call_1", "test", None);
        assert!(!tracker.is_empty());
        assert_eq!(tracker.len(), 1);

        tracker.start_call("call_2", "test2", None);
        assert_eq!(tracker.len(), 2);
    }

    #[test]
    fn test_claude_style_index_mapping() {
        let mut tracker = ToolCallTracker::new();

        // Claude uses numeric indices as content block IDs
        // We generate a call_id and map the index to it
        let index = "0";
        let call_id = format!("call_block_{}", index);

        tracker.start_call(&call_id, "search", Some(index));

        // Use index in delta events
        let event = tracker.append_args(index, "{\"query\":\"test\"}").unwrap();
        match event {
            StreamEvent::ToolCallDelta { id, .. } => {
                assert_eq!(id, call_id);
            }
            _ => panic!("Expected ToolCallDelta"),
        }
    }

    #[test]
    fn test_empty_delta() {
        let mut tracker = ToolCallTracker::new();

        tracker.start_call("call_1", "test", None);

        // Empty delta should still return an event
        let event = tracker.append_args("call_1", "").unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                arguments_delta, ..
            } => {
                assert_eq!(arguments_delta, "");
            }
            _ => panic!("Expected ToolCallDelta"),
        }
    }
}
