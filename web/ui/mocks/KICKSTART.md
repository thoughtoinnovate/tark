# KICKSTART: TDD-Driven Ratatui TUI Development

**START HERE** - This is your ONLY entry point for implementing the Ratatui TUI using Test-Driven Development.

---

## 🚨🚨🚨 CRITICAL: Test the REAL `tark tui` App - NEVER Mocks! 🚨🚨🚨

**THIS IS THE MOST IMPORTANT RULE. READ IT MULTIPLE TIMES.**

### ⛔ ABSOLUTE REQUIREMENT: Run BDD Against ACTUAL `tark tui`

**Every BDD test MUST run against the REAL `tark tui` command, not mock objects!**

```bash
# ✅ CORRECT: BDD tests invoke the REAL tark tui binary
cargo test --test cucumber_tui  # This MUST spawn ./target/release/tark tui

# ❌ WRONG: BDD tests only test mock objects in memory
# (If your tests pass without `tark tui` existing, YOUR TESTS ARE BROKEN!)
```

### 🔴 What "No Mocks" Means

| Term | What It Means | Allowed? |
|------|---------------|----------|
| **TUI Mocks** | Fake step definitions that don't test real TUI | ❌ **NEVER** |
| **Test Stubs** | `assert!(true)` or hardcoded returns | ❌ **NEVER** |
| **Memory-only tests** | Tests that don't render to terminal/TestBackend | ❌ **NEVER** |
| **`tark_sim` provider** | LLM simulation for predictable AI responses | ✅ **ALLOWED** (only for LLM) |
| **`TestBackend`** | Ratatui's in-memory terminal for verification | ✅ **ALLOWED** (real rendering) |

### ⚠️ Clarification: "Mocks" vs "tark_sim"

**The ONLY mock allowed is `tark_sim` - and it's ONLY for LLM simulation:**

```bash
# tark_sim is ONLY for simulating LLM responses (not TUI behavior!)
./target/release/tark tui --provider tark_sim

# This makes AI responses predictable for testing
# BUT the TUI itself is REAL - not mocked!
```

**`tark_sim` does NOT mock:**
- ❌ TUI rendering (real widgets must render)
- ❌ Keyboard input (real events must be handled)
- ❌ Layout/styling (real Ratatui code must run)
- ❌ State management (real AppState must work)

**`tark_sim` ONLY mocks:**
- ✅ LLM API calls (returns predictable responses)
- ✅ Network requests to AI providers

### The cucumber tests MUST:

1. **Spawn the REAL `tark tui` binary** - Not just instantiate objects in memory
2. **Render actual widgets** using `TestBackend` from ratatui  
3. **Verify actual terminal output** - check characters, colors, positions
4. **Run against the compiled binary** for E2E tests
5. **Compare against reference screenshots** for visual verification

### ❌ WRONG approach (mocks that always pass):

```rust
fn has_header(&self) -> bool {
    true  // ❌ USELESS - This always passes without testing anything!
}

#[then("I should see the terminal header")]
async fn see_header(world: &mut TuiWorld) {
    assert!(true);  // ❌ USELESS - No actual verification!
}

#[then("the status bar should show Build mode")]
async fn status_bar_mode(world: &mut TuiWorld) {
    // ❌ WRONG - Just checking a mock field, not real rendering
    assert_eq!(world.mock_mode, "Build");
}
```

### ✅ RIGHT approach (test REAL `tark tui`):

```rust
fn has_header(&self) -> bool {
    // ✅ Actually render and check the buffer
    let buffer = self.render_to_test_backend();
    buffer.cell((0, 0)).symbol() == "╭"  // Check actual rendered output
}

#[then("I should see the terminal header")]
async fn see_header(world: &mut TuiWorld) {
    // ✅ Render the REAL TuiApp and verify buffer contents
    let buffer = world.app.render_to_buffer();
    assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "╭",
        "Expected rounded corner at top-left of REAL rendered output");
}

#[then("the status bar should show Build mode")]
async fn status_bar_mode(world: &mut TuiWorld) {
    // ✅ Read from REAL rendered terminal buffer
    let buffer = world.app.render_to_buffer();
    let status_line = read_line_from_buffer(buffer, world.height - 1);
    assert!(status_line.contains("Build"), 
        "Status bar should show 'Build' in REAL rendered output");
}
```

### 🔍 How to Verify You're Testing Real TUI

**Self-check after implementing each feature:**

```bash
# 1. Delete src/tui_new/ temporarily
mv src/tui_new src/tui_new_backup

# 2. Run BDD tests
cargo test --test cucumber_tui

# 3. If tests PASS → YOUR TESTS ARE BROKEN (testing mocks, not real TUI)
# 4. If tests FAIL → Good! Tests actually verify real implementation

# 5. Restore
mv src/tui_new_backup src/tui_new
```

**If BDD tests pass without the TUI implementation existing, you have written useless mock tests!**

---

## 🛑 STOP: Read This Before Every Feature

```
╔═══════════════════════════════════════════════════════════════════════════╗
║  AFTER EVERY FEATURE IMPLEMENTATION, YOU MUST:                             ║
║                                                                            ║
║  1. cargo build --release --features test-sim                              ║
║  2. cargo test --test cucumber_tui -- features/XX.feature                  ║
║     ⚠️  Tests MUST run against REAL `tark tui` binary!                     ║
║     ⚠️  If tests pass without binary existing, TESTS ARE BROKEN!           ║
║  3. ./tests/visual/tui_e2e_runner.sh --scenario XX                         ║
║  4. Verify: tests/visual/tui/snapshots/XX/ has PNG files                   ║
║  5. Verify: tests/visual/tui/recordings/XX/ has GIF files                  ║
║  6. git add & commit snapshots + recordings                                ║
║                                                                            ║
║  MOCKS CLARIFICATION:                                                      ║
║  • tark_sim = LLM simulation ONLY (allowed for predictable AI responses)   ║
║  • TUI Mocks = NEVER ALLOWED (step definitions must test real rendering)   ║
╚═══════════════════════════════════════════════════════════════════════════╝
```

---

## 🎯 Mission

You are implementing a **new** Terminal User Interface (TUI) in Rust using Ratatui, which will be accessed using `tark tui` following a **Test-Driven Development (TDD)** approach with **Behavior-Driven Development (BDD)** feature files as specifications. Your goals are to:

1. **Follow TDD** - Write/run tests FIRST, then implement (RED-GREEN-REFACTOR)
2. **Use Feature Files** - 15 BDD feature files define ALL requirements (245+ scenarios)
3. **Snapshot-Driven** - Visual snapshots are your acceptance criteria
4. **Match Screenshots** - 49 reference screenshots show exact UI appearance
5. **E2E Testing** - Automated visual regression testing with asciinema + agg

**This is NOT just a React migration** - it's a complete TDD implementation where feature files and snapshots drive development.

---

## 🎯 END RESULT

**When complete, you will have:**

1. **Command**: `tark tui` - A new CLI subcommand that launches the TUI
2. **UI Match**: Visually identical to all 49 screenshots in `web/ui/mocks/screenshots/`
3. **Behavior Match**: Functionally equivalent to React code in `web/ui/mocks/src/app/components/`
4. **Test Pass**: All 270 cucumber scenarios pass (15 feature files, 100% green)
5. **E2E Pass**: Visual snapshots match reference screenshots (RMSE < 0.1)

**The TUI must be:**
- ✅ **Visually identical** to the React mock UI
- ✅ **Behaviorally equivalent** as defined by feature files
- ✅ **Test-verified** at both unit (BDD) and visual (E2E) levels

---

## � PREREQUISITE: The `tark tui` Command Must Exist First!

Before ANY cucumber tests can meaningfully pass, you MUST:

1. **Add `Tui` subcommand** to `src/main.rs`
2. **Implement `TuiApp::run()`** that actually renders to terminal
3. **Wire up the CLI** in `src/transport/cli.rs`

Without a working `tark tui` command, tests are just testing mocks!

---

## � What Exists vs What You Create

### EXISTS (DO NOT MODIFY):

**Pre-built specifications and references:**
- ✅ `tests/visual/tui/features/*.feature` - 15 BDD feature files (245+ scenarios)
- ✅ `web/ui/mocks/screenshots/*.png` - 49 reference screenshots
- ✅ `web/ui/mocks/src/app/components/*.tsx` - React files with @ratatui-* annotations (484 lines of guidance!)
- ✅ `tests/visual/tui/features/step_definitions/common_steps.rs` - Step definition templates
- ✅ `tests/visual/e2e_runner.sh` - E2E runner for chat (reference for TUI runner)
- ✅ `web/ui/mocks/RATATUI_MAPPING.md` - Complete widget mapping guide
- ✅ `web/ui/mocks/SCREENSHOTS_REFERENCE.md` - Visual guide to all screenshots

### AGENT MUST CREATE:

**Your implementation artifacts:**
- 🔨 `src/main.rs` - Add `Tui` subcommand (CLI wiring) **← DO THIS FIRST!**
- 🔨 `src/transport/cli.rs` - Add `run_tui()` function **← DO THIS FIRST!**
- 🔨 `src/tui_new/` - New TUI module (app.rs, widgets/, events.rs, etc.)
- 🔨 `tests/cucumber_tui.rs` - Cucumber test harness that tests REAL rendering
- 🔨 `tests/visual/tui_e2e_runner.sh` - E2E runner for TUI (adapt from e2e_runner.sh)
- 🔨 `tests/visual/tui/snapshots/*.png` - Baseline snapshots (generated by E2E, committed to git)

**Note:** `tui_e2e_runner.sh` doesn't exist yet - you create it by adapting the existing `e2e_runner.sh` (chat) to work with the TUI.

---

## 🔴🟢🔵 TDD Development Workflow

**CRITICAL:** For EVERY component, follow this RED-GREEN-REFACTOR cycle:

```
┌─────────────────────────────────────────────────────┐
│  TDD CYCLE (Repeat for each feature/scenario)      │
└─────────────────────────────────────────────────────┘

1. 📖 READ     → Read the feature file specification
                 Location: tests/visual/tui/features/XX_*.feature
                 
2. 🔴 RED      → Run cucumber test (MUST fail - not implemented)
                 Command: cargo test --test cucumber_tui -- features/XX
                 Expected: Test failures showing missing implementation
                 ⚠️ If tests PASS without implementation, your tests are WRONG!
                 
3. 💻 IMPLEMENT → Write MINIMAL code to make test pass
                  Reference: RATATUI_MAPPING.md + screenshots
                  
4. 🟢 GREEN    → Run cucumber test again (MUST pass)
                 All scenarios in the feature should pass
                 
5. 📸 SNAPSHOT → Run E2E test to capture visual snapshot
                 Command: ./tests/visual/tui_e2e_runner.sh --scenario XX
                 
6. 🔍 COMPARE  → Compare snapshot with reference screenshot
                 Reference: web/ui/mocks/screenshots/*.png
                 Verify: Visual appearance matches exactly
                 
7. 🔧 REFACTOR → Clean up code while keeping tests GREEN
                  Improve structure, remove duplication
                  Re-run tests to ensure still passing
                  
8. ✅ BASELINE → Accept snapshot as new baseline
                 Command: ./tests/visual/tui_e2e_runner.sh --update-baseline
                 Commit baseline to git for regression detection
```

**NEVER skip steps.** If tests pass before implementation (step 2), the test is wrong or incomplete.

---

## 🔁 MANDATORY: Test Real TUI After EVERY Feature

**After implementing EACH feature, you MUST run BDD against the REAL `tark tui`:**

### Per-Feature Testing Checklist (REQUIRED)

```bash
# After implementing feature XX (e.g., 01_terminal_layout):

# 1. BUILD the real binary
cargo build --release --features test-sim

# 2. RUN BDD tests against REAL tark tui
cargo test --test cucumber_tui -- features/XX_feature_name.feature

# ⚠️ These tests MUST spawn ./target/release/tark tui internally!
# ⚠️ If tests pass without the binary, tests are WRONG!

# 3. RUN E2E visual test
./tests/visual/tui_e2e_runner.sh --scenario XX_feature_name

# 4. VERIFY snapshot captured
ls -la tests/visual/tui/snapshots/XX_feature_name/

# 5. VERIFY recording captured  
ls -la tests/visual/tui/recordings/XX_feature_name/

# 6. COMMIT artifacts
git add tests/visual/tui/snapshots/XX_feature_name/
git add tests/visual/tui/recordings/XX_feature_name/
git commit -m "feat(tui): add feature XX with BDD tests and snapshots"
```

### Snapshot & Recording Directory Structure

**EVERY feature scenario MUST produce artifacts in these directories:**

```
tests/visual/tui/
├── snapshots/                           # PNG snapshots (COMMITTED to git)
│   ├── 01_terminal_layout/              # Feature-specific directory
│   │   ├── main_layout_initial.png
│   │   ├── main_layout_final.png
│   │   ├── header_rendering.png
│   │   └── scroll_behavior.png
│   ├── 02_status_bar/
│   │   ├── mode_selector_build.png
│   │   ├── mode_selector_plan.png
│   │   └── working_indicator.png
│   ├── 03_message_display/
│   │   └── ...
│   └── ... (one directory per feature)
│
├── recordings/                          # GIF recordings (COMMITTED to git)
│   ├── 01_terminal_layout/
│   │   ├── main_layout.gif
│   │   ├── scroll_interaction.gif
│   │   └── resize_behavior.gif
│   ├── 02_status_bar/
│   │   └── ...
│   └── ... (one directory per feature)
│
├── current/                             # Latest run (GITIGNORED)
└── diffs/                               # Visual diffs (GITIGNORED)
```

### ⚠️ FAILURE CONDITIONS

**Stop and fix if ANY of these occur:**

| Condition | Problem | Action |
|-----------|---------|--------|
| BDD passes without `tark tui` binary | Tests are mocks | Rewrite step definitions to test real TUI |
| No snapshots in `snapshots/XX/` | E2E not capturing | Fix tui_e2e_runner.sh scenario |
| No recordings in `recordings/XX/` | Recording failed | Check asciinema/agg setup |
| Snapshot doesn't match reference | Visual regression | Fix TUI rendering to match reference |

---

## 📋 Feature Files as Specifications

**ALL requirements are defined in BDD feature files.** These are NOT suggestions - they are the specification.

### ⚠️ CRITICAL: Feature Files are IMMUTABLE Acceptance Criteria

**DO NOT MODIFY FEATURE FILES!** They define the acceptance criteria.

```
❌ WRONG: "This scenario doesn't match my implementation, let me change it"
✅ RIGHT: "This scenario fails - I need to change my implementation to match it"

❌ WRONG: "I'll adjust the feature file to match what I built"
✅ RIGHT: "I'll adjust my code to match what the feature file specifies"

❌ WRONG: "The feature file has a typo/bug - I'll fix it"
✅ RIGHT: "If the feature file has an error, document it and ASK before changing"
```

**Your job is WIRING LOGIC, not specification:**
- ✅ Implement step definitions in `common_steps.rs`
- ✅ Write TuiApp code to make scenarios pass
- ✅ Create widgets and rendering logic
- ❌ Do NOT change Given/When/Then steps in .feature files
- ❌ Do NOT modify scenario descriptions
- ❌ Do NOT remove or skip scenarios because they're "hard"

**Exception:** Only modify feature files if there's a clear SPECIFICATION error (typo, impossible requirement, etc.). Document the issue first.

### Feature File Reference

| # | Feature File | Component | Scenarios | Priority | Test Command |
|---|-------------|-----------|-----------|----------|--------------|
| 01 | `01_terminal_layout.feature` | Core layout, header, scrolling | ~15 | **P0** (Critical) | `--feature 01_terminal_layout` |
| 02 | `02_status_bar.feature` | Status bar, mode selectors | ~25 | **P0** (Critical) | `--feature 02_status_bar` |
| 03 | `03_message_display.feature` | Message types, rendering | ~20 | **P0** (Critical) | `--feature 03_message_display` |
| 04 | `04_input_area.feature` | Input field, submission | ~25 | **P0** (Critical) | `--feature 04_input_area` |
| 05 | `05_modals_provider_picker.feature` | Provider picker modal | ~15 | **P1** (High) | `--feature 05_modals_provider` |
| 06 | `06_modals_model_picker.feature` | Model picker modal | ~15 | **P1** (High) | `--feature 06_modals_model` |
| 07 | `07_modals_file_picker.feature` | File picker modal | ~20 | **P1** (High) | `--feature 07_modals_file` |
| 08 | `08_modals_theme_picker.feature` | Theme picker modal | ~15 | **P1** (High) | `--feature 08_modals_theme` |
| 09 | `09_modals_help.feature` | Help modal | ~15 | **P1** (High) | `--feature 09_modals_help` |
| 10 | `10_questions_multiple_choice.feature` | Multi-select questions | ~15 | **P1** (High) | `--feature 10_questions_multi` |
| 11 | `11_questions_single_choice.feature` | Single-select questions | ~12 | **P1** (High) | `--feature 11_questions_single` |
| 12 | `12_questions_free_text.feature` | Free text input | ~15 | **P1** (High) | `--feature 12_questions_text` |
| 13 | `13_sidebar.feature` | Sidebar panels | ~20 | **P2** (Medium) | `--feature 13_sidebar` |
| 14 | `14_theming.feature` | Theme system | ~18 | **P2** (Medium) | `--feature 14_theming` |
| 15 | `15_keyboard_shortcuts.feature` | Keyboard navigation | ~25 | **P2** (Medium) | `--feature 15_keyboard` |

**Total: 245+ test scenarios across 15 feature files**

### Feature File Location

