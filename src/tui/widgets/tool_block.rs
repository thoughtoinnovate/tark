//! Tool block widget for displaying tool execution in the TUI
//!
//! Provides a specialized widget for visualizing tool calls and their results
//! in the chat area with collapsible support.
//!
//! Requirements:
//! - 4.1: Display Tool_Block when ChatAgent executes a tool
//! - 4.2: Show tool name and arguments
//! - 4.3: Show result preview when completed
//! - 4.4: Support collapsible state (default collapsed after completion)

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Status of a tool execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolStatus {
    /// Tool is currently executing
    #[default]
    Running,
    /// Tool completed successfully
    Completed,
    /// Tool failed with an error
    Failed,
}

impl ToolStatus {
    /// Get the icon for this status
    pub fn icon(&self) -> &'static str {
        match self {
            ToolStatus::Running => "⏳",
            ToolStatus::Completed => "✓",
            ToolStatus::Failed => "✗",
        }
    }

    /// Get the color for this status
    pub fn color(&self) -> Color {
        match self {
            ToolStatus::Running => Color::Yellow,
            ToolStatus::Completed => Color::Green,
            ToolStatus::Failed => Color::Red,
        }
    }
}

/// A tool execution block for display in the message list
///
/// This widget displays tool calls with their arguments and results
/// in a collapsible format.
#[derive(Debug, Clone)]
pub struct ToolBlock {
    /// Unique identifier for this block
    pub id: String,
    /// Tool name (e.g., "ripgrep", "read_file")
    pub tool_name: String,
    /// Tool arguments as JSON
    pub args: serde_json::Value,
    /// Formatted arguments for display
    pub args_display: String,
    /// Result preview (truncated if long)
    pub result_preview: Option<String>,
    /// Full result (for expansion)
    pub full_result: Option<String>,
    /// Current status
    pub status: ToolStatus,
    /// Whether the block is expanded
    pub expanded: bool,
    /// Error message if failed
    pub error: Option<String>,
}

impl ToolBlock {
    /// Create a new tool block for a starting tool call
    ///
    /// Requirements: 4.1, 4.2
    pub fn new(
        id: impl Into<String>,
        tool_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        let tool_name = tool_name.into();
        let args_display = Self::format_args(&args, &tool_name);

        Self {
            id: id.into(),
            tool_name,
            args,
            args_display,
            result_preview: None,
            full_result: None,
            status: ToolStatus::Running,
            expanded: true, // Expanded while running
            error: None,
        }
    }

    /// Create a tool block from AgentEvent data
    pub fn from_started(
        tool: &str,
        args: serde_json::Value,
        message_id: &str,
        index: usize,
    ) -> Self {
        let id = format!("{}-tool-{}", message_id, index);
        Self::new(id, tool, args)
    }

    /// Mark the tool as completed with a result
    ///
    /// Requirements: 4.3, 4.4
    pub fn complete(&mut self, result: impl Into<String>) {
        let result = result.into();
        self.result_preview = Some(Self::truncate_result(&result, 200));
        self.full_result = Some(result);
        self.status = ToolStatus::Completed;
        self.expanded = false; // Collapse after completion (Requirement 4.4)
    }

    /// Mark the tool as failed with an error
    pub fn fail(&mut self, error: impl Into<String>) {
        self.error = Some(error.into());
        self.status = ToolStatus::Failed;
        self.expanded = true; // Keep expanded to show error
    }

