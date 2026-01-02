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
    pub fn line_count(&self, width: u16) -> usize {
        if width == 0 {
            return 1;
        }
        // Header line + content lines
        let content_width = width.saturating_sub(4) as usize; // Account for padding
        if content_width == 0 {
            return 1 + self.content.lines().count();
        }

        let mut lines = 1; // Header line
        for line in self.content.lines() {
            if line.is_empty() {
                lines += 1;
            } else {
                lines += line.len().div_ceil(content_width);
            }
        }
        // Add tool call lines
        lines += self.tool_calls.len();
        lines.max(1)
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
        self.messages.iter().map(|m| m.line_count(width) + 1).sum() // +1 for spacing
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

    // Content lines
    let content_style = if is_selected {
        Style::default().add_modifier(Modifier::REVERSED)
    } else {
        Style::default()
    };

    // Clone content to make it 'static
    let content = message.content.clone();
    let content_lines = format_content_owned(content, content_style);
    lines.extend(content_lines);

    // Tool calls (if any)
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

    // Ensure we don't exceed width (basic truncation)
    let _ = width; // Width is used for future wrapping implementation

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
                    let content_lines = format_content_owned(text.clone(), content_style);
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
                        let content_lines = format_content_owned(text.clone(), content_style);
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
            let content_lines = format_content_owned(message.content.clone(), content_style);
            lines.extend(content_lines);
        }
    } else {
        // Non-assistant messages: render content normally
        let content_style = if is_selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let content_lines = format_content_owned(message.content.clone(), content_style);
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

/// Format content with owned strings for 'static lifetime
fn format_content_owned(content: String, base_style: Style) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code_block = false;
    let mut code_block_lines: Vec<String> = Vec::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if in_code_block {
                for code_line in &code_block_lines {
                    lines.push(Line::from(Span::styled(
                        format!("  {}", code_line),
                        Style::default().fg(Color::Gray).bg(Color::DarkGray),
                    )));
                }
                code_block_lines.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
            }
            continue;
        }

        if in_code_block {
            code_block_lines.push(line.to_string());
            continue;
        }

        let formatted_line = format_inline_owned(line.to_string(), base_style);
        lines.push(formatted_line);
    }

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

/// Format inline markdown with owned strings
fn format_inline_owned(line: String, base_style: Style) -> Line<'static> {
    let mut spans = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_bold = false;
    let mut in_code = false;

    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') && !in_code => {
                chars.next();
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

/// Widget implementation for MessageList
pub struct MessageListWidget<'a> {
    message_list: &'a mut MessageList,
    block: Option<Block<'a>>,
    /// Username to display for user messages (Requirements 2.2, 2.5)
    username: String,
}

impl<'a> MessageListWidget<'a> {
    /// Create a new message list widget
    pub fn new(message_list: &'a mut MessageList) -> Self {
        Self {
            message_list,
            block: None,
            username: "You".to_string(), // Default fallback (Requirement 2.5)
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

        // Render scrollbar if needed
        let total_lines = self.message_list.total_lines(inner.width);
        if total_lines > inner.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(total_lines).position(scroll_offset);

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
}
