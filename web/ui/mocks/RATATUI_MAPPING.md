# Ratatui TUI Mapping Guide

## Overview

This document provides a comprehensive mapping of the Terminal UI (React/TypeScript) to a Rust Ratatui TUI implementation. It serves as a reference guide for AI agents to recreate the interface using Ratatui.

> **Note**: Both **agent name** (default: "Tark") and **user name** (auto-detected from `$USER`/`$USERNAME`) are **fully configurable**. See the [Application Configuration](#application-configuration) section for details.

---

## Table of Contents

1. [Application Configuration](#application-configuration)
2. [Screenshot References](#screenshot-references)
3. [Layout Architecture](#layout-architecture)
4. [Widget Mapping Table](#widget-mapping-table)
5. [Color Palette](#color-palette)
6. [State Management](#state-management)
7. [Event Handling](#event-handling)
8. [Animation Alternatives](#animation-alternatives)
9. [Code Examples](#code-examples)
10. [Function Verification Templates](#function-verification-templates)
11. [Required Crates](#required-crates)
12. [Implementation Checklist](#implementation-checklist)

---

## Application Configuration

The agent name, user name, and branding are **fully configurable**. This allows you to customize the TUI for different projects or identities.

### Configuration File: `src/app/config/appConfig.ts`

```typescript
export interface AppConfig {
  agentName: string;       // Full name in header (e.g., "Tark Terminal")
  agentNameShort: string;  // Short name in messages (e.g., "Tark")
  version: string;         // Version string (e.g., "2.1.0")
  defaultPath: string;     // Working directory in header
  headerIcon: string;      // Icon next to agent name
  agentIcon: string;       // Icon next to agent messages
  userName: string;        // User's display name (auto-detected in Ratatui)
  userIcon: string;        // Icon next to user messages
}

// Default configuration
export const defaultAppConfig: AppConfig = {
  agentName: "Tark Terminal",
  agentNameShort: "Tark",
  version: "2.1.0",
  defaultPath: "~/tark/workspace",
  headerIcon: "🖥",
  agentIcon: "🤖",
  userName: "You",         // Auto-detected from $USER/$USERNAME in Ratatui
  userIcon: "👤",
};
```

### Rust Equivalent

```rust
pub struct AppConfig {
    pub agent_name: String,
    pub agent_name_short: String,
    pub version: String,
    pub default_path: String,
    pub header_icon: String,
    pub agent_icon: String,
    pub user_name: String,
    pub user_icon: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        // Auto-detect username from environment variables
        let user_name = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "You".to_string());
        
        Self {
            agent_name: "Tark Terminal".to_string(),
            agent_name_short: "Tark".to_string(),
            version: "2.1.0".to_string(),
            default_path: "~/tark/workspace".to_string(),
            header_icon: "🖥".to_string(),
            agent_icon: "🤖".to_string(),
            user_name,  // Auto-detected!
            user_icon: "👤".to_string(),
        }
    }
}
```

### Where Configuration is Used

| Location | Field | Example |
|----------|-------|---------|
| Terminal Header (title) | `agentName` | "🖥 TARK TERMINAL" |
| Terminal Header (path) | `defaultPath` | "~/tark/workspace" |
| Agent Message Label | `agentNameShort` | "Tark" |
| User Message Label | `userName` | Auto-detected (e.g., "john", "user") |
| System Init Message | `agentNameShort` + `version` | "Tark Core v2.1.0 initialized" |
| Message Icon | `agentIcon` | 🤖 |

### Customization Examples

**Default Configuration (Tark):**
```typescript
export const defaultAppConfig: AppConfig = {
  agentName: "Tark Terminal",
  agentNameShort: "Tark",
  version: "2.1.0",
  defaultPath: "~/tark/workspace",
  headerIcon: "🖥",
  agentIcon: "🤖",
};
```

**Alternative: "CodePilot":**
```typescript
export const defaultAppConfig: AppConfig = {
  agentName: "CodePilot Terminal",
  agentNameShort: "CodePilot",
  version: "1.0.0",
  defaultPath: "~/workspace",
  headerIcon: "🚀",
  agentIcon: "🤖",
};
```

---

## Screenshot References

**All screenshots are located in `./screenshots/` and fully documented in `SCREENSHOTS_REFERENCE.md`.**

Use these screenshots as your VISUAL ACCEPTANCE CRITERIA. Your Ratatui implementation must match what you see in these images.

### Component-to-Screenshot Mapping

This table maps each component to its relevant screenshot(s). **View these screenshots BEFORE implementing each component.**

| Component | Primary Screenshot(s) | Additional References |
|-----------|----------------------|----------------------|
| **Main Layout** | `01-main-layout.png` | `02-full-page.png` |
| **Terminal Header** | `01-main-layout.png` | `catppuccin-top.png` |
| **Message Types** | `03-message-types-top.png` | `themed-message-bubbles.png` |
| **User Messages** | `themed-message-bubbles.png` | `catppuccin-themed-bubbles.png`, `user-agent-bubbles.png` |
| **Agent Messages** | `themed-message-bubbles.png` | `catppuccin-themed-bubbles.png`, `user-agent-bubbles.png` |
| **Thinking Blocks** | `03-message-types-top.png` | `messages-with-themes.png` |
| **Tool Messages** | `03-message-types-top.png` | `02-full-page.png` |
| **System Messages** | `03-message-types-top.png` | `catppuccin-top.png` |
| **Status Bar** | `compact-status-bar.png` | `01-main-layout.png` |
| **Agent Mode Selector** | `04-agent-mode-dropdown.png` | `compact-status-bar.png` |
| **Build Mode Selector** | `05-build-mode-dropdown.png` | `compact-status-bar.png` |
| **Model Selector** | `model-selection-working.png` | `compact-status-bar.png` |
| **Working Indicator** | `agent-working-indicator.png` | `compact-status-bar.png` |
| **Queue Indicator** | `queue-indicator.png` | `compact-status-bar.png` |
| **Help Button** | `help-button-monochrome.png` | `compact-status-bar.png` |
| **Input Area** | `01-main-layout.png` | `02-full-page.png` |
| **Context File Tags** | `01-main-layout.png` | `02-full-page.png` |
| **Provider Picker** | `06-provider-picker-modal.png` | `picker-components-full.png` |
| **Model Picker** | `07-model-picker-modal.png` | `picker-components-full.png` |
| **File Picker** | `09-file-picker-modal.png` | `picker-components-full.png` |
| **Theme Picker** | `10-theme-picker-modal.png` | `theme-picker-modal.png` |
| **Help Modal** | `08-help-modal.png` | `help-modal-theme-check.png` |
| **Single Choice Question** | `12-question-single-selected.png` | `question-answered-state.png` |
| **Multiple Choice Question** | `questions-answered-with-selections.png` | `02-full-page.png` |
| **Free Text Question** | `03-message-types-top.png` | `02-full-page.png` |
| **Answered Questions** | `questions-answered-with-selections.png` | `question-answered-state.png` |
| **Sidebar Panel** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Sidebar Header** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Session Section** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Context Section** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Tasks Section** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Git Changes Section** | `11-sidebar-panel.png` | `panel-theme-applied.png` |
| **Catppuccin Theme** | `catppuccin-theme-applied.png` | `catppuccin-full-theme.png`, `catppuccin-themed-bubbles.png` |

### How to Use Screenshots

1. **Before Implementation**: Open the primary screenshot for the component
2. **During Implementation**: Keep screenshot visible for constant reference
3. **After Implementation**: Compare your TUI side-by-side with screenshot
4. **For Verification**: Use screenshot as acceptance criteria checklist

### Screenshot Quality Checklist

When comparing your implementation to screenshots:

- [ ] **Layout**: Proportions and positioning match
- [ ] **Colors**: RGB values match exactly (see theme.css for values)
- [ ] **Borders**: Style and thickness match (use box-drawing chars)
- [ ] **Spacing**: Padding and margins are proportional
- [ ] **Icons**: Correct Unicode/Nerd Font glyphs used
- [ ] **Text**: Formatting (bold, dim, etc.) matches emphasis
- [ ] **States**: All visual states (selected, hover, disabled) match
- [ ] **Alignment**: Text and elements align correctly

> **Tip**: For detailed visual analysis of each screenshot, see `SCREENSHOTS_REFERENCE.md`.

---

## Complete TUI Visual Reference

Below is an ASCII representation of the target TUI appearance. This is what your implementation should look like:

> **Note**: "TARK TERMINAL" is the default agent name. This is **configurable** via `AppConfig`.

```
╭─ {config.agent_name} ─────────────────────────────────────────────────────────────────╮╭─ Panel ──────────────────╮
│  {config.default_path}                                                                ││                          │
├───────────────────────────────────────────────────────────────────────────────────────┤│ ▼  Session              │
│                                                                                       ││    main                │
│  ● System initialized. Ready for commands.                                            ││   ⎇ feature/update      │
│                                                                                       ││   ✨ gemini-1.5-pro      │
│  👤 Analyze the Terminal.tsx file and suggest improvements                            ││   ☁ gemini-oauth         │
│                                                                                       ││   💰 $0.015 (3 models)   │
│  🤖 Tark: I'll analyze the Terminal.tsx file for you.                                 ││                          │
│                                                                                       ││ ▼  Context        1.0k  │
│     Looking at the code structure, I can see this is a React component that           ││    1,833 / 1M tokens    │
│     implements the main terminal interface. Here are my suggestions:                  ││   ▼ Loaded Files (8)     │
│                                                                                       ││      📄 Terminal.tsx     │
│     1. **Extract message rendering** - The message type switch is complex             ││      📄 Sidebar.tsx      │
│     2. **Memoize expensive computations** - Use useMemo for filtered lists            ││      📄 package.json     │
│     3. **Split into sub-components** - StatusBar, InputArea, MessageList              ││                          │
│                                                                                       ││ ▼  Tasks            8   │
│  🔧 Reading file: Terminal.tsx                                                        ││   ⟳ Analyzing codebase   │
│     ├─ Lines: 1,247                                                                   ││     Active              │
│     └─ Size: 45.2 KB                                                                  ││   ─────────────────────  │
│                                                                                       ││   ○ Refactor components  │
│  🧠 Thinking...                                                                       ││   ○ Add unit tests       │
│     │ Considering the file structure and import patterns...                           ││   ○ Update documentation │
│     │ The component has grown large - should I suggest splitting it?                  ││                          │
│     │ Looking for performance bottlenecks in the render cycle...                      ││ ▼  Git Changes     12   │
│                                                                                       ││   7 Mod │ 3 New │ 2 Del │
│  ╭──────────────────────────────────────────────────────────────────╮                 ││   ─────────────────────  │
│  │ ❓ How should I proceed?                                         │                 ││   M Terminal.tsx  +45 -12│
│  │                                                                  │                 ││   A helpers.ts      NEW  │
│  │   a) ◉ Continue with analysis                                    │                 ││   D old-utils.ts    DEL  │
│  │   b) ○ Start refactoring immediately                             │                 ││   M globals.css   +10 -5 │
│  │   c) ○ Show me the imports first                                 │                 ││                          │
│  │                                                                  │                 ││                          │
│  │                              [Cancel]  [Confirm]                 │                 ││                          │
│  ╰──────────────────────────────────────────────────────────────────╯                 ││                          │
│                                                                                       │╰──────────────────────────╯
├───────────────────────────────────────────────────────────────────────────────────────┤
│ ⚡ Claude 3.5 Sonnet │ ANTHROPIC ▼ │ Build Mode ▼ │ Careful │ ● Working... │ [?]     │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  📄 Terminal.tsx   📄 package.json                                                    │
│  ▶ Type a message...                                                              █   │
╰───────────────────────────────────────────────────────────────────────────────────────╯
```

### Component Legend

| Symbol | Component | Description |
|--------|-----------|-------------|
| `●` | System message | Cyan dot, system notifications |
| `👤` | User message | User input/commands |
| `🤖` | Agent response | Bot/AI responses |
| `🔧` | Tool execution | Expandable tool details |
| `🧠` | Thinking block | Agent reasoning (can be collapsed) |
| `❓` | Question modal | Interactive question from agent |
| `⎇` | Git branch | Branch indicator |
| `✨` | Model indicator | Active AI model |
| `⟳` | Spinner | Rotating animation for active tasks |
| `▼/▶` | Accordion | Collapsible section indicator |
| `◉/○` | Radio button | Single select option |
| `☑/☐` | Checkbox | Multi-select option |

### Color Reference (Catppuccin Mocha)

| Element | Color | RGB |
|---------|-------|-----|
| Background | Base | `#1e1e2e` |
| Text | Text | `#cdd6f4` |
| Border | Surface0 | `#313244` |
| System msg | Teal | `#94e2d5` |
| Agent msg | Text | `#cdd6f4` |
| Thinking | Yellow | `#f9e2af` |
| Question | Sky | `#89dceb` |
| Command | Green | `#a6e3a1` |
| Error | Red | `#f38ba8` |
| Warning | Yellow | `#f9e2af` |

---

## Layout Architecture

### Root Application Layout

The application uses a horizontal split with two main areas:

```
┌──────────────────────────────────────────────────────────────────────┐
│                                                                        │
│  Terminal Component                    │  Sidebar Component           │
│  (flex/dynamic width)                  │  (30-40 cols or 3-5 cols)    │
│                                                                        │
└──────────────────────────────────────────────────────────────────────┘
```

#### Ratatui Layout Code

```rust
use ratatui::layout::{Layout, Constraint, Direction};

fn create_root_layout(sidebar_collapsed: bool) -> Layout {
    let sidebar_width = if sidebar_collapsed {
        Constraint::Length(5)
    } else {
        Constraint::Length(35)
    };
    
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),      // Terminal (takes remaining)
            sidebar_width,           // Sidebar (fixed or collapsed)
        ])
}
```

### Terminal Component Layout

The Terminal is split vertically into 4 sections:

```
┌──────────────────────────────────────┐
│  Header (fixed: 3 rows)              │
├──────────────────────────────────────┤
│                                      │
│  Output Area (flexible, scrollable)  │
│                                      │
├──────────────────────────────────────┤
│  Status Bar (fixed: 2 rows)          │
├──────────────────────────────────────┤
│  Input Area (fixed: 3-5 rows)        │
└──────────────────────────────────────┘
```

#### Ratatui Layout Code

```rust
fn create_terminal_layout(has_context_files: bool) -> Layout {
    let input_height = if has_context_files { 5 } else { 3 };
    
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),           // Header
            Constraint::Min(0),              // Output (scrollable)
            Constraint::Length(2),           // Status bar
            Constraint::Length(input_height), // Input
        ])
}
```

### Sidebar Component Layout

The Sidebar has a header and scrollable panels:

```
┌─────────────────────────┐
│ Header (fixed: 3 rows)   │
├─────────────────────────┤
│                         │
│ Scrollable Panels       │
│ - Session               │
│ - Context               │
│ - Tasks                 │
│ - Git Changes           │
│                         │
└─────────────────────────┘
```

#### Ratatui Layout Code

```rust
fn create_sidebar_layout() -> Layout {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Panels (scrollable)
        ])
}
```

---

## Widget Mapping Table

| React Component/Element | Ratatui Widget | Notes |
|------------------------|----------------|-------|
| `<div>` with content | `Paragraph` | For text content |
| `<div>` with list items | `List` | For lists of items |
| Flex container | `Layout` | Horizontal/Vertical splits |
| `<button>` | Selectable `ListItem` or styled `Span` | Triggers on key press |
| `<input type="text">` | `Paragraph` with cursor | Custom input handling |
| `<pre>` / code block | `Paragraph` with `Block` | Monospace, bordered |
| Dropdown menu | Popup `List` with `ListState` | Overlay positioning |
| Scrollable area | `Paragraph` + `Scrollbar` | With `ScrollbarState` |
| Badge/Pill | Styled `Span` | Colored background |
| Icons (Lucide React) | Unicode characters | Use nerd fonts |
| Expandable section | Conditional rendering | Toggle in state |
| Modal/Dialog | Popup `Block` | Center overlay |
| Loading spinner | Rotating chars | "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏" |
| Checkbox | "☐" / "☑" chars | Unicode box chars |
| Progress bar | `Gauge` widget | Ratatui built-in |

### Icon Mapping

**Note on Nerd Fonts**: For the best TUI experience, recommend users install a [Nerd Font](https://www.nerdfonts.com/). The application should detect Nerd Font availability and fall back to Unicode characters otherwise.

#### Message Type Icons

| Lucide Icon | Purpose | Unicode | Nerd Font Code | Nerd Glyph |
|-------------|---------|---------|----------------|------------|
| `Circle` | System message | `●` | `\uf111` |  |
| `User` | User input | `👤` | `\uf007` |  |
| `Bot` | Agent output | `🤖` | `\uf544` |  |
| `Wrench` | Tool execution | `🔧` | `\uf0ad` |  |
| `Brain` | Thinking mode | `🧠` | `\uf0962` | 󰥢 |
| `MessageCircleQuestion` | Question | `❓` | `\uf059` |  |
| `Terminal` | Command | `$` | `\uf120` |  |

#### UI Control Icons

| Lucide Icon | Purpose | Unicode | Nerd Font Code | Nerd Glyph |
|-------------|---------|---------|----------------|------------|
| `ChevronDown` | Expanded | `▼` | `\uf078` |  |
| `ChevronRight` | Collapsed | `▶` | `\uf054` |  |
| `ChevronUp` | Scroll up | `▲` | `\uf077` |  |
| `ChevronsLeft` | Collapse sidebar | `«` | `\uf100` |  |
| `ChevronsRight` | Expand sidebar | `»` | `\uf101` |  |
| `Check` | Confirmed/Success | `✓` | `\uf00c` |  |
| `X` | Close/Cancel | `✕` | `\uf00d` |  |
| `Plus` | Add item | `+` | `\uf067` |  |
| `Copy` | Copy content | `📋` | `\uf0c5` |  |

#### Selection State Icons

| State | Unicode | Description |
|-------|---------|-------------|
| Radio unselected | `○` | Empty circle |
| Radio selected | `◉` | Filled circle with dot |
| Checkbox unchecked | `☐` | Empty square |
| Checkbox checked | `☑` | Square with check |
| List bullet | `•` | Small dot |
| Arrow pointer | `▸` | Small right arrow |

#### Status Indicator Icons

| Lucide Icon | Purpose | Unicode | Nerd Font Code |
|-------------|---------|---------|----------------|
| `Circle` (green) | Connected | `●` | `\uf111` |
| `AlertCircle` (yellow) | Warning | `⚠` | `\uf071` |
| `XCircle` (red) | Error/Disconnected | `✖` | `\uf057` |
| `HelpCircle` | Unknown/Help | `?` | `\uf059` |
| `Loader2` | Loading (animated) | `⟳` | - |

#### Spinner Animation Frames

```rust
const SPINNER_UNICODE: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_NERD: [&str; 7] = ["", "", "", "", "", "", ""];
const SPINNER_SIMPLE: [&str; 4] = ["|", "/", "-", "\\"];
```

#### File & Git Icons

| Lucide Icon | Purpose | Unicode | Nerd Font Code | Nerd Glyph |
|-------------|---------|---------|----------------|------------|
| `Folder` | Directory | `📁` | `\uf07b` |  |
| `FolderOpen` | Directory open | `📂` | `\uf07c` |  |
| `File` | Generic file | `📄` | `\uf15b` |  |
| `FileCode` | Code file | `📄` | `\uf1c9` |  |
| `GitBranch` | Branch | `⎇` | `\ue725` |  |
| `GitCommit` | Commit | `◉` | `\uf417` |  |

#### Additional System Icons

| Lucide Icon | Purpose | Unicode | Nerd Font Code |
|-------------|---------|---------|----------------|
| `Zap` | LLM active | `⚡` | `\uf0e7` |
| `Palette` | Theme selector | `🎨` | `\uf53f` |
| `HelpCircle` | Help modal | `?` | `\uf059` |
| `Settings` | Settings | `⚙` | `\uf013` |
| `Database` | Context/Memory | `💾` | `\uf1c0` |
| `Send` | Submit input | `➤` | `\uf1d8` |
| `Sparkles` | AI indicator | `✨` | `\uf005` |

#### Icon Usage in Rust

```rust
pub struct Icons {
    pub system: &'static str,
    pub user: &'static str,
    pub agent: &'static str,
    pub tool: &'static str,
    pub thinking: &'static str,
    pub question: &'static str,
    pub command: &'static str,
    pub chevron_down: &'static str,
    pub chevron_right: &'static str,
    pub check: &'static str,
    pub close: &'static str,
    pub spinner: &'static [&'static str],
}

impl Icons {
    pub const NERD_FONT: Icons = Icons {
        system: "",
        user: "",
        agent: "󰚩",
        tool: "",
        thinking: "󰥢",
        question: "",
        command: "",
        chevron_down: "",
        chevron_right: "",
        check: "",
        close: "",
        spinner: &["", "", "", "", "", "", ""],
    };
    
    pub const UNICODE: Icons = Icons {
        system: "●",
        user: "👤",
        agent: "🤖",
        tool: "🔧",
        thinking: "🧠",
        question: "❓",
        command: "$",
        chevron_down: "▼",
        chevron_right: "▶",
        check: "✓",
        close: "✕",
        spinner: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
    };
}
```

---

## Color Palette

### Base Colors

```rust
pub mod colors {
    use ratatui::style::Color;
    
    // Background colors
    pub const BG_MAIN: Color = Color::Rgb(13, 17, 23);        // #0d1117
    pub const BG_DARK: Color = Color::Rgb(11, 16, 21);        // #0b1015
    pub const BG_DARKER: Color = Color::Rgb(5, 7, 10);        // #05070a
    pub const BG_SIDEBAR: Color = Color::Rgb(22, 27, 34);     // #161b22
    pub const BG_CODE: Color = Color::Rgb(31, 41, 55);        // gray-800
    
    // Border colors
    pub const BORDER: Color = Color::Rgb(48, 54, 61);         // gray-800
    pub const BORDER_LIGHT: Color = Color::Rgb(55, 65, 81);   // gray-700
    
    // Text colors
    pub const TEXT_PRIMARY: Color = Color::Rgb(243, 244, 246); // gray-100
    pub const TEXT_SECONDARY: Color = Color::Rgb(156, 163, 175); // gray-400
    pub const TEXT_MUTED: Color = Color::Rgb(107, 114, 128);  // gray-500
    
    // Accent colors
    pub const CYAN: Color = Color::Rgb(103, 232, 249);        // cyan-400
    pub const BLUE: Color = Color::Rgb(96, 165, 250);         // blue-400
    pub const EMERALD: Color = Color::Rgb(52, 211, 153);      // emerald-400
    pub const AMBER: Color = Color::Rgb(251, 191, 36);        // amber-400
    pub const RED: Color = Color::Rgb(248, 113, 113);         // red-400
    pub const PURPLE: Color = Color::Rgb(192, 132, 252);      // purple-400
    
    // Git colors
    pub const GIT_MODIFIED: Color = Color::Rgb(234, 179, 8);  // yellow-500
    pub const GIT_NEW: Color = Color::Rgb(34, 197, 94);       // emerald-500
    pub const GIT_DELETED: Color = Color::Rgb(239, 68, 68);   // red-500
}
```

### Message Type Colors

| Message Type | Foreground | Background | Icon Color |
|-------------|-----------|------------|------------|
| System | Cyan (#67e8f9) | Cyan/10 | Cyan |
| User Input | Gray-100 | Gray-800 | Gray-400 |
| Bot Output | Gray-200 | Gray-800 | Emerald |
| Tool | Gray-200 | Gray-800 | Gray-400 |
| Thinking | Gray-400 | Gray-900 | Gray-500 |
| Question | Gray-100 | Purple/5 | Purple-400 |
| Command | Gray-200 | Transparent | Emerald (prompt) |

### Question Type Sub-variants

| Type | Icon | Behavior | Keyboard |
|------|------|----------|----------|
| Single Choice | ○ → ◉ | Radio buttons, one selection | Arrow keys, Space, a-z shortcuts |
| Multi Choice | ☐ → ☑ | Checkboxes, multiple selections | Arrow keys, Space to toggle |
| Free Text | ▸ cursor | Text input field | Type normally, Enter to submit |

### Agent Mode Colors

| Mode | Color | RGB |
|------|-------|-----|
| Build | Amber | (251, 191, 36) |
| Plan | Blue | (96, 165, 250) |
| Ask | Purple | (192, 132, 252) |

### Build Mode Colors

| Mode | Color | RGB |
|------|-------|-----|
| Careful | Red | (248, 113, 113) |
| Manual | Amber | (251, 191, 36) |
| Balanced | Emerald | (52, 211, 153) |

---

## State Management

### Main Application State

```rust
use std::collections::HashSet;

/// Main application state
pub struct App {
    // Terminal state
    pub terminal_output: Vec<TerminalLine>,
    pub input: String,
    pub cursor_position: usize,
    pub scroll_state: ScrollbarState,
    pub scroll_offset: usize,
    
    // Mode state
    pub agent_mode: AgentMode,
    pub build_mode: BuildMode,
    pub mode_selector_open: bool,
    pub build_mode_selector_open: bool,
    pub thinking_enabled: bool,
    
    // Tool interaction state
    pub expanded_tool_index: Option<usize>,
    pub copied_index: Option<String>,
    pub copy_notification_timer: Option<Instant>,
    
    // Context state
    pub context_files: Vec<ContextFile>,
    pub focused_context_index: Option<usize>,
    
    // Sidebar state
    pub sidebar_collapsed: bool,
    pub expanded_sections: HashSet<String>,
    pub sidebar_scroll_state: ScrollbarState,
    pub sidebar_scroll_offset: usize,
    
    // Tasks state
    pub active_task: Option<TaskItem>,
    pub queued_tasks: Vec<TaskItem>,
    pub focused_task_index: Option<usize>,
    
    // UI state
    pub is_terminal_focused: bool,
    pub edit_mode: Option<EditMode>,
    pub confirmation_dialog: Option<ConfirmationDialog>,
    
    // LLM state
    pub llm_model: String,
    pub llm_provider: String,
    pub connection_status: ConnectionStatus,
}

impl App {
    pub fn new() -> Self {
        let mut expanded_sections = HashSet::new();
        expanded_sections.insert("session".to_string());
        expanded_sections.insert("context".to_string());
        expanded_sections.insert("tasks".to_string());
        expanded_sections.insert("git".to_string());
        
        Self {
            terminal_output: vec![
                TerminalLine {
                    line_type: LineType::System,
                    content: "Tark Core v2.1.0 initialized".to_string(),
                    meta: None,
                    details: None,
                },
            ],
            input: String::new(),
            cursor_position: 0,
            scroll_state: ScrollbarState::default(),
            scroll_offset: 0,
            agent_mode: AgentMode::Build,
            build_mode: BuildMode::Balanced,
            mode_selector_open: false,
            build_mode_selector_open: false,
            thinking_enabled: true,
            expanded_tool_index: None,
            copied_index: None,
            copy_notification_timer: None,
            context_files: Vec::new(),
            focused_context_index: None,
            sidebar_collapsed: false,
            expanded_sections,
            sidebar_scroll_state: ScrollbarState::default(),
            sidebar_scroll_offset: 0,
            active_task: Some(TaskItem {
                id: "active-1".to_string(),
                name: "Understanding the codebase architecture".to_string(),
                icon: None,
            }),
            queued_tasks: vec![
                TaskItem {
                    id: "q-1".to_string(),
                    name: "Which is the most complex component?".to_string(),
                    icon: None,
                },
                // ... more tasks
            ],
            focused_task_index: None,
            is_terminal_focused: true,
            edit_mode: None,
            confirmation_dialog: None,
            llm_model: "Claude 3.5 Sonnet".to_string(),
            llm_provider: "Anthropic".to_string(),
            connection_status: ConnectionStatus::Active,
        }
    }
}
```

### Type Definitions

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineType {
    System,
    Command,
    Output,
    Input,
    Tool,
    Thinking,  // Agent's internal reasoning process
    Question,  // Agent asking user a question
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestionType {
    FreeText,       // Open text input
    SingleChoice,   // Radio buttons - select one
    MultiChoice,    // Checkboxes - select multiple
    ProviderPicker, // LLM provider selection with status icons
    ModelPicker,    // Model selection with capabilities
    FilePicker,     // File/folder tree selection
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderStatus {
    Active,   // ● Green - fully working
    Warning,  // ⚠ Yellow - needs attention  
    Error,    // ✖ Red - not working
    Unknown,  // ? Gray - status unknown
}

#[derive(Clone, Debug)]
pub struct QuestionOption {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

/// Provider option for LLM provider picker
#[derive(Clone, Debug)]
pub struct ProviderOption {
    pub id: String,
    pub name: String,
    pub description: String,
    pub icon: String,           // Emoji or Unicode character
    pub status: ProviderStatus,
}

/// Model option for model picker
#[derive(Clone, Debug)]
pub struct ModelOption {
    pub id: String,
    pub name: String,
    pub capabilities: Vec<String>,  // "tools", "reasoning", "vision", "structured"
    pub is_latest: bool,            // Highlighted differently
}

/// File option for file picker
#[derive(Clone, Debug)]
pub struct FileOption {
    pub path: String,
    pub name: String,
    pub is_folder: bool,
    pub indent_level: usize,  // For tree indentation
}

#[derive(Clone, Debug)]
pub struct TerminalLine {
    pub line_type: LineType,
    pub content: String,
    pub meta: Option<String>,
    pub details: Option<String>,
    // Question-specific fields
    pub question_type: Option<QuestionType>,
    pub options: Option<Vec<QuestionOption>>,
    pub placeholder: Option<String>,
    pub answered: Option<bool>,
    // Picker-specific fields
    pub providers: Option<Vec<ProviderOption>>,
    pub models: Option<Vec<ModelOption>>,
    pub files: Option<Vec<FileOption>>,
    pub answer: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AgentMode {
    Build,
    Plan,
    Ask,
}

impl AgentMode {
    pub fn color(&self) -> Color {
        match self {
            Self::Build => colors::AMBER,
            Self::Plan => colors::BLUE,
            Self::Ask => colors::PURPLE,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildMode {
    Careful,
    Manual,
    Balanced,
}

impl BuildMode {
    pub fn color(&self) -> Color {
        match self {
            Self::Careful => colors::RED,
            Self::Manual => colors::AMBER,
            Self::Balanced => colors::EMERALD,
        }
    }
    
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Careful => "🛡",
            Self::Manual => "⚡",
            Self::Balanced => "⚖",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ContextFile {
    pub name: String,
    pub path: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TaskItem {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionStatus {
    Active,
    Error,
}

#[derive(Clone, Debug)]
pub enum EditMode {
    TaskName {
        task_id: String,
        buffer: String,
        cursor_pos: usize,
    },
    FilePath {
        buffer: String,
        cursor_pos: usize,
    },
}

#[derive(Clone, Debug)]
pub enum ConfirmationDialog {
    CancelTask,
    DeleteTask(String),
}
```

---

## Event Handling

### Main Event Loop

```rust
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};

pub fn run_app(mut app: App) -> Result<()> {
    let mut terminal = setup_terminal()?;
    
    loop {
        terminal.draw(|frame| app.render(frame))?;
        
        if let Event::Key(key) = event::read()? {
            if handle_key_event(&mut app, key) {
                break; // Exit application
            }
        }
    }
    
    restore_terminal(terminal)?;
    Ok(())
}

fn handle_key_event(app: &mut App, key: KeyEvent) -> bool {
    // Check for edit mode first
    if let Some(edit_mode) = &app.edit_mode {
        return handle_edit_mode(app, key);
    }
    
    // Check for confirmation dialog
    if app.confirmation_dialog.is_some() {
        return handle_confirmation_dialog(app, key);
    }
    
    // Global keybindings
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => return true, // Quit
            KeyCode::Char('q') => return true, // Quit
            KeyCode::Char('t') => {
                app.thinking_enabled = !app.thinking_enabled;
                return false;
            }
            _ => {}
        }
    }
    
    // Mode-specific keybindings
    if app.is_terminal_focused {
        handle_terminal_keys(app, key)
    } else {
        handle_sidebar_keys(app, key)
    }
    
    false
}
```

### Keyboard Mappings

#### Global Keys

| Key | Action |
|-----|--------|
| `Ctrl+C` / `Ctrl+Q` | Quit application |
| `Tab` | Switch focus (Terminal ↔ Sidebar) |
| `Shift+Tab` | Switch focus backwards |
| `Ctrl+T` | Toggle thinking mode |

#### Terminal Keys (when focused)

| Key | Action |
|-----|--------|
| `Char(c)` | Append character to input |
| `Backspace` | Remove character |
| `Delete` | Delete character at cursor |
| `Left` / `Ctrl+B` | Move cursor left |
| `Right` / `Ctrl+F` | Move cursor right |
| `Home` / `Ctrl+A` | Move cursor to start |
| `End` / `Ctrl+E` | Move cursor to end |
| `Enter` | Submit input |
| `Up` | Scroll output up |
| `Down` | Scroll output down |
| `PageUp` | Scroll output page up |
| `PageDown` | Scroll output page down |
| `Esc` | Close open dropdowns |
| `Space` (on tool) | Toggle tool details |
| `c` (on message) | Copy focused message |
| `+` / `Ctrl+O` | Add context file |
| `x` / `Delete` (on context) | Remove context file |

#### Mode Selector Keys

| Key | Action |
|-----|--------|
| `m` | Open mode selector |
| `Up` / `Down` | Navigate options |
| `Enter` | Select mode |
| `Esc` | Close selector |

#### Build Mode Keys (when in Build mode)

| Key | Action |
|-----|--------|
| `Ctrl+1` | Set Careful mode |
| `Ctrl+2` | Set Manual mode |
| `Ctrl+3` | Set Balanced mode |
| `b` | Open build mode selector |

#### Sidebar Keys (when focused)

| Key | Action |
|-----|--------|
| `j` / `Down` | Move down |
| `k` / `Up` | Move up |
| `Enter` / `Space` | Toggle section / Execute action |
| `h` / `Left` | Collapse sidebar |
| `l` / `Right` | Expand sidebar |
| `a` / `Ctrl+A` | Toggle all sections |
| `e` | Edit focused task |
| `x` / `Delete` | Delete focused task / Cancel active |
| `Ctrl+Up` | Move task up |
| `Ctrl+Down` | Move task down |
| `g` / `Home` | Go to top |
| `G` / `End` | Go to bottom |

### Event Handler Functions

```rust
fn handle_terminal_keys(app: &mut App, key: KeyEvent) -> bool {
    // Close dropdowns on Esc
    if key.code == KeyCode::Esc {
        app.mode_selector_open = false;
        app.build_mode_selector_open = false;
        return false;
    }
    
    // Handle mode selector if open
    if app.mode_selector_open {
        return handle_mode_selector_keys(app, key);
    }
    
    // Handle build mode selector if open
    if app.build_mode_selector_open {
        return handle_build_mode_selector_keys(app, key);
    }
    
    // Handle input field
    match key.code {
        KeyCode::Char(c) => {
            app.input.insert(app.cursor_position, c);
            app.cursor_position += 1;
        }
        KeyCode::Backspace => {
            if app.cursor_position > 0 {
                app.input.remove(app.cursor_position - 1);
                app.cursor_position -= 1;
            }
        }
        KeyCode::Left => {
            app.cursor_position = app.cursor_position.saturating_sub(1);
        }
        KeyCode::Right => {
            app.cursor_position = (app.cursor_position + 1).min(app.input.len());
        }
        KeyCode::Enter => {
            app.submit_input();
        }
        KeyCode::Up => {
            app.scroll_up();
        }
        KeyCode::Down => {
            app.scroll_down();
        }
        _ => {}
    }
    
    false
}

fn handle_sidebar_keys(app: &mut App, key: KeyEvent) -> bool {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => {
            app.sidebar_next_item();
        }
        KeyCode::Char('k') | KeyCode::Up => {
            app.sidebar_prev_item();
        }
        KeyCode::Char('h') | KeyCode::Left => {
            app.sidebar_collapsed = true;
        }
        KeyCode::Char('l') | KeyCode::Right => {
            app.sidebar_collapsed = false;
        }
        KeyCode::Char('a') => {
            app.toggle_all_sections();
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            app.sidebar_toggle_selected();
        }
        KeyCode::Char('e') => {
            app.start_edit_task();
        }
        KeyCode::Char('x') | KeyCode::Delete => {
            app.show_delete_confirmation();
        }
        _ if key.modifiers.contains(KeyModifiers::CONTROL) => {
            match key.code {
                KeyCode::Up => app.move_task_up(),
                KeyCode::Down => app.move_task_down(),
                _ => {}
            }
        }
        _ => {}
    }
    
    false
}
```

---

## Animation Alternatives

Since Ratatui doesn't support smooth animations, here are alternatives:

| React Animation | Ratatui Alternative |
|----------------|-------------------|
| Fade-in | Instant render |
| Smooth transitions | Instant state change |
| Hover effects | Selection/focus highlighting |
| Loading spinner | Rotating characters: "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏" |
| Slide in/out | Instant show/hide |
| Opacity changes | Use dimmer colors |
| Scale transitions | Instant size change |
| Color transitions | Instant color change |

### Loading Spinner Implementation

```rust
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub struct Spinner {
    frame: usize,
    last_update: Instant,
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_update: Instant::now(),
        }
    }
    
    pub fn tick(&mut self) {
        if self.last_update.elapsed() > Duration::from_millis(80) {
            self.frame = (self.frame + 1) % SPINNER_FRAMES.len();
            self.last_update = Instant::now();
        }
    }
    
    pub fn current_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.frame]
    }
}

// Usage in render:
let spinner_icon = if let Some(active_task) = &app.active_task {
    format!("{} ", app.spinner.current_frame())
} else {
    String::new()
};
```

---

## Code Examples

### Complete Terminal Rendering

```rust
impl App {
    pub fn render(&mut self, frame: &mut Frame) {
        let size = frame.size();
        
        // Create root layout
        let sidebar_width = if self.sidebar_collapsed { 5 } else { 35 };
        let chunks = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(sidebar_width),
        ]).split(size);
        
        // Render terminal and sidebar
        self.render_terminal(frame, chunks[0]);
        self.render_sidebar(frame, chunks[1]);
    }
    
    fn render_terminal(&mut self, frame: &mut Frame, area: Rect) {
        // Create terminal layout
        let input_height = if self.context_files.is_empty() { 3 } else { 5 };
        let chunks = Layout::vertical([
            Constraint::Length(3),           // Header
            Constraint::Min(0),              // Output
            Constraint::Length(2),           // Status bar
            Constraint::Length(input_height), // Input
        ]).split(area);
        
        self.render_header(frame, chunks[0]);
        self.render_output(frame, chunks[1]);
        self.render_status_bar(frame, chunks[2]);
        self.render_input_area(frame, chunks[3]);
    }
    
    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal([
            Constraint::Percentage(50),
            Constraint::Percentage(50),
        ]).split(area);
        
        let title = Paragraph::new("🖥 TARK TERMINAL")
            .style(Style::default()
                .fg(colors::TEXT_SECONDARY)
                .add_modifier(Modifier::BOLD));
        frame.render_widget(title, chunks[0]);
        
        let path = Paragraph::new("~/tark/workspace")
            .style(Style::default().fg(colors::TEXT_MUTED))
            .alignment(Alignment::Right);
        frame.render_widget(path, chunks[1]);
    }
    
    fn render_output(&mut self, frame: &mut Frame, area: Rect) {
        let mut lines: Vec<Line> = vec![];
        
        for (idx, line) in self.terminal_output.iter().enumerate() {
            lines.extend(self.render_terminal_line(line, idx));
        }
        
        let content_height = lines.len();
        self.scroll_state = self.scroll_state
            .content_length(content_height)
            .viewport_content_length(area.height as usize);
        
        let paragraph = Paragraph::new(lines)
            .scroll((self.scroll_offset as u16, 0))
            .block(Block::default()
                .style(Style::default().bg(colors::BG_MAIN)));
        frame.render_widget(paragraph, area);
        
        // Render scrollbar
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));
        frame.render_stateful_widget(scrollbar, area, &mut self.scroll_state);
    }
    
    fn render_terminal_line(&self, line: &TerminalLine, index: usize) -> Vec<Line> {
        match line.line_type {
            LineType::System => self.render_system_message(&line.content),
            LineType::Input => self.render_input_message(&line.content),
            LineType::Output => self.render_output_message(&line.content),
            LineType::Tool => self.render_tool_message(line, index),
            LineType::Thinking => self.render_thinking_message(&line.content),
            LineType::Question => self.render_question(line, index),
            LineType::Command => self.render_command(&line.content),
        }
    }
    
    fn render_system_message(&self, content: &str) -> Vec<Line> {
        vec![
            Line::from(vec![
                Span::styled("● ", Style::default().fg(colors::CYAN)),
                Span::styled(content, Style::default().fg(colors::CYAN)),
            ]),
            Line::from(""), // Empty line for spacing
        ]
    }
    
    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::horizontal([
            Constraint::Percentage(70),
            Constraint::Percentage(30),
        ]).split(area);
        
        self.render_mode_selector(frame, chunks[0]);
        self.render_llm_status(frame, chunks[1]);
    }
}
```

### Question Rendering

```rust
/// Render interactive question with options
/// Style: Purple theme, interactive elements based on question type
fn render_question(&self, line: &TerminalLine, index: usize) -> Vec<Line> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("❓ ", Style::default().fg(Color::Rgb(192, 132, 252))), // Purple
            Span::styled("Question", Style::default().fg(Color::Rgb(167, 139, 250))),
        ]),
        Line::from(""),
        Line::styled(
            &line.content,
            Style::default()
                .fg(Color::Rgb(243, 244, 246))
                .add_modifier(Modifier::BOLD)
        ),
        Line::from(""),
    ];
    
    match line.question_type {
        Some(QuestionType::SingleChoice) => {
            // Radio buttons - only one can be selected
            if let Some(options) = &line.options {
                for (i, opt) in options.iter().enumerate() {
                    let prefix = if opt.selected { "◉" } else { "○" };
                    let letter = (b'a' + i as u8) as char;
                    let style = if opt.selected {
                        Style::default().fg(Color::Rgb(192, 132, 252)) // Purple when selected
                    } else {
                        Style::default().fg(Color::Rgb(156, 163, 175)) // Gray when not
                    };
                    lines.push(Line::styled(
                        format!("  {} {}) {}", prefix, letter, opt.label),
                        style
                    ));
                }
            }
        }
        Some(QuestionType::MultiChoice) => {
            // Checkboxes - multiple can be selected
            if let Some(options) = &line.options {
                for (i, opt) in options.iter().enumerate() {
                    let prefix = if opt.selected { "☑" } else { "☐" };
                    let letter = (b'a' + i as u8) as char;
                    let style = if opt.selected {
                        Style::default().fg(Color::Rgb(192, 132, 252))
                    } else {
                        Style::default().fg(Color::Rgb(156, 163, 175))
                    };
                    lines.push(Line::styled(
                        format!("  {} {}) {}", prefix, letter, opt.label),
                        style
                    ));
                }
                // Show selection count
                let count = options.iter().filter(|o| o.selected).count();
                lines.push(Line::styled(
                    format!("  ({} selected)", count),
                    Style::default().fg(Color::Rgb(107, 114, 128))
                ));
            }
        }
        Some(QuestionType::FreeText) => {
            // Text input with cursor
            let input = self.question_inputs.get(&index).unwrap_or(&String::new());
            let placeholder = line.placeholder.as_deref().unwrap_or("Type your answer...");
            let display = if input.is_empty() {
                format!("  ▸ {}", placeholder)
            } else {
                format!("  ▸ {}█", input) // Show cursor
            };
            lines.push(Line::styled(
                display,
                Style::default().fg(if input.is_empty() {
                    Color::Rgb(107, 114, 128) // Muted for placeholder
                } else {
                    Color::Rgb(229, 231, 235) // Bright for input
                })
            ));
        }
        Some(QuestionType::ProviderPicker) => {
            // Provider picker with status icons and filter
            // Renders as a bordered block with title
        }
        Some(QuestionType::ModelPicker) => {
            // Model picker with capabilities
            // Renders as a bordered block with title
        }
        Some(QuestionType::FilePicker) => {
            // File picker with tree view
            // Renders as a bordered block with title
        }
        _ => {}
    }
    
    // Submit hint
    if !line.answered.unwrap_or(false) {
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "  [Enter] Submit",
            Style::default().fg(Color::Rgb(107, 114, 128))
        ));
    }
    
    // Show answered state
    if line.answered.unwrap_or(false) {
        if let Some(answer) = &line.answer {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("  ✓ ", Style::default().fg(Color::Rgb(52, 211, 153))),
                Span::styled(
                    format!("Answered: {}", answer),
                    Style::default().fg(Color::Rgb(110, 231, 183))
                ),
            ]));
        }
    }
    
    lines.push(Line::from("")); // Spacing
    lines
}

/// Handle keyboard input for questions
fn handle_question_input(&mut self, key: KeyEvent, question_index: usize) {
    let line = &mut self.terminal_output[question_index];
    
    match line.question_type {
        Some(QuestionType::SingleChoice) => {
            match key.code {
                // Letter shortcuts (a, b, c, d...)
                KeyCode::Char(c) if c >= 'a' && c <= 'z' => {
                    let idx = (c as u8 - b'a') as usize;
                    if let Some(options) = &mut line.options {
                        if idx < options.len() {
                            // Deselect all, select this one
                            for opt in options.iter_mut() {
                                opt.selected = false;
                            }
                            options[idx].selected = true;
                        }
                    }
                }
                KeyCode::Up => { /* Move selection up */ }
                KeyCode::Down => { /* Move selection down */ }
                KeyCode::Enter | KeyCode::Char(' ') => { /* Confirm selection */ }
                _ => {}
            }
        }
        Some(QuestionType::MultiChoice) => {
            match key.code {
                // Letter shortcuts toggle the option
                KeyCode::Char(c) if c >= 'a' && c <= 'z' => {
                    let idx = (c as u8 - b'a') as usize;
                    if let Some(options) = &mut line.options {
                        if idx < options.len() {
                            options[idx].selected = !options[idx].selected;
                        }
                    }
                }
                KeyCode::Char(' ') => { /* Toggle current selection */ }
                KeyCode::Enter => { /* Confirm selections */ }
                _ => {}
            }
        }
        Some(QuestionType::FreeText) => {
            match key.code {
                KeyCode::Char(c) => {
                    self.question_inputs.entry(question_index)
                        .or_default()
                        .push(c);
                }
                KeyCode::Backspace => {
                    self.question_inputs.entry(question_index)
                        .or_default()
                        .pop();
                }
                KeyCode::Enter => { /* Submit answer */ }
                _ => {}
            }
        }
        _ => {}
    }
}
```

### System Modals (User-Triggered UI)

**IMPORTANT: System modals are SEPARATE from agent questions!**

System modals are popup overlays triggered by **user actions**:
- **Provider Picker**: Click LLM button in status bar OR type `/model`
- **Model Picker**: After provider selection
- **File Picker**: Type `@` in message input (for @ mentions/context files)

```rust
/// System modal types - NOT questions, triggered by user actions
pub enum SystemModal {
    None,
    ProviderPicker,  // Triggered: click status bar LLM OR "/model" command
    ModelPicker,     // Triggered: after provider selection
    FilePicker,      // Triggered: type "@" in input
}

pub struct AppState {
    // System modal state
    pub active_system_modal: SystemModal,
    pub modal_filter_text: String,
    pub modal_selected_index: usize,
    pub modal_list_state: ListState,
    
    // Modal data
    pub providers: Vec<ProviderOption>,
    pub models: Vec<ModelOption>,
    pub files: Vec<FileOption>,
}

/// Main render function - modal overlays main UI
fn render(&self, frame: &mut Frame) {
    // 1. Always render the main terminal UI
    self.render_terminal(frame);
    
    // 2. If system modal is active, overlay it on top
    match self.active_system_modal {
        SystemModal::None => {}
        SystemModal::ProviderPicker => self.render_provider_modal(frame),
        SystemModal::ModelPicker => self.render_model_modal(frame),
        SystemModal::FilePicker => self.render_file_modal(frame),
    }
    
    // 3. Question modals rendered separately (agent-triggered)
    if let Some(q_idx) = self.active_question_modal {
        self.render_question_modal(frame, q_idx);
    }
}

/// Detect triggers in input
fn handle_input_change(&mut self, input: &str) {
    // "@" triggers file picker
    if input.ends_with('@') {
        self.active_system_modal = SystemModal::FilePicker;
    }
    // "/model" triggers provider picker
    if input.trim() == "/model" {
        self.active_system_modal = SystemModal::ProviderPicker;
        self.input.clear();
    }
}

/// Modal key handling (takes priority when modal is open)
fn handle_key_event(&mut self, key: KeyEvent) {
    if self.active_system_modal != SystemModal::None {
        match key.code {
            KeyCode::Esc => {
                self.active_system_modal = SystemModal::None;
                self.modal_filter_text.clear();
            }
            KeyCode::Enter => {
                self.confirm_modal_selection();
                self.active_system_modal = SystemModal::None;
            }
            KeyCode::Up => self.modal_selected_index = self.modal_selected_index.saturating_sub(1),
            KeyCode::Down => self.modal_selected_index += 1,
            KeyCode::Char(c) => self.modal_filter_text.push(c),
            KeyCode::Backspace => { self.modal_filter_text.pop(); }
            _ => {}
        }
        return;  // Don't process other keys when modal open
    }
    // Normal key handling...
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}
```

**Invocation Summary:**
| Modal | Trigger | Purpose |
|-------|---------|---------|
| Provider | Click model selector button in status bar OR type `/model` | Select AI provider |
| Model | **Automatically shown** after provider is selected | Select model from chosen provider |
| File | Type `@` in input (immediately) OR click `+` button | Add context file |

**Provider → Model Chaining Flow:**
```rust
fn confirm_modal_selection(&mut self) {
    match self.active_system_modal {
        SystemModal::ProviderPicker => {
            // Save provider, then CHAIN to model picker
            self.selected_provider = Some(self.get_selected_provider());
            self.active_system_modal = SystemModal::ModelPicker;  // Auto-chain!
            self.modal_filter_text.clear();
            self.modal_selected_index = 0;
        }
        SystemModal::ModelPicker => {
            self.selected_model = Some(self.get_selected_model());
            self.active_system_modal = SystemModal::None;  // Close
        }
        SystemModal::FilePicker => {
            self.context_files.push(self.get_selected_file());
            self.active_system_modal = SystemModal::None;  // Close
        }
        _ => {}
    }
}
```

**Click Handlers:**
```rust
// Model selector button in status bar (shows "Claude 3.5 Sonnet ANTHROPIC")
fn on_model_selector_click(&mut self) {
    self.active_system_modal = SystemModal::ProviderPicker;
}

// Plus (+) button in input area
fn on_plus_button_click(&mut self) {
    self.active_system_modal = SystemModal::FilePicker;
}
```

```rust
/// Render provider picker - LLM provider selection
/// Style: Bordered block with status icons and filter input
fn render_provider_picker(&mut self, frame: &mut Frame, area: Rect, providers: &[ProviderOption]) {
    let block = Block::bordered()
        .title(" Select Provider ")
        .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
        .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    let chunks = Layout::vertical([
        Constraint::Length(1),  // Filter input with ">" prompt
        Constraint::Min(0),     // Provider list
    ]).split(inner);
    
    // Filter input
    let filter_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled(&self.filter_text, Style::default().fg(Color::White)),
        Span::styled("_", Style::default().fg(Color::White).add_modifier(Modifier::SLOW_BLINK)),
    ]);
    frame.render_widget(Paragraph::new(filter_line), chunks[0]);
    
    // Build list items with status icons
    let items: Vec<ListItem> = providers.iter()
        .filter(|p| p.name.to_lowercase().contains(&self.filter_text.to_lowercase()))
        .map(|p| {
            // Status icon based on provider status
            let status_icon = match p.status {
                ProviderStatus::Active => Span::styled("● ", Style::default().fg(Color::Green)),
                ProviderStatus::Warning => Span::styled("⚠ ", Style::default().fg(Color::Yellow)),
                ProviderStatus::Error => Span::styled("✖ ", Style::default().fg(Color::Red)),
                ProviderStatus::Unknown => Span::styled("? ", Style::default().fg(Color::Gray)),
            };
            ListItem::new(Line::from(vec![
                status_icon,
                Span::styled(&p.icon, Style::default()),
                Span::raw(" "),
                Span::styled(&p.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::styled(" - ", Style::default().fg(Color::Rgb(107, 114, 128))),
                Span::styled(&p.description, Style::default().fg(Color::Rgb(107, 114, 128)).add_modifier(Modifier::ITALIC)),
            ]))
        })
        .collect();
    
    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
        .highlight_symbol("▶ ");
    
    frame.render_stateful_widget(list, chunks[1], &mut self.provider_list_state);
}

/// Render model picker - Model selection with capabilities
/// Style: Bordered block with model names and capability tags
fn render_model_picker(&mut self, frame: &mut Frame, area: Rect, models: &[ModelOption]) {
    let block = Block::bordered()
        .title(" Select Model ")
        .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
        .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    let chunks = Layout::vertical([
        Constraint::Length(1),  // Filter input
        Constraint::Min(0),     // Model list
    ]).split(inner);
    
    // Filter input
    let filter = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled(&self.filter_text, Style::default().fg(Color::White)),
        Span::styled("...", Style::default().fg(Color::Rgb(107, 114, 128))),
    ]));
    frame.render_widget(filter, chunks[0]);
    
    // Model list items
    let items: Vec<ListItem> = models.iter()
        .filter(|m| m.name.to_lowercase().contains(&self.filter_text.to_lowercase()))
        .map(|m| {
            let mut spans = vec![
                // Highlight latest models in yellow
                Span::styled(&m.name, Style::default()
                    .fg(if m.is_latest { Color::Yellow } else { Color::White })
                    .add_modifier(Modifier::BOLD)),
            ];
            if !m.capabilities.is_empty() {
                spans.push(Span::styled(" - ", Style::default().fg(Color::Rgb(75, 85, 99))));
                spans.push(Span::styled(
                    m.capabilities.join(", "),
                    Style::default().fg(Color::Rgb(107, 114, 128))
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    
    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
        .highlight_symbol("● ");
    
    frame.render_stateful_widget(list, chunks[1], &mut self.model_list_state);
}

/// Render file picker - File tree with folder/file icons
/// Style: Bordered block with indented tree structure
fn render_file_picker(&mut self, frame: &mut Frame, area: Rect, files: &[FileOption]) {
    let block = Block::bordered()
        .title(" Select File ")
        .title_style(Style::default().fg(Color::Rgb(156, 163, 175)))
        .border_style(Style::default().fg(Color::Rgb(52, 58, 64)));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    let chunks = Layout::vertical([
        Constraint::Min(0),     // File list
        Constraint::Length(1),  // INSERT mode indicator
    ]).split(inner);
    
    // File list items with indentation
    let items: Vec<ListItem> = files.iter()
        .map(|f| {
            let indent = "  ".repeat(f.indent_level);
            let icon = if f.is_folder { "📁" } else { "📄" };
            let style = if f.is_folder {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            ListItem::new(Line::from(vec![
                Span::raw(indent),
                Span::styled(icon, style),
                Span::raw(" "),
                Span::styled(&f.name, style),
            ]))
        })
        .collect();
    
    let list = List::new(items)
        .highlight_style(Style::default().bg(Color::Rgb(31, 41, 55)))
        .highlight_symbol("▶ ");
    
    frame.render_stateful_widget(list, chunks[0], &mut self.file_list_state);
    
    // INSERT mode indicator at bottom
    let mode_line = Line::from(vec![
        Span::styled("[INSERT]", Style::default().fg(Color::Rgb(107, 114, 128))),
        Span::raw(" "),
        Span::styled("@", Style::default().fg(Color::Rgb(156, 163, 175))),
        Span::styled("█", Style::default().fg(Color::Rgb(156, 163, 175)).add_modifier(Modifier::SLOW_BLINK)),
    ]);
    frame.render_widget(Paragraph::new(mode_line), chunks[1]);
}
```

### Message Type Rendering

```rust
/// Render thinking/reasoning message
/// Style: Muted gray, italic appearance, dashed border feel, brain icon
fn render_thinking_message(&self, content: &str) -> Vec<Line> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("🧠 ", Style::default().fg(Color::Rgb(107, 114, 128))),
            Span::styled("Thinking...", Style::default()
                .fg(Color::Rgb(107, 114, 128))
                .add_modifier(Modifier::ITALIC)),
        ]),
    ];
    
    // Render content with muted styling
    for content_line in content.lines() {
        lines.push(Line::styled(
            format!("  {}", content_line),
            Style::default()
                .fg(Color::Rgb(156, 163, 175))  // Gray-400
                .add_modifier(Modifier::ITALIC)
        ));
    }
    
    lines.push(Line::from("")); // Spacing
    lines
}
```

```rust
fn render_tool_message(&self, line: &TerminalLine, index: usize) -> Vec<Line> {
    let mut lines = vec![
        Line::from(vec![
            Span::styled("🔧 ", Style::default().fg(colors::TEXT_SECONDARY)),
            Span::styled("Tool", Style::default().fg(colors::TEXT_SECONDARY)),
        ]),
    ];
    
    // Main content
    for content_line in line.content.lines() {
        lines.push(Line::styled(
            format!("  {}", content_line),
            Style::default()
                .bg(colors::BG_CODE)
                .fg(colors::TEXT_PRIMARY)
        ));
    }
    
    // Show expand indicator if has details
    if line.details.is_some() {
        let indicator = if self.expanded_tool_index == Some(index) {
            "▲ Hide details"
        } else {
            "▼ Show details"
        };
        lines.push(Line::styled(
            format!("  {}", indicator),
            Style::default().fg(colors::TEXT_MUTED)
        ));
    }
    
    // Render details if expanded
    if self.expanded_tool_index == Some(index) {
        if let Some(details) = &line.details {
            lines.push(Line::from(""));
            for detail_line in details.lines() {
                lines.push(Line::styled(
                    format!("    {}", detail_line),
                    Style::default().fg(colors::TEXT_SECONDARY)
                ));
            }
        }
    }
    
    lines.push(Line::from("")); // Spacing
    lines
}
```

### Sidebar Rendering

```rust
fn render_sidebar(&mut self, frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::LEFT)
        .border_style(Style::default().fg(colors::BORDER))
        .style(Style::default().bg(colors::BG_SIDEBAR));
    
    let inner = block.inner(area);
    frame.render_widget(block, area);
    
    let chunks = Layout::vertical([
        Constraint::Length(3),  // Header
        Constraint::Min(0),     // Panels
    ]).split(inner);
    
    self.render_sidebar_header(frame, chunks[0]);
    self.render_sidebar_panels(frame, chunks[1]);
}

