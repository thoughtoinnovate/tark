# Instructions for AI Agent: Implementing TUI with Ratatui

## Mission

You are to implement a Terminal User Interface (TUI) in Rust using the Ratatui framework, based on the React/TypeScript web application documented in this codebase. This TUI should replicate the functionality, appearance, and behavior of the original web UI as closely as possible within the constraints of terminal-based interfaces.

> **Note**: Both the **agent name** and **user name** are **configurable**. See `src/app/config/appConfig.ts` for customization.
> - Agent name: Default is "Innodrupe" but can be changed to any name (e.g., "CodePilot", "DevAssist", etc.)
> - User name: **Auto-detected** from `$USER` or `$USERNAME` environment variable in Ratatui. Falls back to "You" if not detected.

---

## What CAN vs CANNOT Be Achieved in Terminal TUI

### ✅ Fully Achievable

| Web Feature | Ratatui Equivalent | Notes |
|-------------|-------------------|-------|
| **Colors** | `Color::Rgb(r, g, b)` | Exact color matching with true color terminals (24-bit) |
| **Layout (Flexbox)** | `Layout::horizontal/vertical()` with `Constraint` | Constraint-based, similar to flexbox |
| **Icons** | Unicode chars or Nerd Fonts | 🧠 `Brain`, ● dots, ▼ chevrons, etc. |
| **Borders** | Box-drawing characters | `╭─╮` rounded, `┌─┐` square, `╔═╗` double |
| **Theming** | Theme struct with colors | Full theme switching support |
| **Scrolling** | `ScrollbarState` | Native scrollbar widget |
| **Focus states** | `Modifier::REVERSED` or bg color | Visual focus indication |
| **Text styling** | `Style::bold()`, `italic()`, `underlined()` | Rich text formatting |
| **Keyboard input** | `crossterm::event` | Full keyboard support |
| **Clipboard** | `arboard` crate | Copy/paste support |
| **Status bar** | Fixed `Constraint::Length(2)` | Bottom bar with indicators |
| **Modals/Popups** | `Clear` + overlay render | Floating windows |

### ⚠️ Achievable with Limitations

| Web Feature | Limitation | Alternative |
|-------------|-----------|-------------|
| **Hover effects** | No mouse hover detection | Use selection/focus highlight |
| **Tooltips** | No floating hints | Use status bar or inline help |
| **Animations** | No smooth transitions | Frame-by-frame spinner characters |
| **Opacity/Alpha** | No transparency | Use dimmer color variants |
| **Custom fonts** | User's terminal font only | Recommend Nerd Fonts |
| **Input caret** | Block cursor only | `▏` or `│` character |

### ❌ Not Possible in TUI

| Web Feature | Why Not | Workaround |
|-------------|---------|------------|
| **Rounded corners on elements** | Character grid only | Use `╭╮╰╯` box chars for borders |
| **Drop shadows** | No layered rendering | Use double-line borders for emphasis |
| **Gradients** | Discrete characters | Use color bands or omit |
| **SVG icons** | No vector graphics | Unicode/Nerd Font glyphs |
| **Smooth scrolling** | Character-based jump | Instant scroll (still looks good) |
| **Background images** | No bitmap support | Use colored backgrounds |
| **Variable font sizes** | Monospace grid | Bold/dim for emphasis |

---

## Visual Comparison Guide

### Web UI → TUI Translation Examples

**Status Bar:**
```
Web:  [⚡] Claude 3.5 Sonnet [ANTHROPIC ▼]  [Build Mode ▼]  ● Connected

TUI:  │ ⚡ Claude 3.5 Sonnet │ ANTHROPIC ▼ │ Build Mode ▼ │ ● Connected │
```

**Message with icon:**
```
Web:  🧠 [thinking block with purple glow and border]

TUI:  │ 🧠 │ The agent is analyzing the request...
      │    │ Considering: file structure, imports, dependencies
```

**Accordion Panel:**
```
Web:  ▼ Session  ←[chevron icon + text + badge]
        content expanded with smooth animation

TUI:  ▼  Session ─────────────────────
         main • gemini-1.5-pro
        ⎇ feature/update
```

**Context file tag:**
```
Web:  [📄 Terminal.tsx ×]  ← blue pill with icon and close button

TUI:   📄 Terminal.tsx   ← colored text, use `x` key to remove
```

**Question modal:**
```
Web:  ╭─────────────────────────────────────╮
      │ Select an option:                   │
      │ ○ Option A                          │
      │ ◉ Option B  ← selected              │
      │ ○ Option C                          │
      │                     [Cancel] [OK]   │
      ╰─────────────────────────────────────╯

TUI:  (Exactly the same! Box chars + Unicode radio buttons)
```

---

## Required Terminal Capabilities

For best results, inform users:

```
Recommended terminal requirements:
• True color support (24-bit) - Most modern terminals support this
• Unicode support - Required for icons
• Nerd Font installed - Optional but recommended for rich icons
• Minimum 80x24 characters - 120x40 recommended

Tested terminals:
✅ iTerm2 (macOS)
✅ Windows Terminal
✅ Alacritty
✅ Kitty
✅ WezTerm
✅ GNOME Terminal
⚠️ macOS Terminal.app (limited true color)
❌ cmd.exe (use Windows Terminal instead)
```

---

## Documentation Structure

This codebase contains comprehensive documentation to guide your implementation:

### 1. **Source Files with Inline Comments**
   - `src/app/App.tsx` - Root application structure and layout
   - `src/app/components/Terminal.tsx` - Terminal interface with message types
   - `src/app/components/Sidebar.tsx` - Sidebar with accordion panels

   **Each file contains:**
   - `@ratatui-widget:` - Which Ratatui widget to use
   - `@ratatui-layout:` - Layout constraints and sizing
   - `@ratatui-style:` - Color and styling information
   - `@ratatui-state:` - Required state fields in Rust
   - `@ratatui-events:` - Keyboard/event handlers needed
   - `@ratatui-behavior:` - How the element should behave
   - `@ratatui-pattern:` - Code examples in Rust

### 2. **Comprehensive Mapping Guide**
   - `RATATUI_MAPPING.md` - Complete implementation reference
   
   **This guide contains:**
   - Layout architecture diagrams
   - Widget mapping tables
   - Complete color palette (RGB values)
   - Full Rust state structure definitions
   - Keyboard navigation mappings
   - Code examples for each component
   - Implementation checklist

---

## Implementation Approach

### Step 1: Read and Understand
1. **Start by reading** `RATATUI_MAPPING.md` in full to understand:
   - Overall architecture
   - State management patterns
   - Event handling approach
   - Color scheme

2. **Then review** the annotated source files:
   - Read all `@ratatui-*` comments
   - Study the Rust code snippets provided
   - Understand the React → Ratatui mappings

### Step 2: Project Setup
Create a new Rust project with the dependencies specified in `RATATUI_MAPPING.md`:

```bash
cargo new innodrupe-tui
cd innodrupe-tui
```

Add to `Cargo.toml`:
```toml
[dependencies]
ratatui = "0.26"
crossterm = "0.27"
arboard = "3.3"
unicode-width = "0.1"
textwrap = "0.16"
```

### Step 3: Implement in Order

Follow the **Implementation Checklist** in `RATATUI_MAPPING.md`:

#### Phase 1: Foundation (Priority: Critical)
1. Create `main.rs` with terminal setup/teardown
2. Implement main event loop
3. Create `App` struct with all state fields from the mapping guide
4. Implement basic layout split (Terminal + Sidebar)

