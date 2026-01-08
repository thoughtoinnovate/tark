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
    /// Message content
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
    /// Detailed tool call info for collapsible block rendering
    pub tool_call_info: Vec<ToolCallInfo>,
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
    pub fn line_count(&self, width: u16, block_state: &CollapsibleBlockState) -> usize {
        if width == 0 {
            return 1;
        }

        // Use the actual rendering function to get accurate line count
        // This ensures line_count always matches what render_message_with_blocks produces
        let dummy_username = "User";
        let rendered_lines = render_message_with_blocks(
            self,
            false, // is_selected doesn't affect line count
            width,
            block_state,
            dummy_username,
        );

        rendered_lines.len()
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

/// Message list widget with scroll state
#[derive(Debug, Default)]
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

    /// Calculate total content height in lines
    fn total_lines(&self, width: u16) -> usize {
        self.messages
            .iter()
            .map(|m| m.line_count(width, &self.block_state))
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
    pub fn get_selected_text(&self, _width: u16) -> String {
        if !self.selection.active {
            return String::new();
        }

        // Build all lines and extract selected portion
        let mut all_text = String::new();
        for message in &self.messages {
            if !all_text.is_empty() {
                all_text.push('\n');
            }
            all_text.push_str(&message.content);
        }

        // This is a simplified implementation
        // In practice, we'd need to map selection positions to actual content
        let lines: Vec<&str> = all_text.lines().collect();
        let start = self.selection.start_pos();
        let end = self.selection.end_pos();

        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i < start.line || i > end.line {
                continue;
            }

            let line_chars: Vec<char> = line.chars().collect();
            let start_col = if i == start.line { start.col } else { 0 };
            let end_col = if i == end.line {
                end.col.min(line_chars.len())
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
            }
        }

        result
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
    _width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Determine colors based on block type and error state (Requirements 7.2)
    let (header_color, border_color) = if block.has_error {
        // Error blocks use red styling
        (Color::Red, Color::Red)
    } else {
        match block.block_type() {
            BlockType::Thinking => (Color::Cyan, Color::DarkGray),
            BlockType::Tool => (Color::Magenta, Color::DarkGray),
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
    let header_text = match block.block_type() {
        BlockType::Thinking => format!("╭── {} {} {} ", indicator, icon, block.header()),
        BlockType::Tool => {
            if block.has_error {
                format!(
                    "╭── {} {} Tool: {} [FAILED] ",
                    indicator,
                    icon,
                    block.header()
                )
            } else {
                format!("╭── {} {} Tool: {} ", indicator, icon, block.header())
            }
        }
    };

    // Pad header to create a box effect
    lines.push(Line::from(Span::styled(header_text, header_style)));

    // Render content if expanded
    if is_expanded {
        let border_style = Style::default().fg(border_color);

        for content_line in block.content() {
            // Use red for error content lines that start with ✗
            let content_style = if block.has_error && content_line.starts_with('✗') {
                Style::default().fg(Color::Red)
            } else {
                Style::default().fg(Color::Gray)
            };

            // Add border and content
            let line = Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(content_line.clone(), content_style),
            ]);
            lines.push(line);
        }

        // Closing border
        lines.push(Line::from(Span::styled(
            "╰──────────────────────────────────────────────────",
            border_style,
        )));
    }

    lines
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

    // Render thinking_content as a collapsible block if present (Requirements 9.1, 9.3)
    if message.role == Role::Assistant && !message.thinking_content.is_empty() {
        let thinking_block_id = format!("{}-thinking-stream", message.id);
        let thinking_content: Vec<String> = message
            .thinking_content
            .lines()
            .map(|s| s.to_string())
            .collect();
        let thinking_block =
            CollapsibleBlock::thinking(thinking_block_id.clone(), thinking_content);
        let is_expanded = block_state.is_expanded(&thinking_block_id, BlockType::Thinking);
        let block_lines = render_collapsible_block(&thinking_block, is_expanded, width);
        lines.extend(block_lines);
    }

    // For assistant messages, parse and render with collapsible blocks
    if message.role == Role::Assistant && !message.tool_call_info.is_empty() {
        // Parse the message content
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
                    let is_expanded = block_state.is_expanded(block.id(), block.block_type());
                    let block_lines = render_collapsible_block(block, is_expanded, width);
                    lines.extend(block_lines);
                }
            }
        }
    } else if message.role == Role::Assistant {
        // Check for thinking blocks in content even without tool calls
        let message_id = message.id.to_string();
        let parsed = ParsedMessageContent::parse(&message.content, &[], &message_id);

        if parsed.has_thinking() {
            // Render with collapsible blocks
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
                        let is_expanded = block_state.is_expanded(block.id(), block.block_type());
                        let block_lines = render_collapsible_block(block, is_expanded, width);
                        lines.extend(block_lines);
                    }
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

    lines
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
        if let Some(list_line) = format_list_item(line.to_string(), base_style) {
            lines.push(list_line);
            continue;
        }

        // Numbered list (1. 2. etc)
        if let Some(num_line) = format_numbered_list(line.to_string(), base_style) {
            lines.push(num_line);
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

/// Format unordered list item (- or * at start)
fn format_list_item(line: String, base_style: Style) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let indent_str = "  ".repeat(indent / 2);

    if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
        let text = trimmed[2..].to_string();
        let formatted = format_inline_owned(text, base_style);
        let mut spans = vec![
            Span::raw(indent_str),
            Span::styled("  • ", Style::default().fg(Color::Yellow)),
        ];
        spans.extend(formatted.spans);
        return Some(Line::from(spans));
    }
    None
}

/// Format numbered list item (1. 2. etc)
fn format_numbered_list(line: String, base_style: Style) -> Option<Line<'static>> {
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();
    let indent_str = "  ".repeat(indent / 2);

    // Check for pattern like "1. " "2. " "10. " etc
    if let Some(dot_pos) = trimmed.find(". ") {
        let num_part = &trimmed[..dot_pos];
        if num_part.chars().all(|c| c.is_ascii_digit()) {
            let text = trimmed[dot_pos + 2..].to_string();
            let formatted = format_inline_owned(text, base_style);
            let mut spans = vec![
                Span::raw(indent_str),
                Span::styled(
                    format!("  {}. ", num_part),
                    Style::default().fg(Color::Yellow),
                ),
            ];
            spans.extend(formatted.spans);
            return Some(Line::from(spans));
        }
    }
    None
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

        // Collect all lines from all messages, using collapsible block rendering
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        let block_state = &self.message_list.block_state;
        let username = &self.username;
        for (idx, message) in self.message_list.messages.iter().enumerate() {
            let is_selected = self.message_list.selected == Some(idx);
            // Use the new render function that supports collapsible blocks
            // Pass username for display name formatting (Requirements 2.2, 2.3)
            let message_lines = render_message_with_blocks(
                message,
                is_selected,
                inner.width,
                block_state,
                username,
            );
            all_lines.extend(message_lines);
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
                let lines = render_message_with_blocks(message, false, 80, &block_state, &username);

                // The first line should be the header with icon and name
                prop_assert!(!lines.is_empty(), "Message should produce at least one line");

                // Convert the first line to a string for checking
                let header_line = &lines[0];
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
                let lines = render_message_with_blocks(message, false, 80, &block_state, fallback_username);

                prop_assert!(!lines.is_empty(), "Message should produce at least one line");

                let header_line = &lines[0];
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
}
