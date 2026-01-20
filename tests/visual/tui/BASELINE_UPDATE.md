# Visual Baseline Update Guide

## Overview

This document explains how to update visual test baselines to match the design mockups located in `web/ui/mocks/screenshots/`.

## Reference Mockups

The mockups are located at: `/home/dev/data/work/code/tark/web/ui/mocks/screenshots/`

### Key Mockups

1. **01-main-layout.png** - Main terminal layout with all components
2. **02-full-page.png** - Full page view showing sidebar and main area  
3. **03-message-types-top.png** - Different message types (User, Agent, System)
4. **06-provider-picker-modal.png** - Provider selection modal
5. **07-model-picker-modal.png** - Model selection modal
6. **08-help-modal.png** - Help keyboard shortcuts modal
7. **09-file-picker-modal.png** - File attachment picker
8. **10-theme-picker-modal.png** - Theme selection modal
9. **11-sidebar-panel.png** - Sidebar panel with Session, Context, Tasks, Git sections
10. **compact-status-bar.png** - Status bar design
11. **agent-working-indicator.png** - LLM processing indicator

## Baseline Directories

Current baselines are stored in:
- `/home/dev/data/work/code/tark/tests/visual/tui/snapshots/`

Organized by feature:
- `01_terminal_layout/` - Main layout
- `02_status_bar/` - Status bar variations
- `03_message_display/` - Message rendering
- `04_input_area/` - Input box
- `05_modals_provider_picker/` - Provider picker
- `06_modals_model_picker/` - Model picker
- `07_modals_file_picker/` - File picker
- `08_modals_theme_picker/` - Theme picker
- `09_modals_help/` - Help modal
- `13_sidebar/` - Sidebar panel
- `14_theming/` - Theme applications
- `16_llm_responses/` - LLM response rendering

## Recent Implementation Changes

### 1. Keybindings Fixed
- **SHIFT+TAB** now handles both `BackTab` and `Tab+SHIFT` for cross-terminal compatibility
- Cycles through: Build → Plan → Ask → Build
- File: `src/tui_new/renderer.rs:77-80`

### 2. Input Widget Enhanced
- Multi-line support with proper scrolling
- Cursor navigation across wrapped lines
- SHIFT+Enter for newline insertion
- Word navigation with Ctrl+Left/Right
- File: `src/tui_new/widgets/input.rs`

### 3. Modal Redesigns
- **Provider Picker**: Enhanced with search bar, navigation hints, configuration status
- **Model Picker**: Enhanced with search, model count, current indicator
- Both modals now show keyboard shortcuts at top
- Files: `src/tui_new/widgets/modal.rs`

### 4. LLM Error Handling
- Better error messages with emoji indicators (⚠️, ❌)
- Helpful troubleshooting hints
- Initialization errors shown in message area
- File: `src/ui_backend/service.rs`

### 5. Build Mode & Thinking Mode
- Ctrl+M cycles build modes (Manual → Balanced → Careful)
- Ctrl+T toggles thinking blocks
- Both properly wired to AgentBridge
- Files: `src/ui_backend/service.rs`, `src/tui_new/renderer.rs`

## Visual Differences to Verify

### Status Bar
✅ Agent mode indicator with icon (🔨 Build, 📋 Plan, 💬 Ask)
✅ Build mode indicator (only in Build mode): 🟢 Manual/Balanced/Careful
✅ Thinking indicator: 🧠 (colored when enabled)
✅ Queue count: ≡ 7
✅ Processing indicator: ● Working...
✅ Model/Provider display: • Claude 3.5 Sonnet ANTHROPIC
✅ Help button: ⊙

### Modals
✅ Search bar at top with cursor indicator
✅ Navigation hints: ↑↓ Navigate | Enter Select | Esc Cancel
✅ Selected items highlighted with cyan background
✅ Provider/model configuration status indicators
✅ Description text shown for selected items

### Input Area
✅ Multi-line rendering with scroll
✅ Cursor visible on correct line
✅ Text wrapping enabled
✅ Title shows "(Shift+Enter for newline)"

### Sidebar
✅ Responsive: hides when terminal width ≤ 80 columns
✅ Collapsible panels with ▼/▶ indicators
✅ Session, Context, Tasks, Git Changes sections
✅ Theme selector at top
✅ File icons and status indicators

### Message Area
✅ User, Agent, System message styling
✅ Thinking blocks (collapsible)
✅ Timestamp on each message
✅ Error messages with emoji indicators

## How to Update Baselines

### 1. Run Visual Tests
```bash
# Run the TUI visual test suite
cd /home/dev/data/work/code/tark
cargo test --test visual_tui_tests

# Or run specific feature
cargo test --test visual_tui_tests -- terminal_layout
```

### 2. Generate New Screenshots
```bash
# Record new terminal sessions
./tests/visual/tui_e2e_runner.sh

# This will create .cast files in:
# tests/visual/tui/recordings/*/
```

### 3. Convert to PNG
```bash
# Convert asciinema casts to PNG
# (requires: npm install -g svg-term-cli)
for file in tests/visual/tui/recordings/*/*.cast; do
    svg-term --in "$file" --out "${file%.cast}.svg"
    # Convert SVG to PNG using your preferred tool
done
```

### 4. Compare with Mockups
Manually compare generated PNGs with mockups in `web/ui/mocks/screenshots/`

### 5. Update Baselines
```bash
# Copy new screenshots to baseline directory
cp new_screenshot.png tests/visual/tui/snapshots/01_terminal_layout/terminal_layout_final.png
```

## Testing Checklist

- [ ] Terminal layout matches `01-main-layout.png`
- [ ] Status bar matches `compact-status-bar.png`
- [ ] Provider picker matches `06-provider-picker-modal.png`
- [ ] Model picker matches `07-model-picker-modal.png`
- [ ] Help modal matches `08-help-modal.png`
- [ ] Theme picker matches `10-theme-picker-modal.png`
- [ ] Sidebar matches `11-sidebar-panel.png`
- [ ] Message types match `03-message-types-top.png`
- [ ] Working indicator matches `agent-working-indicator.png`

## New Feature Tests

Created new BDD feature files:
- `tests/visual/tui/features/17_agent_mode_switching.feature`
- `tests/visual/tui/features/18_build_mode_switching.feature`
- `tests/visual/tui/features/19_input_multiline.feature`
- `tests/visual/tui/features/20_llm_integration.feature`

These need step definitions and visual baselines added.

## Minimum Terminal Size

⚠️ **Important**: For proper testing, ensure terminal is at least:
- **Width**: 100 columns (sidebar requires > 80)
- **Height**: 30 rows (for full UI visibility)

## Next Steps

1. Run manual smoke tests on new TUI features
2. Generate new visual baselines using test runner
3. Create step definitions for new feature files
4. Document any remaining visual discrepancies
5. Update README with new keyboard shortcuts
