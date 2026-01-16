# BDD Feature Files for TUI Testing

This directory contains Gherkin feature files for Behavior-Driven Development (BDD) testing of the Ratatui TUI application.

## Overview

These feature files define the expected behavior of the TUI and serve as:
1. **Specification** - Clear documentation of what the TUI should do
2. **Test Cases** - Executable tests for the Ratatui implementation
3. **Development Guide** - Requirements for the AI agent building the TUI

## Feature Files

| File | Feature | Scenarios |
|------|---------|-----------|
| `01_terminal_layout.feature` | Core layout structure | ~15 |
| `02_status_bar.feature` | Status bar components | ~25 |
| `03_message_display.feature` | Message rendering | ~20 |
| `04_input_area.feature` | Input/prompt functionality | ~25 |
| `05_modals_provider_picker.feature` | Provider selection modal | ~15 |
| `06_modals_model_picker.feature` | Model selection modal | ~15 |
| `07_modals_file_picker.feature` | File picker modal | ~20 |
| `08_modals_theme_picker.feature` | Theme selection modal | ~15 |
| `09_modals_help.feature` | Help & shortcuts modal | ~15 |
| `10_questions_multiple_choice.feature` | Multi-select questions | ~15 |
| `11_questions_single_choice.feature` | Single-select questions | ~12 |
| `12_questions_free_text.feature` | Free text questions | ~15 |
| `13_sidebar.feature` | Sidebar panel | ~20 |
| `14_theming.feature` | Theme system | ~18 |
| `15_keyboard_shortcuts.feature` | Keyboard navigation | ~25 |

**Total: ~245 scenarios**

## Tags

### Component Tags
- `@layout` - Layout and structure tests
- `@status-bar` - Status bar component tests
- `@messages` - Message display tests
- `@input` - Input area tests
- `@modal` - Modal/popup tests
- `@questions` - Question component tests
- `@sidebar` - Sidebar tests
- `@theming` - Theme system tests
- `@keyboard` - Keyboard shortcut tests

### Specific Feature Tags
- `@provider-picker` - Provider picker modal
- `@model-picker` - Model picker modal
- `@file-picker` - File picker modal
- `@theme-picker` - Theme picker modal
- `@help` - Help modal
- `@multiple-choice` - Multi-select questions
- `@single-choice` - Single-select questions
- `@free-text` - Free text questions

### Priority Tags
- `@smoke` - Critical smoke tests
- `@core` - Core functionality tests

## Running Tests in Rust

### Using `cucumber-rs`

```toml
# Cargo.toml
[dev-dependencies]
cucumber = "0.20"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
// tests/cucumber.rs
use cucumber::World;

#[derive(Debug, Default, World)]
pub struct TuiWorld {
    app: Option<App>,
    terminal: Option<TestTerminal>,
}

#[tokio::main]
async fn main() {
    TuiWorld::run("features/").await;
}
```

### Step Definitions Example

```rust
use cucumber::{given, when, then};

#[given("the TUI application is running")]
async fn app_running(world: &mut TuiWorld) {
    world.app = Some(App::new());
    world.terminal = Some(TestTerminal::new(80, 24));
}

#[when(regex = r"I type (.+)")]
async fn type_text(world: &mut TuiWorld, text: String) {
    if let Some(app) = &mut world.app {
        for c in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::empty()));
        }
    }
}

#[then("the input area should display {string}")]
async fn input_displays(world: &mut TuiWorld, expected: String) {
    if let Some(app) = &world.app {
        assert_eq!(app.input_content(), expected);
    }
}
```

## Screenshot References

Each feature file references relevant screenshots from the `screenshots/` directory:
- Layout screenshots: `01-main-layout.png`, `02-full-page.png`
- Modal screenshots: `06-provider-picker-modal.png`, `07-model-picker-modal.png`
- Component screenshots: `compact-status-bar.png`, `agent-working-indicator.png`

## TUI-Specific Considerations

### What's Different in TUI vs Web

| Web Behavior | TUI Equivalent |
|--------------|----------------|
| Click | Focus + Enter |
| Hover | Focus highlight |
| Smooth animations | Instant updates |
| RGB colors | Theme palette colors |
| Lucide icons | Unicode/Nerd Font glyphs |
| CSS styling | Ratatui `Style` |

### Terminal Capabilities

Tests should account for:
- Different terminal sizes (minimum 80x24)
- Color support (256 colors, true color)
- Unicode support for box drawing and icons
- No mouse support in some scenarios

## Development Workflow

1. **Read feature file** for the component you're implementing
2. **Implement step definitions** for the scenarios
3. **Run tests** to verify behavior
4. **Iterate** until all scenarios pass

```bash
# Run all tests
cargo test --test cucumber

# Run specific feature
cargo test --test cucumber -- features/02_status_bar.feature

# Run tests with specific tag
cargo test --test cucumber -- --tags @smoke
```

## Contributing

When adding new features:
1. Create a new `.feature` file with appropriate numbering
2. Follow the existing format and tagging conventions
3. Reference relevant screenshots
4. Update this README with the new file
