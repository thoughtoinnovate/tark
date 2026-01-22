---
name: Native Tools and MCP Client
overview: Implement native Thinking and Memory tools in Rust (avoiding buggy MCP implementations), plus a full MCP Client for connecting to user-managed external MCP servers.
todos:
  - id: thinking-tool
    content: "STEP 1: Create src/tools/builtin/thinking.rs - copy COMPLETE CODE from Section 1.1"
    status: pending
  - id: memory-tool
    content: "STEP 2: Create src/tools/builtin/memory.rs - copy COMPLETE CODE from Section 1.2"
    status: pending
  - id: builtin-mod
    content: "STEP 3: Create src/tools/builtin/mod.rs - copy COMPLETE CODE from Section 1.3"
    status: pending
  - id: tools-mod-update
    content: "STEP 4: Modify src/tools/mod.rs - apply EXACT CHANGES from Section 1.4"
    status: pending
  - id: mcp-deps
    content: "STEP 5: Modify Cargo.toml - add dependencies from Section 2.1"
    status: pending
  - id: mcp-types
    content: "STEP 6: Create src/mcp/types.rs - copy COMPLETE CODE from Section 2.2"
    status: pending
  - id: mcp-transport
    content: "STEP 7: Create src/mcp/transport.rs - copy COMPLETE CODE from Section 2.3"
    status: pending
  - id: mcp-client
    content: "STEP 8: Create src/mcp/client.rs - copy COMPLETE CODE from Section 2.4"
    status: pending
  - id: mcp-wrapper
    content: "STEP 9: Create src/mcp/wrapper.rs - copy COMPLETE CODE from Section 2.5"
    status: pending
  - id: mcp-mod
    content: "STEP 10: Create src/mcp/mod.rs - copy COMPLETE CODE from Section 2.6"
    status: pending
  - id: storage-extend
    content: "STEP 11: Modify src/storage/mod.rs - apply EXACT CHANGES from Section 2.7"
    status: pending
  - id: lib-update
    content: "STEP 12: Modify src/lib.rs - apply EXACT CHANGES from Section 2.8"
    status: pending
  - id: tests
    content: "STEP 13: Create tests/builtin_tools_test.rs - copy COMPLETE CODE from Section 3.1"
    status: pending
  - id: config-update
    content: "STEP 14: Update examples/tark-config/mcp/servers.toml from Section 3.2"
    status: pending
---

# Native Tools and MCP Client - DETAILED IMPLEMENTATION GUIDE

## QUICK REFERENCE - FILES TO CREATE/MODIFY

| Step | Action | File | Section |

|------|--------|------|---------|

| 0a | CREATE DIR | `src/tools/builtin/` | Prerequisites |

| 0b | CREATE DIR | `src/mcp/` | Prerequisites |

| 1 | CREATE | `src/tools/builtin/thinking.rs` | 1.1 |

| 2 | CREATE | `src/tools/builtin/memory.rs` | 1.2 |

| 3 | CREATE | `src/tools/builtin/mod.rs` | 1.3 |

| 4 | MODIFY | `src/tools/mod.rs` (4 changes) | 1.4 |

| 5 | MODIFY | `Cargo.toml` (2 changes) | 2.1 |

| 6 | CREATE | `src/mcp/types.rs` | 2.2 |

| 7 | CREATE | `src/mcp/transport.rs` | 2.3 |

| 8 | CREATE | `src/mcp/client.rs` | 2.4 |

| 9 | CREATE | `src/mcp/wrapper.rs` | 2.5 |

| 10 | CREATE | `src/mcp/mod.rs` | 2.6 |

| 11 | MODIFY | `src/storage/mod.rs` | 2.7 |

| 12 | MODIFY | `src/lib.rs` | 2.8 |

| 13 | CREATE | `tests/builtin_tools_test.rs` | 3.1 |

| 14 | MODIFY | `examples/tark-config/mcp/servers.toml` | 3.2 |

**CHECKPOINT RULES:**

- After Step 4: `cargo check` MUST pass
- After Step 12: `cargo check` MUST pass
- After Step 14: `cargo test --all-features` MUST pass

---

This document contains COMPLETE, COPY-PASTE READY code for implementing:

1. Thinking Tool (native Rust)
2. Memory Tool (native Rust with SQLite)
3. MCP Client (for external servers)

IMPORTANT: Follow the steps IN ORDER. Each section contains the COMPLETE file contents.

---

## BEFORE YOU START - CRITICAL CHECKLIST

### Workspace Path

All file paths in this document are RELATIVE to the workspace root:

```
/home/dev/data/work/code/tark/
```

For example, `src/tools/builtin/thinking.rs` means:

```
/home/dev/data/work/code/tark/src/tools/builtin/thinking.rs
```

### Prerequisites

Before starting, verify:

1. You are in the tark workspace directory
2. `cargo build` currently succeeds (baseline working state)
3. You have write permissions to the src/ directory

### Directory Creation Order

You MUST create directories BEFORE creating files in them:

```bash
# STEP 0a: Create the builtin tools directory
mkdir -p src/tools/builtin

# STEP 0b: Create the mcp directory  
mkdir -p src/mcp
```

### Verification After Each Step

After EACH file creation or modification, run:

```bash
cargo check
```

If it fails, FIX THE ERROR before proceeding to the next step.

### Common Pitfalls to Avoid

1. DO NOT skip directory creation steps
2. DO NOT change the order of steps
3. DO NOT modify code snippets (copy them EXACTLY)
4. DO NOT proceed if `cargo check` fails
5. The crate name is `tark_cli` (not `tark`) in test imports

---

## Architecture Overview

```mermaid
flowchart TB
    subgraph ToolRegistry [ToolRegistry]
        direction TB
        RegisterTools[register tools]
    end

    subgraph BuiltinTools [Built-in Tools - Native Rust]
        ThinkTool[ThinkTool]
        MemoryStore[MemoryStoreTool]
        MemoryQuery[MemoryQueryTool]
        MemoryList[MemoryListTool]
        MemoryDelete[MemoryDeleteTool]
    end

    subgraph ThinkingSystem [Thinking System]
        ThinkingTracker[ThinkingTracker]
        ThoughtHistory[Thought History]
        Branches[Branch History]
    end

    subgraph MemorySystem [Memory System]
        TarkMemory[TarkMemory]
        SQLiteDB[(SQLite DB)]
    end

    subgraph McpClient [MCP Client - Feature Gated]
        McpManager[McpServerManager]
        StdioTransport[StdioTransport]
        McpWrapper[McpToolWrapper]
    end

    subgraph ExternalServers [External MCP Servers - User Managed]
        GitHub[GitHub MCP]
        BraveSearch[Brave Search MCP]
        CustomServer[Custom Servers]
    end

    subgraph Config [Configuration]
        ServersToml[servers.toml]
    end

    ToolRegistry --> BuiltinTools
    ToolRegistry --> McpWrapper

    ThinkTool --> ThinkingTracker
    ThinkingTracker --> ThoughtHistory
    ThinkingTracker --> Branches

    MemoryStore --> TarkMemory
    MemoryQuery --> TarkMemory
    MemoryList --> TarkMemory
    MemoryDelete --> TarkMemory
    TarkMemory --> SQLiteDB

    McpWrapper --> McpManager
    McpManager --> StdioTransport
    ServersToml -->|load config| McpManager
    StdioTransport --> ExternalServers
```

