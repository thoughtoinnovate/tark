//! Plan management tools for creating and tracking execution plans
//!
//! These tools allow the agent to:
//! - Create structured execution plans with tasks and subtasks
//! - Mark tasks as completed during execution
//! - Auto-complete and prompt for archiving when all tasks are done

use crate::services::PlanService;
use crate::storage::ExecutionPlan;
use crate::tools::questionnaire::{
    InteractionRequest, InteractionSender, OptionItem, Question, QuestionType, Questionnaire,
};
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

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
        "Create or update a structured execution plan with tasks and subtasks. \
         Use this when the user requests a multi-step feature, refactoring, or complex change. \
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
                "description": {
                    "type": "string",
                    "description": "Overview of what this plan accomplishes"
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
                                "items": {
                                    "type": "string"
                                }
                            },
                            "files": {
                                "type": "array",
                                "description": "Files that will be modified by this task",
                                "items": {
                                    "type": "string"
                                }
                            }
                        },
                        "required": ["description"]
                    },
                    "minItems": 1
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
        #[derive(Deserialize)]
        struct TaskParam {
            description: String,
            subtasks: Option<Vec<String>>,
            #[allow(dead_code)]
            files: Option<Vec<String>>,
        }

        #[derive(Deserialize)]
        struct PlanParams {
            title: String,
            description: Option<String>,
            tasks: Vec<TaskParam>,
            notes: Option<String>,
        }

        let params: PlanParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // Validate tasks
        if params.tasks.is_empty() {
            return Ok(ToolResult::error("Plan must have at least one task"));
        }

        // Create the plan
        let description = params.description.unwrap_or_default();
        let mut plan = ExecutionPlan::new(&params.title, &description);

        // Add tasks with subtasks
        for task_param in &params.tasks {
            let task_idx = plan.tasks.len();
            plan.add_task(&task_param.description);

            if let Some(ref subtasks) = task_param.subtasks {
                for subtask in subtasks {
                    plan.add_subtask(task_idx, subtask);
                }
            }
        }

        // Add notes as refinement if provided
        if let Some(notes) = params.notes {
            plan.add_refinement(&notes);
        }

        // Save the plan
        let path = self.service.save_plan(&plan).await?;
        self.service.set_current_plan(&plan.id).await?;

        // Return markdown checklist
        let markdown = plan.to_markdown();
        Ok(ToolResult::success(format!(
            "Plan created: {}\nSaved to: {}\n\n{}\n\n\
            Next steps:\n\
            - Review the plan and refine if needed\n\
            - Switch to /build mode to execute\n\
            - Tasks will be tracked as you complete them",
            plan.id,
            path.display(),
            markdown
        )))
    }
}

/// Tool for marking plan tasks as completed
///
/// Available in Plan and Build modes. When all tasks are complete,
/// prompts the user to archive the plan.
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
         Use this after completing each task to track progress. \
         When all tasks are done, the user will be prompted to archive the plan."
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
                    "description": "Optional: 0-based index of specific subtask to mark done. If omitted, marks the entire task done."
                }
            },
            "required": ["task_index"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct MarkParams {
            task_index: usize,
            subtask_index: Option<usize>,
        }

        let params: MarkParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {}", e))?;

        // Mark the task as done
        let progress = self
            .service
            .mark_task_done(params.task_index, params.subtask_index)
            .await?;

        // Build response
        let mut response = format!(
            "Marked task {} as done.\nProgress: {}/{} tasks complete",
            params.task_index + 1,
            progress.completed,
            progress.total
        );

        if progress.is_complete {
            response.push_str("\n\nAll tasks complete!");

            // Prompt for archive
            let archived = self
                .prompt_archive(&progress.plan_id, &progress.title)
                .await?;
            if archived {
                response.push_str("\nPlan has been archived.");
            }
        } else if let Some(next) = progress.next_task {
            response.push_str(&format!("\n\nNext task: {}", next));
        }

        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::TarkStorage;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_save_plan_tool() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();
        let service = Arc::new(PlanService::new(storage));
        let tool = SavePlanTool::new(service.clone());

        let params = json!({
            "title": "Test Plan",
            "description": "A test plan",
            "tasks": [
                {"description": "Task 1"},
                {"description": "Task 2", "subtasks": ["Subtask 2.1", "Subtask 2.2"]}
            ]
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("Test Plan"));
        assert!(result.output.contains("Task 1"));
        assert!(result.output.contains("Task 2"));

        // Verify plan was saved
        let plan = service.get_current_plan().await.unwrap();
        assert!(plan.is_some());
        let plan = plan.unwrap();
        assert_eq!(plan.tasks.len(), 2);
        assert_eq!(plan.tasks[1].subtasks.len(), 2);
    }

    #[tokio::test]
    async fn test_mark_task_done_tool() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();
        let service = Arc::new(PlanService::new(storage));

        // Create a plan first
        let mut plan = ExecutionPlan::new("Test", "Test");
        plan.add_task("Task 1");
        plan.add_task("Task 2");
        service.save_plan(&plan).await.unwrap();
        service.set_current_plan(&plan.id).await.unwrap();

        // Create the tool (no interaction channel for test)
        let tool = MarkTaskDoneTool::new(service.clone(), None);

        // Mark first task done
        let params = json!({"task_index": 0});
        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("1/2"));

        // Mark second task done
        let params = json!({"task_index": 1});
        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("2/2"));
        assert!(result.output.contains("All tasks complete"));
    }

    #[tokio::test]
    async fn test_save_plan_empty_tasks() {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();
        let service = Arc::new(PlanService::new(storage));
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
