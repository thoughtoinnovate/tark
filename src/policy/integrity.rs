//! Integrity verification for policy database
//!
//! Detects tampering with builtin policy tables and provides auto-recovery.

use anyhow::Result;
use rusqlite::Connection;
use sha2::{Digest, Sha256};

/// Result of integrity verification
#[derive(Debug, Clone, PartialEq)]
pub enum VerificationResult {
    /// Hash matches - no tampering detected
    Valid,
    /// Tampering detected - hash mismatch
    Invalid { expected: String, actual: String },
    /// No stored hash found (first run or missing)
    NoHash,
}

/// Integrity verifier for policy database
pub struct IntegrityVerifier<'a> {
    conn: &'a Connection,
}

impl<'a> IntegrityVerifier<'a> {
    /// Create new verifier
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Get list of builtin tables to hash
    fn builtin_tables() -> &'static [&'static str] {
        &[
            "agent_modes",
            "trust_levels",
            "tool_types",
            "tool_categories",
            "tool_classifications",
            "approval_rules",
            "tool_mode_availability",
            "compound_command_rules",
        ]
    }

    /// Calculate hash of all builtin tables
    pub fn calculate_builtin_hash(&self) -> Result<String> {
        let mut hasher = Sha256::new();

        for table in Self::builtin_tables() {
            // Get primary key column(s) for deterministic ordering
            let pk_query = format!(
                "SELECT name FROM pragma_table_info('{}') WHERE pk > 0 ORDER BY pk",
                table
            );
            let mut pk_stmt = self.conn.prepare(&pk_query)?;
            let pk_columns: Vec<String> = pk_stmt
                .query_map([], |row| row.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            drop(pk_stmt);

            let order_by = if pk_columns.is_empty() {
                "rowid".to_string()
            } else {
                pk_columns.join(", ")
            };

            // Query all rows in deterministic order
            let query = format!("SELECT * FROM {} ORDER BY {}", table, order_by);
            let mut stmt = self.conn.prepare(&query)?;
            let column_count = stmt.column_count();

            let rows = stmt.query_map([], |row| {
                let mut values = Vec::new();
                for i in 0..column_count {
                    // Get value as string representation
                    let value: String = match row.get_ref(i)? {
                        rusqlite::types::ValueRef::Null => "NULL".to_string(),
                        rusqlite::types::ValueRef::Integer(i) => i.to_string(),
                        rusqlite::types::ValueRef::Real(f) => f.to_string(),
                        rusqlite::types::ValueRef::Text(s) => {
                            String::from_utf8_lossy(s).to_string()
                        }
                        rusqlite::types::ValueRef::Blob(b) => {
                            format!("BLOB:{}", hex::encode(b))
                        }
                    };
                    values.push(value);
                }
                Ok(values.join("|"))
            })?;

            // Hash each row
            for row_result in rows {
                let row_str = row_result?;
                hasher.update(table.as_bytes());
                hasher.update(b":");
                hasher.update(row_str.as_bytes());
                hasher.update(b"\n");
            }
        }

        let hash = hasher.finalize();
        Ok(hex::encode(hash))
    }

    /// Store hash in integrity_metadata table
    pub fn store_hash(&self, hash: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO integrity_metadata (key, value, updated_at) VALUES ('builtin_hash', ?1, datetime('now'))",
            [hash],
        )?;
        Ok(())
    }

    /// Get stored hash from database
    pub fn get_stored_hash(&self) -> Result<Option<String>> {
        let result: Result<String, rusqlite::Error> = self.conn.query_row(
            "SELECT value FROM integrity_metadata WHERE key = 'builtin_hash'",
            [],
            |row| row.get(0),
        );

        match result {
            Ok(hash) => Ok(Some(hash)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Verify current hash matches stored hash
    pub fn verify_integrity(&self) -> Result<VerificationResult> {
        let stored_hash = self.get_stored_hash()?;

        match stored_hash {
            None => Ok(VerificationResult::NoHash),
            Some(expected) => {
                let actual = self.calculate_builtin_hash()?;
                if expected == actual {
                    Ok(VerificationResult::Valid)
                } else {
                    Ok(VerificationResult::Invalid { expected, actual })
                }
            }
        }
    }

    /// Clear builtin tables (preserves user data)
    pub fn clear_builtin_tables(&self) -> Result<()> {
        // Delete from builtin tables in reverse dependency order
        // Note: We don't use CASCADE because we want explicit control
        // Temporarily disable foreign keys and drop protection triggers
        self.conn.execute("PRAGMA foreign_keys = OFF", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_availability_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_availability_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_rules_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_rules_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_classifications_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_classifications_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_tools_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_tools_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_trust_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_trust_update", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_modes_delete", [])?;
        self.conn
            .execute("DROP TRIGGER IF EXISTS protect_modes_update", [])?;

        // Start with tables that have no dependencies
        self.conn
            .execute("DELETE FROM compound_command_rules", [])?;
        self.conn
            .execute("DELETE FROM tool_mode_availability", [])?;
        self.conn.execute("DELETE FROM approval_rules", [])?;

        // Then classifications (depends on tool_types)
        self.conn.execute("DELETE FROM tool_classifications", [])?;
        self.conn.execute("DELETE FROM classification_config", [])?;
        self.conn.execute("DELETE FROM pattern_validators", [])?;

        // Then tool types (depends on categories)
        self.conn.execute("DELETE FROM tool_types", [])?;

        // Then categories and other top-level tables
        self.conn.execute("DELETE FROM tool_categories", [])?;
        self.conn.execute("DELETE FROM trust_levels", [])?;
        self.conn.execute("DELETE FROM agent_modes", [])?;

        // Re-enable foreign keys and recreate protection triggers
        self.conn.execute("PRAGMA foreign_keys = ON", [])?;
        crate::policy::schema::create_protection_triggers(self.conn)?;

        tracing::debug!("Cleared all builtin tables");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::{schema, seed};

    #[test]
    fn test_hash_calculation_deterministic() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        seed::seed_builtin(&conn).unwrap();

        let verifier = IntegrityVerifier::new(&conn);

        // Calculate hash twice - should be identical
        let hash1 = verifier.calculate_builtin_hash().unwrap();
        let hash2 = verifier.calculate_builtin_hash().unwrap();

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64); // SHA-256 produces 64 hex chars
    }

    #[test]
    fn test_tampering_detection() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        seed::seed_builtin(&conn).unwrap();

        let verifier = IntegrityVerifier::new(&conn);

        // Calculate and store initial hash
        let hash = verifier.calculate_builtin_hash().unwrap();
        verifier.store_hash(&hash).unwrap();

        // Verify - should pass
        let result = verifier.verify_integrity().unwrap();
        assert_eq!(result, VerificationResult::Valid);

        // Tamper with a builtin table (bypass triggers by dropping them first)
        conn.execute("DROP TRIGGER IF EXISTS protect_modes_update", [])
            .unwrap();
        conn.execute(
            "UPDATE agent_modes SET name = 'Hacked' WHERE id = 'ask'",
            [],
        )
        .unwrap();

        // Verify - should detect tampering
        let result = verifier.verify_integrity().unwrap();
        match result {
            VerificationResult::Invalid { .. } => {
                // Expected
            }
            _ => panic!("Expected Invalid, got {:?}", result),
        }
    }

    #[test]
    fn test_clear_builtin_preserves_user_data() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        seed::seed_builtin(&conn).unwrap();

        // Add some user approval patterns
        conn.execute(
            "INSERT INTO approval_patterns (tool_type_id, pattern, match_type, is_denial, is_persistent, session_id, created_at)
             VALUES ('shell', 'test*', 'glob', 0, 0, 'test-session', datetime('now'))",
            [],
        )
        .unwrap();

        let verifier = IntegrityVerifier::new(&conn);

        // Clear builtin tables
        verifier.clear_builtin_tables().unwrap();

        // Verify builtin tables are empty
        let mode_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode_count, 0);

        // Verify user data is preserved
        let pattern_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM approval_patterns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pattern_count, 1);
    }

    #[test]
    fn test_no_hash_on_first_run() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();

        let verifier = IntegrityVerifier::new(&conn);

        // No hash stored yet
        let result = verifier.verify_integrity().unwrap();
        assert_eq!(result, VerificationResult::NoHash);
    }
}
