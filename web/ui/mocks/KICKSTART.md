# KICKSTART: React UI → Ratatui TUI Migration

**START HERE** - This is your ONLY entry point for migrating the React UI to a Rust Ratatui TUI.

---

## 🎯 Mission

You are implementing a Terminal User Interface (TUI) in Rust using Ratatui, based on this fully-documented React/TypeScript codebase. Your goal is to:

1. **Verify** existing Ratatui functions in the target codebase
2. **Map** verified functions to correct UI behaviors
3. **Implement** missing functions with proper signatures
4. **Match** the exact look, feel, and behavior shown in screenshots

---

## 📋 Quick Start Checklist

Follow this workflow for EVERY component:

- [ ] 1. View the screenshot(s) for the component
- [ ] 2. Read the React source code
- [ ] 3. Read RATATUI_MAPPING.md section for the component
- [ ] 4. Search the Ratatui codebase for existing functions
- [ ] 5. Document findings (exists vs missing)
- [ ] 6. Map or create the function
- [ ] 7. Verify behavior matches screenshot
- [ ] 8. Mark component complete

---

## 📖 Mandatory Reading Order

Read these files in this exact order:

### Phase 1: Understand the UI (30 minutes)

1. **SCREENSHOTS_REFERENCE.md** - Visual guide to all 49 screenshots
   - Start here to see what you're building
   - Refer back constantly during implementation

2. **src/app/components/Terminal.tsx** - Main terminal component (700+ lines)
   - Contains all message types, modals, questions, status bar
   - Read EVERY `@ratatui-*` comment carefully

3. **src/app/components/Sidebar.tsx** - Sidebar panel component
   - Session, Context, Tasks, Git Changes sections

4. **src/app/App.tsx** - Application container
   - State management and component wiring

### Phase 2: Understand the Documentation (20 minutes)

5. **RATATUI_MAPPING.md** - Complete mapping guide
   - Widget equivalents, color palette, state management
   - Code examples for every component type

6. **AGENT_INSTRUCTIONS.md** - Implementation instructions
   - Best practices, recommendations, what's achievable in TUI
   - Verification protocols and checklists

### Phase 3: Configuration & Theming (10 minutes)

7. **src/app/config/appConfig.ts** - Configurable settings
   - Agent name, user name, icons, paths

8. **src/app/themes/presets.ts** - Theme presets
   - Catppuccin, Nord, GitHub Dark, etc.
   - All color values documented

9. **src/styles/theme.css** - CSS variables
   - Every color referenced by components

---

## 🔍 Component Verification Workflow

For each UI component, follow this protocol:

### Step 1: Visual Reference
```
→ Open SCREENSHOTS_REFERENCE.md
→ Find the screenshot(s) for this component
→ Study the visual appearance, colors, layout
```

### Step 2: React Code Analysis
```
→ Open the React component file
→ Locate the component code
→ Read all @ratatui-* annotations
→ Understand state variables and event handlers
→ Note all colors, sizes, and styling
```

### Step 3: Mapping Documentation
```
→ Open RATATUI_MAPPING.md
→ Find the section for this component
→ Read Rust equivalents and examples
→ Note any limitations or alternatives
```

### Step 4: Codebase Search
```
→ Search Ratatui codebase for relevant functions
→ Look for similar UI elements or behaviors
→ Check if the function signature matches expected behavior
```

### Step 5: Document Findings
```
Component: [Name]
Screenshot: [Reference]
React File: [Path]
Existing Function: [name] or "NONE"
Action Required: "MAP" or "CREATE"
Notes: [Any discrepancies or concerns]
```

### Step 6: Implementation
```
If EXISTS:
  → Map the function to the UI element
  → Verify parameters match expected behavior
  → Test with sample data

If MISSING:
  → Design function signature based on docs
  → Implement with Ratatui widgets
  → Follow coding patterns from existing code
  → Add comprehensive comments
```

### Step 7: Verification
```
- [ ] Visual appearance matches screenshot
- [ ] Colors match theme specification
- [ ] Interactive behavior matches React code
- [ ] All @ratatui annotations addressed
- [ ] Edge cases handled
- [ ] No unwanted side effects
```