**Key Files to Reference:**
- `App.tsx` - See the root layout comments
- `RATATUI_MAPPING.md` - Section "State Management" for the complete `App` struct

#### Phase 2: Terminal Component (Priority: High)
1. Implement terminal header
2. Create message rendering system for all 5 types:
   - System messages (cyan, with dot icon)
   - User input messages (gray, with user icon)
   - Bot output messages (gray, with bot icon)
   - Tool messages (gray, with wrench icon, expandable)
   - Command messages (emerald prompt)
3. Add scrollable output area with scrollbar
4. Implement status bar with mode selectors
5. Create input area with cursor positioning

**Key Files to Reference:**
- `Terminal.tsx` - Extensively annotated with `@ratatui-*` comments
- `RATATUI_MAPPING.md` - Section "Complete Terminal Rendering" for full code

#### Phase 3: Sidebar Component (Priority: High)
1. Implement sidebar header with collapse button
2. Create scrollable panels area
3. Implement 4 accordion sections:
   - Session (with cost breakdown expansion)
   - Context (with loaded files expansion)
   - Tasks (with CRUD operations)
   - Git Changes (with diff stats)
4. Add panel expansion/collapse logic

**Key Files to Reference:**
- `Sidebar.tsx` - Detailed comments on each panel
- `RATATUI_MAPPING.md` - Section "Sidebar Rendering" for code examples

#### Phase 4: Interactions (Priority: Medium)
1. Implement keyboard navigation (see keyboard mapping tables)
2. Add mode selector dropdown (Build/Plan/Ask)
3. Add build mode selector (Careful/Manual/Balanced) when in Build mode
4. Implement task editing with text input popup
5. Add task deletion with confirmation dialog
6. Implement task reordering (Ctrl+Up/Down)
7. Add context file management (add/remove)

**Key Files to Reference:**
- `RATATUI_MAPPING.md` - Section "Event Handling" for complete key mappings
- All source files - Look for `@ratatui-keyboard:` and `@ratatui-handler:` comments

#### Phase 5: Polish (Priority: Low)
1. Apply exact color theme (use RGB values from mapping guide)
2. Add Unicode icons (see icon mapping table)
3. Implement word wrapping for long text
4. Add loading spinner for active task
5. Implement clipboard integration
6. Add copy notification feedback
7. Optimize scrolling performance

---

## Critical Guidelines

### 1. **Color Accuracy**
- Use **exact RGB values** from `RATATUI_MAPPING.md` Section "Color Palette"
- Don't guess colors - they're all documented
- Apply colors via `Style::default().fg(Color::Rgb(r, g, b))`

### 2. **Layout Precision**
- Follow **exact constraints** documented in layout sections
- Terminal header: `Constraint::Length(3)`
- Output area: `Constraint::Min(0)` (takes remaining)
- Status bar: `Constraint::Length(2)`
- Input area: `Constraint::Length(3)` or `5` with context files

### 3. **State Management**
- Use the **complete `App` struct** from `RATATUI_MAPPING.md`
- Don't create your own state structure - use the documented one
- All React `useState` hooks map to struct fields

### 4. **Event Handling**
- Implement **all keyboard shortcuts** from the mapping tables
- Global keys: Ctrl+C/Q (quit), Tab (focus switch), Ctrl+T (thinking toggle)
- Terminal keys: Char input, Backspace, Arrow keys, Enter
- Sidebar keys: j/k (navigate), Enter (toggle), e (edit), x (delete)

### 5. **Message Types**
Implement all 7 message types with their specific styling:

| Type | Icon | Color | Special |
|------|------|-------|---------|
| System | ● | Cyan | Single line |
| Input | 👤 | Gray-100 | User messages |
| Output | 🤖 | Gray-200 | Bot responses, copyable |
| Tool | 🔧 | Gray-200 | Expandable details |
| Thinking | 🧠⚡ (BrainCircuit) | Gray-400 | Agent reasoning, italic, dashed border |
| Question | ❓ | Purple-400 | Interactive - see question types below |
| Command | $ | Emerald | Shell prompt style |

### 5b. **Question Types (Sub-variants of Question)**
Questions are interactive and come in six flavors:

| Question Type | Icons | Behavior | Keyboard |
|--------------|-------|----------|----------|
| Single Choice | ○ → ◉ | Radio buttons (select ONE) | Arrows, Space, a-z shortcuts |
| Multi Choice | ☐ → ☑ | Checkboxes (select MULTIPLE) | Arrows, Space to toggle, a-z |
| Free Text | ▸ cursor | Text input field | Type text, Enter to submit |
| Provider Picker | ● ⚠ ✖ ? | LLM providers with status | Type to filter, Arrows, Enter |
| Model Picker | ● indicator | Models with capabilities | Type to filter, Arrows, Enter |
| File Picker | 📁 📄 | File tree with indentation | Arrows, Enter to select |

**Question Rendering Notes:**
- Basic questions have purple theme (#c084fc border/accent)
- Show letter shortcuts: a) Option one, b) Option two, etc.
- Single choice: selecting new option deselects previous
- Multi choice: show "X selected" count

### 5c. **System Modals (NOT Questions - User-Triggered UI)**

**IMPORTANT: System modals are SEPARATE from agent questions!**

System modals are **popup/modal overlays** triggered by **user actions**, not agent prompts:

| Modal | Trigger | Purpose |
|-------|---------|---------|
| Provider Picker | Click model selector button in status bar OR type `/model` | Select AI provider |
| Model Picker | **Automatically shown after provider is selected** | Select model from chosen provider |
| File Picker | Type `@` in input (immediately) OR click `+` button | Add context file (@ mentions) |

**Flow for Provider/Model Selection:**
1. User clicks model selector button (shows "Claude 3.5 Sonnet ANTHROPIC") OR types `/model`
2. Provider Picker modal appears
3. User selects a provider (e.g., "OpenAI")
4. Model Picker modal **automatically** appears showing models for that provider
5. User selects a model
6. Modal closes, status bar updates with new selection

**Flow for File Picker:**
1. User types `@` in input area → File Picker **immediately** appears
2. OR user clicks the `+` button → File Picker appears
3. User navigates and selects file
4. File path added as context tag in input area

**Display Pattern:**
- Centered floating window with dimmed background
- Modal captures keyboard focus
- `Escape` to close, `Enter` to confirm
- `Up/Down` arrows to navigate list
- Type to filter options