fn render_sidebar_header(&self, frame: &mut Frame, area: Rect) {
    if self.sidebar_collapsed {
        let button = Paragraph::new("»")
            .alignment(Alignment::Center)
            .style(Style::default().fg(colors::TEXT_SECONDARY));
        frame.render_widget(button, area);
    } else {
        let chunks = Layout::horizontal([
            Constraint::Min(0),
            Constraint::Length(10),
        ]).split(area);
        
        let title = Paragraph::new("Panel")
            .style(Style::default()
                .fg(colors::TEXT_PRIMARY)
                .add_modifier(Modifier::BOLD));
        frame.render_widget(title, chunks[0]);
        
        let buttons = Paragraph::new("⇅ «")
            .alignment(Alignment::Right)
            .style(Style::default().fg(colors::TEXT_SECONDARY));
        frame.render_widget(buttons, chunks[1]);
    }
}

fn render_sidebar_panels(&mut self, frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line> = vec![];
    
    let panels = self.get_panels();
    for panel in panels {
        lines.extend(self.render_panel(&panel));
    }
    
    let paragraph = Paragraph::new(lines)
        .scroll((self.sidebar_scroll_offset as u16, 0));
    frame.render_widget(paragraph, area);
    
    let scrollbar = Scrollbar::default()
        .orientation(ScrollbarOrientation::VerticalRight);
    frame.render_stateful_widget(scrollbar, area, &mut self.sidebar_scroll_state);
}
```

---

## Required Crates

Add these dependencies to `Cargo.toml`:

```toml
[dependencies]
# Core TUI
ratatui = "0.26"
crossterm = "0.27"

