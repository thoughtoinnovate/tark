//! Dynamic Questionnaire Tool for user interaction
//!
//! Allows the agent to ask structured questions (single-select, multi-select, free-text)
//! via a popup in the TUI. Questions are displayed one-at-a-time, and all answers
//! are submitted collectively.
//!
//! Also provides the approval request system for risky operations.

#![allow(dead_code)]

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

use super::risk::{MatchType, RiskLevel};
use super::{Tool, ToolResult};

// ============================================================================
// Data Models
// ============================================================================

/// A complete questionnaire with multiple questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Questionnaire {
    /// Title displayed at the top of the popup
    pub title: String,

    /// Optional description/instructions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// List of questions in order
    pub questions: Vec<Question>,

    /// Label for the submit button (default: "Submit")
    #[serde(default = "default_submit_label")]
    pub submit_label: String,
}

fn default_submit_label() -> String {
    "Submit".to_string()
}

/// A single question with its type and options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    /// Unique identifier for this question (used in response)
    pub id: String,

    /// The question text displayed to the user
    pub text: String,

    /// The type of question and its configuration
    #[serde(flatten)]
    pub kind: QuestionType,
}

/// The type of input expected from the user
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuestionType {
    /// User picks exactly one option (radio buttons)
    SingleSelect {
        /// Available options
        options: Vec<OptionItem>,
        /// Default selected value (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },

    /// User picks one or more options (checkboxes)
    MultiSelect {
        /// Available options
        options: Vec<OptionItem>,
        /// Validation rules
        #[serde(skip_serializing_if = "Option::is_none")]
        validation: Option<MultiSelectValidation>,
    },

    /// User types a free-form string
    FreeText {
        /// Default value (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        /// Placeholder text (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
        /// Validation rules
        #[serde(skip_serializing_if = "Option::is_none")]
        validation: Option<TextValidation>,
    },
}

/// An option for single/multi select questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionItem {
    /// The value returned in the response
    pub value: String,
    /// The label displayed to the user
    pub label: String,
}

/// Validation rules for multi-select questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSelectValidation {
    /// Minimum number of selections required
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_selections: Option<usize>,
    /// Maximum number of selections allowed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_selections: Option<usize>,
}

/// Validation rules for free-text questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextValidation {
    /// Whether the field is required (non-empty)
    #[serde(default)]
    pub required: bool,
    /// Minimum length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    /// Maximum length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    /// Regex pattern the input must match
    #[serde(skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
}

// ============================================================================
// Response Types
// ============================================================================

/// The user's response to the questionnaire
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse {
    /// Whether the user cancelled the questionnaire
    pub cancelled: bool,
    /// The collected answers (empty if cancelled)
    pub answers: HashMap<String, AnswerValue>,
}

impl UserResponse {
    /// Create a cancelled response
    pub fn cancelled() -> Self {
        Self {
            cancelled: true,
            answers: HashMap::new(),
        }
    }

    /// Create a response with answers
    pub fn with_answers(answers: HashMap<String, AnswerValue>) -> Self {
        Self {
            cancelled: false,
            answers,
        }
    }
}

/// A single answer value (can be single string or multiple strings)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnswerValue {
    /// Single selection or free-text
    Single(String),
    /// Multiple selections
    Multi(Vec<String>),
}

// ============================================================================
// Approval Types
// ============================================================================

/// Approval request sent when a risky operation needs user confirmation
#[derive(Debug, Clone)]
pub struct ApprovalRequest {
    /// The tool name (e.g., "shell", "delete_file")
    pub tool: String,
    /// The command or path being executed
    pub command: String,
    /// Risk level of the operation
    pub risk_level: RiskLevel,
    /// Suggested patterns user can approve
    pub suggested_patterns: Vec<SuggestedPattern>,
}

/// A suggested pattern for approval
#[derive(Debug, Clone)]
pub struct SuggestedPattern {
    /// The pattern string
    pub pattern: String,
    /// How to match this pattern
    pub match_type: MatchType,
    /// Human-readable description
    pub description: String,
}

/// A stored approval pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPattern {
    /// The tool name this applies to
    pub tool: String,
    /// The pattern to match
    pub pattern: String,
    /// How to match the pattern
    pub match_type: MatchType,
    /// Optional description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// When this was created
    #[serde(default = "Utc::now")]
    pub created_at: DateTime<Utc>,
}

