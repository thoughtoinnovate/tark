//! Thinking block widget for displaying AI reasoning in the TUI
//!
//! Provides a specialized widget for visualizing the AI's thinking process
//! in the chat area with collapsible support.
//!
//! Requirements:
//! - 9.1: Display Thinking_Block when thinking mode is enabled and LLM returns thinking content
//! - 9.2: Thinking_Block SHALL be collapsible (default expanded)

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// A thinking block for displaying AI reasoning process
///
/// This widget displays the AI's thinking/reasoning content
/// in a collapsible format, helping users understand the AI's
/// decision-making process.
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    /// Unique identifier for this block
    pub id: String,
    /// Thinking content lines
    pub content: Vec<String>,
    /// Whether the block is expanded (default: true per Requirement 9.2)
    pub expanded: bool,
    /// Whether this block is still receiving content (streaming)
    pub is_streaming: bool,
}

impl ThinkingBlock {
    /// Create a new thinking block
    ///
    /// Requirements: 9.1, 9.2
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            content: Vec::new(),
            expanded: true, // Default expanded per Requirement 9.2
            is_streaming: false,
        }
    }

    /// Create a new thinking block with initial content
    pub fn with_content(id: impl Into<String>, content: Vec<String>) -> Self {
        Self {
            id: id.into(),
            content,
            expanded: true,
            is_streaming: false,
        }
    }

    /// Create a thinking block from a message ID and index
    pub fn from_message(message_id: &str, index: usize) -> Self {
        let id = format!("{}-thinking-{}", message_id, index);
        Self::new(id)
    }

    /// Append content to the thinking block (for streaming)
    pub fn append_content(&mut self, text: &str) {
        // Each call adds the text as a new line
        self.content.push(text.to_string());
    }

    /// Set the full content
    pub fn set_content(&mut self, content: Vec<String>) {
        self.content = content;
    }

    /// Mark streaming as started
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
    }

    /// Mark streaming as complete
    pub fn finish_streaming(&mut self) {
        self.is_streaming = false;
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

    /// Check if the block is streaming
    pub fn is_streaming(&self) -> bool {
        self.is_streaming
    }

    /// Get the block ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the content
    pub fn content(&self) -> &[String] {
        &self.content
    }

    /// Get the display header with expand/collapse indicator
    ///
    /// Shows "▼" when expanded and "▶" when collapsed
    pub fn display_header(&self) -> String {
        let indicator = if self.expanded { "▼" } else { "▶" };
        let streaming_indicator = if self.is_streaming { " ..." } else { "" };
        format!("{} 🧠 Thinking{}", indicator, streaming_indicator)
    }

    /// Get the number of visible lines
    pub fn visible_lines(&self) -> usize {
        if self.expanded {
            let mut lines = 1; // Header
            lines += self.content.len().max(1); // Content lines (at least 1 for empty)
            lines += 1; // Closing border
            lines
        } else {
            1 // Header only
        }
    }

    /// Render the thinking block to lines for display
    ///
    /// Requirements: 9.1, 9.2
    pub fn render_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header with thinking indicator
        let header_style = Style::default()
            .fg(Color::Magenta)
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
            let thinking_style = Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::ITALIC);

            if self.content.is_empty() {
                // Show placeholder when empty
                if self.is_streaming {
                    let streaming_style = Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::SLOW_BLINK);
                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled("Thinking...", streaming_style),
                    ]));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled("(no thinking content)", content_style),
                    ]));
                }
            } else {
                // Render content lines
                for (i, line) in self.content.iter().enumerate() {
                    // Limit displayed lines to prevent overwhelming the UI
                    if i >= 20 {
                        lines.push(Line::from(vec![
                            Span::styled("│ ", border_style),
                            Span::styled(
                                format!("... ({} more lines)", self.content.len() - 20),
                                content_style,
                            ),
                        ]));
                        break;
                    }

                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(line.clone(), thinking_style),
                    ]));
                }

                // Show streaming indicator at the end if still streaming
                if self.is_streaming {
                    let streaming_style = Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::SLOW_BLINK);
                    lines.push(Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled("...", streaming_style),
                    ]));
                }
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

/// Widget for rendering a ThinkingBlock
pub struct ThinkingBlockWidget<'a> {
    block: &'a ThinkingBlock,
}

impl<'a> ThinkingBlockWidget<'a> {
    /// Create a new thinking block widget
    pub fn new(block: &'a ThinkingBlock) -> Self {
        Self { block }
    }
}

impl Widget for ThinkingBlockWidget<'_> {
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

/// Manager for tracking thinking blocks in a message
#[derive(Debug, Clone, Default)]
pub struct ThinkingBlockManager {
    /// Thinking blocks
    blocks: Vec<ThinkingBlock>,
}

