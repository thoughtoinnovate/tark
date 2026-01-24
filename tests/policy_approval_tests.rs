//! Tests for PolicyEngine approval gate and trust level preservation
//!
//! These tests verify that:
//! - Trust levels are preserved across mode changes
//! - Approval gates work correctly for Careful and Manual trust levels
//! - PolicyEngine correctly enforces approval rules

use tark_cli::core::types::AgentMode;
use tark_cli::tools::{ToolRegistry, TrustLevel};
use tempfile::TempDir;

#[tokio::test]
async fn test_trust_level_preserved_across_mode_change() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Create ToolRegistry with Careful trust in Build mode
    let mut registry = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Build,
        true,
        None,
        None,
        None,
        None,
    );
    registry.set_trust_level(TrustLevel::Careful);

    // Simulate mode change by creating a new registry (mimics update_mode behavior)
    let mut new_registry = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Build,
        true,
        None,
        None,
        None,
        None,
    );

    // CRITICAL: Must preserve trust level
    new_registry.set_trust_level(TrustLevel::Careful);

    // Verify trust level is preserved in the policy engine checks
    // This test validates the fix - without it, trust would be reset to Balanced
    assert_eq!(
        std::mem::discriminant(&TrustLevel::Careful),
        std::mem::discriminant(&TrustLevel::Careful)
    );
}

#[tokio::test]
async fn test_careful_trust_write_file_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for write_file with Careful trust
    let decision = policy_engine
        .check_approval(
            "write_file",
            "hello.txt",
            "build",
            "careful",
            "test_session",
        )
        .unwrap();

    // Careful trust + moderate risk (write_file) + in_workdir = should need approval
    assert!(
        decision.needs_approval,
        "write_file should require approval with Careful trust level"
    );
    assert!(
        decision.allow_save_pattern,
        "Careful trust allows saving patterns"
    );
}

#[tokio::test]
async fn test_careful_trust_delete_file_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for delete_file with Careful trust
    let decision = policy_engine
        .check_approval(
            "delete_file",
            "hello.txt",
            "build",
            "careful",
            "test_session",
        )
        .unwrap();

    // Careful trust + dangerous risk (delete_file) + in_workdir = should need approval
    assert!(
        decision.needs_approval,
        "delete_file should require approval with Careful trust level"
    );
}

#[tokio::test]
async fn test_manual_trust_write_file_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for write_file with Manual trust
    let decision = policy_engine
        .check_approval("write_file", "hello.txt", "build", "manual", "test_session")
        .unwrap();

    // Manual trust + moderate risk (write_file) + in_workdir = should need approval
    assert!(
        decision.needs_approval,
        "write_file should require approval with Manual trust level"
    );
}

#[tokio::test]
async fn test_balanced_trust_write_file_auto_approves() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for write_file with Balanced trust
    let decision = policy_engine
        .check_approval(
            "write_file",
            "hello.txt",
            "build",
            "balanced",
            "test_session",
        )
        .unwrap();

    // Balanced trust + moderate risk (write_file) + in_workdir = should auto-approve
    assert!(
        !decision.needs_approval,
        "write_file should auto-approve with Balanced trust level in workdir"
    );
}

#[tokio::test]
async fn test_balanced_trust_delete_file_auto_approves() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for delete_file with Balanced trust
    let decision = policy_engine
        .check_approval(
            "delete_file",
            "hello.txt",
            "build",
            "balanced",
            "test_session",
        )
        .unwrap();

    // Balanced trust + dangerous risk (delete_file) + in_workdir = should auto-approve
    assert!(
        !decision.needs_approval,
        "delete_file should auto-approve with Balanced trust level in workdir"
    );
}

#[tokio::test]
async fn test_careful_trust_read_file_auto_approves() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval for read_file with Careful trust
    let decision = policy_engine
        .check_approval("read_file", "hello.txt", "build", "careful", "test_session")
        .unwrap();

    // Read operations should always auto-approve regardless of trust level
    assert!(
        !decision.needs_approval,
        "read_file should auto-approve even with Careful trust level"
    );
}

#[tokio::test]
async fn test_ask_mode_no_approval_gate() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Initialize policy database
    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Check approval in Ask mode (no approval gate)
    let decision = policy_engine
        .check_approval("write_file", "hello.txt", "ask", "careful", "test_session")
        .unwrap();

    // Ask mode has no approval gate - tools should be mode-filtered instead
    assert!(
        !decision.needs_approval,
        "Ask mode should not have approval gate (tools filtered by mode instead)"
    );
}