impl ApprovalPattern {
    /// Create a new approval pattern
    pub fn new(tool: String, pattern: String, match_type: MatchType) -> Self {
        Self {
            tool,
            pattern,
            match_type,
            description: None,
            created_at: Utc::now(),
        }
    }

    /// Create with description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }
}

/// User's response to an approval request
#[derive(Debug, Clone)]
pub struct ApprovalResponse {
    /// The user's choice
    pub choice: ApprovalChoice,
    /// The pattern selected (if applicable)
    pub selected_pattern: Option<ApprovalPattern>,
}

impl ApprovalResponse {
    /// Create a response that approves once (no pattern stored)
    pub fn approve_once() -> Self {
        Self {
            choice: ApprovalChoice::ApproveOnce,
            selected_pattern: None,
        }
    }

    /// Create a response that approves for session
    pub fn approve_session(pattern: ApprovalPattern) -> Self {
        Self {
            choice: ApprovalChoice::ApproveSession,
            selected_pattern: Some(pattern),
        }
    }

    /// Create a response that approves always (persistent)
    pub fn approve_always(pattern: ApprovalPattern) -> Self {
        Self {
            choice: ApprovalChoice::ApproveAlways,
            selected_pattern: Some(pattern),
        }
    }

    /// Create a deny response
    pub fn deny() -> Self {
        Self {
            choice: ApprovalChoice::Deny,
            selected_pattern: None,
        }
    }

    /// Create an always-deny response
    pub fn deny_always(pattern: ApprovalPattern) -> Self {
        Self {
            choice: ApprovalChoice::DenyAlways,
            selected_pattern: Some(pattern),
        }
    }
}

/// The user's approval choice
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalChoice {
    /// Approve just this one execution
    ApproveOnce,
    /// Approve pattern for this session
    ApproveSession,
    /// Approve pattern permanently
    ApproveAlways,
    /// Deny this execution
    Deny,
    /// Always deny this pattern
    DenyAlways,
}

impl ApprovalChoice {
    /// Get the keyboard shortcut for this choice
    pub fn shortcut(&self) -> &'static str {
        match self {
            Self::ApproveOnce => "y/1",
            Self::ApproveSession => "s/2",
            Self::ApproveAlways => "p/3",
            Self::Deny => "n/4",
            Self::DenyAlways => "N/5",
        }
    }

    /// Get the label for this choice
    pub fn label(&self) -> &'static str {
        match self {
            Self::ApproveOnce => "Approve once",
            Self::ApproveSession => "Approve for session",
            Self::ApproveAlways => "Approve always",
            Self::Deny => "Deny",
            Self::DenyAlways => "Always deny",
        }
    }

    /// All choices in order
    pub fn all() -> &'static [ApprovalChoice] {
        &[
            Self::ApproveOnce,
            Self::ApproveSession,
            Self::ApproveAlways,
            Self::Deny,
            Self::DenyAlways,
        ]
    }
}

// ============================================================================
// Channel Types for TUI Communication
// ============================================================================

/// Request sent from a tool to the TUI for user interaction
#[derive(Debug)]
pub enum InteractionRequest {
    /// Display a questionnaire and await user response
    Questionnaire {
        /// The questionnaire data
        data: Questionnaire,
        /// Channel to send the response back
        responder: oneshot::Sender<UserResponse>,
    },
    /// Display an approval card and await user response
    Approval {
        /// The approval request data
        request: ApprovalRequest,
        /// Channel to send the response back
        responder: oneshot::Sender<ApprovalResponse>,
    },
}

/// Sender for interaction requests (passed to tools)
pub type InteractionSender = mpsc::Sender<InteractionRequest>;

/// Receiver for interaction requests (held by TUI)
pub type InteractionReceiver = mpsc::Receiver<InteractionRequest>;

/// Create a new interaction channel
pub fn interaction_channel() -> (InteractionSender, InteractionReceiver) {
    mpsc::channel(4)
}

// ============================================================================
// AskUserTool Implementation
// ============================================================================

/// Tool that allows the agent to ask the user structured questions
pub struct AskUserTool {
    /// Channel to send interaction requests to the TUI
    interaction_tx: Option<InteractionSender>,
}

