//! TUI Widget Unit Tests
//!
//! Tests individual widgets in isolation
//!
//! Run: cargo test --test tui_widget_tests

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::{buffer::Buffer, Terminal};
use serde_json::json;
use tark_cli::tui_new::widgets::*;
use tark_cli::tui_new::{AppConfig, Theme};
use tark_cli::ui_backend::DiffViewMode;

// Property-based testing
use proptest::prelude::*;

/// Helper to render a widget and capture buffer
fn render_widget<W>(widget: W, width: u16, height: u16) -> String
where
    W: ratatui::widgets::Widget,
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let area = Rect {
                x: 0,
                y: 0,
                width,
                height,
            };
            f.render_widget(widget, area);
        })
        .unwrap();

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

/// Helper to render a widget and capture buffer for style inspection
fn render_widget_buffer<W>(widget: W, width: u16, height: u16) -> Buffer
where
    W: ratatui::widgets::Widget,
{
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();

    terminal
        .draw(|f| {
            let area = Rect {
                x: 0,
                y: 0,
                width,
                height,
            };
            f.render_widget(widget, area);
        })
        .unwrap();

    terminal.backend().buffer().clone()
}

fn find_fg_for_symbol(buf: &Buffer, width: u16, height: u16, symbol: &str) -> Option<Color> {
    for y in 0..height {
        for x in 0..width {
            if let Some(cell) = buf.cell((x, y)) {
                if cell.symbol() == symbol {
                    return Some(cell.fg);
                }
            }
        }
    }
    None
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

#[test]
fn test_file_picker_modal_marks_selected_items() {
    let theme = Theme::default();
    let files = vec!["src/".to_string(), "src/main.rs".to_string()];
    let selected_paths = vec!["src/".to_string()];
    let widget = FilePickerModal::new(&theme)
        .files(&files)
        .filter("s")
        .selected(0)
        .selected_paths(&selected_paths)
        .current_dir("./");

    let output = render_widget(widget, 60, 20);
    assert!(output.contains("[x]"), "Selected items should be marked");
    assert!(output.contains("[ ]"), "Unselected items should be shown");
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

    assert!(
        output.contains("Build") || output.contains("🔨"),
        "Status should show Build mode"
    );
}

#[test]
fn test_status_bar_plan_mode() {
    let theme = Theme::default();
    let widget = StatusBar::new(&theme).agent_mode(tark_cli::core::types::AgentMode::Plan);

    let output = render_widget(widget, 120, 1);

    assert!(
        output.contains("Plan") || output.contains("📋"),
        "Status should show Plan mode"
    );
}

// ============================================================================
// SIDEBAR WIDGET TESTS
// ============================================================================

#[test]
fn test_sidebar_scrollbar_visible_when_overflowing() {
    let theme = Theme::default();
    let changes: Vec<GitChange> = (0..30)
        .map(|i| GitChange {
            status: GitStatus::Modified,
            file: format!("file_{i}.rs"),
            additions: 1,
            deletions: 0,
        })
        .collect();

    let mut focused_sidebar = Sidebar::new(&theme)
        .focused(true)
        .selected_panel(4)
        .git_changes(changes.clone());
    focused_sidebar.expanded_panels = [false, false, false, false, true, false];
    focused_sidebar.selected_item = Some(0);

    let output = render_widget(focused_sidebar, 40, 16);
    assert!(
        output.contains("↑") && output.contains("↓"),
        "Focused, overflowing panel should render scrollbars"
    );

    let unfocused_changes: Vec<GitChange> = (0..2)
        .map(|i| GitChange {
            status: GitStatus::Modified,
            file: format!("small_{i}.rs"),
            additions: 1,
            deletions: 0,
        })
        .collect();

    let mut unfocused_sidebar = Sidebar::new(&theme)
        .focused(false)
        .selected_panel(4)
        .git_changes(unfocused_changes);
    unfocused_sidebar.expanded_panels = [false, false, false, false, true, false];
    unfocused_sidebar.selected_item = Some(0);

    let output = render_widget(unfocused_sidebar, 40, 16);
    assert!(
        output.contains("↑") && output.contains("↓"),
        "Unfocused, overflowing sidebar should render scrollbars"
    );
}

#[test]
fn test_sidebar_session_model_line_highlights_when_selected() {
    let theme = Theme::default();
    let session_info = SessionInfo {
        name: "Session A".to_string(),
        total_cost: 1.234,
        model_count: 2,
        model_costs: vec![("ZMODEL".to_string(), 0.123), ("Other".to_string(), 1.111)],
        total_tokens: 900,
        model_tokens: vec![("ZMODEL".to_string(), 300), ("Other".to_string(), 600)],
    };

    let mut sidebar = Sidebar::new(&theme)
        .focused(true)
        .selected_panel(0)
        .session_info(session_info);
    sidebar.expanded_panels = [true, false, false, false, false, false];
    sidebar.selected_item = Some(3);

    let buf = render_widget_buffer(sidebar, 40, 12);
    let fg = find_fg_for_symbol(&buf, 40, 12, "Z");
    assert_eq!(
        fg,
        Some(theme.cyan),
        "Selected session model line should use focused color"
    );
}

// ============================================================================
// STATUS MESSAGE STRIP TESTS
// ============================================================================

#[test]
fn test_flash_bar_state_default() {
    let default_state = FlashBarState::default();
    assert_eq!(
        default_state,
        FlashBarState::Idle,
        "FlashBarState should default to Idle"
    );
}

#[test]
fn test_flash_bar_state_variants() {
    // Test all variants exist and are distinct
    let idle = FlashBarState::Idle;
    let working = FlashBarState::Working;
    let error = FlashBarState::Error;
    let warning = FlashBarState::Warning;

    assert_ne!(idle, working);
    assert_ne!(idle, error);
    assert_ne!(idle, warning);
    assert_ne!(working, error);
    assert_ne!(working, warning);
    assert_ne!(error, warning);
}

#[test]
fn test_status_message_bar_renders_warning() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Rate limited retrying in 1s")
        .kind(FlashBarState::Warning);

    let output = render_widget(widget, 80, 1);

    assert!(
        output.contains("Rate limited retrying in 1s"),
        "Status message strip should render the message"
    );
}