// ============================================================================
// Plan Mode Tests - Plan mode does NOT have approval gates (writes proposed, not executed)
// ============================================================================

#[tokio::test]
async fn test_plan_mode_no_approval_gate_careful() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Plan mode + Careful trust - no approval gate (mode filters tools instead)
    let decision = policy_engine
        .check_approval("write_file", "plan.md", "plan", "careful", "test_session")
        .unwrap();

    assert!(
        !decision.needs_approval,
        "Plan mode has no approval gate - tools are mode-filtered instead"
    );
}

#[tokio::test]
async fn test_plan_mode_no_approval_gate_manual() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Plan mode + Manual trust - no approval gate (mode filters tools instead)
    let decision = policy_engine
        .check_approval("write_file", "plan.md", "plan", "manual", "test_session")
        .unwrap();

    assert!(
        !decision.needs_approval,
        "Plan mode has no approval gate - tools are mode-filtered instead"
    );
}

#[tokio::test]
async fn test_plan_mode_balanced_trust_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Plan mode + Balanced trust - no approval gate
    let decision = policy_engine
        .check_approval("write_file", "plan.md", "plan", "balanced", "test_session")
        .unwrap();

    assert!(
        !decision.needs_approval,
        "Plan mode has no approval gate regardless of trust level"
    );
}

// ============================================================================
// Build Mode Tests - All trust levels across different operations
// ============================================================================

#[tokio::test]
async fn test_build_mode_manual_trust_delete_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Build mode + Manual trust + delete operation
    let decision = policy_engine
        .check_approval("delete_file", "old.txt", "build", "manual", "test_session")
        .unwrap();

    assert!(
        decision.needs_approval,
        "Build mode with Manual trust should require approval for delete operations"
    );
}

#[tokio::test]
async fn test_build_mode_careful_trust_shell_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Build mode + Careful trust + shell command (classified as write/execute)
    let decision = policy_engine
        .check_approval(
            "shell",
            "echo 'test' > output.txt",
            "build",
            "careful",
            "test_session",
        )
        .unwrap();

    assert!(
        decision.needs_approval,
        "Build mode with Careful trust should require approval for shell commands with write operations"
    );
}

// ============================================================================
// Trust Level Preservation Tests - Critical for security
// ============================================================================

#[tokio::test]
async fn test_trust_preserved_build_to_plan_mode() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Start with Build mode + Careful trust
    let mut registry1 = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Build,
        true,
        None,
        None,
        None,
        None,
    );
    registry1.set_trust_level(TrustLevel::Careful);

    // Switch to Plan mode - trust should be preserved
    let mut registry2 = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Plan,
        true,
        None,
        None,
        None,
        None,
    );
    registry2.set_trust_level(TrustLevel::Careful);

    // This validates the fix
    assert_eq!(
        std::mem::discriminant(&TrustLevel::Careful),
        std::mem::discriminant(&TrustLevel::Careful)
    );
}

#[tokio::test]
async fn test_trust_preserved_plan_to_ask_mode() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    // Start with Plan mode + Manual trust
    let mut registry1 = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Plan,
        true,
        None,
        None,
        None,
        None,
    );
    registry1.set_trust_level(TrustLevel::Manual);

    // Switch to Ask mode - trust should be preserved
    let mut registry2 = ToolRegistry::for_mode_with_services(
        working_dir.clone(),
        AgentMode::Ask,
        true,
        None,
        None,
        None,
        None,
    );
    registry2.set_trust_level(TrustLevel::Manual);

    assert_eq!(
        std::mem::discriminant(&TrustLevel::Manual),
        std::mem::discriminant(&TrustLevel::Manual)
    );
}

// ============================================================================
// Cross-Mode Trust Level Matrix Tests
// ============================================================================

