# TUI Modal Design Guide

This guide ensures consistency across all modals in the new TUI (`tui_new`).

## Core Design Principles

### 1. Theme Integration

All modals MUST use the `Theme` struct for colors:

```rust
use crate::tui_new::theme::Theme;

pub struct MyModal<'a> {
    theme: &'a Theme,
    // ... other fields
}
```

### 2. Standard Modal Structure

```rust
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

impl Widget for MyModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // 1. Calculate centered area
        let modal_width = area.width.min(70);
        let modal_height = area.height.min(20);
        let modal_area = Rect {
            x: (area.width.saturating_sub(modal_width)) / 2,
            y: (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        // 2. Clear background
        Clear.render(modal_area, buf);

        // 3. Create block with ROUNDED borders
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)  // ✅ MUST use rounded borders
            .border_style(Style::default().fg(self.theme.cyan))
            .title(title_line)
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(self.theme.bg_dark));  // ✅ Use bg_dark

        // 4. Render content
        let inner = block.inner(modal_area);
        block.render(modal_area, buf);
        // ... render inner content
    }
}
```

## Color Guidelines

### Border Colors

- **Default modals**: `theme.cyan`
- **Warning/Caution**: `theme.yellow`
- **Error/Danger**: `theme.red`
- **Success/Confirmation**: `theme.green`

### Background Colors

- **Modal background**: `theme.bg_dark` (NOT `bg_main`)
- **Selected items**: `Color::Rgb(45, 60, 83)` (subtle highlight)

### Text Colors

- **Primary text**: `theme.text_primary`
- **Secondary text**: `theme.text_secondary`
- **Muted text**: `theme.text_muted`
- **Labels/hints**: `theme.text_muted`

### Accent Colors

- **Selected items**: `theme.cyan` with `Modifier::BOLD`
- **Success indicators**: `theme.green`
- **Warning indicators**: `theme.yellow`
- **Error indicators**: `theme.red`
- **Interactive elements**: `theme.purple`

## Title Formatting

Use `Line::from` with styled `Span`s for titles:

```rust
let title = Line::from(vec![
    Span::raw(" "),
    Span::styled(
        "Modal Title",
        Style::default()
            .fg(self.theme.text_primary)
            .add_modifier(Modifier::BOLD),
    ),
    Span::raw(" "),
]);
```

## Navigation Hints

Follow this pattern for keyboard hints:

```rust
Line::from(vec![
    Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
    Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
    Span::styled("Enter", Style::default().fg(self.theme.green)),
    Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
    Span::styled("Esc", Style::default().fg(self.theme.yellow)),
    Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
])
```

**Key points**:
- Keys are bold and colored
- Descriptions are muted
- Use spaces for visual separation
- No brackets around keys

## Selection Indicators

For list items:

```rust
let prefix = if is_selected { "▸ " } else { "  " };
let style = if is_selected {
    Style::default()
        .fg(self.theme.cyan)
        .add_modifier(Modifier::BOLD)
        .bg(Color::Rgb(45, 60, 83))
} else {
    Style::default().fg(self.theme.text_primary)
};
```

## Status Indicators

Use consistent icons:

- **Configured/Active**: `✓` in `theme.green`
- **Warning/Missing**: `⚠` in `theme.yellow`
- **Error/Failed**: `✗` in `theme.red`
- **Info**: `ℹ` in `theme.cyan`

## Search/Filter Input

```rust
Line::from(vec![
    Span::styled("Search: ", Style::default().fg(self.theme.text_muted)),
    Span::styled(
        if filter.is_empty() {
            "▏".to_string()
        } else {
            format!("{}▏", filter)
        },
        if filter.is_empty() {
            Style::default()
                .fg(self.theme.text_muted)
                .add_modifier(Modifier::DIM)
        } else {
            Style::default().fg(self.theme.text_primary)
        },
    ),
])
```

## Sizing Guidelines

### Modal Dimensions

- **Small modals**: 50% width, 40% height (alerts, confirmations)
- **Medium modals**: 60% width, 60% height (pickers, forms)
- **Large modals**: 70% width, 80% height (detailed views, help)

Use `.min()` to enforce maximum sizes:

```rust
let modal_width = area.width.min(70);  // Max 70 columns
let modal_height = area.height.min(20); // Max 20 rows
```

## Reference Examples

### Good Examples

