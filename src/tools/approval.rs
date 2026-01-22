//! Approval gate for risky tool operations.
//!
//! This module manages approval patterns and modes for tool execution.
//! It integrates with the TUI via the InteractionRequest channel to
//! prompt users for approval when needed.

#![allow(dead_code)]

use anyhow::{Context, Result};
use glob::Pattern;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::sync::oneshot;

use super::questionnaire::{
    ApprovalChoice, ApprovalPattern, ApprovalRequest, ApprovalResponse, InteractionRequest,
    InteractionSender, SuggestedPattern,
};
use super::risk::{MatchType, RiskLevel, TrustLevel};

/// Result of checking approval status
#[derive(Debug)]
pub enum ApprovalStatus {
    /// Operation is approved, proceed with execution
    Approved,
    /// Operation was denied by user
    Denied,
    /// Operation is blocked by a denial pattern
    Blocked(String),
}

/// File format for persistent approvals
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ApprovalStore {
    /// Storage version for future migrations
    #[serde(default = "default_version")]
    pub version: u32,
    /// Approved patterns
    #[serde(default)]
    pub approvals: Vec<ApprovalPattern>,
    /// Denied patterns
    #[serde(default)]
    pub denials: Vec<ApprovalPattern>,
}

fn default_version() -> u32 {
    1
}

impl ApprovalStore {
    /// Create an empty store
    pub fn new() -> Self {
        Self {
            version: 1,
            approvals: Vec::new(),
            denials: Vec::new(),
        }
    }
}

/// The approval gate manages approval checking and storage.
pub struct ApprovalGate {
    /// Current trust level
    pub trust_level: TrustLevel,
    /// Channel to send interaction requests to TUI
    interaction_tx: Option<InteractionSender>,
    /// Session-only approvals (cleared on restart)
    session_approvals: HashMap<String, Vec<ApprovalPattern>>,
    /// Session-only denials
    session_denials: HashMap<String, Vec<ApprovalPattern>>,
    /// Path to persistent storage
    storage_path: PathBuf,
    /// Cached persistent approvals
    persistent_approvals: Vec<ApprovalPattern>,
    /// Cached persistent denials
    persistent_denials: Vec<ApprovalPattern>,
}

impl ApprovalGate {
    /// Create a new approval gate
    pub fn new(storage_path: PathBuf, interaction_tx: Option<InteractionSender>) -> Self {
        let (approvals, denials) = Self::load_persistent(&storage_path);

        Self {
            trust_level: TrustLevel::default(),
            interaction_tx,
            session_approvals: HashMap::new(),
            session_denials: HashMap::new(),
            storage_path,
            persistent_approvals: approvals,
            persistent_denials: denials,
        }
    }

    /// Update storage path and reload persistent approvals
    pub fn set_storage_path(&mut self, storage_path: PathBuf) {
        self.storage_path = storage_path;
        let (approvals, denials) = Self::load_persistent(&self.storage_path);
        self.persistent_approvals = approvals;
        self.persistent_denials = denials;
        self.session_approvals.clear();
        self.session_denials.clear();
    }

    #[cfg(test)]
    pub fn storage_path(&self) -> &PathBuf {
        &self.storage_path
    }

    /// Create an approval gate without persistence (for testing)
    pub fn new_memory_only(interaction_tx: Option<InteractionSender>) -> Self {
        Self {
            trust_level: TrustLevel::default(),
            interaction_tx,
            session_approvals: HashMap::new(),
            session_denials: HashMap::new(),
            storage_path: PathBuf::new(),
            persistent_approvals: Vec::new(),
            persistent_denials: Vec::new(),
        }
    }

    /// Set the trust level
    pub fn set_trust_level(&mut self, level: TrustLevel) {
        self.trust_level = level;
    }

    /// Cycle to the next trust level
    pub fn cycle_trust_level(&mut self) -> TrustLevel {
        self.trust_level = self.trust_level.cycle_next();
        self.trust_level
    }

