//! Collapsible block widget for the TUI
//!
//! Provides collapsible content blocks for thinking and tool execution
//! in the chat area with expand/collapse support.

#![allow(dead_code)]

use std::collections::HashMap;
use uuid::Uuid;

// ============================================================================
// Block Types (Requirements 7.1, 8.1)
// ============================================================================

/// Type of collapsible block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockType {
    /// Thinking block showing AI reasoning process
    Thinking,
    /// Tool execution block showing tool calls and results
    Tool,
}

impl BlockType {
    /// Get the icon for this block type
    pub fn icon(&self) -> &'static str {
        match self {
            BlockType::Thinking => "🧠",
            BlockType::Tool => "🔧",
        }
    }

    /// Get the default expanded state for this block type
    /// Both Thinking and Tool blocks default to collapsed for cleaner UI
    pub fn default_expanded(&self) -> bool {
        match self {
            BlockType::Thinking => false,
            BlockType::Tool => false,
        }
    }
}

// ============================================================================
// Collapsible Block (Requirements 7.1, 7.2, 7.3, 8.1, 8.2, 8.3)
// ============================================================================

/// A collapsible content block in the chat area
#[derive(Debug, Clone, PartialEq)]
pub struct CollapsibleBlock {
    /// Unique identifier for this block
    pub id: String,
    /// Block type (thinking, tool)
    pub block_type: BlockType,
    /// Header text (e.g., "Thinking", "ripgrep")
    pub header: String,
    /// Content lines
    pub content: Vec<String>,
    /// Whether the block is expanded
    pub expanded: bool,
    /// Whether this block represents an error (Requirements 7.2)
    pub has_error: bool,
}

impl CollapsibleBlock {
    /// Create a new collapsible block
    pub fn new(
        id: impl Into<String>,
        block_type: BlockType,
        header: impl Into<String>,
        content: Vec<String>,
    ) -> Self {
        let expanded = block_type.default_expanded();
        Self {
            id: id.into(),
            block_type,
            header: header.into(),
            content,
            expanded,
            has_error: false,
        }
    }

    /// Create a new thinking block
    pub fn thinking(id: impl Into<String>, content: Vec<String>) -> Self {
        Self::new(id, BlockType::Thinking, "Thinking", content)
    }

    /// Create a new tool block
    pub fn tool(id: impl Into<String>, tool_name: impl Into<String>, content: Vec<String>) -> Self {
        Self::new(id, BlockType::Tool, tool_name, content)
    }

    /// Get the display header with expand/collapse indicator
    /// Shows "▼" when expanded and "▶" when collapsed
    pub fn display_header(&self) -> String {
        let indicator = if self.expanded { "▼" } else { "▶" };
        let icon = self.block_type.icon();
        match self.block_type {
            BlockType::Thinking => format!("{} {} {}", indicator, icon, self.header),
            BlockType::Tool => format!("{} {} Tool: {}", indicator, icon, self.header),
        }
    }

    /// Toggle expanded state
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Get visible line count
    /// Returns 1 for collapsed (header only), or header + content + closing border when expanded
    pub fn visible_lines(&self) -> usize {
        if self.expanded {
            // header + content lines + closing border
            1 + self.content.len() + 1
        } else {
            // header only
            1
        }
    }

    /// Check if the block is expanded
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Set the expanded state
    pub fn set_expanded(&mut self, expanded: bool) {
        self.expanded = expanded;
    }

    /// Get the block ID
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Get the block type
    pub fn block_type(&self) -> BlockType {
        self.block_type
    }

    /// Get the header text
    pub fn header(&self) -> &str {
        &self.header
    }

    /// Get the content lines
    pub fn content(&self) -> &[String] {
        &self.content
    }
}

// ============================================================================
// Collapsible Block State (Requirements 7.7, 8.9)
// ============================================================================

/// State tracking for collapsible blocks
#[derive(Debug, Default, Clone)]
pub struct CollapsibleBlockState {
    /// Map of block ID to expanded state
    states: HashMap<String, bool>,
    /// Map of block ID to scroll offset (for thinking blocks)
    scroll_offsets: HashMap<String, usize>,
    /// Currently focused block ID (for keyboard navigation)
    focused_block: Option<String>,
}

impl CollapsibleBlockState {
    /// Create a new empty state
    pub fn new() -> Self {
        Self {
            states: HashMap::new(),
            scroll_offsets: HashMap::new(),
            focused_block: None,
        }
    }

    /// Get expanded state for a block
    /// Returns the stored state if present, otherwise returns the default based on block type
    pub fn is_expanded(&self, id: &str, block_type: BlockType) -> bool {
        self.states
            .get(id)
            .copied()
            .unwrap_or_else(|| block_type.default_expanded())
    }

