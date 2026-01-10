//! Tests for Plan mode tool isolation
//!
//! These tests verify that:
//! 1. Plan mode only has read-only tools available
//! 2. Write tools (write_file, patch_file, delete_file, shell) are NOT available in Plan mode
//! 3. Build mode has all tools available
//! 4. Calling unavailable tools returns "Unknown tool" error

use serde_json::json;
use tempfile::TempDir;

// Import from the library
use tark_cli::tools::{AgentMode, ToolRegistry};

/// Get all tool names from a registry
fn get_tool_names(registry: &ToolRegistry) -> Vec<String> {
    registry
        .definitions()
        .iter()
        .map(|t| t.name.clone())
        .collect()
}

// ============================================================================
// Plan Mode Isolation Tests
// ============================================================================

#[test]
fn test_plan_mode_excludes_write_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        !tool_names.contains(&"write_file".to_string()),
        "Plan mode should NOT have write_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_plan_mode_excludes_patch_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        !tool_names.contains(&"patch_file".to_string()),
        "Plan mode should NOT have patch_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_plan_mode_excludes_delete_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        !tool_names.contains(&"delete_file".to_string()),
        "Plan mode should NOT have delete_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_plan_mode_includes_safe_shell() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    // Plan mode now includes safe_shell (read-only commands only)
    assert!(
        tool_names.contains(&"shell".to_string()),
        "Plan mode should have shell (safe) tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_plan_mode_has_read_only_tools() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    // Plan mode should have these read-only tools
    let expected_tools = vec![
        "read_file",
        "read_files",
        "list_directory",
        "file_search",
        "grep",
        "codebase_overview",
        "propose_change", // Plan mode specific - shows diffs without applying
    ];

    for tool in expected_tools {
        assert!(
            tool_names.contains(&tool.to_string()),
            "Plan mode should have {} tool. Available tools: {:?}",
            tool,
            tool_names
        );
    }
}

#[test]
fn test_plan_mode_has_propose_change() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"propose_change".to_string()),
        "Plan mode should have propose_change tool for showing diffs. Available tools: {:?}",
        tool_names
    );
}

// ============================================================================
// Build Mode Tests
// ============================================================================

#[test]
fn test_build_mode_has_write_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"write_file".to_string()),
        "Build mode should have write_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_build_mode_has_patch_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"patch_file".to_string()),
        "Build mode should have patch_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_build_mode_has_delete_file() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"delete_file".to_string()),
        "Build mode should have delete_file tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_build_mode_has_shell_when_enabled() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"shell".to_string()),
        "Build mode with shell_enabled=true should have shell tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_build_mode_no_shell_when_disabled() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, false);
    let tool_names = get_tool_names(&registry);

    assert!(
        !tool_names.contains(&"shell".to_string()),
        "Build mode with shell_enabled=false should NOT have shell tool. Available tools: {:?}",
        tool_names
    );
}

#[test]
fn test_build_mode_does_not_have_propose_change() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    // Build mode uses write_file directly, not propose_change
    assert!(
        !tool_names.contains(&"propose_change".to_string()),
        "Build mode should NOT have propose_change (uses write_file instead). Available tools: {:?}",
        tool_names
    );
}

// ============================================================================
// Tool Execution Isolation Tests
// ============================================================================

