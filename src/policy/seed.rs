use anyhow::Result;
use rusqlite::Connection;
use serde::Deserialize;

/// Split builtin policies embedded at compile time from configs directory
const MODES_CONFIG: &str = include_str!("configs/modes.toml");
const TRUST_CONFIG: &str = include_str!("configs/trust.toml");
const TOOLS_CONFIG: &str = include_str!("configs/tools.toml");
const DEFAULTS_CONFIG: &str = include_str!("configs/defaults.toml");

#[derive(Debug, Deserialize)]
struct ModesConfig {
    #[allow(dead_code)]
    version: u32,
    modes: Vec<Mode>,
}

#[derive(Debug, Deserialize)]
struct TrustConfig {
    #[allow(dead_code)]
    version: u32,
    trust_levels: Vec<TrustLevel>,
}

#[derive(Debug, Deserialize)]
struct ToolsConfig {
    #[allow(dead_code)]
    version: u32,
    categories: Vec<Category>,
    tools: Vec<Tool>,
}

#[derive(Debug, Deserialize)]
struct DefaultsConfig {
    #[allow(dead_code)]
    version: u32,
    approval_defaults: std::collections::HashMap<String, String>,
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
    base_risk: String,
    #[serde(default)]
    classification: String, // "dynamic" or "static"
    #[serde(default)]
    operation: String, // "read", "write", "delete", "execute"
    modes: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CompoundRule {
    separator: String,
    strategy: String,
    description: Option<String>,
}

/// Seed the database with builtin policy from split config files
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

    tracing::info!("Seeding builtin policy from split config files");

    // Parse split config files
    let modes_config: ModesConfig = toml::from_str(MODES_CONFIG)?;
    let trust_config: TrustConfig = toml::from_str(TRUST_CONFIG)?;
    let tools_config: ToolsConfig = toml::from_str(TOOLS_CONFIG)?;
    let defaults_config: DefaultsConfig = toml::from_str(DEFAULTS_CONFIG)?;

