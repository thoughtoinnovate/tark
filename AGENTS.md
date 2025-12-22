# Agent Guidelines for tark

This document helps AI coding agents understand the tark codebase and make effective contributions.

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
│   ├── tools/              # Agent tools (file ops, grep, shell)
│   └── transport/          # HTTP server and CLI
├── lua/                    # Neovim plugin (Lua)
│   └── tark/
│       ├── init.lua        # Plugin entry point & setup
│       ├── server.lua      # Server management (binary/docker)
│       ├── chat.lua        # Chat UI
│       ├── ghost.lua       # Ghost text completions
│       └── health.lua      # :checkhealth integration
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

- **Run all checks before committing**:
  ```bash
  cargo fmt --all -- --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test --all-features
  ```

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

## Key Files to Understand

| File | Purpose | When to Modify |
|------|---------|----------------|
| `src/agent/chat.rs` | Chat agent logic | Adding agent features |
| `src/tools/mod.rs` | Tool definitions | Adding new tools |
| `src/llm/types.rs` | LLM message types | Changing API contracts |
| `lua/tark/server.lua` | Server management | Binary/Docker handling |
| `lua/tark/init.lua` | Plugin config | Adding config options |
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
```

## Getting Help

- Check existing code for patterns
- Read the README.md for user-facing documentation
- Look at GitHub Actions for CI/CD details
- Check issues/PRs for context on decisions

