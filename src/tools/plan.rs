//! Plan management tools for creating and tracking execution plans
//!
//! These tools allow the agent to:
//! - Preview plans before saving (preview_plan)
//! - Create structured execution plans with tasks and subtasks (save_plan)
//! - Check current plan progress (get_plan_status)
//! - Update existing plans (update_plan)
//! - Mark tasks as completed during execution (mark_task_done)
//! - Auto-complete and prompt for archiving when all tasks are done

use crate::services::PlanService;
use crate::storage::{ExecutionPlan, TechStack};
use crate::tools::questionnaire::{
    InteractionRequest, InteractionSender, OptionItem, Question, QuestionType, Questionnaire,
};
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

// ============================================================================
// Common parameter structures for plan tools
// ============================================================================

#[derive(Deserialize, Debug)]
struct TaskParam {
    description: String,
    subtasks: Option<Vec<String>>,
    files: Option<Vec<String>>,
}

#[derive(Deserialize, Debug, Default)]
struct StackParam {
    language: Option<String>,
    framework: Option<String>,
    ui_library: Option<String>,
    test_command: Option<String>,
    build_command: Option<String>,
}

impl From<StackParam> for TechStack {
    fn from(p: StackParam) -> Self {
        TechStack {
            language: p.language.unwrap_or_default(),
            framework: p.framework,
            ui_library: p.ui_library,
            test_command: p.test_command,
            build_command: p.build_command,
        }
    }
}

/// Common plan parameters for both preview and save
#[derive(Deserialize, Debug)]
struct PlanParams {
    title: String,
    overview: Option<String>,
    architecture: Option<String>,
    proposed_changes: Option<String>,
    tasks: Vec<TaskParam>,
    acceptance_criteria: Option<Vec<String>>,
    stack: Option<StackParam>,
    notes: Option<String>,
}

/// Build an ExecutionPlan from PlanParams
fn build_plan_from_params(params: &PlanParams) -> Result<ExecutionPlan, String> {
    if params.tasks.is_empty() {
        return Err("Plan must have at least one task".to_string());
    }

    let mut plan = ExecutionPlan::new(&params.title, "");

    // V2 fields
    if let Some(ref overview) = params.overview {
        plan.set_overview(overview);
    }
    if let Some(ref arch) = params.architecture {
        plan.set_architecture(arch);
    }
    if let Some(ref changes) = params.proposed_changes {
        plan.set_proposed_changes(changes);
    }

    // Add tasks with files and subtasks
    for task_param in &params.tasks {
        let files = task_param.files.clone().unwrap_or_default();
        plan.add_task_with_files(&task_param.description, files);

        let task_idx = plan.tasks.len() - 1;
        if let Some(ref subtasks) = task_param.subtasks {
            for subtask in subtasks {
                plan.add_subtask(task_idx, subtask);
            }
        }
    }

    // Acceptance criteria
    if let Some(ref criteria) = params.acceptance_criteria {
        for criterion in criteria {
            plan.add_acceptance_criterion(criterion);
        }
    }

    // Tech stack
    if let Some(ref stack) = params.stack {
        plan.set_stack(TechStack {
            language: stack.language.clone().unwrap_or_default(),
            framework: stack.framework.clone(),
            ui_library: stack.ui_library.clone(),
            test_command: stack.test_command.clone(),
            build_command: stack.build_command.clone(),
        });
    }

    // Notes as refinement
    if let Some(ref notes) = params.notes {
        plan.add_refinement(notes);
    }

    Ok(plan)
}

// ============================================================================
// PreviewPlanTool - Draft a plan without saving
// ============================================================================

/// Tool for previewing a plan before saving
///
/// Available in Plan mode. Creates a draft plan and displays it
/// without persisting to storage. Use save_plan to persist.
pub struct PreviewPlanTool;

impl PreviewPlanTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PreviewPlanTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for PreviewPlanTool {
    fn name(&self) -> &str {
        "preview_plan"
    }

