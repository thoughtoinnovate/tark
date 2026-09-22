//! `spawn_task` tool: delegate bounded exploration to a light subagent.
//!
//! The parent agent calls this to fan out independent, non-conflicting,
//! read-only work. The child runs an isolated `Ask` agent and only its
//! truncated summary returns — never raw transcripts.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::agent::subagent::{
    ChildOutcome, SharedParentCtx, SpawnReq, SpawnResult, SubagentManager,
};
use crate::tools::{RiskLevel, Tool, ToolResult};

/// Default per-spawn budget in seconds (also extends the registry timeout
/// via the `timeout_secs` param, clamped to 300 by the registry).
const DEFAULT_TIMEOUT_SECS: u64 = 180;

/// Tool that spawns a lightweight child subagent for one bounded task.
pub struct SpawnTaskTool {
    manager: Arc<SubagentManager>,
    parent: SharedParentCtx,
}

impl SpawnTaskTool {
    pub fn new(manager: Arc<SubagentManager>, parent: SharedParentCtx) -> Self {
        Self { manager, parent }
    }
}

#[async_trait::async_trait]
impl Tool for SpawnTaskTool {
    fn name(&self) -> &str {
        "spawn_task"
    }

    fn description(&self) -> &str {
        "Delegate ONE bounded, independent exploration task to a lightweight \
         subagent (isolated Ask-mode agent, read-only tools, max 5 steps). \
         Returns a short summary — never full transcripts. \
         SPAWN ONLY when ALL hold: (1) independent — needs no other task's \
         output and shares no writes; (2) substantive — at least 2-3 tool \
         calls of real exploration; (3) summarizable in a few sentences. \
         DO NOT spawn for: dependent/sequential steps, same-file edits, \
         single lookups, trivial reads, anything needing user approval, or \
         work needing full conversation context — do those INLINE yourself. \
         Prefer 2-3 parallel spawns; the system queues beyond capacity."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["description", "prompt"],
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Short (3-5 words) label shown in the Subagents panel"
                },
                "prompt": {
                    "type": "string",
                    "description": "Complete task brief: goal, scope paths, constraints, and EXACTLY what to return (summary shape). The child has zero parent context."
                },
                "subroot": {
                    "type": "string",
                    "description": "Workspace subdirectory confining this task (default: parent working dir). Must stay inside the parent workspace."
                },
                "provider": {
                    "type": "string",
                    "description": "Optional provider override (default: inherit parent). Falls back to parent on error."
                },
                "model": {
                    "type": "string",
                    "description": "Optional model override (default: inherit parent)."
                },
                "effort": {
                    "type": "string",
                    "description": "Optional reasoning effort override: off|low|medium|high (default: inherit parent)."
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "Overall budget for this child in seconds (default 180, clamped 1-300)."
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly // Spawning is orchestration; children are Ask read-only
    }

    fn category(&self) -> crate::tools::ToolCategory {
        crate::tools::ToolCategory::Builtin
    }