### Data Flow

```mermaid
sequenceDiagram
    participant Agent
    participant ToolRegistry
    participant ThinkTool
    participant MemoryTool
    participant McpWrapper
    participant ExternalMCP

    Note over Agent,ExternalMCP: Thinking Flow
    Agent->>ToolRegistry: execute("think", params)
    ToolRegistry->>ThinkTool: execute(params)
    ThinkTool->>ThinkTool: record thought
    ThinkTool-->>Agent: ThinkingSummary

    Note over Agent,ExternalMCP: Memory Flow
    Agent->>ToolRegistry: execute("memory_store", params)
    ToolRegistry->>MemoryTool: execute(params)
    MemoryTool->>MemoryTool: INSERT INTO SQLite
    MemoryTool-->>Agent: success

    Note over Agent,ExternalMCP: MCP Flow
    Agent->>ToolRegistry: execute("gh:create_issue", params)
    ToolRegistry->>McpWrapper: execute(params)
    McpWrapper->>ExternalMCP: JSON-RPC call
    ExternalMCP-->>McpWrapper: result
    McpWrapper-->>Agent: ToolResult
```

### File Structure

```
src/
├── tools/
│   ├── mod.rs              # Modified: add builtin module + registration
│   └── builtin/            # NEW DIRECTORY
│       ├── mod.rs          # NEW: exports thinking + memory
│       ├── thinking.rs     # NEW: ThinkTool, ThinkingTracker
│       └── memory.rs       # NEW: TarkMemory, Memory*Tools
│
├── mcp/                    # NEW DIRECTORY (feature-gated)
│   ├── mod.rs              # NEW: module exports
│   ├── types.rs            # NEW: ConnectionStatus, McpToolDef, etc.
│   ├── transport.rs        # NEW: StdioTransport
│   ├── client.rs           # NEW: McpServerManager
│   └── wrapper.rs          # NEW: McpToolWrapper
│
├── storage/
│   └── mod.rs              # Modified: extend McpServer struct
│
└── lib.rs                  # Modified: add mcp module
```

---

## SECTION 1: BUILT-IN TOOLS (Thinking + Memory)

### PREREQUISITE: Create the builtin directory

BEFORE creating any files in this section, you MUST create the directory:

```bash
mkdir -p src/tools/builtin
```

Verify the directory exists:

```bash
ls -la src/tools/builtin
# Should show an empty directory, not "No such file or directory"
```

---

### 1.1 CREATE FILE: src/tools/builtin/thinking.rs

**Full path:** `/home/dev/data/work/code/tark/src/tools/builtin/thinking.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Verification before:** `ls src/tools/builtin/thinking.rs` should return "No such file or directory"

Create a new file at `src/tools/builtin/thinking.rs` with this EXACT content:

```rust
//! Sequential thinking tool for structured reasoning.
//!
//! Enables LLMs to think step-by-step with support for:
//! - Numbered thought sequences
//! - Thought type categorization
//! - Confidence tracking
//! - Revision of previous thoughts
//! - Branching for alternative reasoning paths

use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A single thought in a reasoning chain
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thought {
    /// The actual thought content
    pub thought: String,
    /// Which thought number this is (1-indexed)
    pub thought_number: u32,
    /// Expected total number of thoughts
    pub total_thoughts: u32,
    /// Whether another thought is needed after this
    pub next_thought_needed: bool,
    /// Type of thinking: hypothesis, analysis, plan, decision, reflection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thought_type: Option<String>,
    /// Confidence level from 0.0 to 1.0
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// If this revises a previous thought, which one
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revises_thought: Option<u32>,
    /// Branch identifier for alternative reasoning paths
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

/// Summary returned after recording a thought
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThinkingSummary {
    /// Current thought number
    pub thought_number: u32,
    /// Total thoughts expected
    pub total_thoughts: u32,
    /// Whether more thoughts are needed
    pub next_thought_needed: bool,
    /// Number of thoughts recorded so far
    pub history_length: usize,
    /// List of branch IDs if any
    pub branches: Vec<String>,
}

/// Tracks the thinking process within a session
#[derive(Debug, Default)]
pub struct ThinkingTracker {
    /// Main thought history
    history: Vec<Thought>,
    /// Thoughts organized by branch ID
    branches: HashMap<String, Vec<Thought>>,
}

impl ThinkingTracker {
    /// Create a new thinking tracker
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            branches: HashMap::new(),
        }
    }

    /// Record a new thought and return a summary
    pub fn record(&mut self, mut thought: Thought) -> ThinkingSummary {
        // Adjust total_thoughts if current number exceeds it
        if thought.thought_number > thought.total_thoughts {
            thought.total_thoughts = thought.thought_number;
        }

        // Add to main history
        self.history.push(thought.clone());

        // If this is a branched thought, also add to branch history
        if let Some(ref branch_id) = thought.branch_id {
            self.branches
                .entry(branch_id.clone())
                .or_default()
                .push(thought.clone());
        }

        // Build and return summary
        ThinkingSummary {
            thought_number: thought.thought_number,
            total_thoughts: thought.total_thoughts,
            next_thought_needed: thought.next_thought_needed,
            history_length: self.history.len(),
            branches: self.branches.keys().cloned().collect(),
        }
    }

    /// Get the full thought history
    pub fn history(&self) -> &[Thought] {
        &self.history
    }

    /// Get thoughts for a specific branch
    pub fn branch_history(&self, branch_id: &str) -> Option<&Vec<Thought>> {
        self.branches.get(branch_id)
    }

    /// Clear all thoughts (e.g., on new session)
    pub fn clear(&mut self) {
        self.history.clear();
        self.branches.clear();
    }

    /// Get the number of thoughts recorded
    pub fn len(&self) -> usize {
        self.history.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.history.is_empty()
    }
}

/// Tool that allows the agent to record structured thinking steps
pub struct ThinkTool {
    tracker: Arc<Mutex<ThinkingTracker>>,
}

impl ThinkTool {
    /// Create a new ThinkTool with the given tracker
    pub fn new(tracker: Arc<Mutex<ThinkingTracker>>) -> Self {
        Self { tracker }
    }

    /// Create a new ThinkTool with its own tracker
    pub fn new_standalone() -> Self {
        Self {
            tracker: Arc::new(Mutex::new(ThinkingTracker::new())),
        }
    }

    /// Get access to the underlying tracker
    pub fn tracker(&self) -> Arc<Mutex<ThinkingTracker>> {
        self.tracker.clone()
    }
}

#[async_trait]
impl Tool for ThinkTool {
    fn name(&self) -> &str {
        "think"
    }

