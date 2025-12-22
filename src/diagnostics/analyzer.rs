//! Code analysis and diagnostics

use crate::llm::{IssueSeverity, LlmProvider};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// A diagnostic message
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub message: String,
    pub line: usize,
    pub end_line: Option<usize>,
    pub column: Option<usize>,
    pub end_column: Option<usize>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
    Hint,
}

impl From<IssueSeverity> for DiagnosticSeverity {
    fn from(severity: IssueSeverity) -> Self {
        match severity {
            IssueSeverity::Error => DiagnosticSeverity::Error,
            IssueSeverity::Warning => DiagnosticSeverity::Warning,
            IssueSeverity::Info => DiagnosticSeverity::Info,
            IssueSeverity::Hint => DiagnosticSeverity::Hint,
        }
    }
}

/// Cache entry for diagnostics
struct CacheEntry {
    diagnostics: Vec<Diagnostic>,
    content_hash: u64,
    created_at: Instant,
}

/// Engine for generating AI-powered diagnostics
pub struct DiagnosticsEngine {
    llm: Arc<dyn LlmProvider>,
    cache: RwLock<HashMap<String, CacheEntry>>,
    cache_ttl: Duration,
    debounce_ms: u64,
}

impl DiagnosticsEngine {
    pub fn new(llm: Arc<dyn LlmProvider>) -> Self {
        Self {
            llm,
            cache: RwLock::new(HashMap::new()),
            cache_ttl: Duration::from_secs(300), // 5 minutes
            debounce_ms: 1000,
        }
    }

    pub fn with_cache_ttl(mut self, ttl: Duration) -> Self {
        self.cache_ttl = ttl;
        self
    }

    pub fn with_debounce(mut self, ms: u64) -> Self {
        self.debounce_ms = ms;
        self
    }

    /// Analyze code and return diagnostics
    pub async fn analyze(
        &self,
        uri: &str,
        content: &str,
        language: &str,
    ) -> Result<Vec<Diagnostic>> {
        let content_hash = self.hash_content(content);

        // Check cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.get(uri) {
                if entry.content_hash == content_hash && entry.created_at.elapsed() < self.cache_ttl
                {
                    return Ok(entry.diagnostics.clone());
                }
            }
        }

        // Get diagnostics from LLM
        let issues = self.llm.review_code(content, language).await?;

        let diagnostics: Vec<Diagnostic> = issues
            .into_iter()
            .map(|issue| Diagnostic {
                severity: issue.severity.into(),
                message: issue.message,
                line: issue.line,
                end_line: issue.end_line,
                column: issue.column,
                end_column: issue.end_column,
                source: "tark".to_string(),
            })
            .collect();

        // Update cache
        {
            let mut cache = self.cache.write().await;
            cache.insert(
                uri.to_string(),
                CacheEntry {
                    diagnostics: diagnostics.clone(),
                    content_hash,
                    created_at: Instant::now(),
                },
            );
        }

        Ok(diagnostics)
    }

    /// Clear cached diagnostics for a URI
    pub async fn invalidate(&self, uri: &str) {
        let mut cache = self.cache.write().await;
        cache.remove(uri);
    }

    /// Clear all cached diagnostics
    pub async fn clear_cache(&self) {
        let mut cache = self.cache.write().await;
        cache.clear();
    }

    fn hash_content(&self, content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }
}
