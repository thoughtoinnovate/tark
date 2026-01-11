# Steering Files Feature Implementation Plan

## Overview
Add auto-discovery of project steering files (AGENTS.md, .cursorrules, CLAUDE.md) with on-demand loading via `read_steering_files` tool and optional enforcement before write operations.

---

## Pre-Implementation Checklist

```bash
# 1. Ensure you're on a clean branch
cd /home/dev/data/work/code/tark
git checkout -b feature/steering-files

# 2. Verify build works before changes
cargo build --release
cargo test --all-features
```

---

## Step 1: Add SteeringConfig to WorkspaceConfig

**File:** `src/storage/mod.rs`

**Action:** Find `pub struct WorkspaceConfig` and add the steering field. Then add the `SteeringConfig` struct.

### 1.1 Find this block (around line 1082-1101):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceConfig {
    /// Preferred LLM provider for this workspace
    pub provider: String,
    /// Preferred model (provider/model format)
    pub model: Option<String>,
    /// Default agent mode
    pub default_mode: String,
    /// Enable thinking/verbose mode by default
    pub verbose: bool,
    /// Custom instructions to prepend to system prompt
    pub custom_instructions: Option<String>,
    /// Files/patterns to always ignore
    pub ignore_patterns: Vec<String>,
    /// Auto-save conversations
    pub auto_save_conversations: bool,
    /// Maximum context tokens before auto-compact
    pub max_context_tokens: Option<usize>,
}
```

### 1.2 Add this field to WorkspaceConfig:

```rust
    /// Steering files configuration
    #[serde(default)]
    pub steering: SteeringConfig,
```

### 1.3 Add SteeringConfig struct BEFORE WorkspaceConfig:

```rust
/// Configuration for steering files (AGENTS.md, .cursorrules, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SteeringConfig {
    /// Maximum tokens for steering file content (default: 8000)
    pub max_tokens: usize,
    /// Auto-discovery enabled (default: true)
    pub auto_discover: bool,
    /// Block write tools until steering files read (default: true when files exist)
    pub enforce_before_writes: bool,
    /// Additional file patterns to discover
    pub additional_patterns: Vec<String>,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        Self {
            max_tokens: 8000,
            auto_discover: true,
            enforce_before_writes: true,
            additional_patterns: vec![],
        }
    }
}
```

### 1.4 Update WorkspaceConfig Default impl to include steering:

Find the `impl Default for WorkspaceConfig` block and add:

```rust
            steering: SteeringConfig::default(),
```

### 1.5 Verify Step 1:

```bash
cargo build --release
```

### 1.6 Commit Step 1:

```bash
git add src/storage/mod.rs
git commit -m "feat(steering): add SteeringConfig to WorkspaceConfig"
```

---

## Step 2: Create Steering Discovery Module

**File:** `src/agent/steering.rs` (NEW FILE)

**Action:** Create new file with the following content:

```rust
//! Steering file discovery and management
//!
//! Discovers project steering files (AGENTS.md, .cursorrules, CLAUDE.md, etc.)
//! and provides them to the agent via the read_steering_files tool.

#![allow(dead_code)]

use crate::storage::SteeringConfig;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Steering file discovery patterns (priority order, highest first)
const STEERING_PATTERNS: &[&str] = &[
    ".tark/instructions.md",
    ".cursorrules",
    "AGENTS.md",
    "agents.md",
    "CLAUDE.md",
    "claude.md",
    "CONTRIBUTING.md",
    "contributing.md",
];

/// Discovered steering files with ephemeral caching
pub struct SteeringContext {
    /// Working directory
    working_dir: PathBuf,
    /// Discovered files in priority order (path relative to working_dir)
    files: Vec<PathBuf>,
    /// Cached file contents (loaded lazily on first read)
    contents: HashMap<PathBuf, String>,
    /// Total estimated tokens across all files
    estimated_tokens: usize,
    /// Whether steering files have been read this session
    has_been_read: AtomicBool,
    /// Max tokens config
    max_tokens: usize,
}