```
tests/visual/tui/features/
├── 01_terminal_layout.feature
├── 02_status_bar.feature
├── 03_message_display.feature
├── 04_input_area.feature
├── 05_modals_provider_picker.feature
├── 06_modals_model_picker.feature
├── 07_modals_file_picker.feature
├── 08_modals_theme_picker.feature
├── 09_modals_help.feature
├── 10_questions_multiple_choice.feature
├── 11_questions_single_choice.feature
├── 12_questions_free_text.feature
├── 13_sidebar.feature
├── 14_theming.feature
├── 15_keyboard_shortcuts.feature
├── README.md
└── step_definitions/
    └── common_steps.rs
```

### Reading Feature Files

Feature files use **Gherkin syntax** (Given-When-Then):

```gherkin
@smoke @core
Scenario: Main layout renders with all sections
  Given the TUI application is running
  And the terminal has at least 80 columns and 24 rows
  Then I should see the terminal header at the top
  And I should see the message area in the center
  And I should see the input area at the bottom
  And I should see the status bar below the input area
```

**Each line maps to a step definition** in `step_definitions/common_steps.rs`.

---

## 🧪 BDD Test Framework Setup

### Prerequisites

Add cucumber-rs to your `Cargo.toml`:

```toml
[dev-dependencies]
cucumber = "0.21"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

### Create Test Harness

Create `tests/cucumber_tui.rs`:

```rust
use cucumber::{given, when, then, World};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tark::tui_new::{TuiApp, AppState};

/// Test world that renders to a REAL TestBackend
#[derive(Debug, World)]
#[world(init = Self::new)]
pub struct TuiWorld {
    /// The actual TUI application
    app: TuiApp<TestBackend>,
    /// Terminal dimensions
    terminal_size: (u16, u16),
}

impl TuiWorld {
    fn new() -> Self {
        let backend = TestBackend::new(80, 24);
        let terminal = Terminal::new(backend).unwrap();
        Self {
            app: TuiApp::new(terminal),
            terminal_size: (80, 24),
        }
    }
    
    /// Render the app and return the buffer for verification
    fn render(&mut self) -> &ratatui::buffer::Buffer {
        self.app.render().unwrap();
        self.app.terminal().backend().buffer()
    }
    
    /// Check if a specific character exists at position
    fn char_at(&mut self, x: u16, y: u16) -> &str {
        let buffer = self.render();
        buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or("")
    }
}

#[tokio::main]
async fn main() {
    TuiWorld::cucumber()
        .max_concurrent_scenarios(1)
        .run("tests/visual/tui/features/")
        .await;
}
```

### ⚠️ CRITICAL: Step Definitions Must Test REAL Rendering

**WRONG - Mock that always passes:**
```rust
#[then("I should see the terminal header at the top")]
async fn see_header(world: &mut TuiWorld) {
    assert!(true);  // ❌ USELESS - always passes!
}
```

**RIGHT - Actually verify rendered output:**
```rust
#[then("I should see the terminal header at the top")]
async fn see_header(world: &mut TuiWorld) {
    // ✅ Render and check actual buffer content
    let buffer = world.render();
    
    // Check for rounded corner at top-left
    assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "╭",
        "Expected rounded corner at top-left");
    
    // Check header text exists
    let header_line: String = (1..world.terminal_size.0-1)
        .map(|x| buffer.cell((x, 1)).unwrap().symbol())
        .collect();
    assert!(header_line.contains("Tark"), 
        "Header should contain agent name");
}
```

### Run BDD Tests

```bash
# Run all feature tests
cargo test --test cucumber_tui

# Run specific feature
cargo test --test cucumber_tui -- features/01_terminal_layout.feature

# Run tests with specific tag
cargo test --test cucumber_tui -- --tags @smoke

# Run P0 (critical) tests only
cargo test --test cucumber_tui -- --tags @core
```

### Understanding Step Definitions

Step definitions link Gherkin steps to Rust code that tests REAL rendering:

```rust
// tests/visual/tui/features/step_definitions/common_steps.rs

#[given("the TUI application is running")]
async fn app_running(world: &mut TuiWorld) {
    // App is initialized in TuiWorld::new()
    // Verify it can render without errors
    world.render();
}

#[when(regex = r#"I type "(.+)""#)]
async fn type_text(world: &mut TuiWorld, text: String) {
    // Send actual key events to the app
    for c in text.chars() {
        world.app.handle_key(KeyCode::Char(c));
    }
}

#[then("I should see the terminal header at the top")]
async fn see_header(world: &mut TuiWorld) {
    let buffer = world.render();
    // Actually verify the header is rendered
    assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "╭");
}
```

### Adding New Step Definitions

When you encounter an unimplemented step:

1. Open `common_steps.rs`
2. Add the step with appropriate attribute (`#[given]`, `#[when]`, `#[then]`)
3. **Implement the step logic that tests REAL `tark tui` rendering**
4. Wire it to the `TuiWorld` state

**⚠️ REMINDER: Step definitions must test the REAL TUI, not mocks!**

Example:

```rust
#[then(regex = r"the status bar should show (.+)")]
async fn status_bar_shows(world: &mut TuiWorld, expected: String) {
    // ✅ CORRECT: Render REAL TuiApp to TestBackend
    let buffer = world.render();  // This calls real TuiApp::render()
    let height = world.terminal_size.1;
    
    // ✅ CORRECT: Read from REAL rendered buffer
    let status_line: String = (0..world.terminal_size.0)
        .map(|x| buffer.cell((x, height - 1)).unwrap().symbol())
        .collect();
    
    assert!(status_line.contains(&expected), 
        "Expected status bar to contain '{}', got '{}'", expected, status_line);
}

// ❌ WRONG: Do NOT do this!
#[then(regex = r"the status bar should show (.+)")]
async fn status_bar_shows_WRONG(world: &mut TuiWorld, expected: String) {
    // ❌ WRONG: This tests a mock field, not real rendering!
    assert!(world.mock_status.contains(&expected));
}
```

### Self-Verification: Are Your Steps Testing Real TUI?

```bash
# Quick test to verify step definitions test real TUI:

# 1. Comment out TuiApp::render() implementation
# 2. Run cucumber tests
# 3. If tests PASS → Your steps are mocks (BROKEN!)
# 4. If tests FAIL → Your steps test real TUI (CORRECT!)

# Your steps must FAIL when TUI rendering is broken!
```

---

## 📸 Snapshot-Driven Development

Visual snapshots are your **visual acceptance criteria**. Development flow:

```
Reference Screenshots → Implementation → E2E Snapshots → Comparison → Baseline
(What to build)        (Your code)      (What you built) (Match?)     (Commit)
```

### Reference Snapshots (Source of Truth)

**Location:** `web/ui/mocks/screenshots/`

These are the **target** - what your TUI should look like:

| Screenshot | Component | Use For |
|-----------|-----------|---------|
| `01-main-layout.png` | Overall layout | Layout proportions, spacing |
| `compact-status-bar.png` | Status bar | Colors, icons, alignment |
| `03-message-types-top.png` | Messages | Message styling, icons |
| `06-provider-picker-modal.png` | Provider modal | Modal appearance |
| ... | ... | ... |

**Always keep reference screenshots visible** while implementing.

### Baseline Snapshots (Regression Detection)

**Location:** `tests/visual/tui/snapshots/`

These are **generated** from E2E test runs and committed to git:

- `terminal_layout_initial.png` - Starting state
- `terminal_layout_final.png` - After interactions
- `status_bar_initial.png` - Status bar snapshot
- ... (one per scenario)

**Purpose:** Detect unintended visual changes in future runs.

### E2E Visual Testing Workflow

```bash
# 1. Install dependencies (first time only)
./tests/visual/tui_e2e_runner.sh --install-deps

# This installs:
# - asciinema (terminal recording)
# - agg (cast → GIF converter)
# - imagemagick (PNG extraction)
# - Fonts (Unicode/Nerd Font support)

# 2. Implement a feature (e.g., terminal layout)
# ... write code based on feature file ...

# 3. Run E2E test for that feature
./tests/visual/tui_e2e_runner.sh --scenario terminal_layout

# This:
# - Builds tark with test-sim feature
# - Runs expect script to interact with TUI
# - Records terminal session with asciinema
# - Converts to GIF with agg
# - Extracts PNG snapshots with ImageMagick

# 4. Compare snapshot with reference screenshot
./tests/visual/tui_e2e_runner.sh --verify

# This compares tests/visual/tui/snapshots/*.png 
# with reference in web/ui/mocks/screenshots/

# 5. If visuals match, update baseline
./tests/visual/tui_e2e_runner.sh --update-baseline

# This copies current snapshots to baseline
# Commit these to git for future regression detection

# 6. Run regression check (CI/automated)
./tests/visual/tui_e2e_runner.sh --verify

# Detects if any visual element changed unexpectedly
```

### E2E Test Tiers

Run different test tiers based on your needs:

```bash
# P0: Smoke tests (fastest - ~2 min)
./tests/visual/tui_e2e_runner.sh --tier p0

# P1: Core tests (PRs - ~10 min)
./tests/visual/tui_e2e_runner.sh --tier p1

# P2: All tests (Release - ~30 min)
./tests/visual/tui_e2e_runner.sh --tier all
```

---

## 🏗️ CLI Command Setup

You need to add a new `tark tui` command to run the TUI:

### Step 1: Add Subcommand to `src/main.rs`

```rust
#[derive(Subcommand)]
enum Commands {
    // ... existing commands ...
    
    /// Interactive TUI mode with the AI agent
    Tui {
        /// Initial message to send
        message: Option<String>,
        
        /// Working directory
        #[arg(long)]
        cwd: Option<String>,
        
        /// Unix socket path for Neovim integration
        #[arg(long)]
        socket: Option<String>,
        
        /// LLM provider override
        #[arg(long)]
        provider: Option<String>,
        
        /// Model override
        #[arg(long)]
        model: Option<String>,
        
        /// Enable debug logging
        #[arg(long)]
        debug: bool,
    },
}
```

### Step 2: Create TUI Module

```bash
# Create new module
mkdir -p src/tui_new

# Module structure
src/tui_new/
├── mod.rs           # Public exports
├── app.rs           # TuiApp struct, main loop
├── renderer.rs      # Rendering logic
├── events.rs        # Event handling
├── config.rs        # TUI config
└── widgets/         # Ratatui widgets
    ├── mod.rs
    ├── terminal.rs
    ├── status_bar.rs
    ├── input.rs
    └── ...
```

### Step 3: Wire Command Handler

In `src/transport/cli.rs`:

```rust
pub async fn run_tui(
    initial_message: Option<String>,
    working_dir: &str,
    socket_path: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    debug: bool,
) -> Result<()> {
    // Initialize TUI app
    let mut app = TuiApp::new()?;
    
    // Run main loop
    app.run().await?;
    
    Ok(())
}
```

In `src/main.rs` match block:

```rust
Commands::Tui {
    message,
    cwd,
    socket,
    provider,
    model,
    debug,
} => {
    transport::cli::run_tui(message, &cwd.unwrap_or_else(|| ".".to_string()), 
                           socket, provider, model, debug).await?;
}
```

### Step 4: Test CLI Command

```bash
# Test help output
cargo run -- tui --help

# Should show:
# Interactive TUI mode with the AI agent
# 
# Options:
#   --cwd <CWD>
#   --socket <SOCKET>
#   --provider <PROVIDER>
#   ...
```

---

## 🧩 Implementation Phases with Test Gates

Each phase has a **test gate** - ALL feature scenarios must pass before proceeding.

### Phase 1: Core Layout ✅ Gate: `01_terminal_layout.feature` passes

**Scenarios: ~15 | Priority: P0 (Critical)**

**Implementation Tasks:**
- [ ] Create terminal frame with rounded borders (`╭─╮╰─╯`)
- [ ] Render header with agent name and path
- [ ] Create scrollable message area
- [ ] Implement input area at bottom
- [ ] Add status bar between input and messages
- [ ] Handle terminal resize events
- [ ] Implement vim-style scrolling (gg, G, j, k)

**Test Gate Command:**
```bash
cargo test --test cucumber_tui -- features/01_terminal_layout.feature
```

**E2E Verification:**
```bash
./tests/visual/tui_e2e_runner.sh --scenario terminal_layout
./tests/visual/tui_e2e_runner.sh --verify
```

**Exit Criteria:**
- ✅ All 15 scenarios in feature 01 pass
- ✅ Visual snapshot matches `01-main-layout.png`
- ✅ Layout adapts to terminal resize
- ✅ Minimum terminal size (80x24) handled

---

### Phase 2: Status Bar ✅ Gate: `02_status_bar.feature` passes

**Scenarios: ~25 | Priority: P0 (Critical)**

**Implementation Tasks:**
- [ ] Agent mode selector (Build/Plan/Ask) with dropdown
- [ ] Build mode selector (Careful/Manual/Balanced)
- [ ] Model/Provider display with click-to-change
- [ ] Thinking mode toggle (brain icon 🧠)
- [ ] Task queue indicator with count badge
- [ ] Working indicator (green blinking dot)
- [ ] Help button (? icon)

**Test Gate Command:**
```bash
cargo test --test cucumber_tui -- features/02_status_bar.feature
```

**E2E Verification:**
```bash
./tests/visual/tui_e2e_runner.sh --scenario status_bar
```

**Exit Criteria:**
- ✅ All 25 scenarios pass
- ✅ Snapshot matches `compact-status-bar.png`
- ✅ Dropdowns render and respond to keys
- ✅ Icons display correctly

---

### Phase 3: Message Display ✅ Gate: `03_message_display.feature` passes

**Scenarios: ~20 | Priority: P0 (Critical)**

**Implementation Tasks:**
- [ ] System messages (● icon, cyan color)
- [ ] User messages (👤 icon, blue bubble)
- [ ] Agent messages (🤖 icon, green bubble)
- [ ] Tool messages (🔧 icon, collapsible)
- [ ] Thinking blocks (🧠 icon, italic, collapsible)
- [ ] Command messages ($ prompt, emerald)
- [ ] Markdown rendering (code blocks, formatting)

**Test Gate Command:**
```bash
cargo test --test cucumber_tui -- features/03_message_display.feature
```

**Exit Criteria:**
- ✅ All 7 message types render correctly
- ✅ Snapshot matches `03-message-types-top.png`
- ✅ Colors match theme spec exactly
- ✅ Collapsible sections work

---

### Phase 4: Input Area ✅ Gate: `04_input_area.feature` passes

**Scenarios: ~25 | Priority: P0 (Critical)**

**Implementation Tasks:**
- [ ] Text input with cursor positioning
- [ ] Multi-line input with wrapping
- [ ] Context file tags (@mentions)
- [ ] Submission on Enter
- [ ] Clear on Escape
- [ ] Input history (Up/Down arrows)

**Test Gate Command:**
```bash
cargo test --test cucumber_tui -- features/04_input_area.feature
```

---

### Phase 5-9: Modals ✅ Gate: Features 05-09 pass

**Scenarios: ~80 | Priority: P1 (High)**

Each modal (provider, model, file, theme, help) has its own feature file.

---

### Phase 10-12: Questions ✅ Gate: Features 10-12 pass

**Scenarios: ~42 | Priority: P1 (High)**

Interactive question components with keyboard navigation.

---

### Phase 13-15: Sidebar, Themes, Shortcuts ✅ Gate: Features 13-15 pass

**Scenarios: ~63 | Priority: P2 (Medium)**

Sidebar panels, theme system, complete keyboard navigation.

---

## 📋 Quick Start Checklist (TDD Enhanced)

Follow this **TEST-FIRST** workflow for EVERY component:

- [ ] **1. READ FEATURE** - Read the feature file for this component (tests/visual/tui/features/)
- [ ] **2. RUN TEST (RED)** - Run cucumber test, verify it FAILS
- [ ] **3. VIEW SCREENSHOT** - Open reference screenshot (web/ui/mocks/screenshots/)
- [ ] **4. READ REACT CODE** - Understand component behavior (src/app/components/)
- [ ] **5. READ MAPPING** - Study RATATUI_MAPPING.md section for Rust patterns
- [ ] **6. IMPLEMENT** - Write minimal code to pass the test
- [ ] **7. RUN TEST (GREEN)** - Verify cucumber test now PASSES
- [ ] **8. E2E SNAPSHOT** - Run visual E2E test, capture snapshot
- [ ] **9. COMPARE VISUALS** - Compare snapshot with reference screenshot
- [ ] **10. REFACTOR** - Clean up code while keeping tests green
- [ ] **11. UPDATE BASELINE** - Accept snapshot as regression baseline
- [ ] **12. COMMIT** - Commit code + baseline snapshot to git

**Key Principle:** NEVER write code before having a failing test. The test defines what to build.

---

## 📖 Mandatory Reading Order

Read these files in this exact order:

### Phase 1: Understand the UI (30 minutes)

1. **SCREENSHOTS_REFERENCE.md** - Visual guide to all 49 screenshots
   - Start here to see what you're building
   - Refer back constantly during implementation

2. **src/app/components/Terminal.tsx** - Main terminal component (700+ lines)
   - Contains all message types, modals, questions, status bar
   - **READ EVERY `@ratatui-*` COMMENT** - These are implementation guides!
   - Each React component has inline Ratatui conversion instructions

3. **src/app/components/Sidebar.tsx** - Sidebar panel component
   - Session, Context, Tasks, Git Changes sections
   - **@ratatui annotations** explain widget mappings

4. **src/app/App.tsx** - Application container
   - State management and component wiring
   - **@ratatui-state annotations** show Rust struct equivalents

### 🎯 Understanding @ratatui-* Annotations

**Every React file in `web/ui/mocks/src/app/` contains inline implementation guides:**

