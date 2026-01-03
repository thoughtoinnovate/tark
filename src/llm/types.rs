//! Shared types for LLM providers

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Role in a conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A message in a conversation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

/// Content of a message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Parts(Vec<ContentPart>),
}

impl MessageContent {
    pub fn as_text(&self) -> Option<&str> {
        match self {
            MessageContent::Text(s) => Some(s),
            MessageContent::Parts(parts) => parts.iter().find_map(|p| {
                if let ContentPart::Text { text } = p {
                    Some(text.as_str())
                } else {
                    None
                }
            }),
        }
    }
}

/// Part of a multi-part message
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: MessageContent::Text(content.into()),
            tool_call_id: None,
        }
    }

    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: Role::Tool,
            content: MessageContent::Text(content.into()),
            tool_call_id: Some(tool_call_id.into()),
        }
    }
}

/// Response from an LLM
#[derive(Debug, Clone)]
pub enum LlmResponse {
    /// Plain text response
    Text {
        text: String,
        usage: Option<TokenUsage>,
    },
    /// Tool calls requested by the model
    ToolCalls {
        calls: Vec<ToolCall>,
        usage: Option<TokenUsage>,
    },
    /// Mixed response with text and tool calls
    Mixed {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
        usage: Option<TokenUsage>,
    },
}

impl LlmResponse {
    pub fn text(&self) -> Option<&str> {
        match self {
            LlmResponse::Text { text, .. } => Some(text),
            LlmResponse::Mixed { text, .. } => text.as_deref(),
            LlmResponse::ToolCalls { .. } => None,
        }
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        match self {
            LlmResponse::ToolCalls { calls, .. } => calls,
            LlmResponse::Mixed { tool_calls, .. } => tool_calls,
            LlmResponse::Text { .. } => &[],
        }
    }

    pub fn usage(&self) -> Option<&TokenUsage> {
        match self {
            LlmResponse::Text { usage, .. } => usage.as_ref(),
            LlmResponse::ToolCalls { usage, .. } => usage.as_ref(),
            LlmResponse::Mixed { usage, .. } => usage.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }

    #[test]
    fn test_llm_response_text_with_usage() {
        let usage = TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
        };
        let response = LlmResponse::Text {
            text: "Hello".to_string(),
            usage: Some(usage.clone()),
        };

        assert_eq!(response.text(), Some("Hello"));
        assert!(response.usage().is_some());
        let resp_usage = response.usage().unwrap();
        assert_eq!(resp_usage.input_tokens, 10);
        assert_eq!(resp_usage.output_tokens, 5);
    }

    #[test]
    fn test_llm_response_text_without_usage() {
        let response = LlmResponse::Text {
            text: "Hello".to_string(),
            usage: None,
        };

        assert_eq!(response.text(), Some("Hello"));
        assert!(response.usage().is_none());
    }

    #[test]
    fn test_llm_response_tool_calls_with_usage() {
        let usage = TokenUsage {
            input_tokens: 20,
            output_tokens: 10,
            total_tokens: 30,
        };
        let response = LlmResponse::ToolCalls {
            calls: vec![],
            usage: Some(usage),
        };

        assert!(response.text().is_none());
        assert!(response.usage().is_some());
    }

    #[test]
    fn test_llm_response_mixed_with_usage() {
        let usage = TokenUsage {
            input_tokens: 15,
            output_tokens: 8,
            total_tokens: 23,
        };
        let response = LlmResponse::Mixed {
            text: Some("Result".to_string()),
            tool_calls: vec![],
            usage: Some(usage),
        };

        assert_eq!(response.text(), Some("Result"));
        assert!(response.usage().is_some());
        let resp_usage = response.usage().unwrap();
        assert_eq!(resp_usage.total_tokens, 23);
    }
}

/// A tool call from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Definition of a tool for the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// A refactoring suggestion from the LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefactoringSuggestion {
    pub title: String,
    pub description: String,
    pub new_code: String,
}

/// A code issue found during review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeIssue {
    pub severity: IssueSeverity,
    pub message: String,
    pub line: usize,
    pub end_line: Option<usize>,
    pub column: Option<usize>,
    pub end_column: Option<usize>,
}

/// Severity of a code issue
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

/// Token usage statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
}

/// Result of a completion request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResult {
    pub text: String,
    pub usage: Option<TokenUsage>,
}

// ============================================================================
// Streaming Types
// ============================================================================

/// Events emitted during streaming responses
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Regular text chunk from the assistant
    TextDelta(String),
    /// Thinking/reasoning content (e.g., Claude extended thinking)
    ThinkingDelta(String),
    /// Tool call started
    ToolCallStart { id: String, name: String },
    /// Tool call arguments chunk (arguments come incrementally)
    ToolCallDelta { id: String, arguments_delta: String },
    /// Tool call completed (all arguments received)
    ToolCallComplete { id: String },
    /// Stream completed successfully
    Done,
    /// Error during streaming
    Error(String),
}

/// Callback type for streaming events
///
/// This is called for each chunk as it arrives from the LLM.
/// Implementations should be fast and non-blocking.
pub type StreamCallback = Box<dyn Fn(StreamEvent) + Send + Sync>;

/// Builder for accumulating streaming response
#[derive(Debug, Default)]
pub struct StreamingResponseBuilder {
    /// Accumulated text content
    pub text: String,
    /// Accumulated thinking content
    pub thinking: String,
    /// Tool calls being built (id -> (name, accumulated_args))
    pub tool_calls: std::collections::HashMap<String, (String, String)>,
    /// Token usage (if provided at end)
    pub usage: Option<TokenUsage>,
}

impl StreamingResponseBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Process a stream event and accumulate content
    pub fn process(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::TextDelta(text) => {
                self.text.push_str(text);
            }
            StreamEvent::ThinkingDelta(text) => {
                self.thinking.push_str(text);
            }
            StreamEvent::ToolCallStart { id, name } => {
                self.tool_calls
                    .insert(id.clone(), (name.clone(), String::new()));
            }
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                if let Some((_, args)) = self.tool_calls.get_mut(id) {
                    args.push_str(arguments_delta);
                }
            }
            StreamEvent::ToolCallComplete { .. } | StreamEvent::Done | StreamEvent::Error(_) => {}
        }
    }

    /// Build the final LlmResponse
    pub fn build(self) -> LlmResponse {
        let tool_calls: Vec<ToolCall> = self
            .tool_calls
            .into_iter()
            .map(|(id, (name, args))| {
                let arguments = serde_json::from_str(&args).unwrap_or(serde_json::Value::Null);
                ToolCall {
                    id,
                    name,
                    arguments,
                }
            })
            .collect();

        if tool_calls.is_empty() {
            LlmResponse::Text {
                text: self.text,
                usage: self.usage,
            }
        } else if self.text.is_empty() {
            LlmResponse::ToolCalls {
                calls: tool_calls,
                usage: self.usage,
            }
        } else {
            LlmResponse::Mixed {
                text: Some(self.text),
                tool_calls,
                usage: self.usage,
            }
        }
    }
}