#[tokio::test]
async fn test_plan_mode_rejects_write_file_execution() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    // Try to execute write_file in Plan mode - should fail with "Unknown tool"
    let result = registry
        .execute(
            "write_file",
            json!({
                "path": "test.txt",
                "content": "should not be written"
            }),
        )
        .await
        .unwrap();

    assert!(!result.success, "write_file should fail in Plan mode");
    assert!(
        result.output.contains("Unknown tool"),
        "Error should indicate unknown tool. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_plan_mode_rejects_patch_file_execution() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    let result = registry
        .execute(
            "patch_file",
            json!({
                "path": "test.txt",
                "patches": []
            }),
        )
        .await
        .unwrap();

    assert!(!result.success, "patch_file should fail in Plan mode");
    assert!(
        result.output.contains("Unknown tool"),
        "Error should indicate unknown tool. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_plan_mode_rejects_delete_file_execution() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    let result = registry
        .execute(
            "delete_file",
            json!({
                "path": "test.txt"
            }),
        )
        .await
        .unwrap();

    assert!(!result.success, "delete_file should fail in Plan mode");
    assert!(
        result.output.contains("Unknown tool"),
        "Error should indicate unknown tool. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_plan_mode_shell_allows_safe_commands() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    // Safe commands should work in Plan mode (safe_shell)
    let result = registry
        .execute(
            "shell",
            json!({
                "command": "pwd"
            }),
        )
        .await
        .unwrap();

    assert!(
        result.success,
        "Safe shell commands should work in Plan mode. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_plan_mode_shell_rejects_unsafe_commands() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    // Unsafe commands should be rejected in Plan mode
    let result = registry
        .execute(
            "shell",
            json!({
                "command": "npm install something"
            }),
        )
        .await
        .unwrap();

    assert!(
        !result.success,
        "Unsafe shell commands should fail in Plan mode"
    );
    assert!(
        result.output.contains("not allowed"),
        "Error should indicate command is not allowed. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_plan_mode_allows_read_file_execution() {
    let tmp = TempDir::new().unwrap();

    // Create a test file to read
    let test_file = tmp.path().join("test.txt");
    std::fs::write(&test_file, "test content").unwrap();

    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    let result = registry
        .execute(
            "read_file",
            json!({
                "path": "test.txt"
            }),
        )
        .await
        .unwrap();

    assert!(
        result.success,
        "read_file should succeed in Plan mode. Error: {}",
        result.output
    );
    assert!(
        result.output.contains("test content"),
        "read_file should return file content. Got: {}",
        result.output
    );
}

#[tokio::test]
async fn test_build_mode_allows_write_file_execution() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);

    let result = registry
        .execute(
            "write_file",
            json!({
                "path": "test.txt",
                "content": "test content"
            }),
        )
        .await
        .unwrap();

    assert!(
        result.success,
        "write_file should succeed in Build mode. Error: {}",
        result.output
    );

    // Verify file was actually created
    let written_content = std::fs::read_to_string(tmp.path().join("test.txt")).unwrap();
    assert_eq!(written_content, "test content");
}

// ============================================================================
// File Safety Tests - Verify Plan mode can't modify files
// ============================================================================

#[tokio::test]
async fn test_plan_mode_cannot_create_files() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    let test_file = tmp.path().join("should_not_exist.txt");

    // Try to create a file via write_file
    let _ = registry
        .execute(
            "write_file",
            json!({
                "path": "should_not_exist.txt",
                "content": "malicious content"
            }),
        )
        .await;

    // File should NOT exist
    assert!(
        !test_file.exists(),
        "Plan mode should not be able to create files"
    );
}

#[tokio::test]
async fn test_plan_mode_cannot_modify_files() {
    let tmp = TempDir::new().unwrap();

    // Create an existing file
    let test_file = tmp.path().join("existing.txt");
    std::fs::write(&test_file, "original content").unwrap();

    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    // Try to modify via write_file
    let _ = registry
        .execute(
            "write_file",
            json!({
                "path": "existing.txt",
                "content": "modified content"
            }),
        )
        .await;

    // File should still have original content
    let content = std::fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "original content",
        "Plan mode should not be able to modify files"
    );
}

#[tokio::test]
async fn test_plan_mode_cannot_delete_files() {
    let tmp = TempDir::new().unwrap();

    // Create an existing file
    let test_file = tmp.path().join("should_not_delete.txt");
    std::fs::write(&test_file, "important content").unwrap();

    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);

    // Try to delete
    let _ = registry
        .execute(
            "delete_file",
            json!({
                "path": "should_not_delete.txt"
            }),
        )
        .await;

    // File should still exist
    assert!(
        test_file.exists(),
        "Plan mode should not be able to delete files"
    );
}

// ============================================================================
// Review Mode Tests (same tools as Build mode)
// ============================================================================

#[test]
fn test_ask_mode_is_read_only() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Ask, true);
    let tool_names = get_tool_names(&registry);

    // Ask mode should NOT have write tools
    assert!(
        !tool_names.contains(&"write_file".to_string()),
        "Ask mode should NOT have write_file tool"
    );
    assert!(
        !tool_names.contains(&"patch_file".to_string()),
        "Ask mode should NOT have patch_file tool"
    );
    assert!(
        !tool_names.contains(&"delete_file".to_string()),
        "Ask mode should NOT have delete_file tool"
    );

    // Ask mode should have read tools
    assert!(
        tool_names.contains(&"read_file".to_string()),
        "Ask mode should have read_file tool"
    );
    assert!(
        tool_names.contains(&"grep".to_string()),
        "Ask mode should have grep tool"
    );
}

