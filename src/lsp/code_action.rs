//! LSP code action handler

use super::document::DocumentStore;
use crate::llm::LlmProvider;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tower_lsp::lsp_types::*;

/// Handle code action request
pub async fn handle_code_action(
    llm: Arc<dyn LlmProvider>,
    documents: &Arc<DocumentStore>,
    params: CodeActionParams,
) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;
    let range = params.range;

    let doc = match documents.get(&uri) {
        Some(d) => d,
        None => return Ok(None),
    };

    // Get selected code
    let selected_code = match doc.get_range(&range) {
        Some(code) => code,
        None => return Ok(None),
    };

    // Don't suggest refactorings for very short selections
    if selected_code.trim().len() < 10 {
        return Ok(None);
    }

    // Get surrounding context
    let lines: Vec<&str> = doc.content.lines().collect();
    let start_line = range.start.line as usize;
    let context_start = start_line.saturating_sub(10);
    let context_end = ((range.end.line as usize) + 10).min(lines.len());
    let context: String = lines[context_start..context_end].join("\n");

    // Get refactoring suggestions from LLM
    let suggestions = llm.suggest_refactorings(&selected_code, &context).await?;

    if suggestions.is_empty() {
        return Ok(None);
    }

    let actions: Vec<CodeActionOrCommand> = suggestions
        .into_iter()
        .map(|suggestion| {
            let mut changes = HashMap::new();
            changes.insert(
                uri.clone(),
                vec![TextEdit {
                    range,
                    new_text: suggestion.new_code,
                }],
            );

            CodeActionOrCommand::CodeAction(CodeAction {
                title: suggestion.title,
                kind: Some(CodeActionKind::REFACTOR),
                diagnostics: None,
                edit: Some(WorkspaceEdit {
                    changes: Some(changes),
                    document_changes: None,
                    change_annotations: None,
                }),
                command: None,
                is_preferred: None,
                disabled: None,
                data: None,
            })
        })
        .collect();

    Ok(Some(actions))
}

