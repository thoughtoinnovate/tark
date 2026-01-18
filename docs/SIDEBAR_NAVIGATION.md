# Sidebar Navigation Guide

## Overview

The sidebar now supports full keyboard and mouse navigation with expand/collapse functionality, theme selector, and nested section navigation.

## Visual Improvements

### Header Section
- **Theme selector**: "Theme: Catppuccin Mocha" (cyan colored, clickable)
- **Close button**: "⟩" on the right
- **Collapse all** button (planned)

### Section Headers
- **Expand/Collapse icons**: 
  - `▼` = Expanded section
  - `▶` = Collapsed section
- **Section badges**: Show counts (e.g., "Tasks 8", "Context 1.8k")
- **Selection highlighting**: Cyan background when focused and selected

### Item Highlighting
- **Focused item**: Cyan text with blue background
- **Unfocused**: Normal text colors
- **Nested items**: Proper indentation

## Keyboard Navigation

### Focus Management

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: Input → Messages → **Sidebar** → Input |
| `Shift+Tab` | Reverse cycle |
| `Ctrl+B` | Toggle sidebar visibility |

### Sidebar Navigation (when Panel focused)

#### Panel-Level Navigation
| Key | Action |
|-----|--------|
| `j` or `↓` | Next panel (Session → Context → Tasks → Git Changes) |
| `k` or `↑` | Previous panel |
| `Enter` | Expand collapsed panel, or enter into panel |
| `Space` | Toggle panel expand/collapse |

#### Item-Level Navigation (inside panel)
| Key | Action |
|-----|--------|
| `j` or `↓` | Next item in current panel |
| `k` or `↑` | Previous item |
| `l` or `→` | Enter into panel (select first item) |
| `h` or `←` or `-` | Exit panel (back to panel header) |
| `Esc` | Exit panel or collapse panel |

### Navigation Flow Example

```
1. Press Tab → Focus moves to sidebar (Panel)
2. Sidebar header shows highlighted selection

3. Press j → Move to Session section (highlighted)
4. Press Enter → Enter into Session section
5. Now j/k navigate through:
   - Branch: main
   - Branch: feature/sidebar-update
   - Branch: gemini-1.5-pro-preview
   - Cost: $0.015 (3 models)

6. Press h or Esc → Back to Session header
7. Press j → Move to Context section
8. Press Enter → Enter into Context section
9. j/k navigate through loaded files

10. Press - or Esc → Back to Context header
11. Press Space → Collapse Context section (▶ appears)
12. Press Space → Expand again (▼ appears)
```

## Mouse Support

### Click Actions
- **Section header**: Toggle expand/collapse
- **Section items**: Select item
- **Theme selector**: Open theme picker
- **Close button (⟩)**: Close sidebar

### Planned Features
- Drag to resize sidebar width
- Right-click context menus
- Double-click to open files

## Panel States

### Expanded vs Collapsed

**Expanded (▼)**:
```
▼ Session
  ⎇ main
  ⎇ feature/sidebar-update
  $0.015 (3 models)
```

**Collapsed (▶)**:
```
▶ Session
```

### Focused vs Unfocused

**Focused Panel** (cyan highlight):
```
▼ Context  1.8k    ← Cyan background when selected
  1,833 / 1,000,000 tokens
```

**Unfocused**:
```
▼ Context  1.8k    ← Normal colors
  1,833 / 1,000,000 tokens
```

### Item Selection

**Selected Item** (inside focused panel):
```
▼ Context  1.8k
  LOADED FILES (8)
  📄 src/components/Sidebar.tsx    ← Cyan highlight
  📄 src/styles/
  📄 package.json
```

## State Management

### AppState Fields
```rust
pub struct AppState {
    // Sidebar focus
    focused_component: FocusedComponent,  // Panel when sidebar focused
    
    // Sidebar navigation
    sidebar_selected_panel: usize,        // 0-3 (Session, Context, Tasks, Git)
    sidebar_selected_item: Option<usize>, // None = panel header, Some(i) = item
    sidebar_expanded_panels: [bool; 4],   // Which panels are expanded
    
    // Sidebar visibility
    sidebar_visible: bool,                // Toggle with Ctrl+B
}
```

### Navigation Methods
```rust
impl AppState {
    // Panel navigation
    sidebar_next_panel()     // j or ↓ (when at panel level)
    sidebar_prev_panel()     // k or ↑ (when at panel level)
    
    // Item navigation
    sidebar_next_item()      // j or ↓ (when inside panel)
    sidebar_prev_item()      // k or ↑ (when inside panel)
    
    // Enter/Exit
    sidebar_enter_panel()    // Enter, l, or → (enter into panel)
    sidebar_exit_panel()     // Esc, h, ←, or - (exit from panel)
    
    // Toggle
    sidebar_toggle_panel(i)  // Toggle specific panel expansion
}
```

