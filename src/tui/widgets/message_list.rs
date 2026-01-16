//! Message list widget for displaying chat history
//!
//! Provides a scrollable list of chat messages with role indicators
//! and markdown-like formatting support, including collapsible blocks
//! for thinking and tool execution content.

#![allow(dead_code)]

use chrono::{DateTime, Utc};

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    prelude::StatefulWidget,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
};
use uuid::Uuid;

use super::collapsible::{
    BlockType, CollapsibleBlock, CollapsibleBlockState, ContentSegment, ParsedMessageContent,
    ToolCallInfo,
};

// ============================================================================
// Message Segment (for interleaved tool/text rendering)
// ============================================================================

/// A segment of message content for interleaved rendering.
///
/// During streaming, segments are built in chronological order as TextChunk
/// and ToolCallStarted events arrive. This preserves the natural flow of
/// text interspersed with tool executions.
///
/// Tool segments use indices into `tool_call_info` to avoid data duplication
/// and ensure a single source of truth for tool state.
#[derive(Debug, Clone)]
pub enum MessageSegment {
    /// Text content from the assistant
    Text(String),
    /// Reference to a tool in tool_call_info by index
    ToolRef(usize),
}

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Role {
    /// User message
    #[default]
    User,
    /// Assistant/AI message
    Assistant,
    /// System message
    System,
    /// Tool call/result
    Tool,
}

impl Role {
    /// Get the display icon for this role
    pub fn icon(&self) -> &'static str {
        match self {
            Role::User => "👤",
            Role::Assistant => "🤖",
            Role::System => "⚙️",
            Role::Tool => "🔧",
        }
    }

    /// Get the display color for this role
    pub fn color(&self) -> Color {
        match self {
            Role::User => Color::Cyan,
            Role::Assistant => Color::Green,
            Role::System => Color::Yellow,
            Role::Tool => Color::Magenta,
        }
    }

    /// Get the display name for this role
    ///
    /// Note: For user messages, the actual username should be used instead
    /// of this default. This method returns fallback values.
    /// Requirements 2.2, 2.3, 2.5
    pub fn name(&self) -> &'static str {
        match self {
            Role::User => "You", // Fallback when username detection fails (Requirement 2.5)
            Role::Assistant => "Tark", // Always "Tark" for assistant (Requirement 2.3)
            Role::System => "System",
            Role::Tool => "Tool",
        }
    }
}

/// A chat message with content and metadata
#[derive(Debug, Clone)]
pub struct ChatMessage {
    /// Unique identifier
    pub id: Uuid,
    /// Role of the sender
    pub role: Role,
    /// Message content (kept for persistence/backward compatibility)
    pub content: String,
    /// Thinking content (for assistant messages when thinking mode is enabled)
    /// Requirements: 9.1, 9.3
    pub thinking_content: String,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Whether this message is currently streaming
    pub is_streaming: bool,
    /// Tool calls associated with this message (for assistant messages)
    pub tool_calls: Vec<String>,
    /// Detailed tool call info for collapsible block rendering (backward compat)
    pub tool_call_info: Vec<ToolCallInfo>,
    /// Interleaved segments built during streaming.
    /// This preserves the chronological order of text and tool executions.
    pub segments: Vec<MessageSegment>,
}

impl ChatMessage {
    /// Create a new chat message
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            thinking_content: String::new(),
            timestamp: Utc::now(),
            is_streaming: false,
            tool_calls: Vec::new(),
            tool_call_info: Vec::new(),
            segments: Vec::new(),
        }
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(Role::User, content)
    }

    /// Create an assistant message
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(Role::Assistant, content)
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(Role::System, content)
    }

    /// Create a tool message
    pub fn tool(content: impl Into<String>) -> Self {
        Self::new(Role::Tool, content)
    }

    /// Set streaming state
    pub fn with_streaming(mut self, streaming: bool) -> Self {
        self.is_streaming = streaming;
        self
    }

    /// Add a tool call
    pub fn with_tool_call(mut self, tool_call: impl Into<String>) -> Self {
        self.tool_calls.push(tool_call.into());
        self
    }

    /// Add detailed tool call info for collapsible block rendering
    pub fn with_tool_call_info(mut self, info: ToolCallInfo) -> Self {
        self.tool_call_info.push(info);
        self
    }

    /// Add multiple tool call infos
    pub fn with_tool_call_infos(mut self, infos: Vec<ToolCallInfo>) -> Self {
        self.tool_call_info.extend(infos);
        self
    }

    /// Calculate the number of lines this message will take
    /// This must match the rendering logic in render_message_with_blocks
    pub fn line_count(
        &self,
        width: u16,
        block_state: &CollapsibleBlockState,
        show_thinking: bool,
    ) -> usize {
        if width == 0 {
            return 1;
        }

        // Use the actual rendering function to get accurate line count
        // This ensures line_count always matches what render_message_with_blocks produces
        let dummy_username = "User";
        let result = render_message_with_blocks(
            self,
            false, // is_selected doesn't affect line count
            width,
            block_state,
            dummy_username,
            show_thinking,
        );

        result.lines.len()
    }
}

/// Position in the text content (line and column)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TextPosition {
    /// Line index (0-based)
    pub line: usize,
    /// Column index (0-based, character position)
    pub col: usize,
}

impl TextPosition {
    /// Create a new text position
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    /// Check if this position is before another
    pub fn is_before(&self, other: &Self) -> bool {
        self.line < other.line || (self.line == other.line && self.col < other.col)
    }
}

/// Text selection range
#[derive(Debug, Clone, Default)]
pub struct TextSelection {
    /// Selection anchor (where selection started)
    pub anchor: TextPosition,
    /// Selection cursor (where selection ends)
    pub cursor: TextPosition,
    /// Whether selection is active
    pub active: bool,
}

impl TextSelection {
    /// Create a new inactive selection
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a selection at the given position
    pub fn start(&mut self, pos: TextPosition) {
        self.anchor = pos;
        self.cursor = pos;
        self.active = true;
    }

    /// Extend selection to the given position
    pub fn extend_to(&mut self, pos: TextPosition) {
        if self.active {
            self.cursor = pos;
        }
    }

    /// Clear the selection
    pub fn clear(&mut self) {
        self.active = false;
    }

    /// Get the start position (min of anchor and cursor)
    pub fn start_pos(&self) -> TextPosition {
        if self.anchor.is_before(&self.cursor) {
            self.anchor
        } else {
            self.cursor
        }
    }

    /// Get the end position (max of anchor and cursor)
    pub fn end_pos(&self) -> TextPosition {
        if self.anchor.is_before(&self.cursor) {
            self.cursor
        } else {
            self.anchor
        }
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: TextPosition) -> bool {
        if !self.active {
            return false;
        }
        let start = self.start_pos();
        let end = self.end_pos();

        if pos.line < start.line || pos.line > end.line {
            return false;
        }

        if pos.line == start.line && pos.line == end.line {
            // Single line selection
            return pos.col >= start.col && pos.col < end.col;
        }

        if pos.line == start.line {
            return pos.col >= start.col;
        }

        if pos.line == end.line {
            return pos.col < end.col;
        }

        // Middle lines are fully selected
        true
    }
}

/// Block click target info
/// Target for block click handling
#[derive(Debug, Clone)]
pub struct BlockClickTarget {
    /// Block ID for toggling
    pub block_id: String,
    /// Block type
    pub block_type: BlockType,
}

/// Message list widget with scroll state
#[derive(Debug)]
pub struct MessageList {
    /// Messages in the list
    messages: Vec<ChatMessage>,
    /// Current scroll offset (line-based)
    scroll_offset: usize,
    /// Currently selected message index
    selected: Option<usize>,
    /// Total visible height (set during render)
    visible_height: u16,
    /// State for collapsible blocks (expand/collapse tracking)
    block_state: CollapsibleBlockState,
    /// Cursor position for text navigation
    cursor_pos: TextPosition,
    /// Text selection state
    selection: TextSelection,
    /// Last known width for line calculations
    last_width: u16,
    /// Cached total lines (invalidated on content change)
    cached_total_lines: Option<usize>,
    /// Whether to display thinking blocks (controlled by /thinking command)
    show_thinking: bool,
    /// Map of line numbers to clickable block targets (updated during render)
    block_click_targets: std::collections::HashMap<usize, BlockClickTarget>,
}

impl Default for MessageList {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            scroll_offset: 0,
            selected: None,
            visible_height: 0,
            block_state: CollapsibleBlockState::new(),
            cursor_pos: TextPosition::default(),
            selection: TextSelection::new(),
            last_width: 0,
            cached_total_lines: None,
            show_thinking: true, // Show thinking blocks by default
            block_click_targets: std::collections::HashMap::new(),
        }
    }
}

impl MessageList {
    /// Create a new empty message list
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a message list with initial messages
    pub fn with_messages(messages: Vec<ChatMessage>) -> Self {
        Self {
            messages,
            scroll_offset: 0,
            selected: None,
            visible_height: 0,
            block_state: CollapsibleBlockState::new(),
            cursor_pos: TextPosition::default(),
            selection: TextSelection::new(),
            last_width: 0,
            cached_total_lines: None,
            show_thinking: true, // Show thinking blocks by default
            block_click_targets: std::collections::HashMap::new(),
        }
    }

    /// Get a reference to the collapsible block state
    pub fn block_state(&self) -> &CollapsibleBlockState {
        &self.block_state
    }

    /// Get a mutable reference to the collapsible block state
    pub fn block_state_mut(&mut self) -> &mut CollapsibleBlockState {
        &mut self.block_state
    }

    /// Toggle a collapsible block's expanded state
    pub fn toggle_block(&mut self, block_id: &str, block_type: BlockType) {
        self.block_state.toggle(block_id, block_type);
    }

    /// Clear all block click targets (called before rendering)
    pub fn clear_block_click_targets(&mut self) {
        self.block_click_targets.clear();
    }

    /// Register a block click target at a specific line
    pub fn add_block_click_target(&mut self, line: usize, block_id: String, block_type: BlockType) {
        self.block_click_targets.insert(
            line,
            BlockClickTarget {
                block_id,
                block_type,
            },
        );
    }

    /// Get block click target at a specific line (if any)
    pub fn get_block_at_line(&self, line: usize) -> Option<&BlockClickTarget> {
        self.block_click_targets.get(&line)
    }

    /// Toggle block at a specific line (returns true if a block was toggled)
    pub fn toggle_block_at_line(&mut self, line: usize) -> bool {
        if let Some(target) = self.block_click_targets.get(&line).cloned() {
            self.block_state.toggle(&target.block_id, target.block_type);
            // Also set this block as focused for scrolling
            if self
                .block_state
                .is_expanded(&target.block_id, target.block_type)
            {
                self.block_state.set_focused_block(Some(target.block_id));
            } else {
                // If collapsing, clear focus
                self.block_state.set_focused_block(None);
            }
            true
        } else {
            false
        }
    }

    /// Get the currently focused block ID
    pub fn focused_block(&self) -> Option<&str> {
        self.block_state.focused_block()
    }

    /// Clear block focus
    pub fn clear_block_focus(&mut self) {
        self.block_state.set_focused_block(None);
    }

    /// Focus the next block in the message list (expands it if collapsed)
    /// Collapses all other blocks and scrolls to show the focused block
    /// Returns true if focus changed
    pub fn focus_next_block(&mut self) -> bool {
        // Get all block IDs from click targets
        let mut block_ids: Vec<(usize, String, BlockType)> = self
            .block_click_targets
            .iter()
            .map(|(line, target)| (*line, target.block_id.clone(), target.block_type))
            .collect();
        block_ids.sort_by_key(|(line, _, _)| *line);

        if block_ids.is_empty() {
            return false;
        }

        let current_focus = self.block_state.focused_block().map(|s| s.to_string());

        let (next_idx, next_id) = if let Some(ref current) = current_focus {
            // Find current position and get next
            let current_idx = block_ids
                .iter()
                .position(|(_, id, _)| id == current)
                .unwrap_or(0);
            let next_idx = (current_idx + 1) % block_ids.len();
            (next_idx, block_ids[next_idx].1.clone())
        } else {
            // No current focus, focus first block
            (0, block_ids[0].1.clone())
        };

        // Collapse all other blocks, expand only the focused one
        for (_, id, _) in &block_ids {
            self.block_state.set(id, id == &next_id);
        }

        self.block_state.set_focused_block(Some(next_id));

        // Scroll to show the focused block
        let target_line = block_ids[next_idx].0;
        self.scroll_to_line(target_line);

        true
    }

