//! Session-scoped todo tracking tool for the agent.
//!
//! Allows the agent to create and update a live todo list that displays
//! in the message area. Unlike the Plan feature (which is persistent and
//! comprehensive), this is lightweight and session-scoped.

use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Status of a todo item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TodoStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Cancelled,
}

impl TodoStatus {
    /// Get display icon for this status
    pub fn icon(&self) -> &'static str {
        match self {
            TodoStatus::Pending => "○",
            TodoStatus::InProgress => "●",
            TodoStatus::Completed => "✓",
            TodoStatus::Cancelled => "✗",
        }
    }
}

/// A single todo item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    /// Unique identifier for this todo
    pub id: String,
    /// Task description
    pub content: String,
    /// Current status
    #[serde(default)]
    pub status: TodoStatus,
    /// When this todo was created (RFC3339)
    #[serde(default = "default_timestamp")]
    pub created_at: String,
}

fn default_timestamp() -> String {
    Utc::now().to_rfc3339()
}

impl TodoItem {
    /// Create a new todo item
    pub fn new(id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            status: TodoStatus::Pending,
            created_at: Utc::now().to_rfc3339(),
        }
    }

    /// Create a todo with a specific status
    pub fn with_status(
        id: impl Into<String>,
        content: impl Into<String>,
        status: TodoStatus,
    ) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            status,
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

/// Summary of todo list state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoSummary {
    /// Total number of items
    pub total: usize,
    /// Number of completed items
    pub completed: usize,
    /// Number of in-progress items
    pub in_progress: usize,
    /// Number of pending items
    pub pending: usize,
    /// Full list of items (so agent knows what exists)
    pub items: Vec<TodoItem>,
}

/// Tracks the current session's todo list
#[derive(Debug)]
pub struct TodoTracker {
    /// List of todo items
    items: Vec<TodoItem>,
    /// Version counter (increments on each change for UI refresh detection)
    version: u64,
}

impl TodoTracker {
    /// Create a new empty todo tracker
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            version: 0,
        }
    }

    /// Update the todo list
    ///
    /// If `merge` is true, updates existing items by ID and adds new ones.
    /// If `merge` is false, replaces the entire list.
    pub fn update(&mut self, new_items: Vec<TodoItem>, merge: bool) {
        if merge {
            // Merge: update existing items by ID, add new ones
            for new_item in new_items {
                if let Some(existing) = self.items.iter_mut().find(|item| item.id == new_item.id) {
                    // Update existing item
                    existing.content = new_item.content;
                    existing.status = new_item.status;
                    // Keep original created_at
                } else {
                    // Add new item
                    self.items.push(new_item);
                }
            }
        } else {
            // Replace entire list
            self.items = new_items;
        }
        self.version += 1;
    }

    /// Get current todo summary
    pub fn to_summary(&self) -> TodoSummary {
        let completed = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::Completed)
            .count();
        let in_progress = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::InProgress)
            .count();
        let pending = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::Pending)
            .count();

        TodoSummary {
            total: self.items.len(),
            completed,
            in_progress,
            pending,
            items: self.items.clone(),
        }
    }

    /// Clear all todo items
    pub fn clear(&mut self) {
        self.items.clear();
        self.version += 1;
    }

    /// Get the items slice
    pub fn items(&self) -> &[TodoItem] {
        &self.items
    }

    /// Get the current version
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Check if the list is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }
}

impl Default for TodoTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Tool for managing session todos
pub struct TodoTool {
    tracker: Arc<Mutex<TodoTracker>>,
}

impl TodoTool {
    /// Create a new TodoTool with the given tracker
    pub fn new(tracker: Arc<Mutex<TodoTracker>>) -> Self {
        Self { tracker }
    }

    /// Get access to the underlying tracker
    pub fn tracker(&self) -> Arc<Mutex<TodoTracker>> {
        self.tracker.clone()
    }
}

#[async_trait]
impl Tool for TodoTool {
    fn name(&self) -> &str {
        "todo"
    }

