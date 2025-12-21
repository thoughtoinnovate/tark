# tark (तर्क)

> **Tark** means "Logic" or "Reasoning" in Sanskrit

AI-powered CLI agent with LSP server for code completion, hover, diagnostics, and chat.

## Features

- **Ghost Text Completions**: Cursor-style inline completions with Tab to accept
- **Chat Agent**: Interactive chat with tools for file operations and shell commands
- **LSP-Powered Tools**: Go to definition, find references, call hierarchy using tree-sitter
- **Multi-Provider**: Supports Claude (Anthropic), OpenAI, and local Ollama models
- **Context Tracking**: Real-time token usage and cost estimation
- **Agent Modes**: Plan (read-only), Build (full access), Review (approval required)
- **blink.cmp Integration**: Works seamlessly with blink.cmp - no config needed!
- **Auto-Start Server**: Server starts automatically when Neovim opens

## Quick Install

```bash
# Option 1: Docker (easiest - no installation needed!)
docker pull ghcr.io/thoughtoinnovate/tark:alpine

# Option 2: Binary install (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash
```

## Installation

### 1. Install the tark Binary

#### Option A: Install Script (Recommended)

The install script automatically detects your platform and installs the correct binary:

```bash
# Auto-detect platform and install
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash

# Or with options
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash -s -- --install-dir ~/.local/bin
```

#### Option B: Manual Download

