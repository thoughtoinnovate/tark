//! Generic OAuth2 PKCE flow handler
//!
//! Provides a reusable OAuth2 implementation that works with any OAuth provider
//! by accepting configuration from plugin manifests.

use crate::plugins::OAuthConfig;
use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// OAuth2 token response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Generic OAuth2 PKCE handler
pub struct OAuthHandler {
    config: OAuthConfig,
    http_client: reqwest::Client,
}

impl OAuthHandler {
    /// Create a new OAuth handler from plugin configuration
    pub fn new(config: OAuthConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Execute complete PKCE flow: authorization → callback → token exchange
    pub async fn execute_pkce_flow(&self) -> Result<TokenResponse> {
        // Generate PKCE code_verifier and code_challenge
        let code_verifier = self.generate_code_verifier();
        let code_challenge = self.generate_code_challenge(&code_verifier);

        // Generate state for CSRF protection if enabled
        let state = if self.config.use_state {
            Some(self.generate_state())
        } else {
            None
        };

        // Build authorization URL
        let auth_url = self.build_authorization_url(&code_challenge, state.as_deref())?;

        // Start local HTTP server for callback
        let auth_code = Arc::new(Mutex::new(None::<String>));
        let server_handle = self.start_callback_server(auth_code.clone()).await?;

        // Open browser for user authentication
        self.open_browser(&auth_url)?;

        // Wait for authorization code
        let authorization_code = self.wait_for_auth_code(auth_code, &server_handle).await?;

        // Exchange code for tokens
        let tokens = self
            .exchange_code_for_tokens(&authorization_code, &code_verifier)
            .await?;

        Ok(tokens)
    }

    /// Generate PKCE code verifier (random 128-character string)
    fn generate_code_verifier(&self) -> String {
        rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(128)
            .map(char::from)
            .collect()
    }

    /// Generate PKCE code challenge from verifier (SHA256 + base64url)
    fn generate_code_challenge(&self, verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        URL_SAFE_NO_PAD.encode(hasher.finalize())
    }

    /// Generate a random state parameter for CSRF protection
    fn generate_state(&self) -> String {
        let random_bytes: Vec<u8> = (0..32).map(|_| rand::random::<u8>()).collect();
        URL_SAFE_NO_PAD.encode(&random_bytes)
    }

    /// Build OAuth authorization URL with PKCE parameters
    fn build_authorization_url(&self, code_challenge: &str, state: Option<&str>) -> Result<String> {
        let scope_string = self.config.scopes.join(" ");

        // Build base parameters
        let mut params: Vec<(&str, &str)> = vec![
            ("response_type", "code"),
            ("client_id", self.config.client_id.as_str()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("scope", &scope_string),
            ("code_challenge", code_challenge),
            ("code_challenge_method", "S256"),
        ];

        // Add state if provided
        let state_owned: String;
        if let Some(s) = state {
            state_owned = s.to_string();
            params.push(("state", &state_owned));
        }

        // Build query string from base params
        let mut query_parts: Vec<String> = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, urlencoding::encode(v)))
            .collect();

        // Add extra parameters from config (provider-specific)
        for (key, value) in &self.config.extra_params {
            query_parts.push(format!("{}={}", key, urlencoding::encode(value)));
        }

        let query_string = query_parts.join("&");
        Ok(format!("{}?{}", self.config.auth_url, query_string))
    }

    /// Start local HTTP server to receive OAuth callback
    async fn start_callback_server(
        &self,
        auth_code: Arc<Mutex<Option<String>>>,
    ) -> Result<tokio::task::JoinHandle<()>> {
        use axum::{extract::Query, response::Html, routing::get, Router};

        #[derive(serde::Deserialize)]
        struct CallbackQuery {
            code: Option<String>,
            error: Option<String>,
        }

        let app = Router::new().route(
            "/callback",
            get(move |Query(query): Query<CallbackQuery>| async move {
                let mut auth_code_lock = auth_code.lock().await;
                let response = if let Some(code) = query.code {
                    *auth_code_lock = Some(code);
                    "<h1>Authentication successful!</h1><p>You can close this window and return to the terminal.</p>".to_string()
                } else {
                    let error = query.error.unwrap_or_else(|| "unknown".to_string());
                    format!(
                        "<h1>Authentication failed</h1><p>Error: {}</p><p>Please close this window and try again.</p>",
                        error
                    )
                };
                Html(response)
            }),
        );

        // Extract port from redirect_uri
        let port = self.extract_port_from_redirect_uri()?;

        let addr = format!("127.0.0.1:{}", port);
        let server_handle = tokio::spawn(async move {
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    tracing::debug!("OAuth callback server listening on {}", addr);
                    if let Err(e) = axum::serve(listener, app).await {
                        tracing::error!("OAuth callback server error: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to bind OAuth callback server to {}: {}", addr, e);
                }
            }
        });

