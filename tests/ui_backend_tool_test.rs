//! Tests for ToolExecutionService
//!
//! Real behavior tests for tool availability and approval flow.
//!
//! NOTE: Tests related to the old ApprovalGate API have been disabled/removed
//! as the approval system is now handled by PolicyEngine integrated into ToolRegistry.

use tark_cli::core::AgentMode;
use tark_cli::tools::RiskLevel;
use tark_cli::ui_backend::ToolExecutionService;

#[tokio::test]
async fn test_list_tools_varies_by_mode() {
    let service = ToolExecutionService::new(AgentMode::Build);

    let ask_tools = service.list_tools(AgentMode::Ask);
    let plan_tools = service.list_tools(AgentMode::Plan);
    let build_tools = service.list_tools(AgentMode::Build);

    // Build mode should have more tools than Plan
    assert!(build_tools.len() >= plan_tools.len());

    // Plan mode should have more or equal tools than Ask
    assert!(plan_tools.len() >= ask_tools.len());

    // All modes should have read-only tools
    assert!(
        !ask_tools.is_empty(),
        "Ask mode should have read-only tools"
    );
}

#[tokio::test]
async fn test_tool_risk_level() {
    let service = ToolExecutionService::new(AgentMode::Build);

    // read_file should be ReadOnly
    if let Some(risk) = service.tool_risk_level("read_file") {
        assert_eq!(risk, RiskLevel::ReadOnly);
    }

    // write_file should be Write or higher
    if let Some(risk) = service.tool_risk_level("write_file") {
        assert!(matches!(risk, RiskLevel::Write | RiskLevel::Risky));
    }

    // shell should be Risky or Dangerous
    if let Some(risk) = service.tool_risk_level("shell") {
        assert!(matches!(risk, RiskLevel::Risky | RiskLevel::Dangerous));
    }
}

#[tokio::test]
async fn test_tool_availability_by_mode() {
    let service = ToolExecutionService::new(AgentMode::Build);

    // read_file available in all modes
    assert!(service.is_available("read_file", AgentMode::Ask));
    assert!(service.is_available("read_file", AgentMode::Plan));
    assert!(service.is_available("read_file", AgentMode::Build));

    // write_file only in Build mode
    assert!(!service.is_available("write_file", AgentMode::Ask));
    assert!(!service.is_available("write_file", AgentMode::Plan));
    assert!(service.is_available("write_file", AgentMode::Build));
}

#[tokio::test]
async fn test_tool_description() {
    let service = ToolExecutionService::new(AgentMode::Build);

    if let Some(desc) = service.tool_description("read_file") {
        assert!(!desc.is_empty());
        assert!(desc.to_lowercase().contains("read"));
    }
}

// NOTE: The following tests have been disabled as they test the old ApprovalGate API
// which has been replaced with PolicyEngine-based approval system.
// Trust level and approval management is now handled through ChatAgent and PolicyEngine.

// #[tokio::test]
// async fn test_trust_level_default() { ... }

// #[tokio::test]
// async fn test_approval_without_gate() { ... }

// #[tokio::test]
// async fn test_set_mode() { ... }

// #[tokio::test]
// async fn test_clear_session() { ... }

// #[tokio::test]
// async fn test_get_persistent_approvals_empty_without_gate() { ... }
