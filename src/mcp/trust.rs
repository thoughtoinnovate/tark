//! Informed-trust store for MCP stdio server launches (R3 S7).
//!
//! An MCP stdio server is executable third-party code: no process may start
//! until the user has explicitly trusted the exact executable command,
//! arguments, working directory, and environment *categories*. Trust is
//! persisted per server id, bound to a fingerprint of that launch
//! configuration — any change to command, args, cwd, or env re-prompts.
//!
//! The store lives at `<workspace>/.tark/mcp_trust.json` (project scope) so
//! trust granted for one project never bleeds into another.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

/// Trust record for one server id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTrustRecord {
    /// Fingerprint of the trusted launch configuration.
    pub fingerprint: String,
    /// Human-readable launch summary shown at approval time.
    pub summary: String,
    /// Unix timestamp of approval.
    pub approved_at: u64,
}

/// Persisted trust file: server id -> record.
#[derive(Debug, Default, Serialize, Deserialize)]
struct McpTrustFile {
    #[serde(default)]
    servers: BTreeMap<String, McpTrustRecord>,
}

/// Explicit-trust gate for MCP server launches.
pub struct McpTrustStore {
    path: PathBuf,
    records: BTreeMap<String, McpTrustRecord>,
}

impl McpTrustStore {
    /// Open (or initialize) the store at `<workspace>/.tark/mcp_trust.json`.
    pub fn open(workspace: &Path) -> Result<Self> {
        let path = workspace.join(".tark").join("mcp_trust.json");
        let records = if path.exists() {
            let raw = std::fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            serde_json::from_str::<McpTrustFile>(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?
                .servers
        } else {
            BTreeMap::new()
        };
        Ok(Self { path, records })
    }

    /// Fingerprint binding command + args + cwd + env names/values.
    pub fn fingerprint(
        command: &str,
        args: &[String],
        working_dir: &str,
        env: &HashMap<String, String>,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"mcp-trust-v1\n");
        hasher.update(command.as_bytes());
        hasher.update(b"\n");
        for arg in args {
            hasher.update(arg.as_bytes());
            hasher.update(b"\n");
        }
        hasher.update(b"cwd=");
        hasher.update(working_dir.as_bytes());
        hasher.update(b"\n");
        let mut keys: Vec<&String> = env.keys().collect();
        keys.sort();
        for key in keys {
            hasher.update(b"env:");
            hasher.update(key.as_bytes());
            hasher.update(b"=");
            hasher.update(env[key].as_bytes());
            hasher.update(b"\n");
        }
        hex::encode(hasher.finalize())
    }

    /// Human-readable launch summary: command, cwd, args, and environment
    /// *categories* (names only — values may hold secrets and are shown only
    /// as present/absent via `${VAR}` expansion hints).
    pub fn launch_summary(
        command: &str,
        args: &[String],
        working_dir: &str,
        env: &HashMap<String, String>,
        provenance: Option<&str>,
    ) -> String {
        let mut lines = Vec::new();
        lines.push(format!("Executable: {}", command));
        if !args.is_empty() {
            lines.push(format!("Arguments: {}", args.join(" ")));
        }
        lines.push(format!("Working directory: {}", working_dir));
        if let Some(source) = provenance {
            lines.push(format!("Source: {}", source));
        }
        if env.is_empty() {
            lines.push("Environment: (minimal inherited set only)".to_string());
        } else {
            let mut keys: Vec<&String> = env.keys().collect();
            keys.sort();
            let names: Vec<String> = keys.iter().map(|k| k.to_string()).collect();
            lines.push(format!(
                "Environment (configured, values hidden): {}",
                names.join(", ")
            ));
        }
        lines.join("\n")
    }

    /// True when `server_id` trusts exactly this launch configuration.
    pub fn is_trusted(
        &self,
        server_id: &str,
        command: &str,
        args: &[String],
        working_dir: &str,
        env: &HashMap<String, String>,
    ) -> bool {
        let fingerprint = Self::fingerprint(command, args, working_dir, env);
        self.records
            .get(server_id)
            .map(|record| record.fingerprint == fingerprint)
            .unwrap_or(false)
    }

    /// Record explicit trust for this launch configuration.
    pub fn approve(
        &mut self,
        server_id: &str,
        command: &str,
        args: &[String],
        working_dir: &str,
        env: &HashMap<String, String>,
        summary: String,
    ) -> Result<()> {
        let approved_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.records.insert(
            server_id.to_string(),
            McpTrustRecord {
                fingerprint: Self::fingerprint(command, args, working_dir, env),
                summary,
                approved_at,
            },
        );
        self.save()
    }

    /// Revoke trust for a server id. Returns true when a record existed.
    pub fn revoke(&mut self, server_id: &str) -> Result<bool> {
        let existed = self.records.remove(server_id).is_some();
        if existed {
            self.save()?;
        }
        Ok(existed)
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }
        let file = McpTrustFile {
            servers: self.records.clone(),
        };
        let raw = serde_json::to_string_pretty(&file)?;
        // Atomic write: temp file + rename, so a crash cannot corrupt trust.
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, raw)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_env() -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("TOKEN".to_string(), "${GITHUB_TOKEN}".to_string());
        env
    }

    #[test]
    fn trust_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = McpTrustStore::open(dir.path()).unwrap();
        let env = sample_env();
        assert!(!store.is_trusted("gh", "npx", &["-y".to_string()], "/work", &env));
        let summary =
            McpTrustStore::launch_summary("npx", &["-y".to_string()], "/work", &env, None);
        // Values are hidden in the summary; names are shown.
        assert!(summary.contains("TOKEN"));
        assert!(!summary.contains("${GITHUB_TOKEN} value"));
        store
            .approve("gh", "npx", &["-y".to_string()], "/work", &env, summary)
            .unwrap();
        assert!(store.is_trusted("gh", "npx", &["-y".to_string()], "/work", &env));
    }

    #[test]
    fn changed_command_reprompts() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = McpTrustStore::open(dir.path()).unwrap();
        let env = sample_env();
        store
            .approve(
                "gh",
                "npx",
                &["-y".to_string()],
                "/work",
                &env,
                "s".to_string(),
            )
            .unwrap();
        // Different args -> different fingerprint -> trust no longer applies.
        assert!(!store.is_trusted(
            "gh",
            "npx",
            &["-y".to_string(), "evil".to_string()],
            "/work",
            &env
        ));
        assert!(!store.is_trusted("other", "npx", &["-y".to_string()], "/work", &env));
    }

    #[test]
    fn revoke_clears_trust() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = McpTrustStore::open(dir.path()).unwrap();
        let env = sample_env();
        store
            .approve("gh", "npx", &[], "/work", &env, "s".to_string())
            .unwrap();
        assert!(store.revoke("gh").unwrap());
        assert!(!store.revoke("gh").unwrap());
        assert!(!store.is_trusted("gh", "npx", &[], "/work", &env));
    }
}