    /// Focus the previous block in the message list (expands it if collapsed)
    /// Collapses all other blocks and scrolls to show the focused block
    /// Returns true if focus changed
    pub fn focus_prev_block(&mut self) -> bool {
        // Get all block IDs from click targets
        let mut block_ids: Vec<(usize, String, BlockType)> = self
            .block_click_targets
            .iter()
            .map(|(line, target)| (*line, target.block_id.clone(), target.block_type))
            .collect();
        block_ids.sort_by_key(|(line, _, _)| *line);

        if block_ids.is_empty() {
            return false;
        }

        let current_focus = self.block_state.focused_block().map(|s| s.to_string());

        let (prev_idx, prev_id) = if let Some(ref current) = current_focus {
            // Find current position and get previous
            let current_idx = block_ids
                .iter()
                .position(|(_, id, _)| id == current)
                .unwrap_or(0);
            // Wrap around to last if at first
            let prev_idx = if current_idx == 0 {
                block_ids.len() - 1
            } else {
                current_idx - 1
            };
            (prev_idx, block_ids[prev_idx].1.clone())
        } else {
            // No current focus, focus last block
            let last_idx = block_ids.len() - 1;
            (last_idx, block_ids[last_idx].1.clone())
        };

        // Collapse all other blocks, expand only the focused one
        for (_, id, _) in &block_ids {
            self.block_state.set(id, id == &prev_id);
        }

        self.block_state.set_focused_block(Some(prev_id));

        // Scroll to show the focused block
        let target_line = block_ids[prev_idx].0;
        self.scroll_to_line(target_line);

        true
    }

    /// Add a message to the list
    pub fn push(&mut self, message: ChatMessage) {
        self.messages.push(message);
    }

    /// Get the number of messages
    pub fn len(&self) -> usize {
        self.messages.len()
    }

    /// Check if the list is empty
    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// Get a reference to all messages
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Get a mutable reference to all messages
    pub fn messages_mut(&mut self) -> &mut Vec<ChatMessage> {
        &mut self.messages
    }

    /// Clear all messages
    pub fn clear(&mut self) {
        self.messages.clear();
        self.scroll_offset = 0;
        self.selected = None;
    }

    /// Get the current scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get the currently selected message index
    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    /// Set the visible height (called during render)
    pub fn set_visible_height(&mut self, height: u16) {
        self.visible_height = height;
    }

    /// Set whether to display thinking blocks
    pub fn set_show_thinking(&mut self, show: bool) {
        if self.show_thinking != show {
            self.show_thinking = show;
            self.cached_total_lines = None; // Invalidate cache
        }
    }

    /// Get whether thinking blocks are displayed
    pub fn show_thinking(&self) -> bool {
        self.show_thinking
    }

    /// Calculate total content height in lines
    fn total_lines(&self, width: u16) -> usize {
        self.messages
            .iter()
            .map(|m| m.line_count(width, &self.block_state, self.show_thinking))
            .sum()
    }

