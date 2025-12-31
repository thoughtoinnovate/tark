//! Input widget with cursor management and command history
//!
//! Provides a text input area with support for multi-line input,
//! command history navigation, and cursor movement.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

/// Input widget state
#[derive(Debug, Default, Clone)]
pub struct InputWidget {
    /// Current input content
    content: String,
    /// Cursor position (byte offset)
    cursor: usize,
    /// Command history
    history: Vec<String>,
    /// Current position in history (None = editing new input)
    history_index: Option<usize>,
    /// Saved input when browsing history
    saved_input: String,
    /// Maximum history size
    max_history: usize,
    /// Whether the input is focused
    focused: bool,
    /// Placeholder text
    placeholder: String,
}

impl InputWidget {
    /// Create a new input widget
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
            history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            max_history: 100,
            focused: true,
            placeholder: "Type a message... (/help for commands)".to_string(),
        }
    }

    /// Set the placeholder text
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Set the maximum history size
    pub fn with_max_history(mut self, max: usize) -> Self {
        self.max_history = max;
        self
    }

    /// Set focus state
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
    }

    /// Check if focused
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Get the current content
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get the cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Check if the input is empty
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Get the number of lines in the content
    pub fn line_count(&self) -> usize {
        self.content.lines().count().max(1)
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.history_index = None;
    }

    /// Insert a string at the cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.content.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.history_index = None;
    }

    /// Delete the character before the cursor (backspace)
    pub fn delete_char_before(&mut self) {
        if self.cursor > 0 {
            // Find the previous character boundary
            let prev_boundary = self.content[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.content.remove(prev_boundary);
            self.cursor = prev_boundary;
            self.history_index = None;
        }
    }

    /// Delete the character at the cursor (delete key)
    pub fn delete_char_at(&mut self) {
        if self.cursor < self.content.len() {
            self.content.remove(self.cursor);
            self.history_index = None;
        }
    }

    /// Delete the word before the cursor (Ctrl+W)
    pub fn delete_word_before(&mut self) {
        if self.cursor == 0 {
            return;
        }

        // Find the start of the previous word
        let before_cursor = &self.content[..self.cursor];
        let trimmed = before_cursor.trim_end();
        let word_start = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);

        self.content.drain(word_start..self.cursor);
        self.cursor = word_start;
        self.history_index = None;
    }

    /// Clear the entire input
    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
        self.history_index = None;
    }

    /// Move cursor left by one character
    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            // Find the previous character boundary
            self.cursor = self.content[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right by one character
    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.content.len() {
            // Find the next character boundary
            self.cursor = self.content[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.content.len());
        }
    }

    /// Move cursor to the start of the line
    pub fn move_cursor_to_start(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to the end of the line
    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.content.len();
    }

    /// Move cursor left by one word (Ctrl+Left)
    pub fn move_cursor_word_left(&mut self) {
        if self.cursor == 0 {
            return;
        }

        let before_cursor = &self.content[..self.cursor];
        let trimmed = before_cursor.trim_end();
        self.cursor = trimmed
            .rfind(|c: char| c.is_whitespace())
            .map(|i| i + 1)
            .unwrap_or(0);
    }

    /// Move cursor right by one word (Ctrl+Right)
    pub fn move_cursor_word_right(&mut self) {
        if self.cursor >= self.content.len() {
            return;
        }

        let after_cursor = &self.content[self.cursor..];
        // Skip current word
        let skip_word = after_cursor
            .find(|c: char| c.is_whitespace())
            .unwrap_or(after_cursor.len());
        // Skip whitespace
        let skip_space = after_cursor[skip_word..]
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after_cursor.len() - skip_word);

        self.cursor += skip_word + skip_space;
    }

    /// Insert a newline (for multi-line input)
    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
    }

    /// Submit the current input and add to history
    pub fn submit(&mut self) -> String {
        let content = std::mem::take(&mut self.content);
        self.cursor = 0;
        self.history_index = None;
        self.saved_input.clear();

        // Add to history if non-empty and different from last entry
        if !content.trim().is_empty() && self.history.last() != Some(&content) {
            self.history.push(content.clone());
            // Trim history if too long
            while self.history.len() > self.max_history {
                self.history.remove(0);
            }
        }

        content
    }

    /// Navigate to the previous history entry (up arrow)
    pub fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Save current input and go to most recent history
                self.saved_input = std::mem::take(&mut self.content);
                self.history_index = Some(self.history.len() - 1);
                self.content = self.history[self.history.len() - 1].clone();
            }
            Some(0) => {
                // Already at oldest entry, do nothing
            }
            Some(idx) => {
                // Go to older entry
                self.history_index = Some(idx - 1);
                self.content = self.history[idx - 1].clone();
            }
        }
        self.cursor = self.content.len();
    }

    /// Navigate to the next history entry (down arrow)
    pub fn history_next(&mut self) {
        match self.history_index {
            None => {
                // Not in history mode, do nothing
            }
            Some(idx) if idx >= self.history.len() - 1 => {
                // At most recent entry, restore saved input
                self.history_index = None;
                self.content = std::mem::take(&mut self.saved_input);
            }
            Some(idx) => {
                // Go to newer entry
                self.history_index = Some(idx + 1);
                self.content = self.history[idx + 1].clone();
            }
        }
        self.cursor = self.content.len();
    }

    /// Get the history entries
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Check if currently browsing history
    pub fn is_browsing_history(&self) -> bool {
        self.history_index.is_some()
    }

    /// Set the content directly (useful for tab completion)
    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.content.len();
        self.history_index = None;
    }
}