    fn description(&self) -> &str {
        "Record your reasoning process step-by-step. Use this tool to think through \
         complex problems before taking actions. Each thought should be a discrete \
         reasoning step. This helps with debugging, planning, and decision-making. \
         The tool tracks your thought chain and supports revisions and branching."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "required": ["thought", "thought_number", "total_thoughts", "next_thought_needed"],
            "properties": {
                "thought": {
                    "type": "string",
                    "description": "Your current thinking step - a discrete piece of reasoning"
                },
                "thought_number": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Which thought number this is (1-indexed)"
                },
                "total_thoughts": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Expected total number of thoughts (can increase if needed)"
                },
                "next_thought_needed": {
                    "type": "boolean",
                    "description": "Whether you need to continue thinking after this"
                },
                "thought_type": {
                    "type": "string",
                    "enum": ["hypothesis", "analysis", "plan", "decision", "reflection"],
                    "description": "Category of this thought"
                },
                "confidence": {
                    "type": "number",
                    "minimum": 0.0,
                    "maximum": 1.0,
                    "description": "How confident you are in this thought (0.0 to 1.0)"
                },
                "revises_thought": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "If this revises a previous thought, which number"
                },
                "branch_id": {
                    "type": "string",
                    "description": "Identifier for alternative reasoning branch"
                }
            }
        })
    }

    fn risk_level(&self) -> RiskLevel {
        // Thinking is always safe - it doesn't modify anything
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Parse required parameters
        let thought_str = params
            .get("thought")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: thought"))?;

        let thought_number = params
            .get("thought_number")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: thought_number"))?
            as u32;

        let total_thoughts = params
            .get("total_thoughts")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: total_thoughts"))?
            as u32;

        let next_thought_needed = params
            .get("next_thought_needed")
            .and_then(|v| v.as_bool())
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: next_thought_needed"))?;

        // Parse optional parameters
        let thought_type = params
            .get("thought_type")
            .and_then(|v| v.as_str())
            .map(String::from);

        let confidence = params
            .get("confidence")
            .and_then(|v| v.as_f64())
            .map(|f| f as f32);

        let revises_thought = params
            .get("revises_thought")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32);

        let branch_id = params
            .get("branch_id")
            .and_then(|v| v.as_str())
            .map(String::from);

        // Build the thought
        let thought = Thought {
            thought: thought_str.to_string(),
            thought_number,
            total_thoughts,
            next_thought_needed,
            thought_type,
            confidence,
            revises_thought,
            branch_id,
        };

        // Record the thought
        let summary = {
            let mut tracker = self.tracker.lock().unwrap();
            tracker.record(thought)
        };

        // Format response
        let response = serde_json::to_string_pretty(&summary)?;
        Ok(ToolResult::success(response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_think_tool_basic() {
        let tool = ThinkTool::new_standalone();

        let params = json!({
            "thought": "First, I need to understand the problem",
            "thought_number": 1,
            "total_thoughts": 3,
            "next_thought_needed": true
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
        assert!(result.output.contains("thought_number"));
    }

    #[tokio::test]
    async fn test_think_tool_with_optional_params() {
        let tool = ThinkTool::new_standalone();

        let params = json!({
            "thought": "Based on analysis, the best approach is X",
            "thought_number": 2,
            "total_thoughts": 3,
            "next_thought_needed": true,
            "thought_type": "decision",
            "confidence": 0.85
        });

        let result = tool.execute(params).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn test_thinking_tracker_history() {
        let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        let tool = ThinkTool::new(tracker.clone());

        // Record three thoughts
        for i in 1..=3 {
            let params = json!({
                "thought": format!("Thought {}", i),
                "thought_number": i,
                "total_thoughts": 3,
                "next_thought_needed": i < 3
            });
            tool.execute(params).await.unwrap();
        }

        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 3);
        assert_eq!(tracker.history()[0].thought, "Thought 1");
    }

    #[tokio::test]
    async fn test_thinking_tracker_branching() {
        let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        let tool = ThinkTool::new(tracker.clone());

        // Main branch thought
        let params = json!({
            "thought": "Main reasoning",
            "thought_number": 1,
            "total_thoughts": 2,
            "next_thought_needed": true
        });
        tool.execute(params).await.unwrap();

        // Branch thought
        let params = json!({
            "thought": "Alternative approach",
            "thought_number": 2,
            "total_thoughts": 2,
            "next_thought_needed": false,
            "branch_id": "alternative-1"
        });
        tool.execute(params).await.unwrap();

        let tracker = tracker.lock().unwrap();
        assert_eq!(tracker.len(), 2);
        assert!(tracker.branch_history("alternative-1").is_some());
        assert_eq!(tracker.branch_history("alternative-1").unwrap().len(), 1);
    }
}
```

**Verification after creating thinking.rs:**

```bash
# Check file exists
ls -la src/tools/builtin/thinking.rs
# Should show the file with ~280 lines

# Check syntax (will fail until mod.rs is created, that's OK)
# cargo check will fail here - this is EXPECTED, continue to next step
```

---

### 1.2 CREATE FILE: src/tools/builtin/memory.rs

**Full path:** `/home/dev/data/work/code/tark/src/tools/builtin/memory.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

Create a new file at `src/tools/builtin/memory.rs` with this EXACT content:

```rust
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
            Ok(ToolResult::error(format!(
                "Memory '{}' not found",
                key
            )))
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
```

---

### 1.3 CREATE FILE: src/tools/builtin/mod.rs

**Full path:** `/home/dev/data/work/code/tark/src/tools/builtin/mod.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Why this file:** This file tells Rust that `builtin` is a module and exports the types from `thinking.rs` and `memory.rs`.

Create a new file at `src/tools/builtin/mod.rs` with this EXACT content:

```rust
//! Built-in tools that extend tark's capabilities.
//!
//! These are native Rust implementations that avoid external dependencies:
//! - `thinking`: Structured step-by-step reasoning
//! - `memory`: Persistent cross-session memory storage

pub mod memory;
pub mod thinking;

// Re-export main types for convenient access
pub use memory::{
    MemoryDeleteTool, MemoryEntry, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory,
};
pub use thinking::{ThinkTool, ThinkingTracker, Thought, ThinkingSummary};
```

---

**Verification after creating mod.rs:**

```bash
# List all files in builtin directory - should show 3 files
ls -la src/tools/builtin/
# Expected output:
# thinking.rs
# memory.rs  
# mod.rs

# cargo check will STILL fail - that's OK, we need to wire it up in the next step
```

---

### 1.4 MODIFY FILE: src/tools/mod.rs

**Full path:** `/home/dev/data/work/code/tark/src/tools/mod.rs`

**Action:** MODIFY an EXISTING file (DO NOT create a new file)

**This is a MODIFICATION step, not a creation step.** You are editing an existing file.

Apply these EXACT changes to `src/tools/mod.rs`:

**CHANGE 1: Add module declaration**

FIND this line (around line 18):

```rust
pub mod write;
```

ADD immediately AFTER it:

```rust
pub mod builtin;
```

---

**CHANGE 2: Add exports**

FIND this line (around line 52):

```rust
pub use shell::ShellTool;
```

ADD immediately AFTER it:

```rust

// Built-in extra tools
pub use builtin::{
    MemoryDeleteTool, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory,
    ThinkTool, ThinkingTracker,
};
```

---

**CHANGE 3: Update imports**

FIND these lines (around line 60-63):

```rust
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::Arc;
```

REPLACE with:

```rust
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
```

---

**CHANGE 4: Add tool registration**

FIND this block in `for_mode_with_services()` function (around line 205):

```rust
        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // ===== Read-only tools (available in ALL modes) =====

        // Code exploration
        registry.register(Arc::new(CodebaseOverviewTool::new(working_dir.clone()))); // Overview (use first!)
```

REPLACE with:

```rust
        tracing::debug!("Creating tool registry for mode: {:?}", mode);

        // ===== Built-in extra tools (available in ALL modes) =====
        
        // Thinking tool - always available, helps with reasoning
        let thinking_tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
        registry.register(Arc::new(ThinkTool::new(thinking_tracker)));
        
        // Memory tools - persistent storage across sessions
        let memory_db_path = working_dir.join(".tark").join("memory.db");
        match TarkMemory::open(&memory_db_path) {
            Ok(memory) => {
                let memory = Arc::new(memory);
                registry.register(Arc::new(MemoryStoreTool::new(memory.clone())));
                registry.register(Arc::new(MemoryQueryTool::new(memory.clone())));
                registry.register(Arc::new(MemoryListTool::new(memory.clone())));
                registry.register(Arc::new(MemoryDeleteTool::new(memory)));
                tracing::debug!("Registered memory tools (db: {})", memory_db_path.display());
            }
            Err(e) => {
                tracing::warn!("Failed to initialize memory tools: {}", e);
            }
        }

        // ===== Read-only tools (available in ALL modes) =====

        // Code exploration
        registry.register(Arc::new(CodebaseOverviewTool::new(working_dir.clone()))); // Overview (use first!)
```

**CRITICAL VERIFICATION CHECKPOINT - SECTION 1 COMPLETE:**

```bash
# After completing ALL changes in Section 1.4, run:
cargo check

# EXPECTED RESULT: Should compile successfully!
# If you see errors, DO NOT proceed to Section 2.
# Go back and verify each change in Section 1.4 was applied correctly.

# Common errors at this point:
# - "cannot find module `builtin`" -> CHANGE 1 was not applied
# - "cannot find `ThinkTool`" -> CHANGE 2 was not applied  
# - "cannot find `Mutex`" -> CHANGE 3 was not applied
# - "cannot find `ThinkingTracker`" -> CHANGE 4 was not applied
```

---

## SECTION 2: MCP CLIENT

### PREREQUISITE: Create the mcp directory

BEFORE creating any files in this section, you MUST create the directory:

```bash
mkdir -p src/mcp
```

Verify the directory exists:

```bash
ls -la src/mcp
# Should show an empty directory, not "No such file or directory"
```

---

### 2.1 MODIFY FILE: Cargo.toml

**Full path:** `/home/dev/data/work/code/tark/Cargo.toml`

**Action:** MODIFY an EXISTING file (DO NOT create a new file)

**CHANGE 1: Add dependencies**

FIND the end of `[dependencies]` section (before `[dev-dependencies]`).

**How to find it:** Search for `[dev-dependencies]` in the file. The line BEFORE it is where you add the new dependencies.

**Note:** If `regex` is already in dependencies, do NOT add it again. Only add `mcp-protocol-sdk`.

ADD these lines BEFORE `[dev-dependencies]`:

```toml
# MCP Client support (optional)
mcp-protocol-sdk = { version = "0.5", optional = true, default-features = false, features = ["stdio"] }
regex = "1"
```

---

**CHANGE 2: Update features**

FIND the `[features]` section. It should look like:

```toml
[features]
default = ["test-sim"]
test-sim = ["gherkin", "tiktoken-rs", "sha2"]
```

REPLACE with:

```toml
[features]
default = ["test-sim", "mcp-client"]
test-sim = ["gherkin", "tiktoken-rs", "sha2"]
mcp-client = ["mcp-protocol-sdk"]
```

---

**Verification after Cargo.toml changes:**

```bash
# Verify the changes by checking features
grep -A3 "\[features\]" Cargo.toml
# Should show: mcp-client = ["mcp-protocol-sdk"]

# cargo check will fail here because src/mcp/ doesn't have files yet - that's OK
```

---

### 2.2 CREATE FILE: src/mcp/types.rs

**Full path:** `/home/dev/data/work/code/tark/src/mcp/types.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** Defines data structures for MCP protocol communication (ConnectionStatus, McpToolDef, etc.)

Create a new file at `src/mcp/types.rs` with this EXACT content:

```rust
//! MCP protocol types and data structures.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Status of an MCP server connection
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionStatus {
    /// Not connected
    Disconnected,
    /// Currently attempting to connect
    Connecting,
    /// Successfully connected
    Connected,
    /// Connection failed with error message
    Failed(String),
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        Self::Disconnected
    }
}

impl ConnectionStatus {
    /// Check if connected
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected)
    }

    /// Get display string
    pub fn display(&self) -> &str {
        match self {
            Self::Disconnected => "Disconnected",
            Self::Connecting => "Connecting...",
            Self::Connected => "Connected",
            Self::Failed(_) => "Failed",
        }
    }

    /// Get icon for TUI
    pub fn icon(&self) -> &str {
        match self {
            Self::Disconnected => "○",
            Self::Connecting => "◐",
            Self::Connected => "●",
            Self::Failed(_) => "✗",
        }
    }
}

/// Tool definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolDef {
    /// Tool name
    pub name: String,
    /// Tool description
    #[serde(default)]
    pub description: String,
    /// JSON Schema for input parameters
    #[serde(default, rename = "inputSchema")]
    pub input_schema: Value,
}

/// Resource definition from MCP server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResourceDef {
    /// Resource URI
    pub uri: String,
    /// Resource name
    pub name: String,
    /// Resource description
    #[serde(default)]
    pub description: String,
    /// MIME type
    #[serde(default, rename = "mimeType")]
    pub mime_type: Option<String>,
}

/// Result of a tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResult {
    /// Content returned by the tool
    pub content: Vec<McpContent>,
    /// Whether the call resulted in an error
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

/// Content item in MCP responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    /// Text content
    #[serde(rename = "text")]
    Text { text: String },
    /// Image content (base64)
    #[serde(rename = "image")]
    Image { data: String, mime_type: String },
    /// Resource reference
    #[serde(rename = "resource")]
    Resource { uri: String },
}

impl McpToolResult {
    /// Convert to string representation
    pub fn to_text(&self) -> String {
        self.content
            .iter()
            .filter_map(|c| match c {
                McpContent::Text { text } => Some(text.clone()),
                McpContent::Image { .. } => Some("[Image]".to_string()),
                McpContent::Resource { uri } => Some(format!("[Resource: {}]", uri)),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Server capabilities returned during initialization
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    /// Whether server supports tools
    #[serde(default)]
    pub tools: Option<ToolsCapability>,
    /// Whether server supports resources
    #[serde(default)]
    pub resources: Option<ResourcesCapability>,
    /// Whether server supports prompts
    #[serde(default)]
    pub prompts: Option<PromptsCapability>,
}

/// Tools capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolsCapability {
    /// Whether tool list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}

/// Resources capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourcesCapability {
    /// Whether resource list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
    /// Whether server supports subscriptions
    #[serde(default)]
    pub subscribe: bool,
}

/// Prompts capability details
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PromptsCapability {
    /// Whether prompt list can change
    #[serde(default, rename = "listChanged")]
    pub list_changed: bool,
}
```

---

### 2.3 CREATE FILE: src/mcp/transport.rs

**Full path:** `/home/dev/data/work/code/tark/src/mcp/transport.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** Handles communication with external MCP servers via stdin/stdout (STDIO transport)

Create a new file at `src/mcp/transport.rs` with this EXACT content:

```rust
//! MCP transport implementations.
//!
//! Supports:
//! - STDIO: Spawn a child process and communicate via stdin/stdout
//! - HTTP/SSE: Connect to HTTP endpoints (future)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// JSON-RPC request
#[derive(Debug, Clone, Serialize)]
struct JsonRpcRequest {
    jsonrpc: &'static str,
    id: u64,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

/// JSON-RPC response
#[derive(Debug, Clone, Deserialize)]
struct JsonRpcResponse {
    #[allow(dead_code)]
    jsonrpc: String,
    #[allow(dead_code)]
    id: Option<u64>,
    result: Option<Value>,
    error: Option<JsonRpcError>,
}

/// JSON-RPC error
#[derive(Debug, Clone, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[allow(dead_code)]
    data: Option<Value>,
}

/// STDIO transport for MCP servers
pub struct StdioTransport {
    /// Child process
    child: Arc<Mutex<Child>>,
    /// Request ID counter
    next_id: AtomicU64,
    /// Stdin writer
    stdin: Arc<Mutex<std::process::ChildStdin>>,
    /// Stdout reader  
    stdout: Arc<Mutex<BufReader<std::process::ChildStdout>>>,
}

impl StdioTransport {
    /// Spawn a new MCP server process
    pub fn spawn(
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<&PathBuf>,
    ) -> Result<Self> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Pass stderr through for debugging

        // Set environment variables (expand ${VAR} references)
        for (key, value) in env {
            let expanded = expand_env_vars(value);
            cmd.env(key, expanded);
        }

        // Set working directory if specified
        if let Some(dir) = working_dir {
            cmd.current_dir(dir);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("Failed to spawn MCP server: {}", command))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get stdout"))?;

        Ok(Self {
            child: Arc::new(Mutex::new(child)),
            next_id: AtomicU64::new(1),
            stdin: Arc::new(Mutex::new(stdin)),
            stdout: Arc::new(Mutex::new(BufReader::new(stdout))),
        })
    }

    /// Send a request and wait for response
    pub fn request(&self, method: &str, params: Option<Value>) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let request = JsonRpcRequest {
            jsonrpc: "2.0",
            id,
            method: method.to_string(),
            params,
        };

        // Serialize and send
        let request_str = serde_json::to_string(&request)?;
        tracing::debug!("MCP request: {}", request_str);

        {
            let mut stdin = self.stdin.lock().unwrap();
            writeln!(stdin, "{}", request_str)?;
            stdin.flush()?;
        }

        // Read response
        let response: JsonRpcResponse = {
            let mut stdout = self.stdout.lock().unwrap();
            let mut line = String::new();
            stdout.read_line(&mut line)?;
            tracing::debug!("MCP response: {}", line.trim());
            serde_json::from_str(&line)?
        };

        // Handle response
        if let Some(error) = response.error {
            return Err(anyhow::anyhow!(
                "MCP error {}: {}",
                error.code,
                error.message
            ));
        }

        response
            .result
            .ok_or_else(|| anyhow::anyhow!("MCP response missing result"))
    }

    /// Send a notification (no response expected)
    pub fn notify(&self, method: &str, params: Option<Value>) -> Result<()> {
        #[derive(Serialize)]
        struct JsonRpcNotification {
            jsonrpc: &'static str,
            method: String,
            #[serde(skip_serializing_if = "Option::is_none")]
            params: Option<Value>,
        }

        let notification = JsonRpcNotification {
            jsonrpc: "2.0",
            method: method.to_string(),
            params,
        };

        let notification_str = serde_json::to_string(&notification)?;
        tracing::debug!("MCP notification: {}", notification_str);

        let mut stdin = self.stdin.lock().unwrap();
        writeln!(stdin, "{}", notification_str)?;
        stdin.flush()?;

        Ok(())
    }

    /// Check if the child process is still running
    pub fn is_alive(&self) -> bool {
        let mut child = self.child.lock().unwrap();
        match child.try_wait() {
            Ok(None) => true,  // Still running
            Ok(Some(_)) => false, // Exited
            Err(_) => false,   // Error checking
        }
    }

    /// Kill the child process
    pub fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().unwrap();
        child.kill().context("Failed to kill MCP server")?;
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        // Try to gracefully terminate
        let _ = self.kill();
    }
}

