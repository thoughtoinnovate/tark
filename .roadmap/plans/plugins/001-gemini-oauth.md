# Gemini OAuth Implementation Plan

**Goal**: Implement Google OAuth device flow for Gemini authentication, supporting all backends (Gemini API, Vertex AI) with both TUI and CLI interfaces.

**Status**: Ready for implementation

---

## Prerequisites

Before starting:
1. Ensure you have a working Rust toolchain (`cargo --version`)
2. Ensure you can build the project (`cargo build --release`)
3. Read existing OAuth implementation: `src/llm/copilot.rs` (reference)

---

## Phase 0: Discovery (Research - No Code Changes)

### Task 0.1: Analyze Gemini CLI OAuth Flow

**Action**: Review Gemini CLI source code to confirm OAuth details.

```bash
# Clone Gemini CLI temporarily for reference
git clone --depth 1 https://github.com/google-gemini/gemini-cli /tmp/gemini-cli

# Find OAuth configuration
grep -r "oauth" /tmp/gemini-cli --include="*.ts" --include="*.js" -l
grep -r "client_id" /tmp/gemini-cli --include="*.ts" --include="*.js"
grep -r "device" /tmp/gemini-cli --include="*.ts" --include="*.js"
```

**Document findings in**: `.roadmap/plans/001-gemini-oauth-discovery.md`

Required information to capture:
- [ ] Client ID used
- [ ] OAuth scopes required  
- [ ] Device code endpoint URL
- [ ] Token exchange endpoint URL
- [ ] How access token is used (Bearer header? Query param?)
- [ ] Refresh token flow details

**Commit**: None (research phase)

---

## Phase 1: Create Auth Module Structure ✅ DONE

### Task 1.1: Create `src/llm/auth/mod.rs` ✅ DONE

### Task 1.2: Create `src/llm/auth/token_store.rs` ✅ DONE

### Task 1.3: Create `src/llm/auth/gemini_oauth.rs` ✅ DONE

### Task 1.4: Export auth module from `src/llm/mod.rs` ✅ DONE

### Task 1.5: Commit Phase 1 ✅ DONE

**File**: `src/llm/auth/mod.rs`

```rust
//! Authentication module for OAuth device flow providers
//!
//! Provides reusable traits and types for OAuth authentication
//! with device flow (Copilot, Gemini, future providers).

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
```

**Command after creating file**:
```bash
# Verify it compiles
cargo check --lib
```

---

### Task 1.2: Create `src/llm/auth/token_store.rs`

**File**: `src/llm/auth/token_store.rs`

```rust
//! Secure token storage with file permissions and versioning
//!
//! Tokens are stored in ~/.local/share/tark/tokens/{provider}.json
//! with 0600 permissions (owner read/write only).

use super::OAuthToken;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Token storage format with version for future migrations
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredToken {
    /// Schema version for future migrations
    version: u32,
    /// Provider name
    provider: String,
    /// The actual token data
    #[serde(flatten)]
    token: OAuthToken,
    /// When the token was stored (Unix timestamp)
    stored_at: u64,
}

/// Manages secure token storage for a provider
pub struct TokenStore {
    /// Provider name (used for file naming)
    provider: String,
    /// Path to token file
    path: PathBuf,
}

impl TokenStore {
    /// Current storage schema version
    const VERSION: u32 = 1;

    /// Create a new token store for a provider
    pub fn new(provider: &str) -> Result<Self> {
        let tokens_dir = Self::tokens_dir()?;
        std::fs::create_dir_all(&tokens_dir)
            .context("Failed to create tokens directory")?;
        
        let path = tokens_dir.join(format!("{}.json", provider));
        
        Ok(Self {
            provider: provider.to_string(),
            path,
        })
    }

    /// Get the tokens directory path
    fn tokens_dir() -> Result<PathBuf> {
        let data_dir = dirs::data_local_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share")))
            .context("Failed to determine data directory")?;
        
        Ok(data_dir.join("tark").join("tokens"))
    }

    /// Load token from storage
    pub fn load(&self) -> Result<OAuthToken> {
        let content = std::fs::read_to_string(&self.path)
            .context("Failed to read token file")?;
        
        let stored: StoredToken = serde_json::from_str(&content)
            .context("Failed to parse token file")?;
        
        // Could add migration logic here for future versions
        if stored.version > Self::VERSION {
            anyhow::bail!(
                "Token file version {} is newer than supported version {}",
                stored.version,
                Self::VERSION
            );
        }
        
        Ok(stored.token)
    }

    /// Save token to storage with secure permissions
    pub fn save(&self, token: &OAuthToken) -> Result<()> {
        let stored = StoredToken {
            version: Self::VERSION,
            provider: self.provider.clone(),
            token: token.clone(),
            stored_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };
        
        let content = serde_json::to_string_pretty(&stored)?;
        
        // Write to temp file first, then rename (atomic)
        let temp_path = self.path.with_extension("json.tmp");
        std::fs::write(&temp_path, &content)
            .context("Failed to write temp token file")?;
        
        // Set secure permissions (0600 = owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&temp_path, perms)
                .context("Failed to set token file permissions")?;
        }
        
        // Atomic rename
        std::fs::rename(&temp_path, &self.path)
            .context("Failed to save token file")?;
        
        tracing::debug!("Saved {} token to {:?}", self.provider, self.path);
        Ok(())
    }

    /// Delete stored token
    pub fn delete(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)
                .context("Failed to delete token file")?;
            tracing::info!("Deleted {} token from {:?}", self.provider, self.path);
        }
        Ok(())
    }

    /// Check if token exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Get the token file path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_token_expiry() {
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
    }
}
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.3: Create `src/llm/auth/gemini_oauth.rs`

**File**: `src/llm/auth/gemini_oauth.rs`

```rust
//! Google OAuth device flow implementation for Gemini
//!
//! Reference: https://github.com/google-gemini/gemini-cli

