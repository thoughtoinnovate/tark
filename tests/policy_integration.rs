//! Integration tests for Policy Engine
//!
//! Tests end-to-end approval flows across different modes and trust levels.

use anyhow::Result;
use tempfile::TempDir;

use tark_cli::policy::PolicyEngine;

/// Test helper to create a PolicyEngine with a temporary database
fn setup_engine() -> Result<(TempDir, PolicyEngine)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("policy.db");
    let work_dir = temp_dir.path().to_path_buf();

    let engine = PolicyEngine::open(&db_path, &work_dir)?;
    Ok((temp_dir, engine))
}

#[test]
fn test_ask_mode_no_approval_required() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    // Ask mode should never require approval
    let decision = engine.check_approval("shell", "rm -rf /", "ask", "balanced", "session1")?;

    assert!(
        !decision.needs_approval,
        "Ask mode should not require approval"
    );
    Ok(())
}

#[test]
fn test_plan_mode_no_approval_required() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    // Plan mode should never require approval
    let decision = engine.check_approval("shell", "rm -rf /", "plan", "balanced", "session1")?;

    assert!(
        !decision.needs_approval,
        "Plan mode should not require approval"
    );
    Ok(())
}

#[test]
fn test_build_balanced_read_in_workdir_auto_approved() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "cat file.txt", "build", "balanced", "session1")?;

    assert!(
        !decision.needs_approval,
        "Read in workdir should be auto-approved"
    );
    assert_eq!(
        decision.classification.operation,
        tark_cli::policy::Operation::Read
    );
    assert!(decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_balanced_read_outside_workdir_auto_approved() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "cat /etc/passwd", "build", "balanced", "session1")?;

    assert!(
        !decision.needs_approval,
        "Read outside workdir should be auto-approved in balanced"
    );
    assert_eq!(
        decision.classification.operation,
        tark_cli::policy::Operation::Read
    );
    assert!(!decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_careful_read_outside_workdir_needs_approval() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "cat /etc/passwd", "build", "careful", "session1")?;

    assert!(
        decision.needs_approval,
        "Read outside workdir should need approval in careful"
    );
    assert!(decision.allow_save_pattern, "Should allow saving pattern");
    assert!(!decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_balanced_write_in_workdir_auto_approved() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision = engine.check_approval(
        "shell",
        "echo test > file.txt",
        "build",
        "balanced",
        "session1",
    )?;

    assert!(
        !decision.needs_approval,
        "Write in workdir should be auto-approved in balanced"
    );
    assert_eq!(
        decision.classification.operation,
        tark_cli::policy::Operation::Write
    );
    assert!(decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_balanced_write_outside_workdir_needs_approval() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision = engine.check_approval(
        "shell",
        "echo test > /tmp/file.txt",
        "build",
        "balanced",
        "session1",
    )?;

    assert!(
        decision.needs_approval,
        "Write outside workdir should need approval"
    );
    assert!(decision.allow_save_pattern, "Should allow saving pattern");
    assert!(!decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_careful_write_in_workdir_needs_approval() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision = engine.check_approval(
        "shell",
        "echo test > file.txt",
        "build",
        "careful",
        "session1",
    )?;

    assert!(
        decision.needs_approval,
        "Write in workdir should need approval in careful"
    );
    assert!(decision.allow_save_pattern, "Should allow saving pattern");
    Ok(())
}

#[test]
fn test_build_balanced_rm_in_workdir_auto_approved() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm file.txt", "build", "balanced", "session1")?;

    assert!(
        !decision.needs_approval,
        "Rm in workdir should be auto-approved in balanced"
    );
    assert_eq!(
        decision.classification.operation,
        tark_cli::policy::Operation::Delete
    );
    assert!(decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_balanced_rm_outside_workdir_needs_approval() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm /tmp/file.txt", "build", "balanced", "session1")?;

    assert!(
        decision.needs_approval,
        "Rm outside workdir should need approval"
    );
    assert!(
        decision.allow_save_pattern,
        "Should allow saving pattern in balanced"
    );
    assert!(!decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_careful_rm_outside_workdir_always_prompts() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm /tmp/file.txt", "build", "careful", "session1")?;

    assert!(
        decision.needs_approval,
        "Rm outside workdir should ALWAYS need approval in careful"
    );
    assert!(
        !decision.allow_save_pattern,
        "Should NOT allow saving pattern (ALWAYS mode)"
    );
    assert!(!decision.classification.in_workdir);
    Ok(())
}

#[test]
fn test_build_manual_rm_outside_workdir_always_prompts() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm -rf /tmp/test", "build", "manual", "session1")?;

    assert!(
        decision.needs_approval,
        "Rm outside workdir should ALWAYS need approval in manual"
    );
    assert!(
        !decision.allow_save_pattern,
        "Should NOT allow saving pattern (ALWAYS mode)"
    );
    Ok(())
}

#[test]
fn test_command_classification_read() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let read_commands = vec![
        "cat file.txt",
        "ls -la",
        "grep pattern file.txt",
        "git status",
        "npm list",
    ];

    for cmd in read_commands {
        let classification = engine.classify_command(cmd)?;
        assert_eq!(
            classification.operation,
            tark_cli::policy::Operation::Read,
            "Command '{}' should be classified as Read",
            cmd
        );
    }

    Ok(())
}

#[test]
fn test_command_classification_write() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let write_commands = vec![
        "echo test > file.txt",
        "touch newfile.txt",
        "npm install express",
        "git commit -m 'test'",
        "mkdir newdir",
    ];

    for cmd in write_commands {
        let classification = engine.classify_command(cmd)?;
        assert_eq!(
            classification.operation,
            tark_cli::policy::Operation::Write,
            "Command '{}' should be classified as Write",
            cmd
        );
    }

    Ok(())
}