    /// Toggle the expanded state
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Set the expanded state
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }

    /// Check if the block is expanded
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Check if the tool is still running
    pub fn is_running(&self) -> bool {
        self.status == ToolStatus::Running
    }

    /// Get the display header with status indicator
    ///
    /// Shows "▼" when expanded and "▶" when collapsed
    pub fn display_header(&self) -> String {
        let indicator = if self.expanded { "▼" } else { "▶" };
        let status_icon = self.status.icon();
        format!("{} ⚙️ {} Tool: {}", indicator, status_icon, self.tool_name)
    }

    /// Get the number of visible lines
    pub fn visible_lines(&self) -> usize {
        if self.expanded {
            let mut lines = 1; // Header
            lines += 1; // Arguments line
            if let Some(ref preview) = self.result_preview {
                lines += preview.lines().count().min(10) + 1; // Result lines + separator
            }
            if let Some(ref error) = self.error {
                lines += error.lines().count() + 1; // Error lines + separator
            }
            lines += 1; // Closing border
            lines
        } else {
            1 // Header only
        }
    }

    /// Format tool arguments for display
    fn format_args(args: &serde_json::Value, tool_name: &str) -> String {
        match args {
            serde_json::Value::Object(map) => {
                // Tool-specific formatting
                match tool_name {
                    "ripgrep" | "grep" => {
                        if let Some(pattern) = map.get("pattern").and_then(|v| v.as_str()) {
                            let path = map.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                            return format!("rg \"{}\" {}", pattern, path);
                        }
                    }
                    "read_file" => {
                        if let Some(path) = map.get("path").and_then(|v| v.as_str()) {
                            return format!("cat {}", path);
                        }
                    }
                    "write_file" => {
                        if let Some(path) = map.get("path").and_then(|v| v.as_str()) {
                            return format!("write → {}", path);
                        }
                    }
                    "shell" | "execute" => {
                        if let Some(cmd) = map.get("command").and_then(|v| v.as_str()) {
                            return format!("shell> {}", cmd);
                        }
                    }
                    "list_files" | "ls" => {
                        if let Some(path) = map.get("path").and_then(|v| v.as_str()) {
                            return format!("ls {}", path);
                        }
                    }
                    _ => {}
                }

                // Generic formatting: show first few key-value pairs
                let pairs: Vec<String> = map
                    .iter()
                    .take(3)
                    .map(|(k, v)| {
                        let v_str = match v {
                            serde_json::Value::String(s) => {
                                if s.len() > 50 {
                                    format!("\"{}...\"", &s[..47])
                                } else {
                                    format!("\"{}\"", s)
                                }
                            }
                            _ => v.to_string(),
                        };
                        format!("{}={}", k, v_str)
                    })
                    .collect();

                if map.len() > 3 {
                    format!("{} +{} more", pairs.join(", "), map.len() - 3)
                } else {
                    pairs.join(", ")
                }
            }
            serde_json::Value::Null => String::new(),
            _ => args.to_string(),
        }
    }

    /// Truncate result for preview (respects UTF-8 char boundaries)
    fn truncate_result(result: &str, max_len: usize) -> String {
        if result.chars().count() <= max_len {
            result.to_string()
        } else {
            let chars: String = result.chars().take(max_len.saturating_sub(3)).collect();
            format!("{}...", chars)
        }
    }

    /// Render the tool block to lines for display
    ///
    /// Requirements: 4.1, 4.2, 4.3, 4.4
    pub fn render_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header with status indicator
        let header_color = self.status.color();
        let header_style = Style::default()
            .fg(header_color)
            .add_modifier(Modifier::BOLD);

        let header_text = self.display_header();
        lines.push(Line::from(Span::styled(
            format!("╭── {} ", header_text),
            header_style,
        )));

        // Content if expanded
        if self.expanded {
            let border_style = Style::default().fg(Color::DarkGray);
            let content_style = Style::default().fg(Color::Gray);
            let cmd_style = Style::default().fg(Color::Cyan);

            // Arguments line
            if !self.args_display.is_empty() {
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("$ {}", self.args_display), cmd_style),
                ]));
            }

            // Result preview
            if let Some(ref preview) = self.result_preview {
                lines.push(Line::from(Span::styled("│", border_style)));
                for (i, line) in preview.lines().take(10).enumerate() {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(format!("> {}", line), content_style),
                    ]));
                    if i >= 9 && preview.lines().count() > 10 {
                        lines.push(Line::from(vec![
                            Span::styled("│ ", border_style),
                            Span::styled("> ...", content_style),
                        ]));
                        break;
                    }
                }
            }

            // Error message
            if let Some(ref error) = self.error {
                let error_style = Style::default().fg(Color::Red);
                lines.push(Line::from(Span::styled("│", border_style)));
                for line in error.lines() {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(format!("✗ {}", line), error_style),
                    ]));
                }
            }

            // Running indicator
            if self.status == ToolStatus::Running {
                let running_style = Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::SLOW_BLINK);
                lines.push(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled("⏳ Running...", running_style),
                ]));
            }

            // Closing border
            lines.push(Line::from(Span::styled(
                "╰──────────────────────────────────────────────────",
                border_style,
            )));
        }

        lines
    }
}

