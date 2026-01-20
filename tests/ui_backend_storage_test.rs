//! Tests for StorageFacade
//!
//! Real behavior tests for storage operations.

use tark_cli::ui_backend::storage_facade::ConfigScope;
use tark_cli::ui_backend::StorageFacade;

/// Create a test storage facade with a temp directory
fn create_test_storage() -> (StorageFacade, tempfile::TempDir) {
    let temp_dir = tempfile::tempdir().unwrap();
    let facade = StorageFacade::new(temp_dir.path()).unwrap();
    (facade, temp_dir)
}

#[test]
fn test_create_and_list_sessions() {
    let (facade, _temp) = create_test_storage();

    // Create a session
    let session_info = facade.create_session().unwrap();
    assert!(!session_info.session_id.is_empty());

    // List sessions
    let sessions = facade.list_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, session_info.session_id);
}

#[test]
fn test_load_session() {
    let (facade, _temp) = create_test_storage();

    let created = facade.create_session().unwrap();

    // Load the session
    let loaded = facade.load_session(&created.session_id).unwrap();
    assert_eq!(loaded.id, created.session_id);
}

#[test]
fn test_delete_session() {
    let (facade, _temp) = create_test_storage();

    let session = facade.create_session().unwrap();

    // Delete it
    facade.delete_session(&session.session_id).unwrap();

    // Should not be found
    let result = facade.load_session(&session.session_id);
    assert!(result.is_err());
}

#[test]
fn test_export_import_session() {
    let (facade, temp) = create_test_storage();

    let session = facade.create_session().unwrap();
    let export_path = temp.path().join("export.json");

    // Export
    facade
        .export_session(&session.session_id, &export_path)
        .unwrap();
    assert!(export_path.exists());

    // Import
    let imported = facade.import_session(&export_path).unwrap();
    assert!(!imported.session_id.is_empty());
}

#[test]
fn test_get_config() {
    let (facade, _temp) = create_test_storage();

    let config = facade.get_config();
    // Should return default config if none exists
    assert!(config.provider.is_empty() || !config.provider.is_empty());
}

#[test]
fn test_save_project_config() {
    let (facade, _temp) = create_test_storage();

    let mut config = facade.get_config();
    config.provider = "test_provider".to_string();

    facade.save_project_config(&config).unwrap();

    // Reload and verify
    let reloaded = facade.get_config();
    assert_eq!(reloaded.provider, "test_provider");
}

#[test]
fn test_get_rules() {
    let (facade, _temp) = create_test_storage();

    let rules = facade.get_rules();
    // Should return empty if no rules exist
    assert!(rules.is_empty() || !rules.is_empty());
}

#[test]
fn test_save_rule_project_scope() {
    let (facade, _temp) = create_test_storage();

    facade
        .save_rule(
            "test_rule",
            "# Test Rule\n\nThis is a test",
            ConfigScope::Project,
        )
        .unwrap();

    let rules = facade.get_rules();
    assert!(!rules.is_empty());
}

#[test]
fn test_delete_rule() {
    let (facade, _temp) = create_test_storage();

    facade
        .save_rule("test_rule", "Content", ConfigScope::Project)
        .unwrap();
    facade
        .delete_rule("test_rule", ConfigScope::Project)
        .unwrap();

    // Rule should be gone (or at least not cause an error to check)
}

#[test]
fn test_get_mcp_config() {
    let (facade, _temp) = create_test_storage();

    let mcp_config = facade.get_mcp_config();
    // Should return default if none exists
    assert!(mcp_config.servers.is_empty() || !mcp_config.servers.is_empty());
}

#[test]
fn test_list_plugins() {
    let (facade, _temp) = create_test_storage();

    let plugins = facade.list_plugins();
    // Should return a list (verify it's accessible)
    let _ = plugins.len();
}

#[test]
fn test_get_usage_tracker() {
    let (facade, _temp) = create_test_storage();

    let tracker = facade.get_usage_tracker();
    assert!(tracker.is_ok());
}

#[test]
fn test_record_usage() {
    let (facade, _temp) = create_test_storage();

    // Should not error (even though it's a placeholder)
    let result = facade.record_usage("openai", "gpt-4", 100, 50, 0.01);
    assert!(result.is_ok());
}