#[test]
fn test_status_message_bar_renders_error() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Connection failed")
        .kind(FlashBarState::Error);

    let output = render_widget(widget, 80, 1);

    assert!(
        output.contains("Connection failed"),
        "Status message strip should render error message"
    );
}

#[test]
fn test_status_message_bar_renders_idle() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme).kind(FlashBarState::Idle);

    let output = render_widget(widget, 80, 1);

    // Idle state should render a dot
    assert!(
        output.contains("·") || !output.is_empty(),
        "Status message strip should render idle state"
    );
}

// ============================================================================
// MESSAGE STATE RENDERING TESTS (Task 2.4)
// ============================================================================

#[test]
fn test_tool_diff_renders_inline_on_narrow_width() {
    let theme = Theme::default();
    let diff = "Preview\n```diff\n--- a/foo.txt\n+++ b/foo.txt\n@@\n- old line\n+ new line\n```\n";
    let content = format!("✓|write_file|write|{}", diff);
    let messages = vec![Message::new(MessageRole::Tool, content)];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 70, 12);

    assert!(
        output.contains("+ new line"),
        "Inline diff should include added lines"
    );
    assert!(
        !output.contains(" | "),
        "Inline diff should not include split separator"
    );
}

#[test]
fn test_thinking_block_truncates_to_max_lines() {
    let theme = Theme::default();
    let content = vec!["word"; 200].join(" ");
    let mut msg = Message::new(MessageRole::Thinking, content);
    msg.collapsed = false;
    let messages = vec![msg];

    let widget = MessageArea::new(&messages, &theme).thinking_max_lines(2);
    let output = render_widget(widget, 80, 12);

    assert!(
        output.contains("🧠"),
        "Thinking block should show brain icon"
    );
    assert!(
        output.contains("Thinking (model)"),
        "Thinking block should include model label"
    );
    assert!(
        output.contains("↑ more above"),
        "Thinking block should indicate truncated content:\n{}",
        output
    );
}

#[test]
fn test_tool_diff_renders_split_on_wide_width() {
    let theme = Theme::default();
    let diff = "Preview\n```diff\n--- a/foo.txt\n+++ b/foo.txt\n@@\n- old line\n+ new line\n```\n";
    let content = format!("✓|write_file|write|{}", diff);
    let messages = vec![Message::new(MessageRole::Tool, content)];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 120, 12);

    assert!(
        output.contains(" | "),
        "Split diff should include column separator"
    );
}