## Visual States

### 1. No Focus (Tab on Input or Messages)
```
┌─ Panel ──────────────┐
│  Theme: Catppuccin Mocha  ⟩
│
│ ▼ Session
│   ⎇ main
│   $0.015 (3 models)
│
│ ▼ Context  1.8k
│   ...
└──────────────────────┘
```

### 2. Focused on Panel Header
```
┌─ Panel ──────────────┐  ← Cyan border
│  Theme: Catppuccin Mocha  ⟩
│
│ ▼ Session             ← Cyan highlight
│   ⎇ main
│   $0.015 (3 models)
│
│ ▼ Context  1.8k
│   ...
└──────────────────────┘
```

### 3. Focused Inside Panel
```
┌─ Panel ──────────────┐  ← Cyan border
│  Theme: Catppuccin Mocha  ⟩
│
│ ▼ Context  1.8k
│   1,833 / 1,000,000 tokens
│   LOADED FILES (8)
│   📄 Sidebar.tsx      ← Cyan highlight (selected)
│   📄 styles/
│   📄 package.json
└──────────────────────┘
```

### 4. Collapsed Panel
```
┌─ Panel ──────────────┐
│  Theme: Catppuccin Mocha  ⟩
│
│ ▶ Session             ← Collapsed
│
│ ▶ Context  1.8k       ← Collapsed
│
│ ▼ Tasks  8            ← Expanded
│   ● Understanding...
└──────────────────────┘
```

## Implementation Details

### Sidebar Widget Methods
```rust
impl Sidebar {
    // State setters
    .focused(bool)               // Set focus state
    .selected_panel(usize)       // Set selected panel index
    .expanded(panel, bool)       // Set panel expansion
    .theme_name(String)          // Set theme display name
    
    // Navigation
    .next_panel()                // Move to next panel
    .prev_panel()                // Move to previous panel
    .next_item()                 // Navigate within panel
    .prev_item()                 // Navigate within panel
    .enter_panel()               // Enter/expand panel
    .exit_panel()                // Exit/collapse panel
    .toggle_panel(idx)           // Toggle panel state
}
```

### Event Handling Flow

```
User Action → Event → AppState Method → Sidebar Updated → Re-render

Example: Press 'j' when sidebar focused
1. KeyCode::Char('j') detected
2. Check: focused_component == Panel?
3. Call: state.sidebar_next_panel() or sidebar_next_item()
4. Update: sidebar_selected_panel or sidebar_selected_item
5. Re-render with new state → Visual feedback
```

## Advanced Features

### Nested Navigation
Some sections have subsections:

```
▼ Context  1.8k
  1,833 / 1,000,000 tokens
  LOADED FILES (8)          ← Subsection header
    📄 Sidebar.tsx          ← Can navigate here with j/k
    📄 styles/              ← And here
    📄 package.json         ← And here
```

### Smart Enter Behavior
- **On collapsed panel**: Expand it
- **On expanded panel header**: Enter into panel (select first item)
- **On expanded panel (with item selected)**: Could trigger action (open file, view task, etc.)

### Smart Escape Behavior
- **Inside panel** (item selected): Exit to panel header
- **At panel header**: Collapse panel
- **At collapsed panel**: No action (already collapsed)

## Customization

### Theme Selector in Header
Clicking or pressing Enter on "Theme: Catppuccin Mocha" opens the theme picker modal with:
- Live preview as you navigate
- Search/filter support
- All 7 themes available

### Keyboard Shortcuts Reference

| Context | Key | Action |
|---------|-----|--------|
| **Any** | `Ctrl+B` | Toggle sidebar |
| **Panel** | `Tab` | Cycle to next component |
| **Panel** | `j` / `↓` | Next panel/item |
| **Panel** | `k` / `↑` | Previous panel/item |
| **Panel** | `l` / `→` / `Enter` | Enter/expand |
| **Panel** | `h` / `←` / `-` / `Esc` | Exit/collapse |
| **Panel** | `Space` | Toggle panel |
| **Panel** | `g g` | Go to top |
| **Panel** | `G` | Go to bottom |

## Testing

Try the navigation:

```bash
cargo run --features test-sim -- tui

# In TUI:
Tab Tab          # Cycle to sidebar (Messages → Panel)
j j j           # Navigate through panels
Enter           # Enter into Context panel
j               # Navigate through files
k               # Navigate up
h               # Exit back to panel header
Space           # Collapse Context panel
j               # Move to Tasks panel
Enter           # Enter into tasks
```

## See Also
- `src/tui_new/widgets/sidebar.rs` - Sidebar implementation
- `src/tui_new/app.rs` - App state and event handling
- `docs/THEMES.md` - Theme system documentation