# Clipboard support
arboard = "3.3"
# OR: clipboard = "0.5"

# Text handling
unicode-width = "0.1"
textwrap = "0.16"

# Async/timing (if needed)
tokio = { version = "1", features = ["full"] }
```

---

## Function Verification Templates

Use these templates to document your function verification process for each component.

### Template 1: Existing Function Mapping

When you find an existing function in the Ratatui codebase:

```markdown
### Component: [Component Name]

**React Source:** `src/app/components/[file].tsx` lines [X-Y]
**Screenshot:** `screenshots/[filename].png`
**RATATUI_MAPPING:** Section [X.Y]

**Function Found:**
- Name: `[function_name]`
- Location: `[file_path:line]`
- Signature: `[full function signature]`
- Match Quality: ☐ EXACT  ☐ PARTIAL (XX%)  ☐ NONE

**Parameter Mapping:**
| React Prop/State | Rust Parameter | Conversion Notes |
|-----------------|----------------|------------------|
| [prop_name] | [param_name] | [notes] |

**Behavior Verification:**
- [ ] Function behavior matches React component
- [ ] All visual states supported
- [ ] Event handling matches
- [ ] Edge cases covered

**Required Modifications:**
- [ ] None (exact match)
- [ ] Parameter adjustment: [describe]
- [ ] Behavior extension: [describe]
- [ ] Return type change: [describe]

