//! Secure credential storage for MCP authorization tokens (R6 S17).
//!
//! Remote MCP authorization (OAuth, client credentials) must persist tokens
//! through the platform-appropriate secure mechanism, scoped to the correct
//! issuer/server, and never display them. This module provides the
//! abstraction plus the backends available today:
//!
//! - [`EncryptedFileStore`]: Argon2id + ChaCha20-Poly1305 vault (via
//!   [`crate::secure_store`]) keyed by a random master key held in an
//!   owner-only (`0600`) file next to the vault. Protects against other OS
//!   users; like any file-backed store it does not defend against same-user
//!   attackers or memory inspection (in-memory wiping is best-effort).
//! - [`MemoryCredentialStore`]: process-local store for tests and headless
//!   flows. Never persisted.
//!
//! A platform keyring backend (Keychain/Secret Service/Credential Manager)
//! is intentionally a separate follow-up: it needs a new `keyring`
//! dependency, and the [`CredentialStore`] trait keeps that swap mechanical.
//! Token values never appear in logs, traces, error messages, or [`Debug`]
//! output.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Lookup key for a stored credential: `service/account`.
///
/// Convention: `service` names the credential class (`"tark-mcp-oauth"`),
/// `account` scopes it to the issuer or server URL.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CredentialKey {
    /// Credential class, e.g. `"tark-mcp-oauth"`.
    pub service: String,
    /// Scope, e.g. the issuer URL or server id.
    pub account: String,
}

impl CredentialKey {
    /// Create a key from its parts (both must be non-empty).
    pub fn new(service: &str, account: &str) -> Result<Self> {
        if service.trim().is_empty() || account.trim().is_empty() {
            anyhow::bail!("Credential key service and account must both be non-empty");
        }
        Ok(Self {
            service: service.to_string(),
            account: account.to_string(),
        })
    }

    /// Parse `"service/account"`.
    pub fn parse(input: &str) -> Result<Self> {
        let (service, account) = input
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("Credential key must look like 'service/account'"))?;
        Self::new(service, account)
    }

    /// Canonical `"service/account"` form (map key; never logged with values).
    fn canonical(&self) -> String {
        format!("{}/{}", self.service, self.account)
    }
}

impl std::fmt::Display for CredentialKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The key identifies the slot, not the secret; safe to display.
        write!(f, "{}", self.canonical())
    }
}

/// Secure token storage backend.
///
/// Implementations must never expose token values in logs, errors, or
/// [`Debug`] output.
pub trait CredentialStore: Send + Sync + std::fmt::Debug {
    /// Persist `token` under `key`, overwriting any previous value.
    fn store_token(&self, key: &CredentialKey, token: &str) -> Result<()>;
    /// Load the token for `key`, or `None` when absent.
    fn load_token(&self, key: &CredentialKey) -> Result<Option<String>>;
    /// Delete the token for `key`. Returns true when a value existed.
    fn delete_token(&self, key: &CredentialKey) -> Result<bool>;
}

/// File-backed credential vault encrypted with a local master key.
///
/// Layout next to `vault_path`:
/// - `<vault>.key`: 32 random bytes (hex), owner-only, created once.
/// - `<vault>`: JSON map of canonical key → encrypted blob, owner-only,
///   written atomically (temp file + rename).
#[derive(Debug)]
pub struct EncryptedFileStore {
    vault_path: PathBuf,
    key_path: PathBuf,
    /// Serializes vault read-modify-write cycles within this process.
    /// Cross-process concurrent writes fail closed (temp-file creation is
    /// exclusive) instead of interleaving; callers may retry.
    lock: RwLock<()>,
}

