//! Integration tests for auth module types
//!
//! Note: GeminiOAuth is now handled by external plugins.
//! This file tests the shared auth types that plugins can use.

use tark_cli::llm::auth::{AuthStatus, OAuthToken, TokenStore};

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
fn test_token_store_creation() {
    // Verify TokenStore can be created
    let result = TokenStore::new("test_provider");
    assert!(result.is_ok());
    let store = result.unwrap();
    assert!(store.path().to_string_lossy().contains("test_provider"));
}

#[test]
fn test_auth_status_equality() {
    assert_eq!(AuthStatus::NotAuthenticated, AuthStatus::NotAuthenticated);
    assert_eq!(AuthStatus::ApiKey, AuthStatus::ApiKey);
    assert_eq!(AuthStatus::OAuth, AuthStatus::OAuth);
    assert_eq!(AuthStatus::ADC, AuthStatus::ADC);

    assert_ne!(AuthStatus::NotAuthenticated, AuthStatus::ApiKey);
    assert_ne!(AuthStatus::OAuth, AuthStatus::ADC);
}
