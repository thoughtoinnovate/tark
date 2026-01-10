//! Input widget with cursor management and command history
//!
//! Provides a text input area with support for multi-line input,
//! command history navigation, cursor movement, and automatic text wrapping.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

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
    /// Vertical scroll offset (in wrapped lines)
    scroll_offset: usize,
    /// Last known width for wrapping (used to detect resize)
    last_width: u16,
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
            scroll_offset: 0,
            last_width: 0,
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
        self.scroll_offset = 0;
        self.history_index = None;
    }

    /// Delete from cursor to end of line (Ctrl+K)
    pub fn delete_to_end(&mut self) {
        if self.cursor < self.content.len() {
            // Find the end of the current line
            let after_cursor = &self.content[self.cursor..];
            let line_end = after_cursor
                .find('\n')
                .map(|i| self.cursor + i)
                .unwrap_or(self.content.len());

            // Delete from cursor to end of line (but not the newline itself)
            self.content.drain(self.cursor..line_end);
            self.history_index = None;
        }
    }

    /// Delete from cursor to start of line (Ctrl+U style - delete backwards)
    pub fn delete_to_start(&mut self) {
        if self.cursor > 0 {
            // Find the start of the current line
            let before_cursor = &self.content[..self.cursor];
            let line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);

            // Delete from line start to cursor
            self.content.drain(line_start..self.cursor);
            self.cursor = line_start;
            self.history_index = None;
        }
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

    /// Move cursor up one line (for multi-line input)
    pub fn move_cursor_up(&mut self) {
        if self.content.is_empty() {
            return;
        }

        // Find the current line start and the previous line
        let before_cursor = &self.content[..self.cursor];

        // Find current line start
        let current_line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);

        if current_line_start == 0 {
            // Already on first line, can't go up
            return;
        }

        // Find previous line start
        let prev_line_end = current_line_start - 1; // Position of the \n
        let prev_line_start = self.content[..prev_line_end]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Calculate column position on current line
        let current_col = self.cursor - current_line_start;

        // Calculate the previous line length
        let prev_line_len = prev_line_end - prev_line_start;

        // Move to same column on previous line, or end of line if shorter
        self.cursor = prev_line_start + current_col.min(prev_line_len);
    }

    /// Move cursor down one line (for multi-line input)
    pub fn move_cursor_down(&mut self) {
        if self.content.is_empty() {
            return;
        }

        // Find the current line start
        let before_cursor = &self.content[..self.cursor];
        let current_line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);

        // Find the next line start
        let after_cursor = &self.content[self.cursor..];
        let next_newline = after_cursor.find('\n');

        if next_newline.is_none() {
            // Already on last line, can't go down
            return;
        }

        let next_line_start = self.cursor + next_newline.unwrap() + 1;

        // Find next line end
        let next_line_end = self.content[next_line_start..]
            .find('\n')
            .map(|i| next_line_start + i)
            .unwrap_or(self.content.len());

        // Calculate column position on current line
        let current_col = self.cursor - current_line_start;

        // Calculate next line length
        let next_line_len = next_line_end - next_line_start;

        // Move to same column on next line, or end of line if shorter
        self.cursor = next_line_start + current_col.min(next_line_len);
    }

    /// Check if the input has multiple lines
    pub fn is_multiline(&self) -> bool {
        self.content.contains('\n')
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

    /// Set the cursor position directly
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.content.len());
    }

    /// Get the scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Set the scroll offset
    pub fn set_scroll_offset(&mut self, offset: usize) {
        self.scroll_offset = offset;
    }

    /// Update scroll offset to ensure cursor is visible
    /// Returns the new scroll offset
    pub fn ensure_cursor_visible(&mut self, width: u16, height: u16) -> usize {
        if width == 0 || height == 0 {
            return self.scroll_offset;
        }

        let width = width as usize;
        let height = height as usize;

        // Calculate which wrapped line the cursor is on
        let cursor_line = self.get_cursor_wrapped_line(width);

        // Adjust scroll to keep cursor visible
        if cursor_line < self.scroll_offset {
            self.scroll_offset = cursor_line;
        } else if cursor_line >= self.scroll_offset + height {
            self.scroll_offset = cursor_line.saturating_sub(height - 1);
        }

        self.scroll_offset
    }

    /// Get the wrapped line index where the cursor is located
    fn get_cursor_wrapped_line(&self, width: usize) -> usize {
        if width == 0 || self.content.is_empty() {
            return 0;
        }

        let mut line_idx = 0;
        let mut current_line_width = 0;
        let mut byte_pos = 0;

        for ch in self.content.chars() {
            if byte_pos >= self.cursor {
                break;
            }

            let ch_width = if ch == '\n' {
                // Newline always starts a new line
                line_idx += 1;
                current_line_width = 0;
                byte_pos += ch.len_utf8();
                continue;
            } else {
                unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1)
            };

            // Check if adding this char would exceed width
            if current_line_width + ch_width > width {
                line_idx += 1;
                current_line_width = ch_width;
            } else {
                current_line_width += ch_width;
            }

            byte_pos += ch.len_utf8();
        }

        line_idx
    }

    /// Get total number of wrapped lines for given width
    pub fn get_wrapped_line_count(&self, width: usize) -> usize {
        if width == 0 {
            return 1;
        }
        if self.content.is_empty() {
            return 1;
        }

        let mut line_count = 1;
        let mut current_line_width = 0;

        for ch in self.content.chars() {
            if ch == '\n' {
                line_count += 1;
                current_line_width = 0;
                continue;
            }

            let ch_width = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);

            if current_line_width + ch_width > width {
                line_count += 1;
                current_line_width = ch_width;
            } else {
                current_line_width += ch_width;
            }
        }

        line_count
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

