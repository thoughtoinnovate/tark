//! TUI Snapshot Tests using insta
//!
//! These tests use insta for fast visual regression testing.
//! They capture the rendered TUI buffer as text and compare against saved snapshots.
//!
//! Run: cargo test --test tui_snapshot_tests
//! Review changes: cargo insta review
//! Accept changes: cargo insta accept

mod tui_test_driver;

use insta::assert_snapshot;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tark_cli::tui_new::TuiApp;

/// Create a test app with specified size
fn create_test_app(width: u16, height: u16) -> TuiApp<TestBackend> {
    let backend = TestBackend::new(width, height);
    let terminal = Terminal::new(backend).unwrap();
    let mut app = TuiApp::new(terminal);
    app.state_mut().set_terminal_size(width, height);
    app
}

/// Capture buffer as string for snapshot
fn capture_buffer(app: &mut TuiApp<TestBackend>) -> String {
    app.render().unwrap();
    let buf = app.terminal().backend().buffer();
    let area = buf.area();
    
    let mut result = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            result.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        result.push('\n');
    }
    result
}

// ============================================================================
// BASIC LAYOUT SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_initial_layout() {
    let mut app = create_test_app(100, 30);
    assert_snapshot!("initial_layout", capture_buffer(&mut app));
}

#[test]
fn snapshot_narrow_terminal() {
    let mut app = create_test_app(80, 24);
    assert_snapshot!("narrow_terminal", capture_buffer(&mut app));
}

#[test]
fn snapshot_wide_terminal_with_sidebar() {
    let mut app = create_test_app(120, 30);
    // Sidebar should be visible on wide terminals
    assert_snapshot!("wide_with_sidebar", capture_buffer(&mut app));
}

// ============================================================================
// SIDEBAR SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_sidebar_hidden() {
    let mut app = create_test_app(120, 30);
    // Toggle sidebar off
    app.state_mut().sidebar_visible = false;
    assert_snapshot!("sidebar_hidden", capture_buffer(&mut app));
}

#[test]
fn snapshot_sidebar_visible() {
    let mut app = create_test_app(120, 30);
    app.state_mut().sidebar_visible = true;
    assert_snapshot!("sidebar_visible", capture_buffer(&mut app));
}

// ============================================================================
// MODE SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_mode_build() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_agent_mode(tark_cli::core::types::AgentMode::Build);
    assert_snapshot!("mode_build", capture_buffer(&mut app));
}

#[test]
fn snapshot_mode_plan() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_agent_mode(tark_cli::core::types::AgentMode::Plan);
    assert_snapshot!("mode_plan", capture_buffer(&mut app));
}

#[test]
fn snapshot_mode_ask() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_agent_mode(tark_cli::core::types::AgentMode::Ask);
    assert_snapshot!("mode_ask", capture_buffer(&mut app));
}

// ============================================================================
// INPUT SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_with_input_text() {
    let mut app = create_test_app(100, 30);
    app.state_mut().insert_str("Hello, how are you?");
    assert_snapshot!("with_input_text", capture_buffer(&mut app));
}

#[test]
fn snapshot_empty_input() {
    let mut app = create_test_app(100, 30);
    app.state_mut().clear_input();
    assert_snapshot!("empty_input", capture_buffer(&mut app));
}

// ============================================================================
// THEME SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_catppuccin_mocha() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_theme(tark_cli::tui_new::ThemePreset::CatppuccinMocha);
    assert_snapshot!("theme_catppuccin_mocha", capture_buffer(&mut app));
}

#[test]
fn snapshot_nord_theme() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_theme(tark_cli::tui_new::ThemePreset::Nord);
    assert_snapshot!("theme_nord", capture_buffer(&mut app));
}

// ============================================================================
// MESSAGE SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_with_welcome_message() {
    let mut app = create_test_app(100, 30);
    // Default app has welcome messages
    assert_snapshot!("with_welcome_message", capture_buffer(&mut app));
}

// ============================================================================
// COMBINED STATE SNAPSHOTS
// ============================================================================

#[test]
fn snapshot_plan_mode_with_input() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_agent_mode(tark_cli::core::types::AgentMode::Plan);
    app.state_mut().insert_str("Create a test plan");
    assert_snapshot!("plan_mode_with_input", capture_buffer(&mut app));
}

#[test]
fn snapshot_ask_mode_with_question() {
    let mut app = create_test_app(100, 30);
    app.state_mut().set_agent_mode(tark_cli::core::types::AgentMode::Ask);
    app.state_mut().insert_str("What is rust?");
    assert_snapshot!("ask_mode_with_question", capture_buffer(&mut app));
}
