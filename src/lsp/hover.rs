//! LSP hover handler

use super::document::DocumentStore;
use crate::llm::LlmProvider;
use anyhow::Result;
use std::sync::Arc;
use tower_lsp::lsp_types::*;

/// Handle hover request
pub async fn handle_hover(
    llm: Arc<dyn LlmProvider>,
    documents: &Arc<DocumentStore>,
    params: HoverParams,
) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match documents.get(&uri) {
        Some(d) => d,
        None => return Ok(None),
    };

    // Get the word or expression at the cursor
    let word = match doc.get_word_at(&position) {
        Some(w) => w,
        None => return Ok(None),
    };

    // Get surrounding context (a few lines before and after)
    let lines: Vec<&str> = doc.content.lines().collect();
    let line_num = position.line as usize;
    let start = line_num.saturating_sub(5);
    let end = (line_num + 6).min(lines.len());
    let context: String = lines[start..end].join("\n");

    // Get explanation from LLM
    let explanation = llm.explain_code(&word, &context).await?;

    Ok(Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: explanation,
        }),
        range: None,
    }))
}