        // Give server time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        Ok(server_handle)
    }

    /// Extract port number from redirect URI
    fn extract_port_from_redirect_uri(&self) -> Result<u16> {
        let uri = &self.config.redirect_uri;
        if let Some(port_str) = uri.split(':').nth(2) {
            if let Some(port_part) = port_str.split('/').next() {
                return port_part.parse().context("Invalid port in redirect_uri");
            }
        }
        anyhow::bail!("Could not extract port from redirect_uri: {}", uri);
    }

    /// Open browser to authorization URL
    fn open_browser(&self, url: &str) -> Result<()> {
        println!("Opening browser for authentication...");
        println!();
        println!("If the browser doesn't open automatically, visit:");
        println!("{}", url);
        println!();

        if let Err(e) = open::that(url) {
            tracing::warn!("Failed to open browser: {}", e);
            println!("⚠️  Could not open browser automatically. Please visit the URL above.");
        }

        Ok(())
    }

    /// Wait for authorization code from callback
    async fn wait_for_auth_code(
        &self,
        auth_code: Arc<Mutex<Option<String>>>,
        server_handle: &tokio::task::JoinHandle<()>,
    ) -> Result<String> {
        let mut attempts = 0;
        let max_attempts = 120; // 2 minutes timeout

        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            let code_guard = auth_code.lock().await;
            if let Some(code) = code_guard.as_ref() {
                let result = code.clone();
                drop(code_guard);
                server_handle.abort();
                return Ok(result);
            }
            drop(code_guard);

            attempts += 1;
            if attempts >= max_attempts {
                server_handle.abort();
                anyhow::bail!(
                    "Authentication timeout - no response after {} seconds. Please try again.",
                    max_attempts
                );
            }
        }
    }

    /// Exchange authorization code for access tokens
    async fn exchange_code_for_tokens(
        &self,
        code: &str,
        code_verifier: &str,
    ) -> Result<TokenResponse> {
        println!("Exchanging authorization code for tokens...");

        let mut params = vec![
            ("client_id", self.config.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("code_verifier", code_verifier),
        ];

        // Add client_secret if provided (though not recommended for CLI tools)
        let client_secret_string;
        if let Some(secret) = &self.config.client_secret {
            client_secret_string = secret.clone();
            params.push(("client_secret", &client_secret_string));
        }

        let response = self
            .http_client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .context("Failed to send token request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token request failed ({}): {}", status, body);
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .context("Failed to parse token response")?;

        Ok(token_response)
    }

    /// Refresh an expired token using refresh_token
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<TokenResponse> {
        tracing::debug!("Refreshing OAuth token");

        let params = [
            ("client_id", self.config.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ];

        let response = self
            .http_client
            .post(&self.config.token_url)
            .form(&params)
            .send()
            .await
            .context("Failed to send refresh request")?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("Token refresh failed ({}): {}", status, body);
        }

        let token_response: TokenResponse = response
            .json()
            .await
            .context("Failed to parse refresh response")?;

        Ok(token_response)
    }

    /// Save credentials to configured path with secure permissions
    pub fn save_credentials(&self, tokens: &TokenResponse) -> Result<PathBuf> {
        let creds_path = if let Some(path_str) = &self.config.credentials_path {
            self.expand_path(path_str)
        } else {
            // Default: ~/.config/tark/<provider>_oauth.json
            let config_dir = dirs::config_dir()
                .context("Could not determine config directory")?
                .join("tark");
            config_dir.join("oauth_credentials.json")
        };

        // Ensure parent directory exists
        if let Some(parent) = creds_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        // Add expires_at timestamp if expires_in provided
        let mut credentials = serde_json::to_value(tokens)?;
        if let Some(expires_in) = tokens.expires_in {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_secs();
            credentials["expires_at"] = serde_json::json!(now + expires_in);
        }

        // Write to file
        std::fs::write(&creds_path, serde_json::to_string_pretty(&credentials)?)
            .with_context(|| format!("Failed to write credentials to {}", creds_path.display()))?;

        // Set secure permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&creds_path)?.permissions();
            perms.set_mode(0o600); // Owner read/write only
            std::fs::set_permissions(&creds_path, perms)?;
        }

        tracing::info!("Saved OAuth credentials to {}", creds_path.display());
        Ok(creds_path)
    }

    /// Expand ~ in path to home directory
    fn expand_path(&self, path: &str) -> PathBuf {
        if let Some(stripped) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(stripped);
            }
        } else if path == "~" {
            if let Some(home) = dirs::home_dir() {
                return home;
            }
        }
        PathBuf::from(path)
    }
}