**Ratatui System Modal Pattern:**
```rust
enum SystemModal {
    None,
    ProviderPicker,
    ModelPicker,
    FilePicker,
}

struct AppState {
    active_modal: SystemModal,
    modal_filter: String,
    modal_selected_idx: usize,
    selected_provider: Option<String>,  // Track for chaining to model picker
}

fn render(&self, frame: &mut Frame) {
    // 1. Render main terminal UI
    self.render_terminal(frame);
    
    // 2. If modal is active, render overlay on top
    if self.active_modal != SystemModal::None {
        self.render_modal_overlay(frame);
    }
}

// Called when input text changes
fn handle_input_change(&mut self, input: &str, prev_input: &str) {
    // "@" typed → IMMEDIATELY show file picker
    if input.ends_with('@') && !prev_input.ends_with('@') {
        self.active_modal = SystemModal::FilePicker;
    }
    // "/model" command → show provider picker
    if input.trim() == "/model" {
        self.active_modal = SystemModal::ProviderPicker;
        self.input.clear();
    }
}

// Click handlers for UI buttons
fn handle_plus_button_click(&mut self) {
    self.active_modal = SystemModal::FilePicker;  // Same as typing "@"
}

fn handle_model_selector_click(&mut self) {
    self.active_modal = SystemModal::ProviderPicker;  // Same as "/model"
}

// Confirm selection - handles chaining provider → model
fn confirm_modal_selection(&mut self) {
    match self.active_modal {
        SystemModal::ProviderPicker => {
            // Save provider, then AUTOMATICALLY show model picker
            self.selected_provider = Some(self.get_selected_provider_id());
            self.active_modal = SystemModal::ModelPicker;  // ← Chain!
            self.modal_filter.clear();
            self.modal_selected_idx = 0;
        }
        SystemModal::ModelPicker => {
            self.selected_model = Some(self.get_selected_model_id());
            self.active_modal = SystemModal::None;
        }
        SystemModal::FilePicker => {
            self.add_context_file(self.get_selected_file_path());
            self.active_modal = SystemModal::None;
        }
        _ => {}
    }
    self.modal_filter.clear();
}

fn handle_key(&mut self, key: KeyEvent) {
    // If modal active, handle modal keys first
    if self.active_modal != SystemModal::None {
        match key.code {
            KeyCode::Esc => {
                self.active_modal = SystemModal::None;
                self.modal_filter.clear();
            }
            KeyCode::Enter => self.confirm_modal_selection(),
            KeyCode::Up => self.modal_selected_idx = self.modal_selected_idx.saturating_sub(1),
            KeyCode::Down => self.modal_selected_idx += 1,
            KeyCode::Char(c) => self.modal_filter.push(c),
            KeyCode::Backspace => { self.modal_filter.pop(); }
            _ => {}
        }
        return;  // Don't process other keys when modal open
    }
    // Normal key handling...
}
```

**Provider Picker:**
- Title: "Select Provider"
- Filter: `> Type to filter...`
- Items: `[status] [icon] Name - Description`
- Status icons: ● (green), ⚠ (yellow), ✖ (red), ? (gray)
- Highlight: ▶ indicator

**Model Picker:**
- Title: "Select Model"
- Filter: `> Type to filter...`
- Items: `● Model Name - capabilities`
- Latest models in yellow
- Capabilities: gray italic text

**File Picker (@ Mentions):**
- Title: "Select File"
- Tree view with indentation
- Icons: 📁 folder (cyan), 📄 file (white)
- Footer: `[INSERT] @█` cursor
- Result: File added to context tags in input

### 5d. **Agent Question Dialogs (Modal)**

When the **agent** needs user input, it asks questions that appear as modals:

| Question Type | UI | Behavior |
|--------------|-----|----------|
| Single Choice | Radio buttons ○ → ◉ | Select ONE option |
| Multi Choice | Checkboxes ☐ → ☑ | Select MULTIPLE options |
| Free Text | Text input with cursor | Type response |

**Question modals have purple theme** (distinct from system modals)

```rust
// Questions are part of terminal output, but DISPLAY as modals
struct TerminalLine {
    line_type: LineType,
    question_type: Option<QuestionType>,  // Only for LineType::Question
    // ...
}

// When rendering a question line, show as modal if unanswered
fn render_question(&self, frame: &mut Frame, question: &TerminalLine) {
    if !question.answered {
        self.render_question_modal(frame, question);
    } else {
        // Show inline "Answered: X" in chat
    }
}
```
- Free text: show placeholder when empty, cursor when typing
- Answered state: show green checkmark with answer

### 6. **Icons**
- Use **Unicode characters** from the icon mapping table in `RATATUI_MAPPING.md`
- Examples: "●" for circle, "▼" for chevron down, "📄" for file
- Or use nerd fonts if preferred (nerd font codes also provided)

### 7. **No Animations**
- TUI doesn't support smooth animations
- All state changes should render **instantly**
- Use rotating characters for loading spinner: "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
- Don't try to implement fade-ins or transitions

---

## Code Structure

Organize your Rust project as follows:

```
src/
├── main.rs              # Entry point, terminal setup, main loop
├── app.rs               # App struct and main state management
├── ui/
│   ├── mod.rs           # UI module exports
│   ├── terminal.rs      # Terminal rendering functions
│   ├── sidebar.rs       # Sidebar rendering functions
│   ├── messages.rs      # Message type rendering
│   └── popups.rs        # Popup dialogs (mode selector, etc.)
├── event.rs             # Event handling logic
├── state/
│   ├── mod.rs           # State module exports
│   └── types.rs         # Type definitions (enums, structs)
└── theme.rs             # Color constants and theme
```

---

## Testing Your Implementation

### Visual Comparison Checklist

Compare your TUI against the React UI screenshots/descriptions:

- [ ] Layout split is 70/30 (Terminal/Sidebar) when expanded
- [ ] Sidebar collapses to ~3-5 columns wide
- [ ] Terminal header shows "INNODRUPE TERMINAL" and path
- [ ] All 5 message types render with correct icons and colors
- [ ] Status bar shows mode selector with colored circle
- [ ] Build mode shows when in Build mode
- [ ] Input area has "▶" prompt character
- [ ] Context files show as blue pills with file icon
- [ ] Sidebar has 4 sections: Session, Context, Tasks, Git
- [ ] Active task shows loading spinner (rotating animation)
- [ ] Queued tasks are editable, deletable, reorderable
- [ ] Git changes show colored status (yellow/emerald/red)
- [ ] All keyboard shortcuts work as documented

### Functional Testing

- [ ] Type in input field, press Enter to submit
- [ ] Switch between Build/Plan/Ask modes
- [ ] Change build mode with Ctrl+1/2/3
- [ ] Scroll output area with Up/Down or j/k
- [ ] Toggle sidebar sections with Enter
- [ ] Collapse/expand entire sidebar with h/l
- [ ] Edit task with 'e' key
- [ ] Delete task with 'x' or Delete key
- [ ] Move tasks with Ctrl+Up/Down
- [ ] Add/remove context files
- [ ] Tab to switch focus between Terminal and Sidebar
- [ ] Quit with Ctrl+C or Ctrl+Q

---

## Common Pitfalls to Avoid

### ❌ DON'T:
1. Create your own state structure - use the documented `App` struct
2. Guess colors - use the exact RGB values provided
3. Skip keyboard shortcuts - implement all of them
4. Try to add animations - they don't work in TUI
5. Use different icons - stick to the Unicode characters provided
6. Change the layout constraints - use the exact values documented
7. Implement only some message types - all 5 are required
8. Forget scrollbars - output and sidebar need them
9. Ignore the implementation checklist - follow it in order
10. Skip the inline comments - they contain critical implementation details

### ✅ DO:
1. Read ALL documentation before starting
2. Use the exact `App` struct from `RATATUI_MAPPING.md`
3. Follow the implementation checklist in order
4. Test each phase before moving to the next
5. Use the provided Rust code examples as templates
6. Match colors exactly using RGB values
7. Implement all keyboard shortcuts
8. Add scrollbars to scrollable areas
9. Handle all 5 message types
10. Test focus management (Tab switching)

---

## Reference Quick Links

### For Layout Questions:
- `RATATUI_MAPPING.md` → Section "Layout Architecture"
- `App.tsx` → Look for `@ratatui-layout:` comments

### For Styling Questions:
- `RATATUI_MAPPING.md` → Section "Color Palette"
- All source files → Look for `@ratatui-style:` comments

