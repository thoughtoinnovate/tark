use anyhow::{anyhow, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::policy::{
    classifier::CommandClassifier,
    schema,
    security::{PathSanitizer, PatternValidator},
    seed,
    types::*,
};

/// Policy engine for approval decisions
pub struct PolicyEngine {
    conn: Arc<Mutex<Connection>>,
    #[allow(dead_code)]
    working_dir: PathBuf,
    classifier: CommandClassifier,
    path_sanitizer: PathSanitizer,
    pattern_validator: PatternValidator,
}

#[allow(dead_code)]
impl PolicyEngine {
    /// Open or create policy database
    pub fn open(db_path: &Path, working_dir: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(db_path)?;

        // Configure SQLite for better concurrency
        // PRAGMA commands return results, so we use pragma_update or query_row
        conn.pragma_update(None, "journal_mode", "WAL")?; // Enable Write-Ahead Logging
        conn.pragma_update(None, "busy_timeout", 5000)?; // Wait up to 5 seconds on locks
        conn.pragma_update(None, "synchronous", "NORMAL")?; // Balance safety and speed

        // Create tables (includes integrity_metadata)
        schema::create_tables(&conn)?;
        schema::init_schema_version(&conn)?;

        // Seed builtin policy (idempotent)
        let was_seeded = {
            let count: i64 =
                conn.query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))?;
            count > 0
        };

        if !was_seeded {
            seed::seed_builtin(&conn)?;
            tracing::info!("Seeded builtin policy tables");

            // Calculate and store initial hash
            let verifier = crate::policy::integrity::IntegrityVerifier::new(&conn);
            let hash = verifier.calculate_builtin_hash()?;
            verifier.store_hash(&hash)?;
            tracing::debug!("Stored initial integrity hash: {}", &hash[..16]);
        }

        // Verify integrity (auto-repair if tampering detected)
        let verifier = crate::policy::integrity::IntegrityVerifier::new(&conn);
        match verifier.verify_integrity()? {
            crate::policy::integrity::VerificationResult::Valid => {
                tracing::debug!("Policy integrity verified");
            }
            crate::policy::integrity::VerificationResult::Invalid { expected, actual } => {
                tracing::error!(
                    "⚠️  SECURITY WARNING: Tampering detected in policy.db!\n\
                     Expected hash: {}...\n\
                     Actual hash:   {}...\n\
                     Auto-repairing: clearing builtin tables and reseeding from embedded configs.\n\
                     User approval patterns will be preserved.",
                    &expected[..16],
                    &actual[..16]
                );

                // Auto-repair
                verifier.clear_builtin_tables()?;
                seed::seed_builtin(&conn)?;
                let new_hash = verifier.calculate_builtin_hash()?;
                verifier.store_hash(&new_hash)?;

                tracing::info!(
                    "✓ Policy database repaired successfully (new hash: {})",
                    &new_hash[..16]
                );
            }
            crate::policy::integrity::VerificationResult::NoHash => {
                // First run after upgrade, calculate and store hash
                tracing::debug!("No stored hash found, calculating initial hash");
                let hash = verifier.calculate_builtin_hash()?;
                verifier.store_hash(&hash)?;
                tracing::debug!("Stored integrity hash: {}", &hash[..16]);
            }
        }

        // Load user patterns from config files
        let pattern_loader = crate::policy::config::PatternLoader::new(Some(working_dir));
        if let Err(e) = pattern_loader.sync_to_db(&conn, "persistent") {
            tracing::warn!("Failed to load patterns from config: {}", e);
        }

        // Load MCP policies from config files
        let config_loader = crate::policy::config::ConfigLoader::new(Some(working_dir));
        if let Err(e) = config_loader.sync_to_db(&conn) {
            tracing::warn!("Failed to load MCP policies from config: {}", e);
        }

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
            working_dir: working_dir.to_path_buf(),
            classifier: CommandClassifier::new(working_dir.to_path_buf()),
            path_sanitizer: PathSanitizer::new(working_dir.to_path_buf()),
            pattern_validator: PatternValidator::new(working_dir.to_path_buf()),
        })
    }

    /// Classify a shell command
    pub fn classify_command(&self, command: &str) -> Result<CommandClassification> {
        // Check for compound commands
        if command.contains("&&") || command.contains("||") || command.contains(';') {
            Ok(self.classifier.classify_compound(command))
        } else {
            Ok(self.classifier.classify(command))
        }
    }

    /// Check if tool execution needs approval
    pub fn check_approval(
        &self,
        tool_id: &str,
        command: &str,
        mode: &str,
        trust: &str,
        session_id: &str,
    ) -> Result<ApprovalDecision> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Step 1: Classify command
        let classification = if tool_id == "shell" {
            self.classify_command(command)?
        } else {
            // For non-shell tools, use static classification
            self.get_static_classification(&conn, tool_id)?
        };

        // Step 2: Check if mode has approval gate
        let has_gate: bool = conn.query_row(
            "SELECT has_approval_gate FROM agent_modes WHERE id = ?1",
            [mode],
            |row| row.get(0),
        )?;

        if !has_gate {
            // No approval needed for Ask/Plan modes
            return Ok(ApprovalDecision {
                needs_approval: false,
                allow_save_pattern: false,
                classification: classification.clone(),
                matched_pattern: None,
                rationale: format!("{} mode does not require approval", mode),
            });
        }

        // Step 3: Lookup approval rule
        let rule_result: Result<(bool, bool, Option<String>), rusqlite::Error> = conn.query_row(
            "SELECT needs_approval, allow_save_pattern, rationale
             FROM approval_rules
             WHERE classification_id = ?1
               AND mode_id = ?2
               AND trust_id = ?3
               AND in_workdir = ?4",
            (
                &classification.classification_id,
                mode,
                trust,
                classification.in_workdir,
            ),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        );

        // Handle unknown tools gracefully - auto-approve read-only tools
        let (needs_approval, allow_save_pattern, rationale) = match rule_result {
            Ok(rule) => rule,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Unknown tool - use smart defaults based on operation
                let is_read_only = matches!(classification.operation, Operation::Read);
                tracing::debug!(
                    "No policy rule for tool '{}' (classification: {}), using defaults: needs_approval={}",
                    tool_id,
                    classification.classification_id,
                    !is_read_only
                );
                (
                    !is_read_only, // Read-only tools auto-approved, others need approval
                    true,          // Allow saving patterns
                    Some(format!(
                        "Unknown tool '{}' - using default policy (read-only: {})",
                        tool_id, is_read_only
                    )),
                )
            }
            Err(e) => return Err(anyhow!("Policy rule lookup failed: {}", e)),
        };

        // Step 4: If needs approval, check for saved patterns
        let matched_pattern = if needs_approval && allow_save_pattern {
            self.check_patterns(&conn, tool_id, command, session_id)?
        } else {
            None
        };

        Ok(ApprovalDecision {
            needs_approval: needs_approval && matched_pattern.is_none(),
            allow_save_pattern,
            classification,
            matched_pattern,
            rationale: rationale.unwrap_or_else(|| "Based on internal policy".to_string()),
        })
    }

    /// Check for matching approval/denial patterns
    fn check_patterns(
        &self,
        conn: &Connection,
        tool_id: &str,
        command: &str,
        session_id: &str,
    ) -> Result<Option<PatternMatch>> {
        // Note: For internal tools, patterns are stored in approval_patterns table
        // For MCP tools, patterns are in mcp_approval_patterns table
        // This implementation focuses on internal tools first

        // Check denial patterns first (take precedence)
        let denial_query = r#"
            SELECT id, pattern, match_type
            FROM approval_patterns
            WHERE tool_type_id = ?1
              AND is_denial = 1
              AND (is_persistent = 1 OR session_id = ?2)
        "#;

        let mut stmt = conn.prepare(denial_query)?;
        let patterns = stmt
            .query_map([tool_id, session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if let Some((id, pattern, match_type)) = patterns.into_iter().next() {
            if self.matches_pattern(command, &pattern, &match_type) {
                return Ok(Some(PatternMatch {
                    pattern_id: id,
                    pattern,
                    match_type: self.parse_match_type(&match_type)?,
                    is_denial: true,
                }));
            }
        }

        // Check approval patterns
        let approval_query = r#"
            SELECT id, pattern, match_type
            FROM approval_patterns
            WHERE tool_type_id = ?1
              AND is_denial = 0
              AND (is_persistent = 1 OR session_id = ?2)
        "#;

        let mut stmt = conn.prepare(approval_query)?;
        let patterns = stmt
            .query_map([tool_id, session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if let Some((id, pattern, match_type)) = patterns.into_iter().next() {
            if self.matches_pattern(command, &pattern, &match_type) {
                return Ok(Some(PatternMatch {
                    pattern_id: id,
                    pattern,
                    match_type: self.parse_match_type(&match_type)?,
                    is_denial: false,
                }));
            }
        }

        Ok(None)
    }

    /// Check if command matches pattern
    fn matches_pattern(&self, command: &str, pattern: &str, match_type: &str) -> bool {
        match match_type {
            "exact" => command == pattern,
            "prefix" => command.starts_with(pattern),
            "glob" => {
                // Use glob matching
                if let Ok(glob) = glob::Pattern::new(pattern) {
                    glob.matches(command)
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn parse_match_type(&self, match_type: &str) -> Result<MatchType> {
        match match_type {
            "exact" => Ok(MatchType::Exact),
            "prefix" => Ok(MatchType::Prefix),
            "glob" => Ok(MatchType::Glob),
            _ => Err(anyhow!("Invalid match type: {}", match_type)),
        }
    }

    /// Get static classification for non-shell tools
    fn get_static_classification(
        &self,
        conn: &Connection,
        tool_id: &str,
    ) -> Result<CommandClassification> {
        let (classification_id, operation, base_risk): (String, String, String) = conn
            .query_row(
                "SELECT id, operation, base_risk
                 FROM tool_classifications
                 WHERE tool_type_id = ?1
                 LIMIT 1",
                [tool_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or_else(|_| {
                // Fallback for tools without classifications
                (
                    format!("{}-default", tool_id),
                    "execute".to_string(),
                    "moderate".to_string(),
                )
            });

        Ok(CommandClassification {
            classification_id,
            operation: self.parse_operation(&operation)?,
            in_workdir: true, // Non-shell tools assume workdir
            risk_level: self.parse_risk_level(&base_risk)?,
        })
    }

    fn parse_operation(&self, op: &str) -> Result<Operation> {
        match op {
            "read" => Ok(Operation::Read),
            "write" => Ok(Operation::Write),
            "delete" => Ok(Operation::Delete),
            "execute" => Ok(Operation::Execute),
            _ => Err(anyhow!("Invalid operation: {}", op)),
        }
    }

    fn parse_risk_level(&self, risk: &str) -> Result<RiskLevel> {
        match risk {
            "safe" => Ok(RiskLevel::Safe),
            "moderate" => Ok(RiskLevel::Moderate),
            "dangerous" => Ok(RiskLevel::Dangerous),
            _ => Err(anyhow!("Invalid risk level: {}", risk)),
        }
    }

    /// Save approval pattern
    pub fn save_pattern(&self, pattern: ApprovalPattern) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Validate pattern
        self.pattern_validator.validate(
            &pattern.tool,
            &pattern.pattern,
            &pattern.match_type.to_string(),
        )?;

        // Insert pattern
        conn.execute(
            "INSERT INTO approval_patterns (tool_type_id, pattern, match_type, is_denial, is_persistent, session_id, created_at, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7)",
            (
                &pattern.tool,
                &pattern.pattern,
                &pattern.match_type.to_string(),
                pattern.is_denial,
                matches!(pattern.source, PatternSource::User | PatternSource::Workspace),
                if matches!(pattern.source, PatternSource::Session) { Some("session") } else { None },
                &pattern.description,
            ),
        )?;

        Ok(())
    }

    /// Log approval decision for audit
    pub fn log_decision(&self, entry: AuditEntry) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        conn.execute(
            "INSERT INTO approval_audit_log (timestamp, tool_id, command, classification_id, mode_id, trust_id, decision, matched_pattern_id, session_id, working_directory)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            (
                &entry.timestamp,
                &entry.tool_id,
                &entry.command,
                &entry.classification_id,
                &entry.mode_id,
                &entry.trust_id,
                &entry.decision.to_string(),
                entry.matched_pattern_id,
                &entry.session_id,
                &entry.working_directory,
            ),
        )?;

        Ok(())
    }

    /// Get available tools for a mode
    pub fn get_available_tools(&self, mode: &str) -> Result<Vec<ToolInfo>> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.category_id, t.permissions, t.base_risk
             FROM tool_types t
             JOIN tool_mode_availability a ON t.id = a.tool_type_id
             WHERE a.mode_id = ?1 AND a.is_available = 1",
        )?;

        let tools = stmt
            .query_map([mode], |row| {
                let risk_str: String = row.get(4)?;
                Ok(ToolInfo {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    category: row.get(2)?,
                    permissions: row.get(3)?,
                    base_risk: match risk_str.as_str() {
                        "safe" => RiskLevel::Safe,
                        "moderate" => RiskLevel::Moderate,
                        "dangerous" => RiskLevel::Dangerous,
                        _ => RiskLevel::Moderate,
                    },
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(tools)
    }

    /// Check if path is within working directory
    pub fn is_in_workdir(&self, path: &Path) -> bool {
        self.path_sanitizer
            .is_in_workdir(path.to_str().unwrap_or(""))
            .unwrap_or(false)
    }

    /// List session approval patterns (approvals and denials)
    /// Returns (approvals, denials) tuples
    pub fn list_session_patterns(
        &self,
        session_id: &str,
    ) -> Result<(Vec<ApprovalPatternEntry>, Vec<ApprovalPatternEntry>)> {
        // Load from policy.db
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Query approval patterns from policy.db (is_denial = 0)
        // Include both persistent patterns (is_persistent = 1) and session patterns (is_persistent = 0)
        let approval_query = r#"
            SELECT id, tool_type_id, pattern, match_type, description, is_persistent
            FROM approval_patterns
            WHERE is_denial = 0
              AND (is_persistent = 1 OR (is_persistent = 0 AND session_id = ?1))
            ORDER BY is_persistent DESC, created_at DESC
        "#;

        let mut stmt = conn.prepare(approval_query)?;
        let approval_rows: Vec<_> = stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);

        let approvals: Vec<ApprovalPatternEntry> = approval_rows
            .into_iter()
            .map(
                |(id, tool, pattern, match_type, description, is_persistent)| {
                    let desc = if is_persistent {
                        Some(format!(
                            "Persistent{}",
                            description
                                .as_ref()
                                .map(|d| format!(" · {}", d))
                                .unwrap_or_default()
                        ))
                    } else {
                        Some(format!(
                            "Session-only{}",
                            description
                                .as_ref()
                                .map(|d| format!(" · {}", d))
                                .unwrap_or_default()
                        ))
                    };
                    ApprovalPatternEntry {
                        id,
                        tool,
                        pattern,
                        match_type,
                        is_denial: false,
                        description: desc,
                    }
                },
            )
            .collect();

        // Query denial patterns from policy.db (is_denial = 1)
        // Include both persistent patterns (is_persistent = 1) and session patterns (is_persistent = 0)
        let denial_query = r#"
            SELECT id, tool_type_id, pattern, match_type, description, is_persistent
            FROM approval_patterns
            WHERE is_denial = 1
              AND (is_persistent = 1 OR (is_persistent = 0 AND session_id = ?1))
            ORDER BY is_persistent DESC, created_at DESC
        "#;

        let mut stmt = conn.prepare(denial_query)?;
        let denial_rows: Vec<_> = stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);

        let denials: Vec<ApprovalPatternEntry> = denial_rows
            .into_iter()
            .map(
                |(id, tool, pattern, match_type, description, is_persistent)| {
                    let desc = if is_persistent {
                        Some(format!(
                            "Persistent{}",
                            description
                                .as_ref()
                                .map(|d| format!(" · {}", d))
                                .unwrap_or_default()
                        ))
                    } else {
                        Some(format!(
                            "Session-only{}",
                            description
                                .as_ref()
                                .map(|d| format!(" · {}", d))
                                .unwrap_or_default()
                        ))
                    };
                    ApprovalPatternEntry {
                        id,
                        tool,
                        pattern,
                        match_type,
                        is_denial: true,
                        description: desc,
                    }
                },
            )
            .collect();

        Ok((approvals, denials))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_policy_engine_open() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("policy.db");
        let work_dir = temp_dir.path().to_path_buf();

        let engine = PolicyEngine::open(&db_path, &work_dir).unwrap();

        // Verify builtin data is seeded
        let conn = engine.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_classify_shell_command() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("policy.db");
        let work_dir = temp_dir.path().to_path_buf();

        let engine = PolicyEngine::open(&db_path, &work_dir).unwrap();

        // Classify read command
        let classification = engine.classify_command("cat file.txt").unwrap();
        assert_eq!(classification.classification_id, "shell-read");
        assert_eq!(classification.operation, Operation::Read);
        assert!(classification.in_workdir);

        // Classify write command
        let classification = engine.classify_command("echo x > file.txt").unwrap();
        assert_eq!(classification.classification_id, "shell-write");
        assert_eq!(classification.operation, Operation::Write);

        // Classify delete command
        let classification = engine.classify_command("rm file.txt").unwrap();
        assert_eq!(classification.classification_id, "shell-rm");
        assert_eq!(classification.operation, Operation::Delete);
    }

    #[test]
    fn test_check_approval_ask_mode() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("policy.db");
        let work_dir = temp_dir.path().to_path_buf();

        let engine = PolicyEngine::open(&db_path, &work_dir).unwrap();

        // Ask mode should not require approval (no approval gate)
        let decision = engine
            .check_approval("shell", "rm -rf /tmp", "ask", "balanced", "session1")
            .unwrap();

        assert!(!decision.needs_approval);
    }

    #[test]
    fn test_check_approval_build_mode() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("policy.db");
        let work_dir = temp_dir.path().to_path_buf();

        let engine = PolicyEngine::open(&db_path, &work_dir).unwrap();

        // Build mode + balanced + read in workdir = no approval
        let decision = engine
            .check_approval("shell", "cat file.txt", "build", "balanced", "session1")
            .unwrap();
        assert!(!decision.needs_approval);

        // Build mode + balanced + rm outside workdir = needs approval
        let decision = engine
            .check_approval("shell", "rm /tmp/file", "build", "balanced", "session1")
            .unwrap();
        assert!(decision.needs_approval);
        assert!(decision.allow_save_pattern); // Can save in balanced

        // Build mode + careful + rm outside workdir = ALWAYS (cannot save)
        let decision = engine
            .check_approval("shell", "rm /tmp/file", "build", "careful", "session1")
            .unwrap();
        assert!(decision.needs_approval);
        assert!(!decision.allow_save_pattern); // ALWAYS - cannot save
    }
}