/// Expand environment variable references like ${VAR} in a string
fn expand_env_vars(input: &str) -> String {
    let mut result = input.to_string();
    
    // Find all ${VAR} patterns
    let re = regex::Regex::new(r"\$\{([^}]+)\}").unwrap();
    
    for cap in re.captures_iter(input) {
        let var_name = &cap[1];
        if let Ok(value) = std::env::var(var_name) {
            result = result.replace(&cap[0], &value);
        }
    }
    
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars() {
        std::env::set_var("TEST_VAR", "hello");
        assert_eq!(expand_env_vars("${TEST_VAR} world"), "hello world");
        assert_eq!(expand_env_vars("no vars here"), "no vars here");
        assert_eq!(expand_env_vars("${NONEXISTENT}"), "${NONEXISTENT}");
    }
}
```

---

### 2.4 CREATE FILE: src/mcp/client.rs

**Full path:** `/home/dev/data/work/code/tark/src/mcp/client.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** McpServerManager - manages connections to multiple MCP servers, handles connect/disconnect, tool discovery

Create a new file at `src/mcp/client.rs` with this EXACT content:

```rust
//! MCP client for connecting to external MCP servers.

use super::transport::StdioTransport;
use super::types::{ConnectionStatus, McpToolDef, McpToolResult, ServerCapabilities};
use crate::storage::{McpConfig, McpServer};
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

/// Information about a connected MCP server
pub struct McpServerConnection {
    /// Server configuration
    pub config: McpServer,
    /// Connection status
    pub status: ConnectionStatus,
    /// Transport (if connected)
    transport: Option<StdioTransport>,
    /// Discovered tools
    pub tools: Vec<McpToolDef>,
    /// Server capabilities
    pub capabilities: ServerCapabilities,
}

impl McpServerConnection {
    /// Create a new disconnected connection
    pub fn new(config: McpServer) -> Self {
        Self {
            config,
            status: ConnectionStatus::Disconnected,
            transport: None,
            tools: Vec::new(),
            capabilities: ServerCapabilities::default(),
        }
    }
}

/// Manages connections to multiple MCP servers
pub struct McpServerManager {
    /// Server connections by ID
    connections: RwLock<HashMap<String, McpServerConnection>>,
    /// Data directory for downloads
    data_dir: PathBuf,
    /// Working directory for spawned processes
    working_dir: PathBuf,
}

impl McpServerManager {
    /// Create a new manager
    pub fn new(data_dir: PathBuf, working_dir: PathBuf) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            data_dir,
            working_dir,
        }
    }

    /// Load server configurations
    pub fn load_config(&self, config: &McpConfig) {
        let mut connections = self.connections.write().unwrap();
        for (id, server_config) in &config.servers {
            if server_config.enabled {
                connections.insert(id.clone(), McpServerConnection::new(server_config.clone()));
            }
        }
    }

    /// Get list of configured server IDs
    pub fn server_ids(&self) -> Vec<String> {
        self.connections.read().unwrap().keys().cloned().collect()
    }

    /// Get connection status for a server
    pub fn status(&self, server_id: &str) -> Option<ConnectionStatus> {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.status.clone())
    }

    /// Get all tools from a connected server
    pub fn tools(&self, server_id: &str) -> Vec<McpToolDef> {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.tools.clone())
            .unwrap_or_default()
    }

    /// Get all tools from all connected servers
    pub fn all_tools(&self) -> Vec<(String, McpToolDef)> {
        let connections = self.connections.read().unwrap();
        let mut tools = Vec::new();
        for (server_id, conn) in connections.iter() {
            if conn.status.is_connected() {
                for tool in &conn.tools {
                    tools.push((server_id.clone(), tool.clone()));
                }
            }
        }
        tools
    }

    /// Connect to a server
    pub fn connect(&self, server_id: &str) -> Result<()> {
        let config = {
            let connections = self.connections.read().unwrap();
            connections
                .get(server_id)
                .map(|c| c.config.clone())
                .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?
        };

        // Update status to connecting
        {
            let mut connections = self.connections.write().unwrap();
            if let Some(conn) = connections.get_mut(server_id) {
                conn.status = ConnectionStatus::Connecting;
            }
        }

        // Spawn the transport
        let transport = match StdioTransport::spawn(
            &config.command,
            &config.args,
            &config.env,
            Some(&self.working_dir),
        ) {
            Ok(t) => t,
            Err(e) => {
                let mut connections = self.connections.write().unwrap();
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Initialize the connection
        let init_result = transport.request(
            "initialize",
            Some(json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "tark",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        );

        let capabilities = match init_result {
            Ok(result) => {
                // Send initialized notification
                let _ = transport.notify("notifications/initialized", None);

                // Parse capabilities
                result
                    .get("capabilities")
                    .cloned()
                    .and_then(|c| serde_json::from_value(c).ok())
                    .unwrap_or_default()
            }
            Err(e) => {
                let mut connections = self.connections.write().unwrap();
                if let Some(conn) = connections.get_mut(server_id) {
                    conn.status = ConnectionStatus::Failed(e.to_string());
                }
                return Err(e);
            }
        };

        // Discover tools
        let tools = if capabilities.tools.is_some() {
            match transport.request("tools/list", None) {
                Ok(result) => result
                    .get("tools")
                    .and_then(|t| serde_json::from_value(t.clone()).ok())
                    .unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to list tools: {}", e);
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Update connection
        {
            let mut connections = self.connections.write().unwrap();
            if let Some(conn) = connections.get_mut(server_id) {
                conn.transport = Some(transport);
                conn.capabilities = capabilities;
                conn.tools = tools;
                conn.status = ConnectionStatus::Connected;
            }
        }

        tracing::info!("Connected to MCP server: {}", server_id);
        Ok(())
    }

    /// Disconnect from a server
    pub fn disconnect(&self, server_id: &str) -> Result<()> {
        let mut connections = self.connections.write().unwrap();
        if let Some(conn) = connections.get_mut(server_id) {
            if let Some(transport) = conn.transport.take() {
                let _ = transport.kill();
            }
            conn.status = ConnectionStatus::Disconnected;
            conn.tools.clear();
            tracing::info!("Disconnected from MCP server: {}", server_id);
        }
        Ok(())
    }

    /// Call a tool on a server
    pub fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: Value,
    ) -> Result<McpToolResult> {
        let connections = self.connections.read().unwrap();
        let conn = connections
            .get(server_id)
            .ok_or_else(|| anyhow::anyhow!("Unknown server: {}", server_id))?;

        if !conn.status.is_connected() {
            return Err(anyhow::anyhow!("Server not connected: {}", server_id));
        }

        let transport = conn
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("No transport for server: {}", server_id))?;

        let result = transport
            .request(
                "tools/call",
                Some(json!({
                    "name": tool_name,
                    "arguments": arguments
                })),
            )
            .with_context(|| format!("Failed to call tool: {}", tool_name))?;

        serde_json::from_value(result).context("Failed to parse tool result")
    }

    /// Check if a server is connected
    pub fn is_connected(&self, server_id: &str) -> bool {
        self.connections
            .read()
            .unwrap()
            .get(server_id)
            .map(|c| c.status.is_connected())
            .unwrap_or(false)
    }

    /// Get data directory
    pub fn data_dir(&self) -> &PathBuf {
        &self.data_dir
    }
}

impl Drop for McpServerManager {
    fn drop(&mut self) {
        // Disconnect all servers
        let server_ids: Vec<String> = self.server_ids();
        for id in server_ids {
            let _ = self.disconnect(&id);
        }
    }
}
```

