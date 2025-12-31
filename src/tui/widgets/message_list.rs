//! Message list widget for displaying chat history
//!
//! Provides a scrollable list of chat messages with role indicators
//! and markdown-like formatting support.

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
    pub fn name(&self) -> &'static str {
        match self {
            Role::User => "You",
            Role::Assistant => "Assistant",
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
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Whether this message is currently streaming
    pub is_streaming: bool,
    /// Tool calls associated with this message (for assistant messages)
    pub tool_calls: Vec<String>,
}

impl ChatMessage {
    /// Create a new chat message
    pub fn new(role: Role, content: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            role,
            content: content.into(),
            timestamp: Utc::now(),
            is_streaming: false,
            tool_calls: Vec::new(),
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
        }
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
fn render_message(message: &ChatMessage, is_selected: bool, width: u16) -> Vec<Line<'static>> {
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

    let timestamp = message.timestamp.format("%H:%M").to_string();
    let header = format!(
        "{} {} [{}]",
        message.role.icon(),
        message.role.name(),
        timestamp
    );
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
}

impl<'a> MessageListWidget<'a> {
    /// Create a new message list widget
    pub fn new(message_list: &'a mut MessageList) -> Self {
        Self {
            message_list,
            block: None,
        }
    }

    /// Set the block for the widget
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
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

        // Collect all lines from all messages
        let mut all_lines: Vec<Line<'static>> = Vec::new();
        for (idx, message) in self.message_list.messages.iter().enumerate() {
            let is_selected = self.message_list.selected == Some(idx);
            let message_lines = render_message(message, is_selected, inner.width);
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
        assert_eq!(Role::Assistant.name(), "Assistant");
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
    }
}
