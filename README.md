# tark

AI-powered CLI agent with TUI chat interface and Neovim integration.

## Features

- **TUI Chat Interface**: Interactive terminal chat with AI assistant
- **Editor Adapters**: Neovim adapter now lives in the plugins monorepo and uses editor adapter APIs
- **File Operations**: Read, write, and search files through chat
- **Shell Commands**: Execute commands directly from chat
- **Image Attachments**: Paste images from clipboard with `Ctrl-v`
- **Multi-Provider**: Supports Claude (Anthropic), OpenAI, Google (Gemini), GitHub Copilot, and local Ollama models
- **Usage Dashboard**: Track costs, tokens, and sessions with interactive web dashboard
- **Agent Modes**: Ask (read-only), Plan (propose changes), Build (full access)
- **Approval System**: Approve risky operations with pattern matching

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
export GEMINI_API_KEY="your-api-key"

# Or for GitHub Copilot (Device Flow OAuth)
tark auth copilot

# Or for local Ollama (no key needed)
ollama serve
```

### 3. Install Neovim Adapter Plugin

Neovim adapter source has moved to the plugins monorepo:
`../plugins/tark/editors/neovim` (repository path `thoughtoinnovate/plugins/tark/editors/neovim`).

#### Local checkout (Recommended for now)

```lua
return {
    dir = "~/code/plugins/tark/editors/neovim",
    lazy = false,
    keys = {
        { "<leader>tc", "<cmd>TarkToggle<cr>", desc = "Toggle tark chat" },
    },
}
```

The adapter plugin automatically downloads the correct binary for your platform.

#### Lazy.nvim from monorepo (recommended for most users)

```lua
return {
    url = "https://github.com/thoughtoinnovate/plugins",
    dir = "tark/editors/neovim",
    lazy = false,
    keys = {
        { "<leader>tc", "<cmd>TarkToggle<cr>", desc = "Toggle tark chat" },
    },
}
```

Note: Lazy.nvim will clone the monorepo, but it will only load the Neovim adapter from `tark/editors/neovim`.

#### Full Config (all options)

```lua
return {
    dir = "~/code/plugins/tark/editors/neovim",
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

### Slash Commands

| Command | Description |
|---------|-------------|
| `/help` | Show all available commands |
| `/model` | Open provider/model picker |
| `/provider` | Open provider picker |
| `/theme` | Open theme picker with live preview |
| `/diff [auto\|inline\|split]` | Set diff preview mode |
| `/ask` | Switch to Ask mode (read-only) |
| `/plan` | Switch to Plan mode (propose changes) |
| `/build` | Switch to Build mode (full access) |
| `/trust` | Open trust level selector |
| `/clear` | Clear chat history |
| `/clear-costs` | Reset session usage totals |
| `/compact` | Manually compact context window |
| `/attach <file>` | Attach a file to context |
| `/file` | Open file picker |
| `/sessions` | List all sessions |
| `/session <id>` | Switch to specific session |
| `/new` | Start a new session |
| `/export [path]` | Export session to JSON |
| `/import <path>` | Import session from JSON |
| `/tools` | Show available tools |
| `/plugins` | Show installed plugins |
| `/usage` | Show usage statistics |
| `/exit` | Close chat |

### Keyboard Shortcuts

#### Global

| Key | Description |
|-----|-------------|
| `Ctrl+C` | Cancel LLM request / Quit (when idle) |
| `Ctrl+Q` | Quit application |
| `Ctrl+?` | Toggle help modal |
| `Tab` | Cycle focus (Input → Messages → Sidebar) |
| `Shift+Tab` | Cycle agent mode (Build → Plan → Ask) |
| `Ctrl+B` | Toggle sidebar visibility |
| `Ctrl+T` | Toggle model-level thinking |
| `Ctrl+M` | Cycle build mode (Manual → Balanced → Careful) |
| `Ctrl+Shift+B` | Open trust level selector (Build mode only) |
| `Esc` | Close modal / Cancel operation |
| `Esc Esc` | Cancel ongoing agent operation (double-tap) |

#### Input Area

| Key | Description |
|-----|-------------|
| `Enter` | Send message |
| `Shift+Enter` | Insert newline |
| `Ctrl+Left/Right` | Word navigation |
| `Home/End` | Line start/end |
| `Up/Down` | Input history navigation |
| `@` | Open file picker for attachments (Enter toggles, Esc closes) |
| `Ctrl+V` | Paste text or attach clipboard image |

Selected files and folders are inserted into the prompt as `@path` tokens. Removing a token from the prompt clears the matching attachments; folders appear as a single token with their file count.

#### Message Area (Vim-style)

| Key | Description |
|-----|-------------|
| `j/k` | Move focus to next/previous message |
| `v` | Start visual selection in focused message |
| `h/l` | Move cursor left/right (message selection) |
| `w/b` | Next/previous word (message selection) |
| `0/$` | Line start/end (message selection) |
| `y` | Yank selection (visual) / yank message (normal) |
| `Enter` | Toggle collapse for focused tool/group or tool item |
| `Right` | Enter a tool group |
| `-` | Exit a tool group |

#### Sidebar Navigation

| Key | Description |
|-----|-------------|
| `j/k` | Navigate items |
| `Enter` | Expand/collapse panel or select item |
| `h/l` | Exit/enter panel |
| `d` | Delete context file (in Context panel) |
| `e` | Edit task (in Tasks panel) |
| `D` | Delete task (in Tasks panel) |
| `J/K` | Move task up/down (in Tasks panel) |

### Agent Modes

| Mode | Access | Use For |
|------|--------|---------|
| **Ask** | Read-only | Explore, analyze, understand code |
| **Plan** | Read-only + propose | Propose changes as diffs without applying |
| **Build** | Full access | Execute changes, run commands |

Diff previews render inline on narrow terminals and side-by-side when there is enough width.

### Trust Levels (Build Mode)

Control how risky operations are approved. Use `/trust` command or `Ctrl+Shift+B` in Build mode:

| Trust Level | Description |
|-------------|-------------|
| **Manual** | All tool executions require explicit approval |
| **Balanced** | Auto-approve reads; prompt for writes and risky ops (default) |
| **Careful** | Auto-approve most ops; only prompt for dangerous operations |

When prompted for approval, you can:
- `Y` - Approve once
- `S` - Approve for this session (pattern-matched)
- `A` - Always approve (persisted to `.tark/approvals.json`)
- `N` - Deny once
- `D` - Always deny (persisted)

### Task Queue

When you send multiple messages while the agent is working, they're queued and processed in order.

**Managing queued tasks (in Sidebar → Tasks panel):**
- `e` - Edit a queued task before it runs
- `D` - Delete a queued task
- `J/K` - Reorder tasks in the queue

### Context & Archives

When context is compacted, Tark keeps the latest summary in the active conversation and archives older messages. Archived chunks can be loaded from the top of the message list; they are UI-only and not sent to the LLM.

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
# Default provider (tark_sim is built-in for testing, no API key required)
default_provider = "tark_sim"

[llm.tark_sim]
model = "tark_llm"
max_tokens = 8192

[llm.openai]
model = "gpt-4o"
max_tokens = 4096

[llm.claude]
model = "claude-sonnet-4-20250514"

[llm.ollama]
model = "codellama"

[server]
port = 8765

[tui]
theme = "catppuccin_mocha"
plugin_widget_poll_ms = 2000
session_usage_poll_ms = 1000

[tools]
shell_enabled = true
tool_timeout_secs = 60
```

Tool calls can override the default timeout by including `timeout_secs` in their arguments.

### Project Config (`.tark/`)

Create `.tark/` in your project for local settings:

```
.tark/
├── config.toml      # Project settings
├── rules/           # Custom instructions
│   └── style.md
└── conversations/   # Saved sessions
```

## Statusline Integration

Show Tark status in your statusline with a nice icon:

### With Lualine

```lua
require('lualine').setup({
    sections = {
        lualine_x = {
            -- Full: icon + "tark"
            require('tark.statusline').lualine,
            
            -- Or compact: icon only
            -- require('tark.statusline').lualine_icon,
        }
    }
})
```

### Manual Statusline

```lua
-- In your statusline string
vim.o.statusline = "%f %m %= %{%v:lua.require('tark.statusline').status()%}"

-- Or with highlights
vim.o.statusline = "%f %m %= " .. require('tark.statusline').status_with_hl()
```

### Status Icons (Nerd Fonts)

| Icon | Status | Meaning |
|------|--------|---------|
| 󱙺 | Active | Completions working |
| 󰊠 | Idle | Ready, no recent activity |
| 󰌆 | No Key | Missing API key |
| 󰚌 | Disabled | Ghost text disabled |
|  | Error | Binary not found |

## Docs

### Setup & Configuration
- [TUI Setup Guide](docs/TUI_SETUP.md) - Terminal requirements, provider configuration
- [Provider Setup (Copilot/Gemini/OpenRouter)](docs/NEW_PROVIDERS.md)

### Customization
- [Theme System](docs/THEMES.md) - Full theme documentation
- [Theme Quick Start](docs/THEME_QUICKSTART.md) - Quick guide to theme switching

### Architecture
- [BFF Architecture](docs/BFF_ARCHITECTURE.md) - Backend-for-Frontend design
- [Architecture & Agent Guidelines](AGENTS.md)
- [External Agent Integration (RFC)](docs/EXTERNAL_AGENTS_ARCHITECTURE.md)

### Development
- [TUI Modal Design Guide](docs/TUI_MODAL_DESIGN_GUIDE.md) - Design patterns for modals
- [Contributing Guide](CONTRIBUTING.md)
- [Neovim Adapter Tests](../plugins/tark/editors/neovim/tests/README.md)

## Architecture

The TUI uses a Backend-for-Frontend (BFF) architecture separating UI from business logic:

```
┌─────────────────────────────────────────────────────────┐
│                    Presentation Layer                    │
│  ┌───────────────────────────────────────────────────┐  │
│  │              TuiRenderer (ratatui)                │  │
│  │   • Widgets: Header, Messages, Input, Sidebar     │  │
│  │   • Modals: Provider, Model, Theme, Help, etc.    │  │
│  └───────────────────────┬───────────────────────────┘  │
└──────────────────────────┼──────────────────────────────┘
                           │ Commands / Events
                           ▼
┌─────────────────────────────────────────────────────────┐
│                      BFF Layer                           │
│  ┌─────────────────┐  ┌─────────────────┐              │
│  │  TuiController  │─▶│   AppService    │              │
│  └─────────────────┘  └────────┬────────┘              │
│                                │                        │
│  ┌─────────────────────────────┼─────────────────────┐ │
│  │              SharedState (Arc<RwLock>)            │ │
│  │   • messages, streaming, provider, model          │ │
│  │   • agent_mode, build_mode, trust_level           │ │
│  │   • sidebar_data, context_files, tasks            │ │
│  └───────────────────────────────────────────────────┘ │
└────────────────────────────┬────────────────────────────┘
                             │
┌────────────────────────────┼────────────────────────────┐
│                     Domain Layer                         │
│  ┌──────────────┐  ┌─────────────┐  ┌────────────────┐ │
│  │  ChatAgent   │  │   Tools     │  │  LLM Provider  │ │
│  │  + Context   │  │  Registry   │  │   (multiple)   │ │
│  └──────────────┘  └─────────────┘  └────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

See [BFF Architecture](docs/BFF_ARCHITECTURE.md) for details.

## Plugins

Tark supports a WASM-based plugin system for extending functionality.

### Plugin Management

```bash
# List installed plugins
tark plugin list

# Install a plugin from git
tark plugin add https://github.com/user/tark-plugin

# Update a plugin from its recorded source
tark plugin update <plugin-id>
tark plugin update --all

# Show plugin details
tark plugin info <plugin-id>

# Run OAuth for a plugin
tark plugin auth <plugin-id>

# Enable/disable plugins
tark plugin enable <plugin-id>
tark plugin disable <plugin-id>

# Uninstall a plugin
tark plugin remove <plugin-id>
```

### Plugin Types

| Type | Purpose |
|------|---------|
| `auth` | Add authentication methods (OAuth, API keys) |
| `tool` | Add agent capabilities |
| `provider` | Add LLM providers |
| `channel` | Add messaging channels (Slack, Discord, Signal) |
| `hook` | Lifecycle event handlers |

### Remote Channels

Run channel plugins in remote mode to steer Tark from chat systems:

```bash
# Interactive TUI + Discord control
tark --remote discord

# Headless remote mode (prints live events to stdout)
tark --headless --remote discord

# Inspect or manage remote sessions
tark show all
tark show <session-id>
tark stop <session-id>
tark resume <session-id>

```

From your channel (Discord/Slack/etc.), you can also run `/tark interrupt` to cancel a running task.
If a remote prompt requires input, the local TUI will surface it and you can reply from the TUI prompt or from the remote channel.

Remote access is gated by allowlists in `.tark/config.toml`:

```toml
[remote]
http_enabled = false
max_message_chars = 1800
allowed_plugins = ["discord"]
allowed_users = ["1234567890"]
allowed_guilds = ["0987654321"]
allowed_channels = ["55555555"]
allowed_roles = ["role-id"]
allow_model_change = true
allow_mode_change = true
allow_trust_change = false
require_allowlist = true
```

Logs are written to `.tark/logs/remote` (error-only by default; use `--remote-debug` for full logs).

See `docs/REMOTE_CHANNELS.md` for Discord setup details.

### Building Plugins

- **[Plugin Development Guide](docs/PLUGIN_DEVELOPMENT.md)** - Step-by-step guide for building WASM plugins
- **[Plugin SDK Documentation](docs/PLUGIN_SDK.md)** - WIT interface reference and API details

## Security

- API keys are **only** sent to official provider endpoints
- No telemetry; usage/cost tracking is stored locally (`.tark/usage.db`)
- Model metadata/pricing fetched from [models.dev](https://models.dev) for capability detection (no API keys sent)
- Local Ollama option for fully offline LLM usage
- All binaries are built via GitHub Actions (transparent, auditable)
- SHA256 checksums for all release artifacts
- Plugins run in WASM sandbox with capability-based security

## License

Apache-2.0 - See [LICENSE](LICENSE) for details.