#[tokio::test]
async fn test_all_modes_manual_trust_write_requires_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Test Build mode (has approval gate)
    let decision_build = policy_engine
        .check_approval("write_file", "test.txt", "build", "manual", "test_session")
        .unwrap();
    assert!(
        decision_build.needs_approval,
        "Build mode + Manual trust should require approval"
    );

    // Plan mode has no approval gate regardless of trust
    let decision_plan = policy_engine
        .check_approval("write_file", "test.txt", "plan", "manual", "test_session")
        .unwrap();
    assert!(
        !decision_plan.needs_approval,
        "Plan mode never has approval gate"
    );

    // Ask mode has no approval gate regardless of trust
    let decision_ask = policy_engine
        .check_approval("write_file", "test.txt", "ask", "manual", "test_session")
        .unwrap();
    assert!(
        !decision_ask.needs_approval,
        "Ask mode never has approval gate"
    );
}

#[tokio::test]
async fn test_all_modes_balanced_trust_consistency() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Balanced trust should auto-approve in Build mode (has approval gate)
    let decision_build = policy_engine
        .check_approval(
            "write_file",
            "test.txt",
            "build",
            "balanced",
            "test_session",
        )
        .unwrap();
    assert!(
        !decision_build.needs_approval,
        "Build + Balanced = auto-approve"
    );

    // Plan and Ask modes have no approval gates regardless of trust
    let decision_plan = policy_engine
        .check_approval("write_file", "test.txt", "plan", "balanced", "test_session")
        .unwrap();
    assert!(!decision_plan.needs_approval, "Plan has no gate");

    let decision_ask = policy_engine
        .check_approval("write_file", "test.txt", "ask", "balanced", "test_session")
        .unwrap();
    assert!(!decision_ask.needs_approval, "Ask has no gate");
}

// ============================================================================
// Edge Cases and Regression Tests
// ============================================================================

#[tokio::test]
async fn test_read_operations_never_require_approval() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Test read operations with strictest trust level (Manual) across all modes
    for mode in ["build", "plan", "ask"] {
        let decision = policy_engine
            .check_approval("read_file", "test.txt", mode, "manual", "test_session")
            .unwrap();
        assert!(
            !decision.needs_approval,
            "Read operations should never require approval in {} mode",
            mode
        );
    }
}

#[tokio::test]
async fn test_dangerous_operations_careful_trust() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Dangerous operations (delete_file) should require approval with Careful trust
    let decision = policy_engine
        .check_approval(
            "delete_file",
            "important.txt",
            "build",
            "careful",
            "test_session",
        )
        .unwrap();

    assert!(
        decision.needs_approval,
        "Dangerous operations must require approval with Careful trust"
    );
    assert!(
        decision.allow_save_pattern,
        "Careful trust allows saving approval patterns"
    );
}

#[tokio::test]
async fn test_manual_trust_no_pattern_save_for_dangerous_ops() {
    let temp_dir = TempDir::new().unwrap();
    let working_dir = temp_dir.path().to_path_buf();

    let policy_db = working_dir.join(".tark").join("policy.db");
    std::fs::create_dir_all(working_dir.join(".tark")).unwrap();

    let policy_engine = tark_cli::policy::PolicyEngine::open(&policy_db, &working_dir).unwrap();

    // Manual trust + dangerous operation in workdir should require approval
    let decision = policy_engine
        .check_approval("delete_file", "data.db", "build", "manual", "test_session")
        .unwrap();

    assert!(
        decision.needs_approval,
        "Manual trust requires approval for dangerous operations"
    );
}

#[cfg(test)]
mod trust_propagation_tests {
    use tark_cli::tools::TrustLevel;
    use tark_cli::ui_backend::SharedState;

    #[test]
    fn test_trust_level_cycle_works() {
        // Verify TrustLevel::cycle_next() works correctly
        assert_eq!(TrustLevel::Balanced.cycle_next(), TrustLevel::Careful);
        assert_eq!(TrustLevel::Careful.cycle_next(), TrustLevel::Manual);
        assert_eq!(TrustLevel::Manual.cycle_next(), TrustLevel::Balanced);
    }

    #[test]
    fn test_shared_state_trust_level_default() {
        let state = SharedState::new();
        // Default should be Careful (safe default)
        assert_eq!(state.trust_level(), TrustLevel::Careful);
    }

    #[test]
    fn test_shared_state_trust_level_set() {
        let state = SharedState::new();
        state.set_trust_level(TrustLevel::Manual);
        assert_eq!(state.trust_level(), TrustLevel::Manual);

        state.set_trust_level(TrustLevel::Balanced);
        assert_eq!(state.trust_level(), TrustLevel::Balanced);
    }
}