/// Widget for rendering a ToolBlock
pub struct ToolBlockWidget<'a> {
    block: &'a ToolBlock,
}

impl<'a> ToolBlockWidget<'a> {
    /// Create a new tool block widget
    pub fn new(block: &'a ToolBlock) -> Self {
        Self { block }
    }
}

impl Widget for ToolBlockWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let lines = self.block.render_lines();

        for (i, line) in lines.iter().enumerate() {
            if i >= area.height as usize {
                break;
            }
            let y = area.y + i as u16;
            let mut x = area.x;

            for span in &line.spans {
                let width = span.content.len().min((area.width - (x - area.x)) as usize);
                if width > 0 {
                    buf.set_string(x, y, &span.content[..width], span.style);
                    x += width as u16;
                }
            }
        }
    }
}

/// Manager for tracking multiple tool blocks in a message
#[derive(Debug, Clone, Default)]
pub struct ToolBlockManager {
    /// Tool blocks indexed by their ID
    blocks: Vec<ToolBlock>,
}

impl ToolBlockManager {
    /// Create a new empty manager
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Add a new tool block for a starting tool call
    pub fn add_started(
        &mut self,
        tool: &str,
        args: serde_json::Value,
        message_id: &str,
    ) -> &mut ToolBlock {
        let index = self.blocks.len();
        let block = ToolBlock::from_started(tool, args, message_id, index);
        self.blocks.push(block);
        self.blocks.last_mut().unwrap()
    }

    /// Complete the most recent running tool block
    pub fn complete_last(&mut self, result: impl Into<String>) {
        if let Some(block) = self.blocks.iter_mut().rev().find(|b| b.is_running()) {
            block.complete(result);
        }
    }

    /// Fail the most recent running tool block
    pub fn fail_last(&mut self, error: impl Into<String>) {
        if let Some(block) = self.blocks.iter_mut().rev().find(|b| b.is_running()) {
            block.fail(error);
        }
    }

    /// Get all tool blocks
    pub fn blocks(&self) -> &[ToolBlock] {
        &self.blocks
    }

    /// Get a mutable reference to all tool blocks
    pub fn blocks_mut(&mut self) -> &mut [ToolBlock] {
        &mut self.blocks
    }

    /// Get the number of tool blocks
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Check if there are no tool blocks
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Check if any tool is currently running
    pub fn has_running(&self) -> bool {
        self.blocks.iter().any(|b| b.is_running())
    }

    /// Get the currently running tool name (if any)
    pub fn current_tool(&self) -> Option<&str> {
        self.blocks
            .iter()
            .rev()
            .find(|b| b.is_running())
            .map(|b| b.tool_name.as_str())
    }

    /// Toggle a block by ID
    pub fn toggle(&mut self, id: &str) {
        if let Some(block) = self.blocks.iter_mut().find(|b| b.id == id) {
            block.toggle();
        }
    }

    /// Clear all blocks
    pub fn clear(&mut self) {
        self.blocks.clear();
    }

    /// Render all blocks to lines
    pub fn render_all_lines(&self) -> Vec<Line<'static>> {
        self.blocks.iter().flat_map(|b| b.render_lines()).collect()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_status_icon() {
        assert_eq!(ToolStatus::Running.icon(), "⏳");
        assert_eq!(ToolStatus::Completed.icon(), "✓");
        assert_eq!(ToolStatus::Failed.icon(), "✗");
    }

    #[test]
    fn test_tool_status_color() {
        assert_eq!(ToolStatus::Running.color(), Color::Yellow);
        assert_eq!(ToolStatus::Completed.color(), Color::Green);
        assert_eq!(ToolStatus::Failed.color(), Color::Red);
    }

    #[test]
    fn test_tool_block_new() {
        let block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({"pattern": "test"}));

