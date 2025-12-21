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

## Installation

### 1. Install the CLI (Rust)

```bash
# Clone and build
git clone https://github.com/thoughtoinnovate/tark.git
cd tark
cargo install --path .

# Or directly from GitHub
cargo install --git https://github.com/thoughtoinnovate/tark.git
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

#### lazy.nvim (Recommended)

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
    },
    opts = {
        ghost_text = { enabled = true },
        chat = { enabled = true },
    },
}
```

That's it! No need to configure blink.cmp separately.

#### packer.nvim

```lua
use {
    'thoughtoinnovate/tark',
    config = function()
        require('tark').setup()
    end
}
```

### 4. Start the Server

```bash
# In a terminal
tark serve
```

Or add to your shell startup:
```bash
# ~/.zshrc or ~/.bashrc
tark serve &>/dev/null &
```

## Usage

### Keybindings

| Key | Mode | Description |
|-----|------|-------------|
| `<leader>ec` | Normal | Toggle chat window |
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
    ghost_text = {
        enabled = true,
        server_url = 'http://localhost:8765',
        debounce_ms = 200,
        hl_group = 'Comment',
    },
    chat = {
        enabled = true,
        window = {
            width = 80,
            height = 20,
            border = 'rounded',
        },
    },
    lsp = {
        enabled = false,  -- Use built-in LSP instead
    },
})
```

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

- API keys are **only** sent to official provider endpoints
- No telemetry or data collection
- Local Ollama option for fully offline usage

## License

MIT
