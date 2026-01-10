# Agent Guidelines for tark

This document helps AI coding agents understand the tark codebase and make effective contributions.

---

## 🚨 CRITICAL: Developer Workflow Requirements

**Before making ANY changes, read the [Developer Workflow](#developer-workflow-) section.**

**Every code change MUST**:

1. ✅ **Compile** - `cargo build --release` must pass
2. ✅ **Be formatted** - `cargo fmt --all` must be run
3. ✅ **Pass linting** - `cargo clippy` with zero warnings
4. ✅ **Pass all tests** - `cargo test --all-features` must succeed
5. ✅ **Include/update tests** - No code change without corresponding tests
6. ✅ **Update documentation** - README.md, AGENTS.md, or doc comments for significant changes
7. ✅ **Use clean git workflow** - Stash unrelated changes, commit logical units separately

**See the [Pre-Commit Checklist](#pre-commit-checklist-) for the complete workflow.**

---

## Project Overview

**tark** is an AI-powered CLI agent with LSP server for Neovim. It provides:
- Ghost text completions (like Cursor/Copilot)
- Chat interface with file/shell tools
- Multi-provider LLM support (OpenAI, Claude, Ollama)

## Architecture

```
tark/
├── src/                    # Rust backend (tark server)
│   ├── main.rs             # CLI entry point
│   ├── lib.rs              # Library exports
│   ├── agent/              # Chat agent with tool execution
│   ├── completion/         # FIM (fill-in-middle) completions
│   ├── config/             # Configuration management
│   ├── diagnostics/        # Code analysis
│   ├── llm/                # LLM provider implementations
│   │   ├── claude.rs       # Anthropic Claude
│   │   ├── openai.rs       # OpenAI GPT
│   │   └── ollama.rs       # Local Ollama
│   ├── lsp/                # LSP server implementation
│   ├── storage/            # Persistent storage (.tark/)
│   │   └── usage.rs        # Usage tracking with SQLite
│   ├── tools/              # Agent tools with risk-based categorization
│   │   ├── mod.rs          # Tool registry with mode-based composition
│   │   ├── risk.rs         # RiskLevel and TrustLevel enums
│   │   ├── approval.rs     # ApprovalGate with pattern matching
│   │   ├── readonly/       # Read-only tools (grep, file_preview, safe_shell)
│   │   ├── write/          # Write tools (reserved for future)
│   │   ├── risky/          # Shell tools (reserved for future)
│   │   └── dangerous/      # Destructive tools (reserved for future)
│   ├── tui/                # Terminal UI
│   │   └── widgets/        # TUI components
│   │       ├── approval_card.rs           # Approval request popup
│   │       └── approval_mode_selector.rs  # Trust level selector (Shift+A, Build mode only)
│   └── transport/          # HTTP server and CLI
│       └── dashboard.rs    # Usage dashboard HTML
├── lua/                    # Neovim plugin (Lua)
│   └── tark/
│       ├── init.lua        # Plugin entry point & setup
│       ├── tui.lua         # TUI integration (socket RPC)
│       ├── binary.lua      # Binary find/download/version
│       └── health.lua      # :checkhealth integration
├── plugin/
│   └── tark.lua            # Command registration
├── .github/workflows/      # CI/CD
│   ├── ci.yml              # Tests, build, lint
│   ├── release.yml         # Multi-platform releases
│   └── docker.yml          # Docker image builds
├── Dockerfile              # Minimal scratch image (~15MB)
├── Dockerfile.alpine       # Alpine image with shell (~30MB)
└── install.sh              # Binary installer with checksum verification
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

### Versioning

- **Keep versions in sync**:
  - `Cargo.toml`: `version = "0.1.0"`
  - `lua/tark/init.lua`: `M.version = '0.1.0'`
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

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/agent/chat.rs` | Chat agent logic | Adding agent features |
| `src/tools/mod.rs` | Tool registry & mode composition | Adding/modifying tools |
| `src/tools/risk.rs` | Risk levels & trust levels | Changing risk categories |
| `src/tools/approval.rs` | Approval gate & pattern matching | Changing approval flow |
| `src/tools/readonly/` | Safe read-only tools | Adding read-only tools |
| `src/llm/types.rs` | LLM message types | Changing API contracts |
| `src/completion/engine.rs` | FIM completion logic | Changing completion behavior |
| `src/storage/usage.rs` | Usage tracking & SQLite | Adding usage analytics |
| `src/transport/dashboard.rs` | Usage dashboard HTML | Modifying dashboard UI |
| `src/tui/widgets/approval_card.rs` | Approval popup UI | Changing approval UX |
| `lua/tark/init.lua` | Plugin entry & config | Adding config options |
| `lua/tark/tui.lua` | TUI integration | Socket RPC handlers |
| `lua/tark/binary.lua` | Binary management | Download/version logic |
| `plugin/tark.lua` | Command registration | Adding new commands |
| `.github/workflows/release.yml` | Release automation | Adding platforms |

## Common Tasks

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

### Adding a Config Option

1. Add to `M.config` in `lua/tark/init.lua`
2. Use in relevant module
3. Document in README.md

### Releasing a New Version

```bash
# 1. Update versions
# Cargo.toml: version = "0.2.0"
# lua/tark/init.lua: M.version = '0.2.0'

# 2. Commit
git add -A
git commit -m "chore: bump version to v0.2.0"

# 3. Tag and push
git tag v0.2.0
git push && git push --tags

# GitHub Actions handles the rest!
```

## Testing Locally

```bash
# Run Rust tests
cargo test --all-features

# Check formatting
cargo fmt --all -- --check

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Build release binary
cargo build --release

# Test the server
./target/release/tark serve --port 8765

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
cargo build --release
```

**Why**: Compilation errors must be caught immediately, not in CI.

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
cargo test --all-features

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
# One-liner for all checks
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
□ Code compiles (cargo build --release)
□ Code formatted (cargo fmt --all)
□ Format verified (cargo fmt --all -- --check)
□ Clippy passes with zero warnings (cargo clippy -- -D warnings)
□ Rust tests pass (cargo test --all-features)
□ Lua tests pass (if applicable: nvim --headless ... PlenaryBustedDirectory)
□ Tests added/updated for code changes
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

# 4. Compile and test
cargo build --release
cargo test --all-features

# 5. Format and lint
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings

# 6. Update documentation
vim README.md  # Add to features section
vim AGENTS.md  # Update if architecture changed

# 7. Verify all checks pass
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

# 3. Verify test passes
cargo test --all-features

# 4. Format, lint, and compile
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release

# 5. Update docs if needed
vim README.md  # If user-visible behavior changed

# 6. Commit
git add -A
git commit -m "fix: resolve issue with X causing Y"

# 7. Push
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

