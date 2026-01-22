//! Tests for built-in tools (thinking and memory).

use serde_json::json;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

// Import from tark
use tark_cli::tools::builtin::{
    MemoryQueryTool, MemoryStoreTool, TarkMemory, ThinkTool, ThinkingTracker,
};
use tark_cli::tools::Tool;

#[tokio::test]
async fn test_think_tool_basic_flow() {
    let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
    let tool = ThinkTool::new(tracker.clone());

    let result = tool
        .execute(json!({
            "thought": "Let me analyze the problem",
            "thought_number": 1,
            "total_thoughts": 3,
            "next_thought_needed": true
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert!(result.output.contains("thought_number"));
}

#[tokio::test]
async fn test_memory_store_and_query() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_memory.db");
    let memory = Arc::new(TarkMemory::open(&db_path).unwrap());

    let store_tool = MemoryStoreTool::new(memory.clone());
    let query_tool = MemoryQueryTool::new(memory.clone());

    // Store a memory
    let result = store_tool
        .execute(json!({
            "key": "database_choice",
            "value": "We decided to use PostgreSQL",
            "category": "decision",
            "importance": 8
        }))
        .await
        .unwrap();

    assert!(result.success);

    // Query it back
    let result = query_tool
        .execute(json!({
            "search": "PostgreSQL"
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert!(result.output.contains("database_choice"));
}