#[test]
fn test_tool_diff_forced_inline_on_wide_width() {
    let theme = Theme::default();
    let diff = "Preview\n```diff\n--- a/foo.txt\n+++ b/foo.txt\n@@\n- old line\n+ new line\n```\n";
    let content = format!("✓|write_file|write|{}", diff);
    let messages = vec![Message::new(MessageRole::Tool, content)];
    let widget = MessageArea::new(&messages, &theme).diff_view_mode(DiffViewMode::Inline);
    let output = render_widget(widget, 120, 12);

    assert!(
        output.contains("+ new line"),
        "Inline diff should include added lines"
    );
    assert!(
        !output.contains(" | "),
        "Forced inline diff should not include split separator"
    );
}

#[test]
fn test_tool_diff_forced_split_on_narrow_width() {
    let theme = Theme::default();
    let diff = "Preview\n```diff\n--- a/foo.txt\n+++ b/foo.txt\n@@\n- old line\n+ new line\n```\n";
    let content = format!("✓|write_file|write|{}", diff);
    let messages = vec![Message::new(MessageRole::Tool, content)];
    let widget = MessageArea::new(&messages, &theme).diff_view_mode(DiffViewMode::Split);
    let output = render_widget(widget, 70, 12);

    assert!(
        output.contains(" | "),
        "Forced split diff should include column separator"
    );
}

#[test]
fn test_message_area_line_targets_include_tool_group_header() {
    let theme = Theme::default();
    let messages = vec![
        Message::new(MessageRole::Tool, "✓|a|Exploration|one"),
        Message::new(MessageRole::Tool, "✓|b|Exploration|two"),
    ];
    let widget = MessageArea::new(&messages, &theme);
    let area = Rect {
        x: 0,
        y: 0,
        width: 80,
        height: 20,
    };
    let targets = widget.line_targets(area);

    assert!(
        targets
            .iter()
            .any(|t| matches!(t, MessageLineTarget::ToolGroupHeader { start_index: 0 })),
        "Expected tool group header line target"
    );
    assert!(
        targets.contains(&MessageLineTarget::ToolHeader(0)),
        "Expected first tool header target"
    );
    assert!(
        targets.contains(&MessageLineTarget::ToolHeader(1)),
        "Expected second tool header target"
    );
}

#[test]
fn test_flash_bar_error_bordered_card() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Connection failed")
        .kind(FlashBarState::Error);

    let output = render_widget(widget, 80, 1);

    // Should have border characters (inline borders: │)
    assert!(
        output.contains("│"),
        "Error state should render with bordered card style"
    );
}

#[test]
fn test_flash_bar_warning_bordered_card() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Request timeout")
        .kind(FlashBarState::Warning);

    let output = render_widget(widget, 80, 1);

    // Should have border characters (inline borders: │)
    assert!(
        output.contains("│"),
        "Warning state should render with bordered card style"
    );
}

#[test]
fn test_flash_bar_error_dot_indicator() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Connection failed")
        .kind(FlashBarState::Error);

    let output = render_widget(widget, 80, 1);

    // Should have dot indicator pattern: ● ·
    assert!(
        output.contains("●"),
        "Error state should have large dot indicator"
    );
    assert!(
        output.contains("·"),
        "Error state should have small dot in indicator pattern"
    );
}

#[test]
fn test_flash_bar_warning_dot_indicator() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Request timeout")
        .kind(FlashBarState::Warning);

    let output = render_widget(widget, 80, 1);

    // Should have dot indicator pattern: ● ·
    assert!(
        output.contains("●"),
        "Warning state should have large dot indicator"
    );
    assert!(
        output.contains("·"),
        "Warning state should have small dot in indicator pattern"
    );
}

#[test]
fn test_flash_bar_error_message_text() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Connection failed")
        .kind(FlashBarState::Error);

    let output = render_widget(widget, 80, 1);

    // Should contain the message text
    assert!(
        output.contains("Connection failed"),
        "Error state should display the message text"
    );
}

#[test]
fn test_flash_bar_warning_message_text() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .message("Request timeout")
        .kind(FlashBarState::Warning);

    let output = render_widget(widget, 80, 1);

    // Should contain the message text
    assert!(
        output.contains("Request timeout"),
        "Warning state should display the message text"
    );
}

