# Contributing to tark

Thank you for your interest in contributing to tark! This guide will help you get started.

## Getting Started

### Prerequisites

- **Rust** (1.70+): https://rustup.rs/
- **Neovim** (0.9+): For testing the plugin
- **Docker** (optional): For testing Docker mode

### Clone and Build

```bash
git clone https://github.com/thoughtoinnovate/tark.git
cd tark

# Build the Rust binary
cargo build --release

# Run tests
cargo test --all-features

# Check linting
cargo clippy --all-targets --all-features -- -D warnings
```

### Test the Plugin Locally

```bash
# Start the server
./target/release/tark serve

# In another terminal, open Neovim with the plugin
cd /path/to/tark
nvim --cmd "set rtp+=." -c "lua require('tark').setup({ server = { auto_start = false } })"
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/your-bug-fix
```

### 2. Make Changes

- Follow the code style of existing code
- Add tests for new functionality
- Update documentation if needed

### 3. Run Checks

Before committing, ensure all checks pass:

```bash
# Format code
cargo fmt --all

# Run clippy
cargo clippy --all-targets --all-features -- -D warnings

# Run tests
cargo test --all-features
```

### 4. Commit

Use conventional commit messages:

```bash
git commit -m "feat: add new feature"
git commit -m "fix: resolve bug in X"
git commit -m "docs: update README"
git commit -m "chore: update dependencies"
```

### 5. Push and Create PR

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub.

## Code Style

### Rust

- Use `rustfmt` for formatting (run `cargo fmt`)
- Follow clippy suggestions
- Use `anyhow::Result` for error handling
- Use `tracing` for logging
- Add `#![allow(dead_code)]` for intentionally unused public APIs

```rust
//! Module documentation

use anyhow::Result;

/// Function documentation
pub fn example_function() -> Result<()> {
    tracing::info!("Doing something");
    Ok(())
}
```

### Lua

- Use 4-space indentation
- Use local variables
- Document functions with `---` comments

```lua
--- Function documentation
--- @param opts table Configuration options
--- @return boolean success
local function example_function(opts)
    opts = opts or {}
    return true
end
```

## Project Structure

```
src/
├── main.rs           # CLI entry point
├── lib.rs            # Library exports
├── agent/            # Chat agent
├── completion/       # Code completions
├── config/           # Configuration
├── llm/              # LLM providers
├── lsp/              # LSP server
├── storage/          # Persistent storage
├── tools/            # Agent tools
└── transport/        # HTTP/CLI

lua/tark/
├── init.lua          # Plugin setup
├── server.lua        # Server management
├── chat.lua          # Chat UI
├── ghost.lua         # Ghost text
└── health.lua        # Health checks
```

## Adding Features

### New Tool

1. Create `src/tools/your_tool.rs`
2. Implement the `Tool` trait
3. Register in `src/tools/mod.rs`
4. Update agent prompts if needed

### New LLM Provider

1. Create `src/llm/your_provider.rs`
2. Implement the `LlmProvider` trait
3. Export in `src/llm/mod.rs`
4. Add configuration options

### New Config Option

1. Add default in `lua/tark/init.lua`
2. Use in the relevant module
3. Document in README.md

## Testing

### Rust Tests

```bash
# Run all tests
cargo test --all-features

# Run specific test
cargo test test_name

# Run with output
cargo test -- --nocapture
```

### Manual Testing

1. Build: `cargo build --release`
2. Start server: `./target/release/tark serve`
3. Test in Neovim with the plugin loaded

## Documentation

- Update `README.md` for user-facing changes
- Update `AGENTS.md` for architecture changes
- Add inline documentation for code

## Release Process

Releases are automated via GitHub Actions when a tag is pushed:

```bash
# Update versions in:
# - Cargo.toml
# - lua/tark/init.lua

git add -A
git commit -m "chore: bump version to v0.2.0"
git tag v0.2.0
git push && git push --tags
```

This triggers:
1. Multi-platform binary builds
2. SHA256 checksum generation
3. GitHub Release creation
4. Docker image builds

## Getting Help

- **Issues**: Open an issue for bugs or feature requests
- **Discussions**: Use GitHub Discussions for questions
- **Code**: Check `AGENTS.md` for architecture details

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

---

Thank you for contributing! 🎉