    fn description(&self) -> &str {
        "Preview a plan before saving. Use this to show the user the proposed plan \
         for review and confirmation. After user confirms, use save_plan to persist."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Brief title for the plan (max 10 words)"
                },
                "overview": {
                    "type": "string",
                    "description": "High-level overview of what this plan accomplishes"
                },
                "architecture": {
                    "type": "string",
                    "description": "Architecture notes: components, data flow, dependencies"
                },
                "proposed_changes": {
                    "type": "string",
                    "description": "Summary of what will be modified and how"
                },
                "tasks": {
                    "type": "array",
                    "description": "List of tasks to complete",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "subtasks": { "type": "array", "items": { "type": "string" } },
                            "files": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["description"]
                    },
                    "minItems": 1
                },
                "acceptance_criteria": {
                    "type": "array",
                    "description": "Criteria that must be met for plan to be complete",
                    "items": { "type": "string" }
                },
                "stack": {
                    "type": "object",
                    "description": "Detected tech stack",
                    "properties": {
                        "language": { "type": "string" },
                        "framework": { "type": "string" },
                        "ui_library": { "type": "string" },
                        "test_command": { "type": "string" },
                        "build_command": { "type": "string" }
                    }
                }
            },
            "required": ["title", "tasks"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let params: PlanParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let plan = match build_plan_from_params(&params) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Return clean preview (no frontmatter)
        let preview = plan.to_preview();

        Ok(ToolResult::success(format!(
            "{}\n\n---\n\
            This is a **preview**. The plan has NOT been saved yet.\n\
            To save this plan, use `save_plan` with the same parameters.\n\
            To modify, adjust parameters and call `preview_plan` again.",
            preview
        )))
    }
}

// ============================================================================
// SavePlanTool - Create or update execution plans
// ============================================================================

/// Tool for creating or updating execution plans
///
/// Available in Plan mode only. Creates a structured plan with tasks
/// and optional subtasks that can be tracked during execution.
pub struct SavePlanTool {
    service: Arc<PlanService>,
}

impl SavePlanTool {
    /// Create a new SavePlanTool with the given plan service
    pub fn new(service: Arc<PlanService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Tool for SavePlanTool {
    fn name(&self) -> &str {
        "save_plan"
    }

    fn description(&self) -> &str {
        "Create a structured execution plan with tasks and subtasks. \
         Use preview_plan first to show user the plan, then save_plan to persist. \
         The plan will be saved and can be executed task-by-task in Build mode."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "title": {
                    "type": "string",
                    "description": "Brief title for the plan (max 10 words). Auto-generates unique plan ID."
                },
                "overview": {
                    "type": "string",
                    "description": "High-level overview of what this plan accomplishes"
                },
                "architecture": {
                    "type": "string",
                    "description": "Architecture notes: components, data flow, dependencies"
                },
                "proposed_changes": {
                    "type": "string",
                    "description": "Summary of what will be modified and how"
                },
                "tasks": {
                    "type": "array",
                    "description": "List of tasks to complete. Minimum 1 task required.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": {
                                "type": "string",
                                "description": "What this task accomplishes"
                            },
                            "subtasks": {
                                "type": "array",
                                "description": "Optional breakdown of this task into smaller steps",
                                "items": { "type": "string" }
                            },
                            "files": {
                                "type": "array",
                                "description": "Files that will be modified by this task",
                                "items": { "type": "string" }
                            }
                        },
                        "required": ["description"]
                    },
                    "minItems": 1
                },
                "acceptance_criteria": {
                    "type": "array",
                    "description": "Criteria that must be met for plan to be complete",
                    "items": { "type": "string" }
                },
                "stack": {
                    "type": "object",
                    "description": "Detected tech stack",
                    "properties": {
                        "language": { "type": "string" },
                        "framework": { "type": "string" },
                        "ui_library": { "type": "string" },
                        "test_command": { "type": "string" },
                        "build_command": { "type": "string" }
                    }
                },
                "notes": {
                    "type": "string",
                    "description": "Important considerations, risks, or implementation notes"
                }
            },
            "required": ["title", "tasks"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let params: PlanParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let plan = match build_plan_from_params(&params) {
            Ok(p) => p,
            Err(e) => return Ok(ToolResult::error(e)),
        };

        // Save the plan
        let path = self.service.save_plan(&plan).await?;
        self.service.set_current_plan(&plan.id).await?;

        // Return clean preview (no frontmatter)
        let preview = plan.to_preview();

        Ok(ToolResult::success(format!(
            "{}\n\n---\n\
            **Plan saved:** `{}`\n\
            **Location:** `{}`\n\n\
            **Next steps:**\n\
            - Review and refine with `/plans`\n\
            - Switch to `/build` mode to execute\n\
            - Use `mark_task_done` to track progress",
            preview,
            plan.id,
            path.display()
        )))
    }
}