    /// Check and potentially request approval for a command.
    ///
    /// This is the main entry point for tool execution approval.
    /// Returns `Approved` if the operation can proceed.
    pub async fn check_and_approve(
        &mut self,
        tool: &str,
        command: &str,
        risk: RiskLevel,
    ) -> Result<ApprovalStatus> {
        // 1. Check if trust level requires approval for this risk level
        if !self.trust_level.needs_approval_check(risk) {
            return Ok(ApprovalStatus::Approved);
        }

        // 2. Check denials first (both session and persistent)
        if self.matches_denial(tool, command) {
            return Ok(ApprovalStatus::Blocked(format!(
                "Command '{}' is blocked by a denial pattern",
                command
            )));
        }

        // 3. Check session approvals
        if self.matches_session_approval(tool, command) {
            return Ok(ApprovalStatus::Approved);
        }

        // 4. Check persistent approvals
        if self.matches_persistent_approval(tool, command) {
            return Ok(ApprovalStatus::Approved);
        }

        // 5. Need to ask user - check if we have a channel
        let tx = match &self.interaction_tx {
            Some(tx) => tx.clone(),
            None => {
                // No TUI, default to approved (for non-interactive use)
                tracing::warn!(
                    "No TUI channel for approval, auto-approving {} {}",
                    tool,
                    command
                );
                return Ok(ApprovalStatus::Approved);
            }
        };

        // 6. Create approval request with suggested patterns
        let request = self.create_approval_request(tool, command, risk);

        // 7. Send request to TUI and wait for response
        let (response_tx, response_rx) = oneshot::channel();

        tx.send(InteractionRequest::Approval {
            request,
            responder: response_tx,
        })
        .await
        .context("Failed to send approval request to TUI")?;

        // Wait for response with timeout
        let timeout = std::time::Duration::from_secs(300); // 5 minutes
        let response = tokio::time::timeout(timeout, response_rx)
            .await
            .context("Approval request timed out")?
            .context("Approval channel closed")?;

        // 8. Handle the response
        self.handle_response(response)
    }

    /// Handle user's approval response
    fn handle_response(&mut self, response: ApprovalResponse) -> Result<ApprovalStatus> {
        match response.choice {
            ApprovalChoice::ApproveOnce => Ok(ApprovalStatus::Approved),
            ApprovalChoice::ApproveSession => {
                if let Some(pattern) = response.selected_pattern {
                    self.add_session_approval(pattern);
                }
                Ok(ApprovalStatus::Approved)
            }
            ApprovalChoice::ApproveAlways => {
                if let Some(pattern) = response.selected_pattern {
                    self.add_persistent_approval(pattern)?;
                }
                Ok(ApprovalStatus::Approved)
            }
            ApprovalChoice::Deny => Ok(ApprovalStatus::Denied),
            ApprovalChoice::DenyAlways => {
                if let Some(pattern) = response.selected_pattern {
                    self.add_persistent_denial(pattern)?;
                }
                Ok(ApprovalStatus::Denied)
            }
        }
    }

    /// Create an approval request with smart pattern suggestions
    pub fn create_approval_request(
        &self,
        tool: &str,
        command: &str,
        risk: RiskLevel,
    ) -> ApprovalRequest {
        let mut suggestions = Vec::new();

        // Suggestion 1: Exact command
        suggestions.push(SuggestedPattern {
            pattern: command.to_string(),
            match_type: MatchType::Exact,
            description: format!("Approve exactly: {}", command),
        });

        // Suggestion 2: Prefix (first word + second word)
        if let Some(prefix) = Self::suggest_prefix(command) {
            suggestions.push(SuggestedPattern {
                pattern: prefix.clone(),
                match_type: MatchType::Prefix,
                description: format!("Approve commands starting with: {}", prefix),
            });
        }

        // Suggestion 3: Glob pattern (for file operations)
        if let Some(glob) = Self::suggest_glob(tool, command) {
            suggestions.push(SuggestedPattern {
                pattern: glob.clone(),
                match_type: MatchType::Glob,
                description: format!("Approve pattern: {}", glob),
            });
        }

        ApprovalRequest {
            tool: tool.to_string(),
            command: command.to_string(),
            risk_level: risk,
            suggested_patterns: suggestions,
        }
    }