/// A segment of text with style information for wrapped rendering
struct StyledSegment {
    text: String,
    style: Style,
    is_cursor: bool,
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

        let height = inner.height as usize;

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

        // First pass: determine if we need a scrollbar by checking total lines
        // Use full width initially to check line count
        let full_width = inner.width as usize;
        let test_segments = self.build_styled_segments(content, cursor_pos);
        let test_lines = self.wrap_segments_into_lines(&test_segments, full_width);
        let total_lines = test_lines.len();
        let needs_scrollbar = total_lines > height;

        // Reserve 2 chars for scrollbar if needed (space + scrollbar char)
        let text_width = if needs_scrollbar {
            full_width.saturating_sub(2)
        } else {
            full_width
        };

        // Build styled segments for the content
        let segments = self.build_styled_segments(content, cursor_pos);

        // Wrap segments into lines with adjusted width
        let wrapped_lines = self.wrap_segments_into_lines(&segments, text_width);
        let total_wrapped_lines = wrapped_lines.len();

        // Calculate scroll offset to keep cursor visible
        let cursor_line = self.find_cursor_line(&wrapped_lines);
        let scroll_offset = self.input.scroll_offset;

        // Adjust scroll to keep cursor visible
        let effective_scroll = if cursor_line < scroll_offset {
            cursor_line
        } else if cursor_line >= scroll_offset + height {
            cursor_line.saturating_sub(height - 1)
        } else {
            scroll_offset
        };

        // Generate scrollbar characters if needed
        let scrollbar_chars = if needs_scrollbar {
            Self::render_scrollbar(effective_scroll, height, total_wrapped_lines, height)
        } else {
            vec![]
        };

        // Render visible lines
        let visible_lines: Vec<_> = wrapped_lines
            .into_iter()
            .skip(effective_scroll)
            .take(height)
            .collect();

