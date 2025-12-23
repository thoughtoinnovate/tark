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
    Text(String),
    /// Tool calls requested by the model
    ToolCalls(Vec<ToolCall>),
    /// Mixed response with text and tool calls
    Mixed {
        text: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
}

impl LlmResponse {
    pub fn text(&self) -> Option<&str> {
        match self {
            LlmResponse::Text(s) => Some(s),
            LlmResponse::Mixed { text, .. } => text.as_deref(),
            LlmResponse::ToolCalls(_) => None,
        }
    }

    pub fn tool_calls(&self) -> &[ToolCall] {
        match self {
            LlmResponse::ToolCalls(calls) => calls,
            LlmResponse::Mixed { tool_calls, .. } => tool_calls,
            LlmResponse::Text(_) => &[],
        }
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