```typescript
// Example from Terminal.tsx:

{/* @ratatui-widget: Paragraph with Block border */}
{/* @ratatui-layout: Constraint::Length(3) for header */}
{/* @ratatui-style: fg(Color::Rgb(156, 163, 175)) */}
{/* @ratatui-state: terminal_output: Vec<TerminalLine> */}
{/* @ratatui-events: KeyCode::Enter → submit_input() */}
{/* @ratatui-behavior: Scroll on Up/Down, wrap at edges */}
{/* @ratatui-pattern: See RATATUI_MAPPING.md Section 2.1 */}
```

**Annotation Types:**

| Annotation | Purpose | Example |
|------------|---------|---------|
| `@ratatui-widget:` | Which Ratatui widget to use | `List`, `Paragraph`, `Block` |
| `@ratatui-layout:` | Layout constraints | `Constraint::Length(5)` |
| `@ratatui-style:` | Styling (colors, modifiers) | `fg(Color::Rgb(...))`  |
| `@ratatui-state:` | Required state fields | `input: String` |
| `@ratatui-events:` | Event handlers needed | `KeyCode::Char → handle_input()` |
| `@ratatui-behavior:` | How element should behave | Scrolling, toggling, etc. |
| `@ratatui-pattern:` | Code example reference | Section in RATATUI_MAPPING.md |

**How to use annotations:**

1. **Read the React component** (e.g., `Terminal.tsx`)
2. **Find @ratatui-* comments** near each UI element
3. **Use them as implementation hints** for your Rust code
4. **Cross-reference with RATATUI_MAPPING.md** for full examples
5. **Implement in Rust** following the guidance

**Example workflow:**

```typescript
// In Terminal.tsx (React):
<div className="status-bar">
  {/* @ratatui-widget: Paragraph with horizontal Layout */}
  {/* @ratatui-layout: Constraint::Length(2) */}
  {/* @ratatui-state: agent_mode: AgentMode, llm_model: String */}
  <ModeSelector mode={agentMode} />
  <ModelDisplay model={currentModel} />
</div>
```

```rust
// In your Rust implementation:
// Use the @ratatui hints above to implement:

fn render_status_bar(&self, area: Rect, buf: &mut Buffer) {
    // @ratatui-widget → Paragraph
    // @ratatui-layout → Length(2)
    let chunks = Layout::horizontal([...]).split(area);
    
    // @ratatui-state → Use self.agent_mode, self.llm_model
    let mode_text = format!("{:?}", self.agent_mode);
    let model_text = &self.llm_model;
    
    // Render paragraphs...
}
```

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

### 📚 Supplementary Reading (Optional but Helpful)

These provide additional context and visual references:

10. **VISUAL_GUIDE.md** - ASCII representations of all UI elements
    - Visual diagrams of layouts and components
    - Color palette reference tables
    - Interactive workflow diagrams

11. **VISUAL_IMPROVEMENTS.md** - Before/after UX improvements
    - Animation and transition details
    - Design rationale and decisions
    - Accessibility features explained

12. **PROJECT_SUMMARY.md** - Project overview and status
    - List of completed features
    - File structure explanation
    - Testing performed summary

**Note:** While optional, these documents provide valuable context for understanding design decisions and visual expectations.

---

## 🔍 Component Verification Workflow

For each UI component, follow this protocol:

### Step 1: Visual Reference
```
→ Open SCREENSHOTS_REFERENCE.md
→ Find the screenshot(s) for this component
→ Study the visual appearance, colors, layout
```

### Step 2: React Code Analysis (with @ratatui-* Annotations)
```
→ Open the React component file (web/ui/mocks/src/app/components/)
→ Locate the component code
→ **CRITICAL:** Read ALL @ratatui-* annotations carefully
   - @ratatui-widget: tells you WHICH Ratatui widget to use
   - @ratatui-layout: tells you layout constraints
   - @ratatui-style: tells you exact colors and styling
   - @ratatui-state: tells you required state fields
   - @ratatui-events: tells you event handlers needed
   - @ratatui-behavior: tells you how it should work
   - @ratatui-pattern: points to code examples
→ Understand state variables and event handlers
→ Note all colors (exact RGB values), sizes, spacing
→ Trust annotations OVER your interpretation
```

**The @ratatui-* annotations are YOUR IMPLEMENTATION GUIDE.**

If the annotation says `@ratatui-widget: List`, use `List`. If it says `@ratatui-style: fg(Color::Rgb(103, 232, 249))`, use that exact color. Don't guess or approximate.

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

### Step 7: Verification (Enhanced with TDD)
```
BDD Tests:
- [ ] All feature scenarios pass (cucumber)
- [ ] No step definitions missing or pending

Visual Tests:
- [ ] E2E snapshot captured successfully
- [ ] Snapshot matches reference screenshot
- [ ] Baseline updated and committed

Code Quality:
- [ ] Interactive behavior matches React code
- [ ] All @ratatui annotations addressed
- [ ] Edge cases handled (empty, overflow, resize)
- [ ] No unwanted side effects
- [ ] Code is refactored and clean
```

---

## 🔬 E2E Visual Testing Framework

The visual testing framework ensures your TUI looks exactly like the reference screenshots.

### Framework Components

```
┌─────────────────────────────────────────────────────────┐
│  E2E Visual Testing Pipeline                            │
└─────────────────────────────────────────────────────────┘

1. asciinema     → Record terminal session as .cast file
2. expect        → Automate keyboard input/interaction
3. agg           → Convert .cast → GIF with proper fonts
4. imagemagick   → Extract PNG snapshots from GIF
5. compare       → Detect visual differences (RMSE metric)
```

### Installation

```bash
# First-time setup (installs all tools + fonts)
cd tests/visual
./tui_e2e_runner.sh --install-deps

# This installs:
# - asciinema (terminal recorder)
# - agg (cast to GIF with Unicode/Nerd Font support)
# - imagemagick (PNG extraction and comparison)
# - expect (automated interaction)
# - Fonts: Noto Color Emoji, Symbola (for icons)
```

### Running E2E Tests

```bash
# Run P0 (smoke) tests - fastest (~2 min)
./tui_e2e_runner.sh --tier p0

# Run P1 (core) tests - for PRs (~10 min)
./tui_e2e_runner.sh --tier p1

# Run all tests - for releases (~30 min)
./tui_e2e_runner.sh --tier all

# Run specific scenario
./tui_e2e_runner.sh --scenario terminal_layout

# List available scenarios
./tui_e2e_runner.sh --list
```

### Visual Regression Detection

```bash
# Compare current snapshots with baseline
./tui_e2e_runner.sh --verify

# Output shows:
# ✅ Match: terminal_layout_final.png (diff: 0.02)
# ❌ Visual diff: status_bar_final.png (diff: 0.15)
#
# Visual diffs saved to: tests/visual/tui/diffs/

# Accept current as new baseline (after visual review)
./tui_e2e_runner.sh --update-baseline
```

### Understanding Test Output

**⚠️ CRITICAL: Per-Feature Directory Structure**

Each feature MUST have its own directory in both `snapshots/` and `recordings/`:

```
tests/visual/tui/
├── snapshots/                              # ✅ COMMITTED to git
│   ├── 01_terminal_layout/                 # Feature 01 directory
│   │   ├── main_layout_initial.png
│   │   ├── main_layout_final.png
│   │   ├── header_rendering.png
│   │   ├── scroll_top.png
│   │   └── scroll_bottom.png
│   ├── 02_status_bar/                      # Feature 02 directory
│   │   ├── mode_build.png
│   │   ├── mode_plan.png
│   │   ├── mode_ask.png
│   │   └── working_indicator.png
│   ├── 03_message_display/
│   │   ├── system_message.png
│   │   ├── user_message.png
│   │   ├── agent_message.png
│   │   ├── tool_message.png
│   │   ├── thinking_block.png
│   │   └── command_message.png
│   ├── ... (04-15 feature directories)
│   └── complete_feature_test/              # 🎯 FINAL COMPLETE TEST
│       └── complete_demo_final.png
│
├── recordings/                             # ✅ COMMITTED to git
│   ├── 01_terminal_layout/
│   │   ├── layout_demo.gif
│   │   └── scroll_demo.gif
│   ├── 02_status_bar/
│   │   └── mode_switching.gif
│   ├── ... (04-15 feature directories)
│   └── complete_feature_test/              # 🎯 FINAL COMPLETE TEST
│       ├── complete_feature_test.cast      # Raw asciinema
│       └── complete_feature_test.gif       # Full demo (~2-3 min)
│
├── current/                                # ❌ GITIGNORED (latest run)
│   └── ... (same structure, for comparison)
│
└── diffs/                                  # ❌ GITIGNORED (visual diffs)
    └── diff_status_bar_mode_build.png
```

### Verification After Each Feature