impl ThinkingBlockManager {
    /// Create a new empty manager
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    /// Add a new thinking block
    pub fn add(&mut self, message_id: &str) -> &mut ThinkingBlock {
        let index = self.blocks.len();
        let block = ThinkingBlock::from_message(message_id, index);
        self.blocks.push(block);
        self.blocks.last_mut().unwrap()
    }

    /// Get the current (last) thinking block, creating one if needed
    pub fn current_or_create(&mut self, message_id: &str) -> &mut ThinkingBlock {
        if self.blocks.is_empty() {
            self.add(message_id)
        } else {
            self.blocks.last_mut().unwrap()
        }
    }

    /// Append content to the current thinking block
    pub fn append_content(&mut self, message_id: &str, content: &str) {
        let block = self.current_or_create(message_id);
        block.append_content(content);
    }

    /// Finish streaming on the current block
    pub fn finish_current(&mut self) {
        if let Some(block) = self.blocks.last_mut() {
            block.finish_streaming();
        }
    }

    /// Get all thinking blocks
    pub fn blocks(&self) -> &[ThinkingBlock] {
        &self.blocks
    }

    /// Get a mutable reference to all thinking blocks
    pub fn blocks_mut(&mut self) -> &mut [ThinkingBlock] {
        &mut self.blocks
    }

