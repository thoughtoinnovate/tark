//! Authentication module for OAuth device flow providers
//!
//! Provides reusable traits and types for OAuth authentication
//! with device flow (Copilot, Gemini, future providers).

#![allow(clippy::upper_case_acronyms)]
#![allow(unused_imports)]

mod gemini_oauth;
mod token_store;

pub use gemini_oauth::GeminiOAuth;
pub use token_store::TokenStore;

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Device code response from OAuth provider
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    /// The device verification code
    pub device_code: String,
    /// The code the user must enter
    pub user_code: String,
    /// URL where user must authorize
    pub verification_url: String,
    /// How long until the codes expire (seconds)
    pub expires_in: u64,
    /// Minimum seconds between polling attempts
    pub interval: u64,
}

/// OAuth token with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    /// The access token for API calls
    pub access_token: String,
    /// Optional refresh token for getting new access tokens
    pub refresh_token: Option<String>,
    /// Unix timestamp when token expires
    pub expires_at: u64,
    /// Scopes granted
    pub scopes: Vec<String>,
}

impl OAuthToken {
    /// Check if token is expired or will expire within 5 minutes
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Consider expired if within 5 minutes of expiry
        self.expires_at <= now + 300
    }
}

/// Result of polling for token
#[derive(Debug, Clone)]
pub enum PollResult {
    /// Still waiting for user to authorize
    Pending,
    /// Successfully obtained token
    Success(OAuthToken),
    /// Device code expired
    Expired,
    /// Provider returned an error
    Error(String),
}

/// Authentication status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// Not authenticated, no stored credentials
    NotAuthenticated,
    /// Using API key from environment
    ApiKey,
    /// Using OAuth token
    OAuth,
    /// Using Application Default Credentials
    ADC,
}

/// Trait for OAuth device flow authentication
///
/// Implement this trait to add device flow auth for a provider.
/// See `GeminiOAuth` for example implementation.
#[async_trait]
pub trait DeviceFlowAuth: Send + Sync {
    /// Provider name (e.g., "gemini", "copilot")
    fn provider_name(&self) -> &str;

    /// Display name for UI (e.g., "Google Gemini", "GitHub Copilot")
    fn display_name(&self) -> &str;

    /// Start device flow, returns codes for user
    async fn start_device_flow(&self) -> Result<DeviceCodeResponse>;

    /// Poll for token completion
    async fn poll(&self, device_code: &str) -> Result<PollResult>;

    /// Refresh an expired token using refresh token
    async fn refresh(&self, refresh_token: &str) -> Result<OAuthToken>;

    /// Get valid access token (from cache, refresh if needed, or start flow)
    async fn get_valid_token(&self) -> Result<String>;

    /// Check current auth status
    async fn status(&self) -> AuthStatus;

    /// Clear stored credentials (logout)
    async fn logout(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
