//! Core completion engine

use super::{CompletionCache, FimBuilder};
use crate::llm::LlmProvider;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

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

        // Check cache first
        if let Some(cached) = self.cache.get(&prefix, &suffix) {
            return Ok(CompletionResponse {
                line_count: cached.lines().count(),
                completion: cached,
            });
        }

        // Detect language
        let language = FimBuilder::detect_language(&request.file_path);

        // Get completion from LLM
        let completion = self.llm.complete_fim(&prefix, &suffix, language).await?;

        // Cache the result
        self.cache.put(&prefix, &suffix, completion.clone());

        let line_count = completion.lines().count().max(1);

        Ok(CompletionResponse {
            completion,
            line_count,
        })
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