/// Renderable input widget
pub struct InputWidgetRenderer<'a> {
    input: &'a InputWidget,
    block: Option<Block<'a>>,
    style: Style,
    cursor_style: Style,
}

impl<'a> InputWidgetRenderer<'a> {
    /// Create a new input widget renderer
    pub fn new(input: &'a InputWidget) -> Self {
        Self {
            input,
            block: None,
            style: Style::default(),
            cursor_style: Style::default().add_modifier(Modifier::REVERSED),
        }
    }

    /// Set the block for the widget
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }

    /// Set the style for the widget
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the cursor style
    pub fn cursor_style(mut self, style: Style) -> Self {
        self.cursor_style = style;
        self
    }
}

impl Widget for InputWidgetRenderer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate inner area
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if inner.width == 0 || inner.height == 0 {
            return;
        }

        // Render content or placeholder
        if self.input.content.is_empty() && !self.input.focused {
            let placeholder = Paragraph::new(self.input.placeholder.as_str())
                .style(Style::default().fg(Color::DarkGray));
            placeholder.render(inner, buf);
            return;
        }

        // Build the display with cursor
        let content = &self.input.content;
        let cursor_pos = self.input.cursor;

        if content.is_empty() {
            // Show cursor at start
            if self.input.focused {
                let cursor_span = Span::styled(" ", self.cursor_style);
                let line = Line::from(vec![cursor_span]);
                Paragraph::new(line).render(inner, buf);
            } else {
                let placeholder = Paragraph::new(self.input.placeholder.as_str())
                    .style(Style::default().fg(Color::DarkGray));
                placeholder.render(inner, buf);
            }
            return;
        }

        // Split content at cursor position
        let (before, after) = content.split_at(cursor_pos.min(content.len()));

        let mut spans = Vec::new();

        // Text before cursor
        if !before.is_empty() {
            spans.push(Span::styled(before.to_string(), self.style));
        }

        // Cursor
        if self.input.focused {
            if after.is_empty() {
                // Cursor at end - show block cursor
                spans.push(Span::styled(" ", self.cursor_style));
            } else {
                // Cursor in middle - highlight character under cursor
                let cursor_char = after.chars().next().unwrap();
                spans.push(Span::styled(cursor_char.to_string(), self.cursor_style));

                // Text after cursor (excluding cursor char)
                let after_cursor = &after[cursor_char.len_utf8()..];
                if !after_cursor.is_empty() {
                    spans.push(Span::styled(after_cursor.to_string(), self.style));
                }
            }
        } else {
            // Not focused - just show text
            if !after.is_empty() {
                spans.push(Span::styled(after.to_string(), self.style));
            }
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_widget_new() {
        let input = InputWidget::new();
        assert!(input.is_empty());
        assert_eq!(input.cursor(), 0);
        assert!(input.is_focused());
    }

    #[test]
    fn test_insert_char() {
        let mut input = InputWidget::new();
        input.insert_char('H');
        input.insert_char('i');
        assert_eq!(input.content(), "Hi");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn test_insert_str() {
        let mut input = InputWidget::new();
        input.insert_str("Hello");
        assert_eq!(input.content(), "Hello");
        assert_eq!(input.cursor(), 5);
    }

    #[test]
    fn test_delete_char_before() {
        let mut input = InputWidget::new();
        input.insert_str("Hello");
        input.delete_char_before();
        assert_eq!(input.content(), "Hell");
        assert_eq!(input.cursor(), 4);
    }

    #[test]
    fn test_delete_char_at() {
        let mut input = InputWidget::new();
        input.insert_str("Hello");
        input.move_cursor_to_start();
        input.delete_char_at();
        assert_eq!(input.content(), "ello");
    }

    #[test]
    fn test_cursor_movement() {
        let mut input = InputWidget::new();
        input.insert_str("Hello World");

        input.move_cursor_to_start();
        assert_eq!(input.cursor(), 0);

        input.move_cursor_to_end();
        assert_eq!(input.cursor(), 11);

        input.move_cursor_left();
        assert_eq!(input.cursor(), 10);

        input.move_cursor_right();
        assert_eq!(input.cursor(), 11);
    }

    #[test]
    fn test_word_movement() {
        let mut input = InputWidget::new();
        input.insert_str("Hello World Test");

        input.move_cursor_word_left();
        assert!(input.cursor() < 16); // Moved back

        input.move_cursor_to_start();
        input.move_cursor_word_right();
        assert!(input.cursor() > 0); // Moved forward
    }

    #[test]
    fn test_clear() {
        let mut input = InputWidget::new();
        input.insert_str("Hello");
        input.clear();
        assert!(input.is_empty());
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn test_submit() {
        let mut input = InputWidget::new();
        input.insert_str("Hello");
        let content = input.submit();
        assert_eq!(content, "Hello");
        assert!(input.is_empty());
        assert_eq!(input.history().len(), 1);
    }

    #[test]
    fn test_history_navigation() {
        let mut input = InputWidget::new();

        // Add some history
        input.insert_str("First");
        input.submit();
        input.insert_str("Second");
        input.submit();
        input.insert_str("Third");
        input.submit();

        assert_eq!(input.history().len(), 3);

        // Navigate back
        input.insert_str("Current");
        input.history_previous();
        assert_eq!(input.content(), "Third");

        input.history_previous();
        assert_eq!(input.content(), "Second");

        input.history_previous();
        assert_eq!(input.content(), "First");

        // Navigate forward
        input.history_next();
        assert_eq!(input.content(), "Second");

        input.history_next();
        assert_eq!(input.content(), "Third");

        input.history_next();
        assert_eq!(input.content(), "Current");
    }

    #[test]
    fn test_history_dedup() {
        let mut input = InputWidget::new();

        input.insert_str("Same");
        input.submit();
        input.insert_str("Same");
        input.submit();

        // Should not add duplicate
        assert_eq!(input.history().len(), 1);
    }

    #[test]
    fn test_delete_word_before() {
        let mut input = InputWidget::new();
        input.insert_str("Hello World");
        input.delete_word_before();
        assert_eq!(input.content(), "Hello ");
    }

    #[test]
    fn test_multiline() {
        let mut input = InputWidget::new();
        input.insert_str("Line 1");
        input.insert_newline();
        input.insert_str("Line 2");
        assert_eq!(input.line_count(), 2);
    }
}