Download from [GitHub Releases](https://github.com/thoughtoinnovate/tark/releases):

| Platform | Binary | Notes |
|----------|--------|-------|
| **Linux (Any Distro)** | [tark-linux-x86_64-musl](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64-musl) | Static binary - works everywhere |
| Linux (glibc) | [tark-linux-x86_64-gnu](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64-gnu) | Ubuntu, Debian, Fedora, etc. |
| Linux ARM64 (Any) | [tark-linux-arm64-musl](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-arm64-musl) | Static ARM64 binary |
| Linux ARM64 (glibc) | [tark-linux-arm64-gnu](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-arm64-gnu) | ARM64 with glibc |
| **macOS Intel** | [tark-darwin-x86_64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-darwin-x86_64) | |
| **macOS Apple Silicon** | [tark-darwin-arm64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-darwin-arm64) | M1/M2/M3 |
| **FreeBSD** | [tark-freebsd-x86_64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-freebsd-x86_64) | |
| **Windows x64** | [tark-windows-x86_64.exe](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-windows-x86_64.exe) | |

> **Tip:** Use the `-musl` binaries for universal Linux compatibility (Alpine, Arch, NixOS, Void, etc.)

```bash
# Example: Universal Linux binary (works on any distro)
curl -L https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64-musl -o tark
chmod +x tark
sudo mv tark /usr/local/bin/
```

#### Option C: Docker (No Installation Needed!)

Docker is the easiest way - the plugin will automatically pull and run the container:

```bash
# Just make sure Docker is installed and running
docker --version

# The plugin handles the rest automatically!
```

**Two image options:**
| Image | Size | Description |
|-------|------|-------------|
| `ghcr.io/thoughtoinnovate/tark:latest` | ~15MB | Minimal (scratch), binary + certs only |
| `ghcr.io/thoughtoinnovate/tark:alpine` | ~30MB | Alpine-based, includes shell for debugging |

Or run manually:
```bash
# Minimal image (default)
docker run -d --name tark-server \
  -p 8765:8765 \
  -v $(pwd):/workspace \
  -e OPENAI_API_KEY=$OPENAI_API_KEY \
  ghcr.io/thoughtoinnovate/tark:latest

# Alpine image (for debugging)
docker run -it --name tark-server \
  -p 8765:8765 \
  ghcr.io/thoughtoinnovate/tark:alpine sh
```

#### Option D: Build from Source (Requires Rust)

```bash
cargo install --git https://github.com/thoughtoinnovate/tark.git
```

#### Verify Installation

```bash
# Binary
tark --version

# Or Docker
docker run --rm ghcr.io/thoughtoinnovate/tark:latest --version
```

### 2. Set API Key

```bash
# For OpenAI (recommended)
export OPENAI_API_KEY="your-api-key"

# Or for Claude
export ANTHROPIC_API_KEY="your-api-key"

# Or for local Ollama (no key needed)
ollama serve  # start Ollama
```

### 3. Install Neovim Plugin

#### lazy.nvim / LazyVim (Recommended)

Add to your `lua/plugins/tark.lua`:

```lua
return {
    "thoughtoinnovate/tark",
    lazy = false,
    dependencies = {
        { "saghen/blink.cmp", optional = true },  -- Optional: Tab integration
    },
    keys = {
        { "<leader>ec", "<cmd>TarkChatToggle<cr>", desc = "Toggle tark chat" },
        { "<leader>eg", "<cmd>TarkGhostToggle<cr>", desc = "Toggle ghost text" },
        { "<leader>es", "<cmd>TarkServerStatus<cr>", desc = "Server status" },
    },
    opts = {
        server = {
            auto_start = true,  -- Automatically start server when Neovim opens
        },
        ghost_text = { enabled = true },
        chat = { enabled = true },
    },
}
```

That's it! The server starts automatically - no manual setup needed.

#### packer.nvim

```lua
use {
    'thoughtoinnovate/tark',
    config = function()
        require('tark').setup({
            server = { auto_start = true },
        })
    end
}
```

### 4. Server Management

The server starts automatically by default. You can also manage it manually:

| Command | Description |
|---------|-------------|
| `:TarkServerStart` | Start the server |
| `:TarkServerStop` | Stop the server |
| `:TarkServerStatus` | Check server status |
| `:TarkServerRestart` | Restart the server |

Or start manually in a terminal:
```bash
tark serve
```

## Usage

### Keybindings

| Key | Mode | Description |
|-----|------|-------------|
| `<leader>ec` | Normal | Toggle chat window |
| `<leader>es` | Normal | Show server status |
| `<leader>eg` | Normal | Toggle ghost text |
| `Tab` | Insert | Accept ghost text (or blink.cmp) |
| `Ctrl+]` | Insert | Accept ghost text (always) |
| `Ctrl+Space` | Insert | Trigger completion manually |
| `Tab` | Chat | Toggle Plan ↔ Build mode |

### Chat Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/model` | Open provider/model picker |
| `/plan` | Switch to Plan mode (read-only) |
| `/build` | Switch to Build mode (full access) |
| `/thinking` | Toggle verbose output |
| `/clear` | Clear chat history |
| `/exit` | Close chat window |

### Agent Modes

| Mode | Access | Use For |
|------|--------|---------|
| **Plan** | Read-only | Explore, analyze, propose changes |
| **Build** | Full access | Execute changes, run commands |
| **Review** | Approval needed | Careful modifications |

### Special Features

- Type `@` for file autocompletion
- Type `/` for command autocompletion  
- Context usage shown in title bar
- Real-time thinking display with tool calls

## Health Check

```vim
:checkhealth tark
```

## Configuration

### Neovim Options

```lua
require('tark').setup({
    -- Server settings
    server = {
        auto_start = true,       -- Auto-start server when Neovim opens
        mode = 'auto',           -- 'auto', 'binary', or 'docker'
        binary = 'tark',         -- Path to tark binary (if using binary mode)
        host = '127.0.0.1',
        port = 8765,
        stop_on_exit = true,     -- Stop server when Neovim exits
    },
    -- Docker settings (used when mode = 'docker' or 'auto' without binary)
    docker = {
        image = 'ghcr.io/thoughtoinnovate/tark:alpine',  -- Alpine by default (has shell)
        container_name = 'tark-server',
        pull_on_start = true,    -- Pull latest image before starting
        build_local = false,     -- Build from plugin's Dockerfile (no Rust needed!)
        dockerfile = 'alpine',   -- 'alpine' (~30MB, shell) or 'minimal' (~15MB, scratch)
        mount_workspace = true,  -- Mount cwd into container for file access
    },
    -- Ghost text (inline completions)
    ghost_text = {
        enabled = true,
        debounce_ms = 150,       -- Delay before requesting completion
        hl_group = 'Comment',    -- Highlight group for ghost text
    },
    -- Chat window
    chat = {
        enabled = true,
        window = {
            sidepane_width = 0.35,  -- 35% of editor width
            border = 'rounded',
        },
    },
    -- LSP (disabled by default to avoid conflicts)
    lsp = {
        enabled = false,
    },
})
```

### Server Modes

| Mode | Description |
|------|-------------|
| `auto` | Use binary if available, fallback to Docker (default) |
| `binary` | Only use local binary |
| `docker` | Only use Docker container |

### Docker Options

| Option | Description |
|--------|-------------|
| `pull_on_start` | Pull latest image from registry (default: true) |
| `build_local` | Build image from plugin's Dockerfile (default: false) |
| `dockerfile` | Image type: `'alpine'` (~30MB, has shell, default) or `'minimal'` (~15MB, scratch) |

#### Build Docker Image Locally

If you want to build from source without installing Rust:

```lua
-- lua/plugins/tark.lua
return {
    "thoughtoinnovate/tark",
    lazy = false,
    opts = {
        server = { mode = 'docker' },
        docker = { 
            build_local = true,    -- Build from Dockerfile in plugin directory
            pull_on_start = false, -- Don't pull from registry
            -- dockerfile = 'alpine', -- Default, or use 'minimal' for smaller image
        },
    },
}
```

Or build manually with:
```vim
:TarkDockerBuild
```

This builds the image using Docker on your machine - no Rust toolchain required!

**Image sizes:**
- `alpine` (default): ~30MB - Includes shell and curl for debugging
- `minimal`: ~15MB - Super lightweight, binary + CA certs only (no shell)

### CLI Config (`~/.config/tark/config.toml`)

```toml
[llm]
default_provider = "openai"

[llm.openai]
model = "gpt-4o"
max_tokens = 4096

[llm.claude]
model = "claude-sonnet-4-20250514"

[llm.ollama]
model = "codellama"

[server]
port = 8765

[tools]
shell_enabled = true
```

## LSP-Powered Tools

The agent has access to intelligent code understanding:

| Tool | Description |
|------|-------------|
| `list_symbols` | List functions/classes/types in a file |
| `go_to_definition` | Jump to where a symbol is defined |
| `find_all_references` | Find all usages of a symbol |
| `call_hierarchy` | Trace who calls what |
| `get_signature` | Get function signature and docs |
| `codebase_overview` | Get project structure overview |

## Project Config (`.tark/`)

Create `.tark/` in your project for local settings:

```
.tark/
├── config.toml      # Project settings
├── rules/           # Custom instructions
│   └── style.md
├── agents/          # Custom agent configs
└── conversations/   # Saved sessions
```

## Architecture

```
┌─────────────────────────────────────────┐
│              Neovim                      │
│  ┌─────────────┐  ┌─────────────────┐   │
│  │ Ghost Text  │  │      Chat       │   │
│  │   (Tab)     │  │   (<leader>ec)  │   │
│  └──────┬──────┘  └────────┬────────┘   │
└─────────┼──────────────────┼────────────┘
          │                  │
          └────────┬─────────┘
                   ▼
       ┌───────────────────────┐
       │    tark serve         │
       │    (HTTP :8765)       │
       ├───────────────────────┤
       │  ┌─────┐ ┌─────────┐  │
       │  │ FIM │ │  Agent  │  │
       │  │     │ │ + Tools │  │
       │  └──┬──┘ └────┬────┘  │
       └─────┼─────────┼───────┘
             │         │
     ┌───────┴─────────┴───────┐
     │      LLM Providers       │
     │  OpenAI │ Claude │ Ollama│
     └─────────────────────────┘
```

## Security

### Binary Verification

All release binaries include SHA256 checksums for verification:

```bash
# The install script automatically verifies checksums
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash

# Manual verification
curl -L https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64.sha256
sha256sum tark-linux-x86_64  # Compare with downloaded checksum
```

You can also verify your installed binary in Neovim:
```vim
:checkhealth tark
```

This shows the SHA256 hash of your installed binary which you can compare against the official release.

### Privacy & Security

- API keys are **only** sent to official provider endpoints
- No telemetry or data collection
- Local Ollama option for fully offline usage
- All binaries are built via GitHub Actions (transparent, auditable)
- SHA256 checksums for all release artifacts

## License

MIT
