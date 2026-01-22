//! Optional raw LLM streaming log (debug-only)
//!
//! When enabled (via `tark --debug`), providers may append raw streaming payloads
//! to the unified debug log with category `llm_raw`.

tokio::task_local! {
    /// Task-local storage for the current correlation ID (set by DebugProviderWrapper)
    pub static CORRELATION_ID: String;
}

/// Append a single line to the raw log (best-effort).
/// If a correlation_id is set in task-local context, it will be included.
pub fn append_raw_line(line: &str) {
    // Try to get correlation_id from task-local context
    let correlation_id = CORRELATION_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    // Log using unified debug logger
    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "stream_chunk")
                .with_data(serde_json::json!({
                    "data": line
                }));
        logger.log(entry);
    }
}

// ============================================================================
// Structured Debug Logging Helpers
// ============================================================================

/// Log a structured request event
pub fn log_request(provider: &str, model: &str, messages_count: usize, has_tools: bool) {
    let correlation_id = CORRELATION_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "request")
                .with_data(serde_json::json!({
                    "provider": provider,
                    "model": model,
                    "messages_count": messages_count,
                    "has_tools": has_tools
                }));
        logger.log(entry);
    }

    tracing::debug!(
        target: "llm",
        provider = provider,
        model = model,
        messages = messages_count,
        has_tools = has_tools,
        "LLM request"
    );
}

/// Log a thinking/reasoning event
pub fn log_thinking(provider: &str, content: &str) {
    let correlation_id = CORRELATION_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "thinking")
                .with_data(serde_json::json!({
                    "provider": provider,
                    "content": content
                }));
        logger.log(entry);
    }

    // Log a truncated version to tracing
    let truncated = if content.len() > 200 {
        format!(
            "{}...",
            crate::core::truncate_at_char_boundary(content, 200)
        )
    } else {
        content.to_string()
    };

    tracing::debug!(
        target: "llm",
        provider = provider,
        thinking = %truncated,
        "LLM thinking"
    );
}

/// Log a tool call event
pub fn log_tool_call(
    provider: &str,
    tool_id: &str,
    tool_name: &str,
    arguments: &serde_json::Value,
    has_thought_signature: bool,
) {
    let correlation_id = CORRELATION_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "tool_call")
                .with_data(serde_json::json!({
                    "provider": provider,
                    "tool_id": tool_id,
                    "tool_name": tool_name,
                    "arguments": arguments,
                    "has_thought_signature": has_thought_signature
                }));
        logger.log(entry);
    }

    tracing::debug!(
        target: "llm",
        provider = provider,
        tool_id = tool_id,
        tool_name = tool_name,
        has_thought_signature = has_thought_signature,
        "LLM tool call"
    );
}

/// Log a tool result event
pub fn log_tool_result(provider: &str, tool_id: &str, result_preview: &str, is_error: bool) {
    let correlation_id = CORRELATION_ID
        .try_with(|id| id.clone())
        .unwrap_or_else(|_| "unknown".to_string());

    // Truncate result preview
    let preview = if result_preview.len() > 500 {
        format!(
            "{}...",
            crate::core::truncate_at_char_boundary(result_preview, 500)
        )
    } else {
        result_preview.to_string()
    };

    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "tool_result")
                .with_data(serde_json::json!({
                    "provider": provider,
                    "tool_id": tool_id,
                    "result_preview": preview,
                    "is_error": is_error
                }));
        logger.log(entry);
    }

    tracing::debug!(
        target: "llm",
        provider = provider,
        tool_id = tool_id,
        is_error = is_error,
        "LLM tool result"
    );
}

/// Append a line to the raw log with a correlation_id prefix (best-effort).
pub fn append_raw_line_with_id(correlation_id: &str, line: &str) {
    if let Some(logger) = crate::debug_logger() {
        let entry: crate::DebugLogEntry =
            crate::DebugLogEntry::new(correlation_id, crate::LogCategory::LlmRaw, "stream_chunk")
                .with_data(serde_json::json!({
                    "data": line
                }));
        logger.log(entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_append_raw_line_with_correlation_id_context() {
        let temp_dir = TempDir::new().unwrap();
        let debug_config = crate::DebugLoggerConfig {
            log_dir: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            max_rotated_files: 3,
        };

        // Initialize debug logger (this might fail if already init in another test, ignore error)
        let _ = crate::init_debug_logger(debug_config);

        // Use CORRELATION_ID scope to set a correlation ID
        let correlation_id_val = "test-correlation-123";
        CORRELATION_ID
            .scope(correlation_id_val.to_string(), async {
                append_raw_line("test raw line with context");
            })
            .await;

        // Read the log file
        let log_path = temp_dir.path().join("tark-debug.log");

        // Note: If logger was already initialized elsewhere, logs might not be in this file
        // This test mainly verifies the function doesn't panic
        if log_path.exists() {
            let content = fs::read_to_string(&log_path).unwrap();
            if !content.is_empty() {
                let line = content.lines().next().unwrap();
                let entry: serde_json::Value = serde_json::from_str(line).unwrap();
                assert!(entry.get("correlation_id").is_some());
                assert_eq!(
                    entry.get("category").and_then(|v| v.as_str()),
                    Some("llm_raw")
                );
            }
        }
    }

    #[test]
    fn test_append_raw_line_with_id_directly() {
        let temp_dir = TempDir::new().unwrap();
        let debug_config = crate::DebugLoggerConfig {
            log_dir: temp_dir.path().to_path_buf(),
            max_file_size: 10 * 1024 * 1024,
            max_rotated_files: 3,
        };

        // Initialize debug logger (this might fail if already init in another test, ignore error)
        let _ = crate::init_debug_logger(debug_config);

        // Append with explicit correlation_id
        append_raw_line_with_id("explicit-id-456", "direct raw line");

        // Read the log file
        let log_path = temp_dir.path().join("tark-debug.log");

        // Note: If logger was already initialized elsewhere, logs might not be in this file
        // This test mainly verifies the function doesn't panic
        if log_path.exists() {
            let content = fs::read_to_string(&log_path).unwrap();
            if !content.is_empty() {
                let line = content.lines().last().unwrap(); // Get last line in case other tests wrote to it
                let entry: serde_json::Value = serde_json::from_str(line).unwrap();
                assert!(entry.get("correlation_id").is_some());
            }
        }
    }
}
