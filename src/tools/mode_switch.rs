//! Mode Switch Tool
//!
//! Allows the agent to request switching between modes (Ask, Plan, Build)
//! with user confirmation via the ask_user questionnaire system.

use anyhow::Result;
use async_trait::async_trait;
use serde_json::{json, Value};

use super::questionnaire::{
    AnswerValue, InteractionRequest, InteractionSender, OptionItem, Question, QuestionType,
    Questionnaire,
};
use super::{AgentMode, Tool, ToolResult};

/// Tool that allows the agent to request a mode switch with user confirmation
pub struct ModeSwitchTool {
    /// Channel to send interaction requests to the TUI
    interaction_tx: Option<InteractionSender>,
}

impl ModeSwitchTool {
    /// Create a new ModeSwitchTool
    ///
    /// # Arguments
    /// * `interaction_tx` - Optional channel to send requests to TUI.
    ///   If None, the tool will return an error when executed.
    pub fn new(interaction_tx: Option<InteractionSender>) -> Self {
        Self { interaction_tx }
    }

    /// Parse mode string to AgentMode
    fn parse_mode(mode_str: &str) -> Option<AgentMode> {
        match mode_str.to_lowercase().as_str() {
            "ask" => Some(AgentMode::Ask),
            "plan" => Some(AgentMode::Plan),
            "build" => Some(AgentMode::Build),
            _ => None,
        }
    }
}

#[async_trait]
impl Tool for ModeSwitchTool {
    fn name(&self) -> &str {
        "switch_mode"
    }