    /// Get the maximum scroll offset
    fn max_scroll(&self, width: u16) -> usize {
        let total = self.total_lines(width);
        let visible = self.visible_height as usize;
        total.saturating_sub(visible)
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self, width: u16) {
        let max = self.max_scroll(width);
        if self.scroll_offset < max {
            self.scroll_offset += 1;
        }
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down by half a page
    pub fn scroll_half_page_down(&mut self, width: u16) {
        let half_page = (self.visible_height / 2) as usize;
        let max = self.max_scroll(width);
        self.scroll_offset = (self.scroll_offset + half_page).min(max);
    }

    /// Scroll up by half a page
    pub fn scroll_half_page_up(&mut self) {
        let half_page = (self.visible_height / 2) as usize;
        self.scroll_offset = self.scroll_offset.saturating_sub(half_page);
    }

    /// Scroll to the top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to the bottom
    pub fn scroll_to_bottom(&mut self, width: u16) {
        self.scroll_offset = self.max_scroll(width);
    }

    /// Scroll to ensure a specific line is visible
    /// Centers the line in the viewport if possible
    fn scroll_to_line(&mut self, line: usize) {
        let half_height = (self.visible_height / 2) as usize;
        // Try to center the line in the viewport
        self.scroll_offset = line.saturating_sub(half_height);
    }

    /// Select the next message
    pub fn select_next(&mut self) {
        if self.messages.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => (i + 1).min(self.messages.len() - 1),
            None => 0,
        });
    }

    /// Select the previous message
    pub fn select_previous(&mut self) {
        if self.messages.is_empty() {
            self.selected = None;
            return;
        }
        self.selected = Some(match self.selected {
            Some(i) => i.saturating_sub(1),
            None => self.messages.len().saturating_sub(1),
        });
    }

    /// Select the first message
    pub fn select_first(&mut self) {
        if !self.messages.is_empty() {
            self.selected = Some(0);
        }
    }

    /// Select the last message
    pub fn select_last(&mut self) {
        if !self.messages.is_empty() {
            self.selected = Some(self.messages.len() - 1);
        }
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    // ==================== Text Cursor and Selection Methods ====================

    /// Get the current cursor position
    pub fn cursor_pos(&self) -> TextPosition {
        self.cursor_pos
    }

    /// Get the text selection state
    pub fn selection(&self) -> &TextSelection {
        &self.selection
    }

    /// Check if there's an active selection
    pub fn has_selection(&self) -> bool {
        self.selection.active
    }

    /// Clear text selection
    pub fn clear_text_selection(&mut self) {
        self.selection.clear();
    }

    /// Start a new selection at the current cursor position
    pub fn start_selection(&mut self) {
        self.selection.start(self.cursor_pos);
    }

    /// Set the cursor position directly (for mouse selection)
    pub fn set_cursor_position(&mut self, line: usize, col: usize) {
        self.cursor_pos.line = line;
        self.cursor_pos.col = col;
    }

    /// Extend the current selection to the cursor position
    pub fn extend_selection(&mut self) {
        if self.selection.active {
            self.selection.extend_to(self.cursor_pos);
        }
    }

    /// Get the total number of lines
    pub fn get_total_lines(&self, width: u16) -> usize {
        self.total_lines(width)
    }

    /// Get the line length at a given line index
    fn get_line_length(&self, _line_idx: usize, width: u16) -> usize {
        // This is a simplified version - in practice we'd need to
        // track the actual rendered line content
        // For now, return width as max
        width as usize
    }

    /// Move cursor up by one line
    pub fn cursor_up(&mut self, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        if self.cursor_pos.line > 0 {
            self.cursor_pos.line -= 1;
            // Ensure cursor is visible
            if self.cursor_pos.line < self.scroll_offset {
                self.scroll_offset = self.cursor_pos.line;
            }
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor down by one line
    pub fn cursor_down(&mut self, width: u16, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let total = self.total_lines(width);
        if self.cursor_pos.line + 1 < total {
            self.cursor_pos.line += 1;
            // Ensure cursor is visible
            let visible_end = self.scroll_offset + self.visible_height as usize;
            if self.cursor_pos.line >= visible_end {
                self.scroll_offset = self
                    .cursor_pos
                    .line
                    .saturating_sub(self.visible_height as usize - 1);
            }
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor left by one character
    pub fn cursor_left(&mut self, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        if self.cursor_pos.col > 0 {
            self.cursor_pos.col -= 1;
        } else if self.cursor_pos.line > 0 {
            // Move to end of previous line
            self.cursor_pos.line -= 1;
            self.cursor_pos.col = self.get_line_length(self.cursor_pos.line, self.last_width);
            // Ensure cursor is visible
            if self.cursor_pos.line < self.scroll_offset {
                self.scroll_offset = self.cursor_pos.line;
            }
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor right by one character
    pub fn cursor_right(&mut self, width: u16, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let line_len = self.get_line_length(self.cursor_pos.line, width);
        let total = self.total_lines(width);

        if self.cursor_pos.col < line_len {
            self.cursor_pos.col += 1;
        } else if self.cursor_pos.line + 1 < total {
            // Move to start of next line
            self.cursor_pos.line += 1;
            self.cursor_pos.col = 0;
            // Ensure cursor is visible
            let visible_end = self.scroll_offset + self.visible_height as usize;
            if self.cursor_pos.line >= visible_end {
                self.scroll_offset = self
                    .cursor_pos
                    .line
                    .saturating_sub(self.visible_height as usize - 1);
            }
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor to the start of the current line
    pub fn cursor_line_start(&mut self, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        self.cursor_pos.col = 0;

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor to the end of the current line
    pub fn cursor_line_end(&mut self, width: u16, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        self.cursor_pos.col = self.get_line_length(self.cursor_pos.line, width);

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor to the top of the document
    pub fn cursor_to_top(&mut self, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        self.cursor_pos = TextPosition::new(0, 0);
        self.scroll_offset = 0;

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor to the bottom of the document
    pub fn cursor_to_bottom(&mut self, width: u16, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let total = self.total_lines(width);
        if total > 0 {
            self.cursor_pos.line = total - 1;
            self.cursor_pos.col = 0;
            self.scroll_offset = self.max_scroll(width);
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor by word (forward)
    pub fn cursor_word_forward(&mut self, width: u16, with_selection: bool) {
        // Simplified: move to end of line or next line start
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let line_len = self.get_line_length(self.cursor_pos.line, width);
        if self.cursor_pos.col < line_len {
            // Move to end of line (simplified word movement)
            self.cursor_pos.col = line_len;
        } else {
            // Move to next line
            self.cursor_down(width, false);
            self.cursor_pos.col = 0;
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor by word (backward)
    pub fn cursor_word_backward(&mut self, with_selection: bool) {
        // Simplified: move to start of line or previous line end
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        if self.cursor_pos.col > 0 {
            // Move to start of line (simplified word movement)
            self.cursor_pos.col = 0;
        } else if self.cursor_pos.line > 0 {
            // Move to previous line end
            self.cursor_up(false);
            self.cursor_pos.col = self.get_line_length(self.cursor_pos.line, self.last_width);
        }

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor half page up
    pub fn cursor_half_page_up(&mut self, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let half_page = (self.visible_height / 2) as usize;
        self.cursor_pos.line = self.cursor_pos.line.saturating_sub(half_page);
        self.scroll_half_page_up();

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Move cursor half page down
    pub fn cursor_half_page_down(&mut self, width: u16, with_selection: bool) {
        if with_selection && !self.selection.active {
            self.selection.start(self.cursor_pos);
        }

        let half_page = (self.visible_height / 2) as usize;
        let total = self.total_lines(width);
        self.cursor_pos.line = (self.cursor_pos.line + half_page).min(total.saturating_sub(1));
        self.scroll_half_page_down(width);

        if with_selection {
            self.selection.extend_to(self.cursor_pos);
        } else {
            self.selection.clear();
        }
    }

    /// Select all text
    pub fn select_all(&mut self, width: u16) {
        let total = self.total_lines(width);
        if total > 0 {
            self.selection.anchor = TextPosition::new(0, 0);
            self.selection.cursor =
                TextPosition::new(total - 1, self.get_line_length(total - 1, width));
            self.selection.active = true;
        }
    }

    /// Handle mouse click at position (for selection start)
    pub fn handle_mouse_click(&mut self, line: usize, col: usize) {
        self.cursor_pos = TextPosition::new(line + self.scroll_offset, col);
        self.selection.clear();
    }

    /// Handle mouse drag (for selection extension)
    pub fn handle_mouse_drag(&mut self, line: usize, col: usize) {
        if !self.selection.active {
            self.selection.start(self.cursor_pos);
        }
        self.cursor_pos = TextPosition::new(line + self.scroll_offset, col);
        self.selection.extend_to(self.cursor_pos);
    }

    /// Update last width (called during render)
    pub fn set_last_width(&mut self, width: u16) {
        self.last_width = width;
    }

    /// Get selected text content (returns empty string if no selection)
    ///
    /// This builds the rendered text as it appears on screen and extracts
    /// the selected portion based on line/column positions.
    pub fn get_selected_text(&self, width: u16) -> String {
        if !self.selection.active {
            return String::new();
        }

        // Build rendered lines as plain text strings (matching what's displayed)
        let rendered_lines = self.build_rendered_text_lines(width);

        let start = self.selection.start_pos();
        let end = self.selection.end_pos();

        // Handle case where selection is beyond available lines
        if start.line >= rendered_lines.len() {
            return String::new();
        }

        let mut result = String::new();
        let end_line = end.line.min(rendered_lines.len().saturating_sub(1));

        for (i, line) in rendered_lines.iter().enumerate() {
            if i < start.line || i > end_line {
                continue;
            }

            let line_chars: Vec<char> = line.chars().collect();
            let start_col = if i == start.line { start.col } else { 0 };
            let end_col = if i == end_line {
                (end.col + 1).min(line_chars.len()) // +1 because selection end is inclusive
            } else {
                line_chars.len()
            };

            if start_col < line_chars.len() {
                let selected: String = line_chars[start_col..end_col.min(line_chars.len())]
                    .iter()
                    .collect();
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(&selected);
            } else if i > start.line && i < end_line {
                // Include empty lines within selection
                result.push('\n');
            }
        }

        result
    }

    /// Build plain text representation of rendered lines
    ///
    /// This creates a vector of strings representing what's actually displayed
    /// on screen, so selection positions can be mapped correctly.
    fn build_rendered_text_lines(&self, width: u16) -> Vec<String> {
        let mut all_lines = Vec::new();

        // Get username for headers (use default if not available)
        let username = "You";

        for message in &self.messages {
            // Add header line
            let display_name = match message.role {
                Role::User => username.to_string(),
                Role::Assistant => "Tark".to_string(),
                _ => message.role.name().to_string(),
            };
            let timestamp = message.timestamp.format("%H:%M").to_string();
            let header = format!("{} {} [{}]", message.role.icon(), display_name, timestamp);
            all_lines.push(header);

            // Add tool calls (if any)
            for tool_call in &message.tool_calls {
                all_lines.push(format!("  [tool: {}]", tool_call));
            }

            // Add content lines (with wrapping to match rendered width)
            let content_width = width.saturating_sub(4) as usize; // Account for borders/padding
            for line in message.content.lines() {
                if line.is_empty() {
                    all_lines.push(String::new());
                } else {
                    // Wrap long lines
                    let wrapped = wrap_text_simple(line, content_width);
                    all_lines.extend(wrapped);
                }
            }

            // Add empty spacing line
            all_lines.push(String::new());
        }

        all_lines
    }
}

/// Format message content with markdown-like styling
fn format_content<'a>(content: &'a str, base_style: Style) -> Vec<Line<'a>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<&str> = Vec::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if in_code_block {
                // End code block - render accumulated lines
                for code_line in &code_block_lines {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", code_line),
                        Style::default().fg(Color::Gray).bg(Color::DarkGray),
                    )));
                }
                code_block_lines.clear();
                in_code_block = false;
            } else {
                // Start code block
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_block_lines.push(line);
            continue;
        }

        // Process inline formatting
        let formatted_line = format_inline(line, base_style);
        lines.push(formatted_line);
    }

    // Handle unclosed code block
    if in_code_block {
        for code_line in &code_block_lines {
            lines.push(Line::from(Span::styled(
                format!("  {}", code_line),
                Style::default().fg(Color::Gray).bg(Color::DarkGray),
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

/// Format inline markdown (bold, code)
fn format_inline(line: &str, base_style: Style) -> Line<'_> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_bold = false;
    let mut in_code = false;

    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') && !in_code => {
                // Bold marker
                chars.next(); // consume second *
                if !current.is_empty() {
                    let style = if in_bold {
                        base_style.add_modifier(Modifier::BOLD)
                    } else {
                        base_style
                    };
                    spans.push(Span::styled(std::mem::take(&mut current), style));
                }
                in_bold = !in_bold;
            }
            '`' if !in_bold => {
                // Code marker
                if !current.is_empty() {
                    let style = if in_code {
                        Style::default().fg(Color::Yellow).bg(Color::DarkGray)
                    } else {
                        base_style
                    };
                    spans.push(Span::styled(std::mem::take(&mut current), style));
                }
                in_code = !in_code;
            }
            _ => {
                current.push(c);
            }
        }
    }

    // Add remaining text
    if !current.is_empty() {
        let style = if in_bold {
            base_style.add_modifier(Modifier::BOLD)
        } else if in_code {
            Style::default().fg(Color::Yellow).bg(Color::DarkGray)
        } else {
            base_style
        };
        spans.push(Span::styled(current, style));
    }

    Line::from(spans)
}

/// Render a single message to lines
///
/// Note: This function is kept for backward compatibility but
/// `render_message_with_blocks` is preferred for full functionality.
fn render_message(
    message: &ChatMessage,
    is_selected: bool,
    width: u16,
    username: &str,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Header line with role icon and name
    let header_style = if is_selected {
        Style::default()
            .fg(message.role.color())
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(message.role.color())
            .add_modifier(Modifier::BOLD)
    };

    // Use detected username for user messages, "Tark" for assistant (Requirements 2.2, 2.3, 2.4)
    let display_name = match message.role {
        Role::User => username.to_string(),
        Role::Assistant => "Tark".to_string(),
        _ => message.role.name().to_string(),
    };

    let timestamp = message.timestamp.format("%H:%M").to_string();
    let header = format!("{} {} [{}]", message.role.icon(), display_name, timestamp);
    lines.push(Line::from(Span::styled(header, header_style)));

    // Tool calls (if any) - show before content/output
    for tool_call in &message.tool_calls {
        let tool_style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(Span::styled(
            format!("  [tool: {}]", tool_call.clone()),
            tool_style,
        )));
    }

    // Content lines
    let content_style = if is_selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };

    // Clone content to make it 'static
    let content = message.content.clone();
    let content_lines = format_content_owned_with_width(content, content_style, width as usize);
    lines.extend(content_lines);

    // Streaming indicator
    if message.is_streaming {
        lines.push(Line::from(Span::styled(
            "  ▌",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::SLOW_BLINK),
        )));
    }

    // Add spacing
    lines.push(Line::from(""));

    lines
}

/// Render a collapsible block to lines
///
/// Renders the block header with expand/collapse indicator and icon,
/// and optionally the content if expanded.
///
/// # Requirements
/// - 7.2, 7.3: Thinking block headers with ▼/▶ indicators
/// - 8.2, 8.3: Tool block headers with ▼/▶ indicators
/// - 8.4, 8.5: Tool block content with command/result
fn render_collapsible_block(
    block: &CollapsibleBlock,
    is_expanded: bool,
    width: u16,
    scroll_offset: usize,
    is_focused: bool,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Calculate usable width (account for border "│ " = 2 chars)
    let content_width = width.saturating_sub(4) as usize; // 2 for border, 2 for padding

    // Determine colors based on block type and error state (Requirements 7.2)
    let (header_color, border_color, content_color) = if block.has_error {
        // Error blocks use red styling
        (Color::Red, Color::Red, Color::Red)
    } else {
        match block.block_type() {
            BlockType::Thinking => (Color::Cyan, Color::DarkGray, Color::Rgb(180, 180, 200)),
            BlockType::Tool => (Color::Magenta, Color::DarkGray, Color::Gray),
        }
    };

    // Header style
    let header_style = Style::default()
        .fg(header_color)
        .add_modifier(Modifier::BOLD);

    // Build header with indicator and error marker if applicable
    let indicator = if is_expanded { "▼" } else { "▶" };
    let icon = if block.has_error {
        "✗"
    } else {
        block.block_type().icon()
    };
    let focus_prefix = if is_focused { "»" } else { " " };
    // Use box corner only when expanded, plain style when collapsed (matches tool inline)
    let box_prefix = if is_expanded { "╭── " } else { "" };
    let header_text = match block.block_type() {
        BlockType::Thinking => format!(
            "{}{}{} {} {} ",
            focus_prefix,
            box_prefix,
            indicator,
            icon,
            block.header()
        ),
        BlockType::Tool => {
            if block.has_error {
                // Failed tools: warning icon + FAILED label
                format!(
                    "{}{}{} ⚠️ {} FAILED ",
                    focus_prefix,
                    box_prefix,
                    indicator,
                    block.header()
                )
            } else {
                // Normal tools: gear icon + human-readable action
                format!(
                    "{}{}{} {} {} ",
                    focus_prefix,
                    box_prefix,
                    indicator,
                    icon,
                    block.header()
                )
            }
        }
    };

    // Apply focus styling if focused (cyan and bold)
    let effective_header_style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        header_style
    };

    // Pad header to create a box effect
    lines.push(Line::from(Span::styled(
        header_text,
        effective_header_style,
    )));

    // Render content if expanded
    if is_expanded {
        let border_style = Style::default().fg(border_color);
        let scroll_style = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM);
        // Padding for alignment when focused (» adds one char to header)
        let align_pad = if is_focused { " " } else { "" };

        // Max visible lines before scrolling is enabled
        let max_visible_lines: usize = match block.block_type() {
            BlockType::Thinking => 5, // Compact for thinking
            BlockType::Tool => 12,    // More lines for tool output, then scroll
        };

        // First, collect all wrapped content lines
        let mut all_content_lines: Vec<(String, Style)> = Vec::new();
        for content_line in block.content() {
            // Use red for error content lines that start with ✗
            let base_style = if block.has_error && content_line.starts_with('✗') {
                Style::default().fg(Color::Red)
            } else {
                Style::default()
                    .fg(content_color)
                    .add_modifier(Modifier::ITALIC)
            };

            // Wrap long lines to fit within available width
            let wrapped_lines = wrap_text_simple(content_line, content_width);
            for wrapped in wrapped_lines {
                all_content_lines.push((wrapped, base_style));
            }
        }

        let total_lines = all_content_lines.len();
        let is_thinking = block.block_type() == BlockType::Thinking;

        // For blocks with more content than max_visible_lines, show scrollable view
        if total_lines > max_visible_lines {
            // Calculate visible window based on scroll offset
            let max_scroll = total_lines.saturating_sub(max_visible_lines);
            let effective_scroll = scroll_offset.min(max_scroll);

            // Different scroll behavior based on block type:
            // - Thinking: scroll_offset=0 shows END (most recent), increases show earlier
            // - Tool: scroll_offset=0 shows START (beginning), increases show later
            let (start_idx, end_idx) = if is_thinking {
                let start = max_scroll.saturating_sub(effective_scroll);
                let end = (start + max_visible_lines).min(total_lines);
                (start, end)
            } else {
                // Tool blocks: start from beginning, scroll down to see more
                let start = effective_scroll;
                let end = (start + max_visible_lines).min(total_lines);
                (start, end)
            };

            let lines_above = start_idx;
            let lines_below = total_lines.saturating_sub(end_idx);

            // Show "more above" indicator
            if lines_above > 0 {
                lines.push(Line::from(vec![
                    Span::styled(format!("{}│ ", align_pad), border_style),
                    Span::styled(format!("  ↑ {} more above ([)", lines_above), scroll_style),
                ]));
            }

            // Render visible content (from start_idx to end_idx)
            for (wrapped, base_style) in all_content_lines
                .iter()
                .skip(start_idx)
                .take(end_idx - start_idx)
            {
                let styled_spans = if is_thinking {
                    render_thinking_markdown(wrapped, *base_style)
                } else {
                    vec![Span::styled(wrapped.clone(), *base_style)]
                };

                let mut line_spans = vec![Span::styled(format!("{}│ ", align_pad), border_style)];
                line_spans.extend(styled_spans);
                lines.push(Line::from(line_spans));
            }

            // Show "more below" indicator
            if lines_below > 0 {
                lines.push(Line::from(vec![
                    Span::styled(format!("{}│ ", align_pad), border_style),
                    Span::styled(format!("  ↓ {} more below (])", lines_below), scroll_style),
                ]));
            }
        } else {
            // Show all content (either not thinking block or fits within limit)
            for (wrapped, base_style) in &all_content_lines {
                let styled_spans = if is_thinking {
                    render_thinking_markdown(wrapped, *base_style)
                } else {
                    vec![Span::styled(wrapped.clone(), *base_style)]
                };

                let mut line_spans = vec![Span::styled(format!("{}│ ", align_pad), border_style)];
                line_spans.extend(styled_spans);
                lines.push(Line::from(line_spans));
            }
        }

        // Adaptive closing border (matches content width)
        let border_width = width.saturating_sub(2) as usize;
        let closing_border = format!("{}╰{}", align_pad, "─".repeat(border_width.min(60)));
        lines.push(Line::from(Span::styled(closing_border, border_style)));
    }

    lines
}

/// Simple text wrapping that respects word boundaries
fn wrap_text_simple(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    let mut result = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_len = word.chars().count();

        if current_width == 0 {
            // First word on line
            if word_len > max_width {
                // Word is longer than max width, split it
                for chunk in word.chars().collect::<Vec<_>>().chunks(max_width) {
                    result.push(chunk.iter().collect());
                }
            } else {
                current_line = word.to_string();
                current_width = word_len;
            }
        } else if current_width + 1 + word_len <= max_width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_len;
        } else {
            // Word doesn't fit, start new line
            result.push(std::mem::take(&mut current_line));
            if word_len > max_width {
                // Word is longer than max width, split it
                for chunk in word.chars().collect::<Vec<_>>().chunks(max_width) {
                    result.push(chunk.iter().collect());
                }
                current_width = 0;
            } else {
                current_line = word.to_string();
                current_width = word_len;
            }
        }
    }

    if !current_line.is_empty() {
        result.push(current_line);
    }

    if result.is_empty() {
        result.push(String::new());
    }

    result
}

/// Render text with basic markdown styling for thinking blocks
fn render_thinking_markdown(text: &str, base_style: Style) -> Vec<Span<'static>> {
    let bold_style = base_style.add_modifier(Modifier::BOLD);
    let code_style = Style::default()
        .fg(Color::Yellow)
        .bg(Color::Rgb(40, 40, 50));

    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    let mut in_bold = false;
    let mut in_code = false;

    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                // Toggle bold
                chars.next();
                if !current.is_empty() {
                    let style = if in_code {
                        code_style
                    } else if in_bold {
                        bold_style
                    } else {
                        base_style
                    };
                    spans.push(Span::styled(std::mem::take(&mut current), style));
                }
                in_bold = !in_bold;
            }
            '`' => {
                // Toggle inline code
                if !current.is_empty() {
                    let style = if in_code {
                        code_style
                    } else if in_bold {
                        bold_style
                    } else {
                        base_style
                    };
                    spans.push(Span::styled(std::mem::take(&mut current), style));
                }
                in_code = !in_code;
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        let style = if in_code {
            code_style
        } else if in_bold {
            bold_style
        } else {
            base_style
        };
        spans.push(Span::styled(current, style));
    }

    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base_style));
    }

    spans
}