---

## 🗺️ Function Mapping Template

Use this template for documenting each function:

```markdown
### Component: [Name]

**UI Behavior:** [Description from React code]
**Screenshot:** `screenshots/[filename].png`
**React Source:** `src/app/components/[Component].tsx` lines X-Y

**Ratatui Function Search Results:**
- Existing function: `[function_name]` or `NONE`
- Location: `[file_path]` or `N/A`
- Match quality: `EXACT` / `PARTIAL` / `NONE`

**Action:** `MAP` or `CREATE`

**Rust Signature:**
```rust
fn function_name(
    params: Type,
    // ...
) -> ReturnType {
    // Implementation notes
}
```

**Verification Checklist:**
- [ ] Screenshot reviewed
- [ ] React code analyzed
- [ ] RATATUI_MAPPING section read
- [ ] Function searched in codebase
- [ ] Implementation complete
- [ ] Behavior verified
```

---

## 🎨 Visual Reference Guide

All 49 screenshots are in `./screenshots/` and documented in `SCREENSHOTS_REFERENCE.md`.

### Key Screenshots by Category:

| Component | Screenshot | Priority |
|-----------|------------|----------|
| **Main Layout** | `01-main-layout.png` | ⭐⭐⭐ Must-see first |
| **Message Types** | `03-message-types-top.png` | ⭐⭐⭐ Core functionality |
| **Status Bar** | `compact-status-bar.png` | ⭐⭐⭐ Always visible |
| **Modals** | `06-provider-picker-modal.png` | ⭐⭐ Important interactions |
| **Questions** | `12-question-single-selected.png` | ⭐⭐ User input |
| **Sidebar** | `11-sidebar-panel.png` | ⭐⭐ Information display |
| **Themes** | `catppuccin-themed-bubbles.png` | ⭐ Visual polish |

---

## 🏗️ Implementation Priority Order

Implement components in this order for fastest progress:

### Phase 1: Core Terminal (Week 1)
1. **Terminal Layout** - Main container, header, output area, input area, status bar
2. **Message Types** - System, Input, Output, Tool, Command messages
3. **Scrolling** - Output area scrolling with scrollbar
4. **Input Area** - Text input with wrapping and submission

### Phase 2: Interactive Elements (Week 2)
5. **Status Bar** - Agent mode, build mode, model display, indicators
6. **Mode Selectors** - Dropdowns for agent mode and build mode
7. **Thinking Blocks** - Collapsible thinking sections
8. **Tool Messages** - Collapsible tool execution details

### Phase 3: Advanced Features (Week 3)
9. **Questions** - Single choice, multiple choice, free text input
10. **System Modals** - Provider picker, model picker, file picker, theme picker, help modal
11. **Context Files** - @mention functionality, file selection
12. **Keyboard Shortcuts** - Complete keyboard navigation

### Phase 4: Sidebar & Polish (Week 4)
13. **Sidebar Layout** - Collapsible panel with sections
14. **Session Section** - Branch info, model info, costs
15. **Context Section** - Loaded files with token counts
16. **Tasks Section** - Active and queued tasks
17. **Git Changes Section** - Modified, new, deleted files

### Phase 5: Theming (Week 5)
18. **Theme System** - Theme struct and application
19. **Theme Presets** - Catppuccin, Nord, GitHub Dark, etc.
20. **Theme Switcher** - `/theme` command and picker modal

---

## ⚠️ Critical Guidelines

### DO:
- ✅ Read React source code for EVERY component before implementing
- ✅ Verify all `@ratatui-*` annotations match actual behavior
- ✅ Use screenshots as visual acceptance criteria
- ✅ Search for existing functions before creating new ones
- ✅ Follow the exact color values from theme.css
- ✅ Implement keyboard shortcuts from help modal
- ✅ Handle edge cases (empty lists, long text, narrow terminals)

### DON'T:
- ❌ Skip reading the React code
- ❌ Assume annotations are accurate without verification
- ❌ Implement without checking screenshots
- ❌ Create duplicate functions
- ❌ Use approximate colors
- ❌ Ignore accessibility (keyboard navigation)
- ❌ Forget to test on different terminal sizes

---