// ============================================================================
// GetPlanStatusTool - Check current plan progress
// ============================================================================

/// Tool for checking current plan status
///
/// Available in Plan and Build modes. Returns current task,
/// progress, and next steps.
pub struct GetPlanStatusTool {
    service: Arc<PlanService>,
}

impl GetPlanStatusTool {
    pub fn new(service: Arc<PlanService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Tool for GetPlanStatusTool {
    fn name(&self) -> &str {
        "get_plan_status"
    }

    fn description(&self) -> &str {
        "Get the current execution plan status including progress, current task, and remaining tasks. \
         Use this to check where you are in the plan before continuing work."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _params: Value) -> Result<ToolResult> {
        let plan = match self.service.get_current_plan().await? {
            Some(p) => p,
            None => {
                return Ok(ToolResult::success(
                    "No active plan. Use `save_plan` to create one.",
                ))
            }
        };

        let (completed, total) = plan.progress();
        let next_pending = plan.get_next_pending();

        let mut output = format!(
            "**Plan:** {}\n\
            **Status:** {:?}\n\
            **Progress:** {}/{} tasks complete",
            plan.title, plan.status, completed, total
        );

        if let Some((task_idx, subtask_idx)) = next_pending {
            let task = &plan.tasks[task_idx];
            output.push_str(&format!(
                "\n\n**Current Task (#{}):** {}",
                task_idx + 1,
                task.description
            ));

            if !task.files.is_empty() {
                output.push_str(&format!("\n  Files: {}", task.files.join(", ")));
            }

            if let Some(sub_idx) = subtask_idx {
                let subtask = &task.subtasks[sub_idx];
                output.push_str(&format!(
                    "\n  **Current Subtask ({}.{}):** {}",
                    task_idx + 1,
                    sub_idx + 1,
                    subtask.description
                ));
            }
        } else if plan.is_complete() {
            output.push_str("\n\n**All tasks complete!** Use `/plans` to archive.");
        }

        // Show remaining tasks
        let remaining: Vec<_> = plan
            .tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.is_complete())
            .collect();

        if !remaining.is_empty() && remaining.len() <= 5 {
            output.push_str("\n\n**Remaining tasks:**");
            for (idx, task) in remaining {
                output.push_str(&format!("\n  {}. {}", idx + 1, task.description));
            }
        }

        Ok(ToolResult::success(output))
    }
}

// ============================================================================
// UpdatePlanTool - Modify existing plans
// ============================================================================

/// Tool for updating existing plans
///
/// Available in Plan mode. Allows adding tasks, updating descriptions,
/// or modifying acceptance criteria.
pub struct UpdatePlanTool {
    service: Arc<PlanService>,
}

impl UpdatePlanTool {
    pub fn new(service: Arc<PlanService>) -> Self {
        Self { service }
    }
}

#[async_trait]
impl Tool for UpdatePlanTool {
    fn name(&self) -> &str {
        "update_plan"
    }

