use anyhow::{anyhow, Result};
use rusqlite::Connection;
use serde_json::Value;
use std::sync::{Arc, Mutex};

use crate::policy::types::*;

/// MCP tool policy handler
pub struct McpPolicyHandler {
    conn: Arc<Mutex<Connection>>,
}

impl McpPolicyHandler {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Check if MCP tool execution needs approval
    pub fn check_approval(
        &self,
        server_id: &str,
        tool_name: &str,
        _params: &Value,
        session_id: &str,
    ) -> Result<ApprovalDecision> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        // Try to get user-defined policy
        let policy_result: Result<(String, bool, bool, Option<String>), rusqlite::Error> = conn
            .query_row(
                "SELECT risk_level, needs_approval, allow_save_pattern, description
                 FROM mcp_tool_policies
                 WHERE server_id = ?1 AND tool_name = ?2",
                [server_id, tool_name],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            );

        let (risk_level, needs_approval, allow_save_pattern, description) =
            if let Ok(policy) = policy_result {
                policy
            } else {
                // Use defaults for undefined MCP tools
                tracing::debug!(
                    "No policy found for {}:{}, using defaults",
                    server_id,
                    tool_name
                );
                (
                    "moderate".to_string(),
                    true, // Safe default: require approval
                    true, // Allow saving patterns
                    Some("Default MCP policy".to_string()),
                )
            };

        // Check for saved patterns
        let matched_pattern = if needs_approval && allow_save_pattern {
            self.check_mcp_patterns(&conn, server_id, tool_name, session_id)?
        } else {
            None
        };

        let risk = match risk_level.as_str() {
            "safe" => RiskLevel::Safe,
            "moderate" => RiskLevel::Moderate,
            "dangerous" => RiskLevel::Dangerous,
            _ => RiskLevel::Moderate,
        };

        Ok(ApprovalDecision {
            needs_approval: needs_approval && matched_pattern.is_none(),
            allow_save_pattern,
            classification: CommandClassification {
                classification_id: format!("mcp-{}-{}", server_id, tool_name),
                operation: Operation::Execute,
                in_workdir: false, // MCP tools are external
                risk_level: risk,
            },
            matched_pattern,
            rationale: description
                .unwrap_or_else(|| format!("MCP tool: {}:{}", server_id, tool_name)),
        })
    }

    /// Check for MCP approval/denial patterns
    fn check_mcp_patterns(
        &self,
        conn: &Connection,
        server_id: &str,
        tool_name: &str,
        _session_id: &str,
    ) -> Result<Option<PatternMatch>> {
        // Check denial patterns first
        let denial_query = r#"
            SELECT id, pattern, match_type
            FROM mcp_approval_patterns
            WHERE server_id = ?1
              AND tool_name = ?2
              AND is_denial = 1
        "#;

        let mut stmt = conn.prepare(denial_query)?;
        let patterns = stmt
            .query_map([server_id, tool_name], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if let Some((id, pattern, match_type)) = patterns.into_iter().next() {
            // For MCP patterns, we match against tool parameters (simplified)
            return Ok(Some(PatternMatch {
                pattern_id: id,
                pattern,
                match_type: self.parse_match_type(&match_type)?,
                is_denial: true,
            }));
        }

        // Check approval patterns
        let approval_query = r#"
            SELECT id, pattern, match_type
            FROM mcp_approval_patterns
            WHERE server_id = ?1
              AND tool_name = ?2
              AND is_denial = 0
        "#;

        let mut stmt = conn.prepare(approval_query)?;
        let patterns = stmt
            .query_map([server_id, tool_name], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        if let Some((id, pattern, match_type)) = patterns.into_iter().next() {
            return Ok(Some(PatternMatch {
                pattern_id: id,
                pattern,
                match_type: self.parse_match_type(&match_type)?,
                is_denial: false,
            }));
        }

        Ok(None)
    }

    fn parse_match_type(&self, match_type: &str) -> Result<MatchType> {
        match match_type {
            "exact" => Ok(MatchType::Exact),
            "prefix" => Ok(MatchType::Prefix),
            "glob" => Ok(MatchType::Glob),
            _ => Err(anyhow!("Invalid match type: {}", match_type)),
        }
    }

    /// Save MCP approval pattern
    #[allow(clippy::too_many_arguments)]
    pub fn save_mcp_pattern(
        &self,
        server_id: &str,
        tool_name: &str,
        pattern: &str,
        match_type: MatchType,
        is_denial: bool,
        source: PatternSource,
        _session_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().map_err(|e| anyhow!("Lock error: {}", e))?;

        conn.execute(
            "INSERT INTO mcp_approval_patterns (server_id, tool_name, pattern, match_type, is_denial, source, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            (
                server_id,
                tool_name,
                pattern,
                &match_type.to_string(),
                is_denial,
                &source.to_string(),
            ),
        )?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::schema;

    #[test]
    fn test_mcp_defaults() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();

        let handler = McpPolicyHandler::new(Arc::new(Mutex::new(conn)));

        // Check tool without policy - should use defaults
        let decision = handler
            .check_approval("github", "create_issue", &Value::Null, "session1")
            .unwrap();

        assert!(decision.needs_approval); // Default: require approval
        assert!(decision.allow_save_pattern); // Default: allow saving
        assert_eq!(decision.classification.risk_level, RiskLevel::Moderate);
    }

    #[test]
    fn test_mcp_with_policy() {
        let conn = Connection::open_in_memory().unwrap();
        schema::create_tables(&conn).unwrap();

        // Insert a policy
        conn.execute(
            "INSERT INTO mcp_tool_policies (server_id, tool_name, risk_level, needs_approval, allow_save_pattern, source, created_at)
             VALUES ('github', 'list_repos', 'safe', 0, 1, 'user', datetime('now'))",
            [],
        )
        .unwrap();

        let handler = McpPolicyHandler::new(Arc::new(Mutex::new(conn)));

        let decision = handler
            .check_approval("github", "list_repos", &Value::Null, "session1")
            .unwrap();

        assert!(!decision.needs_approval); // Policy says no approval
        assert_eq!(decision.classification.risk_level, RiskLevel::Safe);
    }
}