impl SteeringContext {
    /// Discover steering files in workspace (does NOT read contents yet)
    pub fn discover(working_dir: &Path, config: &SteeringConfig) -> Self {
        let mut files = Vec::new();

        if !config.auto_discover {
            return Self {
                working_dir: working_dir.to_path_buf(),
                files,
                contents: HashMap::new(),
                estimated_tokens: 0,
                has_been_read: AtomicBool::new(false),
                max_tokens: config.max_tokens,
            };
        }

        // Check standard patterns
        for pattern in STEERING_PATTERNS {
            let path = working_dir.join(pattern);
            if path.exists() && path.is_file() {
                files.push(PathBuf::from(pattern));
            }
        }

        // Check additional patterns from config
        for pattern in &config.additional_patterns {
            let path = working_dir.join(pattern);
            if path.exists() && path.is_file() {
                let rel = PathBuf::from(pattern);
                if !files.contains(&rel) {
                    files.push(rel);
                }
            }
        }

        if !files.is_empty() {
            tracing::info!("Discovered steering files: {:?}", files);
        }

        Self {
            working_dir: working_dir.to_path_buf(),
            files,
            contents: HashMap::new(),
            estimated_tokens: 0,
            has_been_read: AtomicBool::new(false),
            max_tokens: config.max_tokens,
        }
    }

    /// Check if any steering files were discovered
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    /// Get list of discovered file names (for system prompt)
    pub fn file_names(&self) -> Vec<String> {
        self.files
            .iter()
            .filter_map(|p| p.to_str())
            .map(|s| s.to_string())
            .collect()
    }

    /// Check if steering files have been read this session
    pub fn has_been_read(&self) -> bool {
        self.has_been_read.load(Ordering::SeqCst)
    }

    /// Mark steering files as read
    pub fn mark_as_read(&self) {
        self.has_been_read.store(true, Ordering::SeqCst);
    }

    /// Get max tokens setting
    pub fn max_tokens(&self) -> usize {
        self.max_tokens
    }

    /// Load and return all steering file contents (with token limit)
    pub fn load_contents(&mut self) -> String {
        self.has_been_read.store(true, Ordering::SeqCst);

        let mut output = String::new();
        let mut total_tokens = 0;
        let mut truncated = false;
        let mut files_loaded = 0;

        output.push_str(
            "═══════════════════════════════════════════════════════════\n",
        );
        output.push_str("📋 PROJECT STEERING FILES\n");
        output.push_str(
            "═══════════════════════════════════════════════════════════\n\n",
        );

        for file_path in &self.files.clone() {
            let full_path = self.working_dir.join(file_path);

            // Load from cache or disk
            let content = if let Some(cached) = self.contents.get(file_path) {
                cached.clone()
            } else {
                match std::fs::read_to_string(&full_path) {
                    Ok(c) => {
                        self.contents.insert(file_path.clone(), c.clone());
                        c
                    }
                    Err(e) => {
                        tracing::warn!("Failed to read steering file {:?}: {}", file_path, e);
                        continue;
                    }
                }
            };

            let file_tokens = content.len() / 4; // ~4 chars per token estimate

            // Check token budget
            if total_tokens + file_tokens > self.max_tokens {
                let remaining = self.max_tokens.saturating_sub(total_tokens);
                if remaining > 100 {
                    // Only include if we can fit meaningful content
                    let truncated_content = truncate_at_char_boundary(&content, remaining * 4);
                    output.push_str(&format!("──── {} ────\n", file_path.display()));
                    output.push_str(truncated_content);
                    output.push_str("\n\n[TRUNCATED: Token limit reached]\n\n");
                    files_loaded += 1;
                }
                truncated = true;
                break;
            }

            output.push_str(&format!("──── {} ────\n", file_path.display()));
            output.push_str(&content);
            output.push_str("\n\n");
            total_tokens += file_tokens;
            files_loaded += 1;
        }

        self.estimated_tokens = total_tokens;

        output.push_str(
            "═══════════════════════════════════════════════════════════\n",
        );
        output.push_str(&format!(
            "Total: {} file(s), ~{} tokens\n",
            files_loaded, total_tokens
        ));
        if truncated {
            output.push_str(&format!(
                "⚠️ Content truncated. Increase steering.max_tokens in .tark/config.toml (current: {})\n",
                self.max_tokens
            ));
        }
        output.push_str(
            "═══════════════════════════════════════════════════════════\n",
        );

        output
    }
}