**Implementation Notes:**
[Any additional notes about using this function]
```

### Template 2: New Function Creation

When no existing function is found:

```markdown
### Component: [Component Name]

**React Source:** `src/app/components/[file].tsx` lines [X-Y]
**Screenshot:** `screenshots/[filename].png`
**RATATUI_MAPPING:** Section [X.Y]

**Search Performed:**
- Query: `[search terms used]`
- Results: No matching function found
- Similar functions: [list any partial matches]

**New Function Design:**

```rust
/// [Brief description of what this function does]
/// 
/// # Arguments
/// * `[param]` - [description]
/// 
/// # Returns
/// [Description of return value]
/// 
/// # Example
/// ```
/// [usage example]
/// ```
pub struct [ComponentName] {
    // State fields
    pub field1: Type1,
    pub field2: Type2,
}

impl [ComponentName] {
    pub fn new(...) -> Self {
        // Constructor
    }
    
    pub fn render(&self, area: Rect, buf: &mut Buffer) {
        // Rendering logic
    }
    
    pub fn handle_event(&mut self, event: Event) -> bool {
        // Event handling
    }
}
```

**Dependencies Required:**
- [ ] New crate: [crate_name]
- [ ] New module: [module_name]
- [ ] Helper functions: [list]

**Implementation Plan:**
1. [Step 1]
2. [Step 2]
3. [Step 3]

**Verification Criteria:**
- [ ] Renders correctly
- [ ] Matches screenshot
- [ ] Handles all events
- [ ] Theme colors applied
- [ ] Edge cases covered
```