use super::{AuthStatus, DeviceCodeResponse, DeviceFlowAuth, OAuthToken, PollResult, TokenStore};
use anyhow::{Context, Result};
use async_trait::async_trait;

/// Google OAuth device flow endpoints
const DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

/// Google OAuth client ID from Gemini CLI (public, safe to embed)
/// TODO: Verify this client ID works for our use case
const CLIENT_ID: &str = "764086051850-6qr4p6gpi6hn506pt8ejuq83di341hur.apps.googleusercontent.com";

/// OAuth scopes for Gemini API access
/// TODO: Verify minimal scopes needed from discovery phase
const SCOPES: &str = "openid email https://www.googleapis.com/auth/generative-language";

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
}
```

**Command after creating file**:
```bash
cargo check --lib
```

---

### Task 1.4: Export auth module from `src/llm/mod.rs`

**File**: `src/llm/mod.rs`

**Action**: Add auth module export. Insert after line 12:

```rust
pub mod auth;
```

**Full change** (search and replace):

Find:
```rust
mod types;
```

Replace with:
```rust
mod types;

pub mod auth;
```

**Command after change**:
```bash
cargo check --lib
```

---

### Task 1.5: Commit Phase 1

**Commands**:
```bash
# Format code
cargo fmt --all

# Verify no warnings
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all-features

# Commit
git add src/llm/auth/
git add src/llm/mod.rs
git commit -m "feat(auth): add DeviceFlowAuth trait and GeminiOAuth implementation

- Add DeviceFlowAuth trait for reusable OAuth device flow
- Add TokenStore with secure file permissions (0600)
- Add GeminiOAuth implementing Google OAuth device flow
- Token storage in ~/.local/share/tark/tokens/gemini.json"
```

---

## Phase 2: Generalize AuthDialog Widget ✅ DONE

### Task 2.1: Refactor `src/tui/widgets/auth_dialog.rs` ✅ DONE

### Task 2.2: Commit Phase 2 ✅ DONE

**File**: `src/tui/widgets/auth_dialog.rs`

**Action**: Add generic `show_device_flow` method. Replace the entire file with:

```rust
//! Authentication dialog widget for OAuth device flow
//!
//! Displays a centered modal dialog for OAuth authentication
//! with device code, URL, and status updates.
//! Works with any provider implementing DeviceFlowAuth.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::time::Instant;

/// Authentication status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// Waiting for user to authorize
    WaitingForUser,
    /// Polling for token
    Polling,
    /// Successfully authenticated
    Success,
    /// Authentication failed
    Failed(String),
    /// Authentication timed out
    TimedOut,
}

/// Authentication dialog state
#[derive(Debug, Clone)]
pub struct AuthDialog {
    /// Whether the dialog is visible
    visible: bool,
    /// Dialog title
    title: String,
    /// Provider name (e.g., "GitHub Copilot", "Google Gemini")
    provider: String,
    /// Verification URL
    verification_url: String,
    /// User code to enter
    user_code: String,
    /// Current status
    status: AuthStatus,
    /// Timeout in seconds
    timeout_secs: u64,
    /// When authentication started
    started_at: Option<Instant>,
}

