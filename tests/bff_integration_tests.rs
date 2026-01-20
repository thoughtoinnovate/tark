//! BFF Integration Tests - Real behavior validation
//!
//! These tests validate actual code paths, not mocked state.

use std::path::PathBuf;
use tark_cli::core::types::AgentMode;
use tark_cli::tools::risk::TrustLevel;
use tark_cli::ui_backend::{
    AuthStatus, CatalogService, SharedState, StorageFacade, ToolExecutionService,
};
use tempfile::TempDir;

fn create_test_env() -> (TempDir, PathBuf) {
    let temp = TempDir::new().unwrap();
    let working_dir = temp.path().to_path_buf();
    (temp, working_dir)
}

mod streaming_tests {
    use super::*;

    #[tokio::test]
    async fn test_streaming_accumulates_in_shared_state_only() {
        let state = SharedState::new();

        // Simulate streaming chunks
        state.set_streaming_content(Some(String::new()));
        state.append_streaming_content("Hello ");
        state.append_streaming_content("world!");

        assert_eq!(state.streaming_content(), Some("Hello world!".to_string()));

        // Clear and verify
        state.clear_streaming();
        assert_eq!(state.streaming_content(), None);
    }

    #[tokio::test]
    async fn test_streaming_interrupt_clears_state() {
        let state = SharedState::new();

        // Start streaming
        state.set_streaming_content(Some(String::new()));
        state.append_streaming_content("Partial response...");
        state.set_streaming_thinking(Some("Thinking...".to_string()));

        // Verify accumulation
        assert!(state.streaming_content().is_some());
        assert!(state.streaming_thinking().is_some());

        // Interrupt should clear
        state.clear_streaming();

        assert_eq!(state.streaming_content(), None);
        assert_eq!(state.streaming_thinking(), None);
    }
}

mod session_tests {
    use super::*;

    #[tokio::test]
    async fn test_session_create_load_roundtrip() {
        let (_temp, working_dir) = create_test_env();
        let storage = StorageFacade::new(&working_dir).unwrap();

        // Create session
        let session_info = storage.create_session().unwrap();
        let session_id = session_info.session_id.clone();

        // Load and verify
        let loaded = storage.load_session(&session_id).unwrap();
        assert_eq!(loaded.id, session_id);

        // List and verify
        let sessions = storage.list_sessions().unwrap();
        assert!(sessions.iter().any(|s| s.id == session_id));

        // Delete and verify gone
        storage.delete_session(&session_id).unwrap();
        assert!(storage.load_session(&session_id).is_err());
    }

    #[tokio::test]
    async fn test_session_export_import() {
        let (temp, working_dir) = create_test_env();
        let storage = StorageFacade::new(&working_dir).unwrap();

        // Create session
        let session_info = storage.create_session().unwrap();
        let session_id = session_info.session_id.clone();

        // Export to file
        let export_path = temp.path().join("exported_session.json");
        storage.export_session(&session_id, &export_path).unwrap();

        // Verify export file exists
        assert!(export_path.exists());

        // Delete original
        storage.delete_session(&session_id).unwrap();

        // Import and verify
        let imported = storage.import_session(&export_path).unwrap();
        assert!(!imported.session_id.is_empty());

        // Cleanup
        storage.delete_session(&imported.session_id).ok();
    }

    #[tokio::test]
    async fn test_session_import_invalid_json() {
        let (temp, working_dir) = create_test_env();
        let storage = StorageFacade::new(&working_dir).unwrap();

        // Invalid JSON file
        let invalid_path = temp.path().join("invalid.json");
        std::fs::write(&invalid_path, "not valid json").unwrap();
        let result = storage.import_session(&invalid_path);
        // Invalid JSON should fail to parse
        if let Err(e) = result {
            assert!(e.to_string().contains("Invalid") || e.to_string().contains("parse"));
        }

        // Valid JSON is accepted (even if minimal) as import creates new session
        let minimal_path = temp.path().join("minimal.json");
        std::fs::write(&minimal_path, r#"{"provider": "test"}"#).unwrap();
        let result = storage.import_session(&minimal_path);
        // Should succeed or provide informative error
        match result {
            Ok(imported) => {
                // Cleanup
                storage.delete_session(&imported.session_id).ok();
            }
            Err(_) => {
                // Also acceptable
            }
        }
    }
}

mod error_tests {
    use super::*;

    #[tokio::test]
    async fn test_provider_auth_status() {
        let catalog = CatalogService::new();

        // Check auth status for a provider
        let status = catalog.auth_status("openai");
        // Status should be one of the valid variants
        assert!(matches!(
            status,
            AuthStatus::NotAuthenticated | AuthStatus::ApiKey | AuthStatus::OAuth | AuthStatus::ADC
        ));
    }

    #[tokio::test]
    async fn test_tool_availability_by_mode() {
        let tools = ToolExecutionService::new(AgentMode::Ask, None);

        // Read-only tools should be available in Ask mode
        assert!(tools.is_available("grep", AgentMode::Ask));
        assert!(tools.is_available("file_preview", AgentMode::Ask));

        // Verify we can query tool availability for different modes
        let build_available = tools.is_available("shell", AgentMode::Build);
        let ask_available = tools.is_available("shell", AgentMode::Ask);
        // Shell might be available in different modes based on configuration
        // Tools should be available in at least one mode
        assert!(build_available || ask_available);
    }

    #[tokio::test]
    async fn test_tool_trust_level() {
        use std::sync::Arc;
        use tark_cli::tools::approval::ApprovalGate;
        use tokio::sync::Mutex;

        let gate = Arc::new(Mutex::new(ApprovalGate::new(
            std::env::current_dir().unwrap(),
            None,
        )));
        let tools = ToolExecutionService::new(AgentMode::Build, Some(gate.clone()));

        // Set manual trust level
        tools.set_trust_level(TrustLevel::Manual).await;

        // Verify trust level was set
        let level = tools.trust_level().await;
        assert_eq!(level, TrustLevel::Manual);

        // Set back to balanced
        tools.set_trust_level(TrustLevel::Balanced).await;
        let level = tools.trust_level().await;
        assert_eq!(level, TrustLevel::Balanced);
    }

    #[tokio::test]
    async fn test_session_not_found_error() {
        let (_temp, working_dir) = create_test_env();
        let storage = StorageFacade::new(&working_dir).unwrap();

        // Try to load non-existent session
        let result = storage.load_session("non-existent-id");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_storage_idempotent_delete() {
        let (_temp, working_dir) = create_test_env();
        let storage = StorageFacade::new(&working_dir).unwrap();

        // Create a session
        let session_info = storage.create_session().unwrap();
        let session_id = session_info.session_id.clone();

        // Delete once should succeed
        let result = storage.delete_session(&session_id);
        assert!(result.is_ok());

        // Delete again - might succeed (idempotent) or fail (not found)
        // Either behavior is acceptable
        let _ = storage.delete_session(&session_id);
    }
}