### Template 3: Component Verification Log

Use this template to track verification status:

```markdown
## [Component Name] Verification Log

**Date:** [YYYY-MM-DD]
**Status:** ☐ Not Started  ☐ In Progress  ☐ Needs Review  ☐ Complete

### Visual Verification
- [ ] Screenshot reviewed: `screenshots/[filename].png`
- [ ] Layout analyzed and understood
- [ ] Colors extracted and documented
- [ ] Spacing/sizing measured
- [ ] Interactive elements identified

### Code Verification
- [ ] React code read: `[file]:lines [X-Y]`
- [ ] All @ratatui-* annotations found: [count]
- [ ] State variables documented: [count]
- [ ] Event handlers mapped: [count]
- [ ] Discrepancies noted: ☐ None  ☐ [describe]

### Documentation Verification
- [ ] RATATUI_MAPPING.md section read: Section [X.Y]
- [ ] Widget mapping understood
- [ ] Color references extracted
- [ ] Event handling approach understood
- [ ] Code examples reviewed

### Function Search Results
- [ ] Search performed with terms: `[terms]`
- [ ] Result: ☐ Function found  ☐ No match  ☐ Partial match
- [ ] Function name: `[name]` or "NONE"
- [ ] Match quality: [percentage]%

### Implementation Status
- [ ] Function mapped/created
- [ ] Rendering implemented
- [ ] Event handling implemented
- [ ] State management implemented
- [ ] Theme colors applied
- [ ] Tests written

### Verification Results
- [ ] Visual match with screenshot: [percentage]%
- [ ] Behavior matches React: ☐ Yes  ☐ Mostly  ☐ Needs work
- [ ] All features working: ☐ Yes  ☐ Missing: [list]
- [ ] Performance acceptable: ☐ Yes  ☐ Needs optimization
- [ ] Edge cases handled: ☐ Yes  ☐ Missing: [list]

### Notes
[Any additional notes, issues, or observations]
```