impl EncryptedFileStore {
    /// Open (or initialize) the vault at `vault_path`.
    pub fn open(vault_path: &Path) -> Result<Self> {
        let key_path = vault_path.with_extension("key");
        if let Some(parent) = vault_path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("Failed to create {}", parent.display()))?;
            }
        }
        load_or_create_master_key(&key_path)?;
        Ok(Self {
            vault_path: vault_path.to_path_buf(),
            key_path,
            lock: RwLock::new(()),
        })
    }

    /// Path of the vault file (safe to log; holds ciphertext only).
    pub fn vault_path(&self) -> &Path {
        &self.vault_path
    }

    fn master_key(&self) -> Result<String> {
        let raw = std::fs::read_to_string(&self.key_path)
            .with_context(|| format!("Failed to read {}", self.key_path.display()))?;
        let key = raw.trim().to_string();
        if key.is_empty() {
            anyhow::bail!("Master key file is empty");
        }
        Ok(key)
    }

    fn read_vault(&self) -> Result<BTreeMap<String, String>> {
        if !self.vault_path.exists() {
            return Ok(BTreeMap::new());
        }
        let raw = std::fs::read_to_string(&self.vault_path)
            .with_context(|| format!("Failed to read {}", self.vault_path.display()))?;
        if raw.trim().is_empty() {
            return Ok(BTreeMap::new());
        }
        let vault: VaultFile = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse {}", self.vault_path.display()))?;
        if vault.version != VAULT_VERSION {
            anyhow::bail!("Unsupported credential vault version");
        }
        Ok(vault.entries)
    }

    fn write_vault(&self, entries: &BTreeMap<String, String>) -> Result<()> {
        let vault = VaultFile {
            version: VAULT_VERSION,
            entries: entries.clone(),
        };
        let raw = serde_json::to_string_pretty(&vault)?;
        let tmp = self.vault_path.with_extension("json.tmp");
        super::loopback_cred::write_owner_only_file(&tmp, raw.as_bytes())
            .with_context(|| format!("Failed to stage {}", self.vault_path.display()))?;
        std::fs::rename(&tmp, &self.vault_path)
            .with_context(|| format!("Failed to commit {}", self.vault_path.display()))?;
        Ok(())
    }
}

impl CredentialStore for EncryptedFileStore {
    fn store_token(&self, key: &CredentialKey, token: &str) -> Result<()> {
        if token.is_empty() {
            anyhow::bail!("Refusing to store an empty token");
        }
        let _guard = self
            .lock
            .write()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?;
        let master = self.master_key()?;
        let blob = crate::secure_store::encrypt_string(token, &master)?;
        let mut entries = self.read_vault()?;
        entries.insert(key.canonical(), blob);
        self.write_vault(&entries)
    }

    fn load_token(&self, key: &CredentialKey) -> Result<Option<String>> {
        let _guard = self
            .lock
            .read()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?;
        let entries = self.read_vault()?;
        let Some(blob) = entries.get(&key.canonical()) else {
            return Ok(None);
        };
        let master = self.master_key()?;
        // Decryption failure surfaces without the token value (R3/R6).
        let token = crate::secure_store::decrypt_string(blob, &master)
            .context("Stored credential failed to decrypt")?;
        Ok(Some(token))
    }

    fn delete_token(&self, key: &CredentialKey) -> Result<bool> {
        let _guard = self
            .lock
            .write()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?;
        let mut entries = self.read_vault()?;
        let existed = entries.remove(&key.canonical()).is_some();
        if existed {
            self.write_vault(&entries)?;
        }
        Ok(existed)
    }
}

const VAULT_VERSION: u8 = 1;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct VaultFile {
    version: u8,
    entries: BTreeMap<String, String>,
}

/// Load the master key, creating it once with owner-only semantics.
///
/// Concurrent creators race on `O_EXCL`: the loser reads the winner's file.
fn load_or_create_master_key(path: &Path) -> Result<()> {
    let mut random = [0u8; 32];
    {
        use rand::RngCore;
        rand::rngs::OsRng.fill_bytes(&mut random);
    }
    match super::loopback_cred::write_owner_only_file(path, hex::encode(random).as_bytes()) {
        Ok(()) => Ok(()),
        Err(e) => {
            // Created concurrently (or already initialized): use the file.
            if path.exists() {
                return Ok(());
            }
            Err(e)
        }
    }
}

/// Process-local credential store for tests and headless flows.
#[derive(Default)]
pub struct MemoryCredentialStore {
    entries: RwLock<BTreeMap<String, String>>,
}

impl MemoryCredentialStore {
    /// Create an empty in-memory store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Debug for MemoryCredentialStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Token values must never appear in diagnostics (R3/R6).
        let count = self.entries.read().map(|e| e.len()).unwrap_or(0);
        f.debug_struct("MemoryCredentialStore")
            .field("entries", &count)
            .finish()
    }
}

impl CredentialStore for MemoryCredentialStore {
    fn store_token(&self, key: &CredentialKey, token: &str) -> Result<()> {
        if token.is_empty() {
            anyhow::bail!("Refusing to store an empty token");
        }
        self.entries
            .write()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?
            .insert(key.canonical(), token.to_string());
        Ok(())
    }