    // Insert modes
    for mode in &modes_config.modes {
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
    tracing::debug!("Inserted {} agent modes", modes_config.modes.len());

    // Insert trust levels
    for trust in &trust_config.trust_levels {
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
    tracing::debug!("Inserted {} trust levels", trust_config.trust_levels.len());

    // Insert categories
    for category in &tools_config.categories {
        conn.execute(
            "INSERT INTO tool_categories (id, name, description) VALUES (?1, ?2, ?3)",
            (&category.id, &category.name, &category.description),
        )?;
    }
    tracing::debug!("Inserted {} tool categories", tools_config.categories.len());

    // Insert tools and generate classifications
    for tool in &tools_config.tools {
        // Determine permissions based on base_risk and operation
        let permissions = match tool.base_risk.as_str() {
            "safe" => "R",
            "moderate" => "W",
            "dangerous" => "W",
            _ => "R",
        };

        conn.execute(
            "INSERT INTO tool_types (id, name, category_id, permissions, base_risk)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            (
                &tool.id,
                &tool.name,
                &tool.category,
                permissions,
                &tool.base_risk,
            ),
        )?;

        // Generate tool availability for ALL modes (both available and unavailable)
        let all_modes = ["ask", "plan", "build"];
        for mode in &all_modes {
            let is_available = tool.modes.iter().any(|m| m == mode);
            conn.execute(
                "INSERT INTO tool_mode_availability (tool_type_id, mode_id, is_available, alternative_tool_id)
                 VALUES (?1, ?2, ?3, ?4)",
                (&tool.id, mode, is_available, None::<String>),
            )?;
        }

        // Generate classification for non-dynamic tools
        if tool.classification != "dynamic" {
            let operation = if !tool.operation.is_empty() {
                &tool.operation
            } else {
                match tool.base_risk.as_str() {
                    "safe" => "read",
                    "moderate" => "write",
                    "dangerous" => "delete",
                    _ => "execute",
                }
            };

            let classification_id = format!("{}-{}", tool.id, tool.base_risk);
            conn.execute(
                "INSERT INTO tool_classifications (id, tool_type_id, name, operation, base_risk, description)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                (
                    &classification_id,
                    &tool.id,
                    &tool.name,
                    operation,
                    &tool.base_risk,
                    None::<String>,
                ),
            )?;
        } else {
            // For dynamic tools (shell), create classifications for each operation type
            for (op_suffix, operation, risk) in &[
                ("read", "read", "safe"),
                ("write", "write", "moderate"),
                ("rm", "delete", "dangerous"),
            ] {
                let classification_id = format!("{}-{}", tool.id, op_suffix);
                conn.execute(
                    "INSERT INTO tool_classifications (id, tool_type_id, name, operation, base_risk, description)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    (
                        &classification_id,
                        &tool.id,
                        &format!("{} {}", tool.name, operation),
                        operation,
                        risk,
                        None::<String>,
                    ),
                )?;
            }
        }
    }
    tracing::debug!("Inserted {} tool types", tools_config.tools.len());

    // Generate approval rules from defaults
    let mut rule_count = 0;
    for (key, behavior_str) in &defaults_config.approval_defaults {
        let parts: Vec<&str> = key.split('.').collect();
        if parts.len() != 3 {
            tracing::warn!("Invalid default key format: {}", key);
            continue;
        }

        let risk = parts[0];
        let trust = parts[1];
        let location = parts[2];

        let in_workdir = location == "in_workdir";
        let (needs_approval, allow_save_pattern) = match behavior_str.as_str() {
            "auto_approve" => (false, true),
            "prompt" => (true, true),
            "prompt_no_save" => (true, false),
            _ => {
                tracing::warn!("Invalid behavior: {}", behavior_str);
                continue;
            }
        };

        // Get all classifications for this risk level
        let mut stmt = conn.prepare("SELECT id FROM tool_classifications WHERE base_risk = ?1")?;
        let classifications: Vec<String> = stmt
            .query_map([risk], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;

        // Insert rule for each classification
        for classification_id in classifications {
            conn.execute(
                "INSERT INTO approval_rules (classification_id, mode_id, trust_id, in_workdir, needs_approval, allow_save_pattern, rationale)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                (
                    &classification_id,
                    "build", // Only build mode has approval gate
                    trust,
                    in_workdir,
                    needs_approval,
                    allow_save_pattern,
                    Some(format!("{} risk with {} trust {}", risk, trust, location)),
                ),
            )?;
            rule_count += 1;
        }
    }
    tracing::debug!("Generated {} approval rules from defaults", rule_count);

    // Insert compound rules
    for compound in &defaults_config.compound_rules {
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
    tracing::debug!(
        "Inserted {} compound rules",
        defaults_config.compound_rules.len()
    );

    conn.execute("COMMIT", [])?;
    tracing::info!("Successfully seeded builtin policy from split configs");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::schema;

    #[test]
    fn test_split_configs_parse() {
        // Test that the embedded TOML files are valid
        let modes: ModesConfig = toml::from_str(MODES_CONFIG).unwrap();
        assert_eq!(modes.version, 1);
        assert_eq!(modes.modes.len(), 3);

        let trust: TrustConfig = toml::from_str(TRUST_CONFIG).unwrap();
        assert_eq!(trust.version, 1);
        assert_eq!(trust.trust_levels.len(), 3);

        let tools: ToolsConfig = toml::from_str(TOOLS_CONFIG).unwrap();
        assert_eq!(tools.version, 1);
        assert!(!tools.tools.is_empty());

        let defaults: DefaultsConfig = toml::from_str(DEFAULTS_CONFIG).unwrap();
        assert_eq!(defaults.version, 1);
        assert!(!defaults.approval_defaults.is_empty());
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
        assert!(classification_count >= 3); // At least shell-read, shell-write, shell-rm

        // Verify rules were generated
        let rule_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM approval_rules", [], |r| r.get(0))
            .unwrap();
        assert!(rule_count > 0); // Rules should be generated from defaults

        // Test idempotency - second seed should be no-op
        seed_builtin(&conn).unwrap();
        let mode_count2: i64 = conn
            .query_row("SELECT COUNT(*) FROM agent_modes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode_count2, 3); // Still 3
    }
}
