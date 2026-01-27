# Agent Guidelines for tark

This document helps AI coding agents understand the tark codebase and make effective contributions.

---

## 🚨 CRITICAL: Developer Workflow Requirements

**Before making ANY changes, read the [Developer Workflow](#developer-workflow-) section.**

**Every code change MUST**:

1. ✅ **Compile** - `cargo check`/`cargo build` during development, `cargo build --release` before commit
2. ✅ **Be formatted** - `cargo fmt --all` must be run
3. ✅ **Pass linting** - `cargo clippy` with zero warnings
4. ✅ **Pass all tests** - `cargo test --all-features` must succeed
5. ✅ **Include/update tests** - No code change without corresponding tests
6. ✅ **Update documentation** - README.md, AGENTS.md, or doc comments for significant changes
7. ✅ **Use clean git workflow** - Stash unrelated changes, commit logical units separately
8. ✅ **Verify performance** - For core agent code changes, ensure no performance regressions

**See the [Pre-Commit Checklist](#pre-commit-checklist-) for the complete workflow.**

---

## Project Overview

**tark** is an AI-powered CLI agent with LSP server for Neovim. It provides:
- Ghost text completions (like Cursor/Copilot)
- Chat interface with file/shell tools
- Multi-provider LLM support (OpenAI, Claude, Gemini, Copilot, OpenRouter, Ollama)

## Architecture

The TUI uses a **Backend-for-Frontend (BFF)** pattern separating UI rendering from business logic.

```
tark/
├── src/                         # Rust backend (tark server)
│   ├── main.rs                  # CLI entry point
│   ├── lib.rs                   # Library exports
│   │
│   ├── core/                    # Shared core modules
│   │   ├── attachments.rs       # File attachment handling
│   │   ├── context_tracker.rs   # Context window management
│   │   ├── conversation_manager.rs # Conversation state
│   │   ├── session_manager.rs   # Session persistence
│   │   ├── tokenizer.rs         # Token counting
│   │   └── types.rs             # AgentMode, BuildMode, ThinkLevel
│   │
│   ├── ui_backend/              # BFF layer (Backend-for-Frontend)
│   │   ├── traits.rs            # UiRenderer trait
│   │   ├── events.rs            # AppEvent enum
│   │   ├── service.rs           # AppService orchestrator
│   │   ├── state.rs             # SharedState (Arc<RwLock<>>)
│   │   ├── commands.rs          # Command enum (100+ variants)
│   │   ├── types.rs             # UI data structures
│   │   ├── approval.rs          # ApprovalCardState
│   │   └── questionnaire.rs     # QuestionnaireState
│   │
│   ├── tui_new/                 # New TUI implementation (ratatui)
│   │   ├── controller.rs        # Event loop coordinator
│   │   ├── renderer.rs          # UiRenderer implementation
│   │   ├── theme.rs             # Theme system (6 presets)
│   │   ├── modals/              # Modal widgets
│   │   │   ├── approval_modal.rs
│   │   │   ├── trust_modal.rs
│   │   │   ├── session_picker.rs
│   │   │   ├── task_edit_modal.rs
│   │   │   └── ...
│   │   └── widgets/             # UI components
│   │       ├── header.rs        # Title bar
│   │       ├── message_area.rs  # Chat messages
│   │       ├── input.rs         # Input field
│   │       ├── sidebar.rs       # Session/Context/Tasks/Git
│   │       ├── status_bar.rs    # Mode/Provider/Model
│   │       └── modal.rs         # Picker modals
│   │
│   ├── agent/                   # Chat agent with tool execution
│   ├── completion/              # FIM (fill-in-middle) completions
│   ├── channels/                # Messaging channels via WASM plugins
│   ├── config/                  # Configuration management
│   ├── diagnostics/             # Code analysis
│   ├── llm/                     # LLM providers + model DB
│   │   ├── claude.rs            # Anthropic Claude
│   │   ├── openai.rs            # OpenAI GPT
│   │   ├── gemini.rs            # Google Gemini
│   │   ├── copilot.rs           # GitHub Copilot
│   │   ├── openrouter.rs        # OpenRouter
│   │   ├── ollama.rs            # Local Ollama (with tool calling)
│   │   ├── tark_sim.rs          # Built-in test provider
│   │   ├── models_db.rs         # models.dev integration
│   │   └── types.rs             # Shared LLM types
│   ├── lsp/                     # LSP server implementation
│   ├── storage/                 # Persistent storage (.tark/)
│   │   └── usage.rs             # Usage tracking with SQLite
│   ├── tools/                   # Agent tools with risk-based categorization
│   │   ├── mod.rs               # Tool registry with mode-based composition
│   │   ├── risk.rs              # RiskLevel and TrustLevel enums
│   │   ├── approval.rs          # ApprovalGate with pattern matching
│   │   ├── questionnaire.rs     # User interaction requests
│   │   ├── builtin/             # Built-in native tools
│   │   │   ├── thinking.rs      # ThinkTool for structured reasoning
│   │   │   └── memory.rs        # Memory tools with SQLite backend
│   │   ├── readonly/            # Read-only tools (grep, file_preview, safe_shell)
│   │   ├── write/               # Write tools
│   │   ├── risky/               # Shell tools
│   │   └── dangerous/           # Destructive tools
│   ├── mcp/                     # MCP client (feature-gated)
│   │   ├── client.rs            # McpServerManager
│   │   ├── transport.rs         # STDIO transport
│   │   ├── wrapper.rs           # Tool wrapper adapters
│   │   └── types.rs             # MCP protocol types
│   └── transport/               # HTTP server and CLI
│       ├── cli.rs               # CLI commands
│       └── dashboard.rs         # Usage dashboard HTML
│
├── lua/                         # Neovim plugin (Lua)
│   └── tark/
│       ├── init.lua             # Plugin entry point & setup
│       ├── binary.lua           # Binary find/download/version
│       ├── tui.lua              # TUI integration (socket RPC)
│       ├── ghost.lua            # Ghost text completions
│       ├── lsp.lua              # LSP integration helpers
│       ├── statusline.lua       # Statusline helpers
│       └── health.lua           # :checkhealth integration
│
├── tests/                       # Test suite
│   ├── cucumber_tui_new.rs      # BDD integration tests
│   ├── tui_snapshot_tests.rs    # Visual snapshot tests
│   ├── tui_widget_tests.rs      # Widget unit tests
│   └── visual/                  # Visual E2E tests
│       └── tui/features/        # Gherkin feature files
│
├── docs/                        # Documentation
│   ├── BFF_ARCHITECTURE.md      # BFF design details
│   ├── TUI_SETUP.md             # Setup guide
│   ├── THEMES.md                # Theme system
│   └── TUI_MODAL_DESIGN_GUIDE.md # Modal design patterns
│
├── .github/workflows/           # CI/CD
├── Dockerfile                   # Minimal scratch image (~15MB)
├── Dockerfile.alpine            # Alpine image with shell (~30MB)
└── install.sh                   # Binary installer with checksum verification
```

## Do's ✅

### Code Style

- **Follow existing patterns** - Look at similar code before writing new code
- **Use `#![allow(dead_code)]`** at module level for intentionally unused API methods
- **Keep functions focused** - One function, one purpose
- **Add doc comments** for public APIs (`///` in Rust, `---` in Lua)

### Rust Backend

- **Use `anyhow::Result`** for error handling
- **Use `tracing`** for logging, not `println!`
- **Async by default** - Use `tokio` for async operations
- **Keep LLM providers isolated** - Each in its own file under `src/llm/`

### Performance (Core Agent Code) ⚡

**CRITICAL**: When modifying core agent code (`src/agent/`, `src/tools/`, `src/mcp/`, `src/llm/`), you MUST verify performance is not negatively impacted.

**Before making changes**:
- Understand the hot paths in the code you're modifying
- Note current response times for typical operations
- Consider the impact on streaming latency and tool execution time

**After making changes**:
1. **Measure response time** - Compare before/after for common operations
2. **Check memory usage** - Ensure no memory leaks or excessive allocations
3. **Verify streaming latency** - Token streaming should remain responsive
4. **Test with multiple tools** - Tool execution overhead should be minimal

**Performance best practices**:
- **Avoid unnecessary allocations** - Reuse buffers, use `&str` over `String` where possible
- **Minimize cloning** - Use references or `Arc` for shared data
- **Batch operations** - Combine multiple small operations when possible
- **Lazy evaluation** - Don't compute values until needed
- **Efficient data structures** - Use `HashMap` for lookups, `Vec` for sequential access
- **Avoid blocking in async** - Never block the async runtime; use `tokio::spawn_blocking` for CPU-heavy work

**How to profile**:
```bash
# Build with debug symbols for profiling
cargo build --release

# Use flamegraph for CPU profiling (requires cargo-flamegraph)
cargo flamegraph --bin tark -- tui

# Use heaptrack or valgrind for memory profiling
heaptrack ./target/release/tark tui

# Simple timing with tracing
RUST_LOG=debug ./target/release/tark tui  # Check span timings in logs
```

**Performance improvements to consider**:
- Cache frequently accessed data (e.g., tool schemas, model info)
- Use connection pooling for HTTP clients
- Implement request batching where applicable
- Consider lazy initialization for expensive resources
- Profile before optimizing - measure, don't guess

### Lua Plugin

- **Lazy-load modules** - Use `require()` inside functions, not at top level
- **Use `vim.schedule()`** for callbacks that touch UI
- **Respect user config** - Always merge with defaults using `vim.tbl_deep_extend`
- **Provide commands** - Every feature should have a `:Tark*` command

### Testing

- **Write tests for ALL code changes** - No exceptions
- **Test-driven development** - Write failing test first, then fix
- **Run all checks before committing** (see [Pre-Commit Checklist](#pre-commit-checklist-)):
  ```bash
  # Rust checks
  cargo fmt --all
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-features
  
  # Neovim/Lua tests (requires nvim and plenary.nvim)
  nvim --headless -u tests/minimal_init.lua \
    -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
  ```
- **Coverage requirements**:
  - New features → Unit tests + integration tests
  - Bug fixes → Regression tests
  - Refactors → Maintain existing test coverage

### TUI Testing (CRITICAL)

The TUI has a multi-level testing strategy. See `tests/TUI_TESTING_README.md` for details.

**Test Levels:**

| Level | Tool | Purpose | Command |
|-------|------|---------|---------|
| Unit | `#[test]` | Widget functions | `cargo test --test tui_widget_tests` |
| Snapshot | `insta` | Visual regression | `cargo test --test tui_snapshot_tests` |
| Integration | Cucumber | Component interactions | `cargo test --test cucumber_tui_new` |
| E2E | Cucumber + PTY | Real binary | `cargo test --test cucumber_e2e --release` |

**⚠️ Cucumber step definitions can CHEAT by directly manipulating state instead of testing real code paths.**

For TUI features, you MUST:

1. **Write unit tests for widget logic in `tests/tui_widget_tests.rs`**

2. **Use snapshot tests for visual changes in `tests/tui_snapshot_tests.rs`**
   ```bash
   cargo test --test tui_snapshot_tests
   cargo insta review  # Review snapshot changes
   ```

3. **Manual smoke test after EVERY TUI change**:
   ```bash
   # During development (faster iteration)
   cargo build
   ./target/debug/tark tui
   
   # Before committing (test optimized binary)
   cargo build --release
   ./target/release/tark tui
   # Verify: /help, /model, /theme, Ctrl+?, Escape, Enter all work
   ```

4. **If manual test fails, fix `src/tui_new/controller.rs` or `renderer.rs`** - Not the test step definitions

### Versioning

- **Keep versions in sync**:
  - `Cargo.toml`: `version = "x.y.z"`
  - `lua/tark/init.lua`: `M.version = 'x.y.z'`
- **Binary downloads are pinned** to plugin version (stable channel)

## Don'ts ❌

### Code Style

- **Don't add unused dependencies** - Keep the binary small
- **Don't use `unwrap()` in production code** - Use `?` or handle errors
- **Don't hardcode paths** - Use `dirs` crate or `vim.fn.stdpath()`
- **Don't print to stdout** in library code - Use tracing/logging

### Security

- **NEVER log API keys** - Even at debug level
- **NEVER send API keys to non-official endpoints**
- **Don't skip checksum verification** - It's there for security

### Rust Backend

- **Don't block the async runtime** - Use `tokio::spawn` for CPU-heavy work
- **Don't add new LLM providers without tests**
- **Don't change tool schemas** without updating the agent prompts

### Performance (Core Agent Code)

- **Don't skip performance verification** for core agent code changes
- **Don't add synchronous I/O** in async hot paths - Use async alternatives
- **Don't clone large data structures** unnecessarily - Use references or `Arc`
- **Don't allocate in tight loops** - Pre-allocate or reuse buffers
- **Don't ignore performance regressions** - If you notice slowdowns, investigate before committing
- **Don't optimize without measuring** - Profile first, then optimize based on data

### Lua Plugin

- **Don't use global variables** - Keep state in module tables
- **Don't block Neovim** - Use `vim.fn.jobstart()` for async operations
- **Don't assume binary exists** - Always check and offer to download

### Git

- **Don't commit large binaries**
- **Don't force push to main**
- **Don't merge without CI passing**
- **Don't commit multiple unrelated changes together** - Use `git stash` to separate logical units
- **Don't commit without running all checks** - See [Pre-Commit Checklist](#pre-commit-checklist-)
- **Don't skip tests** - Every commit must include or update tests

## Key Files to Understand

### BFF Layer (Backend-for-Frontend)

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/ui_backend/service.rs` | AppService orchestrator | Adding UI-agnostic features |
| `src/ui_backend/commands.rs` | Command enum (100+ variants) | Adding new user actions |
| `src/ui_backend/events.rs` | AppEvent enum | Adding new async events |
| `src/ui_backend/state.rs` | SharedState (Arc<RwLock>) | Adding state fields |
| `src/ui_backend/types.rs` | UI data structures | Adding new UI types |
| `src/core/types.rs` | AgentMode, BuildMode, ThinkLevel | Changing core types |

### TUI Layer (Presentation)

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/tui_new/controller.rs` | Event loop coordinator | Changing event handling |
| `src/tui_new/renderer.rs` | Key-to-command mapping, rendering | Adding keybindings |
| `src/tui_new/theme.rs` | Theme presets and colors | Adding themes |
| `src/tui_new/widgets/*.rs` | UI widgets | Modifying widgets |
| `src/tui_new/modals/*.rs` | Modal dialogs | Adding modals |

### Agent & Tools

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/agent/chat.rs` | Chat agent logic | Adding agent features |
| `src/tools/mod.rs` | Tool registry & mode composition | Adding/modifying tools |
| `src/tools/risk.rs` | RiskLevel and TrustLevel enums | Changing risk categories |
| `src/tools/approval.rs` | ApprovalGate with pattern matching | Changing approval flow |
| `src/tools/questionnaire.rs` | User interaction requests | Adding question types |
| `src/tools/builtin/thinking.rs` | Sequential thinking tool | Modifying thinking behavior |
| `src/tools/builtin/memory.rs` | Persistent memory with SQLite | Modifying memory storage |

### MCP Client (Model Context Protocol)

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/mcp/client.rs` | McpServerManager, connection handling | Adding MCP features |
| `src/mcp/transport.rs` | STDIO transport for MCP servers | Changing communication |
| `src/mcp/wrapper.rs` | McpToolWrapper (adapts MCP → Tool) | Changing tool adaptation |
| `src/mcp/types.rs` | MCP protocol data structures | Changing MCP types |
| `src/storage/mod.rs` | McpServer config (servers.toml) | Changing MCP configuration |
| `examples/tark-config/mcp/servers.toml` | Example MCP server configs | Adding examples |

### LLM Providers

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/llm/types.rs` | LLM message types | Changing API contracts |
| `src/llm/ollama.rs` | Ollama provider (with tool calling) | Ollama features |
| `src/llm/tark_sim.rs` | Built-in test provider | Testing/demo |
| `src/llm/models_db.rs` | models.dev integration | Model metadata |

### Neovim Plugin

| File | Purpose | When to Modify |
|------|---------|----------------|
| `lua/tark/init.lua` | Plugin entry & config | Adding config options |
| `lua/tark/tui.lua` | TUI integration | Socket RPC handlers |
| `lua/tark/binary.lua` | Binary management | Download/version logic |
| `plugin/tark.lua` | Command registration | Adding new commands |

### Infrastructure

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/completion/engine.rs` | FIM completion logic | Changing completion behavior |
| `src/storage/usage.rs` | Usage tracking & SQLite | Adding usage analytics |
| `src/transport/dashboard.rs` | Usage dashboard HTML | Modifying dashboard UI |
| `.github/workflows/release.yml` | Release automation | Adding platforms |

## Common Tasks

### Using Built-in Tools

**Thinking Tool**: The `think` tool allows agents to record structured reasoning steps. It's automatically registered for all modes.

**Memory Tools**: Four tools for persistent knowledge storage:
- `memory_store`: Save information for later recall
- `memory_query`: Search stored memories
- `memory_list`: List all memories
- `memory_delete`: Remove a memory

Memory is stored in `.tark/memory.db` using SQLite.

**Todo Tool**: The `todo` tool provides session-scoped task tracking with a live-updating widget in the message area. It's automatically registered for all modes.

Use this tool to:
- Show users what steps you're working on
- Track progress on immediate tasks within the current request
- Display a visual checklist with progress bar

The todo list:
- Updates in-place (single live widget, not new messages)
- Returns full current state so you know what todos exist
- Persists during the session, cleared when session ends
- Supports merge (update by id) and replace (new list) modes
- Valid `status` values: `pending`, `inprogress`, `completed`, `cancelled` (note: no underscore, so **not** `in_progress`)

Operational rules (for agents):
- Create a todo list **once** at the start of any multi-step task (3+ steps) or when the user explicitly asks for tracking.
- Do **not** create todos for trivial requests (single-step answers) unless the user requests it.
- Use `status: "inprogress"` only for the **single** step you are actively working on.
- Mark a step `completed` immediately after it is done; never leave finished items `inprogress`.
- If a step becomes irrelevant, mark it `cancelled` with a short reason in the item `content` (update text via merge).
- When all steps are `completed` (or `cancelled`), **clear** the list by sending an empty list with `merge: false`.

Example usage:
```rust
// Create initial todo list
todo({
  "todos": [
    {"id": "read", "content": "Read existing code"},
    {"id": "impl", "content": "Implement feature"},
    {"id": "test", "content": "Add tests"}
  ],
  "merge": false  // Replace any existing todos
})

// Update specific todo as you progress
todo({
  "todos": [{"id": "read", "status": "completed"}],
  "merge": true   // Merge with existing (default)
})

// Mark a task in progress (use "inprogress", not "in_progress")
todo({
  "todos": [{"id": "impl", "status": "inprogress"}],
  "merge": true
})

// Clear all todos after finishing
todo({
  "todos": [],
  "merge": false
})
```

### Using MCP Servers

**Configuration**: Add MCP servers in `~/.config/tark/mcp/servers.toml` or `.tark/mcp/servers.toml`.

**Example**:
```toml
[servers.github]
name = "GitHub Integration"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-github"]
enabled = true
env = { GITHUB_TOKEN = "${GITHUB_TOKEN}" }

[servers.github.tark]
risk_level = "risky"
auto_connect = false
timeout_seconds = 30
namespace = "gh"
```

**User manages**: Installing MCP servers (npm, pip, etc.) and setting environment variables.
**tark manages**: Connecting, discovering tools, and executing them with risk level enforcement.

### Adding a New Tool

1. Create tool file in `src/tools/`
2. Implement `Tool` trait
3. Register in `ToolRegistry::new()` in `src/tools/mod.rs`
4. Update agent prompts in `src/agent/chat.rs`

### Adding a New LLM Provider

1. Create provider file in `src/llm/`
2. Implement `LlmProvider` trait
3. Export in `src/llm/mod.rs`
4. Add config in `src/config/mod.rs`
5. Add to provider selection in chat

### Adding a New TUI Modal

1. Add modal type to `ModalType` enum in `src/ui_backend/state.rs`
2. Create modal widget in `src/tui_new/modals/` (follow `docs/TUI_MODAL_DESIGN_GUIDE.md`)
3. Add rendering in `src/tui_new/renderer.rs` render method
4. Add keyboard handling in `renderer.rs` `key_to_command()` function
5. Add any needed state fields to `SharedState`
6. Add command handling in `src/tui_new/controller.rs`
7. Update `docs/TUI_MODAL_DESIGN_GUIDE.md` with new modal

### Adding a New Command

1. Add variant to `Command` enum in `src/ui_backend/commands.rs`
2. Handle command in `src/ui_backend/service.rs` `handle_command()`
3. If TUI-specific, add keybinding in `src/tui_new/renderer.rs`
4. Update `README.md` keyboard shortcuts table

### Adding a New Theme

1. Add variant to `ThemePreset` enum in `src/tui_new/theme.rs`
2. Implement `Theme::your_theme()` method with colors
3. Update `from_preset()` match statement
4. Update `display_name()` and `all()` methods
5. Update `docs/THEMES.md`

### Adding a Config Option

1. Add to `M.config` in `lua/tark/init.lua`
2. Use in relevant module
3. Document in README.md

### Releasing a New Version

```bash
# 1. Update versions
# Cargo.toml: version = "X.Y.Z"
# lua/tark/init.lua: M.version = 'X.Y.Z'

# 2. Commit
git add -A
git commit -m "chore: bump version to vX.Y.Z"

# 3. Tag and push
git tag vX.Y.Z
git push && git push --tags

# GitHub Actions handles the rest!
```

## Testing Locally

```bash
# Run Rust tests (runs in debug mode by default, which is faster)
cargo test --all-features

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Build binary
cargo build              # Debug build (fast, for local testing)
cargo build --release    # Release build (before commit)

# Test the server
./target/debug/tark serve --port 8765     # Debug binary
./target/release/tark serve --port 8765   # Release binary

# Test in Neovim (from plugin directory)
nvim --cmd "set rtp+=." -c "lua require('tark').setup()"

# Run Neovim/Lua tests (requires plenary.nvim)
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"

# Run specific Lua test file
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/init_spec.lua"
```

## Developer Workflow 🔄

**CRITICAL**: These guidelines are mandatory for all code changes. They ensure code quality, documentation accuracy, and system stability.

### Core Principles

1. **Documentation First** - After any significant change, update READMEs, functional docs, and architecture documentation
2. **Test-Driven Development** - After any code change, update or add tests to cover the changes
3. **Pre-Commit Validation** - Before committing, ensure code compiles, is properly formatted, and all tests pass
4. **Clean Git History** - If there are multiple uncommitted changes, use `git stash` to manage them properly

---

## After Every Code Change ⚡

**IMPORTANT**: Follow this checklist after making any code changes.

### 1. Verify Code Compiles

```bash
# Fastest - type checking only, no codegen (~2-3x faster than build)
cargo check

# During development (fast, incremental debug builds)
cargo build

# Before committing (optimized binary, required for final check)
cargo build --release
```

**Why**: Compilation errors must be caught immediately, not in CI.

**When to use which**:
- `cargo check` - Use for rapid iteration when you just want to verify types/syntax compile
- `cargo build` - Use when you need to run the binary locally (debug mode, faster builds)
- `cargo build --release` - Use only when done with all changes, before committing

### 2. Format and Lint Code

```bash
# Format code (apply fixes)
cargo fmt --all

# Verify formatting
cargo fmt --all -- --check

# Linting (must pass with zero warnings)
cargo clippy --all-targets --all-features -- -D warnings
```

**Why**: Consistent code style improves readability and maintainability.

### 3. Write/Update Tests

```bash
# After ANY code change, ensure tests exist and pass
# Tests run in debug mode by default (faster compilation)
cargo test --all-features

# For release-mode tests (rarely needed, mainly for perf-sensitive code)
cargo test --all-features --release

# For Lua changes
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
```

**Why**: Tests prevent regressions and document expected behavior.

**Requirements**:
- New features MUST have tests
- Bug fixes MUST have regression tests
- Refactors MUST maintain or improve test coverage
- Changed APIs MUST update affected tests

### 4. Update Documentation

**MANDATORY** for significant changes. If you changed:

| Change Type | Documentation to Update |
|-------------|------------------------|
| **Config options** | `README.md` configuration section |
| **Commands** | `README.md` command tables |
| **Architecture/Design** | `AGENTS.md` (this file) |
| **Public APIs** | Add/update doc comments (`///` in Rust, `---` in Lua) |
| **Features** | `README.md` features section |
| **Breaking changes** | `README.md` + migration guide |
| **Dependencies** | `README.md` requirements section |
| **Build/Install** | `README.md` installation section |

**Why**: Outdated documentation is worse than no documentation.

### 5. Run All Checks Together

Before committing, run the complete validation suite:

```bash
# During development - quick check (use this while iterating)
cargo check && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings

# Before committing - full check with release build
cargo build --release && \
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --all-features
```

For Lua changes, also run:

```bash
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
```

### 6. Fix Any Issues

- **Format errors**: Run `cargo fmt --all` to auto-fix
- **Clippy warnings**: Fix the code, don't just suppress warnings
- **Rust test failures**: Fix the failing tests or update test expectations
- **Lua test failures**: Check `tests/specs/*.lua` for test expectations
- **Compilation errors**: Fix immediately; don't commit broken code

### 7. Update Version (if needed)

For breaking changes or new features:

```bash
# Update both files to same version
# Cargo.toml: version = "0.X.0"
# lua/tark/init.lua: M.version = '0.X.0'
```

### 8. Clean Git Workflow

**Before committing**:

```bash
# If you have multiple uncommitted changes, use stash
git stash push -m "description of changes"

# Work on one logical change at a time
git stash pop

# Add and commit
git add -A
git commit -m "type: description"
```

**Commit message types**:
- `feat:` - New feature
- `fix:` - Bug fix
- `docs:` - Documentation only
- `refactor:` - Code change that neither fixes a bug nor adds a feature
- `test:` - Adding/updating tests
- `chore:` - Maintenance tasks (deps, config, etc.)
- `perf:` - Performance improvements
- `ci:` - CI/CD changes

### 9. Push and Verify CI

```bash
git push
```

Then check GitHub Actions to ensure CI passes. If CI fails:
1. Fix the issue locally
2. Run all checks again
3. Commit the fix
4. Push again

---

## Pre-Commit Checklist ✅

**Use this before every commit**:

```
□ Code compiles (cargo check/build during dev, cargo build --release before commit)
□ Code formatted (cargo fmt --all)
□ Format verified (cargo fmt --all -- --check)
□ Clippy passes with zero warnings (cargo clippy --all-targets --all-features -- -D warnings)
□ Rust tests pass (cargo test --all-features)
□ Lua tests pass (if applicable: nvim --headless ... PlenaryBustedDirectory)
□ Tests added/updated for code changes
□ Performance verified (for core agent code: no regressions, consider improvements)
□ Documentation updated (README.md, AGENTS.md, doc comments)
□ Versions synced (if needed: Cargo.toml & lua/tark/init.lua)
□ Git history clean (stashed unrelated changes)
□ Conventional commit message
□ Ready to push (CI will pass)
```

---

## Workflow Examples

### Example 1: Adding a New Feature

```bash
# 1. Stash unrelated work
git stash push -m "WIP: other changes"

# 2. Write code for new feature
vim src/new_feature.rs

# 3. Write tests
vim tests/new_feature_test.rs

# 4. Compile and test (use cargo check or debug build during development for speed)
cargo check
cargo test --all-features

# 5. Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 6. Update documentation
vim README.md  # Add to features section
vim AGENTS.md  # Update if architecture changed

# 7. Final verification with release build before commit
cargo build --release && \
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --all-features

# 8. Commit
git add -A
git commit -m "feat: add new feature X"

# 9. Push and verify CI
git push
```

### Example 2: Fixing a Bug

```bash
# 1. Write regression test first (TDD)
vim tests/bug_regression_test.rs
cargo test  # Should fail

# 2. Fix the bug
vim src/buggy_module.rs

# 3. Verify test passes (cargo check for fast type checking)
cargo check
cargo test --all-features

# 4. Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 5. Final release build before commit
cargo build --release

# 6. Update docs if needed
vim README.md  # If user-visible behavior changed

# 7. Commit
git add -A
git commit -m "fix: resolve issue with X causing Y"

# 8. Push
git push
```

### Example 3: Multiple Uncommitted Changes

```bash
# Current state: Multiple unrelated changes in working directory

# 1. Review changes
git status
git diff

# 2. Stash everything
git stash push -m "all uncommitted work"

# 3. Pop and commit one logical unit at a time
git stash pop
git add specific_files_for_feature_A
git stash push -m "remaining work"
git commit -m "feat: add feature A"

# 4. Repeat for next change
git stash pop
git add specific_files_for_bugfix_B
git stash push -m "remaining work"
git commit -m "fix: resolve bug B"

# 5. Continue until all work is committed
git stash list  # Should be empty or only intentional WIP
```

## Getting Help

- Check existing code for patterns
- Read the README.md for user-facing documentation
- Look at GitHub Actions for CI/CD details
- Check issues/PRs for context on decisions