    /// Toggle a block's state
    /// If the block has no stored state, it will be set to the opposite of the default
    pub fn toggle(&mut self, id: &str, block_type: BlockType) {
        let current = self.is_expanded(id, block_type);
        self.states.insert(id.to_string(), !current);
    }

    /// Set a block's state explicitly
    pub fn set(&mut self, id: &str, expanded: bool) {
        self.states.insert(id.to_string(), expanded);
    }

    /// Remove a block's state (will revert to default)
    pub fn remove(&mut self, id: &str) {
        self.states.remove(id);
    }

    /// Clear all stored states
    pub fn clear(&mut self) {
        self.states.clear();
        self.scroll_offsets.clear();
        self.focused_block = None;
    }

    /// Get the number of stored states
    pub fn len(&self) -> usize {
        self.states.len()
    }

    /// Check if there are no stored states
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    /// Get scroll offset for a block (0 if not set)
    pub fn scroll_offset(&self, id: &str) -> usize {
        self.scroll_offsets.get(id).copied().unwrap_or(0)
    }

    /// Set scroll offset for a block
    pub fn set_scroll_offset(&mut self, id: &str, offset: usize) {
        self.scroll_offsets.insert(id.to_string(), offset);
    }

    /// Scroll up within a block (returns true if scrolled)
    pub fn scroll_up(&mut self, id: &str) -> bool {
        let offset = self.scroll_offset(id);
        if offset > 0 {
            self.set_scroll_offset(id, offset - 1);
            true
        } else {
            false
        }
    }

    /// Scroll down within a block (returns true if scrolled)
    /// max_offset is total_lines - visible_lines
    pub fn scroll_down(&mut self, id: &str, max_offset: usize) -> bool {
        let offset = self.scroll_offset(id);
        if offset < max_offset {
            self.set_scroll_offset(id, offset + 1);
            true
        } else {
            false
        }
    }

    /// Get the currently focused block ID
    pub fn focused_block(&self) -> Option<&str> {
        self.focused_block.as_deref()
    }

    /// Set the focused block
    pub fn set_focused_block(&mut self, id: Option<String>) {
        self.focused_block = id;
    }

    /// Check if a block is focused
    pub fn is_focused(&self, id: &str) -> bool {
        self.focused_block.as_deref() == Some(id)
    }
}

// ============================================================================
// Tool Call Log Info (for parsing tool blocks)
// ============================================================================

/// Tool call log information for creating tool blocks
///
/// This struct mirrors the ToolCallLogInfo from agent_bridge but is defined
/// here to avoid circular dependencies.
#[derive(Debug, Clone)]
pub struct ToolCallInfo {
    /// Tool name (e.g., "ripgrep", "read_file")
    pub tool: String,
    /// Tool arguments as JSON
    pub args: serde_json::Value,
    /// Preview of the tool result
    pub result_preview: String,
    /// Error message if the tool failed (Requirements 7.2)
    pub error: Option<String>,
    /// Unique block ID for expand/collapse state tracking
    pub block_id: String,
}

impl ToolCallInfo {
    /// Create a new tool call info with auto-generated block_id
    pub fn new(
        tool: impl Into<String>,
        args: serde_json::Value,
        result_preview: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            args,
            result_preview: result_preview.into(),
            error: None,
            block_id: Uuid::new_v4().to_string(),
        }
    }

    /// Create a new tool call info with an error
    pub fn with_error(
        tool: impl Into<String>,
        args: serde_json::Value,
        error: impl Into<String>,
    ) -> Self {
        Self {
            tool: tool.into(),
            args,
            result_preview: String::new(),
            error: Some(error.into()),
            block_id: Uuid::new_v4().to_string(),
        }
    }

    /// Check if this tool call failed
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }
}

// ============================================================================
// Parsed Message Content (Requirements 7.1, 8.1)
// ============================================================================

/// A segment of parsed message content
#[derive(Debug, Clone, PartialEq)]
pub enum ContentSegment {
    /// Regular text content
    Text(String),
    /// A collapsible block (thinking or tool)
    Block(CollapsibleBlock),
}

/// Parsed message content with collapsible blocks
///
/// This struct represents the result of parsing an assistant message
/// to extract thinking blocks and tool blocks for collapsible display.
#[derive(Debug, Clone, Default)]
pub struct ParsedMessageContent {
    /// Content segments in order (text and blocks interleaved)
    pub segments: Vec<ContentSegment>,
}