/// Result of rendering a message, includes lines and clickable block positions
struct RenderResult {
    lines: Vec<Line<'static>>,
    /// Block click targets: (line_offset_within_message, block_id, block_type)
    click_targets: Vec<(usize, String, BlockType)>,
}

/// Render a message with collapsible blocks for assistant messages
///
/// For assistant messages, this parses the content to extract thinking
/// and tool blocks, rendering them as collapsible elements.
///
/// # Requirements
/// - 2.2, 2.3: Display detected username for user, "Tark" for assistant
/// - 7.1: Display thinking content in Thinking_Block
/// - 8.1: Display tool execution in Tool_Block
/// - 9.1, 9.3: Display thinking_content in collapsible ThinkingBlock
fn render_message_with_blocks(
    message: &ChatMessage,
    is_selected: bool,
    width: u16,
    block_state: &CollapsibleBlockState,
    username: &str,
    show_thinking: bool,
) -> RenderResult {
    let mut lines = Vec::new();
    let mut click_targets: Vec<(usize, String, BlockType)> = Vec::new();

    // Header line with role icon and name
    let header_style = if is_selected {
        Style::default()
            .fg(message.role.color())
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(message.role.color())
            .add_modifier(Modifier::BOLD)
    };

    // Use detected username for user messages, "Tark" for assistant (Requirements 2.2, 2.3, 2.4)
    let display_name = match message.role {
        Role::User => username.to_string(),
        Role::Assistant => "Tark".to_string(),
        _ => message.role.name().to_string(),
    };

    let timestamp = message.timestamp.format("%H:%M").to_string();
    let header = format!("{} {} [{}]", message.role.icon(), display_name, timestamp);
    lines.push(Line::from(Span::styled(header, header_style)));

    // Render thinking_content as a collapsible block if present (Requirements 9.1, 9.3)
    // Only render if show_thinking is enabled (controlled by /thinking command)
    if show_thinking && message.role == Role::Assistant && !message.thinking_content.is_empty() {
        let thinking_block_id = format!("{}-thinking-stream", message.id);
        let thinking_content: Vec<String> = message
            .thinking_content
            .lines()
            .map(|s| s.to_string())
            .collect();
        let thinking_block =
            CollapsibleBlock::thinking(thinking_block_id.clone(), thinking_content);
        let is_expanded = block_state.is_expanded(&thinking_block_id, BlockType::Thinking);
        let scroll_offset = block_state.scroll_offset(&thinking_block_id);
        let is_focused = block_state.is_focused(&thinking_block_id);
        // Track click target at current line position (before adding block lines)
        click_targets.push((lines.len(), thinking_block_id.clone(), BlockType::Thinking));
        let block_lines = render_collapsible_block(
            &thinking_block,
            is_expanded,
            width,
            scroll_offset,
            is_focused,
        );
        lines.extend(block_lines);
    }

    // For assistant messages, use segments if available (inline tool display)
    if message.role == Role::Assistant && !message.segments.is_empty() {
        // Use interleaved segments built during streaming
        let content_style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };

        let message_id = message.id.to_string();
        let mut thinking_block_counter = 0;

        for segment in message.segments.iter() {
            match segment {
                MessageSegment::Text(text) => {
                    // Parse thinking blocks from text (for OpenAI and other providers that embed thinking in text)
                    // Use ParsedMessageContent to properly extract and render thinking blocks
                    let parsed = ParsedMessageContent::parse(text, &[], &message_id);

                    for content_segment in &parsed.segments {
                        match content_segment {
                            ContentSegment::Text(remaining_text) => {
                                if !remaining_text.is_empty() {
                                    let content_lines = format_content_owned_with_width(
                                        remaining_text.clone(),
                                        content_style,
                                        width as usize,
                                    );
                                    lines.extend(content_lines);
                                }
                            }
                            ContentSegment::Block(block) => {
                                // Render thinking blocks if show_thinking is enabled
                                if show_thinking && block.block_type() == BlockType::Thinking {
                                    // Use unique ID for this thinking block
                                    let thinking_id = format!(
                                        "{}-inline-thinking-{}",
                                        message_id, thinking_block_counter
                                    );
                                    thinking_block_counter += 1;
                                    let is_expanded =
                                        block_state.is_expanded(&thinking_id, BlockType::Thinking);
                                    let scroll_offset = block_state.scroll_offset(&thinking_id);
                                    let is_focused = block_state.is_focused(&thinking_id);
                                    // Track click target at current line position
                                    click_targets.push((
                                        lines.len(),
                                        thinking_id.clone(),
                                        BlockType::Thinking,
                                    ));
                                    // Create a new block with our tracking ID
                                    let thinking_block = CollapsibleBlock::thinking(
                                        thinking_id,
                                        block.content().to_vec(),
                                    );
                                    let block_lines = render_collapsible_block(
                                        &thinking_block,
                                        is_expanded,
                                        width,
                                        scroll_offset,
                                        is_focused,
                                    );
                                    lines.extend(block_lines);
                                }
                            }
                        }
                    }
                }
                MessageSegment::ToolRef(idx) => {
                    // Dereference to get actual tool info
                    if let Some(tool_info) = message.tool_call_info.get(*idx) {
                        // Use the stored block_id from tool_info (single source of truth)
                        let block_id = &tool_info.block_id;
                        let is_expanded = block_state.is_expanded(block_id, BlockType::Tool);

                        // Track click target at current line position
                        click_targets.push((lines.len(), block_id.clone(), BlockType::Tool));

                        let is_focused = block_state.is_focused(block_id);

                        if is_expanded {
                            // Render expanded box format
                            let tool_block = create_tool_block_from_info(tool_info, block_id);
                            let scroll_offset = block_state.scroll_offset(block_id);
                            let block_lines = render_collapsible_block(
                                &tool_block,
                                true,
                                width,
                                scroll_offset,
                                is_focused,
                            );
                            lines.extend(block_lines);
                        } else {
                            // Render collapsed inline format (single line)
                            let inline_line = render_tool_inline(tool_info, width, is_focused);
                            lines.push(inline_line);
                        }
                    }
                }
            }
        }
    } else if message.role == Role::Assistant && !message.tool_call_info.is_empty() {
        // Fallback: use ParsedMessageContent for legacy/non-streaming path
        let message_id = message.id.to_string();
        let parsed =
            ParsedMessageContent::parse(&message.content, &message.tool_call_info, &message_id);

        // Render each segment
        for segment in &parsed.segments {
            match segment {
                ContentSegment::Text(text) => {
                    let content_style = if is_selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let content_lines = format_content_owned_with_width(
                        text.clone(),
                        content_style,
                        width as usize,
                    );
                    lines.extend(content_lines);
                }
                ContentSegment::Block(block) => {
                    // Skip thinking blocks if show_thinking is disabled
                    if !show_thinking && block.block_type() == BlockType::Thinking {
                        continue;
                    }
                    // Track click target at current line position
                    click_targets.push((lines.len(), block.id().to_string(), block.block_type()));
                    let is_expanded = block_state.is_expanded(block.id(), block.block_type());
                    let scroll_offset = block_state.scroll_offset(block.id());
                    let is_focused = block_state.is_focused(block.id());
                    let block_lines = render_collapsible_block(
                        block,
                        is_expanded,
                        width,
                        scroll_offset,
                        is_focused,
                    );
                    lines.extend(block_lines);
                }
            }
        }
    } else if message.role == Role::Assistant {
        // Check for thinking blocks in content even without tool calls
        let message_id = message.id.to_string();
        let parsed = ParsedMessageContent::parse(&message.content, &[], &message_id);

        if parsed.has_thinking() && show_thinking {
            // Render with collapsible blocks (only if show_thinking enabled)
            for segment in &parsed.segments {
                match segment {
                    ContentSegment::Text(text) => {
                        let content_style = if is_selected {
                            Style::default().add_modifier(Modifier::REVERSED)
                        } else {
                            Style::default()
                        };
                        let content_lines = format_content_owned_with_width(
                            text.clone(),
                            content_style,
                            width as usize,
                        );
                        lines.extend(content_lines);
                    }
                    ContentSegment::Block(block) => {
                        // Skip thinking blocks if show_thinking is disabled
                        if !show_thinking && block.block_type() == BlockType::Thinking {
                            continue;
                        }
                        // Track click target at current line position
                        click_targets.push((
                            lines.len(),
                            block.id().to_string(),
                            block.block_type(),
                        ));
                        let is_expanded = block_state.is_expanded(block.id(), block.block_type());
                        let scroll_offset = block_state.scroll_offset(block.id());
                        let is_focused = block_state.is_focused(block.id());
                        let block_lines = render_collapsible_block(
                            block,
                            is_expanded,
                            width,
                            scroll_offset,
                            is_focused,
                        );
                        lines.extend(block_lines);
                    }
                }
            }
        } else if parsed.has_thinking() && !show_thinking {
            // Has thinking but display is disabled - render only text segments
            for segment in &parsed.segments {
                if let ContentSegment::Text(text) = segment {
                    let content_style = if is_selected {
                        Style::default().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::default()
                    };
                    let content_lines = format_content_owned_with_width(
                        text.clone(),
                        content_style,
                        width as usize,
                    );
                    lines.extend(content_lines);
                }
            }
        } else {
            // No thinking blocks, render normally
            let content_style = if is_selected {
                Style::default().add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            let content_lines = format_content_owned_with_width(
                message.content.clone(),
                content_style,
                width as usize,
            );
            lines.extend(content_lines);
        }
    } else {
        // Non-assistant messages: render content normally
        let content_style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let content_lines =
            format_content_owned_with_width(message.content.clone(), content_style, width as usize);
        lines.extend(content_lines);
    }

    // Legacy tool calls display (for backward compatibility)
    for tool_call in &message.tool_calls {
        let tool_style = Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::ITALIC);
        lines.push(Line::from(Span::styled(
            format!("  [tool: {}]", tool_call.clone()),
            tool_style,
        )));
    }

    // Streaming indicator
    if message.is_streaming {
        lines.push(Line::from(Span::styled(
            "  ▌",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::SLOW_BLINK),
        )));
    }

    // Add spacing
    lines.push(Line::from(""));

    RenderResult {
        lines,
        click_targets,
    }
}

