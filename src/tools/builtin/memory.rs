//! Persistent memory tool for cross-session knowledge retention.
//!
//! Uses SQLite for reliable storage (avoiding race conditions in file-based approaches).
//! Supports:
//! - Key-value storage with categories
//! - Importance ranking
//! - Full-text search
//! - Access tracking

use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::sync::{Arc, Mutex};

/// A single memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier/key for this memory
    pub key: String,
    /// The actual content to remember
    pub value: String,
    /// Category: decision, fact, preference, context, code, todo
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Importance level from 1 (low) to 10 (critical)
    pub importance: i32,
    /// When this memory was created (RFC3339)
    pub created_at: String,
    /// When this memory was last accessed (RFC3339)
    pub accessed_at: String,
    /// How many times this memory has been accessed
    pub access_count: i32,
}

/// Persistent memory storage using SQLite
pub struct TarkMemory {
    conn: Arc<Mutex<Connection>>,
}

impl TarkMemory {
    /// Open or create a memory database at the given path
    pub fn open(path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
        }

        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open memory database: {}", path.display()))?;

        // Create tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS memories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key TEXT UNIQUE NOT NULL,
                value TEXT NOT NULL,
                category TEXT,
                importance INTEGER DEFAULT 5,
                created_at TEXT NOT NULL,
                accessed_at TEXT NOT NULL,
                access_count INTEGER DEFAULT 0
            )",
            [],
        )?;

        // Create indices for fast lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_key ON memories(key)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_category ON memories(category)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_importance ON memories(importance DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_memories_accessed ON memories(accessed_at DESC)",
            [],
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Store a memory entry (insert or update)
    pub fn store(&self, entry: &MemoryEntry) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memories (key, value, category, importance, created_at, accessed_at, access_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                category = excluded.category,
                importance = excluded.importance,
                accessed_at = excluded.accessed_at,
                access_count = access_count + 1",
            params![
                entry.key,
                entry.value,
                entry.category,
                entry.importance,
                entry.created_at,
                entry.accessed_at,
                entry.access_count,
            ],
        )?;
        Ok(())
    }

    /// Query memories by search text and optional filters
    pub fn query(
        &self,
        search: Option<&str>,
        category: Option<&str>,
        min_importance: Option<i32>,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.conn.lock().unwrap();

        let mut sql = String::from(
            "SELECT key, value, category, importance, created_at, accessed_at, access_count 
             FROM memories WHERE 1=1",
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(search_text) = search {
            sql.push_str(" AND (key LIKE ?1 OR value LIKE ?1)");
            params_vec.push(Box::new(format!("%{}%", search_text)));
        }

        if let Some(cat) = category {
            let param_num = params_vec.len() + 1;
            sql.push_str(&format!(" AND category = ?{}", param_num));
            params_vec.push(Box::new(cat.to_string()));
        }

        if let Some(min_imp) = min_importance {
            let param_num = params_vec.len() + 1;
            sql.push_str(&format!(" AND importance >= ?{}", param_num));
            params_vec.push(Box::new(min_imp));
        }

        sql.push_str(" ORDER BY importance DESC, accessed_at DESC");
        sql.push_str(&format!(" LIMIT {}", limit));

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql)?;
        let entries = stmt
            .query_map(params_refs.as_slice(), |row| {
                Ok(MemoryEntry {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    category: row.get(2)?,
                    importance: row.get(3)?,
                    created_at: row.get(4)?,
                    accessed_at: row.get(5)?,
                    access_count: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        // Update access times for returned entries
        let now = Utc::now().to_rfc3339();
        for entry in &entries {
            let _ = conn.execute(
                "UPDATE memories SET accessed_at = ?1, access_count = access_count + 1 WHERE key = ?2",
                params![now, entry.key],
            );
        }

        Ok(entries)
    }

    /// List memories with optional category filter, sorted by importance/recency
    pub fn list(&self, category: Option<&str>, limit: usize) -> Result<Vec<MemoryEntry>> {
        self.query(None, category, None, limit)
    }

    /// Get a specific memory by key
    pub fn get(&self, key: &str) -> Result<Option<MemoryEntry>> {
        let results = self.query(Some(key), None, None, 1)?;
        Ok(results.into_iter().find(|e| e.key == key))
    }

    /// Delete a memory by key
    pub fn delete(&self, key: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        let rows = conn.execute("DELETE FROM memories WHERE key = ?1", params![key])?;
        Ok(rows > 0)
    }

    /// Get the count of memories
    pub fn count(&self) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Clear all memories
    pub fn clear(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM memories", [])?;
        Ok(())
    }
}

// ============================================================================
// MEMORY STORE TOOL
// ============================================================================

/// Tool to store/update a memory entry
pub struct MemoryStoreTool {
    memory: Arc<TarkMemory>,
}

impl MemoryStoreTool {
    pub fn new(memory: Arc<TarkMemory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryStoreTool {
    fn name(&self) -> &str {
        "memory_store"
    }

    fn description(&self) -> &str {
        "Store information for later recall across sessions. Use for important facts, \
         decisions, user preferences, project context, or anything worth remembering. \
         Each memory needs a unique key - using the same key will update the existing memory."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["key", "value"],
            "properties": {
                "key": {
                    "type": "string",
                    "description": "Unique identifier for this memory (e.g., 'user_db_preference', 'project_architecture')"
                },
                "value": {
                    "type": "string",
                    "description": "The information to remember"
                },
                "category": {
                    "type": "string",
                    "enum": ["decision", "fact", "preference", "context", "code", "todo"],
                    "description": "Category of memory for organization"
                },
                "importance": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "description": "Importance level: 1-3 (low), 4-6 (medium), 7-9 (high), 10 (critical). Default: 5"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: key"))?;

        let value = params
            .get("value")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: value"))?;

        let category = params.get("category").and_then(|v| v.as_str());

        let importance = params
            .get("importance")
            .and_then(|v| v.as_i64())
            .map(|n| n.clamp(1, 10) as i32)
            .unwrap_or(5);

        let now = Utc::now().to_rfc3339();

        let entry = MemoryEntry {
            key: key.to_string(),
            value: value.to_string(),
            category: category.map(String::from),
            importance,
            created_at: now.clone(),
            accessed_at: now,
            access_count: 1,
        };

        self.memory.store(&entry)?;

        let category_str = entry.category.as_deref().unwrap_or("none");
        Ok(ToolResult::success(format!(
            "Stored memory '{}' (category: {}, importance: {})",
            key, category_str, importance
        )))
    }
}

// ============================================================================
// MEMORY QUERY TOOL
// ============================================================================

/// Tool to search and retrieve memories
pub struct MemoryQueryTool {
    memory: Arc<TarkMemory>,
}

impl MemoryQueryTool {
    pub fn new(memory: Arc<TarkMemory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryQueryTool {
    fn name(&self) -> &str {
        "memory_query"
    }

    fn description(&self) -> &str {
        "Search stored memories by text, category, or importance. Use this to recall \
         previously stored information. Returns matching memories sorted by importance \
         and recency."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "search": {
                    "type": "string",
                    "description": "Text to search for in memory keys and values"
                },
                "category": {
                    "type": "string",
                    "enum": ["decision", "fact", "preference", "context", "code", "todo"],
                    "description": "Filter by category"
                },
                "min_importance": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 10,
                    "description": "Only return memories with at least this importance"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 50,
                    "description": "Maximum number of memories to return. Default: 10"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let search = params.get("search").and_then(|v| v.as_str());
        let category = params.get("category").and_then(|v| v.as_str());
        let min_importance = params
            .get("min_importance")
            .and_then(|v| v.as_i64())
            .map(|n| n as i32);
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(10)
            .min(50);

        let entries = self.memory.query(search, category, min_importance, limit)?;

        if entries.is_empty() {
            return Ok(ToolResult::success("No memories found matching the query."));
        }

        // Format results
        let mut output = format!("Found {} memories:\n\n", entries.len());
        for entry in entries {
            let category_str = entry.category.as_deref().unwrap_or("none");
            output.push_str(&format!(
                "## {} (importance: {}, category: {})\n{}\n\n",
                entry.key, entry.importance, category_str, entry.value
            ));
        }

        Ok(ToolResult::success(output))
    }
}

// ============================================================================
// MEMORY LIST TOOL
// ============================================================================

/// Tool to list all memories or filter by category
pub struct MemoryListTool {
    memory: Arc<TarkMemory>,
}

impl MemoryListTool {
    pub fn new(memory: Arc<TarkMemory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryListTool {
    fn name(&self) -> &str {
        "memory_list"
    }

    fn description(&self) -> &str {
        "List stored memories, optionally filtered by category. Returns memories \
         sorted by importance and recency. Use this to get an overview of what \
         has been remembered."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "category": {
                    "type": "string",
                    "enum": ["decision", "fact", "preference", "context", "code", "todo"],
                    "description": "Filter by category"
                },
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 50,
                    "description": "Maximum number of memories to return. Default: 20"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let category = params.get("category").and_then(|v| v.as_str());
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n as usize)
            .unwrap_or(20)
            .min(50);

        let entries = self.memory.list(category, limit)?;
        let total = self.memory.count()?;

        if entries.is_empty() {
            return Ok(ToolResult::success("No memories stored yet."));
        }

        // Format as a summary table
        let mut output = format!(
            "Showing {} of {} memories{}:\n\n",
            entries.len(),
            total,
            category
                .map(|c| format!(" (category: {})", c))
                .unwrap_or_default()
        );

        output.push_str("| Key | Category | Importance | Preview |\n");
        output.push_str("|-----|----------|------------|----------|\n");

        for entry in entries {
            let category_str = entry.category.as_deref().unwrap_or("-");
            let preview: String = entry.value.chars().take(50).collect();
            let preview = if entry.value.len() > 50 {
                format!("{}...", preview)
            } else {
                preview
            };
            output.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                entry.key, category_str, entry.importance, preview
            ));
        }

        Ok(ToolResult::success(output))
    }
}

// ============================================================================
// MEMORY DELETE TOOL
// ============================================================================

/// Tool to delete a memory
pub struct MemoryDeleteTool {
    memory: Arc<TarkMemory>,
}

impl MemoryDeleteTool {
    pub fn new(memory: Arc<TarkMemory>) -> Self {
        Self { memory }
    }
}

#[async_trait]
impl Tool for MemoryDeleteTool {
    fn name(&self) -> &str {
        "memory_delete"
    }

    fn description(&self) -> &str {
        "Delete a stored memory by its key. Use this to remove outdated or \
         incorrect information."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["key"],
            "properties": {
                "key": {
                    "type": "string",
                    "description": "The key of the memory to delete"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        let key = params
            .get("key")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: key"))?;

        let deleted = self.memory.delete(key)?;

        if deleted {
            Ok(ToolResult::success(format!("Deleted memory '{}'", key)))
        } else {
            Ok(ToolResult::error(format!("Memory '{}' not found", key)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn create_test_memory() -> (TarkMemory, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test_memory.db");
        let memory = TarkMemory::open(&db_path).unwrap();
        (memory, dir)
    }

    #[test]
    fn test_memory_store_and_get() {
        let (memory, _dir) = create_test_memory();

        let entry = MemoryEntry {
            key: "test_key".to_string(),
            value: "test_value".to_string(),
            category: Some("fact".to_string()),
            importance: 7,
            created_at: Utc::now().to_rfc3339(),
            accessed_at: Utc::now().to_rfc3339(),
            access_count: 1,
        };

        memory.store(&entry).unwrap();

        let retrieved = memory.get("test_key").unwrap().unwrap();
        assert_eq!(retrieved.key, "test_key");
        assert_eq!(retrieved.value, "test_value");
        assert_eq!(retrieved.importance, 7);
    }

    #[test]
    fn test_memory_update() {
        let (memory, _dir) = create_test_memory();

        let entry1 = MemoryEntry {
            key: "update_test".to_string(),
            value: "original".to_string(),
            category: None,
            importance: 5,
            created_at: Utc::now().to_rfc3339(),
            accessed_at: Utc::now().to_rfc3339(),
            access_count: 1,
        };
        memory.store(&entry1).unwrap();

        let entry2 = MemoryEntry {
            key: "update_test".to_string(),
            value: "updated".to_string(),
            category: Some("fact".to_string()),
            importance: 8,
            created_at: Utc::now().to_rfc3339(),
            accessed_at: Utc::now().to_rfc3339(),
            access_count: 1,
        };
        memory.store(&entry2).unwrap();

        let retrieved = memory.get("update_test").unwrap().unwrap();
        assert_eq!(retrieved.value, "updated");
        assert_eq!(retrieved.importance, 8);
    }

    #[test]
    fn test_memory_query() {
        let (memory, _dir) = create_test_memory();

        for i in 1..=5 {
            let entry = MemoryEntry {
                key: format!("key_{}", i),
                value: format!("value containing search term {}", i),
                category: if i % 2 == 0 {
                    Some("fact".to_string())
                } else {
                    Some("decision".to_string())
                },
                importance: i,
                created_at: Utc::now().to_rfc3339(),
                accessed_at: Utc::now().to_rfc3339(),
                access_count: 1,
            };
            memory.store(&entry).unwrap();
        }

        // Test text search
        let results = memory.query(Some("search term"), None, None, 10).unwrap();
        assert_eq!(results.len(), 5);

        // Test category filter
        let results = memory.query(None, Some("fact"), None, 10).unwrap();
        assert_eq!(results.len(), 2);

        // Test importance filter
        let results = memory.query(None, None, Some(3), 10).unwrap();
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_memory_delete() {
        let (memory, _dir) = create_test_memory();

        let entry = MemoryEntry {
            key: "to_delete".to_string(),
            value: "will be deleted".to_string(),
            category: None,
            importance: 5,
            created_at: Utc::now().to_rfc3339(),
            accessed_at: Utc::now().to_rfc3339(),
            access_count: 1,
        };
        memory.store(&entry).unwrap();

        assert!(memory.get("to_delete").unwrap().is_some());
        assert!(memory.delete("to_delete").unwrap());
        assert!(memory.get("to_delete").unwrap().is_none());
        assert!(!memory.delete("to_delete").unwrap()); // Already deleted
    }

    #[tokio::test]
    async fn test_memory_store_tool() {
        let (memory, _dir) = create_test_memory();
        let memory = Arc::new(memory);
        let tool = MemoryStoreTool::new(memory.clone());

        let params = json!({
            "key": "db_type",
            "value": "PostgreSQL is used for the main database",
            "category": "fact",
            "importance": 8
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);

        let retrieved = memory.get("db_type").unwrap().unwrap();
        assert_eq!(retrieved.importance, 8);
    }

    #[tokio::test]
    async fn test_memory_query_tool() {
        let (memory, _dir) = create_test_memory();
        let memory = Arc::new(memory);

        // Store some memories
        let store_tool = MemoryStoreTool::new(memory.clone());
        store_tool
            .execute(json!({
                "key": "pref_1",
                "value": "User prefers dark mode",
                "category": "preference",
                "importance": 6
            }))
            .await
            .unwrap();

        let query_tool = MemoryQueryTool::new(memory);

        let result = query_tool
            .execute(json!({
                "search": "dark mode"
            }))
            .await
            .unwrap();

        assert!(result.success);
        assert!(result.output.contains("pref_1"));
    }
}
