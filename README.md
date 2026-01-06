# tark

AI-powered CLI agent with TUI chat interface and Neovim integration.

## Features

- **TUI Chat Interface**: Interactive terminal chat with AI assistant
- **Neovim Integration**: Socket-based communication with your editor
- **File Operations**: Read, write, and search files through chat
- **Shell Commands**: Execute commands directly from chat
- **Image Attachments**: Paste images from clipboard with `Ctrl-v`
- **Multi-Provider**: Supports Claude (Anthropic), OpenAI, Google (Gemini), GitHub Copilot, and local Ollama models
- **Usage Dashboard**: Track costs, tokens, and sessions with interactive web dashboard
- **Agent Modes**: Plan (read-only), Build (full access)

## Quick Install

```bash
# Option 1: Binary install (Linux/macOS)
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash

# Option 2: Docker
docker pull ghcr.io/thoughtoinnovate/tark:alpine
```

## Installation

### 1. Install the tark Binary

#### Option A: Install Script (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/thoughtoinnovate/tark/main/install.sh | bash
```

#### Option B: Manual Download

Download from [GitHub Releases](https://github.com/thoughtoinnovate/tark/releases):

| Platform | Binary |
|----------|--------|
| **Linux x64** | [tark-linux-x86_64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64) |
| **Linux ARM64** | [tark-linux-arm64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-arm64) |
| **macOS Intel** | [tark-darwin-x86_64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-darwin-x86_64) |
| **macOS Apple Silicon** | [tark-darwin-arm64](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-darwin-arm64) |
| **Windows x64** | [tark-windows-x86_64.exe](https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-windows-x86_64.exe) |

```bash
# Example: Linux
curl -L https://github.com/thoughtoinnovate/tark/releases/latest/download/tark-linux-x86_64 -o tark
chmod +x tark
sudo mv tark /usr/local/bin/
```

#### Option C: Build from Source

```bash
cargo install --git https://github.com/thoughtoinnovate/tark.git
```

#### Verify Installation

```bash
tark --version
```

### 2. Set API Key

```bash
# For OpenAI
export OPENAI_API_KEY="your-api-key"

# Or for Claude
export ANTHROPIC_API_KEY="your-api-key"

# Or for Google Gemini
export GOOGLE_API_KEY="your-api-key"

# Or for GitHub Copilot (interactive auth)
tark chat  # Then use /auth command

# Or for local Ollama (no key needed)
ollama serve
```

### 3. Install Neovim Plugin

#### lazy.nvim (Recommended)

```lua
return {
    "thoughtoinnovate/tark",
    lazy = false,
    keys = {
        { "<leader>tc", "<cmd>TarkToggle<cr>", desc = "Toggle tark chat" },
    },
}
```

The plugin automatically downloads the correct binary for your platform.

#### Full Config (all options)

```lua
return {
    "thoughtoinnovate/tark",
    lazy = false,
    keys = {
        { "<leader>tc", "<cmd>TarkToggle<cr>", desc = "Toggle tark chat" },
        { "<leader>to", "<cmd>TarkOpen<cr>", desc = "Open tark chat" },
        { "<leader>tx", "<cmd>TarkClose<cr>", desc = "Close tark chat" },
    },
    opts = {
        -- Binary path (auto-detected if nil)
        binary = nil,
        
        -- Window settings for TUI
        window = {
            position = 'right',  -- 'right', 'left', 'bottom', 'top', 'float'
            width = 0.4,         -- 40% of screen (or columns if > 1)
            height = 0.5,        -- 50% of screen (or rows if > 1)
        },
        
        -- Auto-download binary if not found
        auto_download = true,
        
        -- Ghost text settings (inline suggestions like Copilot)
        ghost = {
            enabled = true,  -- Enable ghost text completions
            auto_trigger = true,  -- Auto-trigger on typing
            debounce_ms = 300,  -- Debounce delay
            accept_key = '<Tab>',  -- Key to accept suggestion
        },
        
        -- LSP settings for completion menu (optional, disabled by default)
        lsp = {
            enabled = false,  -- Enable LSP for nvim-cmp integration
            exclude_filetypes = { 'TelescopePrompt', 'NvimTree', 'neo-tree' },
        },
    },
}
```

## Commands

### TUI Commands

| Command | Description |
|---------|-------------|
| `:Tark` | Toggle tark TUI |
| `:TarkToggle` | Toggle tark TUI (show/hide) |
| `:TarkOpen` | Open tark TUI |
| `:TarkClose` | Close tark TUI |
| `:TarkDownload` | Download tark binary |
| `:TarkVersion` | Show tark version |

### Ghost Text Commands (Inline Suggestions)

| Command | Description |
|---------|-------------|
| `:TarkGhostEnable` | Enable ghost text suggestions |
| `:TarkGhostDisable` | Disable ghost text suggestions |
| `:TarkGhostToggle` | Toggle ghost text on/off |
| `:TarkGhostUsage` | Show ghost text usage stats |

**Usage:** Type in insert mode and suggestions appear as grey text. Press `<Tab>` to accept.

### LSP Commands (Completion Menu)

| Command | Description |
|---------|-------------|
| `:TarkLspStart` | Start tark LSP server |
| `:TarkLspStop` | Stop tark LSP server |
| `:TarkLspRestart` | Restart tark LSP server |
| `:TarkLspStatus` | Show tark LSP status |
| `:TarkLspEnable` | Enable LSP completions |
| `:TarkLspDisable` | Disable LSP completions |
| `:TarkLspToggle` | Toggle LSP completions on/off |
| `:TarkLspUsage` | Show LSP usage stats |

## Usage

### Standalone (Terminal)

```bash
# Start chat in any terminal
tark chat