    fn load_token(&self, key: &CredentialKey) -> Result<Option<String>> {
        Ok(self
            .entries
            .read()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?
            .get(&key.canonical())
            .cloned())
    }

    fn delete_token(&self, key: &CredentialKey) -> Result<bool> {
        Ok(self
            .entries
            .write()
            .map_err(|_| anyhow::anyhow!("Store lock poisoned"))?
            .remove(&key.canonical())
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> CredentialKey {
        CredentialKey::new("tark-mcp-oauth", "https://example.com").expect("key")
    }

    #[test]
    fn credential_key_parse_and_validate() {
        let key = CredentialKey::parse("svc/acct").expect("parse");
        assert_eq!(key.service, "svc");
        assert_eq!(key.account, "acct");
        assert_eq!(key.to_string(), "svc/acct");
        assert!(CredentialKey::parse("no-slash").is_err());
        assert!(CredentialKey::new("", "a").is_err());
        assert!(CredentialKey::new("s", "").is_err());
    }

    #[test]
    fn file_store_round_trip_overwrite_and_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = EncryptedFileStore::open(&dir.path().join("credentials.json")).expect("open");
        let key = test_key();
        assert_eq!(store.load_token(&key).expect("load"), None);
        store.store_token(&key, "token-one").expect("store");
        assert_eq!(
            store.load_token(&key).expect("load"),
            Some("token-one".to_string())
        );
        store.store_token(&key, "token-two").expect("overwrite");
        assert_eq!(
            store.load_token(&key).expect("load"),
            Some("token-two".to_string())
        );
        assert!(store.delete_token(&key).expect("delete"));
        assert!(!store.delete_token(&key).expect("delete again"));
        assert_eq!(store.load_token(&key).expect("load"), None);
    }

    #[test]
    fn file_store_rejects_empty_token_and_missing_key_is_none() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = EncryptedFileStore::open(&dir.path().join("credentials.json")).expect("open");
        assert!(store.store_token(&test_key(), "").is_err());
        let other = CredentialKey::new("svc", "other").expect("key");
        assert_eq!(store.load_token(&other).expect("load"), None);
    }

    #[test]
    fn file_store_vault_and_key_are_owner_only() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("credentials.json");
        let store = EncryptedFileStore::open(&vault).expect("open");
        store.store_token(&test_key(), "s3cret").expect("store");
        for path in [vault.clone(), vault.with_extension("key")] {
            let meta = std::fs::symlink_metadata(&path).expect("stat");
            assert!(meta.file_type().is_file());
            assert!(!meta.file_type().is_symlink());
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(meta.permissions().mode() & 0o777, 0o600);
            }
        }
        // Ciphertext on disk must not contain the token.
        let raw = std::fs::read_to_string(store.vault_path()).expect("read vault");
        assert!(!raw.contains("s3cret"));
    }

    #[test]
    fn file_store_wrong_master_key_fails_decrypt() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("credentials.json");
        let store = EncryptedFileStore::open(&vault).expect("open");
        store.store_token(&test_key(), "s3cret").expect("store");
        // Swap in a different master key: decryption must fail closed.
        std::fs::remove_file(vault.with_extension("key")).expect("remove key");
        let _ = EncryptedFileStore::open(&vault).expect("reopen rotates key");
        assert!(store.load_token(&test_key()).is_err());
    }

    #[test]
    fn file_store_reopens_existing_vault() {
        let dir = tempfile::tempdir().expect("tempdir");
        let vault = dir.path().join("credentials.json");
        EncryptedFileStore::open(&vault)
            .expect("open")
            .store_token(&test_key(), "persisted")
            .expect("store");
        let reopened = EncryptedFileStore::open(&vault).expect("reopen");
        assert_eq!(
            reopened.load_token(&test_key()).expect("load"),
            Some("persisted".to_string())
        );
    }

    #[test]
    fn memory_store_round_trip_and_redacted_debug() {
        let store = MemoryCredentialStore::new();
        let key = test_key();
        assert_eq!(store.load_token(&key).expect("load"), None);
        store.store_token(&key, "s3cret").expect("store");
        assert_eq!(
            store.load_token(&key).expect("load"),
            Some("s3cret".to_string())
        );
        assert!(store.delete_token(&key).expect("delete"));
        let rendered = format!("{:?}", store);
        assert!(!rendered.contains("s3cret"));
    }
}
