//! Sequential thinking tool for structured reasoning.
//!
//! Enables LLMs to think step-by-step with support for:
//! - Numbered thought sequences
//! - Thought type categorization
//! - Confidence tracking
//! - Revision of previous thoughts
//! - Branching for alternative reasoning paths

use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A single thought in a reasoning chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    /// The actual thought content
    pub thought: String,
    /// Which thought number this is (1-indexed)
    pub thought_number: u32,
    /// Expected total number of thoughts
    pub total_thoughts: u32,
    /// Whether another thought is needed after this
    pub next_thought_needed: bool,
    /// Type of thinking: hypothesis, analysis, plan, decision, reflection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_type: Option<String>,
    /// Confidence level from 0.0 to 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// If this revises a previous thought, which one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revises_thought: Option<u32>,
    /// Branch identifier for alternative reasoning paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Summary returned after recording a thought
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingSummary {
    /// Current thought number
    pub thought_number: u32,
    /// Total thoughts expected
    pub total_thoughts: u32,
    /// Whether more thoughts are needed
    pub next_thought_needed: bool,
    /// Number of thoughts recorded so far
    pub history_length: usize,
    /// List of branch IDs if any
    pub branches: Vec<String>,
}

/// Tracks the thinking process within a session
#[derive(Debug, Default)]
pub struct ThinkingTracker {
    /// Main thought history
    history: Vec<Thought>,
    /// Thoughts organized by branch ID
    branches: HashMap<String, Vec<Thought>>,
}

impl ThinkingTracker {
    /// Create a new thinking tracker
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            branches: HashMap::new(),
        }
    }

    /// Record a new thought and return a summary
    pub fn record(&mut self, mut thought: Thought) -> ThinkingSummary {
        // Adjust total_thoughts if current number exceeds it
        if thought.thought_number > thought.total_thoughts {
            thought.total_thoughts = thought.thought_number;
        }

        // Add to main history
        self.history.push(thought.clone());

        // If this is a branched thought, also add to branch history
        if let Some(ref branch_id) = thought.branch_id {
            self.branches
                .entry(branch_id.clone())
                .or_default()
                .push(thought.clone());
        }

        // Build and return summary
        ThinkingSummary {
            thought_number: thought.thought_number,
            total_thoughts: thought.total_thoughts,
            next_thought_needed: thought.next_thought_needed,
            history_length: self.history.len(),
            branches: self.branches.keys().cloned().collect(),
        }
    }

    /// Get the full thought history
    pub fn history(&self) -> &[Thought] {
        &self.history
    }

    /// Get thoughts for a specific branch
    pub fn branch_history(&self, branch_id: &str) -> Option<&Vec<Thought>> {
        self.branches.get(branch_id)
    }

    /// Clear all thoughts (e.g., on new session)
    pub fn clear(&mut self) {
        self.history.clear();
        self.branches.clear();
    }

    /// Get the number of thoughts recorded
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

/// Tool that allows the agent to record structured thinking steps
pub struct ThinkTool {
    tracker: Arc<Mutex<ThinkingTracker>>,
}

impl ThinkTool {
    /// Create a new ThinkTool with the given tracker
    pub fn new(tracker: Arc<Mutex<ThinkingTracker>>) -> Self {
        Self { tracker }
    }

    /// Create a new ThinkTool with its own tracker
    pub fn new_standalone() -> Self {
        Self {
            tracker: Arc::new(Mutex::new(ThinkingTracker::new())),
        }
    }

    /// Get access to the underlying tracker
    pub fn tracker(&self) -> Arc<Mutex<ThinkingTracker>> {
        self.tracker.clone()
    }
}

#[async_trait]
impl Tool for ThinkTool {
    fn name(&self) -> &str {
        "think"
    }