1. **ProviderPickerModal** (`src/tui_new/widgets/modal.rs`)
   - ✅ Rounded borders
   - ✅ Theme colors throughout
   - ✅ Consistent navigation hints
   - ✅ Selection highlight

2. **ApprovalModal** (`src/tui_new/modals/approval_modal.rs`)
   - ✅ Risk-based border colors
   - ✅ Rounded borders
   - ✅ Clear action hints
   - ✅ Theme integration

3. **HelpModal** (`src/tui_new/widgets/modal.rs`)
   - ✅ Clear layout
   - ✅ Keyboard shortcuts formatting
   - ✅ Section organization

4. **SessionSwitchConfirmModal** (`src/tui_new/modals/session_switch_confirm.rs`)
   - ✅ Warning-style modal (yellow border)
   - ✅ Clear yes/no options
   - ✅ Explains consequences

5. **TaskEditModal** (`src/tui_new/modals/task_edit_modal.rs`)
   - ✅ Text editing within modal
   - ✅ Save/Cancel actions
   - ✅ Shows task context

6. **TrustModal** (`src/tui_new/modals/trust_modal.rs`)
   - ✅ Three-option selector
   - ✅ Clear descriptions for each level
   - ✅ Current selection highlight

## Checklist for New Modals

Before creating a new modal, ensure:

- [ ] Uses `Theme` for all colors
- [ ] Uses `border::ROUNDED` for borders
- [ ] Uses `bg_dark` for modal background
- [ ] Centers modal on screen
- [ ] Uses `Clear` to clear background
- [ ] Title is a styled `Line::from` with `Span`s
- [ ] Navigation hints follow standard format
- [ ] Selection indicators use `▸` prefix
- [ ] Status icons are consistent
- [ ] Text uses appropriate muted/primary/secondary colors
- [ ] Border color matches modal purpose (cyan/yellow/red/green)

## Anti-Patterns to Avoid

❌ **Don't use hardcoded colors**:
```rust
// BAD
.fg(Color::Red)

// GOOD
.fg(self.theme.red)
```

❌ **Don't use sharp borders**:
```rust
// BAD
.borders(Borders::ALL)

// GOOD
.borders(Borders::ALL)
.border_set(border::ROUNDED)
```

❌ **Don't use bg_main for modals**:
```rust
// BAD
.style(Style::default().bg(self.theme.bg_main))

// GOOD
.style(Style::default().bg(self.theme.bg_dark))
```

❌ **Don't wrap keys in brackets**:
```rust
// BAD (old style)
Span::raw("[Y] Approve")

// GOOD (new style)
Span::styled("Y", Style::default().fg(self.theme.green)),
Span::styled(" Approve", Style::default().fg(self.theme.text_muted)),
```

## Available Modal Types

The following modal types are defined in `src/ui_backend/state.rs`:

| Modal Type | Purpose | File Location |
|------------|---------|---------------|
| `ProviderPicker` | Select LLM provider | `widgets/modal.rs` |
| `ModelPicker` | Select model for provider | `widgets/modal.rs` |
| `SessionPicker` | Switch between sessions | `modals/session_picker.rs` |
| `FilePicker` | Add files to context | `widgets/modal.rs` |
| `ThemePicker` | Select UI theme | `widgets/modal.rs` |
| `Help` | Show keyboard shortcuts | `widgets/modal.rs` |
| `Approval` | Approve risky operations | `modals/approval_modal.rs` |
| `TrustLevel` | Set trust/approval level | `modals/trust_modal.rs` |
| `Tools` | View available tools | `modals/tools_modal.rs` |
| `Plugin` | Manage plugins | `modals/plugin_modal.rs` |
| `DeviceFlow` | OAuth device flow auth | `modals/device_flow_modal.rs` |
| `SessionSwitchConfirm` | Confirm session switch | `modals/session_switch_confirm.rs` |
| `TaskEdit` | Edit queued task | `modals/task_edit_modal.rs` |
| `TaskDeleteConfirm` | Confirm task deletion | `modals/task_edit_modal.rs` |

## Theme Support

All modals automatically support theme switching via the `Theme` struct. When users change themes:

- Colors are automatically updated
- No modal-specific code needed
- Consistent look across all themes

Available theme presets:
- Catppuccin Mocha (default)
- Nord
- GitHub Dark
- One Dark
- Gruvbox Dark
- Tokyo Night

All themes are defined in `src/tui_new/theme.rs`.
