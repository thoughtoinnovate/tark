//! LSP diagnostics handler

use super::document::Document;
use crate::diagnostics::{DiagnosticsEngine, DiagnosticSeverity};
use anyhow::Result;
use tower_lsp::lsp_types::*;

/// Run diagnostics on a document and return LSP diagnostics
pub async fn run_diagnostics(
    engine: &DiagnosticsEngine,
    doc: &Document,
) -> Result<Vec<Diagnostic>> {
    let uri_str = doc.uri.to_string();

    let issues = engine
        .analyze(&uri_str, &doc.content, &doc.language_id)
        .await?;

    let diagnostics = issues
        .into_iter()
        .map(|issue| {
            let start_line = issue.line.saturating_sub(1) as u32; // Convert to 0-indexed
            let end_line = issue
                .end_line
                .map(|l| l.saturating_sub(1) as u32)
                .unwrap_or(start_line);
            let start_col = issue.column.unwrap_or(0) as u32;
            let end_col = issue.end_column.map(|c| c as u32).unwrap_or(start_col + 1);

            Diagnostic {
                range: Range {
                    start: Position {
                        line: start_line,
                        character: start_col,
                    },
                    end: Position {
                        line: end_line,
                        character: end_col,
                    },
                },
                severity: Some(match issue.severity {
                    DiagnosticSeverity::Error => tower_lsp::lsp_types::DiagnosticSeverity::ERROR,
                    DiagnosticSeverity::Warning => tower_lsp::lsp_types::DiagnosticSeverity::WARNING,
                    DiagnosticSeverity::Info => tower_lsp::lsp_types::DiagnosticSeverity::INFORMATION,
                    DiagnosticSeverity::Hint => tower_lsp::lsp_types::DiagnosticSeverity::HINT,
                }),
                code: None,
                code_description: None,
                source: Some("tark".to_string()),
                message: issue.message,
                related_information: None,
                tags: None,
                data: None,
            }
        })
        .collect();

    Ok(diagnostics)
}