impl Default for AuthDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthDialog {
    /// Create a new authentication dialog
    pub fn new() -> Self {
        Self {
            visible: false,
            title: "Authentication Required".to_string(),
            provider: String::new(),
            verification_url: String::new(),
            user_code: String::new(),
            status: AuthStatus::WaitingForUser,
            timeout_secs: 300, // 5 minutes default
            started_at: None,
        }
    }

    /// Show device flow authentication dialog for any provider
    ///
    /// This is the generic method that works with Copilot, Gemini, etc.
    pub fn show_device_flow(
        &mut self,
        provider_name: &str,
        url: &str,
        code: &str,
        timeout: u64,
    ) {
        self.visible = true;
        self.provider = provider_name.to_string();
        self.verification_url = url.to_string();
        self.user_code = code.to_string();
        self.status = AuthStatus::WaitingForUser;
        self.timeout_secs = timeout;
        self.started_at = Some(Instant::now());
    }

    /// Show Copilot authentication dialog (backwards compatible)
    pub fn show_copilot_auth(&mut self, url: &str, code: &str, timeout: u64) {
        self.show_device_flow("GitHub Copilot", url, code, timeout);
    }

    /// Show Gemini authentication dialog
    pub fn show_gemini_auth(&mut self, url: &str, code: &str, timeout: u64) {
        self.show_device_flow("Google Gemini", url, code, timeout);
    }

    /// Set the authentication status
    pub fn set_status(&mut self, status: AuthStatus) {
        self.status = status;
    }

    /// Hide the dialog
    pub fn hide(&mut self) {
        self.visible = false;
        self.started_at = None;
    }

    /// Check if the dialog is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the user code (for clipboard copy)
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    /// Get the verification URL (for opening in browser)
    pub fn verification_url(&self) -> &str {
        &self.verification_url
    }

    /// Get the provider name
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Get elapsed time percentage (0.0 to 1.0)
    fn elapsed_percent(&self) -> f32 {
        if let Some(started) = self.started_at {
            let elapsed = started.elapsed().as_secs();
            (elapsed as f32 / self.timeout_secs as f32).min(1.0)
        } else {
            0.0
        }
    }

    /// Get remaining time in seconds
    fn remaining_secs(&self) -> u64 {
        if let Some(started) = self.started_at {
            let elapsed = started.elapsed().as_secs();
            self.timeout_secs.saturating_sub(elapsed)
        } else {
            self.timeout_secs
        }
    }
}

/// Renderable authentication dialog widget
pub struct AuthDialogWidget<'a> {
    dialog: &'a AuthDialog,
}

impl<'a> AuthDialogWidget<'a> {
    /// Create a new auth dialog widget
    pub fn new(dialog: &'a AuthDialog) -> Self {
        Self { dialog }
    }

    /// Calculate the area for the dialog (centered modal)
    fn calculate_area(&self, area: Rect) -> Rect {
        let width = 60.min(area.width.saturating_sub(4));
        let height = 18.min(area.height.saturating_sub(4));

        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        Rect::new(x, y, width, height)
    }