```bash
# After implementing feature 02 (status_bar):

# 1. Run E2E for this feature
./tests/visual/tui_e2e_runner.sh --scenario 02_status_bar

# 2. Verify directories created
ls tests/visual/tui/snapshots/02_status_bar/
# Expected: mode_build.png, mode_plan.png, ...

ls tests/visual/tui/recordings/02_status_bar/
# Expected: mode_switching.gif

# 3. If directories are EMPTY or MISSING → E2E runner is broken!
```

### Creating New E2E Scenarios

To add a new scenario (e.g., for a new feature):

1. **Add scenario to `tui_e2e_runner.sh`:**

```bash
# In get_tier_scenarios() function
p1|core)
    echo "terminal_layout status_bar messages input YOUR_NEW_SCENARIO"
    ;;
```

2. **Create expect script:**

```bash
# In generate_expect_script() function
YOUR_NEW_SCENARIO)
    cat > "$script_file" << 'EXPEOF'
#!/usr/bin/expect -f
set timeout 30
spawn ./target/release/tark tui --provider tark_sim
sleep 3
# ... interaction commands ...
send "/exit\r"
expect eof
EXPEOF
    ;;
```

3. **Run and capture baseline:**

```bash
./tui_e2e_runner.sh --scenario YOUR_NEW_SCENARIO
./tui_e2e_runner.sh --update-baseline
git add tests/visual/tui/snapshots/YOUR_NEW_SCENARIO_*.png
git commit -m "Add E2E baseline for YOUR_NEW_SCENARIO"
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

## 🏗️ Implementation Priority Order (Test-Gated)

Implement in this order. **Each phase has a test gate - all scenarios MUST pass before proceeding.**

### Phase 1: Core Terminal ✅ Test Gate: Features 01, 04 pass (P0)

**Duration:** ~3-5 days | **Scenarios:** 40

1. **Terminal Layout** (Feature 01: 15 scenarios)
   - Main container with rounded borders
   - Header with agent name + path
   - Scrollable message area
   - Status bar + input area
   - **Gate:** `cargo test --test cucumber_tui -- features/01_terminal_layout.feature`
   - **Snapshot:** `terminal_layout_*.png` matches `01-main-layout.png`

2. **Input Area** (Feature 04: 25 scenarios)
   - Text input with cursor
   - Multi-line wrapping
   - Context file tags
   - **Gate:** `cargo test --test cucumber_tui -- features/04_input_area.feature`

**Exit Criteria:**
- ✅ 40 cucumber scenarios pass
- ✅ E2E snapshots match reference
- ✅ `./tui_e2e_runner.sh --tier p0` passes

---

### Phase 2: Status Bar & Messages ✅ Test Gate: Features 02, 03 pass (P0)

**Duration:** ~4-6 days | **Scenarios:** 45

3. **Status Bar** (Feature 02: 25 scenarios)
   - Mode selectors (agent, build)
   - Model display
   - Indicators (working, queue, thinking)
   - **Gate:** `cargo test --test cucumber_tui -- features/02_status_bar.feature`
   - **Snapshot:** `compact-status-bar.png`

4. **Message Display** (Feature 03: 20 scenarios)
   - All 7 message types
   - Markdown rendering
   - Collapsible sections
   - **Gate:** `cargo test --test cucumber_tui -- features/03_message_display.feature`
   - **Snapshot:** `03-message-types-top.png`

**Exit Criteria:**
- ✅ 45 cucumber scenarios pass
- ✅ Visual accuracy: exact color matching
- ✅ All message types render correctly

---

### Phase 3: Modals ✅ Test Gate: Features 05-09 pass (P1)

**Duration:** ~5-7 days | **Scenarios:** 80

5-9. **System Modals** (Features 05-09: ~16 scenarios each)
   - Provider picker (Feature 05)
   - Model picker (Feature 06)
   - File picker (Feature 07)
   - Theme picker (Feature 08)
   - Help modal (Feature 09)
   - **Gate:** All 5 features pass
   - **Snapshots:** `06-*.png`, `07-*.png`, `08-*.png`, `09-*.png`, `10-*.png`

**Exit Criteria:**
- ✅ 80 cucumber scenarios pass
- ✅ Modal keyboard navigation works
- ✅ Filter/search functionality works

---

### Phase 4: Questions ✅ Test Gate: Features 10-12 pass (P1)

**Duration:** ~3-4 days | **Scenarios:** 42

10-12. **Interactive Questions** (Features 10-12: ~14 scenarios each)
   - Multiple choice (Feature 10)
   - Single choice (Feature 11)
   - Free text (Feature 12)
   - **Gate:** All question types pass
   - **Snapshot:** `12-question-single-selected.png`

**Exit Criteria:**
- ✅ 42 cucumber scenarios pass
- ✅ Selection/submission works correctly
- ✅ Answer display matches spec

---

### Phase 5: Sidebar, Themes, Shortcuts ✅ Test Gate: Features 13-15 pass (P2)

**Duration:** ~5-7 days | **Scenarios:** 63

13-15. **Advanced Features** (Features 13-15)
   - Sidebar with panels (Feature 13: 20 scenarios)
   - Theme system (Feature 14: 18 scenarios)
   - Keyboard shortcuts (Feature 15: 25 scenarios)
   - **Gate:** All 3 features pass
   - **Snapshots:** `11-sidebar-panel.png`, theme screenshots

**Exit Criteria:**
- ✅ 63 cucumber scenarios pass
- ✅ All 7 themes work correctly
- ✅ Complete keyboard navigation
- ✅ `./tui_e2e_runner.sh --tier all` passes

---

## 🎯 Test Coverage Summary

| Phase | Features | Scenarios | Priority | Duration | Gate |
|-------|----------|-----------|----------|----------|------|
| 1 | 01, 04 | 40 | **P0** | 3-5 days | `--tier p0` passes |
| 2 | 02, 03 | 45 | **P0** | 4-6 days | Core tests pass |
| 3 | 05-09 | 80 | **P1** | 5-7 days | All modals pass |
| 4 | 10-12 | 42 | **P1** | 3-4 days | Questions pass |
| 5 | 13-15 | 63 | **P2** | 5-7 days | `--tier all` passes |
| **Total** | **15 files** | **270** | | **20-29 days** | **All green** |

---

## ⚠️ Critical Guidelines (TDD Enhanced)

### DO: Test-First Development
- ✅ **ALWAYS start with RED test** - Run cucumber test, verify it fails
- ✅ **Write minimal code** - Just enough to make test pass (GREEN)
- ✅ **Refactor with confidence** - Tests ensure nothing breaks
- ✅ **Commit baselines** - E2E snapshots go into git for regression detection
- ✅ **Run tests frequently** - After every small change
- ✅ **Use feature files as spec** - They define ALL requirements (IMMUTABLE)
- ✅ **Follow @ratatui-* annotations** - React files have implementation guides
- ✅ **Compare snapshots** - Visual accuracy is mandatory
- ✅ **Read React source** - Understand behavior from code + annotations
- ✅ **Follow exact colors** - Use RGB values from theme.css and @ratatui-style
- ✅ **Handle edge cases** - Empty lists, long text, narrow terminals

### DON'T: Anti-Patterns
- ❌ **Write code before test** - Violates TDD principle
- ❌ **Skip test verification** - "It looks right" is not enough
- ❌ **Ignore failing tests** - Fix immediately, don't accumulate debt
- ❌ **MODIFY FEATURE FILES** - They are IMMUTABLE acceptance criteria!
- ❌ **Ignore @ratatui-* annotations** - They tell you HOW to implement
- ❌ **Approximate visuals** - Snapshots must match reference exactly
- ❌ **Skip E2E tests** - Unit tests alone don't catch visual regressions
- ❌ **Commit without baselines** - Baselines are part of the code
- ❌ **Use wrong colors** - RGB values from @ratatui-style annotations are exact
- ❌ **Implement ahead** - Follow phase order, respect test gates
- ❌ **Skip React annotations** - Missing critical implementation details

### Test-Driven Mindset

```
❌ WRONG: "I'll write the feature, then add tests later"
✅ RIGHT: "Let me read the feature file, run the test (RED), then implement"

❌ WRONG: "Tests are passing, ship it!"
✅ RIGHT: "Tests pass AND snapshot matches reference - now ship it"

❌ WRONG: "The visual diff is small, good enough"
✅ RIGHT: "Even small diffs indicate a problem - investigate and fix"

❌ WRONG: "I'll batch all features then test"
✅ RIGHT: "One feature at a time, test-first, gate before proceeding"

❌ WRONG: "This feature file scenario is wrong, let me fix it"
✅ RIGHT: "Feature file is acceptance criteria - I must make my code pass it"

