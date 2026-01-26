# TUI Setup Guide

## Requirements

### 1. Terminal (TTY) Required

The TUI **requires a real terminal device (TTY)** to run. It will not work in:
- Docker containers without `-t` flag
- CI/CD pipelines
- Piped or redirected environments
- Non-interactive shells

**Check if you have a TTY:**
```bash
tty
# Should output: /dev/pts/0 (or similar)
# Should NOT output: not a tty
```

**Solutions:**

**Docker:**
```bash
docker run -it -e OPENAI_API_KEY="$OPENAI_API_KEY" your-image ./tark tui
#          ^^ IMPORTANT: -it flags enable TTY
```

**SSH:**
```bash
ssh -t user@host  # -t forces TTY allocation
```

**Alternative:** Use `tark chat` for non-TTY environments:
```bash
./tark chat  # Works without TTY
```

---

## 2. LLM Provider Configuration

### Provider Filtering (New Feature)

By default, the TUI shows **tark_sim** (built-in test provider), **OpenAI**, **Gemini**, and **Ollama** (local) providers.

**To customize which providers are shown:**

Create or edit `~/.config/tark/config.toml`:

```toml
[llm]
# List only the providers you want to use
enabled_providers = ["openai", "google", "anthropic", "ollama"]

# Or leave empty to show ALL providers from models.dev
# enabled_providers = []
```

**Available providers:**
- `tark_sim` - Built-in test provider (no API key required)
- `openai` - OpenAI GPT models
- `google` - Google Gemini
- `anthropic` - Anthropic Claude
- `openrouter` - OpenRouter (200+ models)
- `ollama` - Local Ollama (loads models dynamically from your local instance)
- Plus any installed plugin providers

### API Key Setup

#### OpenAI
```bash
export OPENAI_API_KEY="sk-your-key-here"
```

#### Gemini
```bash
export GEMINI_API_KEY="your-key-here"
# OR
export GOOGLE_API_KEY="your-key-here"
```

#### Anthropic Claude
```bash
export ANTHROPIC_API_KEY="your-key-here"
```

**Make it permanent:**
Add to `~/.bashrc` or `~/.zshrc`:
```bash
echo 'export OPENAI_API_KEY="sk-your-key"' >> ~/.bashrc
source ~/.bashrc
```

---

## 3. Minimum Terminal Size

For the best experience, ensure your terminal is at least:
- **Width**: 100 columns (sidebar requires > 80)
- **Height**: 30 rows

**Check terminal size:**
```bash
tput cols  # Should be > 80 for sidebar
tput lines # Should be > 20
```

**Resize:**
Most terminals can be resized by dragging window edges or:
```bash
printf '\e[8;30;100t'  # Resize to 30 rows x 100 cols
```

---

## 4. Debug Logging

Enable debug logging to troubleshoot issues:

```bash
# Run with debug flag
./tark tui --debug

# Check debug log (created in working directory)
tail -f tark-debug.log
```

**Note:** Debug log is only created if the TUI successfully starts. If initialization fails before logging setup, check stderr:

```bash
RUST_LOG=debug ./tark tui 2>&1 | tee startup-error.log
```

---

## 5. Quick Start

### Step 1: Verify Requirements
```bash
# Check TTY
tty

# Check API key
echo $OPENAI_API_KEY | head -c 20

# Check terminal size
tput cols
tput lines
```

### Step 2: Configure Providers (Optional)
```bash
mkdir -p ~/.config/tark
cp examples/tark-config/config.toml ~/.config/tark/config.toml

# Edit to customize enabled_providers
vim ~/.config/tark/config.toml
```

Optional TUI settings (polling + theme):
```toml
[tui]
theme = "catppuccin_mocha"
plugin_widget_poll_ms = 2000
session_usage_poll_ms = 1000
```

### Step 3: Run TUI
```bash
cd /path/to/your/project
tark tui
```

Or with specific provider:
```bash
tark tui --provider openai --model gpt-4o
```

---

## Troubleshooting

### "No such device or address (os error 6)"
**Cause:** No TTY available.

**Fix:** Run in a proper terminal or use `tark chat` instead.

### "LLM not connected. Please configure your API key"
**Causes:**
1. API key not set in environment
2. Wrong provider name
3. Storage/session initialization failed