### For State Questions:
- `RATATUI_MAPPING.md` → Section "State Management"
- All source files → Look for `@ratatui-state:` comments

### For Event Handling Questions:
- `RATATUI_MAPPING.md` → Section "Event Handling"
- All source files → Look for `@ratatui-events:` and `@ratatui-keyboard:` comments

### For Widget Choice Questions:
- `RATATUI_MAPPING.md` → Section "Widget Mapping Table"
- All source files → Look for `@ratatui-widget:` comments

### For Code Examples:
- `RATATUI_MAPPING.md` → Section "Code Examples"
- All source files → Look for `@ratatui-pattern:` code blocks

---

## Success Criteria

Your implementation is complete when:

1. ✅ **Visual Parity**: The TUI looks like the React UI (within TUI limitations)
2. ✅ **Functional Parity**: All features work as documented
3. ✅ **Keyboard Navigation**: All shortcuts from the mapping tables work
4. ✅ **State Management**: Uses the documented `App` struct
5. ✅ **Color Accuracy**: Colors match the RGB values exactly
6. ✅ **Performance**: Smooth scrolling, no lag on input
7. ✅ **Code Quality**: Well-organized, follows Rust best practices
8. ✅ **Completeness**: All 5 message types, 4 sidebar panels, all interactions

---

## Getting Help

If you encounter ambiguity or questions:

1. **First**: Search for `@ratatui-*` comments in the relevant source file
2. **Second**: Check the corresponding section in `RATATUI_MAPPING.md`
3. **Third**: Look for similar code examples in the documentation
4. **Last Resort**: Make a reasonable decision based on Ratatui best practices, document your choice

---

## Ratatui Best Practices & Recommendations

### 1. Terminal Capabilities Detection

**Always detect terminal capabilities at startup:**

```rust
use crossterm::terminal;

fn detect_capabilities() -> TerminalCaps {
    TerminalCaps {
        // True color (24-bit) support
        true_color: std::env::var("COLORTERM")
            .map(|v| v == "truecolor" || v == "24bit")
            .unwrap_or(false),
        
        // Terminal size for responsive layouts
        size: terminal::size().unwrap_or((80, 24)),
        
        // Unicode support (most modern terminals)
        unicode: true,
    }
}

// Fallback color palette for 256-color terminals
fn get_fallback_color(rgb: Color) -> Color {
    // Map RGB to closest 256-color palette entry
    match rgb {
        Color::Rgb(30, 30, 46) => Color::Indexed(235),  // Base
        Color::Rgb(148, 226, 213) => Color::Indexed(115), // Teal
        _ => Color::White,
    }
}
```

### 2. Font Recommendations

**Recommend Nerd Fonts for best icon experience:**

Display this on first run or in docs:
```
╭─────────────────────────────────────────────────────────╮
│  For best experience, use a Nerd Font:                  │
│                                                         │
│  Recommended fonts:                                     │
│  • JetBrainsMono Nerd Font                             │
│  • FiraCode Nerd Font                                   │
│  • CaskaydiaCove Nerd Font (Cascadia Code)             │
│  • Hack Nerd Font                                       │
│                                                         │
│  Download: https://www.nerdfonts.com/                   │
│                                                         │
│  Without Nerd Fonts, Unicode fallbacks will be used.   │
╰─────────────────────────────────────────────────────────╯
```

### 3. Icon Mapping (Unicode ↔ Nerd Font)

**Comprehensive icon mapping with fallbacks:**

```rust
pub struct Icons {
    // Use Nerd Font if available, Unicode fallback otherwise
    pub brain: &'static str,
    pub user: &'static str,
    pub bot: &'static str,
    pub check: &'static str,
    // ... etc
}

impl Icons {
    pub fn nerd_font() -> Self {
        Icons {
            brain: "\u{f0962}",      // 󰥢 nf-md-brain
            user: "\u{f007}",        //  nf-fa-user
            bot: "\u{f06a9}",        // 󰚩 nf-md-robot
            check: "\u{f00c}",       //  nf-fa-check
            circle: "\u{f111}",      //  nf-fa-circle
            circle_o: "\u{f10c}",    //  nf-fa-circle_o
            folder: "\u{f07b}",      //  nf-fa-folder
            folder_open: "\u{f07c}", //  nf-fa-folder_open
            file: "\u{f15b}",        //  nf-fa-file
            file_code: "\u{f1c9}",   //  nf-fa-file_code_o
            git_branch: "\u{e725}",  //  nf-dev-git_branch
            terminal: "\u{f120}",    //  nf-fa-terminal
            cog: "\u{f013}",         //  nf-fa-cog
            wrench: "\u{f0ad}",      //  nf-fa-wrench
            question: "\u{f128}",    //  nf-fa-question
            chevron_right: "\u{f054}", //  nf-fa-chevron_right
            chevron_down: "\u{f078}",  //  nf-fa-chevron_down
            spinner: ["", "", "", "", "", "", ""], // Nerd font spinners
        }
    }
    
    pub fn unicode_fallback() -> Self {
        Icons {
            brain: "🧠",
            user: "👤",
            bot: "🤖",
            check: "✓",
            circle: "●",
            circle_o: "○",
            folder: "📁",
            folder_open: "📂",
            file: "📄",
            file_code: "📄",
            git_branch: "⎇",
            terminal: "⬚",
            cog: "⚙",
            wrench: "🔧",
            question: "❓",
            chevron_right: "▶",
            chevron_down: "▼",
            spinner: ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        }
    }
}
```

### 4. Color Theme Implementation

**Implement full theming system:**

```rust
pub struct Theme {
    pub name: &'static str,
    pub is_dark: bool,
    
    // Base colors
    pub background: Color,
    pub foreground: Color,
    pub border: Color,
    
    // Terminal specific
    pub terminal_bg: Color,
    pub terminal_header_bg: Color,
    pub status_bar_bg: Color,
    
    // Semantic colors
    pub thinking_active: Color,
    pub thinking_inactive: Color,
    pub llm_connected: Color,
    pub llm_error: Color,
    
    // Message colors
    pub msg_system: Color,
    pub msg_user: Color,
    pub msg_agent: Color,
    pub msg_tool: Color,
    pub msg_thinking: Color,
    pub msg_question: Color,
    pub msg_command: Color,
    
    // Git colors
    pub git_modified: Color,
    pub git_new: Color,
    pub git_deleted: Color,
    
    // Context/accent
    pub context_bg: Color,
    pub context_text: Color,
    pub accent: Color,
}

// Catppuccin Mocha (default dark theme)
pub const CATPPUCCIN_MOCHA: Theme = Theme {
    name: "Catppuccin Mocha",
    is_dark: true,
    
    background: Color::Rgb(30, 30, 46),       // #1e1e2e
    foreground: Color::Rgb(205, 214, 244),    // #cdd6f4
    border: Color::Rgb(69, 71, 90),           // #45475a
    
    terminal_bg: Color::Rgb(30, 30, 46),
    terminal_header_bg: Color::Rgb(24, 24, 37),
    status_bar_bg: Color::Rgb(24, 24, 37),
    
    thinking_active: Color::Rgb(249, 226, 175),  // #f9e2af (Yellow)
    thinking_inactive: Color::Rgb(147, 153, 178), // #9399b2
    llm_connected: Color::Rgb(166, 227, 161),    // #a6e3a1 (Green)
    llm_error: Color::Rgb(243, 139, 168),        // #f38ba8 (Red)
    
    msg_system: Color::Rgb(148, 226, 213),    // #94e2d5 (Teal)
    msg_user: Color::Rgb(186, 194, 222),      // #bac2de
    msg_agent: Color::Rgb(205, 214, 244),     // #cdd6f4
    msg_tool: Color::Rgb(166, 173, 200),      // #a6adc8
    msg_thinking: Color::Rgb(147, 153, 178),  // #9399b2
    msg_question: Color::Rgb(137, 220, 235),  // #89dceb (Sky)
    msg_command: Color::Rgb(166, 227, 161),   // #a6e3a1 (Green)
    
    git_modified: Color::Rgb(249, 226, 175),  // Yellow
    git_new: Color::Rgb(166, 227, 161),       // Green
    git_deleted: Color::Rgb(243, 139, 168),   // Red
    
    context_bg: Color::Rgb(137, 180, 250),    // #89b4fa (Blue) @ 10%
    context_text: Color::Rgb(137, 180, 250),  // #89b4fa (Blue)
    accent: Color::Rgb(203, 166, 247),        // #cba6f7 (Mauve)
};

// Add more themes: Nord, One Dark, GitHub Dark, etc.
```

