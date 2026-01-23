use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;

/// Builtin policy embedded at compile time
const BUILTIN_POLICY: &str = include_str!("builtin_policy.toml");

#[derive(Debug, Deserialize)]
struct BuiltinPolicy {
    #[allow(dead_code)]
    version: u32,
    modes: Vec<Mode>,
    trust_levels: Vec<TrustLevel>,
    categories: Vec<Category>,
    tools: Vec<Tool>,
    tool_availability: Vec<ToolAvailability>,
    classifications: Vec<Classification>,
    rules: Vec<ApprovalRule>,
    compound_rules: Vec<CompoundRule>,
}

#[derive(Debug, Deserialize)]
struct Mode {
    id: String,
    name: String,
    icon: Option<String>,
    description: Option<String>,
    has_approval_gate: bool,
    display_order: u32,
}

#[derive(Debug, Deserialize)]
struct TrustLevel {
    id: String,
    name: String,
    icon: Option<String>,
    description: Option<String>,
    applies_to_mode: String,
    display_order: u32,
}

#[derive(Debug, Deserialize)]
struct Category {
    id: String,
    name: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Tool {
    id: String,
    name: String,
    category: String,
    permissions: String,
    base_risk: String,
}

#[derive(Debug, Deserialize)]
struct ToolAvailability {
    tool: String,
    mode: String,
    available: bool,
    alternative: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Classification {
    id: String,
    tool: String,
    name: String,
    operation: String,
    base_risk: String,
    description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApprovalRule {
    classification: String,
    mode: String,
    trust: String,
    in_workdir: bool,
    needs_approval: bool,
    allow_save_pattern: Option<bool>,
    rationale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CompoundRule {
    separator: String,
    strategy: String,
    description: Option<String>,
}

/// Seed the database with builtin policy
pub fn seed_builtin(conn: &Connection) -> Result<()> {
    // Use transaction for atomicity
    conn.execute("BEGIN IMMEDIATE", [])?;

    // Check if already seeded
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))?;

    if count > 0 {
        tracing::info!("Database already seeded, skipping");
        conn.execute("ROLLBACK", [])?;
        return Ok(());
    }

    tracing::info!("Seeding builtin policy from embedded TOML");

    // Parse builtin policy
    let policy: BuiltinPolicy = toml::from_str(BUILTIN_POLICY)?;

    // Insert modes
    for mode in &policy.modes {
        conn.execute(
            "INSERT INTO agent_modes (id, name, icon, description, has_approval_gate, display_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &mode.id,
                &mode.name,
                &mode.icon,
                &mode.description,
                mode.has_approval_gate,
                mode.display_order,
            ),
        )?;
    }
    tracing::debug!("Inserted {} agent modes", policy.modes.len());

    // Insert trust levels
    for trust in &policy.trust_levels {
        conn.execute(
            "INSERT INTO trust_levels (id, name, icon, description, applies_to_mode, display_order)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &trust.id,
                &trust.name,
                &trust.icon,
                &trust.description,
                &trust.applies_to_mode,
                trust.display_order,
            ),
        )?;
    }
    tracing::debug!("Inserted {} trust levels", policy.trust_levels.len());

    // Insert categories
    for category in &policy.categories {
        conn.execute(
            "INSERT INTO tool_categories (id, name, description) VALUES (?1, ?2, ?3)",
            (&category.id, &category.name, &category.description),
        )?;
    }
    tracing::debug!("Inserted {} tool categories", policy.categories.len());

    // Insert tools
    for tool in &policy.tools {
        conn.execute(
            "INSERT INTO tool_types (id, name, category_id, permissions, base_risk)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &tool.id,
                &tool.name,
                &tool.category,
                &tool.permissions,
                &tool.base_risk,
            ),
        )?;
    }
    tracing::debug!("Inserted {} tool types", policy.tools.len());

    // Insert tool availability
    for avail in &policy.tool_availability {
        conn.execute(
            "INSERT INTO tool_mode_availability (tool_type_id, mode_id, is_available, alternative_tool_id)
             VALUES (?1, ?2, ?3, ?4)",
            (
                &avail.tool,
                &avail.mode,
                avail.available,
                &avail.alternative,
            ),
        )?;
    }
    tracing::debug!(
        "Inserted {} tool availability rules",
        policy.tool_availability.len()
    );

    // Insert classifications
    for classification in &policy.classifications {
        conn.execute(
            "INSERT INTO tool_classifications (id, tool_type_id, name, operation, base_risk, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            (
                &classification.id,
                &classification.tool,
                &classification.name,
                &classification.operation,
                &classification.base_risk,
                &classification.description,
            ),
        )?;
    }
    tracing::debug!("Inserted {} classifications", policy.classifications.len());

    // Insert approval rules
    for rule in &policy.rules {
        conn.execute(
            "INSERT INTO approval_rules (classification_id, mode_id, trust_id, in_workdir, needs_approval, allow_save_pattern, rationale)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            (
                &rule.classification,
                &rule.mode,
                &rule.trust,
                rule.in_workdir,
                rule.needs_approval,
                rule.allow_save_pattern.unwrap_or(true),
                &rule.rationale,
            ),
        )?;
    }
    tracing::debug!("Inserted {} approval rules", policy.rules.len());

    // Insert compound rules
    for compound in &policy.compound_rules {
        conn.execute(
            "INSERT INTO compound_command_rules (separator, strategy, description)
             VALUES (?1, ?2, ?3)",
            (
                &compound.separator,
                &compound.strategy,
                &compound.description,
            ),
        )?;
    }
    tracing::debug!("Inserted {} compound rules", policy.compound_rules.len());

    conn.execute("COMMIT", [])?;
    tracing::info!("Successfully seeded builtin policy");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::schema;

    #[test]
    fn test_builtin_policy_parses() {
        // Test that the embedded TOML is valid
        let policy: BuiltinPolicy = toml::from_str(BUILTIN_POLICY).unwrap();
        assert_eq!(policy.version, 1);
        assert!(!policy.modes.is_empty());
        assert!(!policy.trust_levels.is_empty());
        assert!(!policy.tools.is_empty());
    }

    #[test]
    fn test_seed_builtin() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();
        schema::init_schema_version(&conn).unwrap();

        // Seed
        seed_builtin(&conn).unwrap();

        // Verify data
        let mode_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode_count, 3); // ask, plan, build

        let trust_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM trust_levels", [], |r| r.get(0))
            .unwrap();
        assert_eq!(trust_count, 3); // balanced, careful, manual

        let classification_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM tool_classifications", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(classification_count, 3); // shell-read, shell-write, shell-rm

        // Test idempotency - second seed should be no-op
        seed_builtin(&conn).unwrap();
        let mode_count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode_count2, 3); // Still 3
    }
}
