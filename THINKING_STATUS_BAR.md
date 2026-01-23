# Thinking Tool Status Bar Indicator

## Summary

Added a thinking tool indicator (💭 thought bubble) to the status bar, displayed next to the existing brain icon (🧠).

## Visual Layout

The status bar now displays both thinking indicators:

```
agent • Build ▼  🟢 Balanced ▼  [🧠] [💭]  ≡ 7    ● Working...    • Model Provider  [?]
```

### Icons

1. **🧠 Brain** - Model-level extended thinking (`/think` command, Ctrl+T)
   - **Yellow border** `[🧠]` - Extended thinking enabled (off/low/medium/high)
   - **Gray border** `[🧠]` - Extended thinking disabled

2. **💭 Thought Bubble** - Think tool for structured reasoning (`/thinking` command)
   - **Cyan border** `[💭]` - Think tool enabled (agent records reasoning steps)
   - **Gray border** `[💭]` - Think tool disabled

## Files Modified

1. **`src/tui_new/widgets/status_bar.rs`**
   - Added `thinking_tool_enabled: bool` field
   - Added `.thinking_tool()` builder method
   - Updated render logic to display thought bubble indicator with cyan/muted border

2. **`src/tui_new/renderer.rs`**
   - Fetch `thinking_tool_enabled` state from `SharedState`
   - Pass to `StatusBar` via `.thinking_tool()` method

3. **`src/tui_new/widgets/command_autocomplete.rs`** (from previous change)
   - Added `/thinking` to command autocomplete

## Commands

- **`/think`** or **Ctrl+T** - Toggle model-level extended thinking (🧠)
- **`/thinking`** - Toggle think tool for structured reasoning (💭)

## Color Coding

- **Yellow** - Model-level thinking indicator (matches agent mode colors)
- **Cyan** - Think tool indicator (matches theme accent colors)
- **Muted/Gray** - Disabled state for both indicators

## Testing

✅ Compiles successfully (`cargo check`)
✅ No clippy warnings (`cargo clippy`)
✅ Code formatted (`cargo fmt`)