### 5. Box Drawing Characters Reference

**Use these for beautiful TUI borders:**

```rust
// Standard box drawing
pub const BOX_LIGHT: BoxChars = BoxChars {
    top_left: '┌',
    top_right: '┐',
    bottom_left: '└',
    bottom_right: '┘',
    horizontal: '─',
    vertical: '│',
    t_down: '┬',
    t_up: '┴',
    t_right: '├',
    t_left: '┤',
    cross: '┼',
};

// Rounded corners (recommended for modern look)
pub const BOX_ROUNDED: BoxChars = BoxChars {
    top_left: '╭',
    top_right: '╮',
    bottom_left: '╰',
    bottom_right: '╯',
    horizontal: '─',
    vertical: '│',
    t_down: '┬',
    t_up: '┴',
    t_right: '├',
    t_left: '┤',
    cross: '┼',
};

// Double line (for emphasis)
pub const BOX_DOUBLE: BoxChars = BoxChars {
    top_left: '╔',
    top_right: '╗',
    bottom_left: '╚',
    bottom_right: '╝',
    horizontal: '═',
    vertical: '║',
    // ...
};

// Heavy borders
pub const BOX_HEAVY: BoxChars = BoxChars {
    top_left: '┏',
    top_right: '┓',
    bottom_left: '┗',
    bottom_right: '┛',
    horizontal: '━',
    vertical: '┃',
    // ...
};
```

**Example: Panel with rounded corners:**
```
╭─ Panel ──────────────────╮
│ ▼  Session              │
│    main                │
│    feature/update     │
╰──────────────────────────╯
```

### 6. Performance Optimizations

**Critical for smooth TUI experience:**

```rust
// 1. Use double buffering (Ratatui handles this)
// 2. Only redraw when state changes
fn should_redraw(&self, prev_state: &AppState) -> bool {
    self.input != prev_state.input ||
    self.scroll_offset != prev_state.scroll_offset ||
    self.active_panel != prev_state.active_panel
    // ... other state comparisons
}

// 3. Debounce spinner animation (every 100ms, not every frame)
fn update_spinner(&mut self) {
    let now = Instant::now();
    if now.duration_since(self.last_spinner_update) >= Duration::from_millis(100) {
        self.spinner_frame = (self.spinner_frame + 1) % self.icons.spinner.len();
        self.last_spinner_update = now;
    }
}

// 4. Pre-compute styled spans for static content
lazy_static! {
    static ref HEADER_STYLE: Style = Style::default()
        .fg(THEME.foreground)
        .add_modifier(Modifier::BOLD);
}

// 5. Use efficient text wrapping
use textwrap::{wrap, Options};

fn wrap_message(text: &str, width: usize) -> Vec<&str> {
    wrap(text, Options::new(width).break_words(false))
        .into_iter()
        .map(|cow| cow.as_ref())
        .collect()
}
```

### 7. Responsive Layout Patterns

**Adapt to terminal size:**

```rust
fn get_layout(&self, area: Rect) -> Layout {
    // Responsive sidebar width
    let sidebar_width = if area.width < 100 {
        // Narrow terminal: smaller sidebar
        Constraint::Length(25)
    } else if area.width < 150 {
        // Medium: standard sidebar
        Constraint::Length(35)
    } else {
        // Wide: larger sidebar
        Constraint::Percentage(25)
    };
    
    Layout::horizontal([
        Constraint::Min(40),  // Terminal minimum
        sidebar_width,
    ])
}

// Hide sidebar on very narrow terminals
fn should_show_sidebar(&self) -> bool {
    self.terminal_size.0 >= 80  // At least 80 columns
}
```

### 8. Clipboard Integration

**Cross-platform clipboard support:**

```rust
use arboard::Clipboard;

fn copy_to_clipboard(text: &str) -> Result<(), String> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("Failed to access clipboard: {}", e))?;
    
    clipboard.set_text(text)
        .map_err(|e| format!("Failed to copy: {}", e))?;
    
    Ok(())
}

// Show feedback after copy
fn handle_copy(&mut self, content: &str) {
    if let Ok(()) = copy_to_clipboard(content) {
        self.show_notification("Copied to clipboard!", Duration::from_secs(2));
    }
}
```

### 9. Graceful Degradation

**Always have fallbacks:**

```rust
// Color fallback
fn safe_color(preferred: Color, fallback: Color, supports_true_color: bool) -> Color {
    if supports_true_color {
        preferred
    } else {
        fallback
    }
}

// Icon fallback  
fn safe_icon(nerd: &str, unicode: &str, supports_nerd_fonts: bool) -> &str {
    if supports_nerd_fonts {
        nerd
    } else {
        unicode
    }
}

// Feature detection with graceful degradation
impl App {
    fn new() -> Self {
        let caps = detect_capabilities();
        
        Self {
            icons: if caps.supports_nerd_fonts {
                Icons::nerd_font()
            } else {
                Icons::unicode_fallback()
            },
            theme: if caps.true_color {
                CATPPUCCIN_MOCHA
            } else {
                CATPPUCCIN_MOCHA_256  // 256-color fallback
            },
            // ...
        }
    }
}
```

### 10. Status Indicators Reference

**Common status patterns:**

```rust
// Connection status
fn render_status_dot(&self, status: ConnectionStatus) -> Span {
    match status {
        ConnectionStatus::Connected => 
            Span::styled("●", Style::default().fg(self.theme.llm_connected)),
        ConnectionStatus::Connecting => 
            Span::styled("○", Style::default().fg(self.theme.thinking_active)),
        ConnectionStatus::Error => 
            Span::styled("●", Style::default().fg(self.theme.llm_error)),
        ConnectionStatus::Unknown => 
            Span::styled("?", Style::default().fg(self.theme.foreground)),
    }
}

// Working indicator (animated)
fn render_working_indicator(&self) -> Line {
    Line::from(vec![
        Span::styled("●", Style::default().fg(self.theme.llm_connected)),
        Span::raw(" "),
        Span::styled("Working", Style::default().fg(self.theme.foreground)),
        Span::styled(
            &self.icons.spinner[self.spinner_frame],
            Style::default().fg(self.theme.llm_connected)
        ),
    ])
}
```

### 11. Example: Complete Panel Rendering