impl ParsedMessageContent {
    /// Create a new empty parsed message content
    pub fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Parse raw message content into segments and blocks
    ///
    /// This method extracts:
    /// - Thinking blocks marked with `<thinking>...</thinking>` tags
    /// - Tool blocks from the provided tool call log
    ///
    /// # Arguments
    /// * `content` - The raw message content to parse
    /// * `tool_calls` - Tool calls associated with this message
    /// * `message_id` - Unique identifier for generating block IDs
    ///
    /// # Returns
    /// A `ParsedMessageContent` with text segments and collapsible blocks
    pub fn parse(content: &str, tool_calls: &[ToolCallInfo], message_id: &str) -> Self {
        let mut result = Self::new();

        // Parse thinking blocks from content
        let (text_without_thinking, thinking_blocks) =
            Self::extract_thinking_blocks(content, message_id);

        // Add thinking blocks first (they appear at the start of reasoning)
        for block in thinking_blocks {
            result.segments.push(ContentSegment::Block(block));
        }

        // Add the remaining text content if non-empty
        let trimmed_text = text_without_thinking.trim();
        if !trimmed_text.is_empty() {
            result
                .segments
                .push(ContentSegment::Text(trimmed_text.to_string()));
        }

        // Add tool blocks from tool calls
        for (idx, tool_call) in tool_calls.iter().enumerate() {
            let block = Self::create_tool_block(tool_call, message_id, idx);
            result.segments.push(ContentSegment::Block(block));
        }

        result
    }

    /// Extract thinking blocks from content
    ///
    /// Looks for `<thinking>...</thinking>` tags and extracts them as
    /// CollapsibleBlock instances.
    ///
    /// # Returns
    /// A tuple of (remaining_text, thinking_blocks)
    fn extract_thinking_blocks(content: &str, message_id: &str) -> (String, Vec<CollapsibleBlock>) {
        let mut thinking_blocks = Vec::new();
        let mut remaining_text = String::new();
        let mut block_counter = 0;

        // Simple state machine for parsing thinking blocks
        let mut chars = content.chars().peekable();
        let mut current_text = String::new();
        let mut in_thinking = false;
        let mut thinking_content = String::new();

        while let Some(c) = chars.next() {
            if !in_thinking {
                // Look for opening <thinking> tag
                if c == '<' {
                    let mut tag = String::from("<");
                    let mut is_thinking_tag = false;

                    // Collect potential tag
                    while let Some(&next_c) = chars.peek() {
                        tag.push(next_c);
                        chars.next();
                        if next_c == '>' {
                            break;
                        }
                        // Limit tag length to avoid infinite loops
                        if tag.len() > 20 {
                            break;
                        }
                    }

                    // Check if it's a thinking tag (case-insensitive)
                    // Support both <thinking> and <think> for different models
                    let tag_lower = tag.to_lowercase();
                    if tag_lower == "<thinking>" || tag_lower == "<think>" {
                        is_thinking_tag = true;
                        in_thinking = true;
                        // Save current text before thinking block
                        if !current_text.is_empty() {
                            remaining_text.push_str(&current_text);
                            current_text.clear();
                        }
                    }

                    if !is_thinking_tag {
                        // Not a thinking tag, add to current text
                        current_text.push_str(&tag);
                    }
                } else {
                    current_text.push(c);
                }
            } else {
                // Inside thinking block, look for closing </thinking> tag
                if c == '<' {
                    let mut tag = String::from("<");

                    // Collect potential tag
                    while let Some(&next_c) = chars.peek() {
                        tag.push(next_c);
                        chars.next();
                        if next_c == '>' {
                            break;
                        }
                        // Limit tag length
                        if tag.len() > 20 {
                            break;
                        }
                    }

                    // Check if it's a closing thinking tag
                    // Support both </thinking> and </think>
                    let tag_lower = tag.to_lowercase();
                    if tag_lower == "</thinking>" || tag_lower == "</think>" {
                        // End of thinking block
                        in_thinking = false;

                        // Create the thinking block
                        let block_id = format!("{}-thinking-{}", message_id, block_counter);
                        block_counter += 1;

                        let content_lines: Vec<String> = thinking_content
                            .lines()
                            .map(|l| l.to_string())
                            .filter(|l| !l.trim().is_empty())
                            .collect();

                        if !content_lines.is_empty() {
                            thinking_blocks
                                .push(CollapsibleBlock::thinking(block_id, content_lines));
                        }

                        thinking_content.clear();
                    } else {
                        // Not a closing tag, add to thinking content
                        thinking_content.push_str(&tag);
                    }
                } else {
                    thinking_content.push(c);
                }
            }
        }

        // Handle unclosed thinking block
        if in_thinking && !thinking_content.is_empty() {
            let block_id = format!("{}-thinking-{}", message_id, block_counter);
            let content_lines: Vec<String> = thinking_content
                .lines()
                .map(|l| l.to_string())
                .filter(|l| !l.trim().is_empty())
                .collect();

            if !content_lines.is_empty() {
                thinking_blocks.push(CollapsibleBlock::thinking(block_id, content_lines));
            }
        }

        // Add any remaining text
        if !current_text.is_empty() {
            remaining_text.push_str(&current_text);
        }

        (remaining_text, thinking_blocks)
    }

