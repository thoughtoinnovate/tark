//! Core completion engine

#![allow(dead_code)]

use super::{CompletionCache, FimBuilder};
use crate::llm::LlmProvider;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// LSP context information from editor
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LspContext {
    /// Language/filetype
    #[serde(default)]
    pub language: Option<String>,
    /// Diagnostics near cursor
    #[serde(default)]
    pub diagnostics: Vec<DiagnosticInfo>,
    /// Type info at cursor (from hover)
    #[serde(default)]
    pub cursor_type: Option<String>,
    /// Nearby symbols
    #[serde(default)]
    pub symbols: Vec<SymbolInfo>,
    /// Whether LSP is available
    #[serde(default)]
    pub has_lsp: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticInfo {
    pub line: usize,
    #[serde(default)]
    pub col: Option<usize>,
    pub message: String,
    #[serde(default)]
    pub severity: Option<i32>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub line: usize,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Request for a code completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    /// Path to the file being edited
    pub file_path: PathBuf,
    /// Full content of the file
    pub file_content: String,
    /// Cursor line (0-indexed)
    pub cursor_line: usize,
    /// Cursor column (0-indexed)
    pub cursor_col: usize,
    /// Optional context from related files
    #[serde(default)]
    pub related_files: Vec<FileSnippet>,
    /// Optional LSP context from editor
    #[serde(default)]
    pub lsp_context: Option<LspContext>,
}

/// A snippet from a related file for context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnippet {
    pub path: PathBuf,
    pub content: String,
}

/// Response containing the completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionResponse {
    /// The completion text to insert
    pub completion: String,
    /// Number of lines in the completion
    pub line_count: usize,
    /// Token usage statistics (if available from provider)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<crate::llm::TokenUsage>,
}

/// The completion engine
pub struct CompletionEngine {
    llm: Arc<dyn LlmProvider>,
    cache: CompletionCache,
    fim_builder: FimBuilder,
}