        let lines: Vec<Line> = visible_lines
            .into_iter()
            .enumerate()
            .map(|(i, line_segments)| {
                let mut spans: Vec<Span> = line_segments
                    .into_iter()
                    .map(|seg| Span::styled(seg.text, seg.style))
                    .collect();

                // Add scrollbar if needed
                if needs_scrollbar && i < scrollbar_chars.len() {
                    spans.push(Span::styled(
                        format!(" {}", scrollbar_chars[i]),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                Line::from(spans)
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

impl InputWidgetRenderer<'_> {
    /// Render a scrollbar for the input widget
    ///
    /// Returns the scrollbar characters to display on the right edge
    fn render_scrollbar(
        scroll_offset: usize,
        visible_lines: usize,
        total_lines: usize,
        available_height: usize,
    ) -> Vec<char> {
        if total_lines <= visible_lines || available_height == 0 {
            // No scrollbar needed
            return vec![' '; available_height];
        }

        let mut scrollbar = vec!['░'; available_height];

        // Calculate thumb position and size
        let thumb_size = std::cmp::max(1, (available_height * visible_lines) / total_lines);
        let max_scroll = total_lines.saturating_sub(visible_lines);
        let thumb_pos = if max_scroll > 0 {
            (scroll_offset * (available_height.saturating_sub(thumb_size))) / max_scroll
        } else {
            0
        };

        // Draw the thumb (solid block for thumb position, light shade for track)
        for item in scrollbar
            .iter_mut()
            .take((thumb_pos + thumb_size).min(available_height))
            .skip(thumb_pos)
        {
            *item = '█';
        }

        scrollbar
    }
}

impl<'a> InputWidgetRenderer<'a> {
    /// Build styled segments from content with cursor highlighting
    fn build_styled_segments(&self, content: &str, cursor_pos: usize) -> Vec<StyledSegment> {
        let mut segments = Vec::new();
        let (before, after) = content.split_at(cursor_pos.min(content.len()));

        // Text before cursor
        if !before.is_empty() {
            segments.push(StyledSegment {
                text: before.to_string(),
                style: self.style,
                is_cursor: false,
            });
        }

        // Cursor
        if self.input.focused {
            if after.is_empty() {
                // Cursor at end - show block cursor
                segments.push(StyledSegment {
                    text: " ".to_string(),
                    style: self.cursor_style,
                    is_cursor: true,
                });
            } else {
                // Cursor in middle - highlight character under cursor
                let cursor_char = after.chars().next().unwrap();
                segments.push(StyledSegment {
                    text: cursor_char.to_string(),
                    style: self.cursor_style,
                    is_cursor: true,
                });

                // Text after cursor (excluding cursor char)
                let after_cursor = &after[cursor_char.len_utf8()..];
                if !after_cursor.is_empty() {
                    segments.push(StyledSegment {
                        text: after_cursor.to_string(),
                        style: self.style,
                        is_cursor: false,
                    });
                }
            }
        } else {
            // Not focused - just show text
            if !after.is_empty() {
                segments.push(StyledSegment {
                    text: after.to_string(),
                    style: self.style,
                    is_cursor: false,
                });
            }
        }

        segments
    }

    /// Wrap styled segments into lines that fit within the given width
    fn wrap_segments_into_lines(
        &self,
        segments: &[StyledSegment],
        width: usize,
    ) -> Vec<Vec<StyledSegment>> {
        let mut lines: Vec<Vec<StyledSegment>> = vec![Vec::new()];
        let mut current_line_width = 0;

        for segment in segments {
            let mut remaining = segment.text.as_str();

            while !remaining.is_empty() {
                // Handle newline characters
                if let Some(newline_pos) = remaining.find('\n') {
                    let before_newline = &remaining[..newline_pos];

                    // Add text before newline to current line
                    if !before_newline.is_empty() {
                        self.add_text_to_lines(
                            &mut lines,
                            &mut current_line_width,
                            before_newline,
                            segment.style,
                            segment.is_cursor,
                            width,
                        );
                    }

                    // Start new line
                    lines.push(Vec::new());
                    current_line_width = 0;
                    remaining = &remaining[newline_pos + 1..];
                } else {
                    // No newline, wrap text as needed
                    self.add_text_to_lines(
                        &mut lines,
                        &mut current_line_width,
                        remaining,
                        segment.style,
                        segment.is_cursor,
                        width,
                    );
                    break;
                }
            }
        }

        // Ensure at least one line
        if lines.is_empty() {
            lines.push(Vec::new());
        }

        lines
    }

    /// Add text to lines, wrapping as needed
    fn add_text_to_lines(
        &self,
        lines: &mut Vec<Vec<StyledSegment>>,
        current_line_width: &mut usize,
        text: &str,
        style: Style,
        is_cursor: bool,
        width: usize,
    ) {
        use unicode_width::UnicodeWidthChar;

        let mut current_segment = String::new();

        for ch in text.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(1);

            // Check if we need to wrap
            if *current_line_width + ch_width > width && *current_line_width > 0 {
                // Flush current segment to current line
                if !current_segment.is_empty() {
                    lines.last_mut().unwrap().push(StyledSegment {
                        text: std::mem::take(&mut current_segment),
                        style,
                        is_cursor: false,
                    });
                }

                // Start new line
                lines.push(Vec::new());
                *current_line_width = 0;
            }

            current_segment.push(ch);
            *current_line_width += ch_width;
        }

        // Flush remaining text
        if !current_segment.is_empty() {
            lines.last_mut().unwrap().push(StyledSegment {
                text: current_segment,
                style,
                is_cursor,
            });
        }
    }

    /// Find which line contains the cursor
    fn find_cursor_line(&self, lines: &[Vec<StyledSegment>]) -> usize {
        for (line_idx, line) in lines.iter().enumerate() {
            for segment in line {
                if segment.is_cursor {
                    return line_idx;
                }
            }
        }
        // Cursor not found (shouldn't happen), return last line
        lines.len().saturating_sub(1)
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