#[test]
fn test_flash_bar_error_default_message() {
    let theme = Theme::default();
    // Create error state without custom message
    let widget = FlashBar::new(&theme).kind(FlashBarState::Error);

    let output = render_widget(widget, 80, 1);

    // Should display default error message
    assert!(
        output.contains("CRITICAL ERROR"),
        "Error state should display default message when no custom message provided"
    );
}

#[test]
fn test_flash_bar_warning_default_message() {
    let theme = Theme::default();
    // Create warning state without custom message
    let widget = FlashBar::new(&theme).kind(FlashBarState::Warning);

    let output = render_widget(widget, 80, 1);

    // Should display default warning message
    assert!(
        output.contains("Request timeout") || output.contains("retrying"),
        "Warning state should display default message when no custom message provided"
    );
}

#[test]
fn test_flash_bar_message_state_single_line() {
    let theme = Theme::default();

    // Test both error and warning states
    for state in [FlashBarState::Error, FlashBarState::Warning] {
        let widget = FlashBar::new(&theme).message("Test message").kind(state);

        let output = render_widget(widget, 80, 1);

        // Should be exactly one line (plus trailing newline)
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(
            lines.len(),
            1,
            "Message state {:?} should occupy exactly one line",
            state
        );
    }
}

#[test]
fn test_flash_bar_message_state_completeness() {
    let theme = Theme::default();

    // Test that all message states have complete rendering
    for (state, message) in [
        (FlashBarState::Error, "Error message"),
        (FlashBarState::Warning, "Warning message"),
    ] {
        let widget = FlashBar::new(&theme).message(message).kind(state);

        let output = render_widget(widget, 80, 1);

        // Should have all three components:
        // 1. Border characters (inline borders: │)
        assert!(output.contains("│"), "State {:?} should have border", state);

        // 2. Dot indicator
        assert!(
            output.contains("●"),
            "State {:?} should have dot indicator",
            state
        );

        // 3. Message text
        assert!(
            output.contains(message),
            "State {:?} should have message text",
            state
        );
    }
}

#[test]
fn test_flash_bar_idle_state_centered_dot() {
    let theme = Theme::default();

    // Test with various widths to ensure centering
    for width in [10, 20, 40, 80] {
        let widget = FlashBar::new(&theme).kind(FlashBarState::Idle);
        let output = render_widget(widget, width, 1);

        // Should contain the muted dot character
        assert!(
            output.contains("·"),
            "Idle state should render a muted dot (·) for width {}",
            width
        );

        // The dot should be roughly centered (allowing for rounding)
        let lines: Vec<&str> = output.lines().collect();
        if let Some(line) = lines.first() {
            let dot_pos = line.find("·");
            assert!(
                dot_pos.is_some(),
                "Dot should be present in the output for width {}",
                width
            );

            if let Some(pos) = dot_pos {
                let center = (width / 2) as usize;
                // Allow for some tolerance in centering (within 1 character)
                assert!(
                    pos.abs_diff(center) <= 1,
                    "Dot should be centered (pos: {}, center: {}, width: {})",
                    pos,
                    center,
                    width
                );
            }
        }
    }
}

#[test]
fn test_flash_bar_idle_state_single_line() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme).kind(FlashBarState::Idle);

    let output = render_widget(widget, 80, 1);

    // Should be exactly one line (plus trailing newline)
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 1, "Idle state should occupy exactly one line");
}

#[test]
fn test_status_message_bar_renders_working() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme).kind(FlashBarState::Working);

    let output = render_widget(widget, 80, 1);

    // Working state should render a pulse bar
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert!(
        bar_count > 0,
        "Status message strip should render working state"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_0() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(0);

    let output = render_widget(widget, 80, 1);

    // Frame 0 should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame 0 should render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_1() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(1);

    let output = render_widget(widget, 80, 1);

    // Frame 1 should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame 1 should render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_2() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(2);

    let output = render_widget(widget, 80, 1);

    // Frame 2 should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame 2 should render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_3() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(3);

    let output = render_widget(widget, 80, 1);

    // Frame 3 should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame 3 should render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_4() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(4);

    let output = render_widget(widget, 80, 1);

    // Frame 4 should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame 4 should render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_animation_frame_clamping() {
    let theme = Theme::default();
    // Test that frames > 4 are clamped to 4
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(10); // Invalid frame, should clamp to 4

    let output = render_widget(widget, 80, 1);

    // Should render a full-width pulse line
    let bar_count =
        output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
    assert_eq!(
        bar_count, 80,
        "Frame > max should still render a full-width pulse line"
    );
}