```
╭─ Panel ──────────────────────╮
│                              │
│ ▼  Session                  │
│    main                    │
│   ⎇ feature/sidebar-update  │
│   ✨ gemini-1.5-pro-preview  │
│   ☁ gemini-oauth             │
│   💰 $0.015 (3 models)       │
│                              │
│ ▼  Context           1.0k   │
│    1,833 / 1,000,000 tokens │
│   ▼ Loaded Files (8)         │
│      📄 Sidebar.tsx          │
│      📁 src/styles/          │
│      📄 package.json         │
│                              │
│ ▼  Tasks               8    │
│   ⟳ Understanding codebase   │
│     Active                  │
│   ────────────────────────── │
│   ○ Which is complex...      │
│   ○ Refactor gaming...       │
│   ○ Optimize queries...      │
│                              │
│ ▼  Git Changes        12    │
│   7 Mod │ 3 New │ 2 Del     │
│   ────────────────────────── │
│   M Sidebar.tsx    +45 -12   │
│   A helpers.ts        NEW    │
│   D legacy-logo.svg   DEL    │
│   M globals.css    +10 -5    │
╰──────────────────────────────╯
```

---

## Final Notes

This is a comprehensive documentation effort. Every UI element, behavior, color, and interaction has been documented with Ratatui equivalents. You should **not need to make assumptions** - if something isn't clear, look for the `@ratatui-*` comment tags or check `RATATUI_MAPPING.md`.

The goal is a fully functional, visually accurate TUI that provides the same user experience as the React web UI, adapted appropriately for terminal constraints.

**Good luck with your implementation!** 🚀

---

## Quick Start Command

```bash
# 1. Create project
cargo new innodrupe-tui --name innodrupe
cd innodrupe-tui

# 2. Add dependencies to Cargo.toml
# (See RATATUI_MAPPING.md → Required Crates)

# 3. Copy theme module
# Create src/theme.rs with color constants from RATATUI_MAPPING.md

# 4. Implement App struct
# Use the complete struct definition from RATATUI_MAPPING.md → State Management

# 5. Follow implementation checklist
# RATATUI_MAPPING.md → Implementation Checklist

# 6. Run and test
cargo run
```

Start with Phase 1, test thoroughly, then move to Phase 2. Don't skip phases!

---

## ASCII Art Quick Reference

### Box Drawing Characters (Copy-Paste Ready)

**Rounded (Recommended for modern look):**
```
╭─────────────────╮
│ Content here    │
├─────────────────┤
│ More content    │
╰─────────────────╯
```

**Standard:**
```
┌─────────────────┐
│ Content here    │
├─────────────────┤
│ More content    │
└─────────────────┘
```

**Double (For emphasis):**
```
╔═════════════════╗
║ Important!      ║
╠═════════════════╣
║ Content         ║
╚═════════════════╝
```

**Heavy:**
```
┏━━━━━━━━━━━━━━━━━┓
┃ Content here    ┃
┣━━━━━━━━━━━━━━━━━┫
┃ More content    ┃
┗━━━━━━━━━━━━━━━━━┛
```

### Common Patterns

**Status bar with sections:**
```
│ ⚡ Model │ Provider ▼ │ Mode ▼ │ ● Working... │
```

**Accordion item:**
```
▼  Section Name ──────── 3
   Item content here
▶  Collapsed Section ─── 5
```

**Radio buttons:**
```
a) ◉ Selected option
b) ○ Unselected option
c) ○ Another option
```

**Checkboxes:**
```
a) ☑ Selected item
b) ☐ Unselected item
c) ☑ Another selected
```

**Context file tags:**
```
 📄 Terminal.tsx   📄 package.json   + 
```

**Progress/Loading:**
```
⟳ Task in progress...
● Task complete
○ Task pending
```

**Git status:**
```
M Terminal.tsx    +45 -12  (yellow)
A helpers.ts         NEW   (green)
D old-file.ts        DEL   (red)
```

---

## Verification Protocol

**CRITICAL:** Before implementing ANY component, you MUST complete this verification protocol.

### Step-by-Step Verification Process

#### 1. Visual Verification (Screenshot Analysis)

**Action:** Open `SCREENSHOTS_REFERENCE.md` and locate the relevant screenshot(s).

**Questions to Answer:**
- What does this component look like?
- What colors are used?
- How is it laid out?
- What are the spacing/sizing characteristics?
- Are there any interactive elements?
- What states does it show (default, hover, selected, disabled)?

**Documentation:**
```
Screenshot: screenshots/[filename].png
Visual Notes:
- Layout: [describe structure]
- Colors: [list key colors with hex codes]
- Spacing: [padding, margins]
- States: [list all states shown]
```

#### 2. React Code Verification

**Action:** Open the relevant React component file(s).

**What to Look For:**
- Component definition and props
- State variables and their types
- Event handlers (onClick, onChange, onKeyDown, etc.)
- Conditional rendering logic
- All `@ratatui-*` annotations
- CSS classes and inline styles
- Child components and their relationships

**Verification Checklist:**
- [ ] Component file identified and read
- [ ] All @ratatui annotations found and noted
- [ ] State management understood
- [ ] Event handling mapped
- [ ] Styling extracted (colors, spacing, borders)
- [ ] Child component relationships documented

**Documentation:**
```
React File: src/app/components/[Component].tsx
Lines: [start]-[end]

State Variables:
- [varName]: [type] - [purpose]

Event Handlers:
- [handlerName]: [trigger] → [action]

Annotations Found:
- @ratatui-[type]: [description]

Discrepancies: [Any mismatches between annotations and actual code]
```

#### 3. Documentation Cross-Reference

**Action:** Read `RATATUI_MAPPING.md` section for this component.

**Verify:**
- [ ] Component is documented in RATATUI_MAPPING.md
- [ ] Widget mapping is specified
- [ ] Color palette is defined
- [ ] State management approach is described
- [ ] Event handling is mapped
- [ ] Code examples are provided

**If Discrepancies Found:**
```
Discrepancy Report:
- Documentation says: [what docs say]
- React code shows: [what code actually does]
- Screenshot shows: [what screenshot displays]
- Resolution: [trust React code and screenshot]
```

#### 4. Existing Ratatui Function Search

**Action:** Search the target Ratatui codebase for existing implementations.

**Search Strategy:**
```bash
# Search for component name
grep -r "struct [ComponentName]" .
grep -r "fn render_[component]" .

# Search for related functionality
grep -r "[keyword]" .

# Search for similar UI patterns
grep -r "Modal" . | grep "struct"
grep -r "Picker" . | grep "impl"
```

**Document Findings:**
```
Search Results:
- Function name: [name] or "NONE"
- Location: [file path] or "N/A"
- Signature: [fn signature] or "N/A"
- Match quality: EXACT | PARTIAL | NONE
- Notes: [Observations about existing implementation]
```

#### 5. Gap Analysis

**Compare:** What React UI needs vs. what Ratatui codebase has.

**Document:**
```
Required Behavior:
1. [Feature from React]
2. [Feature from React]
3. [Feature from React]

Existing Functions:
✅ [function_name] - Provides [feature]
✅ [function_name] - Provides [feature]
❌ [MISSING] - Need to implement [feature]
```

#### 6. Implementation Decision

**Based on gap analysis, decide:**

**Option A: Map Existing Function**
```
Decision: MAP
Function: [existing_function_name]
Location: [file:line]
Required Changes:
- None (exact match)
- OR: Parameter adjustment
- OR: Return type modification
- OR: Behavior extension
```

