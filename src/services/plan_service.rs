//! Plan service for managing execution plans
//!
//! Provides an abstraction layer between plan tools and storage,
//! handling plan lifecycle, task tracking, and archiving.
//! All operations are scoped to a specific session.

#![allow(dead_code)]

use crate::storage::{ExecutionPlan, PlanMeta, PlanStatus, TarkStorage};
use anyhow::{Context, Result};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Progress information returned after marking a task done
#[derive(Debug, Clone)]
pub struct PlanProgress {
    /// Plan ID
    pub plan_id: String,
    /// Plan title
    pub title: String,
    /// Number of completed tasks
    pub completed: usize,
    /// Total number of tasks
    pub total: usize,
    /// Whether the plan is now complete
    pub is_complete: bool,
    /// Description of the next pending task (if any)
    pub next_task: Option<String>,
}

/// Service for managing execution plans (session-scoped)
///
/// Thread-safe wrapper around TarkStorage for plan operations.
/// All plan operations are scoped to a specific session_id.
pub struct PlanService {
    storage: Arc<RwLock<TarkStorage>>,
    /// Session ID that this service operates on
    session_id: String,
    /// ID of the currently active plan (cached)
    current_plan_id: Arc<RwLock<Option<String>>>,
}

impl PlanService {
    /// Create a new plan service for a specific session
    pub fn new(storage: TarkStorage, session_id: String) -> Self {
        // Load current plan ID from storage for this session
        let current_id = storage.get_current_plan_id(&session_id);

        Self {
            storage: Arc::new(RwLock::new(storage)),
            session_id,
            current_plan_id: Arc::new(RwLock::new(current_id)),
        }
    }

    /// Create from shared storage (for when storage is already Arc'd)
    pub fn with_shared_storage(storage: Arc<RwLock<TarkStorage>>, session_id: String) -> Self {
        // We need to get current plan id synchronously, so we'll initialize as None
        // and load lazily
        Self {
            storage,
            session_id,
            current_plan_id: Arc::new(RwLock::new(None)),
        }
    }

    /// Get the session ID
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Update the session ID (when session changes)
    pub fn set_session_id(&mut self, session_id: String) {
        self.session_id = session_id;
        // Clear cached current plan since session changed
        if let Ok(mut current) = self.current_plan_id.try_write() {
            *current = None;
        }
    }

    /// Save an execution plan
    pub async fn save_plan(&self, plan: &ExecutionPlan) -> Result<PathBuf> {
        let storage = self.storage.write().await;
        let path = storage.save_execution_plan(&self.session_id, plan)?;

        // Update current plan ID if this is the active plan
        if plan.status == PlanStatus::Active || plan.status == PlanStatus::Draft {
            let mut current = self.current_plan_id.write().await;
            *current = Some(plan.id.clone());
        }

        Ok(path)
    }

    /// Get the current active plan
    pub async fn get_current_plan(&self) -> Result<Option<ExecutionPlan>> {
        // First check cached ID
        let current_id = self.current_plan_id.read().await;
        let id = if let Some(ref id) = *current_id {
            id.clone()
        } else {
            drop(current_id);
            // Load from storage
            let storage = self.storage.read().await;
            if let Some(id) = storage.get_current_plan_id(&self.session_id) {
                // Update cache
                let mut current = self.current_plan_id.write().await;
                *current = Some(id.clone());
                id
            } else {
                return Ok(None);
            }
        };

        let storage = self.storage.read().await;
        match storage.load_execution_plan(&self.session_id, &id) {
            Ok(plan) => Ok(Some(plan)),
            Err(_) => Ok(None),
        }
    }

    /// Get the current plan ID
    pub async fn get_current_plan_id(&self) -> Option<String> {
        let current = self.current_plan_id.read().await;
        if current.is_some() {
            return current.clone();
        }
        drop(current);

        // Load from storage
        let storage = self.storage.read().await;
        let id = storage.get_current_plan_id(&self.session_id);
        if let Some(ref id) = id {
            let mut current = self.current_plan_id.write().await;
            *current = Some(id.clone());
        }
        id
    }