    fn description(&self) -> &str {
        "Update the current execution plan. Can add tasks, update sections, \
         or add acceptance criteria. Use get_plan_status first to see current state."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "add_tasks": {
                    "type": "array",
                    "description": "New tasks to add to the plan",
                    "items": {
                        "type": "object",
                        "properties": {
                            "description": { "type": "string" },
                            "subtasks": { "type": "array", "items": { "type": "string" } },
                            "files": { "type": "array", "items": { "type": "string" } }
                        },
                        "required": ["description"]
                    }
                },
                "add_acceptance_criteria": {
                    "type": "array",
                    "description": "New acceptance criteria to add",
                    "items": { "type": "string" }
                },
                "update_overview": {
                    "type": "string",
                    "description": "Replace the overview section"
                },
                "update_architecture": {
                    "type": "string",
                    "description": "Replace the architecture section"
                },
                "add_note": {
                    "type": "string",
                    "description": "Add a refinement note to the plan"
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct UpdateParams {
            add_tasks: Option<Vec<TaskParam>>,
            add_acceptance_criteria: Option<Vec<String>>,
            update_overview: Option<String>,
            update_architecture: Option<String>,
            add_note: Option<String>,
        }

        let params: UpdateParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        let mut plan = match self.service.get_current_plan().await? {
            Some(p) => p,
            None => {
                return Ok(ToolResult::error(
                    "No active plan. Use save_plan to create one first.",
                ))
            }
        };

        let mut changes = Vec::new();

        // Add tasks
        if let Some(tasks) = params.add_tasks {
            for task in tasks {
                let files = task.files.unwrap_or_default();
                plan.add_task_with_files(&task.description, files);
                let task_idx = plan.tasks.len() - 1;

                if let Some(subtasks) = task.subtasks {
                    for st in subtasks {
                        plan.add_subtask(task_idx, &st);
                    }
                }
                changes.push(format!("Added task: {}", task.description));
            }
        }

        // Add acceptance criteria
        if let Some(criteria) = params.add_acceptance_criteria {
            for c in criteria {
                plan.add_acceptance_criterion(&c);
                changes.push(format!("Added criterion: {}", c));
            }
        }

        // Update sections
        if let Some(overview) = params.update_overview {
            plan.set_overview(&overview);
            changes.push("Updated overview".to_string());
        }

        if let Some(arch) = params.update_architecture {
            plan.set_architecture(&arch);
            changes.push("Updated architecture".to_string());
        }

        if let Some(note) = params.add_note {
            plan.add_refinement(&note);
            changes.push(format!("Added note: {}", note));
        }

        if changes.is_empty() {
            return Ok(ToolResult::success("No changes specified."));
        }

        // Save updated plan
        self.service.save_plan(&plan).await?;

        Ok(ToolResult::success(format!(
            "Plan updated:\n- {}\n\nUse `get_plan_status` to see current state.",
            changes.join("\n- ")
        )))
    }
}

// ============================================================================
// MarkTaskDoneTool - Mark tasks as completed with audit trail
// ============================================================================

/// Tool for marking plan tasks as completed
///
/// Available in Plan and Build modes. Requires a summary of what was done
/// for audit trail. When all tasks are complete, prompts to archive.
pub struct MarkTaskDoneTool {
    service: Arc<PlanService>,
    interaction_tx: Option<InteractionSender>,
}

impl MarkTaskDoneTool {
    /// Create a new MarkTaskDoneTool
    pub fn new(service: Arc<PlanService>, interaction_tx: Option<InteractionSender>) -> Self {
        Self {
            service,
            interaction_tx,
        }
    }

    /// Prompt user to archive the completed plan
    async fn prompt_archive(&self, plan_id: &str, plan_title: &str) -> Result<bool> {
        let Some(ref tx) = self.interaction_tx else {
            // No interaction channel - don't archive automatically
            return Ok(false);
        };

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        let questionnaire = Questionnaire {
            title: "Plan Complete!".to_string(),
            description: Some(format!(
                "All tasks in \"{}\" are complete.\nWould you like to archive this plan?",
                plan_title
            )),
            questions: vec![Question {
                id: "action".to_string(),
                text: "What would you like to do?".to_string(),
                kind: QuestionType::SingleSelect {
                    options: vec![
                        OptionItem {
                            value: "archive".to_string(),
                            label: "Archive plan (move to archives)".to_string(),
                        },
                        OptionItem {
                            value: "keep".to_string(),
                            label: "Keep as completed (don't archive)".to_string(),
                        },
                    ],
                    default: Some("archive".to_string()),
                },
            }],
            submit_label: "Confirm".to_string(),
        };

        // Send the request
        if tx
            .send(InteractionRequest::Questionnaire {
                data: questionnaire,
                responder: response_tx,
            })
            .await
            .is_err()
        {
            return Ok(false);
        }

        // Wait for response
        match response_rx.await {
            Ok(response) => {
                if response.cancelled {
                    return Ok(false);
                }

                // Check if user chose to archive
                let should_archive = response.answers.get("action").is_some_and(|v| {
                    matches!(v, crate::tools::questionnaire::AnswerValue::Single(s) if s == "archive")
                });

                if should_archive {
                    self.service.archive_plan(plan_id).await?;
                }

                Ok(should_archive)
            }
            Err(_) => Ok(false),
        }
    }
}

#[async_trait]
impl Tool for MarkTaskDoneTool {
    fn name(&self) -> &str {
        "mark_task_done"
    }