        assert_eq!(block.id, "test-1");
        assert_eq!(block.tool_name, "ripgrep");
        assert_eq!(block.status, ToolStatus::Running);
        assert!(block.expanded); // Running tools are expanded
        assert!(block.result_preview.is_none());
    }

    #[test]
    fn test_tool_block_complete() {
        let mut block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({"pattern": "test"}));

        block.complete("Found 5 matches");

        assert_eq!(block.status, ToolStatus::Completed);
        assert!(!block.expanded); // Completed tools collapse
        assert_eq!(block.result_preview, Some("Found 5 matches".to_string()));
    }

    #[test]
    fn test_tool_block_fail() {
        let mut block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({"pattern": "test"}));

        block.fail("File not found");

        assert_eq!(block.status, ToolStatus::Failed);
        assert!(block.expanded); // Failed tools stay expanded
        assert_eq!(block.error, Some("File not found".to_string()));
    }

    #[test]
    fn test_tool_block_toggle() {
        let mut block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({}));

        assert!(block.is_expanded());
        block.toggle();
        assert!(!block.is_expanded());
        block.toggle();
        assert!(block.is_expanded());
    }

    #[test]
    fn test_tool_block_display_header() {
        let mut block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({}));

        // Running and expanded
        let header = block.display_header();
        assert!(header.contains("▼")); // Expanded indicator
        assert!(header.contains("⏳")); // Running status
        assert!(header.contains("ripgrep"));

        // Completed and collapsed
        block.complete("result");
        let header = block.display_header();
        assert!(header.contains("▶")); // Collapsed indicator
        assert!(header.contains("✓")); // Completed status
    }

    #[test]
    fn test_tool_block_format_args_ripgrep() {
        let args = serde_json::json!({"pattern": "test", "path": "src/"});
        let formatted = ToolBlock::format_args(&args, "ripgrep");
        assert!(formatted.contains("rg"));
        assert!(formatted.contains("test"));
        assert!(formatted.contains("src/"));
    }

    #[test]
    fn test_tool_block_format_args_read_file() {
        let args = serde_json::json!({"path": "src/main.rs"});
        let formatted = ToolBlock::format_args(&args, "read_file");
        assert!(formatted.contains("cat"));
        assert!(formatted.contains("src/main.rs"));
    }

    #[test]
    fn test_tool_block_format_args_shell() {
        let args = serde_json::json!({"command": "ls -la"});
        let formatted = ToolBlock::format_args(&args, "shell");
        assert!(formatted.contains("shell>"));
        assert!(formatted.contains("ls -la"));
    }

    #[test]
    fn test_tool_block_format_args_generic() {
        let args = serde_json::json!({"key1": "value1", "key2": "value2"});
        let formatted = ToolBlock::format_args(&args, "unknown_tool");
        assert!(formatted.contains("key1"));
        assert!(formatted.contains("value1"));
    }

    #[test]
    fn test_tool_block_truncate_result() {
        let short = "short result";
        assert_eq!(ToolBlock::truncate_result(short, 200), short);

        let long = "a".repeat(300);
        let truncated = ToolBlock::truncate_result(&long, 200);
        assert!(truncated.len() <= 200);
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_tool_block_visible_lines() {
        let mut block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({"pattern": "test"}));

        // Expanded running block
        let expanded_lines = block.visible_lines();
        assert!(expanded_lines > 1);

        // Collapsed block
        block.toggle();
        assert_eq!(block.visible_lines(), 1);
    }

    #[test]
    fn test_tool_block_render_lines() {
        let block = ToolBlock::new("test-1", "ripgrep", serde_json::json!({"pattern": "test"}));

        let lines = block.render_lines();
        assert!(!lines.is_empty());

        // Check header is present
        let header_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header_text.contains("ripgrep"));
    }

    #[test]
    fn test_tool_block_manager_new() {
        let manager = ToolBlockManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_tool_block_manager_add_started() {
        let mut manager = ToolBlockManager::new();

        manager.add_started("ripgrep", serde_json::json!({"pattern": "test"}), "msg-1");

        assert_eq!(manager.len(), 1);
        assert!(manager.has_running());
        assert_eq!(manager.current_tool(), Some("ripgrep"));
    }

    #[test]
    fn test_tool_block_manager_complete_last() {
        let mut manager = ToolBlockManager::new();

        manager.add_started("ripgrep", serde_json::json!({}), "msg-1");
        manager.complete_last("Found 5 matches");

        assert!(!manager.has_running());
        assert_eq!(manager.current_tool(), None);
        assert_eq!(manager.blocks()[0].status, ToolStatus::Completed);
    }

    #[test]
    fn test_tool_block_manager_multiple_tools() {
        let mut manager = ToolBlockManager::new();

        manager.add_started("ripgrep", serde_json::json!({}), "msg-1");
        manager.complete_last("result 1");

        manager.add_started("read_file", serde_json::json!({}), "msg-1");
        manager.complete_last("result 2");

        assert_eq!(manager.len(), 2);
        assert!(!manager.has_running());
    }

    #[test]
    fn test_tool_block_manager_toggle() {
        let mut manager = ToolBlockManager::new();

        manager.add_started("ripgrep", serde_json::json!({}), "msg-1");
        let id = manager.blocks()[0].id.clone();

        assert!(manager.blocks()[0].is_expanded());
        manager.toggle(&id);
        assert!(!manager.blocks()[0].is_expanded());
    }

    #[test]
    fn test_tool_block_manager_clear() {
        let mut manager = ToolBlockManager::new();

        manager.add_started("ripgrep", serde_json::json!({}), "msg-1");
        manager.add_started("read_file", serde_json::json!({}), "msg-1");

        assert_eq!(manager.len(), 2);
        manager.clear();
        assert!(manager.is_empty());
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

/// Property-based tests for tool block display
///
/// **Property 4: Tool Block Display**
/// **Validates: Requirements 4.1, 4.2, 4.3, 4.5**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a random tool name
    fn arb_tool_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("ripgrep".to_string()),
            Just("read_file".to_string()),
            Just("write_file".to_string()),
            Just("shell".to_string()),
            Just("list_files".to_string()),
            Just("grep".to_string()),
        ]
    }

    /// Generate random tool arguments
    fn arb_tool_args() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            Just(serde_json::json!({})),
            Just(serde_json::json!({"pattern": "test"})),
            Just(serde_json::json!({"path": "src/main.rs"})),
            Just(serde_json::json!({"command": "ls -la"})),
            Just(serde_json::json!({"key": "value", "another": 123})),
        ]
    }

    /// Generate a random result string
    fn arb_result() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("".to_string()),
            "[a-zA-Z0-9 .,!?\n]{1,100}",
            "[a-zA-Z0-9 .,!?\n]{100,500}",
        ]
    }

    /// Generate a random tool block ID
    fn arb_block_id() -> impl Strategy<Value = String> {
        "[a-z0-9-]{5,20}"
    }

    proptest! {
        /// **Feature: tui-llm-integration, Property 4: Tool Block Display**
        /// **Validates: Requirements 4.1, 4.2**
        ///
        /// For any tool execution by the ChatAgent, the TUI SHALL display a Tool_Block
        /// showing the tool name and arguments.
        #[test]
        fn prop_tool_block_shows_name_and_args(
            tool_name in arb_tool_name(),
            args in arb_tool_args(),
            block_id in arb_block_id(),
        ) {
            let block = ToolBlock::new(&block_id, &tool_name, args.clone());

            // Verify tool name is stored (Requirement 4.1)
            prop_assert_eq!(&block.tool_name, &tool_name,
                "Tool name should be stored correctly");

            // Verify args are stored (Requirement 4.2)
            prop_assert_eq!(&block.args, &args,
                "Tool arguments should be stored correctly");

            // Verify header contains tool name
            let header = block.display_header();
            prop_assert!(header.contains(&tool_name),
                "Header should contain tool name '{}', got: {}", tool_name, header);

            // Verify rendered lines contain tool info
            let lines = block.render_lines();
            prop_assert!(!lines.is_empty(),
                "Tool block should render at least one line");

            let all_text: String = lines.iter()
                .flat_map(|l| l.spans.iter())
                .map(|s| s.content.as_ref())
                .collect();
            prop_assert!(all_text.contains(&tool_name),
                "Rendered content should contain tool name");
        }

        /// **Feature: tui-llm-integration, Property 4: Tool Block Display**
        /// **Validates: Requirements 4.3, 4.4**
        ///
        /// When a tool completes, the Tool_Block SHALL show a preview of the result
        /// and SHALL be collapsed by default.
        #[test]
        fn prop_tool_block_shows_result_and_collapses(
            tool_name in arb_tool_name(),
            args in arb_tool_args(),
            result in arb_result(),
            block_id in arb_block_id(),
        ) {
            let mut block = ToolBlock::new(&block_id, &tool_name, args);

            // Initially running and expanded
            prop_assert!(block.is_running(),
                "New block should be running");
            prop_assert!(block.is_expanded(),
                "Running block should be expanded");

            // Complete the tool
            block.complete(&result);

            // Verify status changed (Requirement 4.3)
            prop_assert_eq!(block.status, ToolStatus::Completed,
                "Completed block should have Completed status");

            // Verify collapsed after completion (Requirement 4.4)
            prop_assert!(!block.is_expanded(),
                "Completed block should be collapsed by default");

            // Verify result is stored
            prop_assert!(block.result_preview.is_some(),
                "Completed block should have result preview");
            prop_assert!(block.full_result.is_some(),
                "Completed block should have full result");

            // Verify result preview is truncated if needed
            if let Some(ref preview) = block.result_preview {
                prop_assert!(preview.len() <= 203, // 200 + "..."
                    "Result preview should be truncated to ~200 chars");
            }
        }

        /// **Feature: tui-llm-integration, Property 4: Tool Block Display**
        /// **Validates: Requirements 4.5**
        ///
        /// When multiple tools are executed, the TUI SHALL display them in sequence.
        #[test]
        fn prop_multiple_tools_in_sequence(
            tool_count in 1usize..5usize,
        ) {
            let mut manager = ToolBlockManager::new();
            let tools = ["ripgrep", "read_file", "write_file", "shell", "list_files"];

            // Add multiple tools
            for i in 0..tool_count {
                let tool = tools[i % tools.len()];
                manager.add_started(tool, serde_json::json!({}), "msg-1");
                manager.complete_last(format!("result {}", i));
            }

            // Verify all tools are tracked (Requirement 4.5)
            prop_assert_eq!(manager.len(), tool_count,
                "Manager should track all {} tools", tool_count);

            // Verify tools are in order
            for (i, block) in manager.blocks().iter().enumerate() {
                let expected_tool = tools[i % tools.len()];
                prop_assert_eq!(&block.tool_name, expected_tool,
                    "Tool {} should be '{}', got '{}'", i, expected_tool, block.tool_name);
            }

            // Verify all tools completed
            prop_assert!(!manager.has_running(),
                "No tools should be running after all completed");

            // Verify render produces lines for all tools
            let all_lines = manager.render_all_lines();
            prop_assert!(!all_lines.is_empty(),
                "Should render lines for all tools");
        }

        /// **Feature: tui-llm-integration, Property 4: Tool Block Display**
        /// **Validates: Requirements 4.1, 4.2, 4.3, 4.5**
        ///
        /// For any sequence of tool operations (start, complete, fail),
        /// the tool block manager SHALL maintain consistent state.
        #[test]
        fn prop_tool_manager_state_consistency(
            operations in prop::collection::vec(0u8..3, 1..10),
        ) {
            let mut manager = ToolBlockManager::new();
            let mut expected_count = 0;
            let mut expected_running = 0;

            for op in operations {
                match op {
                    0 => {
                        // Start a new tool
                        manager.add_started("test_tool", serde_json::json!({}), "msg-1");
                        expected_count += 1;
                        expected_running += 1;
                    }
                    1 => {
                        // Complete the last running tool
                        if expected_running > 0 {
                            manager.complete_last("result");
                            expected_running -= 1;
                        }
                    }
                    _ => {
                        // Fail the last running tool
                        if expected_running > 0 {
                            manager.fail_last("error");
                            expected_running -= 1;
                        }
                    }
                }

                // Verify count is correct
                prop_assert_eq!(manager.len(), expected_count,
                    "Tool count should be {}", expected_count);

                // Verify running count is correct
                let actual_running = manager.blocks().iter().filter(|b| b.is_running()).count();
                prop_assert_eq!(actual_running, expected_running,
                    "Running count should be {}", expected_running);

                // Verify has_running is consistent
                prop_assert_eq!(manager.has_running(), expected_running > 0,
                    "has_running should match expected_running > 0");
            }
        }

        /// **Feature: tui-llm-integration, Property 4: Tool Block Display**
        /// **Validates: Requirements 4.4**
        ///
        /// Toggle operation should be idempotent when applied twice.
        #[test]
        fn prop_toggle_idempotent(
            tool_name in arb_tool_name(),
            block_id in arb_block_id(),
        ) {
            let mut block = ToolBlock::new(&block_id, &tool_name, serde_json::json!({}));

            let initial_state = block.is_expanded();

            // Toggle twice should return to original state
            block.toggle();
            block.toggle();

            prop_assert_eq!(block.is_expanded(), initial_state,
                "Double toggle should return to original state");
        }
    }
}