impl AskUserTool {
    /// Create a new AskUserTool
    ///
    /// # Arguments
    /// * `interaction_tx` - Optional channel to send requests to TUI.
    ///   If None, the tool will return an error when executed.
    pub fn new(interaction_tx: Option<InteractionSender>) -> Self {
        Self { interaction_tx }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Ask the user a structured set of questions via a popup dialog. Use this when you need \
         specific configuration choices, confirmations, or free-form input from the user. \
         Questions are displayed one at a time. The user can cancel to answer via chat instead."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Title displayed at the top of the questionnaire popup"
                },
                "description": {
                    "type": "string",
                    "description": "Optional instructions or context for the user"
                },
                "questions": {
                    "type": "array",
                    "description": "List of questions to ask",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for this question (used in response)"
                            },
                            "text": {
                                "type": "string",
                                "description": "The question text displayed to the user"
                            },
                            "type": {
                                "type": "string",
                                "enum": ["single_select", "multi_select", "free_text"],
                                "description": "Type of input: single_select (radio), multi_select (checkboxes), or free_text"
                            },
                            "options": {
                                "type": "array",
                                "description": "Options for single_select and multi_select types",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "value": {
                                            "type": "string",
                                            "description": "The value returned in the response"
                                        },
                                        "label": {
                                            "type": "string",
                                            "description": "The label displayed to the user"
                                        }
                                    },
                                    "required": ["value", "label"]
                                }
                            },
                            "default": {
                                "type": "string",
                                "description": "Default value for single_select or free_text"
                            },
                            "placeholder": {
                                "type": "string",
                                "description": "Placeholder text for free_text input"
                            }
                        },
                        "required": ["id", "text", "type"]
                    }
                },
                "submit_label": {
                    "type": "string",
                    "description": "Label for the submit button (default: 'Submit')"
                }
            },
            "required": ["title", "questions"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Check if we have a channel to the TUI
        let tx = match &self.interaction_tx {
            Some(tx) => tx.clone(),
            None => {
                return Ok(ToolResult::error(
                    "Cannot ask user: No TUI connection available. \
                     You can ask these questions conversationally in the chat instead.",
                ));
            }
        };

        // Parse the questionnaire from params
        let questionnaire: Questionnaire = match serde_json::from_value(params.clone()) {
            Ok(q) => q,
            Err(e) => {
                return Ok(ToolResult::error(format!(
                    "Invalid questionnaire format: {}",
                    e
                )));
            }
        };

        // Validate the questionnaire
        if questionnaire.questions.is_empty() {
            return Ok(ToolResult::error(
                "Questionnaire must have at least one question",
            ));
        }

        for (i, q) in questionnaire.questions.iter().enumerate() {
            if q.id.is_empty() {
                return Ok(ToolResult::error(format!(
                    "Question {} has an empty id",
                    i + 1
                )));
            }
            if q.text.is_empty() {
                return Ok(ToolResult::error(format!(
                    "Question '{}' has empty text",
                    q.id
                )));
            }
            // Validate options for select types
            match &q.kind {
                QuestionType::SingleSelect { options, .. }
                | QuestionType::MultiSelect { options, .. } => {
                    if options.is_empty() {
                        return Ok(ToolResult::error(format!(
                            "Question '{}' has no options",
                            q.id
                        )));
                    }
                }
                QuestionType::FreeText { .. } => {}
            }
        }

        // Create a oneshot channel for the response
        let (response_tx, response_rx) = oneshot::channel();

        // Send the request to the TUI
        if let Err(e) = tx
            .send(InteractionRequest::Questionnaire {
                data: questionnaire,
                responder: response_tx,
            })
            .await
        {
            return Ok(ToolResult::error(format!(
                "Failed to send questionnaire to TUI: {}. \
                 You can ask these questions conversationally in the chat instead.",
                e
            )));
        }

        // Wait for the user's response with a timeout (5 minutes max)
        // This prevents the tool from hanging indefinitely if something goes wrong
        let timeout_duration = std::time::Duration::from_secs(300);

        match tokio::time::timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => {
                if response.cancelled {
                    Ok(ToolResult::success(
                        "User cancelled the questionnaire. \
                         You may ask these questions conversationally via chat prompts instead.",
                    ))
                } else {
                    // Format the response nicely
                    let json_response = serde_json::to_string_pretty(&response)
                        .unwrap_or_else(|_| format!("{:?}", response));
                    Ok(ToolResult::success(json_response))
                }
            }
            Ok(Err(_)) => Ok(ToolResult::error(
                "Failed to receive response from user (channel closed). \
                 You can ask these questions conversationally in the chat instead.",
            )),
            Err(_) => Ok(ToolResult::error(
                "Questionnaire timed out (5 minutes). \
                 You can ask these questions conversationally in the chat instead.",
            )),
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_questionnaire_serialization() {
        let q = Questionnaire {
            title: "Test".to_string(),
            description: Some("A test questionnaire".to_string()),
            questions: vec![
                Question {
                    id: "lang".to_string(),
                    text: "Which language?".to_string(),
                    kind: QuestionType::SingleSelect {
                        options: vec![
                            OptionItem {
                                value: "rust".to_string(),
                                label: "Rust".to_string(),
                            },
                            OptionItem {
                                value: "python".to_string(),
                                label: "Python".to_string(),
                            },
                        ],
                        default: Some("rust".to_string()),
                    },
                },
                Question {
                    id: "features".to_string(),
                    text: "Enable features:".to_string(),
                    kind: QuestionType::MultiSelect {
                        options: vec![
                            OptionItem {
                                value: "tests".to_string(),
                                label: "Unit Tests".to_string(),
                            },
                            OptionItem {
                                value: "ci".to_string(),
                                label: "CI/CD".to_string(),
                            },
                        ],
                        validation: Some(MultiSelectValidation {
                            min_selections: Some(1),
                            max_selections: None,
                        }),
                    },
                },
                Question {
                    id: "output".to_string(),
                    text: "Output directory:".to_string(),
                    kind: QuestionType::FreeText {
                        default: Some("./dist".to_string()),
                        placeholder: Some("Enter path...".to_string()),
                        validation: Some(TextValidation {
                            required: true,
                            min_length: None,
                            max_length: Some(100),
                            regex: None,
                        }),
                    },
                },
            ],
            submit_label: "Generate".to_string(),
        };

        let json = serde_json::to_string_pretty(&q).unwrap();
        assert!(json.contains("\"title\": \"Test\""));
        assert!(json.contains("\"type\": \"single_select\""));
        assert!(json.contains("\"type\": \"multi_select\""));
        assert!(json.contains("\"type\": \"free_text\""));
    }

    #[test]
    fn test_questionnaire_deserialization() {
        let json = r#"{
            "title": "Setup",
            "questions": [
                {
                    "id": "confirm",
                    "text": "Proceed?",
                    "type": "single_select",
                    "options": [
                        { "value": "yes", "label": "Yes" },
                        { "value": "no", "label": "No" }
                    ]
                }
            ]
        }"#;

        let q: Questionnaire = serde_json::from_str(json).unwrap();
        assert_eq!(q.title, "Setup");
        assert_eq!(q.submit_label, "Submit"); // Default
        assert_eq!(q.questions.len(), 1);
        assert_eq!(q.questions[0].id, "confirm");
    }

    #[test]
    fn test_user_response_cancelled() {
        let response = UserResponse::cancelled();
        assert!(response.cancelled);
        assert!(response.answers.is_empty());
    }

    #[test]
    fn test_user_response_with_answers() {
        let mut answers = HashMap::new();
        answers.insert("lang".to_string(), AnswerValue::Single("rust".to_string()));
        answers.insert(
            "features".to_string(),
            AnswerValue::Multi(vec!["tests".to_string(), "ci".to_string()]),
        );

        let response = UserResponse::with_answers(answers);
        assert!(!response.cancelled);
        assert_eq!(response.answers.len(), 2);
    }

    #[test]
    fn test_tool_definition() {
        let tool = AskUserTool::new(None);
        assert_eq!(tool.name(), "ask_user");
        assert!(!tool.description().is_empty());

        let params = tool.parameters();
        assert!(params.get("properties").is_some());
        assert!(params.get("required").is_some());
    }

    #[tokio::test]
    async fn test_tool_without_channel() {
        let tool = AskUserTool::new(None);
        let params = json!({
            "title": "Test",
            "questions": [{
                "id": "q1",
                "text": "Question?",
                "type": "single_select",
                "options": [{ "value": "a", "label": "A" }]
            }]
        });

        let result = tool.execute(params).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("No TUI connection"));
    }

    #[tokio::test]
    async fn test_tool_invalid_params() {
        // Create a channel but drop the receiver so we can test validation errors
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let tool = AskUserTool::new(Some(tx));

        // Empty questions
        let result = tool
            .execute(json!({
                "title": "Test",
                "questions": []
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("at least one question"));

        // Missing options for single_select
        let result = tool
            .execute(json!({
                "title": "Test",
                "questions": [{
                    "id": "q1",
                    "text": "Question?",
                    "type": "single_select",
                    "options": []
                }]
            }))
            .await
            .unwrap();
        assert!(!result.success);
        assert!(result.output.contains("no options"));
    }
}
