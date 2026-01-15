//! Optional raw LLM streaming log (debug-only)
//!
//! When enabled (via `tark --debug`), providers may append raw streaming payloads
//! to `llm_raw_response.log` for debugging/tracing.

use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

tokio::task_local! {
    /// Task-local storage for the current request ID (set by DebugProviderWrapper)
    pub static REQUEST_ID: String;
}

static RAW_LOG_PATH: OnceLock<Mutex<Option<PathBuf>>> = OnceLock::new();

fn cell() -> &'static Mutex<Option<PathBuf>> {
    RAW_LOG_PATH.get_or_init(|| Mutex::new(None))
}

/// Configure the raw log path. Use `None` to disable.
pub fn set_raw_log_path(path: Option<PathBuf>) {
    if let Ok(mut guard) = cell().lock() {
        *guard = path;
    }
}

/// Append a single line to the raw log (best-effort).
/// If a request_id is set in task-local context, it will be included.
pub fn append_raw_line(line: &str) {
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    // Try to get request_id from task-local context
    if let Ok(request_id) = REQUEST_ID.try_with(|id| id.clone()) {
        // Log with request_id as JSONL
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let entry = serde_json::json!({
                "request_id": request_id,
                "raw": line
            });
            let _ = writeln!(f, "{}", entry);
        }
    } else {
        // Fallback: log without request_id (for backward compatibility)
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{}", line);
        }
    }
}

// ============================================================================
// Structured Debug Logging Helpers
// ============================================================================

/// Log a structured request event
pub fn log_request(provider: &str, model: &str, messages_count: usize, has_tools: bool) {
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    let request_id = REQUEST_ID.try_with(|id| id.clone()).ok();

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let entry = serde_json::json!({
            "type": "request",
            "request_id": request_id,
            "provider": provider,
            "model": model,
            "messages_count": messages_count,
            "has_tools": has_tools,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        let _ = writeln!(f, "{}", entry);
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
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    let request_id = REQUEST_ID.try_with(|id| id.clone()).ok();

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let entry = serde_json::json!({
            "type": "thinking",
            "request_id": request_id,
            "provider": provider,
            "content": content,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        let _ = writeln!(f, "{}", entry);
    }

    // Log a truncated version to tracing
    let truncated = if content.len() > 200 {
        format!("{}...", &content[..200])
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
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    let request_id = REQUEST_ID.try_with(|id| id.clone()).ok();

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let entry = serde_json::json!({
            "type": "tool_call",
            "request_id": request_id,
            "provider": provider,
            "tool_id": tool_id,
            "tool_name": tool_name,
            "arguments": arguments,
            "has_thought_signature": has_thought_signature,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        let _ = writeln!(f, "{}", entry);
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
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    let request_id = REQUEST_ID.try_with(|id| id.clone()).ok();

    // Truncate result preview
    let preview = if result_preview.len() > 500 {
        format!("{}...", &result_preview[..500])
    } else {
        result_preview.to_string()
    };

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let entry = serde_json::json!({
            "type": "tool_result",
            "request_id": request_id,
            "provider": provider,
            "tool_id": tool_id,
            "result_preview": preview,
            "is_error": is_error,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        let _ = writeln!(f, "{}", entry);
    }

    tracing::debug!(
        target: "llm",
        provider = provider,
        tool_id = tool_id,
        is_error = is_error,
        "LLM tool result"
    );
}

/// Append a line to the raw log with a request_id prefix (best-effort).
pub fn append_raw_line_with_id(request_id: &str, line: &str) {
    let path = match cell().lock().ok().and_then(|g| g.clone()) {
        Some(p) => p,
        None => return,
    };

    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        // Log as JSONL with request_id and the raw content
        let entry = serde_json::json!({
            "request_id": request_id,
            "raw": line
        });
        let _ = writeln!(f, "{}", entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::NamedTempFile;

    #[test]
    fn test_append_raw_line_without_request_id() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        // Set the raw log path
        set_raw_log_path(Some(log_path.clone()));

        // Append a line without request_id context
        append_raw_line("test raw line");

        // Read the file
        let content = fs::read_to_string(&log_path).unwrap();

        // Should contain the raw line (fallback mode)
        assert!(content.contains("test raw line"));

        // Clean up
        set_raw_log_path(None);
    }

    #[tokio::test]
    async fn test_append_raw_line_with_request_id_context() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        // Set the raw log path
        set_raw_log_path(Some(log_path.clone()));

        // Use REQUEST_ID scope to set a request ID
        let request_id = "test-request-123";
        REQUEST_ID
            .scope(request_id.to_string(), async {
                append_raw_line("test raw line with context");
            })
            .await;

        // Read the file
        let content = fs::read_to_string(&log_path).unwrap();

        // Parse as JSON
        let line = content.lines().next().unwrap();
        let entry: serde_json::Value = serde_json::from_str(line).unwrap();

        // Should have request_id and raw fields
        assert_eq!(
            entry.get("request_id").and_then(|v| v.as_str()),
            Some(request_id)
        );
        assert_eq!(
            entry.get("raw").and_then(|v| v.as_str()),
            Some("test raw line with context")
        );

        // Clean up
        set_raw_log_path(None);
    }

    #[test]
    fn test_append_raw_line_with_id_directly() {
        let temp_file = NamedTempFile::new().unwrap();
        let log_path = temp_file.path().to_path_buf();

        // Set the raw log path
        set_raw_log_path(Some(log_path.clone()));

        // Append with explicit request_id
        append_raw_line_with_id("explicit-id-456", "direct raw line");

        // Read the file
        let content = fs::read_to_string(&log_path).unwrap();

        // Parse as JSON
        let line = content.lines().next().unwrap();
        let entry: serde_json::Value = serde_json::from_str(line).unwrap();

        // Should have the explicit request_id
        assert_eq!(
            entry.get("request_id").and_then(|v| v.as_str()),
            Some("explicit-id-456")
        );
        assert_eq!(
            entry.get("raw").and_then(|v| v.as_str()),
            Some("direct raw line")
        );

        // Clean up
        set_raw_log_path(None);
    }
}