/// UTF-8 safe truncation
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_discovers_agents_md() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "# Guidelines").unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);

        assert!(ctx.has_files());
        assert!(ctx.file_names().contains(&"AGENTS.md".to_string()));
    }

    #[test]
    fn test_discovers_cursorrules() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".cursorrules"), "rules here").unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);

        assert!(ctx.has_files());
        assert!(ctx.file_names().contains(&".cursorrules".to_string()));
    }

    #[test]
    fn test_no_files_graceful() {
        let tmp = TempDir::new().unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);

        assert!(!ctx.has_files());
        assert!(ctx.file_names().is_empty());
    }

    #[test]
    fn test_priority_order() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "agents").unwrap();
        fs::write(tmp.path().join(".cursorrules"), "cursor").unwrap();
        fs::write(tmp.path().join("CONTRIBUTING.md"), "contrib").unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);

        let names = ctx.file_names();
        // .cursorrules should come before AGENTS.md in priority
        let cursor_pos = names.iter().position(|n| n == ".cursorrules");
        let agents_pos = names.iter().position(|n| n == "AGENTS.md");
        assert!(cursor_pos < agents_pos);
    }

    #[test]
    fn test_token_truncation() {
        let tmp = TempDir::new().unwrap();
        // Create a file larger than default max_tokens
        let large_content = "x".repeat(50000); // ~12500 tokens
        fs::write(tmp.path().join("AGENTS.md"), &large_content).unwrap();

        let config = SteeringConfig {
            max_tokens: 1000,
            ..Default::default()
        };
        let mut ctx = SteeringContext::discover(tmp.path(), &config);
        let output = ctx.load_contents();

        assert!(output.contains("TRUNCATED"));
    }

    #[test]
    fn test_additional_patterns() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("CUSTOM.md"), "custom").unwrap();

        let config = SteeringConfig {
            additional_patterns: vec!["CUSTOM.md".to_string()],
            ..Default::default()
        };
        let ctx = SteeringContext::discover(tmp.path(), &config);

        assert!(ctx.has_files());
        assert!(ctx.file_names().contains(&"CUSTOM.md".to_string()));
    }

    #[test]
    fn test_has_been_read_flag() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "test").unwrap();

        let config = SteeringConfig::default();
        let mut ctx = SteeringContext::discover(tmp.path(), &config);

        assert!(!ctx.has_been_read());
        let _ = ctx.load_contents();
        assert!(ctx.has_been_read());
    }
}
```

### 2.1 Verify Step 2:

```bash
cargo build --release
cargo test steering --all-features
```

### 2.2 Commit Step 2:

```bash
git add src/agent/steering.rs
git commit -m "feat(steering): add SteeringContext discovery module"
```

---

## Step 3: Export Steering Module

**File:** `src/agent/mod.rs`

**Action:** Add the steering module export.

### 3.1 Read current content and add:

```rust
pub mod steering;
pub use steering::SteeringContext;
```

### 3.2 Verify:

```bash
cargo build --release
```

### 3.3 Commit:

```bash
git add src/agent/mod.rs
git commit -m "feat(steering): export SteeringContext from agent module"
```

---

## Step 4: Create read_steering_files Tool

**File:** `src/tools/readonly/steering.rs` (NEW FILE)

**Action:** Create new file:

```rust
//! Tool for reading project steering files

use crate::agent::SteeringContext;
use crate::tools::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Tool to read project steering/instruction files
pub struct ReadSteeringFilesTool {
    steering: Arc<RwLock<SteeringContext>>,
}

impl ReadSteeringFilesTool {
    pub fn new(steering: Arc<RwLock<SteeringContext>>) -> Self {
        Self { steering }
    }
}

#[async_trait]
impl Tool for ReadSteeringFilesTool {
    fn name(&self) -> &str {
        "read_steering_files"
    }

    fn description(&self) -> &str {
        "Read project steering/instruction files (AGENTS.md, .cursorrules, CLAUDE.md, CONTRIBUTING.md) \
         that contain coding guidelines, project rules, and conventions. \
         You MUST call this BEFORE making any code changes to understand project-specific requirements."
    }