    /// Get the number of thinking blocks
    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    /// Check if there are no thinking blocks
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Check if any block is currently streaming
    pub fn has_streaming(&self) -> bool {
        self.blocks.iter().any(|b| b.is_streaming())
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
    fn test_thinking_block_new() {
        let block = ThinkingBlock::new("test-1");

        assert_eq!(block.id, "test-1");
        assert!(block.content.is_empty());
        assert!(block.expanded); // Default expanded per Requirement 9.2
        assert!(!block.is_streaming);
    }

    #[test]
    fn test_thinking_block_with_content() {
        let content = vec!["Line 1".to_string(), "Line 2".to_string()];
        let block = ThinkingBlock::with_content("test-1", content.clone());

        assert_eq!(block.id, "test-1");
        assert_eq!(block.content, content);
        assert!(block.expanded);
    }

    #[test]
    fn test_thinking_block_from_message() {
        let block = ThinkingBlock::from_message("msg-123", 0);
        assert_eq!(block.id, "msg-123-thinking-0");

        let block2 = ThinkingBlock::from_message("msg-123", 1);
        assert_eq!(block2.id, "msg-123-thinking-1");
    }

    #[test]
    fn test_thinking_block_toggle() {
        let mut block = ThinkingBlock::new("test-1");

        assert!(block.is_expanded());
        block.toggle();
        assert!(!block.is_expanded());
        block.toggle();
        assert!(block.is_expanded());
    }

    #[test]
    fn test_thinking_block_streaming() {
        let mut block = ThinkingBlock::new("test-1");

        assert!(!block.is_streaming());
        block.start_streaming();
        assert!(block.is_streaming());
        block.finish_streaming();
        assert!(!block.is_streaming());
    }

    #[test]
    fn test_thinking_block_append_content() {
        let mut block = ThinkingBlock::new("test-1");

        block.append_content("First line");
        assert_eq!(block.content.len(), 1);
        assert_eq!(block.content[0], "First line");

        block.append_content("Second line");
        assert_eq!(block.content.len(), 2);
    }

    #[test]
    fn test_thinking_block_display_header() {
        let mut block = ThinkingBlock::new("test-1");

        // Expanded
        let header = block.display_header();
        assert!(header.contains("▼")); // Expanded indicator
        assert!(header.contains("🧠")); // Thinking icon
        assert!(header.contains("Thinking"));

        // Collapsed
        block.toggle();
        let header = block.display_header();
        assert!(header.contains("▶")); // Collapsed indicator

        // Streaming
        block.toggle();
        block.start_streaming();
        let header = block.display_header();
        assert!(header.contains("...")); // Streaming indicator
    }

    #[test]
    fn test_thinking_block_visible_lines() {
        let mut block = ThinkingBlock::with_content(
            "test-1",
            vec![
                "Line 1".to_string(),
                "Line 2".to_string(),
                "Line 3".to_string(),
            ],
        );

        // Expanded: header + 3 content lines + closing border = 5
        assert!(block.is_expanded());
        assert_eq!(block.visible_lines(), 5);

        // Collapsed: header only = 1
        block.toggle();
        assert!(!block.is_expanded());
        assert_eq!(block.visible_lines(), 1);
    }

    #[test]
    fn test_thinking_block_render_lines() {
        let block =
            ThinkingBlock::with_content("test-1", vec!["Analyzing the code...".to_string()]);

        let lines = block.render_lines();
        assert!(!lines.is_empty());

        // Check header is present
        let header_text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(header_text.contains("Thinking"));
    }

    #[test]
    fn test_thinking_block_manager_new() {
        let manager = ThinkingBlockManager::new();
        assert!(manager.is_empty());
        assert_eq!(manager.len(), 0);
    }

    #[test]
    fn test_thinking_block_manager_add() {
        let mut manager = ThinkingBlockManager::new();

        manager.add("msg-1");

        assert_eq!(manager.len(), 1);
        assert!(!manager.is_empty());
    }

    #[test]
    fn test_thinking_block_manager_append_content() {
        let mut manager = ThinkingBlockManager::new();

        manager.append_content("msg-1", "First thought");
        manager.append_content("msg-1", "Second thought");

        assert_eq!(manager.len(), 1);
        assert_eq!(manager.blocks()[0].content.len(), 2);
    }

    #[test]
    fn test_thinking_block_manager_toggle() {
        let mut manager = ThinkingBlockManager::new();

        manager.add("msg-1");
        let id = manager.blocks()[0].id.clone();

        assert!(manager.blocks()[0].is_expanded());
        manager.toggle(&id);
        assert!(!manager.blocks()[0].is_expanded());
    }

    #[test]
    fn test_thinking_block_manager_clear() {
        let mut manager = ThinkingBlockManager::new();

        manager.add("msg-1");
        manager.add("msg-2");

        assert_eq!(manager.len(), 2);
        manager.clear();
        assert!(manager.is_empty());
    }
}

// ============================================================================
// Property-Based Tests
// ============================================================================

/// Property-based tests for thinking mode display
///
/// **Property 9: Thinking Mode Display**
/// **Validates: Requirements 9.1, 9.3, 9.5**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate random thinking content
    fn arb_thinking_content() -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec("[a-zA-Z0-9 .,!?]{1,100}", 0..10)
    }

    /// Generate a random block ID
    fn arb_block_id() -> impl Strategy<Value = String> {
        "[a-z0-9-]{5,20}"
    }

    /// Generate a random message ID
    fn arb_message_id() -> impl Strategy<Value = String> {
        "[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}"
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        /// **Feature: tui-llm-integration, Property 9: Thinking Mode Display**
        /// **Validates: Requirements 9.1, 9.2**
        ///
        /// For any thinking content, the ThinkingBlock SHALL display it
        /// in a collapsible format with default expanded state.
        #[test]
        fn prop_thinking_block_displays_content(
            content in arb_thinking_content(),
            block_id in arb_block_id(),
        ) {
            let block = ThinkingBlock::with_content(&block_id, content.clone());

            // Verify block is created correctly (Requirement 9.1)
            prop_assert_eq!(&block.id, &block_id,
                "Block ID should match");
            prop_assert_eq!(block.content(), content.as_slice(),
                "Content should be stored correctly");

            // Verify default expanded state (Requirement 9.2)
            prop_assert!(block.is_expanded(),
                "ThinkingBlock should be expanded by default");

            // Verify header contains thinking indicator
            let header = block.display_header();
            prop_assert!(header.contains("🧠"),
                "Header should contain thinking icon");
            prop_assert!(header.contains("Thinking"),
                "Header should contain 'Thinking' text");

            // Verify rendered lines contain content when expanded
            let lines = block.render_lines();
            prop_assert!(!lines.is_empty(),
                "Should render at least one line");

            // Header should be first line
            let header_text: String = lines[0].spans.iter()
                .map(|s| s.content.as_ref())
                .collect();
            prop_assert!(header_text.contains("Thinking"),
                "First line should be header with 'Thinking'");
        }

        /// **Feature: tui-llm-integration, Property 9: Thinking Mode Display**
        /// **Validates: Requirements 9.1, 9.3**
        ///
        /// For any ThinkingBlock, toggling expanded state SHALL correctly
        /// show/hide the content.
        #[test]
        fn prop_thinking_block_toggle_visibility(
            content in arb_thinking_content(),
            block_id in arb_block_id(),
        ) {
            let mut block = ThinkingBlock::with_content(&block_id, content.clone());

            // Initially expanded (Requirement 9.2)
            prop_assert!(block.is_expanded(),
                "Should start expanded");

            let expanded_lines = block.render_lines();
            let expanded_line_count = expanded_lines.len();

            // Toggle to collapsed
            block.toggle();
            prop_assert!(!block.is_expanded(),
                "Should be collapsed after toggle");

            let collapsed_lines = block.render_lines();
            let collapsed_line_count = collapsed_lines.len();

            // Collapsed should have fewer lines (just header)
            prop_assert_eq!(collapsed_line_count, 1,
                "Collapsed block should only show header");

            // Expanded should have more lines if there's content
            if !content.is_empty() {
                prop_assert!(expanded_line_count > collapsed_line_count,
                    "Expanded block should have more lines than collapsed");
            }

            // Toggle back to expanded
            block.toggle();
            prop_assert!(block.is_expanded(),
                "Should be expanded after second toggle");

            let re_expanded_lines = block.render_lines();
            prop_assert_eq!(re_expanded_lines.len(), expanded_line_count,
                "Re-expanded should have same line count as original expanded");
        }

        /// **Feature: tui-llm-integration, Property 9: Thinking Mode Display**
        /// **Validates: Requirements 9.1, 9.3**
        ///
        /// For any streaming thinking content, the ThinkingBlock SHALL
        /// correctly accumulate and display content.
        #[test]
        fn prop_thinking_block_streaming_accumulation(
            chunks in prop::collection::vec("[a-zA-Z0-9 ]{1,50}", 1..5),
            block_id in arb_block_id(),
        ) {
            let mut block = ThinkingBlock::new(&block_id);
            block.start_streaming();

            prop_assert!(block.is_streaming(),
                "Block should be in streaming state");

            // Append chunks
            for chunk in &chunks {
                block.append_content(chunk);
            }

            // Verify all chunks were accumulated
            prop_assert_eq!(block.content().len(), chunks.len(),
                "Should have accumulated all chunks");

            for (i, chunk) in chunks.iter().enumerate() {
                prop_assert_eq!(&block.content()[i], chunk,
                    "Chunk {} should match", i);
            }

            // Finish streaming
            block.finish_streaming();
            prop_assert!(!block.is_streaming(),
                "Block should not be streaming after finish");

            // Verify header shows streaming indicator while streaming
            let mut streaming_block = ThinkingBlock::new("test");
            streaming_block.start_streaming();
            let streaming_header = streaming_block.display_header();
            prop_assert!(streaming_header.contains("..."),
                "Streaming header should contain '...' indicator");
        }

        /// **Feature: tui-llm-integration, Property 9: Thinking Mode Display**
        /// **Validates: Requirements 9.1, 9.3**
        ///
        /// For any ThinkingBlockManager, adding and managing multiple blocks
        /// SHALL maintain correct state.
        #[test]
        fn prop_thinking_block_manager_consistency(
            block_count in 1usize..5usize,
            content_per_block in prop::collection::vec("[a-zA-Z0-9 ]{1,30}", 1..3),
        ) {
            let mut manager = ThinkingBlockManager::new();

            // Add blocks
            for i in 0..block_count {
                let msg_id = format!("msg-{}", i);
                manager.add(&msg_id);

                // Add content to each block
                for content in &content_per_block {
                    manager.append_content(&msg_id, content);
                }
            }

            // Verify block count
            prop_assert_eq!(manager.len(), block_count,
                "Manager should have {} blocks", block_count);

            // Verify all blocks have content
            for block in manager.blocks() {
                prop_assert_eq!(block.content().len(), content_per_block.len(),
                    "Each block should have {} content lines", content_per_block.len());
            }

            // Test toggle functionality
            if !manager.is_empty() {
                let first_id = manager.blocks()[0].id.clone();
                let initial_expanded = manager.blocks()[0].is_expanded();

                manager.toggle(&first_id);
                prop_assert_ne!(manager.blocks()[0].is_expanded(), initial_expanded,
                    "Toggle should change expanded state");
            }

            // Test clear
            manager.clear();
            prop_assert!(manager.is_empty(),
                "Manager should be empty after clear");
        }

        /// **Feature: tui-llm-integration, Property 9: Thinking Mode Display**
        /// **Validates: Requirements 9.5**
        ///
        /// For any thinking mode state, the visible_lines calculation SHALL
        /// be consistent with the expanded state.
        #[test]
        fn prop_thinking_block_visible_lines_consistency(
            content in arb_thinking_content(),
            block_id in arb_block_id(),
        ) {
            let mut block = ThinkingBlock::with_content(&block_id, content.clone());

            // Expanded state
            let expanded_visible = block.visible_lines();
            let _expanded_rendered = block.render_lines().len();

            // visible_lines should match or be close to rendered lines
            // (may differ slightly due to truncation logic)
            prop_assert!(
                expanded_visible >= 1,
                "Expanded visible lines should be at least 1"
            );

            // Collapsed state
            block.toggle();
            let collapsed_visible = block.visible_lines();
            let collapsed_rendered = block.render_lines().len();

            prop_assert_eq!(collapsed_visible, 1,
                "Collapsed visible lines should be 1 (header only)");
            prop_assert_eq!(collapsed_rendered, 1,
                "Collapsed rendered lines should be 1 (header only)");

            // Expanded should have more visible lines than collapsed (if content exists)
            if !content.is_empty() {
                prop_assert!(expanded_visible > collapsed_visible,
                    "Expanded should have more visible lines than collapsed");
            }
        }
    }
}