---

### 2.5 CREATE FILE: src/mcp/wrapper.rs

**Full path:** `/home/dev/data/work/code/tark/src/mcp/wrapper.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** McpToolWrapper - adapts MCP tools to tark's Tool trait so they can be used like native tools

Create a new file at `src/mcp/wrapper.rs` with this EXACT content:

```rust
//! Wrapper that adapts MCP tools to tark's Tool trait.

use super::client::McpServerManager;
use super::types::McpToolDef;
use crate::tools::{RiskLevel, Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

/// Default timeout for MCP tool calls (30 seconds)
const DEFAULT_TIMEOUT_SECS: u64 = 30;

/// Wraps an MCP tool to implement tark's Tool trait
pub struct McpToolWrapper {
    /// Server ID this tool belongs to
    server_id: String,
    /// Tool definition from MCP server
    tool_def: McpToolDef,
    /// Reference to the manager for making calls
    manager: Arc<McpServerManager>,
    /// Risk level for this tool
    risk_level: RiskLevel,
    /// Timeout for tool calls
    #[allow(dead_code)]
    timeout_secs: u64,
    /// Optional namespace prefix for tool name
    #[allow(dead_code)]
    namespace: Option<String>,
}

impl McpToolWrapper {
    /// Create a new wrapper
    pub fn new(
        server_id: String,
        tool_def: McpToolDef,
        manager: Arc<McpServerManager>,
    ) -> Self {
        Self {
            server_id,
            tool_def,
            manager,
            risk_level: RiskLevel::Risky, // Default to risky for external tools
            timeout_secs: DEFAULT_TIMEOUT_SECS,
            namespace: None,
        }
    }

    /// Set the risk level
    pub fn with_risk_level(mut self, level: RiskLevel) -> Self {
        self.risk_level = level;
        self
    }

    /// Set the timeout
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Set a namespace prefix for the tool name
    pub fn with_namespace(mut self, namespace: String) -> Self {
        self.namespace = Some(namespace);
        self
    }
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &str {
        &self.tool_def.name
    }

    fn description(&self) -> &str {
        &self.tool_def.description
    }

    fn parameters(&self) -> Value {
        self.tool_def.input_schema.clone()
    }

    fn risk_level(&self) -> RiskLevel {
        self.risk_level
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        // Check if server is connected
        if !self.manager.is_connected(&self.server_id) {
            return Ok(ToolResult::error(format!(
                "MCP server '{}' is not connected. Use /mcp connect {} first.",
                self.server_id, self.server_id
            )));
        }

        // Call the tool
        match self.manager.call_tool(&self.server_id, &self.tool_def.name, params) {
            Ok(result) => {
                if result.is_error {
                    Ok(ToolResult::error(result.to_text()))
                } else {
                    Ok(ToolResult::success(result.to_text()))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("MCP call failed: {}", e))),
        }
    }

    fn get_command_string(&self, params: &Value) -> String {
        format!("mcp:{}:{} {}", self.server_id, self.tool_def.name, params)
    }
}

/// Create tool wrappers for all tools from a connected server
pub fn wrap_server_tools(
    server_id: &str,
    manager: Arc<McpServerManager>,
    risk_level: Option<RiskLevel>,
    timeout_secs: Option<u64>,
    namespace: Option<String>,
) -> Vec<Arc<dyn Tool>> {
    let tools = manager.tools(server_id);
    
    tools
        .into_iter()
        .map(|tool_def| {
            let mut wrapper = McpToolWrapper::new(
                server_id.to_string(),
                tool_def,
                manager.clone(),
            );
            
            if let Some(level) = risk_level {
                wrapper = wrapper.with_risk_level(level);
            }
            if let Some(secs) = timeout_secs {
                wrapper = wrapper.with_timeout(secs);
            }
            if let Some(ref ns) = namespace {
                wrapper = wrapper.with_namespace(ns.clone());
            }
            
            Arc::new(wrapper) as Arc<dyn Tool>
        })
        .collect()
}
```

---

### 2.6 CREATE FILE: src/mcp/mod.rs

**Full path:** `/home/dev/data/work/code/tark/src/mcp/mod.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** Module definition - tells Rust about the mcp module and re-exports public types

**This MUST be the LAST file created in src/mcp/** because it references all the other files.

Create a new file at `src/mcp/mod.rs` with this EXACT content:

```rust
//! MCP (Model Context Protocol) client implementation.
//!
//! Enables tark to connect to external MCP servers and use their tools.

#![allow(dead_code)]

pub mod client;
pub mod transport;
pub mod types;
pub mod wrapper;

// Re-export main types
pub use client::McpServerManager;
pub use transport::StdioTransport;
pub use types::{ConnectionStatus, McpContent, McpToolDef, McpToolResult, ServerCapabilities};
pub use wrapper::{wrap_server_tools, McpToolWrapper};
```

**Verification after creating all mcp files:**

```bash
# List all files in mcp directory - should show 5 files
ls -la src/mcp/
# Expected output:
# types.rs
# transport.rs
# client.rs
# wrapper.rs
# mod.rs

# cargo check will STILL fail - that's OK, we need to wire it up in lib.rs
```

---

### 2.7 MODIFY FILE: src/storage/mod.rs

**Full path:** `/home/dev/data/work/code/tark/src/storage/mod.rs`

**Action:** MODIFY an EXISTING file (DO NOT create a new file)

**Purpose:** Add McpServerTarkConfig struct for tark-specific MCP configuration options

**How to find the location:** Search for `pub struct McpServer` in the file. It should be around line 1289.

FIND this struct (around line 1289-1308):

```rust
/// MCP server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Server name/description
    pub name: String,
    /// Command to run the server
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether the server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Server capabilities/tools it provides
    #[serde(default)]
    pub capabilities: Vec<String>,
}
```

REPLACE with:

```rust
/// MCP server definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServer {
    /// Server name/description
    pub name: String,
    /// Command to run the server
    pub command: String,
    /// Command arguments
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Whether the server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Server capabilities/tools it provides
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Tark-specific configuration
    #[serde(default)]
    pub tark: Option<McpServerTarkConfig>,
}

/// Tark-specific MCP server configuration
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpServerTarkConfig {
    /// Override risk level for all tools from this server
    #[serde(default)]
    pub risk_level: Option<String>,
    /// Auto-connect on startup
    #[serde(default)]
    pub auto_connect: bool,
    /// Timeout in seconds for tool calls
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
    /// Namespace prefix for tool names (e.g., "gh" -> "gh:create_issue")
    #[serde(default)]
    pub namespace: Option<String>,
}

fn default_timeout() -> u32 {
    30
}
```

---

### 2.8 MODIFY FILE: src/lib.rs

**Full path:** `/home/dev/data/work/code/tark/src/lib.rs`

**Action:** MODIFY an EXISTING file (DO NOT create a new file)

**Purpose:** Register the mcp module with the main library (behind a feature flag)

**How to find the location:** Search for `pub mod tools;` - the change goes right after that line.

FIND these lines (around line 22-23):

```rust
pub mod tools;
pub mod transport;
```

REPLACE with:

```rust
pub mod tools;
pub mod transport;

// MCP client (behind feature flag)
#[cfg(feature = "mcp-client")]
pub mod mcp;
```

**CRITICAL VERIFICATION CHECKPOINT - SECTION 2 COMPLETE:**

```bash
# After completing ALL changes in Section 2.8, run:
cargo check

# EXPECTED RESULT: Should compile successfully!
# If you see errors, DO NOT proceed to Section 3.

# Common errors at this point:
# - "cannot find module `mcp`" -> Section 2.8 change was not applied
# - "unresolved import `super::transport`" -> One of the mcp files has wrong imports
# - "cannot find type `McpServerTarkConfig`" -> Section 2.7 change was not applied
```

---

## SECTION 3: TESTING AND DOCUMENTATION

### 3.1 CREATE FILE: tests/builtin_tools_test.rs

**Full path:** `/home/dev/data/work/code/tark/tests/builtin_tools_test.rs`

**Action:** Create a NEW file (this file should NOT exist yet)

**Purpose:** Integration tests for the thinking and memory tools

**IMPORTANT:** The crate name in imports is `tark_cli` (not `tark`). This is defined in Cargo.toml `[lib] name = "tark_cli"`.

Create a new file at `tests/builtin_tools_test.rs` with this content:

```rust
//! Tests for built-in tools (thinking and memory).

use serde_json::json;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;

// Import from tark
use tark_cli::tools::builtin::{
    MemoryDeleteTool, MemoryListTool, MemoryQueryTool, MemoryStoreTool, TarkMemory,
    ThinkTool, ThinkingTracker,
};
use tark_cli::tools::Tool;

#[tokio::test]
async fn test_think_tool_basic_flow() {
    let tracker = Arc::new(Mutex::new(ThinkingTracker::new()));
    let tool = ThinkTool::new(tracker.clone());

    let result = tool
        .execute(json!({
            "thought": "Let me analyze the problem",
            "thought_number": 1,
            "total_thoughts": 3,
            "next_thought_needed": true
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert!(result.output.contains("thought_number"));
}

#[tokio::test]
async fn test_memory_store_and_query() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test_memory.db");
    let memory = Arc::new(TarkMemory::open(&db_path).unwrap());

    let store_tool = MemoryStoreTool::new(memory.clone());
    let query_tool = MemoryQueryTool::new(memory.clone());

    // Store a memory
    let result = store_tool
        .execute(json!({
            "key": "database_choice",
            "value": "We decided to use PostgreSQL",
            "category": "decision",
            "importance": 8
        }))
        .await
        .unwrap();

    assert!(result.success);

    // Query it back
    let result = query_tool
        .execute(json!({
            "search": "PostgreSQL"
        }))
        .await
        .unwrap();

    assert!(result.success);
    assert!(result.output.contains("database_choice"));
}
```

---

### 3.2 UPDATE FILE: examples/tark-config/mcp/servers.toml

**Full path:** `/home/dev/data/work/code/tark/examples/tark-config/mcp/servers.toml`

**Action:** REPLACE the ENTIRE contents of an EXISTING file

**Purpose:** Provide example configuration for users to configure their own MCP servers

Replace the entire file with:

```toml
# MCP (Model Context Protocol) Server Configuration
#
# tark can connect to external MCP servers that you install and manage.
# Place this file in:
#   - ~/.config/tark/mcp/servers.toml (global)
#   - .tark/mcp/servers.toml (project-specific)
#
# USER RESPONSIBILITY:
#   - Install MCP servers (via npm, pip, cargo, etc.)
#   - Manage server binaries and updates
#   - Set required environment variables (API keys, etc.)
#
# TARK RESPONSIBILITY:
#   - Load configuration from this file
#   - Connect to configured servers
#   - Discover and expose tools to the agent
#   - Enforce risk levels and approval gates

# ==============================================================================
# EXAMPLE: GitHub MCP Server (npm-based)
# Install: npm install -g @modelcontextprotocol/server-github
# ==============================================================================
[servers.github]
name = "GitHub Integration"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
enabled = false  # Set to true after installing and setting GITHUB_TOKEN
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }
capabilities = ["create_issue", "list_issues", "create_pr", "search_repos"]

[servers.github.tark]
risk_level = "risky"
auto_connect = false
timeout_seconds = 30
namespace = "gh"

# ==============================================================================
# EXAMPLE: Brave Search MCP Server
# Install: npm install -g @anthropics/mcp-server-brave-search
# ==============================================================================
[servers.brave_search]
name = "Brave Search"
command = "npx"
args = ["-y", "@anthropics/mcp-server-brave-search"]
enabled = false  # Set to true after setting BRAVE_API_KEY
env = { BRAVE_API_KEY = "${BRAVE_API_KEY}" }
capabilities = ["web_search"]

[servers.brave_search.tark]
risk_level = "readonly"
auto_connect = false
timeout_seconds = 15

# ==============================================================================
# EXAMPLE: Filesystem MCP Server
# Install: npm install -g @modelcontextprotocol/server-filesystem
# ==============================================================================
[servers.filesystem]
name = "Filesystem Tools"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/allowed/dir"]
enabled = false
capabilities = ["read_file", "write_file", "list_directory"]

[servers.filesystem.tark]
risk_level = "write"
timeout_seconds = 10

# ==============================================================================
# EXAMPLE: PostgreSQL MCP Server
# Install: npm install -g @modelcontextprotocol/server-postgres
# ==============================================================================
[servers.postgres]
name = "PostgreSQL"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-postgres"]
enabled = false
env = { DATABASE_URL = "postgresql://user:pass@localhost/mydb" }
capabilities = ["query", "list_tables", "describe_table"]

[servers.postgres.tark]
risk_level = "risky"
timeout_seconds = 60

# ==============================================================================
# TEMPLATE: Custom MCP Server
# Copy and modify this template for your own MCP servers
# ==============================================================================
# [servers.my_custom]
# name = "My Custom Tools"
# command = "/path/to/my-mcp-server"
# args = ["--config", "config.json"]
# enabled = false
# env = { MY_API_KEY = "${MY_API_KEY}" }
# capabilities = ["tool1", "tool2"]
#
# [servers.my_custom.tark]
# risk_level = "risky"  # Options: readonly, write, risky, dangerous
# auto_connect = false  # Set true to connect on startup
# timeout_seconds = 30
# namespace = "custom"  # Optional: prefix tool names (e.g., "custom:tool1")
```

---

## FINAL VERIFICATION CHECKLIST

After implementing ALL changes, run these commands IN ORDER:

```bash
# STEP 1: Verify all files exist
echo "=== Checking file structure ==="
ls -la src/tools/builtin/
# Expected: thinking.rs, memory.rs, mod.rs (3 files)

ls -la src/mcp/
# Expected: types.rs, transport.rs, client.rs, wrapper.rs, mod.rs (5 files)

ls -la tests/builtin_tools_test.rs
# Expected: shows the test file

# STEP 2: Format code (fixes style issues)
echo "=== Formatting ==="
cargo fmt --all

# STEP 3: Check compilation
echo "=== Checking compilation ==="
cargo check
# MUST pass with no errors

# STEP 4: Run clippy lints
echo "=== Running clippy ==="
cargo clippy --all-targets --all-features -- -D warnings
# MUST pass with no warnings

# STEP 5: Run all tests
echo "=== Running tests ==="
cargo test --all-features
# MUST pass all tests

# STEP 6: Run specific builtin tools tests
echo "=== Running builtin tools tests ==="
cargo test --test builtin_tools_test
# MUST pass all tests

# STEP 7: Build release binary
echo "=== Building release ==="
cargo build --release
# MUST succeed

echo "=== ALL CHECKS PASSED ==="
```

**If ANY step fails, DO NOT commit.** Fix the issue first.

---

## TROUBLESHOOTING

### Error: "cannot find module `builtin`"

**Cause:** The `pub mod builtin;` line was not added to `src/tools/mod.rs`

**Fix:**

1. Open `src/tools/mod.rs`
2. Find the line `pub mod write;`
3. Add `pub mod builtin;` immediately after it
4. Run `cargo check` again

### Error: "cannot find type `ThinkTool` in module `builtin`"

**Cause:** The exports in Section 1.4 CHANGE 2 were not added

**Fix:**

1. Open `src/tools/mod.rs`
2. Find the line `pub use shell::ShellTool;`
3. Add the `pub use builtin::{...}` block after it
4. Run `cargo check` again

### Error: "cannot find value `Mutex` in `std::sync`"

**Cause:** The imports in Section 1.4 CHANGE 3 were not updated

**Fix:**

1. Open `src/tools/mod.rs`
2. Find `use std::sync::Arc;`
3. Change it to `use std::sync::{Arc, Mutex};`
4. Run `cargo check` again

### Error: "cannot find module `mcp`"

**Cause:** The lib.rs modification in Section 2.8 was not applied

**Fix:**

1. Open `src/lib.rs`
2. Find `pub mod tools;` and `pub mod transport;`
3. Add the mcp module lines after transport
4. Run `cargo check` again

### Error: "unresolved import `super::transport`" in mcp files

**Cause:** The src/mcp/mod.rs file is missing or has wrong content

**Fix:**

1. Verify `src/mcp/mod.rs` exists
2. Check it has all 4 `pub mod` declarations
3. Run `cargo check` again

### Error: "cannot find type `McpServerTarkConfig`"

**Cause:** The storage/mod.rs modification in Section 2.7 was not applied

**Fix:**

1. Open `src/storage/mod.rs`
2. Find `pub struct McpServer`
3. Add the `McpServerTarkConfig` struct and `default_timeout` function after it
4. Run `cargo check` again

### Error: "use of undeclared crate or module `tark_cli`" in tests

**Cause:** The crate name is wrong in test imports

**Fix:** The crate is named `tark_cli` (check Cargo.toml `[lib] name`). Make sure tests use `tark_cli::` not `tark::`

### Error: "Memory tool fails to create database"

**Cause:** The `.tark` directory cannot be created

**Fix:**

1. Check file system permissions in the working directory
2. The memory tool creates `.tark/memory.db` automatically
3. Ensure the parent directory is writable

### Tests fail with "connection refused" or similar

**Cause:** MCP server tests require external servers to be running

**Note:** The MCP client tests only test the builtin tools by default. If testing MCP connections, ensure the external server is installed and running.