    async fn execute(&self, params: Value) -> Result<ToolResult, anyhow::Error> {
        #[derive(serde::Deserialize)]
        struct SpawnParams {
            description: String,
            prompt: String,
            subroot: Option<String>,
            provider: Option<String>,
            model: Option<String>,
            effort: Option<String>,
            timeout_secs: Option<u64>,
        }

        let params: SpawnParams = serde_json::from_value(params)
            .map_err(|e| anyhow::anyhow!("Invalid parameters: {e}"))?;

        // Snapshot parent context (short lock, clone-only, never held across await).
        let parent = self
            .parent
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_else(|poisoned| poisoned.into_inner().clone());

        // Resolve subroot: default parent root; relative joins parent root.
        let subroot = match params.subroot {
            Some(s) if !s.trim().is_empty() => {
                let p = std::path::PathBuf::from(s.trim());
                if p.is_absolute() {
                    p
                } else {
                    parent.working_dir.join(p)
                }
            }
            _ => parent.working_dir.clone(),
        };
        if subroot != parent.working_dir && !subroot.starts_with(&parent.working_dir) {
            return Ok(ToolResult::success(format!(
                "⊘ spawn denied: subroot {} escapes parent workspace {} — do this inline",
                subroot.display(),
                parent.working_dir.display()
            )));
        }

        // Resolve model: explicit override → pinned → inherit.
        let want_provider = params.provider.unwrap_or_default();
        let want_model = params.model.unwrap_or_default();
        let want_effort = params.effort.unwrap_or_default();
        let (llm, provider, model, mut notes) =
            if !want_provider.is_empty() || !want_model.is_empty() {
                let provider = if want_provider.is_empty() {
                    parent.provider.clone()
                } else {
                    want_provider
                };
                let model_opt = if want_model.is_empty() {
                    None
                } else {
                    Some(want_model.as_str())
                };
                match crate::llm::create_provider_with_options(&provider, true, model_opt) {
                    Ok(boxed) => (
                        Arc::from(boxed) as Arc<dyn crate::llm::LlmProvider>,
                        provider,
                        want_model.clone(),
                        Vec::new(),
                    ),
                    Err(e) => (
                        parent.llm.clone(),
                        parent.provider.clone(),
                        parent.model.clone(),
                        vec![format!("override invalid ({e:#}); used parent")],
                    ),
                }
            } else if parent.pin.is_pinned()
                && (!parent.pin.provider.is_empty() || !parent.pin.model.is_empty())
            {
                let provider = if parent.pin.provider.is_empty() {
                    parent.provider.clone()
                } else {
                    parent.pin.provider.clone()
                };
                let model_opt = if parent.pin.model.is_empty() {
                    None
                } else {
                    Some(parent.pin.model.as_str())
                };
                match crate::llm::create_provider_with_options(&provider, true, model_opt) {
                    Ok(boxed) => (
                        Arc::from(boxed) as Arc<dyn crate::llm::LlmProvider>,
                        provider,
                        parent.pin.model.clone(),
                        vec!["pinned model".to_string()],
                    ),
                    Err(e) => (
                        parent.llm.clone(),
                        parent.provider.clone(),
                        parent.model.clone(),
                        vec![format!("pinned model invalid ({e:#}); used parent")],
                    ),
                }
            } else {
                (
                    parent.llm.clone(),
                    parent.provider.clone(),
                    parent.model.clone(),
                    Vec::new(),
                )
            };
        let effort = if !want_effort.is_empty() {
            want_effort
        } else if parent.pin.is_pinned() && !parent.pin.effort.is_empty() {
            parent.pin.effort.clone()
        } else {
            parent.effort.clone()
        };

        let timeout_secs = params
            .timeout_secs
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .clamp(1, 300);
        let req = SpawnReq {
            parent_session: Arc::from(parent.session_id.as_str()),
            parent_root: parent.working_dir,
            title: params.description,
            prompt: params.prompt,
            subroot,
            llm,
            provider: provider.clone(),
            model: model.clone(),
            effort,
            max_iterations: parent.max_iterations.clamp(1, 10),
            timeout: Duration::from_secs(timeout_secs),
        };

        match self.manager.try_spawn(req).await {
            SpawnResult::Denied { reason } => Ok(ToolResult::success(format!(
                "⊘ spawn denied: {reason} — do this work inline instead"
            ))),
            SpawnResult::Spawned { id } | SpawnResult::Queued { id, .. } => {
                let session = parent.session_id.clone();
                match self
                    .manager
                    .await_outcome(&id, &session, Duration::from_secs(timeout_secs))
                    .await
                {
                    ChildOutcome::Completed(summary) => {
                        notes.push(format!(
                            "⑂ {} done in {}s ({} tool calls):\n{}",
                            summary.id, summary.elapsed_s, summary.tool_calls, summary.text
                        ));
                        Ok(ToolResult::success(notes.join("\n")))
                    }
                    ChildOutcome::Failed(e) => Ok(ToolResult::error(format!("⑂ {id} failed: {e}"))),
                    ChildOutcome::Killed(e) => Ok(ToolResult::error(format!("⑂ {id} killed: {e}"))),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::subagent::{ParentCtx, SubagentManager};
    use crate::config::SubagentModelPin;
    use crate::llm::tark_sim::TarkSimProvider;
    use serde_json::json;
    use tokio::sync::watch;

    fn harness(
        tmp: &std::path::Path,
    ) -> (SpawnTaskTool, Arc<crate::agent::subagent::SubagentManager>) {
        let (_tx, rx) = watch::channel(5usize);
        let cfg = crate::config::SubagentConfig::default();
        let manager = Arc::new(SubagentManager::new(&cfg, rx));
        let parent = Arc::new(std::sync::RwLock::new(ParentCtx {
            session_id: "sess-test".to_string(),
            working_dir: tmp.to_path_buf(),
            llm: Arc::new(TarkSimProvider::new()),
            provider: "tark_sim".to_string(),
            model: "tark_llm".to_string(),
            effort: "off".to_string(),
            max_iterations: 5,
            pin: SubagentModelPin::default(),
        }));
        (SpawnTaskTool::new(manager.clone(), parent), manager)
    }

    #[tokio::test]
    async fn tool_metadata_is_readonly_builtin() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (tool, _) = harness(tmp.path());
        assert_eq!(tool.name(), "spawn_task");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(tool.description().contains("DO NOT spawn"));
    }

    #[tokio::test]
    async fn spawn_task_runs_end_to_end() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (tool, _) = harness(tmp.path());
        let result = tool
            .execute(json!({
                "description": "explore auth",
                "prompt": "explore the authentication module call graph in detail and summarize every entry point",
                "timeout_secs": 60
            }))
            .await
            .unwrap();
        assert!(result.success, "output: {}", result.output);
        assert!(result.output.contains("⑂"), "output: {}", result.output);
    }

    #[tokio::test]
    async fn spawn_task_denies_trivial_inline() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (tool, _) = harness(tmp.path());
        let result = tool
            .execute(json!({"description": "x", "prompt": "read x"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("⊘"), "output: {}", result.output);
    }

    #[tokio::test]
    async fn spawn_task_denies_escape() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (tool, _) = harness(tmp.path());
        let result = tool
            .execute(json!({
                "description": "escape attempt",
                "prompt": "a sufficiently long exploration prompt describing cross-repo call graphs",
                "subroot": "/definitely/outside/the/workspace"
            }))
            .await
            .unwrap();
        assert!(result.success);
        assert!(
            result.output.contains("escapes"),
            "output: {}",
            result.output
        );
    }
}