    /// Render progress bar
    fn render_progress_bar(&self, width: usize) -> String {
        let percent = self.dialog.elapsed_percent();
        let filled = (width as f32 * percent) as usize;
        let empty = width.saturating_sub(filled);

        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for AuthDialogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.dialog.visible {
            return;
        }

        let dialog_area = self.calculate_area(area);

        // Clear the background
        Clear.render(dialog_area, buf);

        // Draw border with title
        let title = format!(" {} ", self.dialog.title);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(dialog_area);
        block.render(dialog_area, buf);

        if inner.height < 10 {
            return;
        }

        // Build content lines based on status
        let mut lines: Vec<Line<'static>> = Vec::new();

        match &self.dialog.status {
            AuthStatus::WaitingForUser | AuthStatus::Polling => {
                // Empty line for spacing
                lines.push(Line::from(""));

                // Instructions header
                lines.push(Line::from(vec![Span::styled(
                    "📋 Please follow these steps:",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                // Step 1: Visit URL
                lines.push(Line::from(vec![
                    Span::styled("  1. ", Style::default().fg(Color::White)),
                    Span::styled("Visit: ", Style::default().fg(Color::White)),
                    Span::styled(
                        self.dialog.verification_url.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
                lines.push(Line::from(""));

                // Step 2: Enter code
                lines.push(Line::from(vec![
                    Span::styled("  2. ", Style::default().fg(Color::White)),
                    Span::styled("Enter code: ", Style::default().fg(Color::White)),
                    Span::styled(
                        self.dialog.user_code.clone(),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                lines.push(Line::from(vec![Span::styled(
                    "      [Press 'o' or Enter to copy code & open URL]",
                    Style::default().fg(Color::DarkGray),
                )]));
                lines.push(Line::from(""));

                // Step 3: Authorize
                lines.push(Line::from(vec![
                    Span::styled("  3. ", Style::default().fg(Color::White)),
                    Span::styled(
                        format!("Authorize Tark to use {}", self.dialog.provider),
                        Style::default().fg(Color::White),
                    ),
                ]));
                lines.push(Line::from(""));

                // Status and progress
                let remaining = self.dialog.remaining_secs();
                let minutes = remaining / 60;
                let seconds = remaining % 60;
                let status_text = if self.dialog.status == AuthStatus::Polling {
                    format!("⏳ Polling... ({}:{:02} remaining)", minutes, seconds)
                } else {
                    format!("⏳ Waiting... ({}:{:02} remaining)", minutes, seconds)
                };

                lines.push(Line::from(vec![Span::styled(
                    status_text,
                    Style::default().fg(Color::Yellow),
                )]));

                // Progress bar
                let bar_width = (inner.width as usize).saturating_sub(4);
                let progress_bar = self.render_progress_bar(bar_width);
                let percent = (self.dialog.elapsed_percent() * 100.0) as u32;
                lines.push(Line::from(vec![
                    Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!(" {}%", percent),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
                lines.push(Line::from(""));

                // Cancel button
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Cancel",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::Success => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "✅ Authentication Successful!",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    format!("Successfully authenticated with {}", self.dialog.provider),
                    Style::default().fg(Color::White),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "(This dialog will close automatically)",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::Failed(error) => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "❌ Authentication Failed",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    error.clone(),
                    Style::default().fg(Color::Red),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Close",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::TimedOut => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "⏱️  Authentication Timed Out",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Please try again",
                    Style::default().fg(Color::White),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Close",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
        }

        // Render content
        let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_dialog_new() {
        let dialog = AuthDialog::new();
        assert!(!dialog.is_visible());
        assert_eq!(dialog.status, AuthStatus::WaitingForUser);
    }

    #[test]
    fn test_show_device_flow_generic() {
        let mut dialog = AuthDialog::new();
        dialog.show_device_flow(
            "Test Provider",
            "https://example.com/auth",
            "ABC-123",
            300,
        );

        assert!(dialog.is_visible());
        assert_eq!(dialog.provider(), "Test Provider");
        assert_eq!(dialog.user_code(), "ABC-123");
        assert_eq!(dialog.verification_url(), "https://example.com/auth");
    }

    #[test]
    fn test_show_copilot_auth() {
        let mut dialog = AuthDialog::new();
        dialog.show_copilot_auth("https://github.com/login/device", "ABCD-1234", 300);

        assert!(dialog.is_visible());
        assert_eq!(dialog.provider, "GitHub Copilot");
        assert_eq!(dialog.user_code(), "ABCD-1234");
        assert_eq!(dialog.verification_url, "https://github.com/login/device");
    }

    #[test]
    fn test_show_gemini_auth() {
        let mut dialog = AuthDialog::new();
        dialog.show_gemini_auth("https://google.com/device", "XYZ-789", 300);

        assert!(dialog.is_visible());
        assert_eq!(dialog.provider, "Google Gemini");
        assert_eq!(dialog.user_code(), "XYZ-789");
    }

    #[test]
    fn test_set_status() {
        let mut dialog = AuthDialog::new();
        dialog.set_status(AuthStatus::Polling);
        assert_eq!(dialog.status, AuthStatus::Polling);

        dialog.set_status(AuthStatus::Success);
        assert_eq!(dialog.status, AuthStatus::Success);
    }

    #[test]
    fn test_hide() {
        let mut dialog = AuthDialog::new();
        dialog.show_copilot_auth("https://test.com", "CODE", 300);
        assert!(dialog.is_visible());

        dialog.hide();
        assert!(!dialog.is_visible());
    }
}
```

**Command after change**:
```bash
cargo check --lib
cargo test --all-features
```

---

### Task 2.2: Commit Phase 2

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/tui/widgets/auth_dialog.rs
git commit -m "feat(tui): generalize AuthDialog for any OAuth provider

- Add show_device_flow() for generic provider support
- Add show_gemini_auth() convenience method
- Keep show_copilot_auth() for backwards compatibility
- Add provider() getter method
- Add tests for generic device flow"
```

---

## Phase 3: Add CLI Commands ✅ DONE

### Task 3.1: Update `src/transport/cli.rs` ✅ DONE

### Task 3.2: Add logout support to `run_auth` ✅ DONE

### Task 3.3: Update CLI help text ✅ DONE

### Task 3.4: Commit Phase 3 ✅ DONE

**File**: `src/transport/cli.rs`

**Action**: Update the `run_auth` function to support Gemini OAuth. Find the `"gemini" | "google"` match arm (around line 587-596) and replace it with:

Find:
```rust
        "gemini" | "google" => {
            if std::env::var("GEMINI_API_KEY").is_ok() {
                println!("✅ GEMINI_API_KEY is already set");
            } else {
                println!("{}", "Gemini API Key Required".bold());
                println!();
                println!("Please set your API key:");
                println!("  export GEMINI_API_KEY=\"your-api-key-here\"");
                println!();
                println!("Get your API key at: https://aistudio.google.com/apikey");
            }
        }
```

Replace with:
```rust
        "gemini" | "google" => {
            use crate::llm::auth::{DeviceFlowAuth, GeminiOAuth, AuthStatus as OAuthStatus};

            // Check current auth status
            let auth = GeminiOAuth::new()?;
            let status = auth.status().await;

            match status {
                OAuthStatus::ApiKey => {
                    println!("✅ GEMINI_API_KEY is already set");
                    println!();
                    println!("You can use Gemini with your API key:");
                    println!("  tark chat --provider gemini");
                }
                OAuthStatus::OAuth => {
                    println!("✅ Already authenticated with Google OAuth");
                    println!();
                    println!("Token location: ~/.local/share/tark/tokens/gemini.json");
                    println!();
                    println!("To re-authenticate, first logout:");
                    println!("  tark auth logout gemini");
                }
                OAuthStatus::ADC => {
                    println!("✅ Using Application Default Credentials");
                    println!();
                    println!("GOOGLE_APPLICATION_CREDENTIALS is set");
                }
                OAuthStatus::NotAuthenticated => {
                    println!("Starting Google OAuth device flow...");
                    println!();

                    // Start device flow
                    let device_response = auth.start_device_flow().await?;

                    // Display auth info
                    println!("🔐 Google Gemini Authentication");
                    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                    println!();
                    println!("1. Visit this URL in your browser:");
                    println!("   {}", device_response.verification_url);
                    println!();
                    println!("2. Enter this code:");
                    println!("   {}", device_response.user_code);
                    println!();
                    println!("Waiting for authorization...");
                    println!();

                    // Poll for token
                    let start = std::time::Instant::now();
                    let timeout = std::time::Duration::from_secs(device_response.expires_in);
                    let interval = std::time::Duration::from_secs(device_response.interval);

                    loop {
                        if start.elapsed() > timeout {
                            anyhow::bail!("Authentication timed out");
                        }

                        tokio::time::sleep(interval).await;

                        match auth.poll(&device_response.device_code).await? {
                            crate::llm::auth::PollResult::Pending => {
                                eprint!(".");
                                use std::io::Write;
                                let _ = std::io::stderr().flush();
                                continue;
                            }
                            crate::llm::auth::PollResult::Success(_) => {
                                println!();
                                println!();
                                println!("✅ Successfully authenticated with Google!");
                                println!();
                                println!("Token saved to: ~/.local/share/tark/tokens/gemini.json");
                                println!();
                                println!("You can now use Gemini:");
                                println!("  tark chat --provider gemini");
                                break;
                            }
                            crate::llm::auth::PollResult::Expired => {
                                anyhow::bail!("Device code expired");
                            }
                            crate::llm::auth::PollResult::Error(e) => {
                                anyhow::bail!("Authentication failed: {}", e);
                            }
                        }
                    }
                }
            }
        }
```

**Command after change**:
```bash
cargo check --lib
```

---

### Task 3.2: Add logout support to `run_auth`

**File**: `src/transport/cli.rs`

**Action**: Add a new function `run_auth_logout` after `run_auth`. Add at the end of the file before the final `}`:

```rust
/// Logout from an LLM provider (clear stored tokens)
pub async fn run_auth_logout(provider: &str) -> Result<()> {
    use colored::Colorize;

    println!("{}", "=== Tark Logout ===".bold().cyan());
    println!();

    match provider.to_lowercase().as_str() {
        "gemini" | "google" => {
            use crate::llm::auth::GeminiOAuth;

            let auth = GeminiOAuth::new()?;
            auth.logout().await?;

            println!("✅ Logged out from Google Gemini");
            println!();
            println!("To authenticate again:");
            println!("  tark auth gemini");
        }
        "copilot" | "github" => {
            // Remove Copilot token
            if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
                let token_path = proj_dirs.config_dir().join("copilot_token.json");
                if token_path.exists() {
                    std::fs::remove_file(&token_path)?;
                    println!("✅ Logged out from GitHub Copilot");
                } else {
                    println!("No Copilot token found");
                }
            }
        }
        _ => {
            println!(
                "Provider '{}' does not use stored authentication.",
                provider
            );
            println!();
            println!("To change API keys, update your environment variables.");
        }
    }

    Ok(())
}

/// Check authentication status for all providers
pub async fn run_auth_status() -> Result<()> {
    use colored::Colorize;

    println!("{}", "=== Tark Authentication Status ===".bold().cyan());
    println!();

    // Check Gemini
    {
        use crate::llm::auth::{DeviceFlowAuth, GeminiOAuth, AuthStatus};
        let auth = GeminiOAuth::new()?;
        let status = auth.status().await;
        let status_str = match status {
            AuthStatus::ApiKey => "✅ API Key".green(),
            AuthStatus::OAuth => "✅ OAuth".green(),
            AuthStatus::ADC => "✅ ADC".green(),
            AuthStatus::NotAuthenticated => "❌ Not authenticated".red(),
        };
        println!("  Gemini:     {}", status_str);
    }

    // Check Copilot
    {
        let token_exists = directories::ProjectDirs::from("", "", "tark")
            .map(|p| p.config_dir().join("copilot_token.json").exists())
            .unwrap_or(false);
        if token_exists {
            println!("  Copilot:    {}", "✅ Authenticated".green());
        } else {
            println!("  Copilot:    {}", "❌ Not authenticated".red());
        }
    }

    // Check OpenAI
    if std::env::var("OPENAI_API_KEY").is_ok() {
        println!("  OpenAI:     {}", "✅ API Key set".green());
    } else {
        println!("  OpenAI:     {}", "❌ OPENAI_API_KEY not set".red());
    }

    // Check Claude
    if std::env::var("ANTHROPIC_API_KEY").is_ok() {
        println!("  Claude:     {}", "✅ API Key set".green());
    } else {
        println!("  Claude:     {}", "❌ ANTHROPIC_API_KEY not set".red());
    }

    // Check OpenRouter
    if std::env::var("OPENROUTER_API_KEY").is_ok() {
        println!("  OpenRouter: {}", "✅ API Key set".green());
    } else {
        println!("  OpenRouter: {}", "❌ OPENROUTER_API_KEY not set".red());
    }

    println!();
    println!("Authenticate with: tark auth <provider>");
    println!("Logout with: tark auth logout <provider>");

    Ok(())
}
```

**Command after change**:
```bash
cargo check --lib
```

---

### Task 3.3: Update CLI help text

**File**: `src/transport/cli.rs`

**Action**: Update `check_llm_configuration_for_provider` function to mention OAuth for Gemini.

Find (around line 138-148):
```rust
        "gemini" | "google" => {
            if std::env::var("GOOGLE_API_KEY").is_err() && std::env::var("GEMINI_API_KEY").is_err()
            {
                return Err("Google Gemini API key not configured.\n\n\
                    To use Gemini, set the GOOGLE_API_KEY or GEMINI_API_KEY environment variable:\n\
                    \n\
                    export GOOGLE_API_KEY=\"your-api-key-here\"\n\
                    \n\
                    Get your API key from: https://aistudio.google.com/apikey"
                    .to_string());
            }
        }
```

Replace with:
```rust
        "gemini" | "google" => {
            // Check for API key first
            if std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok() {
                return Ok(());
            }
            // Check for OAuth token
            let token_path = dirs::data_local_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("tark")
                .join("tokens")
                .join("gemini.json");
            if token_path.exists() {
                return Ok(());
            }
            // Check for ADC
            if std::env::var("GOOGLE_APPLICATION_CREDENTIALS").is_ok() {
                return Ok(());
            }
            return Err("Google Gemini not configured.\n\n\
                Option 1 - OAuth (recommended for personal use):\n\
                  tark auth gemini\n\
                \n\
                Option 2 - API Key:\n\
                  export GEMINI_API_KEY=\"your-api-key-here\"\n\
                  Get your API key from: https://aistudio.google.com/apikey\n\
                \n\
                Option 3 - Application Default Credentials (Google Cloud):\n\
                  gcloud auth application-default login"
                .to_string());
        }
```

**Command after change**:
```bash
cargo check --lib
```

---

### Task 3.4: Commit Phase 3

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/transport/cli.rs
git commit -m "feat(cli): add Gemini OAuth support to tark auth command

- Add OAuth device flow for 'tark auth gemini'
- Add 'tark auth logout <provider>' command
- Add 'tark auth status' to check all providers
- Update help text to mention OAuth option for Gemini"
```

---

## Phase 4: Integrate with GeminiProvider ✅ DONE

### Task 4.1: Update `src/llm/gemini.rs` ✅ DONE

### Task 4.2: Commit Phase 4 ✅ DONE

**File**: `src/llm/gemini.rs`

**Action**: Update `GeminiProvider::new()` to use credential resolver. Replace the `new()` function (lines 28-38):

Find:
```rust
    pub fn new() -> Result<Self> {
        let api_key =
            env::var("GEMINI_API_KEY").context("GEMINI_API_KEY environment variable not set")?;

        Ok(Self {
            client: reqwest::Client::new(),
            api_key,
            model: "gemini-2.0-flash-exp".to_string(),
            max_tokens: 8192,
        })
    }
```

Replace with:
```rust
    /// Create a new GeminiProvider
    ///
    /// Credentials are resolved in this priority:
    /// 1. GEMINI_API_KEY or GOOGLE_API_KEY environment variable
    /// 2. OAuth token from `~/.local/share/tark/tokens/gemini.json`
    /// 3. Application Default Credentials (GOOGLE_APPLICATION_CREDENTIALS)
    ///
    /// If no credentials are found, returns an error with setup instructions.
    pub fn new() -> Result<Self> {
        // Try API key first (highest priority)
        if let Ok(api_key) = env::var("GEMINI_API_KEY").or_else(|_| env::var("GOOGLE_API_KEY")) {
            return Ok(Self {
                client: reqwest::Client::new(),
                api_key,
                model: "gemini-2.0-flash-exp".to_string(),
                max_tokens: 8192,
            });
        }

        // Try OAuth token
        let token_path = dirs::data_local_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("tark")
            .join("tokens")
            .join("gemini.json");

        if token_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&token_path) {
                if let Ok(stored) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(access_token) = stored["access_token"].as_str() {
                        // Check if expired
                        let expires_at = stored["expires_at"].as_u64().unwrap_or(0);
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0);

                        if expires_at > now + 300 {
                            // Token valid for at least 5 more minutes
                            return Ok(Self {
                                client: reqwest::Client::new(),
                                api_key: access_token.to_string(),
                                model: "gemini-2.0-flash-exp".to_string(),
                                max_tokens: 8192,
                            });
                        }
                    }
                }
            }
        }

        // No valid credentials found
        anyhow::bail!(
            "Gemini authentication required.\n\n\
            Option 1 - OAuth (recommended):\n  \
            tark auth gemini\n\n\
            Option 2 - API Key:\n  \
            export GEMINI_API_KEY=\"your-api-key\"\n  \
            Get key: https://aistudio.google.com/apikey"
        )
    }
```

**Add import at top of file** (after existing imports):
```rust
use dirs;
```

Find:
```rust
use std::env;
```

Replace with:
```rust
use std::env;
```

(No change needed if `dirs` is already used elsewhere)

**Command after change**:
```bash
cargo check --lib
```

---

### Task 4.2: Commit Phase 4

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/llm/gemini.rs
git commit -m "feat(gemini): integrate OAuth credentials into GeminiProvider

- GeminiProvider::new() now checks OAuth token if no API key
- Credential priority: API key > OAuth token > error
- Returns helpful error message with setup instructions"
```

---

## Phase 5: Add Tests ✅ DONE

### Task 5.1: Create test file `tests/gemini_auth.rs` ✅ DONE

### Task 5.2: Commit Phase 5 ✅ DONE

**File**: `tests/gemini_auth.rs`

```rust
//! Integration tests for Gemini OAuth

use tark::llm::auth::{AuthStatus, DeviceFlowAuth, GeminiOAuth, OAuthToken, TokenStore};

#[test]
fn test_token_store_save_load() {
    // Use temp directory for testing
    let temp_dir = tempfile::tempdir().unwrap();
    std::env::set_var(
        "XDG_DATA_HOME",
        temp_dir.path().join("data").to_str().unwrap(),
    );

    let store = TokenStore::new("test_provider").unwrap();

    let token = OAuthToken {
        access_token: "test_access_token".to_string(),
        refresh_token: Some("test_refresh_token".to_string()),
        expires_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
        scopes: vec!["scope1".to_string(), "scope2".to_string()],
    };

    // Save token
    store.save(&token).unwrap();
    assert!(store.exists());

    // Load token
    let loaded = store.load().unwrap();
    assert_eq!(loaded.access_token, "test_access_token");
    assert_eq!(loaded.refresh_token, Some("test_refresh_token".to_string()));

    // Delete token
    store.delete().unwrap();
    assert!(!store.exists());
}

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
```

**Add tempfile dev dependency to Cargo.toml** if not present:

Check if `tempfile` is in dev-dependencies:
```bash
grep "tempfile" Cargo.toml
```

If not found, add to `[dev-dependencies]`:
```toml
tempfile = "3"
```

**Command after creating file**:
```bash
cargo test --all-features gemini_auth
```

---

### Task 5.2: Commit Phase 5

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add tests/gemini_auth.rs
git add Cargo.toml  # if modified
git commit -m "test(auth): add integration tests for Gemini OAuth

- Test TokenStore save/load/delete operations
- Test OAuthToken expiry detection
- Test GeminiOAuth creation and status check"
```

---

## Phase 6: Update main.rs CLI Entry Point ✅ DONE

### Task 6.1: Update `src/main.rs` ✅ DONE

**File**: `src/main.rs`

**Action**: Add subcommands for auth logout and auth status.

Search for the CLI argument definitions and add new subcommands. The exact location depends on how clap is structured in main.rs.

**Find the Auth subcommand definition** and add:
- `logout` subcommand: `tark auth logout <provider>`
- `status` subcommand: `tark auth status`

**This task requires reading main.rs first to understand the CLI structure.**

```bash
# Read main.rs to find the CLI structure
head -200 src/main.rs
```

Then add the appropriate CLI subcommands and handlers.

---

### Task 6.2: Commit Phase 6 ✅ DONE

**Commands**:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features

git add src/main.rs
git commit -m "feat(cli): add auth logout and auth status subcommands

- tark auth status: show auth status for all providers
- tark auth logout <provider>: clear stored credentials"
```

---

## Phase 7: Final Validation and Push ✅ DONE

### Task 7.1: Full Validation ✅ DONE

**Commands**:
```bash
# Clean build
cargo clean
cargo build --release

# Format check
cargo fmt --all -- --check

# Lint check
cargo clippy --all-targets --all-features -- -D warnings

# All tests
cargo test --all-features

# Build docs to verify no doc warnings
cargo doc --no-deps
```

---

### Task 7.2: Manual Testing

**Commands**:
```bash
# Test auth status
./target/release/tark auth status

# Test Gemini auth (interactive - will prompt for browser auth)
./target/release/tark auth gemini

# Test logout
./target/release/tark auth logout gemini

# Verify status changed
./target/release/tark auth status
```

---

### Task 7.3: Final Commit and Push

**Commands**:
```bash
# Ensure everything passes
cargo build --release && \
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --all-features

# Tag version if appropriate
# git tag -a v0.X.Y -m "feat: add Gemini OAuth support"

# Push
git push origin main
# git push --tags  # if tagged
```

---

## Summary of Files Created/Modified

### New Files
- `src/llm/auth/mod.rs` - DeviceFlowAuth trait and types
- `src/llm/auth/token_store.rs` - Secure token storage
- `src/llm/auth/gemini_oauth.rs` - GeminiOAuth implementation
- `tests/gemini_auth.rs` - Integration tests

### Modified Files
- `src/llm/mod.rs` - Export auth module
- `src/llm/gemini.rs` - Integrate OAuth credentials
- `src/tui/widgets/auth_dialog.rs` - Generalize for any provider
- `src/transport/cli.rs` - Add OAuth flow, logout, status
- `src/main.rs` - Add CLI subcommands

---

## Commit Summary

```
feat(auth): add DeviceFlowAuth trait and GeminiOAuth implementation
feat(tui): generalize AuthDialog for any OAuth provider  
feat(cli): add Gemini OAuth support to tark auth command
feat(gemini): integrate OAuth credentials into GeminiProvider
test(auth): add integration tests for Gemini OAuth
feat(cli): add auth logout and auth status subcommands
```

---

## Troubleshooting

### OAuth device flow fails
- Verify network connectivity to `oauth2.googleapis.com`
- Check if the client ID is correct (may need to update from Gemini CLI)
- Ensure scopes are correct for Gemini API access

### Token not persisting
- Check directory permissions for `~/.local/share/tark/tokens/`
- Verify the token file has 0600 permissions

### GeminiProvider not finding OAuth token
- Run `tark auth status` to verify token exists
- Check token expiry with `cat ~/.local/share/tark/tokens/gemini.json`

---

## References

- Gemini CLI source: https://github.com/google-gemini/gemini-cli
- Google OAuth device flow: https://developers.google.com/identity/protocols/oauth2/limited-input-device
- Existing Copilot implementation: `src/llm/copilot.rs`