    fn description(&self) -> &str {
        "Track progress on multi-step tasks with a visible todo list in the sidebar. \
         WORKFLOW: 1) Create todos at start of task (status: pending), \
         2) Mark current item 'in_progress' before working on it, \
         3) Mark 'completed' when done, then move to next. \
         4) Work through todos SEQUENTIALLY - don't skip ahead. \
         Returns full state so you always know what todos exist."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["todos"],
            "properties": {
                "todos": {
                    "type": "array",
                    "description": "Todo items to add or update. Use 'id' to update existing items. Pass empty array to clear.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "id": {
                                "type": "string",
                                "description": "Unique identifier for this todo (e.g., 'setup-auth', 'test-api')"
                            },
                            "content": {
                                "type": "string",
                                "description": "Task description"
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed", "cancelled"],
                                "description": "Task status. Defaults to 'pending' for new items."
                            }
                        },
                        "required": ["id", "content"]
                    }
                },
                "merge": {
                    "type": "boolean",
                    "description": "If true (default), merge with existing todos (update by id). If false, replace all todos with this list."
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly // Just updates in-memory state
    }

    fn category(&self) -> crate::tools::ToolCategory {
        crate::tools::ToolCategory::Builtin
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct TodoParams {
            todos: Vec<TodoItem>,
            #[serde(default = "default_merge")]
            merge: bool,
        }

        fn default_merge() -> bool {
            true
        }

        let params: TodoParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let mut tracker = self.tracker.lock().unwrap();

        // Handle empty array as clear
        if params.todos.is_empty() && !params.merge {
            tracker.clear();
        } else {
            tracker.update(params.todos, params.merge);
        }

        // Get current state to return to agent
        let summary = tracker.to_summary();
        drop(tracker); // Release lock

        // Format response with full current state
        let response = serde_json::to_string_pretty(&json!({
            "summary": {
                "total": summary.total,
                "completed": summary.completed,
                "in_progress": summary.in_progress,
                "pending": summary.pending
            },
            "items": summary.items
        }))?;

        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_todo_tracker_update_merge() {
        let mut tracker = TodoTracker::new();

        // Add initial items
        tracker.update(
            vec![
                TodoItem::new("task1", "First task"),
                TodoItem::new("task2", "Second task"),
            ],
            false,
        );
        assert_eq!(tracker.len(), 2);

        // Merge update: update task1, add task3
        tracker.update(
            vec![
                TodoItem::with_status("task1", "Updated first task", TodoStatus::Completed),
                TodoItem::new("task3", "Third task"),
            ],
            true,
        );
        assert_eq!(tracker.len(), 3);
        assert_eq!(tracker.items()[0].content, "Updated first task");
        assert_eq!(tracker.items()[0].status, TodoStatus::Completed);
        assert_eq!(tracker.items()[1].content, "Second task");
        assert_eq!(tracker.items()[2].content, "Third task");
    }

    #[test]
    fn test_todo_tracker_update_replace() {
        let mut tracker = TodoTracker::new();

        // Add initial items
        tracker.update(
            vec![
                TodoItem::new("task1", "First task"),
                TodoItem::new("task2", "Second task"),
            ],
            false,
        );
        assert_eq!(tracker.len(), 2);

        // Replace all
        tracker.update(vec![TodoItem::new("task3", "Only task")], false);
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.items()[0].id, "task3");
    }

    #[test]
    fn test_todo_tracker_clear() {
        let mut tracker = TodoTracker::new();
        tracker.update(vec![TodoItem::new("task1", "First task")], false);
        assert_eq!(tracker.len(), 1);

        tracker.clear();
        assert_eq!(tracker.len(), 0);
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_todo_summary() {
        let mut tracker = TodoTracker::new();
        tracker.update(
            vec![
                TodoItem::with_status("task1", "Done", TodoStatus::Completed),
                TodoItem::with_status("task2", "Working", TodoStatus::InProgress),
                TodoItem::new("task3", "Pending"),
                TodoItem::new("task4", "Also pending"),
            ],
            false,
        );

        let summary = tracker.to_summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.completed, 1);
        assert_eq!(summary.in_progress, 1);
        assert_eq!(summary.pending, 2);
    }

    #[tokio::test]
    async fn test_todo_tool_execution() {
        let tracker = Arc::new(Mutex::new(TodoTracker::new()));
        let tool = TodoTool::new(tracker.clone());

        let params = json!({
            "todos": [
                {"id": "test1", "content": "Test task", "status": "pending"}
            ],
            "merge": false
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);

        // Verify tracker was updated
        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 1);
        assert_eq!(tracker.items()[0].id, "test1");
    }

    #[tokio::test]
    async fn test_todo_tool_merge() {
        let tracker = Arc::new(Mutex::new(TodoTracker::new()));
        let tool = TodoTool::new(tracker.clone());

        // Add initial todo
        tool.execute(json!({
            "todos": [{"id": "task1", "content": "First"}],
            "merge": false
        }))
        .await
        .unwrap();

        // Merge in another todo
        let result = tool
            .execute(json!({
                "todos": [{"id": "task2", "content": "Second"}],
                "merge": true
            }))
            .await
            .unwrap();

        assert!(result.success);
        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 2);
    }

    #[tokio::test]
    async fn test_todo_tool_clear() {
        let tracker = Arc::new(Mutex::new(TodoTracker::new()));
        let tool = TodoTool::new(tracker.clone());

        // Add some todos
        tool.execute(json!({
            "todos": [
                {"id": "task1", "content": "First"},
                {"id": "task2", "content": "Second"}
            ],
            "merge": false
        }))
        .await
        .unwrap();

        // Clear by passing empty array with merge=false
        tool.execute(json!({
            "todos": [],
            "merge": false
        }))
        .await
        .unwrap();

        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 0);
    }
}