    /// Create a tool block from a tool call
    fn create_tool_block(
        tool_call: &ToolCallInfo,
        message_id: &str,
        index: usize,
    ) -> CollapsibleBlock {
        let block_id = format!("{}-tool-{}", message_id, index);

        // Format the content with command/arguments and result preview
        let mut content_lines = Vec::new();

        // Add arguments as formatted JSON (simplified)
        if !tool_call.args.is_null() {
            let args_str = Self::format_tool_args(&tool_call.args);
            if !args_str.is_empty() {
                content_lines.push(format!("$ {}", args_str));
            }
        }

        // Handle error case (Requirements 7.2) - display error in red
        if let Some(ref error) = tool_call.error {
            content_lines.push(String::new()); // Empty line separator
            content_lines.push(format!("✗ Error: {}", error));
            // Create block with error flag
            let mut block = CollapsibleBlock::tool(block_id, &tool_call.tool, content_lines);
            block.has_error = true;
            return block;
        }

        // Add result preview
        if !tool_call.result_preview.is_empty() {
            content_lines.push(String::new()); // Empty line separator
            for line in tool_call.result_preview.lines().take(10) {
                content_lines.push(format!("> {}", line));
            }
            // Indicate if truncated
            if tool_call.result_preview.lines().count() > 10 {
                content_lines.push("> ...".to_string());
            }
        }

        CollapsibleBlock::tool(block_id, &tool_call.tool, content_lines)
    }

    /// Format tool arguments for display
    fn format_tool_args(args: &serde_json::Value) -> String {
        match args {
            serde_json::Value::Object(map) => {
                // Extract key arguments for common tools
                if let Some(pattern) = map.get("pattern").and_then(|v| v.as_str()) {
                    return format!("pattern: \"{}\"", pattern);
                }
                if let Some(path) = map.get("path").and_then(|v| v.as_str()) {
                    return format!("path: {}", path);
                }
                if let Some(paths) = map.get("paths").and_then(|v| v.as_array()) {
                    let paths_str: Vec<&str> =
                        paths.iter().filter_map(|v| v.as_str()).take(3).collect();
                    if paths.len() > 3 {
                        return format!("{} +{} more", paths_str.join(", "), paths.len() - 3);
                    }
                    return paths_str.join(", ");
                }
                if let Some(command) = map.get("command").and_then(|v| v.as_str()) {
                    return command.to_string();
                }
                // Fallback: show first key-value pair
                if let Some((key, value)) = map.iter().next() {
                    return format!("{}: {}", key, value);
                }
                String::new()
            }
            _ => args.to_string(),
        }
    }

    /// Get all collapsible blocks from the parsed content
    pub fn blocks(&self) -> Vec<&CollapsibleBlock> {
        self.segments
            .iter()
            .filter_map(|seg| match seg {
                ContentSegment::Block(block) => Some(block),
                ContentSegment::Text(_) => None,
            })
            .collect()
    }

    /// Get all thinking blocks from the parsed content
    pub fn thinking_blocks(&self) -> Vec<&CollapsibleBlock> {
        self.blocks()
            .into_iter()
            .filter(|b| b.block_type == BlockType::Thinking)
            .collect()
    }

    /// Get all tool blocks from the parsed content
    pub fn tool_blocks(&self) -> Vec<&CollapsibleBlock> {
        self.blocks()
            .into_iter()
            .filter(|b| b.block_type == BlockType::Tool)
            .collect()
    }

    /// Get all text segments from the parsed content
    pub fn text_segments(&self) -> Vec<&str> {
        self.segments
            .iter()
            .filter_map(|seg| match seg {
                ContentSegment::Text(text) => Some(text.as_str()),
                ContentSegment::Block(_) => None,
            })
            .collect()
    }

    /// Check if the parsed content has any thinking blocks
    pub fn has_thinking(&self) -> bool {
        self.segments.iter().any(
            |seg| matches!(seg, ContentSegment::Block(b) if b.block_type == BlockType::Thinking),
        )
    }

    /// Check if the parsed content has any tool blocks
    pub fn has_tools(&self) -> bool {
        self.segments
            .iter()
            .any(|seg| matches!(seg, ContentSegment::Block(b) if b.block_type == BlockType::Tool))
    }

    /// Check if the parsed content is empty
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_block_type_icon() {
        assert_eq!(BlockType::Thinking.icon(), "🧠");
        assert_eq!(BlockType::Tool.icon(), "🔧");
    }

    #[test]
    fn test_block_type_default_expanded() {
        // Both thinking and tool blocks default to collapsed
        assert!(!BlockType::Thinking.default_expanded());
        assert!(!BlockType::Tool.default_expanded());
    }