**Option B: Create New Function**
```
Decision: CREATE
Reason: No existing function found / Existing doesn't match requirements
Proposed Signature:
pub fn function_name(
    param1: Type1,
    param2: Type2,
) -> ReturnType {
    // Implementation plan
}
Dependencies: [list any new dependencies needed]
```

---

## Function Mapping Workflow

Use this workflow for EVERY UI element you implement.

### Workflow Diagram

```
┌──────────────────┐
│ Start Component  │
└────────┬─────────┘
         │
         ▼
┌──────────────────────┐
│ 1. View Screenshot   │ ← SCREENSHOTS_REFERENCE.md
└────────┬─────────────┘
         │
         ▼
┌──────────────────────┐
│ 2. Read React Code   │ ← src/app/components/*.tsx
└────────┬─────────────┘
         │
         ▼
┌──────────────────────┐
│ 3. Check Mapping Doc │ ← RATATUI_MAPPING.md
└────────┬─────────────┘
         │
         ▼
┌──────────────────────┐
│ 4. Search Codebase   │ ← grep/ripgrep Ratatui repo
└────────┬─────────────┘
         │
         ▼
    ┌────────┴────────┐
    │  Exists?        │
    └────┬────────┬───┘
         │        │
    Yes  │        │ No
         │        │
         ▼        ▼
   ┌─────────┐  ┌──────────┐
   │   MAP   │  │  CREATE  │
   └────┬────┘  └────┬─────┘
         │            │
         └──────┬─────┘
                │
                ▼
        ┌──────────────┐
        │  Implement   │
        └──────┬───────┘
                │
                ▼
        ┌──────────────┐
        │  Test Match  │ ← Compare to screenshot
        └──────┬───────┘
                │
           ┌────┴────┐
           │ Match?  │
           └────┬────┘
                │
        Yes ────┼──── No
                │      │
                ▼      └──→ (Fix & Re-test)
        ┌──────────────┐
        │  Complete!   │
        └──────────────┘
```

### Detailed Steps

#### Step 1: Identify UI Behavior

From screenshot and React code, extract:

```markdown
Component: [Name]
Purpose: [What it does]
User Actions:
- [action 1] → [result]
- [action 2] → [result]
Visual State:
- Default: [appearance]
- Active/Hover: [appearance]
- Disabled: [appearance]
Data Requirements:
- Input: [what data it needs]
- Output: [what data it produces]
```

#### Step 2: Search for Existing Functions

**Search Patterns:**
```bash
# By component type
rg "struct.*Modal"
rg "fn.*render.*picker"

# By functionality
rg "keyboard.*input"
rg "scroll.*state"

# By widget type
rg "List::new"
rg "Table::new"
```

**Evaluate Match Quality:**
- **EXACT**: Function does exactly what we need
- **PARTIAL**: Function does 70%+ of what we need, can be adapted
- **NONE**: No match found, must create new

#### Step 3: Function Mapping Documentation

**Template:**
```markdown
### UI Element: [Name]

**React Source:** `[file:lines]`
**Screenshot:** `screenshots/[file].png`
**RATATUI_MAPPING:** Section [X.Y]

**Required Behavior:**
[Detailed description of what this UI element must do]

**Existing Function Search:**
- Query: `[search terms used]`
- Found: `[function name]` at `[location]`
- Match: `EXACT` | `PARTIAL` | `NONE`

**Implementation Strategy:**

[If MAP:]
Function: `[existing_function_name]`
Mapping:
- React prop `[prop]` → Rust param `[param]`
- React state `[state]` → Rust field `[field]`
- React event `[event]` → Rust handler `[handler]`

[If CREATE:]
New Function Signature:
```rust
pub struct [ComponentName] {
    // Fields
}

impl [ComponentName] {
    pub fn new(...) -> Self { }
    pub fn render(&self, area: Rect, buf: &mut Buffer) { }
    pub fn handle_event(&mut self, event: Event) -> bool { }
}
```
Dependencies: [crates needed]
```

#### Step 4: Implementation

**If Mapping:**
1. Create wrapper if needed
2. Add conversion functions (React types → Rust types)
3. Wire up event handlers
4. Test with sample data

**If Creating:**
1. Define struct with all needed fields
2. Implement constructor
3. Implement render method following Ratatui patterns
4. Implement event handler
5. Add state management
6. Write tests

#### Step 5: Verification

**Acceptance Criteria:**
- [ ] Visual appearance matches screenshot
- [ ] Colors match theme specification exactly
- [ ] Layout matches proportions
- [ ] Interactive behavior works as expected
- [ ] Keyboard shortcuts functional
- [ ] State management correct
- [ ] Edge cases handled
- [ ] Performance acceptable

---

## Component Cross-Reference Table

This table maps each UI component to its source files and documentation.

| Component | React File | Lines | RATATUI_MAPPING Section | Screenshots | Status |
|-----------|-----------|-------|------------------------|-------------|--------|
| **Terminal Header** | Terminal.tsx | 150-180 | Section 2.1 | `01-main-layout.png` | 🟡 |
| **Status Bar** | Terminal.tsx | 580-680 | Section 2.4 | `compact-status-bar.png` | 🟡 |
| **Agent Mode Selector** | Terminal.tsx | 585-610 | Section 7.1 | `04-agent-mode-dropdown.png` | 🟡 |
| **Build Mode Selector** | Terminal.tsx | 612-637 | Section 7.1 | `05-build-mode-dropdown.png` | 🟡 |
| **Model Selector** | Terminal.tsx | 655-685 | Section 7.1 | `compact-status-bar.png` | 🟡 |
| **System Messages** | Terminal.tsx | 200-220 | Section 6.1 | `03-message-types-top.png` | 🟡 |
| **User Messages** | Terminal.tsx | 221-245 | Section 6.2 | `themed-message-bubbles.png` | 🟡 |
| **Agent Messages** | Terminal.tsx | 246-275 | Section 6.3 | `themed-message-bubbles.png` | 🟡 |
| **Thinking Blocks** | Terminal.tsx | 276-310 | Section 6.4 | `03-message-types-top.png` | 🟡 |
| **Tool Messages** | Terminal.tsx | 311-345 | Section 6.5 | `03-message-types-top.png` | 🟡 |
| **Command Messages** | Terminal.tsx | 346-360 | Section 6.6 | `03-message-types-top.png` | 🟡 |
| **Single Choice Question** | Terminal.tsx | 400-450 | Section 8.1 | `12-question-single-selected.png` | 🟡 |
| **Multiple Choice Question** | Terminal.tsx | 451-505 | Section 8.2 | `questions-answered-with-selections.png` | 🟡 |
| **Free Text Question** | Terminal.tsx | 506-545 | Section 8.3 | `03-message-types-top.png` | 🟡 |
| **Provider Picker** | Terminal.tsx | 723-780 | Section 9.1 | `06-provider-picker-modal.png` | 🟡 |
| **Model Picker** | Terminal.tsx | 781-840 | Section 9.2 | `07-model-picker-modal.png` | 🟡 |
| **File Picker** | Terminal.tsx | 841-900 | Section 9.3 | `09-file-picker-modal.png` | 🟡 |
| **Theme Picker** | Terminal.tsx | 901-955 | Section 9.4 | `10-theme-picker-modal.png` | 🟡 |
| **Help Modal** | Terminal.tsx | 956-1020 | Section 9.5 | `08-help-modal.png` | 🟡 |
| **Input Area** | Terminal.tsx | 1050-1100 | Section 2.3 | `01-main-layout.png` | 🟡 |
| **Context File Tags** | Terminal.tsx | 685-720 | Section 5.2 | `01-main-layout.png` | 🟡 |
| **Sidebar Container** | Sidebar.tsx | 50-100 | Section 3 | `11-sidebar-panel.png` | 🟡 |
| **Session Section** | Sidebar.tsx | 150-200 | Section 3.1 | `11-sidebar-panel.png` | 🟡 |
| **Context Section** | Sidebar.tsx | 201-250 | Section 3.2 | `11-sidebar-panel.png` | 🟡 |
| **Tasks Section** | Sidebar.tsx | 251-300 | Section 3.3 | `11-sidebar-panel.png` | 🟡 |
| **Git Changes Section** | Sidebar.tsx | 301-350 | Section 3.4 | `11-sidebar-panel.png` | 🟡 |