### Template 4: Cross-Reference Index

Track all components and their verification status:

```markdown
## Component Verification Index

| Component | React Lines | Screenshot | Function Found | Status | Notes |
|-----------|------------|------------|----------------|--------|-------|
| Terminal Header | Terminal.tsx:150-180 | 01-main-layout.png | ✅ render_header() | ✅ | None |
| Status Bar | Terminal.tsx:580-680 | compact-status-bar.png | ⚠️ Partial | 🟡 | Needs queue icon |
| Agent Mode | Terminal.tsx:585-610 | 04-agent-mode-dropdown.png | ❌ None | 🔴 | Create new |
| ... | ... | ... | ... | ... | ... |

**Status Legend:**
- ✅ Complete and verified
- 🟡 In progress
- 🔴 Not started
- ⚠️ Needs attention
```

### Using the Templates

1. **Start with Template 3** (Verification Log) for each new component
2. **Use Template 1 or 2** based on function search results
3. **Update Template 4** (Cross-Reference Index) as you progress
4. **Keep templates in a separate `verification/` folder** for easy reference

### Example: Complete Verification for "Status Bar"

```markdown
### Component: Status Bar

**React Source:** `src/app/components/Terminal.tsx` lines 580-680
**Screenshot:** `screenshots/compact-status-bar.png`
**RATATUI_MAPPING:** Section 2.4

**Function Found:**
- Name: `render_status_bar()`
- Location: `ui/terminal.rs:450`
- Signature: `fn render_status_bar(&self, area: Rect, buf: &mut Buffer)`
- Match Quality: ☑ PARTIAL (75%)

**Parameter Mapping:**
| React Prop/State | Rust Parameter | Conversion Notes |
|-----------------|----------------|------------------|
| agentMode | self.state.agent_mode | Enum: Build/Plan/Ask |
| buildMode | self.state.build_mode | Enum: Careful/Manual/Balanced |
| currentModel | self.state.model | String |
| currentProvider | self.state.provider | String |
| thinkingEnabled | self.state.thinking | bool |
| taskQueueCount | self.state.queue_count | usize |

**Behavior Verification:**
- [x] Function behavior matches React component
- [x] All visual states supported
- [x] Event handling matches
- [ ] Edge cases covered - Missing: queue indicator rendering

**Required Modifications:**
- [ ] None (exact match)
- [x] Behavior extension: Add queue icon with count badge
- [x] Behavior extension: Add working indicator animation
- [ ] Parameter adjustment
- [ ] Return type change

**Implementation Notes:**
- Existing function handles basic status bar layout
- Need to add: queue_indicator() helper function
- Need to add: working_indicator() helper function
- Reference: `screenshots/queue-indicator.png` and `agent-working-indicator.png`
```