impl CompletionEngine {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            llm,
            cache: CompletionCache::new(100),
            fim_builder: FimBuilder::new(),
        }
    }

    pub fn with_cache_size(mut self, size: usize) -> Self {
        self.cache = CompletionCache::new(size);
        self
    }

    pub fn with_context_lines(mut self, before: usize, after: usize) -> Self {
        self.fim_builder = self.fim_builder.with_context_lines(before, after);
        self
    }

    /// Get a completion for the given request
    pub async fn complete(&self, request: &CompletionRequest) -> Result<CompletionResponse> {
        let (prefix, suffix) = self.fim_builder.split_at_cursor(
            &request.file_content,
            request.cursor_line,
            request.cursor_col,
        );

        // NOTE: Cache is only valid for context-free completions.
        // With LSP context, the enhanced prefix changes based on current diagnostics/types,
        // so we cannot safely cache those results (same code at different times = different output).
        // Only check cache if there's no LSP context.
        if request.lsp_context.is_none() {
            if let Some(cached) = self.cache.get(&prefix, &suffix) {
                return Ok(CompletionResponse {
                    line_count: cached.lines().count(),
                    completion: cached,
                    usage: None, // Cache hits don't have fresh usage data
                });
            }
        }

        // Detect language (prefer LSP context if available)
        let language = request
            .lsp_context
            .as_ref()
            .and_then(|ctx| ctx.language.as_deref())
            .unwrap_or_else(|| FimBuilder::detect_language(&request.file_path));

        // Build enhanced prefix with LSP context
        let enhanced_prefix = self.build_enhanced_prefix(&prefix, request);

        // Get completion from LLM
        let result = self
            .llm
            .complete_fim(&enhanced_prefix, &suffix, language)
            .await?;

        // Only cache context-free completions (they're deterministic and stable)
        if request.lsp_context.is_none() {
            self.cache.put(&prefix, &suffix, result.text.clone());
        }

        let line_count = result.text.lines().count().max(1);

        Ok(CompletionResponse {
            completion: result.text,
            line_count,
            usage: result.usage,
        })
    }

    /// Build enhanced prefix with LSP context information
    fn build_enhanced_prefix(&self, prefix: &str, request: &CompletionRequest) -> String {
        // Use context if we have LSP or any fallback data (symbols/diagnostics/type)
        let ctx = match &request.lsp_context {
            Some(ctx)
                if ctx.has_lsp
                    || !ctx.symbols.is_empty()
                    || !ctx.diagnostics.is_empty()
                    || ctx.cursor_type.is_some() =>
            {
                ctx
            }
            _ => return prefix.to_string(),
        };

        let mut context_hints = Vec::new();

        // File + cursor metadata (always helpful to steer the model)
        let language_hint = ctx
            .language
            .as_deref()
            .unwrap_or_else(|| FimBuilder::detect_language(&request.file_path));
        let filename = request
            .file_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        context_hints.push(format!("// File: {} ({})", filename, language_hint));
        context_hints.push(format!(
            "// Cursor: line {}, col {}",
            request.cursor_line + 1,
            request.cursor_col + 1
        ));

        // Provide a small before/after snippet around the cursor
        let lines: Vec<&str> = request.file_content.lines().collect();
        let mut before_snippet = String::new();
        let mut after_snippet = String::new();

        // Up to 3 lines before cursor
        if request.cursor_line > 0 {
            let start = request.cursor_line.saturating_sub(3);
            for l in &lines[start..request.cursor_line] {
                before_snippet.push_str(l);
                before_snippet.push('\n');
            }
        }
        // Up to 3 lines after cursor
        if request.cursor_line + 1 < lines.len() {
            let end = (request.cursor_line + 4).min(lines.len());
            for l in &lines[(request.cursor_line + 1)..end] {
                after_snippet.push_str(l);
                after_snippet.push('\n');
            }
        }

        if !before_snippet.trim().is_empty() {
            context_hints.push(format!(
                "// Before:\n// {}",
                before_snippet.trim_end().replace('\n', "\n// ")
            ));
        }
        if !after_snippet.trim().is_empty() {
            context_hints.push(format!(
                "// After:\n// {}",
                after_snippet.trim_end().replace('\n', "\n// ")
            ));
        }

        // Add type info at cursor
        if let Some(cursor_type) = &ctx.cursor_type {
            // Clean up the type info (remove markdown formatting)
            let clean_type = cursor_type
                .lines()
                .next()
                .unwrap_or(cursor_type)
                .trim()
                .trim_start_matches("```")
                .trim_end_matches("```")
                .trim();
            if !clean_type.is_empty() && clean_type.len() < 200 {
                context_hints.push(format!("// Type at cursor: {}", clean_type));
            }
        }

        // Add relevant diagnostics (errors near cursor)
        let cursor_line = request.cursor_line;
        let nearby_errors: Vec<_> = ctx
            .diagnostics
            .iter()
            .filter(|d| {
                d.severity.unwrap_or(1) <= 2 && // Error or Warning
                (d.line as isize - cursor_line as isize).abs() <= 5
            })
            .take(2)
            .collect();

        for diag in nearby_errors {
            let msg = diag.message.lines().next().unwrap_or(&diag.message);
            if msg.len() < 100 {
                context_hints.push(format!("// Line {}: {}", diag.line + 1, msg));
            }
        }

        // Add nearby symbol context (just names, not full definitions)
        let nearby_symbols: Vec<_> = ctx
            .symbols
            .iter()
            .filter(|s| (s.line as isize - cursor_line as isize).abs() <= 20)
            .take(5)
            .collect();

        if !nearby_symbols.is_empty() {
            let symbol_list: Vec<_> = nearby_symbols
                .iter()
                .map(|s| format!("{} {}", s.kind, s.name))
                .collect();
            context_hints.push(format!("// Nearby: {}", symbol_list.join(", ")));
        }

        // If we have context hints, prepend them as comments
        if context_hints.is_empty() {
            prefix.to_string()
        } else {
            format!("{}\n{}", context_hints.join("\n"), prefix)
        }
    }

    /// Clear the completion cache
    pub fn clear_cache(&self) {
        self.cache.clear();
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the LLM provider
}