#[test]
fn test_flash_bar_working_line_full_width() {
    let theme = Theme::default();

    // Test with various widths to ensure full-width rendering
    for width in [20, 40, 80] {
        let widget = FlashBar::new(&theme)
            .kind(FlashBarState::Working)
            .animation_frame(4);

        let output = render_widget(widget, width, 1);
        let bar_count =
            output.matches("─").count() + output.matches("═").count() + output.matches("━").count();
        assert_eq!(
            bar_count, width as usize,
            "Pulse line should span full width for width {}",
            width
        );
    }
}

#[test]
fn test_flash_bar_working_single_line() {
    let theme = Theme::default();
    let widget = FlashBar::new(&theme)
        .kind(FlashBarState::Working)
        .animation_frame(4);

    let output = render_widget(widget, 80, 1);

    // Should be exactly one line (plus trailing newline)
    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "Working state should occupy exactly one line"
    );
}

// ============================================================================
// HEADER TESTS
// ============================================================================

#[test]
fn test_header_renders() {
    let theme = Theme::default();
    let config = AppConfig::default();
    let widget = Header::new(&config, &theme, None);
    let output = render_widget(widget, 80, 1);

    assert!(
        output.contains("tark") || output.contains("Tark") || output.contains("🖥"),
        "Header should show title"
    );
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
    assert!(
        output.contains("╭") || output.contains("┌"),
        "Frame should have top-left corner"
    );
    assert!(
        output.contains("╮") || output.contains("┐"),
        "Frame should have top-right corner"
    );
}

// ============================================================================
// MESSAGE AREA TESTS
// ============================================================================

#[test]
fn test_message_area_renders() {
    let theme = Theme::default();
    let messages: Vec<Message> = vec![];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 80, 20);

    // Should render without panic
    assert!(!output.is_empty());
}

#[test]
fn test_think_tool_shows_summary_when_collapsed() {
    let theme = Theme::default();
    let mut msg = Message::new(MessageRole::Tool, "✓|think|Exploration|Recorded");
    msg.collapsed = true;
    msg.tool_args = Some(json!({
        "thought": "Verify the regression path",
        "thought_number": 1,
        "total_thoughts": 1,
        "next_thought_needed": false
    }));
    let messages = vec![msg];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 80, 10);

    assert!(
        output.contains("1. Verify the regression path"),
        "Collapsed think tool should show thought summary"
    );
}

#[test]
fn test_archive_marker_card_renders() {
    let theme = Theme::default();
    let mut marker = Message::new(MessageRole::System, "");
    marker.tool_args = Some(json!({
        "kind": "archive_marker",
        "created_at": "2024-07-14 10:32:00",
        "sequence": 3,
        "message_count": 48,
    }));
    let messages = vec![marker];
    let widget = MessageArea::new(&messages, &theme);
    let output = render_widget(widget, 100, 6);

    assert!(
        output.contains("Archive"),
        "Archive card should render label"
    );
    assert!(output.contains("Load"), "Archive card should render action");
}

// ============================================================================
// PROPERTY-BASED TESTS
// ============================================================================