    /// Suggest a prefix pattern
    fn suggest_prefix(command: &str) -> Option<String> {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.len() >= 2 {
            Some(format!("{} {}", parts[0], parts[1]))
        } else if !parts.is_empty() {
            Some(parts[0].to_string())
        } else {
            None
        }
    }

    /// Suggest a glob pattern for file operations
    fn suggest_glob(tool: &str, command: &str) -> Option<String> {
        match tool {
            "delete_file" => {
                // "src/temp/file.rs" -> "src/temp/*"
                let path = PathBuf::from(command);
                path.parent().map(|p| format!("{}/*", p.display()))
            }
            "shell" => {
                // "rm src/temp.bak" -> "rm *.bak"
                if command.starts_with("rm ") {
                    let file = command.trim_start_matches("rm ").trim();
                    PathBuf::from(file)
                        .extension()
                        .map(|ext| format!("rm *.{}", ext.to_string_lossy()))
                } else if command.starts_with("git ") {
                    // "git push origin main" -> "git push *"
                    let parts: Vec<&str> = command.split_whitespace().collect();
                    if parts.len() >= 2 {
                        Some(format!("{} {} *", parts[0], parts[1]))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            "write_file" | "patch_file" => {
                // "src/main.rs" -> "src/*"
                let path = PathBuf::from(command);
                path.parent().map(|p| format!("{}/*", p.display()))
            }
            _ => None,
        }
    }

    /// Check if command matches any session approval
    fn matches_session_approval(&self, tool: &str, command: &str) -> bool {
        self.session_approvals
            .get(tool)
            .map(|patterns| patterns.iter().any(|p| Self::matches_pattern(p, command)))
            .unwrap_or(false)
    }

    /// Check if command matches any persistent approval
    fn matches_persistent_approval(&self, tool: &str, command: &str) -> bool {
        self.persistent_approvals
            .iter()
            .filter(|p| p.tool == tool)
            .any(|p| Self::matches_pattern(p, command))
    }

    /// Check if command matches any denial pattern
    fn matches_denial(&self, tool: &str, command: &str) -> bool {
        // Check session denials
        let session_match = self
            .session_denials
            .get(tool)
            .map(|patterns| patterns.iter().any(|p| Self::matches_pattern(p, command)))
            .unwrap_or(false);

        if session_match {
            return true;
        }

        // Check persistent denials
        self.persistent_denials
            .iter()
            .filter(|p| p.tool == tool)
            .any(|p| Self::matches_pattern(p, command))
    }

    /// Check if a command matches a pattern
    fn matches_pattern(pattern: &ApprovalPattern, command: &str) -> bool {
        match pattern.match_type {
            MatchType::Exact => pattern.pattern == command,
            MatchType::Prefix => command.starts_with(&pattern.pattern),
            MatchType::Glob => match Pattern::new(&pattern.pattern) {
                Ok(p) => p.matches(command),
                Err(_) => false,
            },
        }
    }

    /// Add a session-only approval
    fn add_session_approval(&mut self, pattern: ApprovalPattern) {
        self.session_approvals
            .entry(pattern.tool.clone())
            .or_default()
            .push(pattern);
    }

    /// Add a session-only denial
    #[allow(dead_code)]
    fn add_session_denial(&mut self, pattern: ApprovalPattern) {
        self.session_denials
            .entry(pattern.tool.clone())
            .or_default()
            .push(pattern);
    }

    /// Add a persistent approval
    fn add_persistent_approval(&mut self, pattern: ApprovalPattern) -> Result<()> {
        self.persistent_approvals.push(pattern);
        self.save_persistent()
    }

    /// Add a persistent denial
    fn add_persistent_denial(&mut self, pattern: ApprovalPattern) -> Result<()> {
        self.persistent_denials.push(pattern);
        self.save_persistent()
    }

    /// Load persistent approvals from disk
    fn load_persistent(path: &PathBuf) -> (Vec<ApprovalPattern>, Vec<ApprovalPattern>) {
        if !path.exists() {
            return (Vec::new(), Vec::new());
        }

        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<ApprovalStore>(&content) {
                Ok(store) => (store.approvals, store.denials),
                Err(e) => {
                    tracing::warn!("Failed to parse approvals file: {}", e);
                    (Vec::new(), Vec::new())
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read approvals file: {}", e);
                (Vec::new(), Vec::new())
            }
        }
    }

    /// Save persistent approvals to disk
    fn save_persistent(&self) -> Result<()> {
        if self.storage_path.as_os_str().is_empty() {
            return Ok(()); // Memory-only mode
        }

        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let store = ApprovalStore {
            version: 1,
            approvals: self.persistent_approvals.clone(),
            denials: self.persistent_denials.clone(),
        };

        let content = serde_json::to_string_pretty(&store)?;
        std::fs::write(&self.storage_path, content)?;

        Ok(())
    }

    /// Get all session approvals (for display)
    pub fn get_session_approvals(&self) -> &HashMap<String, Vec<ApprovalPattern>> {
        &self.session_approvals
    }

    /// Get all persistent approvals (for display)
    pub fn get_persistent_approvals(&self) -> &[ApprovalPattern] {
        &self.persistent_approvals
    }

    /// Clear all session approvals
    pub fn clear_session(&mut self) {
        self.session_approvals.clear();
        self.session_denials.clear();
    }

    /// Remove a persistent approval by index
    pub fn remove_persistent_approval(&mut self, index: usize) -> Result<()> {
        if index < self.persistent_approvals.len() {
            self.persistent_approvals.remove(index);
            self.save_persistent()?;
        }
        Ok(())
    }

    /// Remove a persistent denial by index
    pub fn remove_persistent_denial(&mut self, index: usize) -> Result<()> {
        if index < self.persistent_denials.len() {
            self.persistent_denials.remove(index);
            self.save_persistent()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pattern_matching_exact() {
        let pattern = ApprovalPattern::new("shell".into(), "git push".into(), MatchType::Exact);
        assert!(ApprovalGate::matches_pattern(&pattern, "git push"));
        assert!(!ApprovalGate::matches_pattern(&pattern, "git push origin"));
    }

    #[test]
    fn test_pattern_matching_prefix() {
        let pattern = ApprovalPattern::new("shell".into(), "git push".into(), MatchType::Prefix);
        assert!(ApprovalGate::matches_pattern(&pattern, "git push"));
        assert!(ApprovalGate::matches_pattern(
            &pattern,
            "git push origin main"
        ));
        assert!(!ApprovalGate::matches_pattern(&pattern, "git pull"));
    }

    #[test]
    fn test_pattern_matching_glob() {
        let pattern = ApprovalPattern::new("shell".into(), "rm *.bak".into(), MatchType::Glob);
        assert!(ApprovalGate::matches_pattern(&pattern, "rm test.bak"));
        assert!(ApprovalGate::matches_pattern(&pattern, "rm src/file.bak"));
        assert!(!ApprovalGate::matches_pattern(&pattern, "rm test.rs"));
    }

    #[test]
    fn test_suggest_prefix() {
        assert_eq!(
            ApprovalGate::suggest_prefix("git push origin main"),
            Some("git push".to_string())
        );
        assert_eq!(
            ApprovalGate::suggest_prefix("npm install"),
            Some("npm install".to_string())
        );
        assert_eq!(ApprovalGate::suggest_prefix("ls"), Some("ls".to_string()));
        assert_eq!(ApprovalGate::suggest_prefix(""), None);
    }

    #[test]
    fn test_suggest_glob() {
        assert_eq!(
            ApprovalGate::suggest_glob("delete_file", "src/temp/file.rs"),
            Some("src/temp/*".to_string())
        );
        assert_eq!(
            ApprovalGate::suggest_glob("shell", "rm test.bak"),
            Some("rm *.bak".to_string())
        );
        assert_eq!(
            ApprovalGate::suggest_glob("shell", "git push origin main"),
            Some("git push *".to_string())
        );
    }

    #[test]
    fn test_storage_path_assignment() {
        let storage_path = std::path::PathBuf::from("/tmp/approvals.json");
        let gate = ApprovalGate::new(storage_path.clone(), None);
        assert_eq!(gate.storage_path(), &storage_path);
    }

    #[test]
    fn test_session_approvals() {
        let mut gate = ApprovalGate::new_memory_only(None);

        // Add a session approval
        gate.add_session_approval(ApprovalPattern::new(
            "shell".into(),
            "npm install".into(),
            MatchType::Prefix,
        ));

        // Check it matches
        assert!(gate.matches_session_approval("shell", "npm install lodash"));
        assert!(!gate.matches_session_approval("shell", "npm run build"));
    }

    #[test]
    fn test_approval_store_serialization() {
        let store = ApprovalStore {
            version: 1,
            approvals: vec![ApprovalPattern::new(
                "shell".into(),
                "git push".into(),
                MatchType::Prefix,
            )],
            denials: vec![ApprovalPattern::new(
                "shell".into(),
                "rm -rf".into(),
                MatchType::Prefix,
            )],
        };

        let json = serde_json::to_string_pretty(&store).unwrap();
        assert!(json.contains("\"version\": 1"));
        assert!(json.contains("git push"));
        assert!(json.contains("rm -rf"));

        let parsed: ApprovalStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.approvals.len(), 1);
        assert_eq!(parsed.denials.len(), 1);
    }

    #[test]
    fn test_trust_level_cycle() {
        let mut gate = ApprovalGate::new_memory_only(None);

        // Default is Careful
        assert_eq!(gate.trust_level, TrustLevel::Careful);

        // Cycle to Manual
        let level = gate.cycle_trust_level();
        assert_eq!(level, TrustLevel::Manual);
        assert_eq!(gate.trust_level, TrustLevel::Manual);

        // Cycle to Balanced
        let level = gate.cycle_trust_level();
        assert_eq!(level, TrustLevel::Balanced);
        assert_eq!(gate.trust_level, TrustLevel::Balanced);

        // Cycle back to Careful
        let level = gate.cycle_trust_level();
        assert_eq!(level, TrustLevel::Careful);
        assert_eq!(gate.trust_level, TrustLevel::Careful);
    }

    #[test]
    fn test_set_trust_level() {
        let mut gate = ApprovalGate::new_memory_only(None);

        gate.set_trust_level(TrustLevel::Manual);
        assert_eq!(gate.trust_level, TrustLevel::Manual);

        gate.set_trust_level(TrustLevel::Careful);
        assert_eq!(gate.trust_level, TrustLevel::Careful);
    }

    #[test]
    fn test_denial_blocks_approval() {
        let mut gate = ApprovalGate::new_memory_only(None);

        // Add a denial pattern
        gate.add_session_denial(ApprovalPattern::new(
            "shell".into(),
            "rm -rf".into(),
            MatchType::Prefix,
        ));

        // Check that it blocks
        assert!(gate.matches_denial("shell", "rm -rf /"));
        assert!(gate.matches_denial("shell", "rm -rf foo"));
        assert!(!gate.matches_denial("shell", "rm foo"));
    }

    #[test]
    fn test_approval_request_suggestions() {
        let gate = ApprovalGate::new_memory_only(None);

        let request =
            gate.create_approval_request("shell", "git push origin main", RiskLevel::Risky);

        assert_eq!(request.tool, "shell");
        assert_eq!(request.command, "git push origin main");
        assert_eq!(request.risk_level, RiskLevel::Risky);

        // Should have suggestions
        assert!(!request.suggested_patterns.is_empty());

        // First suggestion should be exact match
        assert_eq!(request.suggested_patterns[0].match_type, MatchType::Exact);
        assert_eq!(
            request.suggested_patterns[0].pattern,
            "git push origin main"
        );
    }
}