    #[test]
    fn test_collapsible_block_new() {
        let block = CollapsibleBlock::new(
            "block-1",
            BlockType::Thinking,
            "Thinking",
            vec!["Line 1".to_string(), "Line 2".to_string()],
        );

        assert_eq!(block.id(), "block-1");
        assert_eq!(block.block_type(), BlockType::Thinking);
        assert_eq!(block.header(), "Thinking");
        assert_eq!(block.content().len(), 2);
        assert!(!block.is_expanded()); // Thinking defaults to collapsed
    }

    #[test]
    fn test_collapsible_block_thinking() {
        let block = CollapsibleBlock::thinking("think-1", vec!["Analyzing...".to_string()]);

        assert_eq!(block.block_type(), BlockType::Thinking);
        assert_eq!(block.header(), "Thinking");
        assert!(!block.is_expanded()); // Defaults to collapsed
    }

    #[test]
    fn test_collapsible_block_tool() {
        let block = CollapsibleBlock::tool("tool-1", "ripgrep", vec!["$ rg pattern".to_string()]);

        assert_eq!(block.block_type(), BlockType::Tool);
        assert_eq!(block.header(), "ripgrep");
        assert!(!block.is_expanded()); // Tool defaults to collapsed
    }

    #[test]
    fn test_collapsible_block_display_header() {
        let mut thinking = CollapsibleBlock::thinking("t1", vec![]);
        // Thinking defaults to collapsed
        assert_eq!(thinking.display_header(), "▶ 🧠 Thinking");
        thinking.toggle();
        assert_eq!(thinking.display_header(), "▼ 🧠 Thinking");

        let mut tool = CollapsibleBlock::tool("t2", "grep", vec![]);
        // Tool defaults to collapsed, icon is now 🔧
        assert_eq!(tool.display_header(), "▶ 🔧 Tool: grep");
        tool.toggle();
        assert_eq!(tool.display_header(), "▼ 🔧 Tool: grep");
    }

    #[test]
    fn test_collapsible_block_toggle() {
        let mut block = CollapsibleBlock::thinking("t1", vec![]);
        assert!(!block.is_expanded()); // Defaults to collapsed
        block.toggle();
        assert!(block.is_expanded());
        block.toggle();
        assert!(!block.is_expanded());
    }

    #[test]
    fn test_collapsible_block_visible_lines() {
        let mut block = CollapsibleBlock::thinking(
            "t1",
            vec![
                "Line 1".to_string(),
                "Line 2".to_string(),
                "Line 3".to_string(),
            ],
        );

        // Collapsed by default: header only = 1
        assert!(!block.is_expanded());
        assert_eq!(block.visible_lines(), 1);

        // Expanded: header + 3 content lines + closing border = 5
        block.toggle();
        assert!(block.is_expanded());
        assert_eq!(block.visible_lines(), 5);
    }

    #[test]
    fn test_collapsible_block_state_new() {
        let state = CollapsibleBlockState::new();
        assert!(state.is_empty());
        assert_eq!(state.len(), 0);
    }

    #[test]
    fn test_collapsible_block_state_default_values() {
        let state = CollapsibleBlockState::new();

        // Both thinking and tool blocks default to collapsed
        assert!(!state.is_expanded("unknown-thinking", BlockType::Thinking));
        assert!(!state.is_expanded("unknown-tool", BlockType::Tool));
    }

    #[test]
    fn test_collapsible_block_state_set_and_get() {
        let mut state = CollapsibleBlockState::new();

        state.set("block-1", true);
        assert!(state.is_expanded("block-1", BlockType::Tool));

        state.set("block-1", false);
        assert!(!state.is_expanded("block-1", BlockType::Thinking));
    }

    #[test]
    fn test_collapsible_block_state_toggle() {
        let mut state = CollapsibleBlockState::new();

        // Toggle a thinking block (default collapsed -> expanded)
        state.toggle("think-1", BlockType::Thinking);
        assert!(state.is_expanded("think-1", BlockType::Thinking));

        // Toggle again (expanded -> collapsed)
        state.toggle("think-1", BlockType::Thinking);
        assert!(!state.is_expanded("think-1", BlockType::Thinking));

        // Toggle a tool block (default collapsed -> expanded)
        state.toggle("tool-1", BlockType::Tool);
        assert!(state.is_expanded("tool-1", BlockType::Tool));
    }

    #[test]
    fn test_collapsible_block_state_remove() {
        let mut state = CollapsibleBlockState::new();

        state.set("block-1", true);
        assert!(state.is_expanded("block-1", BlockType::Thinking));

        state.remove("block-1");
        // Should revert to default (Thinking = collapsed)
        assert!(!state.is_expanded("block-1", BlockType::Thinking));
    }

    #[test]
    fn test_collapsible_block_state_clear() {
        let mut state = CollapsibleBlockState::new();

        state.set("block-1", false);
        state.set("block-2", true);
        assert_eq!(state.len(), 2);

        state.clear();
        assert!(state.is_empty());
    }

    // ========================================================================
    // ParsedMessageContent Tests
    // ========================================================================

