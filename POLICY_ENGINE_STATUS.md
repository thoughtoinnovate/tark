# Policy Engine Implementation Status

**Branch:** `policies`
**Commits:** 3
**Files Changed:** 13
**Tests Passing:** 21/21
**Status:** Core Complete, Integration Partial

---

## ✅ Completed Implementation (12/17 todos)

### Commit 1: Core Policy Engine (`aee8842`)

**Files Created (11 files):**
```
src/policy/
├── mod.rs              - Module exports
├── types.rs            - Data structures (ApprovalDecision, CommandClassification, etc.)
├── schema.rs           - 15 SQLite tables with immutable triggers
├── engine.rs           - PolicyEngine API (classify_command, check_approval, etc.)
├── classifier.rs       - Dynamic shell command classification
├── security.rs         - Pattern/path validation
├── seed.rs             - TOML parsing and DB seeding
├── config.rs           - User MCP config loading
├── mcp.rs              - MCP tool policy handling
├── migrate.rs          - Migration from approvals.json
├── builtin_policy.toml - Immutable approval rules matrix
└── README.md           - Module documentation
```

**Database Schema (15 tables):**
1. `schema_version` - Version tracking
2. `agent_modes` - Ask, Plan, Build (immutable)
3. `trust_levels` - Balanced, Careful, Manual (immutable)
4. `tool_categories` - Power, ReadOnly, Write, Dangerous
5. `tool_types` - Shell, read_file, write_file, etc.
6. `tool_mode_availability` - Which tools in which modes
7. `tool_classifications` - shell-read, shell-write, shell-rm
8. `approval_rules` - (Classification × Mode × Trust × Location) → needs_approval
9. `compound_command_rules` - &&, ||, ;, | handling
10. `approval_patterns` - User-saved patterns for internal tools
11. `mcp_tool_policies` - Risk/approval settings for MCP tools
12. `mcp_approval_patterns` - Patterns for MCP tools
13. `approval_audit_log` - All approval decisions
14. **Indexes** - Optimized query paths

**Protection Triggers:**
- BEFORE UPDATE/DELETE on all builtin tables → RAISE(ABORT)
- Enforces immutability of internal modes, tools, and rules

**Test Coverage:**
- `policy::schema::tests` - 2 tests (table creation, protection)
- `policy::seed::tests` - 2 tests (TOML parsing, seeding)
- `policy::engine::tests` - 4 tests (open, classify, approval checks)
- `policy::classifier::tests` - 5 tests (read/write/delete, compound)
- `policy::security::tests` - 3 tests (validation, path sanitization)
- `policy::mcp::tests` - 2 tests (defaults, with policy)
- `policy::migrate::tests` - 2 tests (migration logic)
- `policy::config::tests` - 1 test (TOML parsing)

**Total: 21 tests, 100% passing**

### Commit 2: ToolRegistry Integration (`4e7d04d`)

**Modified:** `src/tools/mod.rs`

**Changes:**
- Added `policy_engine: Option<Arc<PolicyEngine>>` field
- Added `session_id: String` for pattern tracking
- Initialize PolicyEngine from `~/.tark/policy.db`
- Added approval checking in `execute()` method
- Falls back to ApprovalGate if PolicyEngine unavailable
- Marked `approval_gate` as DEPRECATED

