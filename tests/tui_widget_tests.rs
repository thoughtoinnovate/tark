//! TUI Widget Unit Tests
//!
//! Tests individual widgets in isolation
//!
//! Run: cargo test --test tui_widget_tests

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use ratatui::layout::Rect;
use tark_cli::tui_new::widgets::*;
use tark_cli::tui_new::theme::Theme;

/// Helper to render a widget and capture buffer
fn render_widget<W>(widget: W, width: u16, height: u16) -> String
where
    W: ratatui::widgets::Widget,
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    
    terminal.draw(|f| {
        let area = Rect {
            x: 0,
            y: 0,
            width,
            height,
        };
        f.render_widget(widget, area);
    }).unwrap();
    
    let buf = terminal.backend().buffer();
    let mut result = String::new();
    for y in 0..height {
        for x in 0..width {
            result.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        result.push('\n');
    }
    result
}

// ============================================================================
// INPUT WIDGET TESTS
// ============================================================================

#[test]
fn test_input_widget_with_text() {
    let theme = Theme::default();
    let widget = InputWidget::new("Hello world", 5, &theme);
    let output = render_widget(widget, 80, 3);
    
    assert!(output.contains("Hello world"), "Input should display text");
}

#[test]
fn test_input_widget_empty() {
    let theme = Theme::default();
    let widget = InputWidget::new("", 0, &theme);
    let output = render_widget(widget, 80, 3);
    
    // Should have some kind of border or prompt
    assert!(output.contains("│") || output.contains("┌") || output.contains("─"));
}

// ============================================================================
// STATUS BAR TESTS
// ============================================================================

#[test]
fn test_status_bar_shows_mode() {
    let theme = Theme::default();
    let widget = StatusBar::new(&theme)
        .agent_mode(tark_cli::core::types::AgentMode::Build)
        .build_mode(tark_cli::core::types::BuildMode::Balanced);
    
    let output = render_widget(widget, 120, 1);
    
    assert!(output.contains("Build") || output.contains("🔨"), "Status should show Build mode");
}

#[test]
fn test_status_bar_plan_mode() {
    let theme = Theme::default();
    let widget = StatusBar::new(&theme)
        .agent_mode(tark_cli::core::types::AgentMode::Plan);
    
    let output = render_widget(widget, 120, 1);
    
    assert!(output.contains("Plan") || output.contains("📋"), "Status should show Plan mode");
}

// ============================================================================
// HEADER TESTS
// ============================================================================

#[test]
fn test_header_renders() {
    let theme = Theme::default();
    let widget = Header::new(&theme, "tark");
    let output = render_widget(widget, 80, 1);
    
    assert!(output.contains("tark") || output.contains("🖥"), "Header should show title");
}

// ============================================================================
// TERMINAL FRAME TESTS
// ============================================================================

#[test]
fn test_terminal_frame_has_borders() {
    let theme = Theme::default();
    let widget = TerminalFrame::new(&theme);
    let output = render_widget(widget, 80, 24);
    
    // Check for box drawing characters
    assert!(output.contains("╭") || output.contains("┌"), "Frame should have top-left corner");
    assert!(output.contains("╮") || output.contains("┐"), "Frame should have top-right corner");
}

// ============================================================================
// MESSAGE AREA TESTS  
// ============================================================================

#[test]
fn test_message_area_renders() {
    let theme = Theme::default();
    let messages: Vec<MessageWidget> = vec![];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 80, 20);
    
    // Should render without panic
    assert!(!output.is_empty());
}