    #[test]
    fn test_parsed_message_content_empty() {
        let parsed = ParsedMessageContent::parse("", &[], "msg-1");
        assert!(parsed.is_empty());
        assert!(!parsed.has_thinking());
        assert!(!parsed.has_tools());
    }

    #[test]
    fn test_parsed_message_content_plain_text() {
        let parsed = ParsedMessageContent::parse("Hello, world!", &[], "msg-1");
        assert!(!parsed.is_empty());
        assert!(!parsed.has_thinking());
        assert!(!parsed.has_tools());

        let text_segments = parsed.text_segments();
        assert_eq!(text_segments.len(), 1);
        assert_eq!(text_segments[0], "Hello, world!");
    }

    #[test]
    fn test_parsed_message_content_thinking_block() {
        let content = "<thinking>I need to analyze this code.</thinking>Here is my response.";
        let parsed = ParsedMessageContent::parse(content, &[], "msg-1");

        assert!(parsed.has_thinking());
        assert!(!parsed.has_tools());

        let thinking_blocks = parsed.thinking_blocks();
        assert_eq!(thinking_blocks.len(), 1);
        assert_eq!(thinking_blocks[0].block_type(), BlockType::Thinking);
        assert!(thinking_blocks[0]
            .content()
            .contains(&"I need to analyze this code.".to_string()));

        let text_segments = parsed.text_segments();
        assert_eq!(text_segments.len(), 1);
        assert_eq!(text_segments[0], "Here is my response.");
    }

    #[test]
    fn test_parsed_message_content_multiple_thinking_blocks() {
        let content = "<thinking>First thought.</thinking>Some text.<thinking>Second thought.</thinking>More text.";
        let parsed = ParsedMessageContent::parse(content, &[], "msg-1");

        let thinking_blocks = parsed.thinking_blocks();
        assert_eq!(thinking_blocks.len(), 2);
    }

    #[test]
    fn test_parsed_message_content_tool_block() {
        let tool_calls = vec![ToolCallInfo::new(
            "ripgrep",
            serde_json::json!({"pattern": "test"}),
            "Found 5 matches",
        )];

        let parsed = ParsedMessageContent::parse("Let me search for that.", &tool_calls, "msg-1");

        assert!(!parsed.has_thinking());
        assert!(parsed.has_tools());

        let tool_blocks = parsed.tool_blocks();
        assert_eq!(tool_blocks.len(), 1);
        assert_eq!(tool_blocks[0].block_type(), BlockType::Tool);
        assert_eq!(tool_blocks[0].header(), "ripgrep");
    }

    #[test]
    fn test_parsed_message_content_multiple_tool_blocks() {
        let tool_calls = vec![
            ToolCallInfo::new(
                "read_file",
                serde_json::json!({"path": "src/main.rs"}),
                "fn main() { ... }",
            ),
            ToolCallInfo::new(
                "grep",
                serde_json::json!({"pattern": "TODO"}),
                "Found 3 TODOs",
            ),
        ];

        let parsed = ParsedMessageContent::parse("Checking the code.", &tool_calls, "msg-1");

        let tool_blocks = parsed.tool_blocks();
        assert_eq!(tool_blocks.len(), 2);
        assert_eq!(tool_blocks[0].header(), "read_file");
        assert_eq!(tool_blocks[1].header(), "grep");
    }

    #[test]
    fn test_parsed_message_content_thinking_and_tools() {
        let content = "<thinking>Let me search for this.</thinking>I found the following:";
        let tool_calls = vec![ToolCallInfo::new(
            "grep",
            serde_json::json!({"pattern": "function"}),
            "5 matches found",
        )];

        let parsed = ParsedMessageContent::parse(content, &tool_calls, "msg-1");

        assert!(parsed.has_thinking());
        assert!(parsed.has_tools());

        let thinking_blocks = parsed.thinking_blocks();
        let tool_blocks = parsed.tool_blocks();

        assert_eq!(thinking_blocks.len(), 1);
        assert_eq!(tool_blocks.len(), 1);
    }

    #[test]
    fn test_parsed_message_content_case_insensitive_tags() {
        let content = "<THINKING>Uppercase tags.</THINKING>Response.";
        let parsed = ParsedMessageContent::parse(content, &[], "msg-1");

        assert!(parsed.has_thinking());
        let thinking_blocks = parsed.thinking_blocks();
        assert_eq!(thinking_blocks.len(), 1);
    }

    #[test]
    fn test_parsed_message_content_unclosed_thinking() {
        let content = "<thinking>Unclosed thinking block";
        let parsed = ParsedMessageContent::parse(content, &[], "msg-1");

        // Should still extract the thinking content
        assert!(parsed.has_thinking());
        let thinking_blocks = parsed.thinking_blocks();
        assert_eq!(thinking_blocks.len(), 1);
    }