    /// Set the current active plan
    pub async fn set_current_plan(&self, id: &str) -> Result<()> {
        // Verify the plan exists
        let storage = self.storage.read().await;
        let _ = storage
            .load_execution_plan(&self.session_id, id)
            .context("Plan not found")?;
        storage.set_current_plan(&self.session_id, id)?;
        drop(storage);

        let mut current = self.current_plan_id.write().await;
        *current = Some(id.to_string());
        Ok(())
    }

    /// Clear the current plan
    pub async fn clear_current_plan(&self) -> Result<()> {
        let storage = self.storage.read().await;
        storage.clear_current_plan(&self.session_id)?;
        drop(storage);

        let mut current = self.current_plan_id.write().await;
        *current = None;
        Ok(())
    }

    /// Load a plan by ID
    pub async fn load_plan(&self, id: &str) -> Result<ExecutionPlan> {
        let storage = self.storage.read().await;
        storage.load_execution_plan(&self.session_id, id)
    }

    /// Mark a task (or subtask) as done
    ///
    /// Returns progress information including whether the plan is now complete.
    pub async fn mark_task_done(
        &self,
        task_idx: usize,
        subtask_idx: Option<usize>,
    ) -> Result<PlanProgress> {
        // Load current plan
        let mut plan = self.get_current_plan().await?.context("No active plan")?;

        // Mark the task/subtask as done
        if let Some(sub_idx) = subtask_idx {
            plan.complete_subtask(task_idx, sub_idx);
        } else {
            plan.complete_task(task_idx);
        }

        // Check if plan is complete
        let is_complete = plan.is_complete();
        if is_complete {
            plan.status = PlanStatus::Completed;
        }

        // Get progress info
        let (completed, total) = plan.progress();
        let next_task = plan.get_next_pending().map(|(t_idx, s_idx)| {
            let task = &plan.tasks[t_idx];
            if let Some(s) = s_idx {
                task.subtasks[s].description.clone()
            } else {
                task.description.clone()
            }
        });

        let progress = PlanProgress {
            plan_id: plan.id.clone(),
            title: plan.title.clone(),
            completed,
            total,
            is_complete,
            next_task,
        };

        // Save the updated plan
        self.save_plan(&plan).await?;

        Ok(progress)
    }

    /// List all active (non-archived) plans for this session
    pub async fn list_plans(&self) -> Result<Vec<PlanMeta>> {
        let storage = self.storage.read().await;
        storage.list_execution_plans(&self.session_id)
    }

    /// List archived plans (global, not session-scoped)
    pub async fn list_archived_plans(&self) -> Result<Vec<PlanMeta>> {
        let storage = self.storage.read().await;
        storage.list_archived_plans()
    }

    /// Archive a plan by ID
    ///
    /// Moves the plan to the archives directory and prunes old archives
    /// to keep only the most recent 5.
    pub async fn archive_plan(&self, id: &str) -> Result<PathBuf> {
        let storage = self.storage.write().await;
        let path = storage.archive_plan(&self.session_id, id)?;

        // Clear current plan if this was it
        drop(storage);
        let current_id = self.current_plan_id.read().await;
        if current_id.as_deref() == Some(id) {
            drop(current_id);
            self.clear_current_plan().await?;
        }

        Ok(path)
    }

    /// Delete a plan by ID
    pub async fn delete_plan(&self, id: &str) -> Result<()> {
        let storage = self.storage.write().await;
        storage.delete_execution_plan(&self.session_id, id)?;

        // Clear current plan if this was it
        drop(storage);
        let current_id = self.current_plan_id.read().await;
        if current_id.as_deref() == Some(id) {
            drop(current_id);
            self.clear_current_plan().await?;
        }

        Ok(())
    }