    fn description(&self) -> &str {
        "Request to switch the agent mode (Ask, Plan, or Build). This will show a confirmation \
         dialog to the user. Use this when the user asks to change modes or when you need \
         different capabilities (e.g., switching to Build mode to make file changes)."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "enum": ["ask", "plan", "build"],
                    "description": "The mode to switch to: 'ask' (read-only Q&A), 'plan' (read + propose changes), or 'build' (full access)"
                },
                "reason": {
                    "type": "string",
                    "description": "Brief explanation of why this mode switch is needed (shown to user)"
                }
            },
            "required": ["mode"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Check if we have a channel to the TUI
        let tx = match &self.interaction_tx {
            Some(tx) => tx.clone(),
            None => {
                return Ok(ToolResult::error(
                    "Cannot switch mode: No TUI connection available. \
                     The user can switch modes manually using /plan, /build, or /ask commands.",
                ));
            }
        };

        // Parse the target mode
        let mode_str = params.get("mode").and_then(|v| v.as_str()).unwrap_or("");

        let target_mode = match Self::parse_mode(mode_str) {
            Some(m) => m,
            None => {
                return Ok(ToolResult::error(format!(
                    "Invalid mode '{}'. Valid modes are: ask, plan, build",
                    mode_str
                )));
            }
        };

        // Get optional reason
        let reason = params
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("The agent is requesting a mode change.");

        // Build the confirmation questionnaire
        let mode_icon = target_mode.icon();
        let mode_label = target_mode.label();
        let mode_desc = target_mode.description();

        let questionnaire = Questionnaire {
            title: format!("Switch to {} Mode?", mode_label),
            description: Some(format!(
                "{}\n\n{} {} - {}",
                reason, mode_icon, mode_label, mode_desc
            )),
            questions: vec![Question {
                id: "confirm".to_string(),
                text: format!("Switch to {} mode?", mode_label),
                kind: QuestionType::SingleSelect {
                    options: vec![
                        OptionItem {
                            value: "yes".to_string(),
                            label: format!("✓ Yes, switch to {}", mode_label),
                        },
                        OptionItem {
                            value: "no".to_string(),
                            label: "✗ No, stay in current mode".to_string(),
                        },
                    ],
                    default: Some("yes".to_string()),
                },
            }],
            submit_label: "Confirm".to_string(),
        };

        // Create a oneshot channel for the response
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        // Send the questionnaire to the TUI
        if let Err(e) = tx
            .send(InteractionRequest::Questionnaire {
                data: questionnaire,
                responder: response_tx,
            })
            .await
        {
            return Ok(ToolResult::error(format!(
                "Failed to send mode switch request: {}. \
                 The user can switch modes manually using /{} command.",
                e, mode_str
            )));
        }

        // Wait for the user's response (2 minute timeout)
        let timeout_duration = std::time::Duration::from_secs(120);

        match tokio::time::timeout(timeout_duration, response_rx).await {
            Ok(Ok(response)) => {
                if response.cancelled {
                    return Ok(ToolResult::success(format!(
                        "Mode switch cancelled by user. Staying in current mode.\n\
                         The user can manually switch to {} mode later using /{} command.",
                        mode_label, mode_str
                    )));
                }

                // Check if user confirmed
                let confirmed = response
                    .answers
                    .get("confirm")
                    .map(|v| match v {
                        AnswerValue::Single(s) => s == "yes",
                        _ => false,
                    })
                    .unwrap_or(false);

                if confirmed {
                    // Return a special result that signals the TUI to switch modes
                    // The result contains a JSON payload that the TUI will parse
                    let result = json!({
                        "action": "mode_switch",
                        "target_mode": mode_str,
                        "confirmed": true
                    });
                    Ok(ToolResult::success(format!(
                        "MODE_SWITCH_CONFIRMED:{}\n\n\
                         ✓ User approved switching to {} mode.\n\
                         The mode change will be applied.",
                        serde_json::to_string(&result).unwrap_or_default(),
                        mode_label
                    )))
                } else {
                    Ok(ToolResult::success(format!(
                        "User declined the mode switch. Staying in current mode.\n\
                         The user can manually switch to {} mode later using /{} command.",
                        mode_label, mode_str
                    )))
                }
            }
            Ok(Err(_)) => Ok(ToolResult::error(
                "Failed to receive response from user (channel closed). \
                 The user can switch modes manually using /plan, /build, or /ask commands.",
            )),
            Err(_) => Ok(ToolResult::error(
                "Mode switch request timed out (2 minutes). \
                 The user can switch modes manually using /plan, /build, or /ask commands.",
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
    fn test_parse_mode() {
        assert_eq!(ModeSwitchTool::parse_mode("ask"), Some(AgentMode::Ask));
        assert_eq!(ModeSwitchTool::parse_mode("ASK"), Some(AgentMode::Ask));
        assert_eq!(ModeSwitchTool::parse_mode("plan"), Some(AgentMode::Plan));
        assert_eq!(ModeSwitchTool::parse_mode("PLAN"), Some(AgentMode::Plan));
        assert_eq!(ModeSwitchTool::parse_mode("build"), Some(AgentMode::Build));
        assert_eq!(ModeSwitchTool::parse_mode("BUILD"), Some(AgentMode::Build));
        assert_eq!(ModeSwitchTool::parse_mode("invalid"), None);
        assert_eq!(ModeSwitchTool::parse_mode(""), None);
    }

    #[test]
    fn test_tool_definition() {
        let tool = ModeSwitchTool::new(None);
        assert_eq!(tool.name(), "switch_mode");
        assert!(!tool.description().is_empty());
        assert!(tool.description().contains("switch"));

        let params = tool.parameters();
        assert!(params.get("properties").is_some());
        assert!(params.get("required").is_some());

        // Check that mode enum is defined
        let mode_prop = params["properties"]["mode"].clone();
        assert!(mode_prop["enum"].is_array());
    }

    #[tokio::test]
    async fn test_tool_without_channel() {
        let tool = ModeSwitchTool::new(None);
        let params = json!({
            "mode": "build",
            "reason": "Need to make file changes"
        });

        let result = tool.execute(params).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("No TUI connection"));
    }

    #[tokio::test]
    async fn test_tool_invalid_mode() {
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let tool = ModeSwitchTool::new(Some(tx));

        let result = tool
            .execute(json!({
                "mode": "invalid_mode"
            }))
            .await
            .unwrap();

        assert!(!result.success);
        assert!(result.output.contains("Invalid mode"));
    }

    #[tokio::test]
    async fn test_tool_missing_mode() {
        let (tx, _rx) = tokio::sync::mpsc::channel(4);
        let tool = ModeSwitchTool::new(Some(tx));

        let result = tool.execute(json!({})).await.unwrap();

        assert!(!result.success);
        assert!(result.output.contains("Invalid mode"));
    }
}