    #[test]
    fn test_tool_call_info_new() {
        let info = ToolCallInfo::new(
            "test_tool",
            serde_json::json!({"key": "value"}),
            "result preview",
        );

        assert_eq!(info.tool, "test_tool");
        assert_eq!(info.args["key"], "value");
        assert_eq!(info.result_preview, "result preview");
    }

    #[test]
    fn test_parsed_message_blocks_method() {
        let content = "<thinking>Thought.</thinking>Text.";
        let tool_calls = vec![ToolCallInfo::new("tool1", serde_json::json!({}), "result")];

        let parsed = ParsedMessageContent::parse(content, &tool_calls, "msg-1");

        let all_blocks = parsed.blocks();
        assert_eq!(all_blocks.len(), 2); // 1 thinking + 1 tool
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating BlockType
    fn block_type_strategy() -> impl Strategy<Value = BlockType> {
        prop_oneof![Just(BlockType::Thinking), Just(BlockType::Tool),]
    }

    // Strategy for generating a CollapsibleBlock
    fn collapsible_block_strategy() -> impl Strategy<Value = CollapsibleBlock> {
        (
            "[a-z0-9-]{1,20}",                                     // id
            block_type_strategy(),                                 // block_type
            "[a-zA-Z0-9_ ]{1,30}",                                 // header
            proptest::collection::vec("[a-zA-Z0-9 ]{0,50}", 0..5), // content
            any::<bool>(),                                         // expanded
        )
            .prop_map(|(id, block_type, header, content, expanded)| {
                let mut block = CollapsibleBlock::new(id, block_type, header, content);
                block.expanded = expanded;
                block
            })
    }

    // Feature: enhanced-tui-layout, Property 8: Block Toggle Round-Trip
    // **Validates: Requirements 7.4, 8.6**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_block_toggle_roundtrip(mut block in collapsible_block_strategy()) {
            let initial_expanded = block.expanded;

            // Toggle twice should return to original state
            block.toggle();
            block.toggle();

            prop_assert_eq!(block.expanded, initial_expanded);
        }
    }

    // Feature: enhanced-tui-layout, Property 7: Collapsible Block Headers
    // **Validates: Requirements 7.2, 7.3, 8.2, 8.3**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_collapsible_block_headers(block in collapsible_block_strategy()) {
            let header = block.display_header();

            // Header should contain the expand/collapse indicator
            if block.expanded {
                prop_assert!(header.starts_with("▼"), "Expanded block header should start with ▼");
            } else {
                prop_assert!(header.starts_with("▶"), "Collapsed block header should start with ▶");
            }

            // Header should contain the appropriate icon
            let icon = block.block_type.icon();
            prop_assert!(header.contains(icon), "Header should contain block type icon");

            // For tool blocks, header should contain "Tool:"
            if block.block_type == BlockType::Tool {
                prop_assert!(header.contains("Tool:"), "Tool block header should contain 'Tool:'");
            }
        }
    }

    // Additional property: visible_lines consistency
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_visible_lines_consistency(mut block in collapsible_block_strategy()) {
            let content_len = block.content.len();

            // When expanded: header + content + closing border
            block.expanded = true;
            prop_assert_eq!(block.visible_lines(), 1 + content_len + 1);

            // When collapsed: header only
            block.expanded = false;
            prop_assert_eq!(block.visible_lines(), 1);
        }
    }

    // Property for CollapsibleBlockState toggle round-trip
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_block_state_toggle_roundtrip(
            id in "[a-z0-9-]{1,20}",
            block_type in block_type_strategy()
        ) {
            let mut state = CollapsibleBlockState::new();

            // Get initial state (default based on block type)
            let initial = state.is_expanded(&id, block_type);

            // Toggle twice should return to original state
            state.toggle(&id, block_type);
            state.toggle(&id, block_type);

            prop_assert_eq!(state.is_expanded(&id, block_type), initial);
        }
    }

    // Strategy for generating thinking content (text without < or > to avoid tag conflicts)
    // Must contain at least one non-whitespace character
    fn thinking_content_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9.,!?][a-zA-Z0-9 .,!?]{0,99}"
    }

    // Strategy for generating message ID
    fn message_id_strategy() -> impl Strategy<Value = String> {
        "[a-z0-9-]{5,20}"
    }

    // Strategy for generating text content (without thinking tags)
    fn text_content_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?]{0,100}"
    }

    // Feature: enhanced-tui-layout, Property 10: Thinking Block Parsing
    // **Validates: Requirements 7.1**
    //
    // For any message content containing thinking markers, parsing should produce
    // a CollapsibleBlock with BlockType::Thinking and the thinking content extracted.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_thinking_block_parsing(
            thinking_content in thinking_content_strategy(),
            before_text in text_content_strategy(),
            after_text in text_content_strategy(),
            message_id in message_id_strategy()
        ) {
            // Construct content with thinking block
            let content = format!(
                "{}<thinking>{}</thinking>{}",
                before_text, thinking_content, after_text
            );

            // Parse the content
            let parsed = ParsedMessageContent::parse(&content, &[], &message_id);

            // Should have thinking blocks
            prop_assert!(parsed.has_thinking(),
                "Parsed content should have thinking blocks");

            // Get thinking blocks
            let thinking_blocks = parsed.thinking_blocks();
            prop_assert!(!thinking_blocks.is_empty(),
                "Should have at least one thinking block");

            // First thinking block should be of type Thinking
            prop_assert_eq!(thinking_blocks[0].block_type(), BlockType::Thinking,
                "Block should be of type Thinking");

            // Thinking block should contain the thinking content
            let block_content = thinking_blocks[0].content().join("\n");
            prop_assert!(block_content.contains(thinking_content.trim()),
                "Thinking block should contain the thinking content. Expected '{}' in '{}'",
                thinking_content.trim(), block_content);

            // Thinking block should default to collapsed
            prop_assert!(!thinking_blocks[0].is_expanded(),
                "Thinking block should default to collapsed");

            // Block ID should contain the message ID
            prop_assert!(thinking_blocks[0].id().contains(&message_id),
                "Block ID should contain message ID");
        }
    }

    // Strategy for generating tool name
    fn tool_name_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("grep".to_string()),
            Just("ripgrep".to_string()),
            Just("read_file".to_string()),
            Just("write_file".to_string()),
            Just("shell".to_string()),
            Just("list_directory".to_string()),
            "[a-z_]{3,15}".prop_map(|s| s),
        ]
    }

    // Strategy for generating tool result preview
    fn result_preview_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 .,!?\n]{0,200}"
    }

    // Strategy for generating tool call info
    fn tool_call_info_strategy() -> impl Strategy<Value = ToolCallInfo> {
        (tool_name_strategy(), result_preview_strategy()).prop_map(|(tool, result_preview)| {
            ToolCallInfo::new(
                tool.clone(),
                serde_json::json!({"pattern": "test", "path": "src/main.rs"}),
                result_preview,
            )
        })
    }

    // Feature: enhanced-tui-layout, Property 11: Tool Block Parsing
    // **Validates: Requirements 8.1, 8.4, 8.5**
    //
    // For any tool call with name and arguments, parsing should produce a
    // CollapsibleBlock with BlockType::Tool containing the tool name in the
    // header and command/result in the content.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_tool_block_parsing(
            tool_call in tool_call_info_strategy(),
            text_content in text_content_strategy(),
            message_id in message_id_strategy()
        ) {
            // Parse content with tool call
            let parsed = ParsedMessageContent::parse(&text_content, std::slice::from_ref(&tool_call), &message_id);

            // Should have tool blocks
            prop_assert!(parsed.has_tools(),
                "Parsed content should have tool blocks");

            // Get tool blocks
            let tool_blocks = parsed.tool_blocks();
            prop_assert_eq!(tool_blocks.len(), 1,
                "Should have exactly one tool block");

            // Tool block should be of type Tool
            prop_assert_eq!(tool_blocks[0].block_type(), BlockType::Tool,
                "Block should be of type Tool");

            // Tool block header should contain the tool name
            prop_assert_eq!(tool_blocks[0].header(), tool_call.tool,
                "Tool block header should be the tool name");

            // Tool block should default to collapsed
            prop_assert!(!tool_blocks[0].is_expanded(),
                "Tool block should default to collapsed");

            // Block ID should contain the message ID
            prop_assert!(tool_blocks[0].id().contains(&message_id),
                "Block ID should contain message ID");

            // Display header should contain "Tool:" prefix
            let display_header = tool_blocks[0].display_header();
            prop_assert!(display_header.contains("Tool:"),
                "Display header should contain 'Tool:' prefix");

            // Display header should contain the tool icon
            prop_assert!(display_header.contains("🔧"),
                "Display header should contain tool icon");
        }
    }

    // Property: Multiple tool calls produce multiple tool blocks
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_multiple_tool_blocks(
            tool_calls in proptest::collection::vec(tool_call_info_strategy(), 1..5),
            text_content in text_content_strategy(),
            message_id in message_id_strategy()
        ) {
            let parsed = ParsedMessageContent::parse(&text_content, &tool_calls, &message_id);

            let tool_blocks = parsed.tool_blocks();

            // Should have same number of tool blocks as tool calls
            prop_assert_eq!(tool_blocks.len(), tool_calls.len(),
                "Number of tool blocks should match number of tool calls");

            // Each tool block should have the correct tool name
            for (i, (block, call)) in tool_blocks.iter().zip(tool_calls.iter()).enumerate() {
                prop_assert_eq!(block.header(), &call.tool,
                    "Tool block {} header should match tool call name", i);
            }
        }
    }
}