/// Wrap text to fit within a given width using word boundaries
/// Returns a vector of wrapped lines
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;

    if max_width == 0 || text.is_empty() {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_inclusive(|c: char| c.is_whitespace()) {
        let word_width = UnicodeWidthStr::width(word);

        if current_width + word_width > max_width && !current_line.is_empty() {
            // Push current line and start new one
            lines.push(current_line.trim_end().to_string());
            current_line = word.trim_start().to_string();
            current_width = UnicodeWidthStr::width(current_line.as_str());
        } else {
            current_line.push_str(word);
            current_width += word_width;
        }
    }

    // Don't forget the last line
    if !current_line.is_empty() {
        lines.push(current_line.trim_end().to_string());
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

/// Format content with owned strings for 'static lifetime
/// Supports markdown: headers, code blocks, lists, blockquotes, horizontal rules
/// If width is provided (> 0), text will be wrapped to fit within the width
fn format_content_owned(content: String, base_style: Style) -> Vec<Line<'static>> {
    format_content_owned_with_width(content, base_style, 0)
}

/// Format content with owned strings and optional word wrapping
fn format_content_owned_with_width(
    content: String,
    base_style: Style,
    width: usize,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();
    let mut code_lang = String::new();

    // Reserve some space for margins/indentation
    let wrap_width = if width > 4 { width - 2 } else { 0 };

    for line in content.lines() {
        // Code block handling
        if line.starts_with("```") {
            if in_code_block {
                // End of code block - render accumulated lines
                for code_line in &code_block_lines {
                    lines.push(Line::from(Span::styled(
                        format!("  │ {}", code_line),
                        Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 40)),
                    )));
                }
                code_block_lines.clear();
                in_code_block = false;
                code_lang.clear();
            } else {
                // Start of code block - capture language
                code_lang = line.trim_start_matches('`').to_string();
                if !code_lang.is_empty() {
                    lines.push(Line::from(Span::styled(
                        format!("  ┌─ {} ", code_lang),
                        Style::default().fg(Color::DarkGray),
                    )));
                }
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_block_lines.push(line.to_string());
            continue;
        }

        // Horizontal rule
        if line.trim() == "---" || line.trim() == "***" || line.trim() == "___" {
            lines.push(Line::from(Span::styled(
                "────────────────────────────────────────",
                Style::default().fg(Color::DarkGray),
            )));
            continue;
        }

        // Headers (# ## ###)
        if let Some(header_line) = format_header(line.to_string()) {
            lines.push(header_line);
            continue;
        }

        // Blockquote (> text)
        if line.trim_start().starts_with('>') {
            let quote_text = line.trim_start().trim_start_matches('>').trim();
            // Wrap quote text if needed
            if wrap_width > 6 {
                for wrapped in wrap_text(quote_text, wrap_width - 4) {
                    lines.push(Line::from(vec![
                        Span::styled("  │ ", Style::default().fg(Color::Blue)),
                        Span::styled(
                            wrapped,
                            Style::default()
                                .fg(Color::Blue)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }
            } else {
                lines.push(Line::from(vec![
                    Span::styled("  │ ", Style::default().fg(Color::Blue)),
                    Span::styled(
                        quote_text.to_string(),
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::ITALIC),
                    ),
                ]));
            }
            continue;
        }

        // Unordered list (- or * at start)
        if let Some(list_lines) = format_list_item_wrapped(line.to_string(), base_style, wrap_width)
        {
            lines.extend(list_lines);
            continue;
        }

        // Numbered list (1. 2. etc)
        if let Some(num_lines) =
            format_numbered_list_wrapped(line.to_string(), base_style, wrap_width)
        {
            lines.extend(num_lines);
            continue;
        }

        // Regular line with inline formatting and wrapping
        if wrap_width > 0 {
            let wrapped_lines = wrap_text(line, wrap_width);
            for wrapped in wrapped_lines {
                let formatted_line = format_inline_owned(wrapped, base_style);
                lines.push(formatted_line);
            }
        } else {
            let formatted_line = format_inline_owned(line.to_string(), base_style);
            lines.push(formatted_line);
        }
    }

    // Handle unclosed code block
    if in_code_block {
        for code_line in &code_block_lines {
            lines.push(Line::from(Span::styled(
                format!("  │ {}", code_line),
                Style::default().fg(Color::Cyan).bg(Color::Rgb(30, 30, 40)),
            )));
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(""));
    }

    lines
}

/// Format markdown header (# ## ### etc)
fn format_header(line: String) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        let level = trimmed.chars().take_while(|c| *c == '#').count();
        if level <= 6 {
            let text = trimmed.trim_start_matches('#').trim();
            if !text.is_empty() {
                let (style, prefix) = match level {
                    1 => (
                        Style::default()
                            .fg(Color::Magenta)
                            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                        "█ ",
                    ),
                    2 => (
                        Style::default()
                            .fg(Color::Blue)
                            .add_modifier(Modifier::BOLD),
                        "▌ ",
                    ),
                    3 => (
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                        "▎ ",
                    ),
                    _ => (
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(Modifier::BOLD),
                        "  ",
                    ),
                };
                return Some(Line::from(vec![
                    Span::styled(prefix.to_string(), style),
                    Span::styled(text.to_string(), style),
                ]));
            }
        }
    }
    None
}

/// Format unordered list item with text wrapping (- or * at start)
fn format_list_item_wrapped(
    line: String,
    base_style: Style,
    wrap_width: usize,
) -> Option<Vec<Line<'static>>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let indent_str = "  ".repeat(indent / 2);

    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let text = trimmed[2..].to_string();

        // Calculate prefix width: indent + "  • "
        let prefix_width = indent_str.len() + 4;
        let continuation_indent = " ".repeat(prefix_width);

        // Wrap the text content
        let text_wrap_width = wrap_width.saturating_sub(prefix_width);

        let wrapped = if text_wrap_width > 0 {
            wrap_text(&text, text_wrap_width)
        } else {
            vec![text]
        };

        let mut result = Vec::new();
        for (i, wrapped_line) in wrapped.into_iter().enumerate() {
            let formatted = format_inline_owned(wrapped_line, base_style);
            if i == 0 {
                // First line: include bullet
                let mut spans = vec![
                    Span::raw(indent_str.clone()),
                    Span::styled("  • ", Style::default().fg(Color::Yellow)),
                ];
                spans.extend(formatted.spans);
                result.push(Line::from(spans));
            } else {
                // Continuation lines: just indent
                let mut spans = vec![Span::raw(continuation_indent.clone())];
                spans.extend(formatted.spans);
                result.push(Line::from(spans));
            }
        }
        return Some(result);
    }
    None
}

/// Format unordered list item (- or * at start) - non-wrapping version for tests
fn format_list_item(line: String, base_style: Style) -> Option<Line<'static>> {
    format_list_item_wrapped(line, base_style, 0).map(|mut v| v.remove(0))
}

/// Format numbered list item with text wrapping (1. 2. etc)
fn format_numbered_list_wrapped(
    line: String,
    base_style: Style,
    wrap_width: usize,
) -> Option<Vec<Line<'static>>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let indent_str = "  ".repeat(indent / 2);

    // Check for pattern like "1. " "2. " "10. " etc
    if let Some(dot_pos) = trimmed.find(". ") {
        let num_part = &trimmed[..dot_pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) {
            let text = trimmed[dot_pos + 2..].to_string();

            // Calculate prefix width: indent + "  N. "
            let prefix = format!("  {}. ", num_part);
            let prefix_width = indent_str.len() + prefix.len();
            let continuation_indent = " ".repeat(prefix_width);

            // Wrap the text content
            let text_wrap_width = wrap_width.saturating_sub(prefix_width);

            let wrapped = if text_wrap_width > 0 {
                wrap_text(&text, text_wrap_width)
            } else {
                vec![text]
            };

            let mut result = Vec::new();
            for (i, wrapped_line) in wrapped.into_iter().enumerate() {
                let formatted = format_inline_owned(wrapped_line, base_style);
                if i == 0 {
                    // First line: include number
                    let mut spans = vec![
                        Span::raw(indent_str.clone()),
                        Span::styled(prefix.clone(), Style::default().fg(Color::Yellow)),
                    ];
                    spans.extend(formatted.spans);
                    result.push(Line::from(spans));
                } else {
                    // Continuation lines: just indent
                    let mut spans = vec![Span::raw(continuation_indent.clone())];
                    spans.extend(formatted.spans);
                    result.push(Line::from(spans));
                }
            }
            return Some(result);
        }
    }
    None
}

/// Format numbered list item (1. 2. etc) - non-wrapping version for tests
fn format_numbered_list(line: String, base_style: Style) -> Option<Line<'static>> {
    format_numbered_list_wrapped(line, base_style, 0).map(|mut v| v.remove(0))
}

/// Format inline markdown with owned strings
/// Supports: **bold**, *italic*, _italic_, `code`, ~~strikethrough~~
fn format_inline_owned(line: String, base_style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_bold = false;
    let mut in_italic = false;
    let mut in_code = false;
    let mut in_strike = false;

    while let Some(c) = chars.next() {
        match c {
            // Bold: **text**
            '*' if chars.peek() == Some(&'*') && !in_code => {
                chars.next();
                if !current.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut current),
                        get_inline_style(base_style, in_bold, in_italic, in_strike),
                    ));
                }
                in_bold = !in_bold;
            }
            // Italic: *text* (single asterisk, not at word boundary for list items)
            '*' if !in_code && !in_bold => {
                if !current.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut current),
                        get_inline_style(base_style, in_bold, in_italic, in_strike),
                    ));
                }
                in_italic = !in_italic;
            }
            // Italic: _text_
            '_' if !in_code => {
                if !current.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut current),
                        get_inline_style(base_style, in_bold, in_italic, in_strike),
                    ));
                }
                in_italic = !in_italic;
            }
            // Strikethrough: ~~text~~
            '~' if chars.peek() == Some(&'~') && !in_code => {
                chars.next();
                if !current.is_empty() {
                    spans.push(Span::styled(
                        std::mem::take(&mut current),
                        get_inline_style(base_style, in_bold, in_italic, in_strike),
                    ));
                }
                in_strike = !in_strike;
            }
            // Inline code: `code`
            '`' => {
                if !current.is_empty() {
                    let style = if in_code {
                        Style::default()
                            .fg(Color::Yellow)
                            .bg(Color::Rgb(40, 40, 50))
                    } else {
                        get_inline_style(base_style, in_bold, in_italic, in_strike)
                    };
                    spans.push(Span::styled(std::mem::take(&mut current), style));
                }
                in_code = !in_code;
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        let style = if in_code {
            Style::default()
                .fg(Color::Yellow)
                .bg(Color::Rgb(40, 40, 50))
        } else {
            get_inline_style(base_style, in_bold, in_italic, in_strike)
        };
        spans.push(Span::styled(current, style));
    }

    if spans.is_empty() {
        Line::from("")
    } else {
        Line::from(spans)
    }
}