    fn parameters(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "required": []
        })
    }

    async fn execute(&self, _args: Value) -> anyhow::Result<ToolResult> {
        let mut steering = self.steering.write().await;

        if !steering.has_files() {
            return Ok(ToolResult {
                output: "No steering files found in this project.\n\nCommon steering files include:\n- AGENTS.md\n- .cursorrules\n- CLAUDE.md\n- CONTRIBUTING.md".to_string(),
                success: true,
            });
        }

        let content = steering.load_contents();

        Ok(ToolResult {
            output: content,
            success: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SteeringConfig;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_steering_files_tool() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("AGENTS.md"), "# Test Guidelines\n\nFollow these rules.").unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);
        let steering = Arc::new(RwLock::new(ctx));

        let tool = ReadSteeringFilesTool::new(steering);
        let result = tool.execute(serde_json::json!({})).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("Test Guidelines"));
        assert!(result.output.contains("AGENTS.md"));
    }

    #[tokio::test]
    async fn test_no_steering_files() {
        let tmp = TempDir::new().unwrap();

        let config = SteeringConfig::default();
        let ctx = SteeringContext::discover(tmp.path(), &config);
        let steering = Arc::new(RwLock::new(ctx));

        let tool = ReadSteeringFilesTool::new(steering);
        let result = tool.execute(serde_json::json!({})).await.unwrap();

        assert!(result.success);
        assert!(result.output.contains("No steering files found"));
    }
}
```

### 4.1 Verify:

```bash
cargo build --release
```

### 4.2 Commit:

```bash
git add src/tools/readonly/steering.rs
git commit -m "feat(steering): add read_steering_files tool"
```

---

## Step 5: Export Steering Tool from readonly module

**File:** `src/tools/readonly/mod.rs`

**Action:** Add the steering module export. Find the file and add:

```rust
pub mod steering;
pub use steering::ReadSteeringFilesTool;
```

### 5.1 Verify:

```bash
cargo build --release
```

### 5.2 Commit:

```bash
git add src/tools/readonly/mod.rs
git commit -m "feat(steering): export ReadSteeringFilesTool from readonly module"
```

---

## Step 6: Register Tool in ToolRegistry

**File:** `src/tools/mod.rs`

**Action:** Multiple changes needed.

### 6.1 Add import at top of file:

```rust
use crate::agent::SteeringContext;
```

### 6.2 Add field to ToolRegistry struct:

Find `pub struct ToolRegistry` and add:

```rust
    /// Steering context for steering files tool
    steering_context: Option<Arc<tokio::sync::RwLock<SteeringContext>>>,
```

### 6.3 Update ToolRegistry::new() or with_defaults():

Find the constructor and initialize steering_context to None initially, then add a method to set it:

```rust
impl ToolRegistry {
    // Add this method
    pub fn with_steering_context(mut self, ctx: Arc<tokio::sync::RwLock<SteeringContext>>) -> Self {
        self.steering_context = Some(ctx);
        // Register the tool
        let tool = Box::new(readonly::ReadSteeringFilesTool::new(ctx.clone()));
        self.tools.insert("read_steering_files".to_string(), tool);
        self
    }
}
```

### 6.4 Ensure steering_context is initialized in constructors:

In `new()` or `with_defaults()`, add:

```rust
            steering_context: None,
```

### 6.5 Verify:

```bash
cargo build --release
```

### 6.6 Commit:

```bash
git add src/tools/mod.rs
git commit -m "feat(steering): register read_steering_files tool in ToolRegistry"
```

---

## Step 7: Integrate SteeringContext into ChatAgent

**File:** `src/agent/chat.rs`

**Action:** Multiple changes.

### 7.1 Add imports at top:

```rust
use super::SteeringContext;
use crate::storage::SteeringConfig;
use std::sync::Arc;
use tokio::sync::RwLock;
```

### 7.2 Add field to ChatAgent struct:

Find `pub struct ChatAgent` and add:

```rust
    /// Steering context for project instruction files
    steering_context: Option<Arc<RwLock<SteeringContext>>>,
    /// Steering config
    steering_config: SteeringConfig,
```

### 7.3 Initialize in constructors:

In `with_mode()` function, add to the Self struct initialization:

```rust
            steering_context: None,
            steering_config: SteeringConfig::default(),