    fn description(&self) -> &str {
        "Mark a task or subtask as completed in the current execution plan. \
         REQUIRES a summary of what was done (for audit trail). \
         Use this after completing each task to track progress."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "task_index": {
                    "type": "integer",
                    "description": "0-based index of the task to mark as done"
                },
                "subtask_index": {
                    "type": "integer",
                    "description": "Optional: 0-based index of specific subtask to mark done"
                },
                "summary": {
                    "type": "string",
                    "description": "REQUIRED: Brief summary of what was accomplished (proves task completion)"
                },
                "files_changed": {
                    "type": "array",
                    "description": "List of files that were modified for this task",
                    "items": { "type": "string" }
                }
            },
            "required": ["task_index", "summary"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct MarkParams {
            task_index: usize,
            subtask_index: Option<usize>,
            summary: String,
            files_changed: Option<Vec<String>>,
        }

        let params: MarkParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // Validate summary is meaningful
        if params.summary.trim().len() < 10 {
            return Ok(ToolResult::error(
                "Summary too short. Provide specific details of what was completed (min 10 chars).",
            ));
        }

        // Build audit note
        let mut audit_note = params.summary.clone();
        if let Some(ref files) = params.files_changed {
            if !files.is_empty() {
                audit_note.push_str(&format!(" [Files: {}]", files.join(", ")));
            }
        }

        // Update task notes with audit trail before marking done
        {
            let mut plan = self
                .service
                .get_current_plan()
                .await?
                .ok_or_else(|| anyhow::anyhow!("No active plan"))?;

            plan.set_task_notes(params.task_index, &audit_note);
            self.service.save_plan(&plan).await?;
        }

        // Mark the task as done
        let progress = self
            .service
            .mark_task_done(params.task_index, params.subtask_index)
            .await?;

        // Build response
        let mut response = format!(
            "**Task {} marked complete.**\n\
            Summary: {}\n\
            Progress: {}/{} tasks complete",
            params.task_index + 1,
            params.summary,
            progress.completed,
            progress.total
        );

        if progress.is_complete {
            response.push_str("\n\n**All tasks complete!**");

            // Prompt for archive
            let archived = self
                .prompt_archive(&progress.plan_id, &progress.title)
                .await?;
            if archived {
                response.push_str("\nPlan has been archived.");
            }
        } else if let Some(next) = progress.next_task {
            response.push_str(&format!("\n\n**Next task:** {}", next));
        }

        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ChatSession, TarkStorage};
    use tempfile::TempDir;

    fn create_test_service(temp: &TempDir) -> (Arc<PlanService>, String) {
        let storage = TarkStorage::new(temp.path()).unwrap();
        // Create a test session
        let session = ChatSession::new();
        storage.save_session(&session).unwrap();
        let session_id = session.id.clone();
        (
            Arc::new(PlanService::new(
                TarkStorage::new(temp.path()).unwrap(),
                session_id.clone(),
            )),
            session_id,
        )
    }