## 🔧 Required Tools & Setup

Before starting, ensure you have:

1. **Rust & Cargo** - Latest stable version
2. **Ratatui** - v0.26+ (check Cargo.toml)
3. **Crossterm** - For event handling
4. **Arboard** - For clipboard support
5. **Terminal with true color** - For accurate colors
6. **Nerd Font** - For icon display (recommended: FiraCode Nerd Font)

Test your terminal:
```bash
# Check true color support
printf "\x1b[38;2;255;100;0mTRUECOLOR\x1b[0m\n"

# Test Nerd Font icons
echo "     "
```

---

## 📊 Progress Tracking

Use this checklist to track overall progress:

### Core Components
- [ ] Terminal layout (header, output, input, status)
- [ ] Message rendering (all 7 types)
- [ ] Scrolling and navigation
- [ ] Input handling and submission

### Interactive Elements
- [ ] Mode selectors (agent, build)
- [ ] Thinking blocks (collapsible)
- [ ] Tool messages (collapsible)
- [ ] Copy buttons

### Questions & Modals
- [ ] Single choice questions
- [ ] Multiple choice questions
- [ ] Free text questions
- [ ] Provider picker modal
- [ ] Model picker modal
- [ ] File picker modal (@mentions)
- [ ] Theme picker modal
- [ ] Help & shortcuts modal

### Sidebar
- [ ] Collapsible panel
- [ ] Session section
- [ ] Context section
- [ ] Tasks section
- [ ] Git changes section

### Theming
- [ ] Theme struct and system
- [ ] All 7 theme presets
- [ ] Theme switching
- [ ] Color accuracy

### Polish
- [ ] Keyboard shortcuts (all)
- [ ] Clipboard support
- [ ] Error handling
- [ ] Terminal resize handling
- [ ] Performance optimization

---

## 🆘 Troubleshooting

### "I can't find a function in the Ratatui codebase"
→ Use `grep -r "fn function_name"` or similar search
→ Look for related functionality with different names
→ Check if it's in a trait implementation
→ Document as "NONE" and create it

### "The React code doesn't match the annotations"
→ Trust the React code, not the annotations
→ Document the discrepancy in your mapping
→ Update implementation to match actual behavior
→ Note it for documentation updates

### "The screenshot looks different from the code"
→ The screenshot is the source of truth
→ Check if it's a theme-specific appearance
→ Look for dynamic styling based on state
→ Implement what you see in the screenshot

### "The terminal doesn't support true color"
→ Fall back to 256-color palette
→ Use closest approximations
→ Document color mapping strategy
→ Test on both true color and 256-color terminals

### "I don't understand a Ratatui concept"
→ Check RATATUI_MAPPING.md for examples
→ Read official Ratatui documentation
→ Look at existing code in the codebase
→ Ask for clarification if still unclear

---

## 🎓 Learning Resources

- **Ratatui Book**: https://ratatui.rs/
- **Crossterm Docs**: https://docs.rs/crossterm/
- **This Codebase**:
  - RATATUI_MAPPING.md - Complete widget mapping
  - AGENT_INSTRUCTIONS.md - Best practices
  - SCREENSHOTS_REFERENCE.md - Visual guide

---

## ✅ Final Checklist Before Completion

Before marking the migration complete:

- [ ] All 20 components from priority list implemented
- [ ] All 49 screenshots reviewed and matched
- [ ] All React code annotations verified
- [ ] All keyboard shortcuts working
- [ ] All 7 themes implemented and tested
- [ ] Clipboard operations functional
- [ ] Terminal resize handling correct
- [ ] Performance acceptable (< 16ms frame time)
- [ ] No crashes or panics
- [ ] Code documented with comments
- [ ] README updated with build/run instructions

---

## 🚀 Ready to Start?

1. Open `SCREENSHOTS_REFERENCE.md` to see what you're building
2. Read `src/app/components/Terminal.tsx` to understand the main component
3. Start with Phase 1: Core Terminal components
4. Follow the verification workflow for each component
5. Document your progress using the function mapping template

**Remember:** Screenshots are your acceptance criteria. If it doesn't look like the screenshot, it's not done.

Good luck! 🎉
