use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Old approvals.json format
#[derive(Debug, Serialize, Deserialize, Default)]
struct OldApprovals {
    #[serde(default)]
    approved_commands: HashMap<String, Vec<ApprovalEntry>>,
    #[serde(default)]
    denied_commands: HashMap<String, Vec<ApprovalEntry>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ApprovalEntry {
    pattern: String,
    match_type: String,
    #[serde(default)]
    timestamp: String,
}

/// Migration report
#[derive(Debug)]
pub struct MigrationReport {
    pub total_patterns: usize,
    pub migrated: usize,
    pub skipped_internal: usize,
    pub errors: Vec<String>,
}

/// Migrate from old approvals.json to new policy.db
///
/// NOTE: Since users cannot add patterns for internal tools in the new system,
/// internal tool patterns will be logged and skipped.
pub fn migrate_approvals(json_path: &Path) -> Result<MigrationReport> {
    if !json_path.exists() {
        tracing::info!(
            "No approvals.json found at {:?}, skipping migration",
            json_path
        );
        return Ok(MigrationReport {
            total_patterns: 0,
            migrated: 0,
            skipped_internal: 0,
            errors: vec![],
        });
    }

    tracing::info!("Migrating approvals from {:?}", json_path);

    let content = std::fs::read_to_string(json_path)?;
    let old_approvals: OldApprovals = serde_json::from_str(&content).unwrap_or_default();

    let mut report = MigrationReport {
        total_patterns: 0,
        migrated: 0,
        skipped_internal: 0,
        errors: vec![],
    };

    // Count total patterns
    for patterns in old_approvals.approved_commands.values() {
        report.total_patterns += patterns.len();
    }
    for patterns in old_approvals.denied_commands.values() {
        report.total_patterns += patterns.len();
    }

    // Check each pattern
    for (tool, patterns) in old_approvals.approved_commands {
        for _pattern in patterns {
            if is_internal_tool(&tool) {
                report.skipped_internal += 1;
                tracing::warn!(
                    "Skipped approval pattern for internal tool '{}' - users cannot add patterns for internal tools",
                    tool
                );
            } else {
                // TODO: Migrate MCP patterns when we have MCP server info
                report.skipped_internal += 1;
                tracing::warn!(
                    "Cannot migrate pattern for '{}' - need MCP server info",
                    tool
                );
            }
        }
    }

    for (tool, patterns) in old_approvals.denied_commands {
        for _pattern in patterns {
            if is_internal_tool(&tool) {
                report.skipped_internal += 1;
                tracing::warn!(
                    "Skipped denial pattern for internal tool '{}' - users cannot add patterns for internal tools",
                    tool
                );
            } else {
                report.skipped_internal += 1;
                tracing::warn!(
                    "Cannot migrate pattern for '{}' - need MCP server info",
                    tool
                );
            }
        }
    }

    // Rename old file to backup
    let backup_path = json_path.with_extension("json.bak");
    std::fs::rename(json_path, &backup_path)?;
    tracing::info!("Backed up approvals.json to {:?}", backup_path);

    Ok(report)
}

/// Check if tool is an internal tool
fn is_internal_tool(tool: &str) -> bool {
    const INTERNAL_TOOLS: &[&str] = &[
        "shell",
        "safe_shell",
        "read_file",
        "write_file",
        "delete_file",
        "grep",
        "glob",
        "think",
        "memory_store",
        "memory_query",
        "memory_list",
        "memory_delete",
        "todo",
    ];

    INTERNAL_TOOLS.contains(&tool)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_migrate_with_internal_tools() {
        let temp_dir = TempDir::new().unwrap();
        let json_path = temp_dir.path().join("approvals.json");

        // Create old approvals.json with internal tool patterns
        let old_data = serde_json::json!({
            "approved_commands": {
                "shell": [
                    {
                        "pattern": "cargo build",
                        "match_type": "prefix"
                    }
                ],
                "write_file": [
                    {
                        "pattern": "test.txt",
                        "match_type": "exact"
                    }
                ]
            },
            "denied_commands": {
                "shell": [
                    {
                        "pattern": "rm -rf /",
                        "match_type": "prefix"
                    }
                ]
            }
        });

        fs::write(&json_path, serde_json::to_string_pretty(&old_data).unwrap()).unwrap();

        // Migrate
        let report = migrate_approvals(&json_path).unwrap();

        assert_eq!(report.total_patterns, 3);
        assert_eq!(report.skipped_internal, 3); // All are internal tools
        assert_eq!(report.migrated, 0);

        // Verify backup was created
        assert!(temp_dir.path().join("approvals.json.bak").exists());
        assert!(!json_path.exists()); // Original should be renamed
    }

    #[test]
    fn test_is_internal_tool() {
        assert!(is_internal_tool("shell"));
        assert!(is_internal_tool("write_file"));
        assert!(!is_internal_tool("github_create_issue"));
    }
}