    fn description(&self) -> &str {
        "Record your reasoning process step-by-step. Use this tool to think through \
         complex problems before taking actions. Each thought should be a discrete \
         reasoning step. This helps with debugging, planning, and decision-making. \
         The tool tracks your thought chain and supports revisions and branching."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["thought", "thought_number", "total_thoughts", "next_thought_needed"],
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "Your current thinking step - a discrete piece of reasoning"
                },
                "thought_number": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Which thought number this is (1-indexed)"
                },
                "total_thoughts": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Expected total number of thoughts (can increase if needed)"
                },
                "next_thought_needed": {
                    "type": "boolean",
                    "description": "Whether you need to continue thinking after this"
                },
                "thought_type": {
                    "type": "string",
                    "enum": ["hypothesis", "analysis", "plan", "decision", "reflection"],
                    "description": "Category of this thought"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "How confident you are in this thought (0.0 to 1.0)"
                },
                "revises_thought": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "If this revises a previous thought, which number"
                },
                "branch_id": {
                    "type": "string",
                    "description": "Identifier for alternative reasoning branch"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        // Thinking is always safe - it doesn't modify anything
        RiskLevel::ReadOnly
    }

    fn category(&self) -> crate::tools::ToolCategory {
        crate::tools::ToolCategory::Builtin
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Parse required parameters
        let thought_str = params
            .get("thought")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: thought"))?;

        let thought_number = params
            .get("thought_number")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: thought_number"))?
            as u32;

        let total_thoughts = params
            .get("total_thoughts")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: total_thoughts"))?
            as u32;

        let next_thought_needed = params
            .get("next_thought_needed")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: next_thought_needed"))?;

        // Parse optional parameters
        let thought_type = params
            .get("thought_type")
            .and_then(|v| v.as_str())
            .map(String::from);

        let confidence = params
            .get("confidence")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32);

        let revises_thought = params
            .get("revises_thought")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        let branch_id = params
            .get("branch_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Build the thought
        let thought = Thought {
            thought: thought_str.to_string(),
            thought_number,
            total_thoughts,
            next_thought_needed,
            thought_type,
            confidence,
            revises_thought,
            branch_id,
        };

        // Record the thought
        let summary = {
            let mut tracker = self.tracker.lock().unwrap();
            tracker.record(thought.clone())
        };

        // Return context-aware response with hints to guide model behavior
        let response = if summary.history_length > 1 {
            json!({
                "recorded": true,
                "step": summary.thought_number,
                "total_recorded": summary.history_length,
                "hint": if thought.next_thought_needed {
                    "Continue with next thought (increment thought_number)"
                } else {
                    "Thinking complete - now take action or respond"
                }
            })
        } else {
            json!({
                "recorded": true,
                "step": summary.thought_number,
                "hint": if thought.next_thought_needed {
                    "Continue with thought_number: 2"
                } else {
                    "Thinking complete - now take action or respond"
                }
            })
        }
        .to_string();
        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_think_tool_basic() {
        let tool = ThinkTool::new_standalone();

        let params = json!({
            "thought": "First, I need to understand the problem",
            "thought_number": 1,
            "total_thoughts": 3,
            "next_thought_needed": true
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("thought_number"));
    }

    #[tokio::test]
    async fn test_think_tool_with_optional_params() {
        let tool = ThinkTool::new_standalone();

        let params = json!({
            "thought": "Based on analysis, the best approach is X",
            "thought_number": 2,
            "total_thoughts": 3,
            "next_thought_needed": true,
            "thought_type": "decision",
            "confidence": 0.85
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_thinking_tracker_history() {
        let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        let tool = ThinkTool::new(tracker.clone());

        // Record three thoughts
        for i in 1..=3 {
            let params = json!({
                "thought": format!("Thought {}", i),
                "thought_number": i,
                "total_thoughts": 3,
                "next_thought_needed": i < 3
            });
            tool.execute(params).await.unwrap();
        }

        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 3);
        assert_eq!(tracker.history()[0].thought, "Thought 1");
    }

    #[tokio::test]
    async fn test_thinking_tracker_branching() {
        let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        let tool = ThinkTool::new(tracker.clone());

        // Main branch thought
        let params = json!({
            "thought": "Main reasoning",
            "thought_number": 1,
            "total_thoughts": 2,
            "next_thought_needed": true
        });
        tool.execute(params).await.unwrap();

        // Branch thought
        let params = json!({
            "thought": "Alternative approach",
            "thought_number": 2,
            "total_thoughts": 2,
            "next_thought_needed": false,
            "branch_id": "alternative-1"
        });
        tool.execute(params).await.unwrap();

        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 2);
        assert!(tracker.branch_history("alternative-1").is_some());
        assert_eq!(tracker.branch_history("alternative-1").unwrap().len(), 1);
    }
}