---

## Implementation Checklist

### Phase 1: Basic Structure
- [ ] Set up Rust project with Ratatui
- [ ] Implement basic terminal setup/teardown
- [ ] Create main event loop
- [ ] Implement root layout (Terminal + Sidebar split)

### Phase 2: Terminal Component
- [ ] Implement terminal header
- [ ] Create output area with scrolling
- [ ] Implement different message types (System, Input, Output, Tool, Command)
- [ ] Add status bar with mode selector
- [ ] Create input area with cursor
- [ ] Implement context file pills

### Phase 3: Message Rendering
- [ ] System messages with cyan styling
- [ ] User input messages with user icon
- [ ] Bot output messages with bot icon
- [ ] Tool messages with expandable details
- [ ] Command messages with shell prompt

### Phase 4: Sidebar Component
- [ ] Implement sidebar header with toggle buttons
- [ ] Create scrollable panels area
- [ ] Implement accordion sections
- [ ] Add Session panel with cost breakdown
- [ ] Add Context panel with loaded files
- [ ] Add Tasks panel with active/queued tasks
- [ ] Add Git Changes panel with diff stats

### Phase 5: Interactions
- [ ] Mode selector dropdown
- [ ] Build mode selector dropdown
- [ ] Task editing (prompt for new name)
- [ ] Task deletion with confirmation
- [ ] Task reordering (move up/down)
- [ ] Context file addition/removal
- [ ] Section expansion/collapse
- [ ] Sidebar collapse/expand

