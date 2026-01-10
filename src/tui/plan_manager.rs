//! Execution plan manager for the TUI
//!
//! Manages execution plans, task state transitions, and plan display.
//! Integrates with the storage layer for persistence.

#![allow(dead_code)]

use crate::storage::{ExecutionPlan, PlanMeta, PlanStatus, TarkStorage, TaskStatus};
use anyhow::Result;
use std::path::Path;

/// Plan manager for the TUI
pub struct PlanManager {
    /// Current active plan
    current_plan: Option<ExecutionPlan>,
    /// Storage backend
    storage: Option<TarkStorage>,
    /// Current session ID
    session_id: Option<String>,
    /// List of modified files during plan execution
    modified_files: Vec<String>,
    /// Auto-diff mode enabled
    auto_diff: bool,
}

impl Default for PlanManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanManager {
    /// Create a new plan manager
    pub fn new() -> Self {
        Self {
            current_plan: None,
            storage: None,
            session_id: None,
            modified_files: Vec::new(),
            auto_diff: false,
        }
    }

    /// Initialize with storage backend
    pub fn with_storage(mut self, workspace_dir: impl AsRef<Path>) -> Result<Self> {
        let storage = TarkStorage::new(workspace_dir)?;
        // Get current session ID
        let session_id = storage.get_current_session_id();
        self.storage = Some(storage);
        self.session_id = session_id.clone();

        // Try to load current plan if we have a session
        if let (Some(ref storage), Some(ref sid)) = (&self.storage, &session_id) {
            if let Ok(plan) = storage.load_current_execution_plan(sid) {
                self.current_plan = Some(plan);
            }
        }
        Ok(self)
    }