❌ WRONG: "I'll ignore the @ratatui-* annotations, I know better"
✅ RIGHT: "@ratatui-* annotations are implementation guides - follow them"
```

---

## 🔧 Required Tools & Setup (TDD Stack)

Before starting, ensure you have the **complete TDD stack**:

### Core Development Tools

1. **Rust & Cargo** - Latest stable version (1.75+)
   ```bash
   rustc --version  # Should be 1.75+
   ```

2. **Ratatui** - v0.26+ (check Cargo.toml)
3. **Crossterm** - For event handling
4. **Arboard** - For clipboard support

### BDD Testing Tools

5. **cucumber-rs** - v0.21+ (BDD test framework)
   ```toml
   [dev-dependencies]
   cucumber = "0.21"
   ```

6. **tokio** - Async runtime for cucumber
   ```toml
   tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
   ```

### E2E Visual Testing Tools

7. **asciinema** - Terminal session recorder
   ```bash
   # macOS
   brew install asciinema
   
   # Linux (Debian/Ubuntu)
   sudo apt-get install asciinema
   ```

8. **agg** - Asciinema GIF generator with font support
   ```bash
   cargo install agg
   ```

9. **ImageMagick** - PNG extraction and comparison
   ```bash
   # macOS
   brew install imagemagick
   
   # Linux
   sudo apt-get install imagemagick
   ```

10. **expect** - Automated terminal interaction
    ```bash
    # macOS
    brew install expect
    
    # Linux
    sudo apt-get install expect
    ```

### Font Support (for proper icon rendering)

11. **Nerd Font** - For icon display
    - Recommended: FiraCode Nerd Font, JetBrainsMono Nerd Font
    - Download: https://www.nerdfonts.com/

12. **Unicode Fonts** - For emoji and symbols
    ```bash
    # Linux
    sudo apt-get install fonts-noto-color-emoji fonts-symbola
    fc-cache -f
    ```

### One-Command Setup

```bash
# Install ALL E2E testing dependencies
cd tests/visual
./tui_e2e_runner.sh --install-deps

# This installs:
# - asciinema, agg, imagemagick, expect
# - Unicode/Nerd fonts (Linux)
# - Font cache refresh
```

### Terminal Requirements

13. **Terminal with true color** - 24-bit color support
14. **Minimum size** - 80x24 characters (120x40 recommended)

Test your terminal setup:
```bash
# Check true color support
printf "\x1b[38;2;255;100;0mTRUECOLOR\x1b[0m\n"

# Test Nerd Font icons (should show distinct icons)
echo "     "

# Test Unicode symbols
echo "● ◉ ○ ☐ ☑ ▶ ▼ ╭─╮╰─╯"
```

### Verify Complete Setup

```bash
# 1. Check Rust tools
cargo --version
cargo test --version

# 2. Check E2E tools
asciinema --version
agg --version
convert --version  # ImageMagick
expect -version

# 3. Run setup verification
./tests/visual/tui_e2e_runner.sh --list

# Should show available scenarios without errors
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

## 🆘 Troubleshooting (TDD Edition)

### "Cucumber test fails with 'step not found'"
→ The step definition doesn't exist yet
→ Open `tests/visual/tui/features/step_definitions/common_steps.rs`
→ Add the step with `#[given]`, `#[when]`, or `#[then]` attribute
→ Implement the step logic
→ Wire it to `TuiWorld` state

### "Test passes but shouldn't (false positive)"
→ The test is not asserting correctly
→ Review the step definition implementation
→ Add more specific assertions
→ Check that you're testing actual app state, not just mocks

### "E2E snapshot doesn't match reference"
→ **First:** Review the visual diff in `tests/visual/tui/diffs/`
→ If diff is intentional (new feature): Update reference screenshot
→ If diff is unintentional: Fix implementation to match reference
→ Run `./tui_e2e_runner.sh --verify` after fix

### "asciinema/agg not found"
→ Run `./tests/visual/tui_e2e_runner.sh --install-deps`
→ Ensure `~/.cargo/bin` is in PATH for agg
→ On Linux: Install font packages for proper rendering

### "Test hangs or times out"
→ Check for missing `expect eof` in expect scripts
→ Increase timeout in expect script: `set timeout 60`
→ Verify TUI has an exit path (`/exit` command or Ctrl+C)

### "Colors don't match screenshot"
→ Check RGB values in theme.css match exactly
→ Verify terminal supports true color: `echo $COLORTERM`
→ Set `COLORTERM=truecolor` if needed
→ Compare hex codes: screenshot vs implementation

### "Feature file scenario is ambiguous"
→ Read the scenario's `@tag` for context
→ Check RATATUI_MAPPING.md for the component section
→ Look at reference screenshot mentioned in feature file
→ If still unclear: Note it, implement best interpretation, verify in review

### "I can't find a function in the Ratatui codebase"
→ Use `grep -r "fn function_name"` or similar search
→ Look for related functionality with different names
→ Check if it's in a trait implementation
→ Document as "NONE" and create it per RATATUI_MAPPING.md

### "The React code doesn't match the annotations"
→ **Trust the React code**, not the annotations
→ Feature files take precedence over both
→ Document the discrepancy
→ Implement based on: Feature file > React code > Annotations

### "The screenshot looks different from the code"
→ **The screenshot is the source of truth**
→ Check if it's a theme-specific appearance
→ Look for dynamic styling based on state
→ Implement what you see in the screenshot

### "All tests pass but visual doesn't match"
→ This indicates incomplete test coverage
→ Add more specific visual assertions to cucumber tests
→ Ensure E2E snapshots are being compared correctly
→ Review step definitions for correctness

### "I don't understand a Ratatui concept"
→ Check RATATUI_MAPPING.md for examples
→ Read official Ratatui documentation: https://ratatui.rs/
→ Look at existing code in the codebase
→ Feature files show the WHAT, RATATUI_MAPPING shows the HOW

---

## 🎓 Learning Resources

- **Ratatui Book**: https://ratatui.rs/
- **Crossterm Docs**: https://docs.rs/crossterm/
- **This Codebase**:
  - KICKSTART.md (this file) - TDD workflow and complete guide
  - RATATUI_MAPPING.md - Complete widget mapping
  - AGENT_INSTRUCTIONS.md - Best practices
  - SCREENSHOTS_REFERENCE.md - Visual guide (49 screenshots)
  - VISUAL_GUIDE.md - ASCII diagrams and color palette
  - VISUAL_IMPROVEMENTS.md - UX improvements and design rationale
  - PROJECT_SUMMARY.md - Project overview and feature status

---

## ✅ Final Checklist Before Completion (Test-Verified)

Before marking the implementation complete, ALL of these MUST be checked:

### BDD Test Coverage
- [ ] **All 15 feature files** pass completely (270 scenarios)
- [ ] **All step definitions** implemented (no pending/undefined steps)
- [ ] `cargo test --test cucumber_tui` passes with 0 failures
- [ ] **P0 tests** (features 01-04) pass: `--tier p0`
- [ ] **P1 tests** (features 05-12) pass: `--tier p1`
- [ ] **P2 tests** (features 13-15) pass: `--tier all`

### E2E Visual Coverage
- [ ] **All E2E scenarios** run successfully (no crashes/hangs)
- [ ] **All snapshots** captured without errors
- [ ] **Visual comparison** passes: `./tui_e2e_runner.sh --verify`
- [ ] **Baseline snapshots** committed to git
- [ ] **Reference screenshots** (all 49) reviewed and matched
- [ ] **RMSE diff** < 0.1 for all snapshot comparisons

### Functional Coverage
- [ ] **All 7 message types** render correctly (system, user, agent, tool, thinking, question, command)
- [ ] **All 5 modals** work (provider, model, file, theme, help)
- [ ] **All 3 question types** work (single, multi, free text)
- [ ] **All keyboard shortcuts** functional (see feature 15)
- [ ] **All 7 themes** implemented and tested
- [ ] **Sidebar panels** work (session, context, tasks, git)
- [ ] **Mode selectors** work (agent mode, build mode)
- [ ] **Indicators** work (working, queue, thinking)

### Code Quality
- [ ] **No crashes or panics** in any scenario
- [ ] **Error handling** comprehensive (no unwrap() in prod code)
- [ ] **Performance** acceptable (< 16ms frame time, smooth scrolling)
- [ ] **Code documented** with comments explaining complex logic
- [ ] **Terminal resize** handled gracefully
- [ ] **Edge cases** covered (empty lists, long text, narrow terminals)

### Documentation
- [ ] **README** updated with `tark tui` command
- [ ] **Build instructions** include test-sim feature
- [ ] **Test documentation** explains how to run BDD + E2E tests
- [ ] **Baseline management** process documented

### Git Hygiene
- [ ] **All code** committed with clear commit messages
- [ ] **All baseline snapshots** committed (tests/visual/tui/snapshots/)
- [ ] **No gitignored test files** accidentally committed
- [ ] **CI pipeline** passes (if applicable)

### Final Validation

Run this command sequence to validate everything:

```bash
# 1. Clean build
cargo clean
cargo build --release --features test-sim

# 2. Run all BDD tests against REAL tark tui
cargo test --test cucumber_tui

# 3. Run all E2E tests
cd tests/visual
./tui_e2e_runner.sh --tier all

# 4. Verify visual regression
./tui_e2e_runner.sh --verify

# 5. Manual smoke test
cd ../..
./target/release/tark tui --provider tark_sim

# All should pass with no errors
```

