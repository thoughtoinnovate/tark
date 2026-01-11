//! Integration tests for Gemini OAuth

use tark_cli::llm::auth::{AuthStatus, DeviceFlowAuth, GeminiOAuth, OAuthToken, TokenStore};

#[test]
fn test_oauth_token_expiry() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Token expires in 10 minutes - not expired
    let token = OAuthToken {
        access_token: "test".to_string(),
        refresh_token: None,
        expires_at: now + 600,
        scopes: vec![],
    };
    assert!(!token.is_expired());

    // Token expires in 2 minutes - considered expired (within 5min buffer)
    let token = OAuthToken {
        access_token: "test".to_string(),
        refresh_token: None,
        expires_at: now + 120,
        scopes: vec![],
    };
    assert!(token.is_expired());

    // Token already expired
    let token = OAuthToken {
        access_token: "test".to_string(),
        refresh_token: None,
        expires_at: now - 100,
        scopes: vec![],
    };
    assert!(token.is_expired());
}

#[test]
fn test_gemini_oauth_creation() {
    // Just verify GeminiOAuth can be created
    let result = GeminiOAuth::new();
    // Should succeed even without credentials
    assert!(result.is_ok());
}

#[test]
fn test_token_store_creation() {
    // Verify TokenStore can be created
    let result = TokenStore::new("test_provider");
    assert!(result.is_ok());
    let store = result.unwrap();
    assert!(store.path().to_string_lossy().contains("test_provider"));
}

#[tokio::test]
async fn test_gemini_oauth_status_no_credentials() {
    // Clear any existing credentials for test isolation
    std::env::remove_var("GEMINI_API_KEY");
    std::env::remove_var("GOOGLE_API_KEY");
    std::env::remove_var("GOOGLE_APPLICATION_CREDENTIALS");

    // Use temp directory to avoid affecting real token storage
    let temp_dir = tempfile::tempdir().unwrap();
    std::env::set_var(
        "XDG_DATA_HOME",
        temp_dir.path().join("data").to_str().unwrap(),
    );

    let auth = GeminiOAuth::new().unwrap();
    let status = auth.status().await;

    assert_eq!(status, AuthStatus::NotAuthenticated);
}

#[tokio::test]
async fn test_gemini_oauth_status_with_api_key() {
    // Use a unique env var name to avoid conflicts with other tests
    // Save and restore original value
    let original = std::env::var("GEMINI_API_KEY").ok();

    // Set API key
    std::env::set_var("GEMINI_API_KEY", "test_api_key_for_status_test");

    let auth = GeminiOAuth::new().unwrap();
    let status = auth.status().await;

    // Restore original value
    match original {
        Some(val) => std::env::set_var("GEMINI_API_KEY", val),
        None => std::env::remove_var("GEMINI_API_KEY"),
    }

    assert_eq!(status, AuthStatus::ApiKey);
}

#[test]
fn test_gemini_oauth_provider_name() {
    let auth = GeminiOAuth::new().unwrap();
    assert_eq!(auth.provider_name(), "gemini");
    assert_eq!(auth.display_name(), "Google Gemini");
}