**Integration Status:** ✅ Passive (logs but doesn't block)

### Commit 3: MCP Wrapper Integration (`a2ff714`)

**Modified:** `src/mcp/wrapper.rs`

**Changes:**
- Added `wrap_server_tools_with_policy()` function
- Queries PolicyEngine for MCP tool risk levels
- Backward compatible with `wrap_server_tools()`
- Added `query_mcp_risk_level()` helper

**Integration Status:** ✅ Ready (needs caller update)

---

## 📋 Approval Rules Matrix (Implemented)

```
                    BALANCED              CAREFUL               MANUAL
               in_workdir  outside   in_workdir  outside   in_workdir  outside
             ┌───────────┬─────────┬───────────┬─────────┬───────────┬─────────┐
shell-read   │   Auto    │  Auto   │   Auto    │ Prompt  │   Auto    │ Prompt  │
shell-write  │   Auto    │ Prompt  │  Prompt   │ Prompt  │  Prompt   │ Prompt  │
shell-rm     │   Auto    │ Prompt  │  Prompt   │ ALWAYS  │  Prompt   │ ALWAYS  │
             └───────────┴─────────┴───────────┴─────────┴───────────┴─────────┘

Legend:
- Auto = Auto-approved (no prompt)
- Prompt = Needs approval (can save pattern)
- ALWAYS = Needs approval (CANNOT save pattern - allow_save_pattern=false)
```

---

## ⏳ Remaining Work (5/17 todos)

### 1. `/policy` TUI Command (todo: policy-command)

**Purpose:** Allow users to view and manage approval/denial patterns

**Requirements:**
- Add `PolicyManager` variant to `ModalType` enum
- Create `src/tui_new/modals/policy_modal.rs` widget
- Add `OpenPolicyManager` command to `Command` enum
- Add `/policy` slash command handler
- Display session patterns (approvals + denials)
- Allow deletion by pattern ID
- Show pattern details (tool, pattern, match_type, source)

**UI Mockup:**
```
┌─────────── Policy Manager ───────────┐
│                                       │
│  Approvals:                           │
│  [1] shell: cargo build (prefix)      │
│  [2] shell: npm install (prefix)      │
│                                       │
│  Denials:                             │
│  [3] shell: rm -rf / (prefix)         │
│                                       │
│  Press 'd' to delete, Esc to close    │
└───────────────────────────────────────┘
```

### 2. `patterns.toml` Loading (todo: policy-file)

**Purpose:** Sync user-defined patterns to database on startup

**File Location:** `~/.config/tark/policy/patterns.toml`

**Format:**
```toml
[[approvals]]
tool = "shell"
pattern = "cargo build"
match_type = "prefix"
description = "Auto-approve cargo builds"

[[denials]]
tool = "shell"
pattern = "rm -rf /"
match_type = "prefix"
description = "Never delete root"
```

**Implementation:**
1. Create pattern loader in `src/policy/config.rs`
2. Load on PolicyEngine initialization
3. Sync to `approval_patterns` table
4. Validate against `allow_save_pattern` rules
5. Log warnings for invalid patterns

### 3. Unit Tests (todo: tests-unit)

**Missing Coverage:**
- `ToolRegistry::execute()` with PolicyEngine
- Pattern loading from `patterns.toml`
- Trust level synchronization
- Session ID generation and tracking
- Fallback behavior when PolicyEngine fails

**Files to Test:**
- `src/tools/mod.rs` (integration points)
- `src/policy/config.rs` (pattern loading)
- `src/mcp/wrapper.rs` (policy-driven risk levels)

### 4. Integration Tests (todo: tests-integration)

**End-to-End Scenarios:**
1. **Build mode + Balanced + shell-write in workdir**
   - Command: `echo "test" > file.txt`
   - Expected: Auto-approved
   
2. **Build mode + Careful + shell-read outside workdir**
   - Command: `cat /etc/passwd`
   - Expected: Prompt for approval
   
3. **Build mode + Careful + shell-rm outside workdir**
   - Command: `rm /tmp/test`
   - Expected: ALWAYS prompt (cannot save)
   
4. **Pattern matching**
   - Save pattern: `cargo build*`
   - Command: `cargo build --release`
   - Expected: Auto-approved (pattern match)
   
5. **Denial pattern**
   - Save denial: `rm -rf /`
   - Command: `rm -rf /tmp`
   - Expected: Blocked silently

**Test Infrastructure:**
- Create test helper in `tests/policy_integration.rs`
- Use in-memory SQLite for isolation
- Mock InteractionSender for approvals
- Verify audit log entries

### 5. Cleanup (todo: cleanup)

**Deprecation Path:**
1. Mark `ApprovalGate` with `#[deprecated]`
2. Add migration warnings to logs
3. Update documentation to use PolicyEngine
4. Remove `approval_gate` field after validation period
5. Delete `src/tools/approval.rs` entirely
6. Update all callers to use PolicyEngine

**Files to Update:**
- `src/tools/mod.rs` - Remove approval_gate field
- `src/tools/approval.rs` - Delete file
- `src/agent/chat.rs` - Update approval flow
- `src/ui_backend/service.rs` - Remove approval gate handling
- `docs/` - Update documentation

---

## 🔧 Configuration Files

### User Config: `~/.config/tark/policy/mcp.toml`
```toml
[[tools]]
server = "github"
tool = "create_issue"
risk = "moderate"
needs_approval = true
allow_save_pattern = true

[[tools]]
server = "github"
tool = "list_repos"
risk = "safe"
needs_approval = false
```

### User Patterns: `~/.config/tark/policy/patterns.toml`
```toml
[[approvals]]
tool = "shell"
pattern = "cargo*"
match_type = "glob"

[[denials]]
tool = "shell"
pattern = "rm -rf /"
match_type = "prefix"
```

### Workspace Config: `.tark/policy/mcp.toml`
*(Same format, overrides user config)*

---

## 📊 Database Locations

- **Policy DB:** `~/.tark/policy.db` (15 tables, ~100KB typical)
- **Memory DB:** `~/.tark/memory.db` (for memory tools)
- **Usage DB:** `~/.tark/usage.db` (for analytics)

---

## 🚀 Next Steps

### Immediate (1-2 hours)
1. Add `/policy` command and modal
2. Implement `patterns.toml` loading
3. Write basic integration tests

### Short-term (2-4 hours)
1. Full integration test suite
2. Performance testing
3. Documentation updates

### Long-term (Future PR)
1. Validation period (1-2 weeks)
2. Remove ApprovalGate
3. Add config migration tool
4. User-facing documentation

---

## 🐛 Known Limitations

1. **Trust level not synchronized:** Currently defaults to "balanced"
   - TODO: Add `trust_level` field to ToolRegistry
   - Sync with approval_gate during transition

2. **MCP risk levels are default:** `query_mcp_risk_level()` returns Risky
   - TODO: Query `mcp_tool_policies` table directly

3. **Pattern validation partial:** Some forbidden patterns not caught
   - TODO: Expand forbidden pattern list

4. **No UI for policy management:** Command-line only
   - TODO: Add TUI `/policy` command (in progress)

5. **Migration incomplete:** Only warns about internal tool patterns
   - TODO: Support MCP pattern migration with server info

---

## 📝 Testing Commands

```bash
# Run all policy tests
cargo test --lib policy::

# Run specific test
cargo test --lib policy::engine::tests::test_check_approval_build_mode

# Check formatting
cargo fmt --all -- --check

# Lint
cargo clippy --lib -- -D warnings

# Build
cargo build --release
```

---

## 🎯 Success Criteria

- [ ] `/policy` command functional
- [ ] `patterns.toml` loading working
- [ ] 30+ integration tests passing
- [ ] Zero regression in existing approval flow
- [ ] Documentation complete
- [ ] ApprovalGate removed
- [ ] Performance validated (no slowdown)

---

**Last Updated:** 2026-01-23
**Branch Status:** Ready for final integration
**Review Status:** Pending