// ============================================================================
// Mode Comparison Tests
// ============================================================================

#[test]
fn test_plan_mode_has_fewer_tools_than_build_mode() {
    let tmp = TempDir::new().unwrap();

    let plan_registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let build_registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);

    let plan_tools = get_tool_names(&plan_registry);
    let build_tools = get_tool_names(&build_registry);

    // Plan mode should have fewer tools (no write tools)
    // Note: Plan has propose_change and safe_shell which Build doesn't,
    // but it should not have the dangerous write tools
    let write_only_tools = vec!["write_file", "patch_file", "delete_file"];

    for tool in write_only_tools {
        assert!(
            !plan_tools.contains(&tool.to_string()),
            "Plan mode should not have {} tool",
            tool
        );
        assert!(
            build_tools.contains(&tool.to_string()),
            "Build mode should have {} tool",
            tool
        );
    }

    // Both Plan and Build modes have shell (different versions)
    // Plan has safe_shell, Build has full shell
    assert!(
        plan_tools.contains(&"shell".to_string()),
        "Plan mode should have shell (safe) tool"
    );
    assert!(
        build_tools.contains(&"shell".to_string()),
        "Build mode should have shell tool"
    );
}

// ============================================================================
// New Tool Tests (Ripgrep, FilePreview, SafeShell)
// ============================================================================

#[test]
fn test_all_modes_have_grep_tool() {
    let tmp = TempDir::new().unwrap();

    for mode in [AgentMode::Ask, AgentMode::Plan, AgentMode::Build] {
        let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), mode, true);
        let tool_names = get_tool_names(&registry);

        assert!(
            tool_names.contains(&"grep".to_string()),
            "{:?} mode should have grep tool",
            mode
        );
    }
}

#[test]
fn test_all_modes_have_file_preview_tool() {
    let tmp = TempDir::new().unwrap();

    for mode in [AgentMode::Ask, AgentMode::Plan, AgentMode::Build] {
        let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), mode, true);
        let tool_names = get_tool_names(&registry);

        assert!(
            tool_names.contains(&"file_preview".to_string()),
            "{:?} mode should have file_preview tool",
            mode
        );
    }
}

#[test]
fn test_ask_mode_has_safe_shell() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Ask, true);
    let tool_names = get_tool_names(&registry);

    // Ask mode should have shell (the safe version)
    assert!(
        tool_names.contains(&"shell".to_string()),
        "Ask mode should have shell tool (safe version)"
    );
}

#[test]
fn test_build_mode_has_full_shell() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Build, true);
    let tool_names = get_tool_names(&registry);

    assert!(
        tool_names.contains(&"shell".to_string()),
        "Build mode should have shell tool"
    );
}

#[test]
fn test_plan_mode_has_safe_shell() {
    let tmp = TempDir::new().unwrap();
    let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), AgentMode::Plan, true);
    let tool_names = get_tool_names(&registry);

    // Plan mode has safe_shell (limited to read-only commands)
    assert!(
        tool_names.contains(&"shell".to_string()),
        "Plan mode should have shell (safe) tool"
    );
}

// ============================================================================
// Risk Level Tests
// ============================================================================

#[test]
fn test_readonly_tools_in_all_modes() {
    let tmp = TempDir::new().unwrap();
    let readonly_tools = vec![
        "read_file",
        "read_files",
        "list_directory",
        "file_search",
        "grep",
        "file_preview",
        "codebase_overview",
    ];

    for mode in [AgentMode::Ask, AgentMode::Plan, AgentMode::Build] {
        let registry = ToolRegistry::for_mode(tmp.path().to_path_buf(), mode, true);
        let tool_names = get_tool_names(&registry);

        for tool in &readonly_tools {
            assert!(
                tool_names.contains(&tool.to_string()),
                "{:?} mode should have {} tool",
                mode,
                tool
            );
        }
    }
}