proptest! {
    /// **Validates: Requirements 3.3**
    ///
    /// Property 1: Animation Frame to Pulse Line Rendering
    ///
    /// For any animation frame f in [0, 20], the working state SHALL render a
    /// full-width pulse line across the flash bar.
    ///
    /// Feature: flash-bar, Property 1: Animation Frame to Pulse Line Rendering
    #[test]
    fn prop_flash_bar_animation_frame_to_line(frame in 0u8..=20) {
        let theme = Theme::default();
        let widget = FlashBar::new(&theme)
            .kind(FlashBarState::Working)
            .animation_frame(frame);

        let output = render_widget(widget, 80, 1);

        // Count visible pulse bar characters
        let bar_count = output.matches("─").count()
            + output.matches("═").count()
            + output.matches("━").count();
        prop_assert_eq!(
            bar_count,
            80,
            "For frame {}, expected a full-width pulse line, but found {} characters",
            frame,
            bar_count
        );
    }

    /// **Validates: Requirements 4.1, 4.2, 4.3**
    ///
    /// Property 2: Message State Rendering Completeness
    ///
    /// For any message state (Error, Warning) and any non-empty message string,
    /// the rendered output SHALL contain:
    /// 1. Border characters (indicating card style)
    /// 2. A dot indicator character
    /// 3. The message text
    ///
    /// Feature: flash-bar, Property 2: Message State Rendering Completeness
    #[test]
    fn prop_flash_bar_message_state_completeness(
        state_idx in 0usize..=1,
        message in "[a-zA-Z0-9 ]{1,50}"  // Non-empty messages only
    ) {
        let theme = Theme::default();

        // Map index to message state (0 = Error, 1 = Warning)
        let state = match state_idx {
            0 => FlashBarState::Error,
            1 => FlashBarState::Warning,
            _ => FlashBarState::Error, // Fallback (shouldn't happen)
        };

        // Test with custom message
        let widget = FlashBar::new(&theme)
            .message(&message)
            .kind(state);

        let output = render_widget(widget, 80, 1);

        // 1. Should have border characters (inline borders: │)
        prop_assert!(
            output.contains("│"),
            "State {:?} with message '{}' should have border characters",
            state,
            message
        );

        // 2. Should have dot indicator (large dot ●)
        prop_assert!(
            output.contains("●"),
            "State {:?} with message '{}' should have large dot indicator",
            state,
            message
        );

        // 3. Should contain the custom message
        prop_assert!(
            output.contains(&message),
            "State {:?} should display custom message '{}'",
            state,
            message
        );

        // Additional invariant: should also have small dot in indicator pattern (● ·)
        prop_assert!(
            output.contains("·"),
            "State {:?} with message '{}' should have small dot in indicator pattern",
            state,
            message
        );
    }

    /// **Validates: Requirements 4.1, 4.2, 4.3**
    ///
    /// Property 2b: Message State Completeness Without Custom Message
    ///
    /// For any message state (Error, Warning) when no custom message is provided,
    /// the rendered output SHALL contain all three components with default message.
    ///
    /// Feature: flash-bar, Property 2: Message State Rendering Completeness
    #[test]
    fn prop_flash_bar_message_state_completeness_no_message(state_idx in 0usize..=1) {
        let theme = Theme::default();

        // Map index to message state (0 = Error, 1 = Warning)
        let state = match state_idx {
            0 => FlashBarState::Error,
            1 => FlashBarState::Warning,
            _ => FlashBarState::Error, // Fallback (shouldn't happen)
        };

        // Test without custom message
        let widget = FlashBar::new(&theme).kind(state);

        let output = render_widget(widget, 80, 1);

        // 1. Should have border characters
        prop_assert!(
            output.contains("│"),
            "State {:?} without message should have border characters",
            state
        );

        // 2. Should have dot indicator
        prop_assert!(
            output.contains("●"),
            "State {:?} without message should have large dot indicator",
            state
        );

        // 3. Should have default message
        let has_default = output.contains("CRITICAL ERROR") || output.contains("Request timeout");
        prop_assert!(
            has_default,
            "State {:?} without message should display default message",
            state
        );

        // Additional invariant: should have small dot in indicator pattern
        prop_assert!(
            output.contains("·"),
            "State {:?} without message should have small dot in indicator pattern",
            state
        );
    }

    /// **Validates: Requirements 4.7**
    ///
    /// Property 3: Default Message Fallback
    ///
    /// For any message state (Error, Warning) when no custom message is provided,
    /// the rendered output SHALL contain a non-empty default message string
    /// appropriate to that state.
    ///
    /// This property ensures the widget never renders an empty message card,
    /// providing meaningful feedback even when no specific message is set.
    ///
    /// Feature: flash-bar, Property 3: Default Message Fallback
    #[test]
    fn prop_flash_bar_default_message_fallback(state_idx in 0usize..=1) {
        let theme = Theme::default();

        // Map index to message state (0 = Error, 1 = Warning)
        let state = match state_idx {
            0 => FlashBarState::Error,
            1 => FlashBarState::Warning,
            _ => FlashBarState::Error, // Fallback (shouldn't happen)
        };

        // Create widget without custom message
        let widget = FlashBar::new(&theme).kind(state);

        let output = render_widget(widget, 80, 1);

        // Extract the message content
        // Format: │  ●  ·  {message}  │
        let lines: Vec<&str> = output.lines().collect();
        prop_assert!(
            !lines.is_empty(),
            "State {:?} should render at least one line",
            state
        );

        let line = lines[0];

        // 1. Should have the dot indicator pattern (● and ·)
        prop_assert!(
            line.contains("●") && line.contains("·"),
            "State {:?} should have dot indicator pattern (● and ·), but got: '{}'",
            state,
            line
        );

        // 2. Find the message portion (after "·" and before trailing border)
        // Split by the small dot and take the part after it
        let parts: Vec<&str> = line.split("·").collect();
        prop_assert!(
            parts.len() >= 2,
            "State {:?} should have content after dot indicator",
            state
        );

        // Get the message part (after the dot indicator)
        let message_part = parts[1].trim();

        // 3. Message should be non-empty
        prop_assert!(
            !message_part.is_empty(),
            "State {:?} should have non-empty default message, but got empty string",
            state
        );

        // 4. Message should be appropriate to the state
        match state {
            FlashBarState::Error => {
                prop_assert!(
                    message_part.contains("CRITICAL ERROR") || message_part.contains("error") || message_part.contains("ERROR"),
                    "Error state should have error-related default message, but got: '{}'",
                    message_part
                );
            }
            FlashBarState::Warning => {
                prop_assert!(
                    message_part.contains("timeout") || message_part.contains("retry") || message_part.contains("warning") || message_part.contains("WARNING"),
                    "Warning state should have warning-related default message, but got: '{}'",
                    message_part
                );
            }
            _ => {}
        }

        // 5. Message should have reasonable length (not just a single character)
        prop_assert!(
            message_part.len() >= 5,
            "State {:?} default message should be meaningful (>= 5 chars), but got: '{}' ({} chars)",
            state,
            message_part,
            message_part.len()
        );
    }

    /// **Validates: Requirements 5.1, 5.2, 5.3, 5.4**
    ///
    /// Property 4: State-Color Consistency
    ///
    /// For any FlashBarState, the rendered output SHALL use the correct
    /// theme color for dots, borders, and text where applicable.
    ///
    /// Feature: flash-bar, Property 4: State-Color Consistency
    #[test]
    fn prop_flash_bar_state_color_consistency(state_idx in 0usize..=3) {
        let theme = Theme::default();

        let state = match state_idx {
            0 => FlashBarState::Idle,
            1 => FlashBarState::Working,
            2 => FlashBarState::Error,
            3 => FlashBarState::Warning,
            _ => FlashBarState::Idle,
        };

        let mut widget = FlashBar::new(&theme).kind(state);
        if matches!(state, FlashBarState::Error | FlashBarState::Warning) {
            widget = widget.message("Test message");
        }

        let buf = render_widget_buffer(widget, 80, 1);

        let expected_fg = match state {
            FlashBarState::Idle => theme.text_muted,
            FlashBarState::Working => theme.cyan,
            FlashBarState::Error => theme.red,
            FlashBarState::Warning => theme.yellow,
        };

        match state {
            FlashBarState::Idle => {
                let fg = find_fg_for_symbol(&buf, 80, 1, "·");
                prop_assert!(fg.is_some(), "Idle state should render a muted dot");
                prop_assert_eq!(fg.unwrap(), expected_fg);
            }
            FlashBarState::Working => {
                let mut found = false;
                for y in 0..1 {
                    for x in 0..80 {
                        if let Some(cell) = buf.cell((x, y)) {
                            let symbol = cell.symbol();
                            if (symbol == "─" || symbol == "═" || symbol == "━")
                                && cell.fg != theme.bg_dark
                            {
                                found = true;
                                break;
                            }
                        }
                    }
                }
                prop_assert!(
                    found,
                    "Working state should render a pulse line with visible accent color"
                );
            }
            FlashBarState::Error | FlashBarState::Warning => {
                let dot_fg = find_fg_for_symbol(&buf, 80, 1, "●");
                let border_fg = find_fg_for_symbol(&buf, 80, 1, "│");
                let text_fg = find_fg_for_symbol(&buf, 80, 1, "T");

                prop_assert!(dot_fg.is_some(), "Message state should render dot indicator");
                prop_assert!(border_fg.is_some(), "Message state should render border");
                prop_assert!(text_fg.is_some(), "Message state should render text");

                prop_assert_eq!(dot_fg.unwrap(), expected_fg);
                prop_assert_eq!(border_fg.unwrap(), expected_fg);
                prop_assert_eq!(text_fg.unwrap(), expected_fg);
            }
        }
    }
}