---

## 🎬 REQUIRED: Complete Feature Recording

**After ALL 15 features are implemented and tested, create a COMPLETE feature demonstration:**

### Complete Recording Requirements

```bash
# After ALL features pass (features 01-15):

# 1. Create complete demonstration recording
./tests/visual/tui_e2e_runner.sh --scenario complete_feature_test

# 2. This recording MUST demonstrate ALL features:
#    - Terminal layout (01)
#    - Status bar interactions (02)
#    - All 7 message types (03)
#    - Input area with context files (04)
#    - All 5 modals (05-09)
#    - All 3 question types (10-12)
#    - Sidebar panels (13)
#    - Theme switching (14)
#    - Keyboard shortcuts (15)

# 3. Verify complete recording exists
ls -la tests/visual/tui/recordings/complete_feature_test/
# Should contain:
#   - complete_feature_test.gif (full demo ~2-3 min)
#   - complete_feature_test.cast (raw asciinema recording)
```

### Complete Recording Structure

```
tests/visual/tui/recordings/
├── 01_terminal_layout/          # Individual feature recordings
├── 02_status_bar/
├── ...
├── 15_keyboard_shortcuts/
└── complete_feature_test/       # 🎯 FINAL COMPLETE RECORDING
    ├── complete_feature_test.cast   # Raw asciinema
    ├── complete_feature_test.gif    # Animated demo
    └── complete_feature_test_summary.png  # Final state snapshot
```

### Complete Recording Script

The `complete_feature_test` scenario in `tui_e2e_runner.sh` should:

```bash
# Example expect script for complete_feature_test
#!/usr/bin/expect -f
set timeout 180

spawn ./target/release/tark tui --provider tark_sim
sleep 2

# Feature 01: Terminal layout
send "Hello, testing terminal layout\r"
sleep 2

# Feature 02: Status bar
send "\x01m"  # Ctrl+A m for mode selector
sleep 1
send "\x1b"   # Escape to close

# Feature 03: Messages - send various types
send "Show me different message types\r"
sleep 3

# Feature 04: Input area
send "@file.txt Test context file\r"
sleep 2

# Feature 05-09: Modals
send "/provider\r"
sleep 1
send "\x1b"
send "/model\r"
sleep 1
send "\x1b"
send "/theme\r"
sleep 1
send "j"  # Navigate down
send "\r" # Select theme
sleep 1
send "/help\r"
sleep 2
send "\x1b"

# Feature 10-12: Questions (triggered by LLM sim)
# ... interaction with question responses ...

# Feature 13: Sidebar
send "\x01b"  # Ctrl+A b for sidebar toggle
sleep 1
send "\x01b"

# Feature 14: Theme switching (already done above)

# Feature 15: Keyboard shortcuts demo
send "gg"     # Scroll to top
sleep 0.5
send "G"      # Scroll to bottom
sleep 0.5
send "?"      # Help
sleep 1
send "\x1b"

# Exit
send "/exit\r"
expect eof
```

### Commit Complete Recording

```bash
# After complete recording is captured:
git add tests/visual/tui/recordings/complete_feature_test/
git commit -m "feat(tui): add complete feature demonstration recording

- All 15 features demonstrated in sequence
- 270 BDD scenarios passing
- Visual regression tests green
- Complete TUI implementation verified"
```

### ✅ Definition of Done

**The TUI implementation is COMPLETE when:**

1. ✅ All 270 BDD scenarios pass against REAL `tark tui`
2. ✅ All 15 feature directories exist in `snapshots/` with PNGs
3. ✅ All 15 feature directories exist in `recordings/` with GIFs
4. ✅ `complete_feature_test/` recording demonstrates all features
5. ✅ Visual regression passes (RMSE < 0.1)
6. ✅ All artifacts committed to git

---

## 🚀 Ready to Start? (TDD Quickstart)

### Step 1: Setup (10 minutes)

```bash
# 1. Install testing tools
cd tests/visual
./tui_e2e_runner.sh --install-deps

# 2. Add cucumber to Cargo.toml
# (See "BDD Test Framework Setup" section above)

# 3. Create cucumber test harness
# (See "BDD Test Framework Setup" section above)

# 4. Verify setup
cargo test --test cucumber_tui --help
./tui_e2e_runner.sh --list
```

### Step 2: Read Documentation (30 minutes)

1. **SCREENSHOTS_REFERENCE.md** - See what you're building (49 screenshots)
2. **Feature files** - Read `tests/visual/tui/features/01_terminal_layout.feature`
3. **RATATUI_MAPPING.md** - Understand Rust patterns
4. **AGENT_INSTRUCTIONS.md** - Best practices

### Step 3: Start TDD Cycle (First feature)

```bash
# 1. READ: Open feature file
cat tests/visual/tui/features/01_terminal_layout.feature

# 2. RED: Run test (should fail - not implemented)
cargo test --test cucumber_tui -- features/01_terminal_layout.feature

# 3. IMPLEMENT: Create src/tui_new/mod.rs, app.rs, etc.
#    - Reference: RATATUI_MAPPING.md Section 2.1
#    - Screenshot: web/ui/mocks/screenshots/01-main-layout.png

# 4. GREEN: Run test again (should pass)
cargo test --test cucumber_tui -- features/01_terminal_layout.feature

# 5. SNAPSHOT: Capture E2E visual
./tests/visual/tui_e2e_runner.sh --scenario terminal_layout

# 6. COMPARE: Check visual match
./tests/visual/tui_e2e_runner.sh --verify

# 7. REFACTOR: Clean up code

# 8. BASELINE: Accept snapshot
./tests/visual/tui_e2e_runner.sh --update-baseline
git add tests/visual/tui/snapshots/terminal_layout_*.png
git commit -m "feat: implement terminal layout (feature 01)"
```

### Step 4: Repeat for Each Feature

Follow the TDD cycle for features 02-15, respecting phase gates.

---

## 📚 Documentation Reference

| Document | Purpose | When to Read | Key Feature |
|----------|---------|--------------|-------------|
| **KICKSTART.md** (this file) | TDD workflow, overview | **First** - Start here | Complete workflow guide |
| **Feature files** (tests/visual/tui/features/) | **IMMUTABLE** acceptance criteria | **Before each component** | ⚠️ DO NOT MODIFY |
| **React source** (web/ui/mocks/src/app/components/) | Behavior + implementation guides | **During implementation** | **@ratatui-* annotations** |
| **SCREENSHOTS_REFERENCE.md** | Visual reference guide | **During implementation** | 49 reference images |
| **RATATUI_MAPPING.md** | Rust implementation patterns | **During implementation** | Code examples |
| **AGENT_INSTRUCTIONS.md** | Best practices, do's/don'ts | **Before starting** | Verification protocols |
| **PROJECT_SUMMARY.md** | Project overview, features, status | **Optional** - For context | Completed features list |
| **VISUAL_GUIDE.md** | ASCII representations, color palette | **Optional** - Helpful reference | UI element diagrams |
| **VISUAL_IMPROVEMENTS.md** | Before/after, UX improvements | **Optional** - Design context | Animation details |

### Critical Reading Order

1. **Feature file** (e.g., `01_terminal_layout.feature`) → Defines WHAT to build
2. **React source** (e.g., `Terminal.tsx`) → Shows HOW it works + @ratatui-* annotations
3. **Screenshot** (e.g., `01-main-layout.png`) → Shows what it LOOKS like
4. **RATATUI_MAPPING.md** → Rust code examples
5. **Implement** → Write Rust code following all above

**Never skip the React files!** The `@ratatui-*` annotations are essential implementation details.

---

## 🎯 Success Metrics

You've successfully completed the TUI when:

✅ **All 270 scenarios pass** (15 feature files, 100% green)
✅ **All E2E visual tests pass** (RMSE < 0.1 for all snapshots)
✅ **All 49 reference screenshots matched** (visual accuracy verified)
✅ **Manual testing smooth** (no crashes, good performance)
✅ **Code quality high** (documented, refactored, no tech debt)
✅ **Git history clean** (baselines committed, clear commits)

---

## 💡 TDD Philosophy

**Remember these principles:**

1. **Tests define requirements** - Feature files ARE the spec
2. **Red before green** - Always see the test fail first
3. **Small steps** - One scenario at a time
4. **Refactor fearlessly** - Tests protect you
5. **Snapshots verify visuals** - Not just "looks good to me"
6. **Commit baselines** - They're part of the codebase
7. **Respect gates** - Don't skip ahead

**The TDD cycle ensures:**
- ✅ You build exactly what's needed (no more, no less)
- ✅ You know when you're done (all tests green)
- ✅ You catch regressions immediately (baselines)
- ✅ You can refactor safely (tests as safety net)

---

**Ready? Start with `01_terminal_layout.feature` and follow the TDD cycle!** 🚀

Good luck! The tests will guide you. 🎉