### Phase 6: Advanced Features
- [ ] Clipboard integration (copy messages)
- [ ] Copy notification feedback
- [ ] Scrollbar for long content
- [ ] Loading spinner for active task
- [ ] Keyboard shortcuts
- [ ] Focus management (Terminal ↔ Sidebar)
- [ ] Thinking mode toggle

### Phase 7: Polish
- [ ] Color theme application
- [ ] Unicode icon integration
- [ ] Word wrapping for long text
- [ ] Proper alignment and spacing
- [ ] Error handling
- [ ] Performance optimization

---

## Additional Notes

### Text Wrapping

Use the `textwrap` crate for wrapping long messages:

```rust
use textwrap::wrap;

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    wrap(text, width)
        .into_iter()
        .map(|cow| cow.to_string())
        .collect()
}
```

### Clipboard Integration

```rust
use arboard::Clipboard;

fn copy_to_clipboard(text: &str) -> Result<()> {
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(text)?;
    Ok(())
}
```

### Unicode Width Calculation

```rust
use unicode_width::UnicodeWidthStr;

let text = "Hello 世界";
let width = text.width();  // Correctly handles multi-byte chars
```

### Focus Management

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FocusArea {
    Terminal,
    Sidebar,
}

impl App {
    fn cycle_focus(&mut self) {
        self.focus_area = match self.focus_area {
            FocusArea::Terminal => FocusArea::Sidebar,
            FocusArea::Sidebar => FocusArea::Terminal,
        };
    }
}
```

---

## Conclusion

This guide provides a comprehensive mapping from the React-based Tark Terminal UI to a Ratatui TUI implementation. Follow the implementation checklist and refer to the code examples to build the TUI step by step.

Key takeaways:
1. **Layout**: Use `Layout` for splits, `Block` for borders
2. **Widgets**: `Paragraph` for text, `List` for lists, `Scrollbar` for scrolling
3. **State**: Single `App` struct holds all state
4. **Events**: Keyboard-driven navigation and interaction
5. **Colors**: Define RGB colors matching the original design
6. **Icons**: Use Unicode characters or nerd fonts
7. **No Animations**: Everything renders instantly

For questions or clarifications, refer to the inline comments in the source files:
- `src/app/App.tsx` - Root layout and composition
- `src/app/components/Terminal.tsx` - Terminal UI and message types
- `src/app/components/Sidebar.tsx` - Sidebar panels and tasks

Good luck with your Ratatui implementation!
