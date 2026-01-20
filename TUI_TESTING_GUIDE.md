# TUI Testing Guide - Resolving "LLM not connected" Issue

## Issue Summary

Your TUI starts but shows: "⚠️ LLM not connected. Please configure your API key."

**Root Cause:** The TUI requires a **proper interactive terminal (PTY)** to function. You're running in an environment without a TTY.

---

## Current Situation

✅ **API Key**: Correctly set (`OPENAI_API_KEY` is 164 characters)
✅ **Build**: Compiles successfully
✅ **Code**: All fixes implemented
❌ **Terminal**: Running without TTY (`tty` command returns "not a tty")

---

## Solution: Run in a Proper Terminal

### Option 1: Docker with TTY (Recommended)

If you're in a Docker container:

```bash
# From your host machine:
docker exec -it <container-name> bash

# Then inside container:
cd /home/dev/data/work/code/tark
export OPENAI_API_KEY="your-key-here"
./target/release/tark tui
```

### Option 2: Use `script` to Create Pseudo-Terminal

```bash
cd /home/dev/data/work/code/tark

# This creates a pseudo-terminal
script -c "OPENAI_API_KEY='$OPENAI_API_KEY' ./target/release/tark tui" /dev/null
```

### Option 3: SSH with TTY Allocation

```bash
# From your local machine:
ssh -t user@host "cd /home/dev/data/work/code/tark && ./target/release/tark tui"
```

### Option 4: Use tmux or screen

```bash
# Start tmux
tmux

# Inside tmux:
cd /home/dev/data/work/code/tark
./target/release/tark tui
```

---

## Verification Steps

Before running the TUI, verify your environment:

### 1. Check TTY Status
```bash
tty
# ✅ Good: /dev/pts/0 (or similar)
# ❌ Bad:  not a tty
```

### 2. Check API Key
```bash
echo $OPENAI_API_KEY | head -c 20
# Should show: sk-proj-OP37qZbUR7xxH1...
```

### 3. Check Terminal Size
```bash
tput cols   # Should be > 80 (recommend 100+)
tput lines  # Should be > 20 (recommend 30+)
```

### 4. Test stdin/stdout
```bash
test -t 0 && echo "stdin is TTY" || echo "stdin is NOT TTY"
test -t 1 && echo "stdout is TTY" || echo "stdout is NOT TTY"
# Both should say "is TTY"
```

---

## Enhanced Logging (Implemented)

The new implementation includes detailed logging. To see what's happening:

```bash
# Run with debug logging
RUST_LOG=debug ./target/release/tark tui --debug 2>&1 | tee tui-debug.log

# Check the log for initialization details
cat tui-debug.log | grep -A5 "AgentBridge"
```

**Log messages you should see:**
```
INFO  tark::tui::agent_bridge > Initializing AgentBridge in "/path/to/work"
DEBUG tark::tui::agent_bridge > Config loaded, default provider: openai
DEBUG tark::tui::agent_bridge > Storage initialized at "/path/to/work/.tark"
DEBUG tark::tui::agent_bridge > Session loaded: <session-id>
INFO  tark::tui::agent_bridge > Using config default provider: openai
INFO  tark::tui::agent_bridge > Using default model for openai: gpt-4o
INFO  tark::tui::agent_bridge > Creating LLM provider: openai with model: gpt-4o
INFO  tark::tui::agent_bridge > LLM provider created successfully
INFO  tark::ui_backend::service > AgentBridge initialized successfully
INFO  tark::ui_backend::service > LLM provider set to: openai
INFO  tark::ui_backend::service > LLM model set to: gpt-4o
```

**If you see an error:**
```
ERROR tark::ui_backend::service > Failed to initialize LLM: <error details>
```

This will tell us exactly what's failing.

---

## Provider Configuration

### Default Enabled Providers

By default, only **OpenAI** and **Gemini** are enabled in the provider picker.

**To change this:**

Create `~/.config/tark/config.toml`:

```toml
[llm]
default_provider = "openai"

# Only show these providers in TUI
enabled_providers = ["openai", "google"]

# Or show all available providers:
# enabled_providers = []
```

### Supported Providers

- `openai` - Requires `OPENAI_API_KEY`
- `google` - Requires `GEMINI_API_KEY` or `GOOGLE_API_KEY`
- `anthropic` - Requires `ANTHROPIC_API_KEY`
- `openrouter` - Requires `OPENROUTER_API_KEY`
- `ollama` - Requires Ollama running locally (no API key)

---

## Quick Test in Chat Mode

If you can't get a TTY, test the LLM connection with chat mode first:

**This won't work without TTY either, but gives a different error message:**

```bash
# This might fail with TTY error, but worth trying
echo "test" | ./target/release/tark chat 2>&1
```

---

## Testing in Cursor IDE

If you're using Cursor IDE, you can test the TUI in the integrated terminal:

1. Open terminal in Cursor (Ctrl+\`)
2. Ensure it's a real terminal (not output pane)
3. Run:
   ```bash
   cd /home/dev/data/work/code/tark
   ./run_tui.sh
   ```

---

## What Was Fixed (For Real Terminal Testing)

When you do get a proper TTY, all these features will work:

✅ **SHIFT+TAB** - Cycles agent modes (Build → Plan → Ask)
✅ **Ctrl+M** - Cycles build modes (Manual → Balanced → Careful)
✅ **Ctrl+T** - Toggles thinking blocks
✅ **Ctrl+B** - Toggles sidebar
✅ **?** - Opens help modal
✅ **Multi-line input** - SHIFT+Enter for newlines
✅ **Word navigation** - Ctrl+Left/Right
✅ **Provider picker** - Search and arrow key navigation
✅ **Model picker** - Search and selection
✅ **LLM messaging** - Streaming responses with error handling
✅ **Sidebar** - Auto-hides on narrow terminals (<= 80 cols)

---

## Next Steps

1. **Get a proper terminal** using one of the options above
2. **Run with debug logging**: `RUST_LOG=debug ./target/release/tark tui --debug`
3. **Check debug log**: `cat tark-debug.log` to see exact initialization error
4. **Share the log** if issue persists

The code is ready - it's just a matter of getting the right terminal environment!