/// Get style for inline markdown based on active formatting
fn get_inline_style(base_style: Style, bold: bool, italic: bool, strike: bool) -> Style {
    let mut style = base_style;
    if bold {
        style = style.add_modifier(Modifier::BOLD);
    }
    if italic {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if strike {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}

/// Widget implementation for MessageList
pub struct MessageListWidget<'a> {
    message_list: &'a mut MessageList,
    block: Option<Block<'a>>,
    /// Username to display for user messages (Requirements 2.2, 2.5)
    username: String,
    /// Whether the widget is focused (controls scrollbar visibility)
    focused: bool,
}

impl<'a> MessageListWidget<'a> {
    /// Create a new message list widget
    pub fn new(message_list: &'a mut MessageList) -> Self {
        Self {
            message_list,
            block: None,
            username: "You".to_string(), // Default fallback (Requirement 2.5)
            focused: false,
        }
    }

    /// Set the block for the widget
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Set the username to display for user messages (Requirements 2.2, 2.5)
    pub fn username(mut self, username: impl Into<String>) -> Self {
        self.username = username.into();
        self
    }

    /// Set whether the widget is focused (controls scrollbar visibility)
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Render the widget to a buffer
    pub fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate inner area
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        // Update visible height
        self.message_list.set_visible_height(inner.height);

        if self.message_list.is_empty() {
            // Show empty state
            let empty_text = Paragraph::new("No messages yet. Type a message to start chatting.")
                .style(Style::default().fg(Color::DarkGray));
            empty_text.render(inner, buf);
            return;
        }

        // Clear click targets before rendering
        self.message_list.clear_block_click_targets();

        // Collect all lines from all messages, using collapsible block rendering
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        let mut accumulated_lines = 0usize;
        let block_state = &self.message_list.block_state;
        let username = &self.username;
        let show_thinking = self.message_list.show_thinking;
        // Collect messages and their render results first (to avoid borrow issues)
        let messages_snapshot: Vec<_> = self.message_list.messages.iter().enumerate().collect();
        let selected = self.message_list.selected;

        // Collect click targets to add after iteration
        let mut click_targets_to_add: Vec<(usize, String, BlockType)> = Vec::new();

        for (idx, message) in messages_snapshot {
            let is_selected = selected == Some(idx);
            // Use the new render function that supports collapsible blocks
            // Pass username for display name formatting (Requirements 2.2, 2.3)
            let result = render_message_with_blocks(
                message,
                is_selected,
                inner.width,
                block_state,
                username,
                show_thinking,
            );

            // Collect click targets with adjusted line offsets
            for (line_offset, block_id, block_type) in result.click_targets {
                click_targets_to_add.push((accumulated_lines + line_offset, block_id, block_type));
            }

            accumulated_lines += result.lines.len();
            all_lines.extend(result.lines);
        }

        // Register all click targets
        for (line, block_id, block_type) in click_targets_to_add {
            self.message_list
                .add_block_click_target(line, block_id, block_type);
        }

        // Apply scroll offset
        let scroll_offset = self.message_list.scroll_offset;
        let visible_lines: Vec<Line<'static>> = all_lines
            .into_iter()
            .skip(scroll_offset)
            .take(inner.height as usize)
            .collect();

        // Render the paragraph
        let paragraph = Paragraph::new(Text::from(visible_lines));
        paragraph.render(inner, buf);

        // Apply selection highlighting when focused
        if self.focused && self.message_list.selection.active {
            let selection = &self.message_list.selection;
            let start = selection.start_pos();
            let end = selection.end_pos();
            let cursor_pos = &self.message_list.cursor_pos;

            // Calculate which lines are visible
            let visible_start = scroll_offset;
            let visible_end = scroll_offset + inner.height as usize;

            // Highlight selected lines
            for screen_row in 0..inner.height {
                let content_line = scroll_offset + screen_row as usize;

                // Check if this line is within the selection
                if content_line >= start.line && content_line <= end.line {
                    let line_start_col = if content_line == start.line {
                        start.col
                    } else {
                        0
                    };
                    let line_end_col = if content_line == end.line {
                        end.col
                    } else {
                        inner.width as usize
                    };

                    // Apply selection highlight to the cells in this line
                    for col in line_start_col..line_end_col.min(inner.width as usize) {
                        let x = inner.x + col as u16;
                        let y = inner.y + screen_row;
                        if x < inner.x + inner.width && y < inner.y + inner.height {
                            if let Some(cell) = buf.cell_mut((x, y)) {
                                cell.set_bg(Color::DarkGray);
                                cell.set_fg(Color::White);
                            }
                        }
                    }
                }
            }

            // Show cursor position indicator (only if within visible area)
            if cursor_pos.line >= visible_start && cursor_pos.line < visible_end {
                let cursor_screen_row = (cursor_pos.line - scroll_offset) as u16;
                let cursor_col = cursor_pos.col.min(inner.width.saturating_sub(1) as usize) as u16;
                let x = inner.x + cursor_col;
                let y = inner.y + cursor_screen_row;
                if x < inner.x + inner.width && y < inner.y + inner.height {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(Color::Cyan);
                        cell.set_fg(Color::Black);
                    }
                }
            }
        } else if self.focused {
            // Show cursor position even without selection (when focused)
            let cursor_pos = &self.message_list.cursor_pos;
            let visible_start = scroll_offset;
            let visible_end = scroll_offset + inner.height as usize;

            if cursor_pos.line >= visible_start && cursor_pos.line < visible_end {
                let cursor_screen_row = (cursor_pos.line - scroll_offset) as u16;

                // Visual line indicator: show `│` at left edge of cursor line
                // This helps users know which line they're on in Normal mode
                let y = inner.y + cursor_screen_row;
                if y < inner.y + inner.height {
                    // Add a subtle left-edge indicator for the cursor line
                    if let Some(cell) = buf.cell_mut((inner.x, y)) {
                        cell.set_symbol("│");
                        cell.set_fg(Color::Cyan);
                    }
                }

                // Also highlight the cursor character position
                let cursor_col = cursor_pos.col.min(inner.width.saturating_sub(1) as usize) as u16;
                let x = inner.x + cursor_col;
                if x < inner.x + inner.width && y < inner.y + inner.height {
                    if let Some(cell) = buf.cell_mut((x, y)) {
                        cell.set_bg(Color::Cyan);
                        cell.set_fg(Color::Black);
                    }
                }
            }
        }

        // Render scrollbar only when focused and content overflows
        let total_lines = self.message_list.total_lines(inner.width);
        let visible_height = inner.height as usize;

        if self.focused && total_lines > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"))
                .track_symbol(Some("│"))
                .thumb_symbol("█");

            // ScrollbarState expects:
            // - content_length: total number of lines (for thumb size calculation)
            // - viewport_content_length: number of visible lines (optional, improves thumb sizing)
            // - position: current scroll offset (0 = top)
            //
            // The thumb size = (viewport / content_length) * track_height
            // Position maps scroll_offset to thumb position on the track
            let mut scrollbar_state = ScrollbarState::new(total_lines)
                .viewport_content_length(visible_height)
                .position(scroll_offset);

            let scrollbar_area = Rect {
                x: inner.x + inner.width.saturating_sub(1),
                y: inner.y,
                width: 1,
                height: inner.height,
            };

            scrollbar.render(scrollbar_area, buf, &mut scrollbar_state);
        }
    }
}

impl Widget for MessageListWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render(area, buf);
    }
}

// ============================================================================
// Inline Tool Rendering Helpers
// ============================================================================

/// Create a CollapsibleBlock from ToolCallInfo for expanded rendering
fn create_tool_block_from_info(tool_info: &ToolCallInfo, block_id: &str) -> CollapsibleBlock {
    // Build content with args and result
    let mut content: Vec<String> = Vec::new();

    // Format args as key=value pairs (no truncation - will be wrapped when rendered)
    if let Some(obj) = tool_info.args.as_object() {
        for (key, value) in obj {
            let value_str = match value {
                serde_json::Value::String(s) => s.clone(),
                _ => value.to_string(),
            };
            // Don't truncate - let the renderer wrap long lines
            content.push(format!("  {}: {}", key, value_str));
        }
        if !obj.is_empty() {
            content.push(String::new()); // Empty line separator
        }
    }

    // Add result preview lines (no truncation - will be wrapped when rendered)
    for line in tool_info.result_preview.lines() {
        content.push(format!("  {}", line));
    }

    // Use human-readable action as the header (truncated for display)
    let header = format_tool_action(&tool_info.tool, &tool_info.args);
    CollapsibleBlock::tool(block_id.to_string(), header, content)
}

/// Render a tool as a single inline line (collapsed format)
///
/// Format (success): `▶ ⚙️ tool_action  →  ✓ summary  │  preview...`
/// Format (running): `▶ ⚙️ tool_action  →  ⏳ Running...`
/// Format (error):   `▶ ⚠️ tool_action FAILED  →  error message...`
/// When focused (for scrolling), adds a visual indicator `»`
fn render_tool_inline(tool_info: &ToolCallInfo, width: u16, is_focused: bool) -> Line<'static> {
    let tool_display_raw = format_tool_action(&tool_info.tool, &tool_info.args);
    let is_error = tool_info.is_error();
    let is_running = tool_info.result_preview == "⏳ Running...";

    // Fixed prefix widths:
    // focus_indicator (1) + "▶ 🔧 " (5) = 6 chars before tool_display
    // For errors: focus_indicator (1) + "▶ ⚠️ " (5) + " FAILED" (7) + "  →  " (5) = 18 chars fixed
    let prefix_width = 6;
    let available_width = (width as usize).saturating_sub(prefix_width);

    // For errors, use warning icon and red styling
    if is_error {
        // Error format: "▶ ⚠️ " (5) + tool_display + " FAILED" (7) + "  →  " (5) + error_preview
        let error_fixed_chars = 5 + 7 + 5 + 1; // +1 for focus indicator
        let max_tool_width = (width as usize)
            .saturating_sub(error_fixed_chars + 10)
            .min(50);
        let tool_display = truncate_str(&tool_display_raw, max_tool_width);
        let remaining =
            (width as usize).saturating_sub(error_fixed_chars + tool_display.chars().count());
        let error_preview = tool_info
            .error
            .as_ref()
            .map(|e| truncate_str(e, remaining.max(10)))
            .unwrap_or_else(|| truncate_str(&tool_info.result_preview, remaining.max(10)));

        let focus_indicator = if is_focused { "»" } else { " " };
        let focus_style = if is_focused {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        return Line::from(vec![
            Span::styled(focus_indicator, focus_style),
            Span::styled("▶ ⚠️ ", Style::default().fg(Color::Red)),
            Span::styled(tool_display, Style::default().fg(Color::Red)),
            Span::styled(
                " FAILED",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled("  →  ", Style::default().fg(Color::DarkGray)),
            Span::styled(error_preview, Style::default().fg(Color::Red)),
        ]);
    }

    let (status_icon, status_style) = if is_running {
        ("⏳", Style::default().fg(Color::Yellow))
    } else {
        ("✓", Style::default().fg(Color::Green))
    };

    let summary = extract_summary(&tool_info.result_preview, &tool_info.tool);

    // Calculate max width for tool_display to fit within line
    // Layout: focus(1) + "▶ 🔧 "(5) + tool_display + "  →  "(5) + status(2) + summary
    // Reserve space for status and arrow, truncate tool_display if needed
    let fixed_overhead = 1 + 5 + 5 + 2; // focus + icon + arrow + status_icon + space
    let summary_len = summary.chars().count();
    let max_tool_width = (width as usize)
        .saturating_sub(fixed_overhead + summary_len)
        .min(available_width.saturating_sub(12)); // Leave room for arrow and status

    let tool_display = if tool_display_raw.chars().count() > max_tool_width && max_tool_width > 3 {
        truncate_str(&tool_display_raw, max_tool_width)
    } else {
        tool_display_raw
    };

    // Recalculate with actual tool_display length
    let tool_display_len = tool_display.chars().count();
    let used = fixed_overhead + tool_display_len + summary_len;
    let preview_width = if (used + 10) < width as usize {
        (width as usize).saturating_sub(used + 5).min(60) // +5 for "  │  "
    } else {
        0
    };

    let preview = if preview_width > 5 && !is_running {
        let first_line = tool_info
            .result_preview
            .lines()
            .find(|l| !l.is_empty() && !l.starts_with("shell>"))
            .unwrap_or("");
        truncate_str(first_line, preview_width)
    } else {
        String::new()
    };

    // Build the line spans - add focus indicator if focused
    let focus_indicator = if is_focused { "»" } else { " " };
    let focus_style = if is_focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let mut spans = vec![
        Span::styled(focus_indicator, focus_style),
        Span::styled("▶ 🔧 ", Style::default().fg(Color::Magenta)),
        Span::styled(tool_display, Style::default()),
        Span::styled("  →  ", Style::default().fg(Color::DarkGray)),
        Span::styled(format!("{} ", status_icon), status_style),
        Span::styled(summary, Style::default().fg(Color::Gray)),
    ];

    if !preview.is_empty() {
        spans.push(Span::styled("  │  ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(preview, Style::default().fg(Color::DarkGray)));
    }

    Line::from(spans)
}

/// Format a tool action in human-readable form
///
/// Shell tools show `$ command`, others show key parameters
fn format_tool_action(tool: &str, args: &serde_json::Value) -> String {
    match tool {
        "shell" | "safe_shell" | "execute" => {
            let cmd = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("$ {}", truncate_str(cmd, 50))
        }
        "grep" | "ripgrep" => {
            let pattern = args
                .get("pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("grep \"{}\" in {}", truncate_str(pattern, 20), path)
        }
        "read_file" | "file_preview" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("...");
            let start = args.get("start_line").and_then(|v| v.as_u64());
            let end = args.get("end_line").and_then(|v| v.as_u64());
            if let (Some(s), Some(e)) = (start, end) {
                format!("read {}:{}-{}", path, s, e)
            } else {
                format!("read {}", path)
            }
        }
        "read_files" => {
            let paths = args.get("paths").and_then(|v| v.as_array());
            if let Some(p) = paths {
                let count = p.len();
                format!("read {} files", count)
            } else {
                "read files".to_string()
            }
        }
        "write_file" | "patch_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("...");
            format!("{} {}", tool.replace('_', " "), path)
        }
        "delete_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("...");
            format!("delete {}", path)
        }
        "list_directory" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("ls {}", path)
        }
        "file_search" => {
            let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("...");
            format!("find \"{}\"", truncate_str(query, 30))
        }
        "codebase_overview" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
            format!("overview {}", path)
        }
        "find_references" | "find_all_references" => {
            let pattern = args
                .get("pattern")
                .or_else(|| args.get("symbol"))
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("refs \"{}\"", truncate_str(pattern, 30))
        }
        "list_symbols" => {
            let file = args
                .get("file")
                .or_else(|| args.get("path"))
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("symbols {}", file)
        }
        "go_to_definition" => {
            let symbol = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("...");
            format!("goto {}", truncate_str(symbol, 30))
        }
        "call_hierarchy" => {
            let symbol = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("...");
            format!("calls {}", truncate_str(symbol, 30))
        }
        "get_signature" => {
            let symbol = args.get("symbol").and_then(|v| v.as_str()).unwrap_or("...");
            format!("sig {}", truncate_str(symbol, 30))
        }
        "ask_user" => {
            let question = args
                .get("question")
                .and_then(|v| v.as_str())
                .unwrap_or("...");
            format!("ask: {}", truncate_str(question, 40))
        }
        "propose_change" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("...");
            format!("propose {}", path)
        }
        _ => tool.replace('_', " "),
    }
}

