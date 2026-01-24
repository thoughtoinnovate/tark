# Testing the Thinking Feature

## Issue
User reports thoughts not showing when using the tool. Logs show `has_thinking:false`.

## Root Cause
The `/thinking` command must be run to enable the think tool before the agent will use it.

## How to Test

### 1. Start TUI with latest build
```bash
cd /home/dev/data/work/code/play
../tark/target/release/tark tui
```

### 2. Enable thinking tool
Type in the TUI:
```
/thinking
```

You should see a system message:
```
✓ Thinking tool enabled. Agent will use structured reasoning.
```

### 3. Verify status bar
Look at the status bar at the bottom. You should see:
- `[🧠]` - Brain icon (model-level thinking, controlled by `/think`)
- `[💭]` - Thought bubble icon **should now have CYAN border** (thinking tool enabled)

### 4. Ask agent a complex question
```
Can you analyze the authentication flow in this codebase?
```

### 5. Verify thoughts appear
The agent should call the `think()` tool multiple times. You should see:

```
╭─ think tool output ─────────────────────────────────────
│   
│   💭 Thought 1 of 3
│      First I need to understand the authentication module structure
│      Type: analysis | Confidence: 90%
│   
│   💭 Thought 2 of 3  
│      Let me examine the auth/ directory and identify key components
│      Type: plan | Confidence: 85%
│   
│   💭 Thinking...
╰─────────────────────────────────────────────────────────
```

## Debugging

### Check if thinking tool is enabled
The state is stored in `SharedState`. You can verify by:

1. Type `/thinking` to toggle (first time enables it)
2. Check status bar - thought bubble `[💭]` should be cyan (enabled) or gray (disabled)

### Check logs
```bash
cd /home/dev/data/work/code/play
tail -f .tark/debug/tark-debug.log | grep -i think
```

Look for:
- `"category":"tool_call","tool":"think"` - Tool being called
- `"thinking_tool_enabled":true` - State is set

### Verify tool is registered
```bash
cd /home/dev/data/work/code/play
../tark/target/release/tark tui
# Type: /tools
```

You should see `think` in the list of available tools.

## Expected Behavior

### When `/thinking` is OFF (default)
- Status bar: `[💭]` with gray border
- Agent does NOT use think tool
- Logs show: `"thinking_tool_enabled":false`

### When `/thinking` is ON
- Status bar: `[💭]` with cyan border  
- Agent CAN use think tool
- System prompt includes think tool instructions
- Logs show: `"thinking_tool_enabled":true`
- Agent will call `think()` for complex reasoning

## Commands

- `/thinking` - Toggle thinking tool on/off
- `/think` - Toggle model-level extended thinking (different feature)
- Ctrl+T - Same as `/think`
- `/tools` - View all available tools
- `/help` - View all commands

## Note

The thinking tool is **opt-in**. The agent will only use it when:
1. The `/thinking` command has been run to enable it
2. The task is complex enough to warrant structured reasoning
3. The system prompt instructs the agent to use it (automatically added when enabled)