**Status Legend:**
- 🔴 Not started
- 🟡 Needs verification
- 🟢 Verified and mapped
- ✅ Implementation complete

---

## Required Verifications

Before marking ANY component as complete, verify ALL of the following:

### Visual Verification
- [ ] Screenshot reviewed and understood
- [ ] Layout matches screenshot proportions
- [ ] Colors match theme specification (RGB values)
- [ ] Borders and spacing match screenshot
- [ ] Icons/glyphs are correct
- [ ] Text formatting matches (bold, italics, etc.)
- [ ] All visual states captured (default, active, disabled)

### Code Verification
- [ ] React source code read completely
- [ ] All @ratatui-* annotations found and analyzed
- [ ] State variables identified and understood
- [ ] Event handlers mapped to Ratatui events
- [ ] Prop types converted to Rust types
- [ ] Child component relationships documented
- [ ] Any discrepancies between annotations and code noted

### Documentation Verification
- [ ] RATATUI_MAPPING.md section read
- [ ] Widget mapping understood and applied
- [ ] Color palette references extracted
- [ ] State management pattern followed
- [ ] Event handling approach implemented
- [ ] Code examples adapted to codebase

### Implementation Verification
- [ ] Existing function searched in codebase
- [ ] Function mapping or creation decision made
- [ ] Implementation matches React behavior
- [ ] All interactive features work correctly
- [ ] Keyboard shortcuts implemented
- [ ] Mouse interactions (if applicable) work
- [ ] Edge cases handled (empty, overflow, errors)

### Integration Verification
- [ ] Component integrates with parent layout
- [ ] State management connected correctly
- [ ] Events propagate to correct handlers
- [ ] Theme colors applied correctly
- [ ] Resizing behavior correct
- [ ] No performance issues

### Quality Verification
- [ ] Code follows Rust best practices
- [ ] Comments explain complex logic
- [ ] No unwanted side effects
- [ ] Memory usage reasonable
- [ ] No panic!() calls (use Result/Option)
- [ ] Error handling comprehensive

---

## Example: Complete Verification Workflow

Here's a complete example of the verification workflow for the "Single Choice Question" component:

### 1. Visual Verification
```
Screenshot: screenshots/12-question-single-selected.png

Visual Analysis:
- Layout: Question icon + text header, 4 radio options vertically, confirm button
- Colors: Cyan icon (#89dceb), selected option has teal background
- Spacing: 2-line padding between options, 1-line margin from buttons
- States: Shows selected state (option 'a' with checkmark)
- Size: Full width of terminal, height ~15 lines
```

### 2. React Code Verification
```
File: src/app/components/Terminal.tsx
Lines: 400-450

State Variables:
- selectedOption: string | null - Currently selected option ID
- options: Array<{id, label, description}> - List of choices

Event Handlers:
- handleOptionClick(id) - Sets selectedOption
- handleConfirm() - Submits answer, updates answeredQuestions

Annotations:
- @ratatui-widget: List with selectable items
- @ratatui-state: selected_index: usize
- @ratatui-event: Up/Down for navigation, Enter to select, 'c' to confirm

Discrepancies: None found
```

### 3. Documentation Check
```
RATATUI_MAPPING.md: Section 8.1 - Single Choice Questions

Widget: ratatui::widgets::List
State: selected_index, options: Vec<QuestionOption>
Events: KeyCode::Up/Down, KeyCode::Enter, KeyCode::Char('c')
Colors: question_icon: #89dceb, selected_bg: theme.accent

Code Example Provided: ✅
```

### 4. Function Search
```
Search: rg "struct.*Question" --type rust
Found: pub struct Question { ... } in ui/questions.rs

Search: rg "fn.*single_choice" --type rust
Found: fn render_single_choice(...) in ui/questions.rs:145

Match Quality: PARTIAL (80%)
- Has basic structure
- Missing confirm button UI
- Missing answer persistence
```

### 5. Implementation Decision
```
Decision: MAP with modifications

Existing Function: render_single_choice()
Modifications Needed:
1. Add confirm button rendering
2. Add answered state display
3. Wire up 'c' key for confirm
4. Persist answer on confirm

Estimated Effort: 2-3 hours
```

### 6. Implementation
```rust
// Extended existing function
impl Question {
    pub fn render_single_choice(&self, area: Rect, buf: &mut Buffer) {
        // Existing code...
        
        // Added: Confirm button
        if let Some(selected) = self.selected_index {
            let button_area = /* calculate */;
            self.render_confirm_button(button_area, buf);
        }
        
        // Added: Answered state
        if let Some(answer) = &self.answer {
            self.render_answer_badge(area, buf, answer);
        }
    }
    
    pub fn handle_confirm(&mut self) {
        if let Some(idx) = self.selected_index {
            self.answer = Some(self.options[idx].clone());
        }
    }
}
```

### 7. Verification
```
✅ Visual appearance matches screenshot
✅ Colors match: cyan icon (#89dceb), teal selection
✅ Layout proportions correct
✅ Up/Down navigation works
✅ Enter selects option
✅ 'c' confirms selection
✅ Answer persistence works
✅ Answered state displays correctly
✅ Edge case: Works with 0 options (shows message)
✅ Performance: < 1ms render time

Status: ✅ COMPLETE
```

---

## Tips for Efficient Verification

1. **Batch Similar Components**: Verify all message types together, all pickers together, etc.

2. **Create Verification Templates**: Save your verification notes for each component type to reuse

3. **Use Checklists**: Print or save the verification checklists and check items off

4. **Screenshot Comparison Tool**: Use a split-screen or dual monitor setup to compare side-by-side

5. **Automate Searches**: Create shell scripts for common searches:
```bash
#!/bin/bash
# find_function.sh
rg "$1" --type rust -A 5
```

6. **Document Everything**: Even if it seems obvious, write it down. Future you will thank current you.

7. **Test Early, Test Often**: Don't wait until implementation is "done" to test against screenshots

---

## Common Verification Mistakes to Avoid

1. ❌ **Skipping the Screenshot**: Never implement without seeing the visual reference
2. ❌ **Trusting Annotations Only**: Always verify against actual React code
3. ❌ **Approximate Colors**: Use exact RGB values from theme.css
4. ❌ **Ignoring Edge Cases**: Test with empty data, long text, narrow terminals
5. ❌ **Partial Function Search**: Search thoroughly before assuming function doesn't exist
6. ❌ **Incomplete Testing**: Test ALL keyboard shortcuts and interactions
7. ❌ **Forgetting Themes**: Test with multiple theme presets
8. ❌ **Skipping Documentation**: Read RATATUI_MAPPING.md section for each component

---

**Remember:** Verification is not optional. It's the foundation of accurate implementation. Take the time to verify thoroughly, and your implementation will be solid. ✅