    /// Export a plan as markdown
    pub async fn export_as_markdown(&self, id: &str) -> Result<String> {
        let storage = self.storage.read().await;
        let plan = storage.load_execution_plan(&self.session_id, id)?;
        Ok(plan.to_markdown())
    }

    /// Get progress for the current plan (for status bar)
    pub async fn get_current_progress(&self) -> Option<(usize, usize)> {
        if let Ok(Some(plan)) = self.get_current_plan().await {
            Some(plan.progress())
        } else {
            None
        }
    }

    /// Create a new plan and set it as current
    pub async fn create_plan(&self, title: &str, description: &str) -> Result<ExecutionPlan> {
        let mut plan = ExecutionPlan::new(title, description);
        plan.session_id = Some(self.session_id.clone());
        self.save_plan(&plan).await?;
        self.set_current_plan(&plan.id).await?;
        Ok(plan)
    }

    /// Update an existing plan (add tasks, refinements, etc.)
    pub async fn update_plan<F>(&self, id: &str, updater: F) -> Result<ExecutionPlan>
    where
        F: FnOnce(&mut ExecutionPlan),
    {
        let storage = self.storage.write().await;
        let mut plan = storage.load_execution_plan(&self.session_id, id)?;
        updater(&mut plan);
        storage.save_execution_plan(&self.session_id, &plan)?;
        Ok(plan)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_service(temp: &TempDir) -> (PlanService, String) {
        let storage = TarkStorage::new(temp.path()).unwrap();
        // Create a test session
        let session = crate::storage::ChatSession::new();
        storage.save_session(&session).unwrap();
        let session_id = session.id.clone();
        (PlanService::new(storage, session_id.clone()), session_id)
    }

    #[tokio::test]
    async fn test_plan_service_create_and_get() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        // Create a plan
        let plan = service
            .create_plan("Test Plan", "Test description")
            .await
            .unwrap();
        assert_eq!(plan.title, "Test Plan");

        // Get current plan
        let current = service.get_current_plan().await.unwrap();
        assert!(current.is_some());
        assert_eq!(current.unwrap().id, plan.id);
    }

    #[tokio::test]
    async fn test_plan_service_mark_task_done() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        // Create a plan with tasks
        let mut plan = ExecutionPlan::new("Test", "Test");
        plan.add_task("Task 1");
        plan.add_task("Task 2");
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        // Mark first task done
        let progress = service.mark_task_done(0, None).await.unwrap();
        assert_eq!(progress.completed, 1);
        assert_eq!(progress.total, 2);
        assert!(!progress.is_complete);

        // Mark second task done
        let progress = service.mark_task_done(1, None).await.unwrap();
        assert_eq!(progress.completed, 2);
        assert!(progress.is_complete);
    }

    #[tokio::test]
    async fn test_plan_service_session_isolation() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();

        // Create two sessions
        let session1 = crate::storage::ChatSession::new();
        let session2 = crate::storage::ChatSession::new();
        storage.save_session(&session1).unwrap();
        storage.save_session(&session2).unwrap();

        let service1 =
            PlanService::new(TarkStorage::new(temp.path()).unwrap(), session1.id.clone());
        let service2 =
            PlanService::new(TarkStorage::new(temp.path()).unwrap(), session2.id.clone());

        // Create plans in each session
        let plan1 = service1
            .create_plan("Plan 1", "Session 1 plan")
            .await
            .unwrap();
        let plan2 = service2
            .create_plan("Plan 2", "Session 2 plan")
            .await
            .unwrap();

        // Verify isolation
        let plans1 = service1.list_plans().await.unwrap();
        let plans2 = service2.list_plans().await.unwrap();

        assert_eq!(plans1.len(), 1);
        assert_eq!(plans1[0].id, plan1.id);
        assert_eq!(plans2.len(), 1);
        assert_eq!(plans2[0].id, plan2.id);
    }
}