/// Extract a concise summary from the result preview
fn extract_summary(result_preview: &str, tool: &str) -> String {
    if result_preview == "⏳ Running..." {
        return "Running...".to_string();
    }

    // For shell commands, extract exit code
    if tool == "shell" || tool == "safe_shell" || tool == "execute" {
        if result_preview.contains("Exit: 0") || result_preview.contains("exit: 0") {
            return "Exit: 0".to_string();
        } else if let Some(line) = result_preview.lines().find(|l| l.contains("Exit:")) {
            if let Some(code) = line.split("Exit:").nth(1) {
                return format!("Exit:{}", code.split_whitespace().next().unwrap_or("?"));
            }
        }
    }

    // Count output lines
    let line_count = result_preview.lines().count();
    if line_count > 1 {
        return format!("{} lines", line_count);
    }

    // Count matches for grep-like tools
    if tool == "grep" || tool == "ripgrep" || tool.contains("find") {
        let match_count = result_preview
            .lines()
            .filter(|l| !l.is_empty() && !l.starts_with("shell>"))
            .count();
        if match_count > 0 {
            return format!(
                "{} match{}",
                match_count,
                if match_count == 1 { "" } else { "es" }
            );
        }
    }

    // Default: truncate first line
    let first_line = result_preview.lines().next().unwrap_or("");
    truncate_str(first_line, 20)
}

/// Truncate a string to max_len, adding ellipsis if needed
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_len.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_icon() {
        assert_eq!(Role::User.icon(), "👤");
        assert_eq!(Role::Assistant.icon(), "🤖");
        assert_eq!(Role::System.icon(), "⚙️");
        assert_eq!(Role::Tool.icon(), "🔧");
    }

    #[test]
    fn test_role_name() {
        assert_eq!(Role::User.name(), "You");
        assert_eq!(Role::Assistant.name(), "Tark"); // Changed from "Assistant" per Requirements 2.3
        assert_eq!(Role::System.name(), "System");
        assert_eq!(Role::Tool.name(), "Tool");
    }

    #[test]
    fn test_chat_message_creation() {
        let msg = ChatMessage::user("Hello");
        assert_eq!(msg.role, Role::User);
        assert_eq!(msg.content, "Hello");
        assert!(!msg.is_streaming);
        assert!(msg.tool_calls.is_empty());
    }

    #[test]
    fn test_chat_message_with_streaming() {
        let msg = ChatMessage::assistant("Thinking...").with_streaming(true);
        assert!(msg.is_streaming);
    }

    #[test]
    fn test_chat_message_with_tool_call() {
        let msg = ChatMessage::assistant("Let me check that file").with_tool_call("read_file");
        assert_eq!(msg.tool_calls.len(), 1);
        assert_eq!(msg.tool_calls[0], "read_file");
    }

    #[test]
    fn test_message_list_empty() {
        let list = MessageList::new();
        assert!(list.is_empty());
        assert_eq!(list.len(), 0);
    }

    #[test]
    fn test_message_list_push() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Hello"));
        assert_eq!(list.len(), 1);
        assert!(!list.is_empty());
    }

    #[test]
    fn test_message_list_clear() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Hello"));
        list.push(ChatMessage::assistant("Hi"));
        list.clear();
        assert!(list.is_empty());
    }

    #[test]
    fn test_message_list_scroll() {
        let mut list = MessageList::new();
        list.set_visible_height(10);

        // Add enough messages to enable scrolling
        for i in 0..20 {
            list.push(ChatMessage::user(format!("Message {}", i)));
        }

        assert_eq!(list.scroll_offset(), 0);

        list.scroll_down(80);
        assert!(list.scroll_offset() > 0);

        list.scroll_up();
        // Scroll offset should decrease or stay at 0
    }

    #[test]
    fn test_message_list_selection() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("First"));
        list.push(ChatMessage::assistant("Second"));
        list.push(ChatMessage::user("Third"));

        assert_eq!(list.selected(), None);

        list.select_next();
        assert_eq!(list.selected(), Some(0));

        list.select_next();
        assert_eq!(list.selected(), Some(1));

        list.select_previous();
        assert_eq!(list.selected(), Some(0));

        list.select_last();
        assert_eq!(list.selected(), Some(2));

        list.select_first();
        assert_eq!(list.selected(), Some(0));

        list.clear_selection();
        assert_eq!(list.selected(), None);
    }

    #[test]
    fn test_message_list_selection_bounds() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Only message"));

        // Select next should not go past the last message
        list.select_next();
        assert_eq!(list.selected(), Some(0));
        list.select_next();
        assert_eq!(list.selected(), Some(0)); // Still 0, can't go further

        // Select previous should not go below 0
        list.select_previous();
        assert_eq!(list.selected(), Some(0)); // Still 0
    }

    #[test]
    fn test_empty_list_selection() {
        let mut list = MessageList::new();

        list.select_next();
        assert_eq!(list.selected(), None);

        list.select_previous();
        assert_eq!(list.selected(), None);

        list.select_first();
        assert_eq!(list.selected(), None);

        list.select_last();
        assert_eq!(list.selected(), None);
    }

    #[test]
    fn test_get_selected_text_basic() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Hello world"));
        list.push(ChatMessage::assistant("Hi there!"));

        // No selection initially
        assert_eq!(list.get_selected_text(80), "");

        // Start selection at position (0, 0) - header line
        list.start_selection();
        list.selection.anchor = TextPosition::new(0, 0);
        list.selection.cursor = TextPosition::new(0, 5);

        // Should have some text selected
        let selected = list.get_selected_text(80);
        assert!(!selected.is_empty(), "Should have selected text");
    }

    #[test]
    fn test_get_selected_text_multiline() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Line one\nLine two\nLine three"));

        // Select across multiple lines
        list.start_selection();
        list.selection.anchor = TextPosition::new(1, 0); // Start of content
        list.selection.cursor = TextPosition::new(2, 5);

        let selected = list.get_selected_text(80);
        // Should contain text from selected lines
        assert!(!selected.is_empty(), "Should have multiline selected text");
    }

    #[test]
    fn test_build_rendered_text_lines() {
        let mut list = MessageList::new();
        list.push(ChatMessage::user("Test content"));

        let lines = list.build_rendered_text_lines(80);

        // Should have at least header + content + spacing
        assert!(
            lines.len() >= 3,
            "Should have header, content, and spacing lines"
        );

        // Header should contain role icon and name
        assert!(lines[0].contains("👤"), "Header should contain user icon");
        assert!(lines[0].contains("You"), "Header should contain username");
    }

    #[test]
    fn test_render_tool_inline_truncates_long_commands() {
        // Test that very long commands are truncated to fit within width
        let long_command = format!(
            "git ls-remote --exit-code --heads https://github.com/{}",
            "x".repeat(200)
        );
        let tool_info = ToolCallInfo::new(
            "safe_shell",
            serde_json::json!({"command": long_command}),
            "success",
        );

        // Render at a narrow width (80 chars)
        let line = render_tool_inline(&tool_info, 80, false);

        // The total character count should be less than 80
        let total_chars: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert!(
            total_chars <= 85,
            "Line should fit within width (got {} chars)",
            total_chars
        );
    }

    #[test]
    fn test_render_tool_inline_shows_full_short_commands() {
        // Short commands should be displayed fully
        let tool_info = ToolCallInfo::new(
            "safe_shell",
            serde_json::json!({"command": "ls"}),
            "file1\nfile2",
        );

        let line = render_tool_inline(&tool_info, 120, false);

        // The line should contain "$ ls"
        let full_text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            full_text.contains("$ ls"),
            "Short command should be shown fully"
        );
    }

    #[test]
    fn test_render_tool_inline_error_truncates() {
        // Test error case also truncates properly
        let long_error = "x".repeat(200);
        let tool_info = ToolCallInfo::with_error(
            "safe_shell",
            serde_json::json!({"command": "git ls-remote --exit-code --heads https://github.com/test"}),
            long_error.clone(),
        );

        let line = render_tool_inline(&tool_info, 80, false);

        // The total character count should be reasonable
        let total_chars: usize = line.spans.iter().map(|s| s.content.chars().count()).sum();
        assert!(
            total_chars <= 90,
            "Error line should fit within width (got {} chars)",
            total_chars
        );
    }
}