```

### 7.4 Add builder method:

```rust
    /// Set steering context for the agent
    pub fn with_steering(
        mut self,
        working_dir: &std::path::Path,
        config: SteeringConfig,
    ) -> Self {
        if config.auto_discover {
            let ctx = SteeringContext::discover(working_dir, &config);
            if ctx.has_files() {
                self.steering_context = Some(Arc::new(RwLock::new(ctx)));
            }
        }
        self.steering_config = config;
        self
    }

    /// Get steering context for tool registration
    pub fn steering_context(&self) -> Option<Arc<RwLock<SteeringContext>>> {
        self.steering_context.clone()
    }

    /// Check if steering files exist but haven't been read
    pub async fn steering_files_pending(&self) -> bool {
        if let Some(ref ctx) = self.steering_context {
            let ctx = ctx.read().await;
            ctx.has_files() && !ctx.has_been_read()
        } else {
            false
        }
    }

    /// Get steering file names for system prompt
    pub async fn steering_file_names(&self) -> Vec<String> {
        if let Some(ref ctx) = self.steering_context {
            let ctx = ctx.read().await;
            ctx.file_names()
        } else {
            vec![]
        }
    }
```

### 7.5 Update get_system_prompt function signature:

Find `fn get_system_prompt(` and add a new parameter:

```rust
fn get_system_prompt(
    mode: AgentMode,
    supports_native_thinking: bool,
    thinking_enabled: bool,
    trust_level: crate::tools::TrustLevel,
    plan_context: Option<&PlanContext>,
    steering_files: Option<Vec<String>>,  // NEW
) -> String {
```

### 7.6 Add steering directive in get_system_prompt:

After the status header section, before `let base_prompt = match mode {`, add:

```rust
    // Add steering files directive if files detected
    let steering_directive = if let Some(files) = steering_files {
        if !files.is_empty() {
            let file_list = files.join(", ");
            format!(r#"
═══════════════════════════════════════════════════════════
⚠️ MANDATORY: PROJECT STEERING FILES DETECTED
═══════════════════════════════════════════════════════════
This project has steering files with coding guidelines:
  {}

🚨 YOU MUST call `read_steering_files` BEFORE:
  • Writing, patching, or deleting any files
  • Running shell commands that modify the project
  • Creating execution plans involving code changes

These files contain project-specific rules and requirements
that you are REQUIRED to follow.
═══════════════════════════════════════════════════════════

"#, file_list)
        } else {
            String::new()
        }
    } else {
        String::new()
    };
```

### 7.7 Include steering_directive in final prompt:

Change the final return to include the directive:

```rust
    format!("{}{}{}", status_header, steering_directive, prompt_with_thinking)
```

### 7.8 Update all calls to get_system_prompt:

Find all places where `get_system_prompt(` is called and add `None` as the last argument for now. There should be several calls - update each one:

```rust
get_system_prompt(mode, supports_thinking, false, trust_level, None, None)
```

### 7.9 Verify:

```bash
cargo build --release
cargo test --all-features
```

### 7.10 Commit:

```bash
git add src/agent/chat.rs
git commit -m "feat(steering): integrate SteeringContext into ChatAgent with system prompt directive"
```

---

## Step 8: Wire Up Steering in Agent Bridge (TUI)

**File:** `src/tui/agent_bridge.rs`

**Action:** Load steering config and pass to agent.

### 8.1 Find where ChatAgent is created and add steering:

Look for `ChatAgent::new(` or `ChatAgent::with_mode(` and chain `.with_steering()`:

```rust
// After creating the agent, add:
let workspace_config = storage.load_config().unwrap_or_default();
let agent = agent.with_steering(&working_dir, workspace_config.steering.clone());

// If tools need steering context:
if let Some(steering_ctx) = agent.steering_context() {
    tools = tools.with_steering_context(steering_ctx);
}
```

### 8.2 Verify:

```bash
cargo build --release
```

### 8.3 Commit:

```bash
git add src/tui/agent_bridge.rs
git commit -m "feat(steering): wire up steering context in TUI agent bridge"
```

---

## Step 9: Wire Up Steering in HTTP Transport

**File:** `src/transport/http.rs`

**Action:** Similar to agent_bridge, load steering config.

### 9.1 Find where ChatAgent is created and add steering setup.

### 9.2 Verify:

```bash
cargo build --release
```

### 9.3 Commit:

```bash
git add src/transport/http.rs
git commit -m "feat(steering): wire up steering context in HTTP transport"
```

---

## Step 10: Add Optional Enforcement Gate

**File:** `src/agent/chat.rs`

**Action:** Add pre-flight check before write tools.

### 10.1 Add helper method to ChatAgent:

```rust
    /// Check if a tool call should be blocked due to unread steering files
    async fn should_block_for_steering(&self, tool_name: &str) -> Option<String> {
        // Only enforce in Build mode
        if self.mode != AgentMode::Build {
            return None;
        }

        // Check if enforcement is enabled
        if !self.steering_config.enforce_before_writes {
            return None;
        }

        // Check if steering context exists and has files
        let ctx = self.steering_context.as_ref()?;
        let ctx_read = ctx.read().await;
        
        if !ctx_read.has_files() {
            return None;
        }

        // Check if already read
        if ctx_read.has_been_read() {
            return None;
        }

        // Block write tools
        const WRITE_TOOLS: &[&str] = &[
            "write_file",
            "patch_file", 
            "delete_file",
        ];

        if WRITE_TOOLS.contains(&tool_name) {
            let files = ctx_read.file_names().join(", ");
            return Some(format!(
                "⚠️ BLOCKED: You must call `read_steering_files` before using `{}`.\n\n\
                 This project has steering files ({}) that contain required coding guidelines.\n\n\
                 Please read them first with `read_steering_files`, then retry this operation.",
                tool_name,
                files
            ));
        }

        None
    }
```

### 10.2 Add enforcement check in chat() and chat_streaming() methods:

In the tool execution loop, before `self.tools.execute(`, add:

```rust
                        // Check steering enforcement
                        if let Some(block_msg) = self.should_block_for_steering(&call.name).await {
                            self.context.add_tool_result(&call.id, &block_msg);
                            tool_call_log.push(ToolCallLog {
                                tool: call.name.clone(),
                                args: call.arguments.clone(),
                                result_preview: block_msg.clone(),
                            });
                            continue; // Skip execution
                        }
```

### 10.3 Verify:

```bash
cargo build --release
cargo test --all-features
```

### 10.4 Commit:

```bash
git add src/agent/chat.rs
git commit -m "feat(steering): add optional enforcement gate blocking writes until steering read"
```

---

## Step 11: Final Verification

```bash
# Full build
cargo build --release

# Format check
cargo fmt --all
cargo fmt --all -- --check

# Clippy
cargo clippy --all-targets --all-features -- -D warnings

# All tests
cargo test --all-features

# If all pass, create final commit for any remaining changes
git add -A
git commit -m "feat(steering): complete steering files feature implementation"
```

---

## Step 12: Push and Create PR

```bash
# Push branch
git push -u origin feature/steering-files

# Create PR (if using GitHub CLI)
gh pr create --title "feat: Add steering files auto-discovery" --body "
## Summary
Adds auto-discovery of project steering files (AGENTS.md, .cursorrules, CLAUDE.md, CONTRIBUTING.md) with:

- On-demand loading via \`read_steering_files\` tool
- Strong system prompt directive
- Optional enforcement gate (blocks writes until steering read)
- Configurable token limits

## Configuration
\`\`\`toml
# .tark/config.toml
[steering]
max_tokens = 8000
auto_discover = true
enforce_before_writes = true
additional_patterns = [\"CUSTOM.md\"]
\`\`\`

## Testing
- [x] cargo build --release
- [x] cargo fmt --all -- --check
- [x] cargo clippy -- -D warnings
- [x] cargo test --all-features
"
```

---

## File Summary

| File | Action |
|------|--------|
| `src/storage/mod.rs` | Add `SteeringConfig` struct |
| `src/agent/steering.rs` | NEW: Discovery + caching |
| `src/agent/mod.rs` | Export steering module |
| `src/tools/readonly/steering.rs` | NEW: `read_steering_files` tool |
| `src/tools/readonly/mod.rs` | Export tool |
| `src/tools/mod.rs` | Register tool, add steering context |
| `src/agent/chat.rs` | Integrate steering, add directive + enforcement |
| `src/tui/agent_bridge.rs` | Wire up steering |
| `src/transport/http.rs` | Wire up steering |

---

## Rollback Plan

If something goes wrong:

```bash
# Discard all changes and return to main
git checkout main
git branch -D feature/steering-files

# Or reset to a specific commit
git reset --hard <commit-hash>
```