    #[tokio::test]
    async fn test_preview_plan_tool() {
        let tool = PreviewPlanTool::new();

        let params = json!({
            "title": "Test Preview",
            "overview": "Testing the preview functionality",
            "tasks": [
                {"description": "Task 1", "files": ["src/lib.rs"]},
                {"description": "Task 2", "subtasks": ["Sub 2.1"]}
            ],
            "acceptance_criteria": ["Tests pass"]
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test Preview"));
        assert!(result.output.contains("Testing the preview functionality"));
        assert!(result.output.contains("Task 1"));
        assert!(result.output.contains("src/lib.rs"));
        assert!(result.output.contains("Sub 2.1"));
        assert!(result.output.contains("Tests pass"));
        assert!(result.output.contains("preview"));
        assert!(result.output.contains("NOT been saved"));
    }

    #[tokio::test]
    async fn test_save_plan_tool_v2() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);
        let tool = SavePlanTool::new(service.clone());

        let params = json!({
            "title": "Test Plan V2",
            "overview": "Testing the V2 schema",
            "architecture": "Component A -> Component B",
            "tasks": [
                {"description": "Task 1", "files": ["src/a.rs", "src/b.rs"]},
                {"description": "Task 2", "subtasks": ["Subtask 2.1", "Subtask 2.2"]}
            ],
            "acceptance_criteria": ["All tests pass", "Documentation updated"],
            "stack": {
                "language": "rust",
                "framework": "axum",
                "test_command": "cargo test"
            }
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test Plan V2"));
        assert!(result.output.contains("Testing the V2 schema"));
        assert!(result.output.contains("src/a.rs"));

        // Verify plan was saved with V2 fields
        let plan = service.get_current_plan().await.unwrap().unwrap();
        assert_eq!(plan.overview, "Testing the V2 schema");
        assert_eq!(plan.architecture, "Component A -> Component B");
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[0].files, vec!["src/a.rs", "src/b.rs"]);
        assert_eq!(plan.tasks[1].subtasks.len(), 2);
        assert_eq!(plan.acceptance_criteria.len(), 2);
        assert_eq!(plan.stack.language, "rust");
        assert_eq!(plan.stack.framework, Some("axum".to_string()));
    }

    #[tokio::test]
    async fn test_get_plan_status_tool() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        // Create and save a plan
        let mut plan = ExecutionPlan::new("Status Test", "");
        plan.add_task_with_files("Task 1", vec!["file1.rs".to_string()]);
        plan.add_task("Task 2");
        plan.complete_task(0);
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        let tool = GetPlanStatusTool::new(service);
        let result = tool.execute(json!({})).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("Status Test"));
        assert!(result.output.contains("1/2"));
        assert!(result.output.contains("Task 2")); // Current task
    }

    #[tokio::test]
    async fn test_get_plan_status_no_plan() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        let tool = GetPlanStatusTool::new(service);
        let result = tool.execute(json!({})).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("No active plan"));
    }

    #[tokio::test]
    async fn test_update_plan_tool() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        // Create initial plan
        let mut plan = ExecutionPlan::new("Update Test", "");
        plan.add_task("Initial task");
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        let tool = UpdatePlanTool::new(service.clone());

        // Add a task and criterion
        let params = json!({
            "add_tasks": [{"description": "New task", "files": ["new.rs"]}],
            "add_acceptance_criteria": ["New criterion"],
            "update_overview": "Updated overview",
            "add_note": "Added during test"
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Added task"));
        assert!(result.output.contains("Added criterion"));
        assert!(result.output.contains("Updated overview"));

        // Verify changes
        let updated = service.get_current_plan().await.unwrap().unwrap();
        assert_eq!(updated.tasks.len(), 2);
        assert_eq!(updated.tasks[1].description, "New task");
        assert_eq!(updated.overview, "Updated overview");
        assert_eq!(updated.acceptance_criteria.len(), 1);
    }

    #[tokio::test]
    async fn test_mark_task_done_with_summary() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        // Create a plan first
        let mut plan = ExecutionPlan::new("Test", "Test");
        plan.add_task("Task 1");
        plan.add_task("Task 2");
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        let tool = MarkTaskDoneTool::new(service.clone(), None);

        // Mark first task done with summary
        let params = json!({
            "task_index": 0,
            "summary": "Implemented the feature with proper error handling",
            "files_changed": ["src/lib.rs", "src/feature.rs"]
        });
        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("1/2"));
        assert!(result.output.contains("Implemented the feature"));

        // Verify audit trail was saved
        let plan = service.get_current_plan().await.unwrap().unwrap();
        assert!(plan.tasks[0].notes.is_some());
        let notes = plan.tasks[0].notes.as_ref().unwrap();
        assert!(notes.contains("Implemented the feature"));
        assert!(notes.contains("src/lib.rs"));
    }

    #[tokio::test]
    async fn test_mark_task_done_summary_required() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);

        let mut plan = ExecutionPlan::new("Test", "Test");
        plan.add_task("Task 1");
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        let tool = MarkTaskDoneTool::new(service, None);

        // Try with too-short summary
        let params = json!({
            "task_index": 0,
            "summary": "done"
        });
        let result = tool.execute(params).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("too short"));
    }

    #[tokio::test]
    async fn test_save_plan_empty_tasks() {
        let temp = TempDir::new().unwrap();
        let (service, _) = create_test_service(&temp);
        let tool = SavePlanTool::new(service);

        let params = json!({
            "title": "Empty Plan",
            "tasks": []
        });

        let result = tool.execute(params).await.unwrap();
        assert!(!result.success);
        assert!(result.output.contains("at least one task"));
    }
}