**Debug:**
```bash
# 1. Verify key is set
printenv | grep API_KEY

# 2. Try with explicit provider
./tark tui --provider openai

# 3. Check if session storage is corrupted
rm -rf .tark/sessions/*
./tark tui
```

### "Failed to initialize AgentBridge"
**Check:**
1. `.tark/` directory exists and is writable
2. Provider name matches config
3. Debug logs for detailed error

```bash
# Enable full logging
RUST_LOG=trace ./tark tui --debug 2>&1 | grep -A10 "AgentBridge"
```

### Sidebar Not Visible
**Cause:** Terminal too narrow (<= 80 columns).

**Fix:**
```bash
# Resize terminal
tput cols  # Check current width
# Resize to at least 100 columns
```

### Provider/Model Picker Empty
**Cause:** `enabled_providers` in config filters out all providers, or models.dev failed.

**Fix:**
```bash
# Check config
cat ~/.config/tark/config.toml

# Set to show all providers
# In config.toml:
# enabled_providers = []

# Or remove the config to use defaults
rm ~/.config/tark/config.toml
```

---

## Testing Without API Keys

Use Ollama for local testing (no API key needed):

```bash
# 1. Install Ollama
curl -fsSL https://ollama.ai/install.sh | sh

# 2. Start Ollama
ollama serve &

# 3. Pull a model
ollama pull codellama

# 4. Run TUI with Ollama
./tark tui --provider ollama --model codellama
```

---

## Example Configurations

### Default (Test/Demo Mode)
```toml
[llm]
default_provider = "tark_sim"  # Built-in test provider, no API key required
enabled_providers = ["tark_sim", "openai", "google", "ollama"]

[llm.tark_sim]
model = "tark_llm"
max_tokens = 8192

[llm.ollama]
base_url = "http://localhost:11434"
model = "codellama"  # Models load dynamically from local Ollama
```

### Minimal (OpenAI only)
```toml
[llm]
default_provider = "openai"
enabled_providers = ["openai"]
```

### Multi-Provider (OpenAI + Gemini)
```toml
[llm]
default_provider = "openai"
enabled_providers = ["openai", "google"]

[llm.openai]
model = "gpt-4o"

[llm.gemini]
model = "gemini-2.0-flash-exp"
```

### All Providers
```toml
[llm]
default_provider = "openai"
enabled_providers = []  # Empty = show all
```

---

## New Features

### Theme System

Switch themes with `/theme` command for live preview:

```bash
/theme         # Opens theme picker
↑/↓            # Preview themes in real-time
Enter          # Apply selected theme
Escape         # Cancel and restore original
```

Available themes: Catppuccin Mocha, Nord, GitHub Dark, One Dark, Gruvbox Dark, Tokyo Night

See [THEMES.md](THEMES.md) for custom themes.

### Task Queue Management

When you send multiple messages while the agent is working, they're queued.

**Managing queued tasks (Sidebar → Tasks panel):**
- `e` - Edit a queued task
- `D` - Delete a queued task
- `J/K` - Reorder tasks

### Session Management

```bash
/sessions        # List all sessions
/session <id>    # Switch to session
/new             # Create new session
/export [path]   # Export session to JSON
/import <path>   # Import session from JSON
```

### Trust Levels (Build Mode)

Control how risky operations are approved with `Ctrl+Shift+B` or `/trust`:

| Level | Behavior |
|-------|----------|
| Manual | Approve everything |
| Balanced | Auto-approve reads, prompt for writes (default) |
| Careful | Auto-approve most, prompt only for dangerous |

---

## Keyboard Quick Reference

| Key | Action |
|-----|--------|
| `Ctrl+C` | Cancel LLM / Quit |
| `Ctrl+B` | Toggle sidebar |
| `Ctrl+T` | Toggle model-level thinking |
| `Shift+Tab` | Cycle agent mode |
| `Ctrl+M` | Cycle build mode |
| `Ctrl+?` | Help modal |
| `Tab` | Cycle focus |
| `j/k` | Vim navigation |
| `Esc` | Close modal |

See [README.md](../README.md) for full keyboard reference.

---

## Getting Help

1. Check logs: `cat tark-debug.log` (if --debug was used)
2. Run diagnostics: `./test_tui_init.sh`
3. Test with chat mode: `./tark chat` (works without TTY)
4. Check authentication: `./tark auth status`
5. See [BFF_ARCHITECTURE.md](BFF_ARCHITECTURE.md) for architecture details
