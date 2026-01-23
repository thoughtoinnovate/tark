use anyhow::Result;
use rusqlite::Connection;

pub const SCHEMA_VERSION: u32 = 1;

/// Create all policy engine tables
pub fn create_tables(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- 1. Schema versioning
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER PRIMARY KEY,
            applied_at TEXT NOT NULL,
            description TEXT
        );

        -- 2. Agent modes (Ask, Plan, Build)
        CREATE TABLE IF NOT EXISTS agent_modes (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            icon TEXT,
            description TEXT,
            has_approval_gate BOOLEAN DEFAULT 0,
            display_order INTEGER DEFAULT 0
        );

        -- 3. Trust levels (Balanced, Careful, Manual)
        CREATE TABLE IF NOT EXISTS trust_levels (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            icon TEXT,
            description TEXT,
            applies_to_mode TEXT REFERENCES agent_modes(id),
            display_order INTEGER
        );

        -- 4. Tool categories
        CREATE TABLE IF NOT EXISTS tool_categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            description TEXT
        );

        -- 5. Tool types
        CREATE TABLE IF NOT EXISTS tool_types (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            category_id TEXT REFERENCES tool_categories(id),
            permissions TEXT CHECK(permissions IN ('R', 'W', 'X', 'RW', 'RX', 'WX', 'RWX')),
            base_risk TEXT CHECK(base_risk IN ('safe', 'moderate', 'dangerous')),
            description TEXT
        );

        -- 6. Tool classifications (simplified: operation type only)
        CREATE TABLE IF NOT EXISTS tool_classifications (
            id TEXT PRIMARY KEY,
            tool_type_id TEXT NOT NULL REFERENCES tool_types(id),
            name TEXT NOT NULL,
            operation TEXT CHECK(operation IN ('read', 'write', 'delete', 'execute')),
            base_risk TEXT CHECK(base_risk IN ('safe', 'moderate', 'dangerous')),
            description TEXT
        );

        -- 7. Approval rules (Classification x Mode x Trust x Location)
        CREATE TABLE IF NOT EXISTS approval_rules (
            classification_id TEXT NOT NULL REFERENCES tool_classifications(id),
            mode_id TEXT NOT NULL REFERENCES agent_modes(id),
            trust_id TEXT NOT NULL REFERENCES trust_levels(id),
            in_workdir BOOLEAN NOT NULL,
            needs_approval BOOLEAN NOT NULL,
            allow_save_pattern BOOLEAN DEFAULT 1,
            rationale TEXT,
            PRIMARY KEY (classification_id, mode_id, trust_id, in_workdir)
        );

        -- 8. Tool mode availability
        CREATE TABLE IF NOT EXISTS tool_mode_availability (
            tool_type_id TEXT NOT NULL REFERENCES tool_types(id),
            mode_id TEXT NOT NULL REFERENCES agent_modes(id),
            is_available BOOLEAN NOT NULL,
            alternative_tool_id TEXT REFERENCES tool_types(id),
            PRIMARY KEY (tool_type_id, mode_id)
        );

        -- 9. Compound command rules
        CREATE TABLE IF NOT EXISTS compound_command_rules (
            separator TEXT PRIMARY KEY,
            strategy TEXT CHECK(strategy IN ('all', 'highest_risk', 'first')),
            description TEXT
        );

        -- 10. Pattern validators (security constraints)
        CREATE TABLE IF NOT EXISTS pattern_validators (
            tool_type_id TEXT PRIMARY KEY REFERENCES tool_types(id),
            max_length INTEGER,
            forbidden_patterns TEXT,
            require_workdir_prefix BOOLEAN DEFAULT 0
        );

        -- 11. Classification config
        CREATE TABLE IF NOT EXISTS classification_config (
            tool_type_id TEXT PRIMARY KEY REFERENCES tool_types(id),
            strategy TEXT CHECK(strategy IN ('operation_based', 'static')),
            default_classification_id TEXT REFERENCES tool_classifications(id),
            config_json TEXT
        );

        -- 12. Internal tool approval patterns (when allow_save_pattern=true)
        CREATE TABLE IF NOT EXISTS approval_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tool_type_id TEXT NOT NULL REFERENCES tool_types(id),
            pattern TEXT NOT NULL,
            match_type TEXT CHECK(match_type IN ('exact', 'prefix', 'glob')),
            is_denial BOOLEAN DEFAULT 0,
            is_persistent BOOLEAN DEFAULT 1,
            session_id TEXT,
            created_at TEXT NOT NULL,
            description TEXT
        );

        -- 13. MCP tool policies (user-defined)
        CREATE TABLE IF NOT EXISTS mcp_tool_policies (
            server_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            risk_level TEXT CHECK(risk_level IN ('safe', 'moderate', 'dangerous')),
            needs_approval BOOLEAN NOT NULL,
            allow_save_pattern BOOLEAN DEFAULT 1,
            description TEXT,
            source TEXT NOT NULL CHECK(source IN ('user', 'workspace')),
            created_at TEXT NOT NULL,
            PRIMARY KEY (server_id, tool_name)
        );

        -- 13. MCP approval patterns (user-defined)
        CREATE TABLE IF NOT EXISTS mcp_approval_patterns (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            server_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            pattern TEXT NOT NULL,
            match_type TEXT CHECK(match_type IN ('exact', 'prefix', 'glob')),
            is_denial BOOLEAN DEFAULT 0,
            source TEXT NOT NULL CHECK(source IN ('user', 'workspace', 'session')),
            created_at TEXT NOT NULL,
            description TEXT
        );

        -- 14. Audit log
        CREATE TABLE IF NOT EXISTS approval_audit_log (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            tool_id TEXT NOT NULL,
            command TEXT NOT NULL,
            classification_id TEXT,
            mode_id TEXT NOT NULL,
            trust_id TEXT,
            decision TEXT CHECK(decision IN (
                'auto_approved', 'user_approved', 'user_denied',
                'pattern_matched', 'pattern_denied', 'blocked'
            )),
            matched_pattern_id INTEGER,
            session_id TEXT,
            working_directory TEXT
        );

        -- 15. Integrity metadata (for tamper detection)
        CREATE TABLE IF NOT EXISTS integrity_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        -- Indexes
        CREATE INDEX IF NOT EXISTS idx_classifications_tool ON tool_classifications(tool_type_id);
        CREATE INDEX IF NOT EXISTS idx_classifications_priority ON tool_classifications(tool_type_id, base_risk);
        CREATE INDEX IF NOT EXISTS idx_patterns_tool ON approval_patterns(tool_type_id, is_denial, is_persistent);
        CREATE INDEX IF NOT EXISTS idx_patterns_session ON approval_patterns(session_id) WHERE session_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_mcp_patterns_lookup ON mcp_approval_patterns(server_id, tool_name, is_denial);
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON approval_audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_session ON approval_audit_log(session_id);
        "#,
    )?;

    // Create protection triggers for builtin tables
    create_protection_triggers(conn)?;

    Ok(())
}

