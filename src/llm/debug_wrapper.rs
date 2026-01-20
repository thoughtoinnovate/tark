//! Debug wrapper for LLM providers
//!
//! This wrapper intercepts all LLM calls and logs detailed information
//! to a debug log file for troubleshooting and analysis.

use super::{
    CodeIssue, CompletionResult, LlmProvider, LlmResponse, Message, RefactoringSuggestion,
    StreamCallback, ThinkSettings, TokenUsage, ToolCall, ToolDefinition,
};
use anyhow::Result;
use async_trait::async_trait;
use serde::Serialize;
use std::io::Write;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Debug provider wrapper that logs all LLM interactions
pub struct DebugProviderWrapper {
    inner: Box<dyn LlmProvider>,
    log_file: Arc<Mutex<std::fs::File>>,
}

impl DebugProviderWrapper {
    /// Create a new debug wrapper around an LLM provider
    pub fn new(inner: Box<dyn LlmProvider>, log_path: &std::path::Path) -> Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_path)?;

        // Note: Raw response logging is now handled by the unified debug logger
        // via the CORRELATION_ID task-local and debug_log! macro

        Ok(Self {
            inner,
            log_file: Arc::new(Mutex::new(file)),
        })
    }

    /// Log a debug entry
    async fn log(&self, entry: &DebugLogEntry) {
        let mut file = self.log_file.lock().await;
        let json = serde_json::to_string(entry).unwrap_or_default();
        let _ = writeln!(file, "{}", json);
    }

    /// Create a timestamp string
    fn timestamp() -> String {
        use chrono::{DateTime, Utc};
        let now: DateTime<Utc> = Utc::now();
        now.to_rfc3339()
    }
}

#[async_trait]
impl LlmProvider for DebugProviderWrapper {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn supports_native_thinking(&self) -> bool {
        self.inner.supports_native_thinking()
    }

    async fn supports_native_thinking_async(&self) -> bool {
        self.inner.supports_native_thinking_async().await
    }