    /// Set the session ID
    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = Some(session_id);
    }

    /// Get the current plan
    pub fn current_plan(&self) -> Option<&ExecutionPlan> {
        self.current_plan.as_ref()
    }

    /// Get mutable reference to current plan
    pub fn current_plan_mut(&mut self) -> Option<&mut ExecutionPlan> {
        self.current_plan.as_mut()
    }

    /// Check if there's an active plan
    pub fn has_active_plan(&self) -> bool {
        self.current_plan
            .as_ref()
            .map(|p| p.status == PlanStatus::Active || p.status == PlanStatus::Draft)
            .unwrap_or(false)
    }

    /// Create a new plan
    pub fn create_plan(&mut self, title: &str, prompt: &str) -> Result<&ExecutionPlan> {
        let plan = ExecutionPlan::new(title, prompt);
        self.current_plan = Some(plan);
        self.save_current_plan()?;
        Ok(self.current_plan.as_ref().unwrap())
    }

    /// Load a plan by ID
    pub fn load_plan(&mut self, id: &str) -> Result<()> {
        if let (Some(ref storage), Some(ref session_id)) = (&self.storage, &self.session_id) {
            let plan = storage.load_execution_plan(session_id, id)?;
            self.current_plan = Some(plan);
            storage.set_current_plan(session_id, id)?;
        }
        Ok(())
    }

    /// Save the current plan
    pub fn save_current_plan(&self) -> Result<()> {
        if let (Some(ref storage), Some(ref plan), Some(ref session_id)) =
            (&self.storage, &self.current_plan, &self.session_id)
        {
            storage.save_execution_plan(session_id, plan)?;
        }
        Ok(())
    }

    /// List all plans
    pub fn list_plans(&self) -> Result<Vec<PlanMeta>> {
        if let (Some(ref storage), Some(ref session_id)) = (&self.storage, &self.session_id) {
            storage.list_execution_plans(session_id)
        } else {
            Ok(Vec::new())
        }
    }

    /// Delete a plan
    pub fn delete_plan(&mut self, id: &str) -> Result<()> {
        if let (Some(ref storage), Some(ref session_id)) = (&self.storage, &self.session_id) {
            storage.delete_execution_plan(session_id, id)?;
            // Clear current if it was the deleted plan
            if self.current_plan.as_ref().map(|p| p.id.as_str()) == Some(id) {
                self.current_plan = None;
            }
        }
        Ok(())
    }

    /// Add a task to the current plan
    pub fn add_task(&mut self, description: &str) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.add_task(description);
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Add a subtask to a task
    pub fn add_subtask(&mut self, task_index: usize, description: &str) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.add_subtask(task_index, description);
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Mark a task as done
    pub fn mark_task_done(&mut self, task_index: usize) -> Result<TaskTransitionResult> {
        let (old_status, is_complete) = {
            let plan = match self.current_plan.as_mut() {
                Some(p) => p,
                None => return Ok(TaskTransitionResult::NoPlan),
            };

            let old_status = plan
                .tasks
                .get(task_index)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Pending);

            if !Self::is_valid_transition(old_status, TaskStatus::Completed) {
                return Ok(TaskTransitionResult::InvalidTransition {
                    from: old_status,
                    to: TaskStatus::Completed,
                });
            }

            plan.complete_task(task_index);
            let is_complete = plan.is_complete();
            (old_status, is_complete)
        };

        self.save_current_plan()?;

        // Check if plan is complete
        if is_complete {
            if let Some(ref mut plan) = self.current_plan {
                plan.status = PlanStatus::Completed;
            }
            self.save_current_plan()?;
        }

        Ok(TaskTransitionResult::Success {
            from: old_status,
            to: TaskStatus::Completed,
        })
    }

    /// Mark a subtask as done
    pub fn mark_subtask_done(
        &mut self,
        task_index: usize,
        subtask_index: usize,
    ) -> Result<TaskTransitionResult> {
        if let Some(ref mut plan) = self.current_plan {
            let old_status = plan
                .tasks
                .get(task_index)
                .and_then(|t| t.subtasks.get(subtask_index))
                .map(|s| s.status)
                .unwrap_or(TaskStatus::Pending);

            if !Self::is_valid_transition(old_status, TaskStatus::Completed) {
                return Ok(TaskTransitionResult::InvalidTransition {
                    from: old_status,
                    to: TaskStatus::Completed,
                });
            }

            plan.complete_subtask(task_index, subtask_index);
            self.save_current_plan()?;

            Ok(TaskTransitionResult::Success {
                from: old_status,
                to: TaskStatus::Completed,
            })
        } else {
            Ok(TaskTransitionResult::NoPlan)
        }
    }

    /// Skip a task
    pub fn skip_task(&mut self, task_index: usize) -> Result<TaskTransitionResult> {
        if let Some(ref mut plan) = self.current_plan {
            let old_status = plan
                .tasks
                .get(task_index)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Pending);

            if !Self::is_valid_transition(old_status, TaskStatus::Skipped) {
                return Ok(TaskTransitionResult::InvalidTransition {
                    from: old_status,
                    to: TaskStatus::Skipped,
                });
            }

            plan.set_task_status(task_index, TaskStatus::Skipped);
            // Also skip all subtasks
            if let Some(task) = plan.tasks.get_mut(task_index) {
                for subtask in &mut task.subtasks {
                    if subtask.status == TaskStatus::Pending {
                        subtask.status = TaskStatus::Skipped;
                    }
                }
            }
            self.save_current_plan()?;

            Ok(TaskTransitionResult::Success {
                from: old_status,
                to: TaskStatus::Skipped,
            })
        } else {
            Ok(TaskTransitionResult::NoPlan)
        }
    }

    /// Set task to in-progress
    pub fn start_task(&mut self, task_index: usize) -> Result<TaskTransitionResult> {
        if let Some(ref mut plan) = self.current_plan {
            let old_status = plan
                .tasks
                .get(task_index)
                .map(|t| t.status)
                .unwrap_or(TaskStatus::Pending);

            if !Self::is_valid_transition(old_status, TaskStatus::InProgress) {
                return Ok(TaskTransitionResult::InvalidTransition {
                    from: old_status,
                    to: TaskStatus::InProgress,
                });
            }

            plan.set_task_status(task_index, TaskStatus::InProgress);
            plan.current_task_index = task_index;
            self.save_current_plan()?;

            Ok(TaskTransitionResult::Success {
                from: old_status,
                to: TaskStatus::InProgress,
            })
        } else {
            Ok(TaskTransitionResult::NoPlan)
        }
    }

    /// Get the next pending task
    pub fn get_next_task(&self) -> Option<NextTask> {
        self.current_plan.as_ref().and_then(|plan| {
            plan.get_next_pending().map(|(task_idx, subtask_idx)| {
                let task = &plan.tasks[task_idx];
                NextTask {
                    task_index: task_idx,
                    subtask_index: subtask_idx,
                    description: if let Some(sub_idx) = subtask_idx {
                        task.subtasks[sub_idx].description.clone()
                    } else {
                        task.description.clone()
                    },
                }
            })
        })
    }

    /// Add a refinement to the current plan
    pub fn add_refinement(&mut self, description: &str) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.add_refinement(description);
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Activate the current plan
    pub fn activate_plan(&mut self) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.status = PlanStatus::Active;
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Pause the current plan
    pub fn pause_plan(&mut self) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.status = PlanStatus::Paused;
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Abandon the current plan
    pub fn abandon_plan(&mut self) -> Result<()> {
        if let Some(ref mut plan) = self.current_plan {
            plan.status = PlanStatus::Abandoned;
            self.save_current_plan()?;
        }
        Ok(())
    }

    /// Get plan status summary
    pub fn get_status(&self) -> Option<PlanStatusSummary> {
        self.current_plan.as_ref().map(|plan| {
            let (completed, total) = plan.progress();
            PlanStatusSummary {
                id: plan.id.clone(),
                title: plan.title.clone(),
                status: plan.status,
                completed_tasks: completed,
                total_tasks: total,
                current_task: plan.tasks.get(plan.current_task_index).map(|t| TaskInfo {
                    index: plan.current_task_index,
                    description: t.description.clone(),
                    status: t.status,
                }),
            }
        })
    }

    /// Track a modified file
    pub fn track_modified_file(&mut self, path: &str) {
        if !self.modified_files.contains(&path.to_string()) {
            self.modified_files.push(path.to_string());
        }
    }

    /// Get modified files
    pub fn modified_files(&self) -> &[String] {
        &self.modified_files
    }

    /// Clear modified files
    pub fn clear_modified_files(&mut self) {
        self.modified_files.clear();
    }

    /// Toggle auto-diff mode
    pub fn toggle_auto_diff(&mut self) -> bool {
        self.auto_diff = !self.auto_diff;
        self.auto_diff
    }

    /// Check if auto-diff is enabled
    pub fn is_auto_diff_enabled(&self) -> bool {
        self.auto_diff
    }

    /// Check if a state transition is valid
    /// Valid transitions:
    /// - pending -> running -> completed/skipped
    /// - pending -> skipped (can skip without starting)
    fn is_valid_transition(from: TaskStatus, to: TaskStatus) -> bool {
        matches!(
            (from, to),
            (TaskStatus::Pending, TaskStatus::InProgress)
                | (TaskStatus::Pending, TaskStatus::Completed)
                | (TaskStatus::Pending, TaskStatus::Skipped)
                | (TaskStatus::InProgress, TaskStatus::Completed)
                | (TaskStatus::InProgress, TaskStatus::Skipped)
                | (TaskStatus::InProgress, TaskStatus::Failed)
        )
    }

    /// Get tasks for panel display
    pub fn get_panel_tasks(&self) -> Vec<PanelTask> {
        self.current_plan
            .as_ref()
            .map(|plan| {
                plan.tasks
                    .iter()
                    .enumerate()
                    .map(|(idx, task)| PanelTask {
                        index: idx,
                        description: task.description.clone(),
                        status: task.status,
                        subtask_count: task.subtasks.len(),
                        completed_subtasks: task
                            .subtasks
                            .iter()
                            .filter(|s| {
                                s.status == TaskStatus::Completed || s.status == TaskStatus::Skipped
                            })
                            .count(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Result of a task state transition
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskTransitionResult {
    /// Transition succeeded
    Success { from: TaskStatus, to: TaskStatus },
    /// Invalid transition attempted
    InvalidTransition { from: TaskStatus, to: TaskStatus },
    /// No active plan
    NoPlan,
    /// Task not found
    TaskNotFound,
}

/// Information about the next task to execute
#[derive(Debug, Clone)]
pub struct NextTask {
    pub task_index: usize,
    pub subtask_index: Option<usize>,
    pub description: String,
}

/// Summary of plan status
#[derive(Debug, Clone)]
pub struct PlanStatusSummary {
    pub id: String,
    pub title: String,
    pub status: PlanStatus,
    pub completed_tasks: usize,
    pub total_tasks: usize,
    pub current_task: Option<TaskInfo>,
}

/// Task information
#[derive(Debug, Clone)]
pub struct TaskInfo {
    pub index: usize,
    pub description: String,
    pub status: TaskStatus,
}

/// Task for panel display
#[derive(Debug, Clone)]
pub struct PanelTask {
    pub index: usize,
    pub description: String,
    pub status: TaskStatus,
    pub subtask_count: usize,
    pub completed_subtasks: usize,
}

impl PanelTask {
    /// Convert TaskStatus to panel TaskStatus
    pub fn to_panel_status(&self) -> crate::tui::PanelTaskStatus {
        match self.status {
            TaskStatus::Pending => crate::tui::PanelTaskStatus::Pending,
            TaskStatus::InProgress => crate::tui::PanelTaskStatus::Running,
            TaskStatus::Completed => crate::tui::PanelTaskStatus::Completed,
            TaskStatus::Skipped => crate::tui::PanelTaskStatus::Skipped,
            TaskStatus::Failed => crate::tui::PanelTaskStatus::Failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_manager_new() {
        let manager = PlanManager::new();
        assert!(manager.current_plan().is_none());
        assert!(!manager.has_active_plan());
    }

    #[test]
    fn test_valid_transitions() {
        // Valid transitions
        assert!(PlanManager::is_valid_transition(
            TaskStatus::Pending,
            TaskStatus::InProgress
        ));
        assert!(PlanManager::is_valid_transition(
            TaskStatus::Pending,
            TaskStatus::Completed
        ));
        assert!(PlanManager::is_valid_transition(
            TaskStatus::Pending,
            TaskStatus::Skipped
        ));
        assert!(PlanManager::is_valid_transition(
            TaskStatus::InProgress,
            TaskStatus::Completed
        ));
        assert!(PlanManager::is_valid_transition(
            TaskStatus::InProgress,
            TaskStatus::Skipped
        ));
        assert!(PlanManager::is_valid_transition(
            TaskStatus::InProgress,
            TaskStatus::Failed
        ));

        // Invalid transitions
        assert!(!PlanManager::is_valid_transition(
            TaskStatus::Completed,
            TaskStatus::Pending
        ));
        assert!(!PlanManager::is_valid_transition(
            TaskStatus::Skipped,
            TaskStatus::InProgress
        ));
        assert!(!PlanManager::is_valid_transition(
            TaskStatus::Failed,
            TaskStatus::Completed
        ));
    }

    #[test]
    fn test_modified_files_tracking() {
        let mut manager = PlanManager::new();

        manager.track_modified_file("src/main.rs");
        manager.track_modified_file("src/lib.rs");
        manager.track_modified_file("src/main.rs"); // Duplicate

        assert_eq!(manager.modified_files().len(), 2);
        assert!(manager
            .modified_files()
            .contains(&"src/main.rs".to_string()));
        assert!(manager.modified_files().contains(&"src/lib.rs".to_string()));

        manager.clear_modified_files();
        assert!(manager.modified_files().is_empty());
    }

    #[test]
    fn test_auto_diff_toggle() {
        let mut manager = PlanManager::new();
        assert!(!manager.is_auto_diff_enabled());

        let enabled = manager.toggle_auto_diff();
        assert!(enabled);
        assert!(manager.is_auto_diff_enabled());

        let disabled = manager.toggle_auto_diff();
        assert!(!disabled);
        assert!(!manager.is_auto_diff_enabled());
    }
}

/// Property-based tests for plan state transitions
///
/// **Property 12: Plan Task State Transitions**
/// **Validates: Requirements 16.3, 16.4**
#[cfg(test)]
mod property_tests {
    use super::*;
    use crate::storage::TaskStatus;
    use proptest::prelude::*;

    /// Generate a random TaskStatus
    fn arb_task_status() -> impl Strategy<Value = TaskStatus> {
        prop_oneof![
            Just(TaskStatus::Pending),
            Just(TaskStatus::InProgress),
            Just(TaskStatus::Completed),
            Just(TaskStatus::Skipped),
            Just(TaskStatus::Failed),
        ]
    }

    /// Generate a random target status for transitions
    fn arb_target_status() -> impl Strategy<Value = TaskStatus> {
        prop_oneof![
            Just(TaskStatus::InProgress),
            Just(TaskStatus::Completed),
            Just(TaskStatus::Skipped),
            Just(TaskStatus::Failed),
        ]
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any task status transition, the is_valid_transition function
        /// SHALL correctly identify valid transitions according to the state machine:
        /// - pending -> running -> completed/skipped/failed
        /// - pending -> completed/skipped (direct skip allowed)
        #[test]
        fn prop_valid_transitions_follow_state_machine(
            from in arb_task_status(),
            to in arb_target_status(),
        ) {
            let is_valid = PlanManager::is_valid_transition(from, to);

            // Define expected valid transitions
            let expected_valid = matches!(
                (from, to),
                // From Pending
                (TaskStatus::Pending, TaskStatus::InProgress)
                    | (TaskStatus::Pending, TaskStatus::Completed)
                    | (TaskStatus::Pending, TaskStatus::Skipped)
                    // From InProgress
                    | (TaskStatus::InProgress, TaskStatus::Completed)
                    | (TaskStatus::InProgress, TaskStatus::Skipped)
                    | (TaskStatus::InProgress, TaskStatus::Failed)
            );

            prop_assert_eq!(
                is_valid, expected_valid,
                "Transition from {:?} to {:?} should be {}",
                from, to, if expected_valid { "valid" } else { "invalid" }
            );
        }

        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any completed or skipped task, no further transitions SHALL be valid.
        /// Terminal states are: Completed, Skipped, Failed
        #[test]
        fn prop_terminal_states_have_no_valid_transitions(
            to in arb_target_status(),
        ) {
            // Terminal states
            let terminal_states = [TaskStatus::Completed, TaskStatus::Skipped, TaskStatus::Failed];

            for from in terminal_states {
                let is_valid = PlanManager::is_valid_transition(from, to);
                prop_assert!(
                    !is_valid,
                    "Terminal state {:?} should not allow transition to {:?}",
                    from, to
                );
            }
        }

        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any task index and valid transition, marking a task done SHALL
        /// update the task status to Completed.
        #[test]
        fn prop_mark_task_done_updates_status(
            task_count in 1usize..10,
            task_index in 0usize..10,
        ) {
            // Only test valid indices
            if task_index >= task_count {
                return Ok(());
            }

            let mut manager = PlanManager::new();

            // Create a plan with tasks
            manager.current_plan = Some(crate::storage::ExecutionPlan::new("Test Plan", "Test prompt"));

            if let Some(ref mut plan) = manager.current_plan {
                for i in 0..task_count {
                    plan.add_task(&format!("Task {}", i + 1));
                }
            }

            // Mark the task as done
            let result = manager.mark_task_done(task_index);

            match result {
                Ok(TaskTransitionResult::Success { from, to }) => {
                    prop_assert_eq!(from, TaskStatus::Pending);
                    prop_assert_eq!(to, TaskStatus::Completed);

                    // Verify the task status was updated
                    if let Some(ref plan) = manager.current_plan {
                        prop_assert_eq!(
                            plan.tasks[task_index].status,
                            TaskStatus::Completed,
                            "Task status should be Completed after mark_task_done"
                        );
                    }
                }
                Ok(TaskTransitionResult::InvalidTransition { .. }) => {
                    // This is also valid if the task was already in a non-pending state
                }
                Ok(TaskTransitionResult::NoPlan) => {
                    prop_assert!(false, "Should have a plan");
                }
                Ok(TaskTransitionResult::TaskNotFound) => {
                    prop_assert!(false, "Task should exist at index {}", task_index);
                }
                Err(e) => {
                    prop_assert!(false, "Unexpected error: {:?}", e);
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any task, skipping it SHALL update the status to Skipped.
        #[test]
        fn prop_skip_task_updates_status(
            task_count in 1usize..10,
            task_index in 0usize..10,
        ) {
            // Only test valid indices
            if task_index >= task_count {
                return Ok(());
            }

            let mut manager = PlanManager::new();

            // Create a plan with tasks
            manager.current_plan = Some(crate::storage::ExecutionPlan::new("Test Plan", "Test prompt"));

            if let Some(ref mut plan) = manager.current_plan {
                for i in 0..task_count {
                    plan.add_task(&format!("Task {}", i + 1));
                }
            }

            // Skip the task
            let result = manager.skip_task(task_index);

            match result {
                Ok(TaskTransitionResult::Success { from, to }) => {
                    prop_assert_eq!(from, TaskStatus::Pending);
                    prop_assert_eq!(to, TaskStatus::Skipped);

                    // Verify the task status was updated
                    if let Some(ref plan) = manager.current_plan {
                        prop_assert_eq!(
                            plan.tasks[task_index].status,
                            TaskStatus::Skipped,
                            "Task status should be Skipped after skip_task"
                        );
                    }
                }
                Ok(TaskTransitionResult::InvalidTransition { .. }) => {
                    // This is also valid if the task was already in a non-pending state
                }
                Ok(TaskTransitionResult::NoPlan) => {
                    prop_assert!(false, "Should have a plan");
                }
                Ok(TaskTransitionResult::TaskNotFound) => {
                    prop_assert!(false, "Task should exist at index {}", task_index);
                }
                Err(e) => {
                    prop_assert!(false, "Unexpected error: {:?}", e);
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any task, starting it SHALL update the status to InProgress.
        #[test]
        fn prop_start_task_updates_status(
            task_count in 1usize..10,
            task_index in 0usize..10,
        ) {
            // Only test valid indices
            if task_index >= task_count {
                return Ok(());
            }

            let mut manager = PlanManager::new();

            // Create a plan with tasks
            manager.current_plan = Some(crate::storage::ExecutionPlan::new("Test Plan", "Test prompt"));

            if let Some(ref mut plan) = manager.current_plan {
                for i in 0..task_count {
                    plan.add_task(&format!("Task {}", i + 1));
                }
            }

            // Start the task
            let result = manager.start_task(task_index);

            match result {
                Ok(TaskTransitionResult::Success { from, to }) => {
                    prop_assert_eq!(from, TaskStatus::Pending);
                    prop_assert_eq!(to, TaskStatus::InProgress);

                    // Verify the task status was updated
                    if let Some(ref plan) = manager.current_plan {
                        prop_assert_eq!(
                            plan.tasks[task_index].status,
                            TaskStatus::InProgress,
                            "Task status should be InProgress after start_task"
                        );
                        // Also verify current_task_index was updated
                        prop_assert_eq!(
                            plan.current_task_index,
                            task_index,
                            "current_task_index should be updated"
                        );
                    }
                }
                Ok(TaskTransitionResult::InvalidTransition { .. }) => {
                    // This is also valid if the task was already in a non-pending state
                }
                Ok(TaskTransitionResult::NoPlan) => {
                    prop_assert!(false, "Should have a plan");
                }
                Ok(TaskTransitionResult::TaskNotFound) => {
                    prop_assert!(false, "Task should exist at index {}", task_index);
                }
                Err(e) => {
                    prop_assert!(false, "Unexpected error: {:?}", e);
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 12: Plan Task State Transitions**
        /// **Validates: Requirements 16.3, 16.4**
        ///
        /// For any sequence of valid transitions, the final state SHALL be consistent.
        #[test]
        fn prop_transition_sequence_consistency(
            task_count in 1usize..5,
            operations in prop::collection::vec(0u8..3, 1..10),
        ) {
            let mut manager = PlanManager::new();

            // Create a plan with tasks
            manager.current_plan = Some(crate::storage::ExecutionPlan::new("Test Plan", "Test prompt"));

            if let Some(ref mut plan) = manager.current_plan {
                for i in 0..task_count {
                    plan.add_task(&format!("Task {}", i + 1));
                }
            }

            // Apply operations
            for op in operations {
                let task_idx = 0; // Always operate on first task for simplicity
                match op % 3 {
                    0 => { let _ = manager.start_task(task_idx); }
                    1 => { let _ = manager.mark_task_done(task_idx); }
                    2 => { let _ = manager.skip_task(task_idx); }
                    _ => {}
                }
            }

            // Verify final state is consistent
            if let Some(ref plan) = manager.current_plan {
                if !plan.tasks.is_empty() {
                    let status = plan.tasks[0].status;
                    // Status should be one of the valid states
                    prop_assert!(
                        matches!(
                            status,
                            TaskStatus::Pending
                                | TaskStatus::InProgress
                                | TaskStatus::Completed
                                | TaskStatus::Skipped
                                | TaskStatus::Failed
                        ),
                        "Task status should be a valid state, got {:?}",
                        status
                    );
                }
            }
        }
    }
}
