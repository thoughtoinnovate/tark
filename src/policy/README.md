# Policy Engine

SQLite-backed policy engine for tool approval and risk management.

## Architecture

```
Policy Engine (this module)
├── Core
│   ├── schema.rs         - 15 SQLite tables
│   ├── types.rs          - Data structures (ModeId, TrustId enums)
│   ├── engine.rs         - PolicyEngine API
│   ├── seed.rs           - Load builtin policy from split configs
│   └── resolver.rs       - Dynamic rule resolution
│
├── Classification
│   ├── classifier.rs     - Command operation detection
│   └── security.rs       - Pattern/path validation
│
├── Configuration (Split Config Files)
│   └── configs/
│       ├── modes.toml    - Agent modes (Ask, Plan, Build)
│       ├── trust.toml    - Trust levels with defaults
│       ├── tools.toml    - Tool declarations
│       └── defaults.toml - Hierarchical approval defaults
│
├── User Configuration
│   └── config.rs         - Load user MCP configs
│
└── Integration
    └── mcp.rs            - MCP tool policies
```

## Key Concepts

### Strict Separation Model

**Builtin (Immutable):**
- Internal modes: Ask, Plan, Build
- Trust levels: Balanced, Careful, Manual
- Internal tools: shell, read_file, write_file, etc.
- All approval rules for internal tools

**User Additions:**
- MCP tool policies (risk/approval settings)
- MCP approval patterns (pre-approve/deny specific calls)

**Users CANNOT:**
- Modify internal modes, tools, or rules
- Override builtin behavior
- Add patterns for internal tools (unless allow_save_pattern=true)

### Classifications

Shell commands are dynamically classified:
- `shell-read`: Read-only operations (cat, ls, grep)
- `shell-write`: Write operations (echo >, touch, npm install)
- `shell-rm`: Delete operations (rm, rmdir)

Each classification has rules per (Mode x Trust x Location):
- `in_workdir=true`: Paths within working directory
- `in_workdir=false`: Paths outside working directory

### Approval Rules Matrix

```
                    BALANCED              CAREFUL               MANUAL
               in_workdir  outside   in_workdir  outside   in_workdir  outside
             ┌───────────┬─────────┬───────────┬─────────┬───────────┬─────────┐
shell-read   │   Auto    │  Auto   │   Auto    │ Prompt  │   Auto    │ Prompt  │
shell-write  │   Auto    │ Prompt  │  Prompt   │ Prompt  │  Prompt   │ Prompt  │
shell-rm     │   Auto    │ Prompt  │  Prompt   │ ALWAYS  │  Prompt   │ ALWAYS  │
             └───────────┴─────────┴───────────┴─────────┴───────────┴─────────┘

ALWAYS = Cannot save pattern to skip future prompts (allow_save_pattern=false)
```

## Usage

```rust
use tark::policy::PolicyEngine;

// Open policy database
let engine = PolicyEngine::open(
    &db_path,
    &working_dir,
)?;

// Check if command needs approval
let decision = engine.check_approval(
    "shell",                    // tool_id
    "rm -rf /tmp/test",        // command
    "build",                    // mode
    "careful",                  // trust level
    "session123",               // session_id
)?;

if decision.needs_approval {
    // Show approval modal to user
    if user_approves {
        // Save pattern if allowed
        if decision.allow_save_pattern {
            engine.save_pattern(pattern)?;
        }
    }
}

// Log decision for audit
engine.log_decision(audit_entry)?;
```

## Database Location

- Default: `~/.tark/policy.db`
- Schema version: 1
- Auto-seeded with builtin policy on first run

## Configuration Files

### MCP Tool Policies

`~/.config/tark/policy/mcp.toml`:
```toml
[[tools]]
server = "github"
tool = "create_issue"
risk = "moderate"
needs_approval = true
allow_save_pattern = true
```

### User Patterns

`~/.config/tark/policy/patterns.toml`:
```toml
[[approvals]]
tool = "shell"
pattern = "cargo build"
match_type = "prefix"

[[denials]]
tool = "shell"
pattern = "rm -rf /"
match_type = "prefix"
```

## Testing

```bash
# Run policy module tests
cargo test --lib policy::

# Run specific test
cargo test --lib policy::engine::tests::test_check_approval_build_mode
```

## Implementation Status

### ✅ Completed (3 commits, 12 files)

**Core Infrastructure:**
- ✅ 15 SQLite tables with protection triggers (`src/policy/schema.rs`)
- ✅ Data types and enums (`src/policy/types.rs`)
- ✅ PolicyEngine API (`src/policy/engine.rs`)
- ✅ Command classifier (`src/policy/classifier.rs`)
- ✅ Security validators (`src/policy/security.rs`)

**Configuration:**
- ✅ Split config files (`src/policy/configs/*.toml`)
  - ✅ `modes.toml` - Agent modes (3 modes, ~30 lines)
  - ✅ `trust.toml` - Trust levels with default behaviors (~50 lines)
  - ✅ `tools.toml` - Tool declarations (~300 lines)
  - ✅ `defaults.toml` - Hierarchical approval defaults (~50 lines)
- ✅ Type-safe enums (`ModeId`, `TrustId` in `types.rs`)
- ✅ Dynamic rule resolver (`src/policy/resolver.rs`)
- ✅ TOML seeding logic (`src/policy/seed.rs`)
- ✅ MCP config loader (`src/policy/config.rs`)
- ✅ Migration utility (`src/policy/migrate.rs`)

**Integration:**
- ✅ MCP tool policies (`src/policy/mcp.rs`)
- ✅ ToolRegistry integration (`src/tools/mod.rs`)
- ✅ MCP wrapper functions (`src/mcp/wrapper.rs`)

**Test Results:**
- ✅ 21/21 policy module tests passing
- ✅ Zero clippy warnings
- ✅ Code formatted and clean

### ⏳ Remaining (5 todos)

- ⏳ `/policy` TUI command for pattern management
- ⏳ `patterns.toml` loading and syncing
- ⏳ Unit tests for integrated flows
- ⏳ Integration tests for full approval flow
- ⏳ Remove deprecated ApprovalGate after validation

## Next Steps

1. Add PolicyEngine to ToolRegistry
2. Replace ApprovalGate calls with PolicyEngine
3. Add /policy TUI command
4. Load patterns.toml on startup
5. Add comprehensive tests
6. Remove old approval system
