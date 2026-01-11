//! Google OAuth device flow implementation for Gemini
//!
//! Uses Google's OAuth 2.0 device flow for authentication.
//! Reference: https://developers.google.com/identity/protocols/oauth2/limited-input-device

use super::{AuthStatus, DeviceCodeResponse, DeviceFlowAuth, OAuthToken, PollResult, TokenStore};
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Google OAuth device flow endpoints
const DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Google OAuth client ID from Gemini CLI (public, safe to embed for installed apps)
/// Reference: https://developers.google.com/identity/protocols/oauth2#installed
const CLIENT_ID: &str = "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";

/// OAuth scopes for Gemini API access
/// - cloud-platform: Required for Gemini API access
/// - userinfo.email/profile: For user identification
const SCOPES: &str =
    "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email";

/// Google OAuth implementation for Gemini
pub struct GeminiOAuth {
    client: reqwest::Client,
    token_store: TokenStore,
}

impl GeminiOAuth {
    /// Create a new GeminiOAuth instance
    pub fn new() -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            token_store: TokenStore::new("gemini")?,
        })
    }

    /// Get API key from environment if set
    fn get_api_key() -> Option<String> {
        std::env::var("GEMINI_API_KEY")
            .or_else(|_| std::env::var("GOOGLE_API_KEY"))
            .ok()
    }

    /// Check for Application Default Credentials
    fn has_adc() -> bool {
        std::env::var("GOOGLE_APPLICATION_CREDENTIALS")
            .map(|p| std::path::Path::new(&p).exists())
            .unwrap_or(false)
    }
}

#[async_trait]
impl DeviceFlowAuth for GeminiOAuth {
    fn provider_name(&self) -> &str {
        "gemini"
    }

    fn display_name(&self) -> &str {
        "Google Gemini"
    }

    async fn start_device_flow(&self) -> Result<DeviceCodeResponse> {
        let response = self
            .client
            .post(DEVICE_CODE_URL)
            .form(&[("client_id", CLIENT_ID), ("scope", SCOPES)])
            .send()
            .await
            .context("Failed to request device code from Google")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Google device code request failed ({}): {}", status, text);
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse device code response")?;

        Ok(DeviceCodeResponse {
            device_code: data["device_code"]
                .as_str()
                .context("Missing device_code")?
                .to_string(),
            user_code: data["user_code"]
                .as_str()
                .context("Missing user_code")?
                .to_string(),
            verification_url: data["verification_uri"]
                .as_str()
                .or_else(|| data["verification_url"].as_str())
                .context("Missing verification_uri")?
                .to_string(),
            expires_in: data["expires_in"].as_u64().unwrap_or(300),
            interval: data["interval"].as_u64().unwrap_or(5),
        })
    }

    async fn poll(&self, device_code: &str) -> Result<PollResult> {
        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", CLIENT_ID),
                ("device_code", device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .context("Failed to poll for token")?;

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse token response")?;

        // Check for error
        if let Some(error) = data["error"].as_str() {
            return match error {
                "authorization_pending" => Ok(PollResult::Pending),
                "slow_down" => Ok(PollResult::Pending),
                "expired_token" => Ok(PollResult::Expired),
                "access_denied" => Ok(PollResult::Error("User denied authorization".to_string())),
                _ => Ok(PollResult::Error(format!(
                    "{}: {}",
                    error,
                    data["error_description"].as_str().unwrap_or("")
                ))),
            };
        }

        // Success - parse token
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let token = OAuthToken {
            access_token: data["access_token"]
                .as_str()
                .context("Missing access_token")?
                .to_string(),
            refresh_token: data["refresh_token"].as_str().map(|s| s.to_string()),
            expires_at: now + data["expires_in"].as_u64().unwrap_or(3600),
            scopes: SCOPES.split(' ').map(|s| s.to_string()).collect(),
        };

        // Save token
        self.token_store.save(&token)?;

        Ok(PollResult::Success(token))
    }

    async fn refresh(&self, refresh_token: &str) -> Result<OAuthToken> {
        let response = self
            .client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", CLIENT_ID),
                ("refresh_token", refresh_token),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("Failed to refresh token")?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            anyhow::bail!("Token refresh failed ({}): {}", status, text);
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse refresh response")?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs();

        let token = OAuthToken {
            access_token: data["access_token"]
                .as_str()
                .context("Missing access_token in refresh response")?
                .to_string(),
            // Google may return a new refresh token, or we keep the old one
            refresh_token: data["refresh_token"]
                .as_str()
                .map(|s| s.to_string())
                .or_else(|| Some(refresh_token.to_string())),
            expires_at: now + data["expires_in"].as_u64().unwrap_or(3600),
            scopes: SCOPES.split(' ').map(|s| s.to_string()).collect(),
        };

        // Save refreshed token
        self.token_store.save(&token)?;

        Ok(token)
    }

    async fn get_valid_token(&self) -> Result<String> {
        // Priority 1: API key from environment
        if let Some(key) = Self::get_api_key() {
            return Ok(key);
        }

        // Priority 2: Cached OAuth token
        if let Ok(token) = self.token_store.load() {
            if !token.is_expired() {
                return Ok(token.access_token);
            }

            // Try to refresh
            if let Some(refresh_token) = &token.refresh_token {
                match self.refresh(refresh_token).await {
                    Ok(new_token) => return Ok(new_token.access_token),
                    Err(e) => {
                        tracing::warn!("Failed to refresh token: {}", e);
                        // Fall through to require re-authentication
                    }
                }
            }
        }

        // No valid credentials
        anyhow::bail!(
            "Gemini authentication required. Run `tark auth gemini` or set GEMINI_API_KEY"
        )
    }

    async fn status(&self) -> AuthStatus {
        // Check API key first
        if Self::get_api_key().is_some() {
            return AuthStatus::ApiKey;
        }

        // Check ADC
        if Self::has_adc() {
            return AuthStatus::ADC;
        }

        // Check OAuth token
        if let Ok(token) = self.token_store.load() {
            if !token.is_expired() {
                return AuthStatus::OAuth;
            }
            // Try refresh silently
            if let Some(refresh) = &token.refresh_token {
                if self.refresh(refresh).await.is_ok() {
                    return AuthStatus::OAuth;
                }
            }
        }

        AuthStatus::NotAuthenticated
    }

    async fn logout(&self) -> Result<()> {
        self.token_store.delete()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_detection() {
        // This test just verifies the function exists and returns None when no env var
        let key = GeminiOAuth::get_api_key();
        // We can't assert much since it depends on environment
        let _ = key;
    }

    #[test]
    fn test_gemini_oauth_creation() {
        // Just verify GeminiOAuth can be created
        let result = GeminiOAuth::new();
        assert!(result.is_ok());
    }
}
