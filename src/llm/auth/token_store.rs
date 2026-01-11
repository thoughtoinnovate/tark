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
        std::fs::create_dir_all(&tokens_dir).context("Failed to create tokens directory")?;

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
        let content = std::fs::read_to_string(&self.path).context("Failed to read token file")?;

        let stored: StoredToken =
            serde_json::from_str(&content).context("Failed to parse token file")?;

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
        std::fs::write(&temp_path, &content).context("Failed to write temp token file")?;

        // Set secure permissions (0600 = owner read/write only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&temp_path, perms)
                .context("Failed to set token file permissions")?;
        }

        // Atomic rename
        std::fs::rename(&temp_path, &self.path).context("Failed to save token file")?;

        tracing::debug!("Saved {} token to {:?}", self.provider, self.path);
        Ok(())
    }

    /// Delete stored token
    pub fn delete(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).context("Failed to delete token file")?;
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

    #[test]
    fn test_token_store_path() {
        // Just verify we can create a token store
        let store = TokenStore::new("test_provider");
        assert!(store.is_ok());
        let store = store.unwrap();
        assert!(store.path.to_string_lossy().contains("test_provider"));
    }
}