#[test]
fn test_command_classification_delete() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let delete_commands = vec![
        "rm file.txt",
        "rm -rf directory",
        "rmdir emptydir",
        "git clean -fd",
    ];

    for cmd in delete_commands {
        let classification = engine.classify_command(cmd)?;
        assert_eq!(
            classification.operation,
            tark_cli::policy::Operation::Delete,
            "Command '{}' should be classified as Delete",
            cmd
        );
    }

    Ok(())
}

#[test]
fn test_compound_command_classification() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    // Compound with highest risk = delete
    let classification = engine.classify_command("ls && rm file.txt")?;
    assert_eq!(
        classification.operation,
        tark_cli::policy::Operation::Delete,
        "Compound should take highest risk (delete)"
    );

    // Compound with write
    let classification = engine.classify_command("cat file.txt && echo test > out.txt")?;
    assert_eq!(
        classification.operation,
        tark_cli::policy::Operation::Write,
        "Compound should take highest risk (write)"
    );

    Ok(())
}

#[test]
fn test_path_detection_relative() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm ./file.txt", "build", "balanced", "session1")?;

    assert!(
        decision.classification.in_workdir,
        "Relative path should be in_workdir"
    );
    Ok(())
}

#[test]
fn test_path_detection_absolute() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let decision =
        engine.check_approval("shell", "rm /tmp/file.txt", "build", "balanced", "session1")?;

    assert!(
        !decision.classification.in_workdir,
        "Absolute path /tmp should be outside workdir"
    );
    Ok(())
}

#[test]
fn test_get_available_tools_ask_mode() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let tools = engine.get_available_tools("ask")?;

    // Ask mode should not have shell tool
    assert!(
        !tools.iter().any(|t| t.id == "shell"),
        "Ask mode should not have shell tool"
    );

    // Should have safe_shell
    assert!(
        tools.iter().any(|t| t.id == "safe_shell"),
        "Ask mode should have safe_shell tool"
    );

    Ok(())
}

#[test]
fn test_get_available_tools_build_mode() -> Result<()> {
    let (_temp, engine) = setup_engine()?;

    let tools = engine.get_available_tools("build")?;

    // Build mode should have shell tool
    assert!(
        tools.iter().any(|t| t.id == "shell"),
        "Build mode should have shell tool"
    );

    Ok(())
}

#[test]
fn test_audit_logging() -> Result<()> {
    use tark_cli::policy::{ApprovalDecisionType, AuditEntry};

    let (_temp, engine) = setup_engine()?;

    // Log a decision
    let entry = AuditEntry {
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_id: "shell".to_string(),
        command: "rm file.txt".to_string(),
        classification_id: Some("shell-rm".to_string()),
        mode_id: "build".to_string(),
        trust_id: Some("balanced".to_string()),
        decision: ApprovalDecisionType::AutoApproved,
        matched_pattern_id: None,
        session_id: "session1".to_string(),
        working_directory: "/work".to_string(),
    };

    engine.log_decision(entry)?;

    // Verification would require direct DB access, which we've logged successfully
    Ok(())
}