# With specific model
tark chat --model gpt-4o

# In a specific directory
cd /my/project && tark chat
```

### In Neovim

Press `<leader>tc` (or your configured keymap) to toggle the chat window.

### Chat Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all commands |
| `/model` | Open provider/model picker |
| `/plan` | Switch to Plan mode (read-only) |
| `/build` | Switch to Build mode (full access) |
| `/clear` | Clear chat history |
| `/attach <file>` | Attach a file |
| `/sessions` | List and switch sessions |
| `/new` | Start a new session |
| `/usage` | Show usage stats |
| `/exit` | Close chat |

### Keyboard Shortcuts

| Key | Description |
|-----|-------------|
| `Ctrl-v` | Paste image from clipboard |
| `@filepath` | Inline file attachment |
| `j/k` | Vim-style navigation in messages |
| `Tab` | Toggle Plan ↔ Build mode |
| `Ctrl-c` | Cancel current request |

### Agent Modes

| Mode | Access | Use For |
|------|--------|---------|
| **Plan** | Read-only | Explore, analyze, propose changes |
| **Build** | Full access | Execute changes, run commands |

## CLI Usage

```bash
# Start TUI chat
tark chat

# Show usage statistics
tark usage

# Serve HTTP API
tark serve --port 8765

# Show version
tark --version
```

## Health Check

```vim
:checkhealth tark
```

## Configuration

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

### Project Config (`.tark/`)

Create `.tark/` in your project for local settings:

```
.tark/
├── config.toml      # Project settings
├── rules/           # Custom instructions
│   └── style.md
└── conversations/   # Saved sessions
```

## Architecture

```
┌─────────────────────────────────────────┐
│              Neovim                      │
│  ┌─────────────────────────────────┐    │
│  │         Terminal Window          │    │
│  │    (tark chat --socket ...)     │    │
│  └──────────────┬──────────────────┘    │
└─────────────────┼────────────────────────┘
                  │ Unix Socket
                  ▼
       ┌───────────────────────┐
       │      tark TUI         │
       │   (Rust + ratatui)    │
       ├───────────────────────┤
       │  ┌─────────────────┐  │
       │  │  Agent + Tools  │  │
       │  └────────┬────────┘  │
       └───────────┼───────────┘
                   │
       ┌───────────┴───────────┐
       │     LLM Providers      │
       │ OpenAI│Claude│Gemini│  │
       │ Copilot│Ollama         │
       └───────────────────────┘
```

## Security

- API keys are **only** sent to official provider endpoints
- No telemetry or data collection
- Local Ollama option for fully offline usage
- All binaries are built via GitHub Actions (transparent, auditable)
- SHA256 checksums for all release artifacts

## License

MIT