/// Property-based tests for message list navigation
///
/// **Property 1: Message List Navigation Bounds**
/// **Validates: Requirements 1.3, 1.4**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a random role
    fn arb_role() -> impl Strategy<Value = Role> {
        prop_oneof![
            Just(Role::User),
            Just(Role::Assistant),
            Just(Role::System),
            Just(Role::Tool),
        ]
    }

    /// Generate a random chat message
    fn arb_message() -> impl Strategy<Value = ChatMessage> {
        (arb_role(), "[a-zA-Z0-9 ]{1,100}")
            .prop_map(|(role, content)| ChatMessage::new(role, content))
    }

    /// Generate a list of messages
    fn arb_message_list(max_size: usize) -> impl Strategy<Value = Vec<ChatMessage>> {
        prop::collection::vec(arb_message(), 0..=max_size)
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 1: Message List Navigation Bounds**
        /// **Validates: Requirements 1.3, 1.4**
        ///
        /// For any message list with N messages (N >= 0), vim-style navigation commands
        /// (j, k, gg, G, Ctrl-d, Ctrl-u) SHALL keep the cursor position within valid bounds
        /// [0, max(0, N-1)].
        #[test]
        fn prop_navigation_bounds(
            messages in arb_message_list(50),
            nav_commands in prop::collection::vec(0u8..6, 0..100),
            visible_height in 5u16..50u16,
            width in 40u16..200u16,
        ) {
            let mut list = MessageList::with_messages(messages.clone());
            list.set_visible_height(visible_height);

            let n = messages.len();

            // Execute random navigation commands
            for cmd in nav_commands {
                match cmd {
                    0 => list.scroll_down(width),      // j-like
                    1 => list.scroll_up(),             // k-like
                    2 => list.scroll_to_top(),         // gg
                    3 => list.scroll_to_bottom(width), // G
                    4 => list.scroll_half_page_down(width), // Ctrl-d
                    _ => list.scroll_half_page_up(),   // Ctrl-u
                }

                // Verify scroll offset is within bounds
                let max_scroll = if n == 0 {
                    0
                } else {
                    list.total_lines(width).saturating_sub(visible_height as usize)
                };
                prop_assert!(list.scroll_offset() <= max_scroll,
                    "Scroll offset {} exceeds max {}", list.scroll_offset(), max_scroll);
            }
        }

        /// **Feature: terminal-tui-chat, Property 1: Message List Navigation Bounds**
        /// **Validates: Requirements 1.3, 1.4**
        ///
        /// For any message list with N messages, selection navigation (select_next,
        /// select_previous, select_first, select_last) SHALL keep the selected index
        /// within valid bounds [0, N-1] when N > 0, or None when N == 0.
        #[test]
        fn prop_selection_bounds(
            messages in arb_message_list(50),
            selection_commands in prop::collection::vec(0u8..4, 0..100),
        ) {
            let mut list = MessageList::with_messages(messages.clone());
            let n = messages.len();

            // Execute random selection commands
            for cmd in selection_commands {
                match cmd {
                    0 => list.select_next(),
                    1 => list.select_previous(),
                    2 => list.select_first(),
                    _ => list.select_last(),
                }

                // Verify selection is within bounds
                if n == 0 {
                    prop_assert!(list.selected().is_none(),
                        "Selection should be None for empty list");
                } else if let Some(idx) = list.selected() {
                    prop_assert!(idx < n,
                        "Selection index {} exceeds list size {}", idx, n);
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 1: Message List Navigation Bounds**
        /// **Validates: Requirements 1.3, 1.4**
        ///
        /// For any message list, scroll_to_top followed by scroll_to_bottom and back
        /// should result in consistent state (idempotent at boundaries).
        #[test]
        fn prop_scroll_boundary_idempotent(
            messages in arb_message_list(50),
            visible_height in 5u16..50u16,
            width in 40u16..200u16,
        ) {
            let mut list = MessageList::with_messages(messages);
            list.set_visible_height(visible_height);

            // Scroll to top multiple times
            list.scroll_to_top();
            let top_offset = list.scroll_offset();
            list.scroll_to_top();
            prop_assert_eq!(list.scroll_offset(), top_offset,
                "scroll_to_top should be idempotent");
            prop_assert_eq!(top_offset, 0, "Top offset should be 0");

            // Scroll to bottom multiple times
            list.scroll_to_bottom(width);
            let bottom_offset = list.scroll_offset();
            list.scroll_to_bottom(width);
            prop_assert_eq!(list.scroll_offset(), bottom_offset,
                "scroll_to_bottom should be idempotent");
        }

        /// **Feature: terminal-tui-chat, Property 1: Message List Navigation Bounds**
        /// **Validates: Requirements 1.3, 1.4**
        ///
        /// For any message list, select_first and select_last should be idempotent
        /// and produce consistent results.
        #[test]
        fn prop_selection_boundary_idempotent(
            messages in arb_message_list(50),
        ) {
            let mut list = MessageList::with_messages(messages.clone());
            let n = messages.len();

            if n > 0 {
                // select_first should be idempotent
                list.select_first();
                let first_selection = list.selected();
                list.select_first();
                prop_assert_eq!(list.selected(), first_selection,
                    "select_first should be idempotent");
                prop_assert_eq!(first_selection, Some(0),
                    "First selection should be index 0");

                // select_last should be idempotent
                list.select_last();
                let last_selection = list.selected();
                list.select_last();
                prop_assert_eq!(list.selected(), last_selection,
                    "select_last should be idempotent");
                prop_assert_eq!(last_selection, Some(n - 1),
                    "Last selection should be index N-1");
            }
        }

        /// **Feature: terminal-tui-chat, Property 1: Message List Navigation Bounds**
        /// **Validates: Requirements 1.3, 1.4**
        ///
        /// For any non-empty message list, navigating from first to last using
        /// select_next should visit all indices in order.
        #[test]
        fn prop_selection_traversal(
            messages in arb_message_list(20).prop_filter("non-empty", |m| !m.is_empty()),
        ) {
            let mut list = MessageList::with_messages(messages.clone());
            let n = messages.len();

            // Start at first
            list.select_first();
            prop_assert_eq!(list.selected(), Some(0));

            // Navigate through all messages
            for expected_idx in 1..n {
                list.select_next();
                prop_assert_eq!(list.selected(), Some(expected_idx),
                    "Expected selection at index {}", expected_idx);
            }

            // One more select_next should stay at last
            list.select_next();
            prop_assert_eq!(list.selected(), Some(n - 1),
                "Selection should stay at last index");
        }

        /// **Feature: tui-llm-integration, Property 2: Display Name Formatting**
        /// **Validates: Requirements 2.2, 2.3**
        ///
        /// For any message displayed in the TUI, user messages SHALL show the detected
        /// system username (or "You" as fallback), and assistant messages SHALL show "Tark".
        /// Icons SHALL be preserved: 👤 for user, 🤖 for assistant.
        #[test]
        fn prop_display_name_formatting(
            messages in arb_message_list(20).prop_filter("non-empty", |m| !m.is_empty()),
            // Use usernames that won't be substrings of "Tark" to avoid false positives
            username in "[a-zA-Z][a-zA-Z0-9_]{3,15}".prop_filter("not-in-tark", |u| !u.to_lowercase().contains("tark") && !"tark".contains(&u.to_lowercase())),
        ) {
            // Test that render_message_with_blocks produces correct display names
            let block_state = CollapsibleBlockState::new();

            for message in &messages {
                let result = render_message_with_blocks(message, false, 80, &block_state, &username, true);

                // The first line should be the header with icon and name
                prop_assert!(!result.lines.is_empty(), "Message should produce at least one line");

                // Convert the first line to a string for checking
                let header_line = &result.lines[0];
                let header_text: String = header_line.spans.iter()
                    .map(|span| span.content.as_ref())
                    .collect();

                // Verify icon is present based on role (Requirements 2.4)
                match message.role {
                    Role::User => {
                        prop_assert!(header_text.contains("👤"),
                            "User message should have 👤 icon, got: {}", header_text);
                        // Verify username is displayed (Requirements 2.2)
                        prop_assert!(header_text.contains(&username),
                            "User message should show username '{}', got: {}", username, header_text);
                        // Verify "Tark" is NOT displayed for user messages
                        prop_assert!(!header_text.contains("Tark"),
                            "User message should not show 'Tark', got: {}", header_text);
                    }
                    Role::Assistant => {
                        prop_assert!(header_text.contains("🤖"),
                            "Assistant message should have 🤖 icon, got: {}", header_text);
                        // Verify "Tark" is displayed (Requirements 2.3)
                        prop_assert!(header_text.contains("Tark"),
                            "Assistant message should show 'Tark', got: {}", header_text);
                        // Verify username is NOT displayed for assistant messages
                        // (username is filtered to not be a substring of "Tark")
                        prop_assert!(!header_text.contains(&username),
                            "Assistant message should not show username '{}', got: {}", username, header_text);
                    }
                    Role::System => {
                        prop_assert!(header_text.contains("⚙️"),
                            "System message should have ⚙️ icon, got: {}", header_text);
                        prop_assert!(header_text.contains("System"),
                            "System message should show 'System', got: {}", header_text);
                    }
                    Role::Tool => {
                        prop_assert!(header_text.contains("🔧"),
                            "Tool message should have 🔧 icon, got: {}", header_text);
                        prop_assert!(header_text.contains("Tool"),
                            "Tool message should show 'Tool', got: {}", header_text);
                    }
                }
            }
        }

        /// **Feature: tui-llm-integration, Property 2: Display Name Formatting**
        /// **Validates: Requirements 2.5**
        ///
        /// When username detection fails (empty or whitespace-only username),
        /// the TUI SHALL fall back to "You" for user messages.
        #[test]
        fn prop_display_name_fallback(
            messages in arb_message_list(10).prop_filter("has-user", |m| m.iter().any(|msg| msg.role == Role::User)),
        ) {
            let block_state = CollapsibleBlockState::new();

            // Test with fallback username "You"
            let fallback_username = "You";

            for message in messages.iter().filter(|m| m.role == Role::User) {
                let result = render_message_with_blocks(message, false, 80, &block_state, fallback_username, true);

                prop_assert!(!result.lines.is_empty(), "Message should produce at least one line");

                let header_line = &result.lines[0];
                let header_text: String = header_line.spans.iter()
                    .map(|span| span.content.as_ref())
                    .collect();

                // Verify fallback "You" is displayed (Requirements 2.5)
                prop_assert!(header_text.contains("You"),
                    "User message with fallback should show 'You', got: {}", header_text);
                prop_assert!(header_text.contains("👤"),
                    "User message should have 👤 icon, got: {}", header_text);
            }
        }
    }

    // ============================================================================
    // Markdown Rendering Tests
    // ============================================================================

    #[test]
    fn test_markdown_headers() {
        // H1 header
        let h1 = format_header("# Main Title".to_string());
        assert!(h1.is_some());
        let h1_text: String = h1
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(h1_text.contains("Main Title"));

        // H2 header
        let h2 = format_header("## Section".to_string());
        assert!(h2.is_some());

        // H3 header
        let h3 = format_header("### Subsection".to_string());
        assert!(h3.is_some());

        // Not a header
        let not_header = format_header("Normal text".to_string());
        assert!(not_header.is_none());

        // Empty hash
        let empty = format_header("#".to_string());
        assert!(empty.is_none());
    }

    #[test]
    fn test_markdown_list_items() {
        let base_style = Style::default();

        // Dash list
        let dash = format_list_item("- Item one".to_string(), base_style);
        assert!(dash.is_some());
        let dash_text: String = dash
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(dash_text.contains("•"));
        assert!(dash_text.contains("Item one"));

        // Asterisk list
        let asterisk = format_list_item("* Item two".to_string(), base_style);
        assert!(asterisk.is_some());

        // Not a list
        let not_list = format_list_item("Normal text".to_string(), base_style);
        assert!(not_list.is_none());

        // Indented list
        let indented = format_list_item("  - Nested".to_string(), base_style);
        assert!(indented.is_some());
    }

    #[test]
    fn test_markdown_numbered_list() {
        let base_style = Style::default();

        // Single digit
        let num1 = format_numbered_list("1. First item".to_string(), base_style);
        assert!(num1.is_some());
        let num1_text: String = num1
            .unwrap()
            .spans
            .iter()
            .map(|s| s.content.as_ref())
            .collect();
        assert!(num1_text.contains("1."));
        assert!(num1_text.contains("First item"));

        // Double digit
        let num10 = format_numbered_list("10. Tenth item".to_string(), base_style);
        assert!(num10.is_some());

        // Not a numbered list
        let not_num = format_numbered_list("Not numbered".to_string(), base_style);
        assert!(not_num.is_none());

        // Missing space after dot
        let no_space = format_numbered_list("1.No space".to_string(), base_style);
        assert!(no_space.is_none());
    }

    #[test]
    fn test_markdown_inline_bold() {
        let base_style = Style::default();

        let line = format_inline_owned("This is **bold** text".to_string(), base_style);
        assert!(!line.spans.is_empty());

        // Should have at least 3 spans: before, bold, after
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("bold"));
        assert!(text.contains("This is"));
        assert!(text.contains("text"));
    }

    #[test]
    fn test_markdown_inline_italic() {
        let base_style = Style::default();

        // Underscore italic
        let line = format_inline_owned("This is _italic_ text".to_string(), base_style);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("italic"));
    }

    #[test]
    fn test_markdown_inline_code() {
        let base_style = Style::default();

        let line = format_inline_owned("Run `cargo build` command".to_string(), base_style);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("cargo build"));
    }

    #[test]
    fn test_markdown_code_block() {
        let base_style = Style::default();

        let content = "```rust\nfn main() {\n    println!(\"Hello\");\n}\n```".to_string();
        let lines = format_content_owned(content, base_style);

        // Should have language header + code lines
        assert!(lines.len() >= 3);

        // Check that code is rendered
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();
        assert!(all_text.contains("rust") || all_text.contains("fn main"));
    }

    #[test]
    fn test_markdown_blockquote() {
        let base_style = Style::default();

        let content = "> This is a quote".to_string();
        let lines = format_content_owned(content, base_style);

        assert!(!lines.is_empty());
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("This is a quote"));
        assert!(text.contains("│")); // Quote border
    }

    #[test]
    fn test_markdown_horizontal_rule() {
        let base_style = Style::default();

        for rule in &["---", "***", "___"] {
            let lines = format_content_owned(rule.to_string(), base_style);
            assert!(!lines.is_empty());
            let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
            assert!(text.contains("─")); // Horizontal line character
        }
    }

    #[test]
    fn test_markdown_mixed_content() {
        let base_style = Style::default();

        let content = r#"# Header

This is **bold** and `code`.

- List item 1
- List item 2

> A quote

```
code block
```
"#
        .to_string();

        let lines = format_content_owned(content, base_style);

        // Should render multiple lines
        assert!(lines.len() > 5);

        // Collect all text
        let all_text: String = lines
            .iter()
            .flat_map(|l| l.spans.iter())
            .map(|s| s.content.as_ref())
            .collect();

        // Verify various elements are present
        assert!(all_text.contains("Header"));
        assert!(all_text.contains("bold"));
        assert!(all_text.contains("List item"));
        assert!(all_text.contains("quote"));
    }

    #[test]
    fn test_focus_next_block_empty() {
        let mut list = MessageList::new();
        // No blocks registered, should return false
        assert!(!list.focus_next_block());
    }

    #[test]
    fn test_focus_prev_block_empty() {
        let mut list = MessageList::new();
        // No blocks registered, should return false
        assert!(!list.focus_prev_block());
    }

    #[test]
    fn test_focus_block_with_targets() {
        let mut list = MessageList::new();

        // Register some block targets
        list.add_block_click_target(5, "block-1".to_string(), BlockType::Tool);
        list.add_block_click_target(10, "block-2".to_string(), BlockType::Thinking);
        list.add_block_click_target(15, "block-3".to_string(), BlockType::Tool);

        // Focus next should work and focus first block
        assert!(list.focus_next_block());
        assert_eq!(list.focused_block(), Some("block-1"));

        // Block should be expanded
        assert!(list.block_state().is_expanded("block-1", BlockType::Tool));

        // Focus next again should move to second block
        assert!(list.focus_next_block());
        assert_eq!(list.focused_block(), Some("block-2"));

        // Focus prev should go back to first
        assert!(list.focus_prev_block());
        assert_eq!(list.focused_block(), Some("block-1"));

        // Clear focus
        list.clear_block_focus();
        assert!(list.focused_block().is_none());
    }

    #[test]
    fn test_focus_block_wrap_around() {
        let mut list = MessageList::new();

        // Register two blocks
        list.add_block_click_target(5, "block-1".to_string(), BlockType::Tool);
        list.add_block_click_target(10, "block-2".to_string(), BlockType::Tool);

        // Focus first
        assert!(list.focus_next_block());
        assert_eq!(list.focused_block(), Some("block-1"));

        // Focus second
        assert!(list.focus_next_block());
        assert_eq!(list.focused_block(), Some("block-2"));

        // Focus next should wrap to first
        assert!(list.focus_next_block());
        assert_eq!(list.focused_block(), Some("block-1"));

        // Focus prev should wrap to last
        list.clear_block_focus();
        assert!(list.focus_prev_block());
        assert_eq!(list.focused_block(), Some("block-2"));
    }
}