/// Create triggers to protect builtin tables from modification
fn create_protection_triggers(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        r#"
        -- Protect agent_modes
        CREATE TRIGGER IF NOT EXISTS protect_modes_update
        BEFORE UPDATE ON agent_modes
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin modes');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_modes_delete
        BEFORE DELETE ON agent_modes
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin modes');
        END;

        -- Protect trust_levels
        CREATE TRIGGER IF NOT EXISTS protect_trust_update
        BEFORE UPDATE ON trust_levels
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin trust levels');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_trust_delete
        BEFORE DELETE ON trust_levels
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin trust levels');
        END;

        -- Protect tool_types
        CREATE TRIGGER IF NOT EXISTS protect_tools_update
        BEFORE UPDATE ON tool_types
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin tools');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_tools_delete
        BEFORE DELETE ON tool_types
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin tools');
        END;

        -- Protect tool_classifications
        CREATE TRIGGER IF NOT EXISTS protect_classifications_update
        BEFORE UPDATE ON tool_classifications
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin classifications');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_classifications_delete
        BEFORE DELETE ON tool_classifications
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin classifications');
        END;

        -- Protect approval_rules
        CREATE TRIGGER IF NOT EXISTS protect_rules_update
        BEFORE UPDATE ON approval_rules
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin approval rules');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_rules_delete
        BEFORE DELETE ON approval_rules
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin approval rules');
        END;

        -- Protect tool_mode_availability
        CREATE TRIGGER IF NOT EXISTS protect_availability_update
        BEFORE UPDATE ON tool_mode_availability
        BEGIN
            SELECT RAISE(ABORT, 'Cannot modify builtin tool availability');
        END;

        CREATE TRIGGER IF NOT EXISTS protect_availability_delete
        BEFORE DELETE ON tool_mode_availability
        BEGIN
            SELECT RAISE(ABORT, 'Cannot delete builtin tool availability');
        END;
        "#,
    )?;

    Ok(())
}

/// Initialize schema version
pub fn init_schema_version(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO schema_version (version, applied_at, description) VALUES (?1, datetime('now'), ?2)",
        (SCHEMA_VERSION, "Initial policy engine schema"),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_tables() {
        let conn = Connection::open_in_memory().unwrap();
        create_tables(&conn).unwrap();
        init_schema_version(&conn).unwrap();

        // Verify all tables exist
        let tables: Vec<String> = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        assert!(tables.contains(&"schema_version".to_string()));
        assert!(tables.contains(&"agent_modes".to_string()));
        assert!(tables.contains(&"trust_levels".to_string()));
        assert!(tables.contains(&"approval_rules".to_string()));
    }

    #[test]
    fn test_protection_triggers() {
        let conn = Connection::open_in_memory().unwrap();
        create_tables(&conn).unwrap();

        // Insert a test mode
        conn.execute(
            "INSERT INTO agent_modes (id, name, description) VALUES ('test', 'Test', 'Test mode')",
            [],
        )
        .unwrap();

        // Try to update (should fail)
        let result = conn.execute(
            "UPDATE agent_modes SET name = 'Modified' WHERE id = 'test'",
            [],
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot modify"));

        // Try to delete (should fail)
        let result = conn.execute("DELETE FROM agent_modes WHERE id = 'test'", []);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot delete"));
    }
}