    async fn chat(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Result<LlmResponse> {
        let request_id = DebugLogEntry::new_request_id();

        // Log request
        self.log(&DebugLogEntry::request(
            &request_id,
            self.name(),
            messages,
            tools,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(request_id.clone(), self.inner.chat(messages, tools))
            .await;
        let duration = start.elapsed();

        // Log response
        self.log(&DebugLogEntry::response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn chat_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        let request_id = DebugLogEntry::new_request_id();

        // Log request with thinking settings
        self.log(&DebugLogEntry::request_with_thinking(
            &request_id,
            self.name(),
            messages,
            tools,
            settings,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(
                request_id.clone(),
                self.inner.chat_with_thinking(messages, tools, settings),
            )
            .await;
        let duration = start.elapsed();

        // Log response
        self.log(&DebugLogEntry::response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
    ) -> Result<LlmResponse> {
        let request_id = DebugLogEntry::new_request_id();

        // Log request
        self.log(&DebugLogEntry::request(
            &request_id,
            self.name(),
            messages,
            tools,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(
                request_id.clone(),
                self.inner
                    .chat_streaming(messages, tools, callback, interrupt_check),
            )
            .await;
        let duration = start.elapsed();

        // Log response
        self.log(&DebugLogEntry::response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn chat_streaming_with_thinking(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
        interrupt_check: Option<&(dyn Fn() -> bool + Send + Sync)>,
        settings: &ThinkSettings,
    ) -> Result<LlmResponse> {
        let request_id = DebugLogEntry::new_request_id();

        // Log request with thinking settings
        self.log(&DebugLogEntry::request_with_thinking(
            &request_id,
            self.name(),
            messages,
            tools,
            settings,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(
                request_id.clone(),
                self.inner.chat_streaming_with_thinking(
                    messages,
                    tools,
                    callback,
                    interrupt_check,
                    settings,
                ),
            )
            .await;
        let duration = start.elapsed();

        // Log response
        self.log(&DebugLogEntry::response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    fn supports_streaming(&self) -> bool {
        self.inner.supports_streaming()
    }

    async fn complete_fim(
        &self,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Result<CompletionResult> {
        let request_id = DebugLogEntry::new_request_id();

        // Log FIM request
        self.log(&DebugLogEntry::fim_request(
            &request_id,
            self.name(),
            prefix,
            suffix,
            language,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(
                request_id.clone(),
                self.inner.complete_fim(prefix, suffix, language),
            )
            .await;
        let duration = start.elapsed();

        // Log FIM response
        self.log(&DebugLogEntry::fim_response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn explain_code(&self, code: &str, context: &str) -> Result<String> {
        let request_id = DebugLogEntry::new_request_id();

        // Log code explanation request
        self.log(&DebugLogEntry::code_explanation_request(
            &request_id,
            self.name(),
            code,
            context,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(request_id.clone(), self.inner.explain_code(code, context))
            .await;
        let duration = start.elapsed();

        // Log code explanation response
        self.log(&DebugLogEntry::code_explanation_response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn suggest_refactorings(
        &self,
        code: &str,
        context: &str,
    ) -> Result<Vec<RefactoringSuggestion>> {
        let request_id = DebugLogEntry::new_request_id();

        // Log refactoring request
        self.log(&DebugLogEntry::refactoring_request(
            &request_id,
            self.name(),
            code,
            context,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(
                request_id.clone(),
                self.inner.suggest_refactorings(code, context),
            )
            .await;
        let duration = start.elapsed();

        // Log refactoring response
        self.log(&DebugLogEntry::refactoring_response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }

    async fn review_code(&self, code: &str, language: &str) -> Result<Vec<CodeIssue>> {
        let request_id = DebugLogEntry::new_request_id();

        // Log code review request
        self.log(&DebugLogEntry::code_review_request(
            &request_id,
            self.name(),
            code,
            language,
        ))
        .await;

        let start = std::time::Instant::now();
        // Set correlation_id in task-local context for raw logging
        let result = crate::llm::raw_log::CORRELATION_ID
            .scope(request_id.clone(), self.inner.review_code(code, language))
            .await;
        let duration = start.elapsed();

        // Log code review response
        self.log(&DebugLogEntry::code_review_response(
            &request_id,
            self.name(),
            &result,
            duration,
        ))
        .await;

        result
    }
}

/// Structured debug log entry
#[derive(Debug, Serialize)]
pub struct DebugLogEntry {
    request_id: String,
    timestamp: String,
    provider: String,
    event_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    messages: Option<Vec<MessageSummary>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking_settings: Option<ThinkingSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fim_request: Option<FimRequestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    code_request: Option<CodeRequestSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    response: Option<ResponseSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MessageSummary {
    role: String,
    content_preview: String, // First 500 chars
    content_len: usize,
}

#[derive(Debug, Serialize)]
pub struct ThinkingSummary {
    enabled: bool,
    level_name: String,
    budget_tokens: u32,
    reasoning_effort: String,
}

#[derive(Debug, Serialize)]
pub struct FimRequestSummary {
    prefix_preview: String,
    suffix_preview: String,
    language: String,
}

#[derive(Debug, Serialize)]
pub struct CodeRequestSummary {
    code_preview: String,
    context_preview: Option<String>,
    language: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ResponseSummary {
    response_type: String, // "text", "tool_calls", "mixed"
    text_preview: Option<String>,
    text_len: Option<usize>,
    tool_calls: Option<Vec<ToolCallSummary>>,
    usage: Option<TokenUsageSummary>,
    fim_text: Option<String>,
    code_issues: Option<usize>,
    refactorings: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct ToolCallSummary {
    id: String,
    name: String,
    args_preview: String,
}

#[derive(Debug, Serialize)]
pub struct TokenUsageSummary {
    input_tokens: u32,
    output_tokens: u32,
    total_tokens: u32,
}

impl DebugLogEntry {
    /// Generate a new request ID
    fn new_request_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    /// Create a request log entry
    pub fn request(
        request_id: &str,
        provider: &str,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "request".to_string(),
            messages: Some(messages.iter().map(MessageSummary::from).collect()),
            tool_count: tools.map(|t| t.len()),
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a request log entry with thinking settings
    pub fn request_with_thinking(
        request_id: &str,
        provider: &str,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        settings: &ThinkSettings,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "request_with_thinking".to_string(),
            messages: Some(messages.iter().map(MessageSummary::from).collect()),
            tool_count: tools.map(|t| t.len()),
            thinking_settings: Some(ThinkingSummary::from(settings)),
            fim_request: None,
            code_request: None,
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a response log entry
    pub fn response(
        request_id: &str,
        provider: &str,
        result: &Result<LlmResponse>,
        duration: std::time::Duration,
    ) -> Self {
        let (response, error) = match result {
            Ok(resp) => (Some(ResponseSummary::from(resp)), None),
            Err(e) => (None, Some(e.to_string())),
        };

        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "response".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response,
            duration_ms: Some(duration.as_millis() as u64),
            error,
        }
    }

    /// Create a FIM request log entry
    pub fn fim_request(
        request_id: &str,
        provider: &str,
        prefix: &str,
        suffix: &str,
        language: &str,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "fim_request".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: Some(FimRequestSummary {
                prefix_preview: truncate_str(prefix, 500),
                suffix_preview: truncate_str(suffix, 500),
                language: language.to_string(),
            }),
            code_request: None,
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a FIM response log entry
    pub fn fim_response(
        request_id: &str,
        provider: &str,
        result: &Result<CompletionResult>,
        duration: std::time::Duration,
    ) -> Self {
        let (response, error) = match result {
            Ok(result) => (Some(ResponseSummary::from(result)), None),
            Err(e) => (None, Some(e.to_string())),
        };

        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "fim_response".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response,
            duration_ms: Some(duration.as_millis() as u64),
            error,
        }
    }

    /// Create a code explanation request log entry
    pub fn code_explanation_request(
        request_id: &str,
        provider: &str,
        code: &str,
        context: &str,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "code_explanation_request".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: Some(CodeRequestSummary {
                code_preview: truncate_str(code, 500),
                context_preview: Some(truncate_str(context, 500)),
                language: None,
            }),
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a code explanation response log entry
    pub fn code_explanation_response(
        request_id: &str,
        provider: &str,
        result: &Result<String>,
        duration: std::time::Duration,
    ) -> Self {
        let (response, error) = match result {
            Ok(text) => (Some(ResponseSummary::from(text.as_str())), None),
            Err(e) => (None, Some(e.to_string())),
        };

        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "code_explanation_response".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response,
            duration_ms: Some(duration.as_millis() as u64),
            error,
        }
    }

    /// Create a refactoring request log entry
    pub fn refactoring_request(
        request_id: &str,
        provider: &str,
        code: &str,
        context: &str,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "refactoring_request".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: Some(CodeRequestSummary {
                code_preview: truncate_str(code, 500),
                context_preview: Some(truncate_str(context, 500)),
                language: None,
            }),
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a refactoring response log entry
    pub fn refactoring_response(
        request_id: &str,
        provider: &str,
        result: &Result<Vec<RefactoringSuggestion>>,
        duration: std::time::Duration,
    ) -> Self {
        let (response, error) = match result {
            Ok(suggestions) => (Some(ResponseSummary::from(suggestions)), None),
            Err(e) => (None, Some(e.to_string())),
        };

        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "refactoring_response".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response,
            duration_ms: Some(duration.as_millis() as u64),
            error,
        }
    }

    /// Create a code review request log entry
    pub fn code_review_request(
        request_id: &str,
        provider: &str,
        code: &str,
        language: &str,
    ) -> Self {
        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "code_review_request".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: Some(CodeRequestSummary {
                code_preview: truncate_str(code, 500),
                context_preview: None,
                language: Some(language.to_string()),
            }),
            response: None,
            duration_ms: None,
            error: None,
        }
    }

    /// Create a code review response log entry
    pub fn code_review_response(
        request_id: &str,
        provider: &str,
        result: &Result<Vec<CodeIssue>>,
        duration: std::time::Duration,
    ) -> Self {
        let (response, error) = match result {
            Ok(issues) => (Some(ResponseSummary::from(issues)), None),
            Err(e) => (None, Some(e.to_string())),
        };

        Self {
            request_id: request_id.to_string(),
            timestamp: DebugProviderWrapper::timestamp(),
            provider: provider.to_string(),
            event_type: "code_review_response".to_string(),
            messages: None,
            tool_count: None,
            thinking_settings: None,
            fim_request: None,
            code_request: None,
            response,
            duration_ms: Some(duration.as_millis() as u64),
            error,
        }
    }
}

impl From<&Message> for MessageSummary {
    fn from(msg: &Message) -> Self {
        let content_string: String = match &msg.content {
            crate::llm::MessageContent::Text(t) => t.clone(),
            crate::llm::MessageContent::Parts(parts) => {
                // MessageContent::as_text() intentionally ignores tool parts.
                // For debug logs, include a compact representation so tool-only
                // assistant messages don't look like empty strings.
                let mut out = String::new();
                for part in parts {
                    match part {
                        crate::llm::ContentPart::Text { text } => {
                            out.push_str(text);
                        }
                        crate::llm::ContentPart::ToolUse {
                            id, name, input, ..
                        } => {
                            if !out.is_empty() {
                                out.push('\n');
                            }
                            let input_str =
                                serde_json::to_string(input).unwrap_or_else(|_| input.to_string());
                            out.push_str(&format!(
                                "[tool_use name={} id={} input={}]",
                                name,
                                id,
                                truncate_str(&input_str, 200)
                            ));
                        }
                        crate::llm::ContentPart::ToolResult {
                            tool_use_id,
                            content,
                        } => {
                            if !out.is_empty() {
                                out.push('\n');
                            }
                            out.push_str(&format!(
                                "[tool_result id={} content={}]",
                                tool_use_id,
                                truncate_str(content, 200)
                            ));
                        }
                    }
                }
                out
            }
        };
        Self {
            role: format!("{:?}", msg.role).to_lowercase(),
            content_preview: truncate_str(&content_string, 500),
            content_len: content_string.len(),
        }
    }
}

impl From<&ThinkSettings> for ThinkingSummary {
    fn from(settings: &ThinkSettings) -> Self {
        Self {
            enabled: settings.enabled,
            level_name: settings.level_name.clone(),
            budget_tokens: settings.budget_tokens,
            reasoning_effort: settings.reasoning_effort.clone(),
        }
    }
}

impl From<&LlmResponse> for ResponseSummary {
    fn from(response: &LlmResponse) -> Self {
        match response {
            LlmResponse::Text { text, usage } => Self {
                response_type: "text".to_string(),
                text_preview: Some(truncate_str(text, 500)),
                text_len: Some(text.len()),
                tool_calls: None,
                usage: usage.as_ref().map(TokenUsageSummary::from),
                fim_text: None,
                code_issues: None,
                refactorings: None,
            },
            LlmResponse::ToolCalls { calls, usage } => Self {
                response_type: "tool_calls".to_string(),
                text_preview: None,
                text_len: None,
                tool_calls: Some(calls.iter().map(ToolCallSummary::from).collect()),
                usage: usage.as_ref().map(TokenUsageSummary::from),
                fim_text: None,
                code_issues: None,
                refactorings: None,
            },
            LlmResponse::Mixed {
                text,
                tool_calls,
                usage,
            } => Self {
                response_type: "mixed".to_string(),
                text_preview: text.as_ref().map(|t| truncate_str(t, 500)),
                text_len: text.as_ref().map(|t| t.len()),
                tool_calls: Some(tool_calls.iter().map(ToolCallSummary::from).collect()),
                usage: usage.as_ref().map(TokenUsageSummary::from),
                fim_text: None,
                code_issues: None,
                refactorings: None,
            },
        }
    }
}

impl From<&CompletionResult> for ResponseSummary {
    fn from(result: &CompletionResult) -> Self {
        Self {
            response_type: "fim".to_string(),
            text_preview: None,
            text_len: None,
            tool_calls: None,
            usage: result.usage.as_ref().map(TokenUsageSummary::from),
            fim_text: Some(truncate_str(&result.text, 500)),
            code_issues: None,
            refactorings: None,
        }
    }
}

impl From<&str> for ResponseSummary {
    fn from(text: &str) -> Self {
        Self {
            response_type: "text".to_string(),
            text_preview: Some(truncate_str(text, 500)),
            text_len: Some(text.len()),
            tool_calls: None,
            usage: None,
            fim_text: None,
            code_issues: None,
            refactorings: None,
        }
    }
}

impl From<&Vec<RefactoringSuggestion>> for ResponseSummary {
    fn from(suggestions: &Vec<RefactoringSuggestion>) -> Self {
        Self {
            response_type: "refactorings".to_string(),
            text_preview: None,
            text_len: None,
            tool_calls: None,
            usage: None,
            fim_text: None,
            code_issues: None,
            refactorings: Some(suggestions.len()),
        }
    }
}

impl From<&Vec<CodeIssue>> for ResponseSummary {
    fn from(issues: &Vec<CodeIssue>) -> Self {
        Self {
            response_type: "code_issues".to_string(),
            text_preview: None,
            text_len: None,
            tool_calls: None,
            usage: None,
            fim_text: None,
            code_issues: Some(issues.len()),
            refactorings: None,
        }
    }
}

impl From<&ToolCall> for ToolCallSummary {
    fn from(call: &ToolCall) -> Self {
        let args_str = serde_json::to_string(&call.arguments).unwrap_or("{}".to_string());
        Self {
            id: call.id.clone(),
            name: call.name.clone(),
            args_preview: truncate_str(&args_str, 500),
        }
    }
}

impl From<&TokenUsage> for TokenUsageSummary {
    fn from(usage: &TokenUsage) -> Self {
        Self {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
        }
    }
}

/// Safely truncate a string to at most `max_bytes` bytes without splitting UTF-8 characters.
fn truncate_str(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    // Mock provider for testing
    struct MockProvider;
    #[async_trait]
    impl LlmProvider for MockProvider {
        fn name(&self) -> &str {
            "mock"
        }
        async fn chat(
            &self,
            messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<LlmResponse> {
            Ok(LlmResponse::Text {
                text: format!(
                    "Response to: {}",
                    messages
                        .first()
                        .map(|m| m.content.as_text().unwrap_or(""))
                        .unwrap_or("")
                ),
                usage: Some(TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                }),
            })
        }
        async fn complete_fim(
            &self,
            _prefix: &str,
            _suffix: &str,
            _language: &str,
        ) -> Result<CompletionResult> {
            Ok(CompletionResult {
                text: "completed".to_string(),
                usage: None,
            })
        }
        async fn explain_code(&self, _code: &str, _context: &str) -> Result<String> {
            Ok("This is test code".to_string())
        }
        async fn suggest_refactorings(
            &self,
            _code: &str,
            _context: &str,
        ) -> Result<Vec<RefactoringSuggestion>> {
            Ok(vec![])
        }
        async fn review_code(&self, _code: &str, _language: &str) -> Result<Vec<CodeIssue>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn test_debug_wrapper_logs_requests_and_responses() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        let mock_provider = Box::new(MockProvider);
        let wrapper = DebugProviderWrapper::new(mock_provider, &log_path).unwrap();

        let messages = vec![Message::user("Hello world")];

        // Make a request
        let result = wrapper.chat(&messages, None).await;
        assert!(result.is_ok());

        // Check that log file was created and contains expected entries
        let log_content = fs::read_to_string(&log_path).unwrap();

        // Should contain both request and response entries
        assert!(log_content.contains(r#""event_type":"request""#));
        assert!(log_content.contains(r#""event_type":"response""#));
        assert!(log_content.contains(r#""provider":"mock""#));

        // Parse as JSON lines and verify structure
        for line in log_content.lines() {
            let entry: serde_json::Value = serde_json::from_str(line).unwrap();
            assert!(entry.get("timestamp").is_some());
            assert!(entry.get("provider").is_some());
            assert!(entry.get("event_type").is_some());
        }
    }

    #[tokio::test]
    async fn test_debug_wrapper_includes_request_id() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        let mock_provider = Box::new(MockProvider);
        let wrapper = DebugProviderWrapper::new(mock_provider, &log_path).unwrap();

        let messages = vec![Message::user("Test message")];

        // Make a request
        let result = wrapper.chat(&messages, None).await;
        assert!(result.is_ok());

        // Read log file
        let log_content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = log_content.lines().collect();

        // Should have exactly 2 lines (request + response)
        assert_eq!(lines.len(), 2);

        // Parse both entries
        let request_entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
        let response_entry: serde_json::Value = serde_json::from_str(lines[1]).unwrap();

        // Both should have request_id
        let request_id = request_entry.get("request_id").and_then(|v| v.as_str());
        let response_id = response_entry.get("request_id").and_then(|v| v.as_str());

        assert!(request_id.is_some(), "Request entry should have request_id");
        assert!(
            response_id.is_some(),
            "Response entry should have request_id"
        );

        // request_id should match between request and response
        assert_eq!(
            request_id, response_id,
            "Request and response should have the same request_id"
        );

        // Verify the request_id is a valid UUID format (8-4-4-4-12 hex digits)
        let id = request_id.unwrap();
        assert_eq!(id.len(), 36, "UUID should be 36 characters");
        assert_eq!(
            id.chars().filter(|c| *c == '-').count(),
            4,
            "UUID should have 4 hyphens"
        );
    }

    #[tokio::test]
    async fn test_different_requests_have_different_ids() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        let mock_provider = Box::new(MockProvider);
        let wrapper = DebugProviderWrapper::new(mock_provider, &log_path).unwrap();

        let messages = vec![Message::user("First request")];

        // Make first request
        let result1 = wrapper.chat(&messages, None).await;
        assert!(result1.is_ok());

        // Make second request
        let messages2 = vec![Message::user("Second request")];
        let result2 = wrapper.chat(&messages2, None).await;
        assert!(result2.is_ok());

        // Read log file
        let log_content = fs::read_to_string(&log_path).unwrap();
        let lines: Vec<&str> = log_content.lines().collect();

        // Should have 4 lines (2 requests + 2 responses)
        assert_eq!(lines.len(), 4);

        // Extract all request_ids
        let request_ids: Vec<String> = lines
            .iter()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .map(|entry| {
                entry
                    .get("request_id")
                    .and_then(|v| v.as_str())
                    .unwrap()
                    .to_string()
            })
            .collect();

        // First request and response should share the same ID
        assert_eq!(request_ids[0], request_ids[1]);

        // Second request and response should share the same ID
        assert_eq!(request_ids[2], request_ids[3]);

        // But the two requests should have different IDs
        assert_ne!(
            request_ids[0], request_ids[2],
            "Different requests should have different request_ids"
        );
    }
}
