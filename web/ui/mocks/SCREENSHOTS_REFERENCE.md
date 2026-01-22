# Screenshot Reference Guide

This document provides a comprehensive guide to all screenshots in `./screenshots/`, organized by category. Use these as your visual acceptance criteria when implementing the Ratatui TUI.

**Total Screenshots:** 51 images

---

## 📊 Quick Navigation

- [Main Layout](#main-layout)
- [Status Bar & Dropdowns](#status-bar--dropdowns)
- [System Modals](#system-modals)
- [Message Types](#message-types)
- [Questions & Interactive Elements](#questions--interactive-elements)
- [Sidebar Panel](#sidebar-panel)
- [Theme Variations](#theme-variations)
- [Component States](#component-states)

---

## 🖼️ Main Layout

These screenshots show the overall application structure and layout.

### Primary Layout Screenshots

| Screenshot | Description | Key Features |
|------------|-------------|--------------|
| `01-main-layout.png` | Current viewport view | Terminal header, message area, input box, status bar, sidebar |
| `02-full-page.png` | Full scrollable page | Complete message history, all component types visible |
| `01-main-layout-full.png` | Alternative full view | Similar to 02 but different scroll position |
| `02-main-viewport.png` | Another viewport angle | Shows active question with selections |

**What to Implement:**
- Header: "🖥 INNODRUPE TERMINAL" with path "~/innodrupe/core/engine"
- Three-column layout: Main terminal (center), Sidebar (right)
- Status bar at bottom with agent mode, model selector, indicators
- Multi-line input area with textarea wrapping
- Fixed header and status bar, scrollable message area

**Ratatui Mapping:**
```rust
Layout::vertical([
    Constraint::Length(3),      // Header
    Constraint::Min(0),          // Message area (scrollable)
    Constraint::Length(5),       // Input area
    Constraint::Length(2),       // Status bar
])
```

---

## ⚡ Status Bar & Dropdowns

Screenshots showing the status bar and its interactive elements.

### Status Bar Components

| Screenshot | Description | Interactive Elements |
|------------|-------------|---------------------|
| `compact-status-bar.png` | Compact status bar design | Agent mode, Build mode, Model selector, Thinking toggle, Queue indicator |
| `04-agent-mode-dropdown.png` | Agent mode selector open | Build, Plan, Ask options with descriptions |
| `05-build-mode-dropdown.png` | Build mode selector open | Careful, Manual, Balanced options with shortcuts (⌘1, ⌘2, ⌘3) |

**Status Bar Layout (left to right):**
1. **Agent Section**:
   - Label: "agent" (small, gray)
   - Mode button: "Build" with hammer icon and chevron
   - Build mode: "Balanced" with icon and chevron

2. **Indicators**:
   - Brain icon (🧠) - Thinking mode toggle (amber when active)
   - Queue icon (📋) with count badge (e.g., "7")

3. **Working Indicator** (center):
   - Blinking green dot with "Working..." text

4. **Right Section**:
   - Model display: "Claude 3.5 Sonnet" / "Anthropic"
   - Help button: "?" icon (monochrome)

**Dropdown Styling:**
- Dark background with border
- Hover highlight on options
- Icons next to each option
- Keyboard shortcuts displayed (⌘1, ⌘2, etc.)
- Descriptions below option names

---

## 🎭 System Modals

Screenshots of all modal/popup overlays.

### Modal Screenshots

| Screenshot | Modal Type | Trigger | Key Features |
|------------|-----------|---------|--------------|
| `06-provider-picker-modal.png` | Provider Picker | Click model selector or `/model` | Lists AI providers (Anthropic, OpenAI, Google, etc.) with icons |
| `07-model-picker-modal.png` | Model Picker | After selecting provider | Shows models for selected provider with descriptions |
| `08-help-modal.png` | Help & Shortcuts | Click "?" button or `/help` | Complete keyboard shortcuts and commands reference |
| `09-file-picker-modal.png` | File Picker | Type "@" or click "+" button | File browser with @mention completion |
| `10-theme-picker-modal.png` | Theme Picker | Type `/theme` | Theme presets with color swatches and descriptions |
| `theme-picker-modal.png` | Theme picker (alt view) | Same as above | Shows Catppuccin Mocha as current theme |
| `picker-components-full.png` | All pickers overview | N/A | Shows multiple picker states |

**Modal Common Features:**
- Centered overlay with rounded borders
- Dark background with blur effect (in TUI: solid background)
- Title at top with close button (✕)
- Search/filter input below title
- Scrollable list of options
- Footer with current selection and instructions
- ESC to close, Enter to select

**Modal Sizes:**
- Small: 40 chars wide × 15 lines (Provider picker)
- Medium: 50 chars wide × 20 lines (Model picker, Theme picker)
- Large: 60 chars wide × 25 lines (File picker, Help modal)

---

## 💬 Message Types

Screenshots showing different message types in the conversation.

### Message Type Screenshots

| Screenshot | Message Types Shown | Key Visual Features |
|------------|-------------------|-------------------|
| `03-message-types-top.png` | System, User (You), Thinking, Agent, Tool | Each type has distinct icon, color, and styling |
| `themed-message-bubbles.png` | User and Agent messages | Shows themed bubble backgrounds and borders |
| `catppuccin-themed-bubbles.png` | User and Agent with Catppuccin theme | Distinct blue (user) and green (agent) tints |
| `user-agent-bubbles.png` | User and Agent message comparison | Side-by-side of message styling |
| `messages-with-themes.png` | Multiple message types with theming | Complete theme application |

**Message Type Breakdown:**

1. **System Messages** (● icon, teal color)
   - Example: "Innodrupe Core v2.1.0 initialized"
   - No bubble, just icon and text

2. **User Messages** (👤 icon, blue tint)
   - Icon: User icon in blue rounded square
   - Label: "You" (or auto-detected username)
   - Bubble: Light blue background with blue border
   - Example: "Help me refactor the authentication module..."

3. **Agent Messages** (🤖 icon, green tint)
   - Icon: Bot icon in green rounded square
   - Label: "Innodrupe" (configurable)
   - Bubble: Light green background with green border
   - Copy button on hover
   - Example: "I'll analyze your authentication module..."

4. **Thinking Messages** (🧠 icon, gray)
   - Collapsible section with "Thinking..." header
   - Gray background box with thinking content
   - Copy button for thinking content
   - Example: "The user wants to refactor authentication..."

5. **Tool Messages** (🔧 icon, blue-gray)
   - Collapsible with "Tool" label and chevron
   - Command/output display
   - Copy button
   - Example: "SCAN: src/auth/ - Analyzing authentication patterns"

6. **Command Messages** (✓ icon, green)
   - System success/completion messages
   - Example: "Migration task completed successfully..."

7. **Question Messages** (❓ icon, cyan)
   - Special question icon in cyan rounded square
   - Question text as header
   - Interactive options below
   - Varies by question type (see next section)

---

## ❓ Questions & Interactive Elements

Screenshots showing interactive question components.

### Question Type Screenshots

| Screenshot | Question Type | Visual Features |
|------------|--------------|-----------------|
| `12-question-single-selected.png` | Single choice (radio) | PostgreSQL option selected with checkmark icon |
| `questions-answered-with-selections.png` | Answered questions | Shows "Answered: pnpm" in badge format |
| `question-answered-state.png` | Post-answer state | Question grayed out with answer displayed |

**Question Types:**

### 1. Single Choice (Radio Buttons)
```
Question: "Which database would you like to use for this project?"

Options:
○ a) PostgreSQL - Recommended for complex queries
○ b) MySQL - Good for web applications
○ c) SQLite - Lightweight, file-based
○ d) MongoDB - NoSQL document store

[Confirm Selection] button at bottom
```

**Visual Details:**
- Unselected: Gray circle (○)
- Selected: Checkmark icon (✓) with highlight background
- Options have letter prefix (a, b, c, d)
- Description text below each option
- Confirm button enabled when selection made

### 2. Multiple Choice (Checkboxes)
```
Question: "Select all the features you want to include:"

Options:
☐ a) User Authentication (JWT + Sessions)
☐ b) REST API endpoints
☐ c) GraphQL API
☐ d) WebSocket real-time updates
☐ e) Redis caching layer
☐ f) Structured logging

"X selected" counter + [Confirm Selection] button
```

**Visual Details:**
- Unchecked: Empty square (☐)
- Checked: Checkmark in square (☑)
- Counter shows "X selected"
- Can select multiple options

### 3. Free Text Input
```
Question: "What should the main API endpoint prefix be?"

[Text input box] (placeholder: "e.g., /api/v1")
[Submit] button
```

**Visual Details:**
- Single-line text input
- Placeholder text in gray
- Submit button enabled when input not empty

### 4. Answered State
After submission, questions show:
- Badge with checkmark icon and answer text
- Example: "✓ Answered: pnpm"
- Original question text remains visible
- Options/input replaced with answer summary

---

## 📋 Sidebar Panel

Screenshots of the collapsible right sidebar.

### Sidebar Screenshots

| Screenshot | Description | Sections Visible |
|------------|-------------|------------------|
| `11-sidebar-panel.png` | Sidebar fully expanded | Session, Context, Tasks, Git Changes |
| `panel-theme-applied.png` | Sidebar with theme applied | Shows theme integration with sidebar |

**Sidebar Structure (top to bottom):**

1. **Header**
   - Title: "Panel"
   - Collapse all button
   - Theme selector dropdown: "Theme: Catppuccin Mocha"
   - Close sidebar button (⟩)

2. **Session Section** (▼ expanded)
   - Branch icons with names:
     - `main`
     - `feature/sidebar-update`
     - `gemini-1.5-pro-preview`
     - `gemini-oauth`
   - Cost indicator: "$0.015 (3 models)"

3. **Context Section** (▼ expanded)
   - Token usage: "1,833 / 1,000,000 tokens"
   - "LOADED FILES (8)" subsection
   - File list with icons:
     - 📄 src/components/Sidebar.tsx
     - 📁 src/styles/
     - 📄 package.json
     - (etc.)

4. **Tasks Section** (▼ expanded)
   - Count badge: "8"
   - Active task (●): "Understanding the codebase architecture" with "Active" label
   - "QUEUED" subheading
   - Queued tasks (○):
     - "Which is the most complex component?"
     - "Refactor the gaming class structure"
     - "Optimize database queries"
     - "Fix authentication bug"
     - "Update documentation"
     - "Review pull requests"
     - "Implement dark mode toggle"

5. **Git Changes Section** (▼ expanded)
   - Count badge: "12"
   - Summary: "7 Mod | 3 New | 2 Del"
   - File list with indicators:
     - 📄 src/components/Sidebar.tsx `+45 -12`
     - 📄 src/utils/helpers.ts `NEW`
     - 📄 public/legacy-logo.svg `DEL`
     - 📄 src/styles/globals.css `+10 -5`
     - 📄 README.md `+2 -1`

**Collapsed State:**
- Narrow vertical bar on right edge
- Small ⟨ button to expand

---

## 🎨 Theme Variations

Screenshots showing different theme applications.

### Theme Screenshots

| Screenshot | Theme | Description |
|------------|-------|-------------|
| `catppuccin-theme-applied.png` | Catppuccin Mocha | Main dark theme with pastel accents |
| `catppuccin-theme-fixed.png` | Catppuccin (fixed view) | Theme with corrections applied |
| `catppuccin-full-theme.png` | Catppuccin (complete) | Full application with theme |
| `catppuccin-top.png` | Catppuccin (top portion) | Header and first messages |
| `dracula-theme-applied.png` | Dracula | Purple-tinted dark theme |

**Catppuccin Mocha Color Scheme:**
- Background: `#1e1e2e` (dark navy)
- Surface: `#313244` (slightly lighter)
- Text: `#cdd6f4` (light blue-white)
- Accent (Blue): `#89b4fa` (sapphire)
- Accent (Green): `#a6e3a1` (green)
- Accent (Peach): `#fab387` (orange)
- Accent (Sky): `#89dceb` (cyan for questions)

**User Message Colors (Catppuccin):**
- Background: `rgba(137, 180, 250, 0.1)` (sapphire tint)
- Border: `rgba(137, 180, 250, 0.2)`
- Icon color: `#89b4fa` (sapphire)

**Agent Message Colors (Catppuccin):**
- Background: `rgba(166, 227, 161, 0.1)` (green tint)
- Border: `rgba(166, 227, 161, 0.2)`
- Icon color: `#a6e3a1` (green)

---

## 🔄 Component States

Screenshots showing various component states and interactions.

### State Screenshots

| Screenshot | Component | State Shown |
|------------|-----------|-------------|
| `agent-working-indicator.png` | Working indicator | Blinking green dot with "Working..." text |
| `help-button-monochrome.png` | Help button | Monochrome "?" icon styled for theme |
| `help-modal-theme-check.png` | Help modal | Modal with theme colors applied |
| `model-selection-working.png` | Model selector | Active model display with provider |
| `queue-indicator.png` | Queue indicator | Task queue icon with count badge |

### Additional State Images

These screenshots capture various intermediate states and specific details:

- `innodrupe-terminal-ui-final.png` - Final UI mockup
- `page-2026-01-16-*.png` (multiple) - Timestamped progress snapshots during development

---

## 🎯 Visual Acceptance Criteria

Use this checklist when comparing your Ratatui implementation to screenshots:

### Layout Match
- [ ] Header height and content matches
- [ ] Message area scrolls correctly
- [ ] Input area has proper height with wrapping
- [ ] Status bar has all elements in correct positions
- [ ] Sidebar width and sections match

### Color Match
- [ ] Background colors match exactly (use RGB values from theme.css)
- [ ] Text colors match for each message type
- [ ] Border colors match
- [ ] Icon colors match
- [ ] Hover/selection colors match

### Typography Match
- [ ] Font sizes are proportional (TUI: use bold/dim for emphasis)
- [ ] Text alignment matches (left/center/right)
- [ ] Line spacing is similar
- [ ] Text wrapping behaves the same

### Interactive Elements
- [ ] Buttons look clickable (TUI: highlighted/bracketed)
- [ ] Dropdowns show chevron indicators
- [ ] Selected items have visual feedback
- [ ] Disabled states are visually distinct
- [ ] Focus indicators are clear

### Spacing Match
- [ ] Padding around elements matches
- [ ] Margins between components match
- [ ] List item spacing is consistent
- [ ] Modal borders and padding match

---

## 📐 Measurement Reference

Key measurements extracted from screenshots:

| Element | Height (lines) | Width | Notes |
|---------|---------------|-------|-------|
| Header | 3 | Full | Icon + title + path |
| Status Bar | 2 | Full | Multiple sections |
| Input Area | 5 (min) | Full | Expands with text |
| Message Icon | 1 | 2-3 chars | Square icon container |
| Modal Title | 2 | Modal width | With close button |
| Modal Option | 2-3 | Modal width | With description |
| Sidebar Width | N/A | ~35 chars | When expanded |
| Sidebar Collapsed | N/A | 2 chars | Just border + icon |

---

## 🔍 Screenshot Comparison Tips

When implementing each component:

1. **Open the relevant screenshot** in an image viewer
2. **Split your terminal** to show both the screenshot and your TUI
3. **Compare side-by-side**: Layout, colors, spacing, text
4. **Zoom in on details**: Icons, borders, button styles
5. **Test interactions**: Does your TUI behave like the React app?
6. **Check edge cases**: Long text, empty states, errors
7. **Verify themes**: Test with multiple theme presets

---

## 📝 Notes on TUI Limitations

Some visual effects from the web UI cannot be exactly replicated in a TUI:

- **Rounded corners**: Use box-drawing characters (`╭╮╰╯`) instead
- **Shadows**: Omit or use double-line borders for emphasis
- **Smooth animations**: Use character frames (e.g., spinner: `⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏`)
- **Blur effects**: Use solid backgrounds for modals
- **Hover tooltips**: Show info in status bar or inline
- **Mouse hover**: Use keyboard selection with highlight

Despite these limitations, a well-implemented TUI can achieve 95%+ visual similarity to the web UI.

---

## ✅ Verification Checklist

Before marking a component complete:

- [ ] Opened and studied relevant screenshot(s)
- [ ] Compared my implementation side-by-side
- [ ] Colors match theme specification
- [ ] Layout matches proportions
- [ ] Interactive elements work correctly
- [ ] Edge cases handled (empty, overflow, etc.)
- [ ] Looks good in multiple terminal sizes
- [ ] Tested with multiple theme presets

---

## 🔗 Cross-References

- See `KICKSTART.md` for implementation workflow
- See `RATATUI_MAPPING.md` for widget mappings
- See `AGENT_INSTRUCTIONS.md` for best practices
- See React source files for exact behavior specifications

---

**Remember:** Screenshots are your source of truth. If it doesn't look like the screenshot, keep iterating until it does. 📸✨
