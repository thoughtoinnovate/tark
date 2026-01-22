//! Input Widget
//!
//! Text input area with cursor, multi-line support, and text wrapping
//! Feature: 04_input_area.feature

#![allow(clippy::unused_enumerate_index)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::time::{Duration, Instant};

use crate::tui_new::theme::Theme;

/// Cursor blink interval
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// Global cursor blink state (shared across renders)
static mut LAST_BLINK: Option<Instant> = None;
static mut CURSOR_VISIBLE: bool = true;

/// Get current cursor visibility state (blinks every 530ms)
fn get_cursor_visible() -> bool {
    unsafe {
        let now = Instant::now();
        if let Some(last) = LAST_BLINK {
            if now.duration_since(last) >= CURSOR_BLINK_INTERVAL {
                CURSOR_VISIBLE = !CURSOR_VISIBLE;
                LAST_BLINK = Some(now);
            }
        } else {
            LAST_BLINK = Some(now);
            CURSOR_VISIBLE = true;
        }
        CURSOR_VISIBLE
    }
}

/// Input widget for user text entry
pub struct InputWidget<'a> {
    /// Current input text
    content: &'a str,
    /// Cursor position (byte offset)
    cursor: usize,
    /// Whether the input is focused
    focused: bool,
    /// Theme for styling
    theme: &'a Theme,
    /// Placeholder text
    placeholder: &'a str,
    /// Scroll offset for long multi-line inputs
    scroll_offset: usize,
    /// Context files (for displaying badges with cross button)
    context_files: Vec<String>,
    /// Attachments to display above input
    attachments: Vec<AttachmentBadge>,
    /// Selection range (byte offsets)
    selection: Option<(usize, usize)>,
}

/// Attachment badge for display above input
#[derive(Debug, Clone)]
pub struct AttachmentBadge {
    pub filename: String,
    pub path: String,
    pub icon: String,
}

#[derive(Debug, Clone)]
struct VisualLine {
    text: String,
    start_char: usize,
}

impl<'a> InputWidget<'a> {
    /// Create a new input widget
    pub fn new(content: &'a str, cursor: usize, theme: &'a Theme) -> Self {
        Self {
            content,
            cursor,
            focused: false,
            theme,
            placeholder: "Type a message...",
            scroll_offset: 0,
            context_files: Vec::new(),
            attachments: Vec::new(),
            selection: None,
        }
    }

    /// Set focused state
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Set placeholder text
    pub fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = placeholder;
        self
    }

    /// Set context files for badge display
    pub fn context_files(mut self, files: Vec<String>) -> Self {
        self.context_files = files;
        self
    }

    /// Set attachments for display above input
    pub fn attachments(mut self, attachments: Vec<AttachmentBadge>) -> Self {
        self.attachments = attachments;
        self
    }

    /// Set selection range (byte offsets)
    pub fn selection(mut self, selection: Option<(usize, usize)>) -> Self {
        self.selection = selection;
        self
    }

    /// Calculate which line the cursor is on and adjust scroll if needed
    fn calculate_cursor_line(&self) -> usize {
        let text_up_to_cursor = &self.content[..self.cursor.min(self.content.len())];
        text_up_to_cursor.lines().count().saturating_sub(1)
    }

    /// Calculate optimal scroll offset to keep cursor visible
    fn calculate_scroll(&self, available_height: u16) -> usize {
        if available_height == 0 {
            return 0;
        }

        let cursor_line = self.calculate_cursor_line();
        let max_visible_lines = available_height as usize;

        // Keep cursor in view with some padding
        if cursor_line < self.scroll_offset {
            // Cursor above visible area, scroll up
            cursor_line
        } else if cursor_line >= self.scroll_offset + max_visible_lines {
            // Cursor below visible area, scroll down
            cursor_line.saturating_sub(max_visible_lines - 1)
        } else {
            // Cursor is visible, don't change scroll
            self.scroll_offset
        }
    }

    /// Wrap text to fit within given width, returning visual lines with cursor position tracking
    fn wrap_text_with_cursor(&self, width: usize) -> (Vec<VisualLine>, usize, usize) {
        if width == 0 {
            return (vec![], 0, 0);
        }

        let mut visual_lines: Vec<VisualLine> = Vec::new();
        let mut cursor_visual_line = 0;
        let mut cursor_visual_col = 0;
        let mut current_char_offset = 0;

        for line in self.content.split('\n') {
            if line.is_empty() {
                // Check if cursor is on this empty line
                if current_char_offset == self.cursor {
                    cursor_visual_line = visual_lines.len();
                    cursor_visual_col = 0;
                }
                visual_lines.push(VisualLine {
                    text: String::new(),
                    start_char: current_char_offset,
                });
                current_char_offset += 1; // Account for the newline
                continue;
            }

            // Wrap this logical line into visual lines
            let chars: Vec<char> = line.chars().collect();
            let mut start_idx = 0;

            while start_idx < chars.len() {
                let end_idx = (start_idx + width).min(chars.len());
                let chunk: String = chars[start_idx..end_idx].iter().collect();
                let chunk_char_len = chunk.chars().count();

                // Check if cursor is in this chunk
                let chunk_start_char = current_char_offset;
                let chunk_end_char = current_char_offset + chunk_char_len;

                if self.cursor >= chunk_start_char && self.cursor <= chunk_end_char {
                    cursor_visual_line = visual_lines.len();
                    // Calculate column position within this visual line
                    let chars_before_cursor = self.cursor - chunk_start_char;
                    cursor_visual_col = chars_before_cursor.min(chunk_char_len);
                }

                visual_lines.push(VisualLine {
                    text: chunk,
                    start_char: chunk_start_char,
                });
                current_char_offset += chunk_char_len;
                start_idx = end_idx;
            }

            current_char_offset += 1; // Account for the newline
        }

        // Handle cursor at very end
        if self.cursor >= self.content.len() {
            cursor_visual_line = visual_lines.len().saturating_sub(1);
            if let Some(last_line) = visual_lines.last() {
                cursor_visual_col = last_line.text.chars().count();
            }
        }

        // Ensure at least one empty line
        if visual_lines.is_empty() {
            visual_lines.push(VisualLine {
                text: String::new(),
                start_char: 0,
            });
        }

        (visual_lines, cursor_visual_line, cursor_visual_col)
    }

    /// Convert char index to byte index in a string
    fn char_to_byte(s: &str, char_idx: usize) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(i, _)| i)
            .unwrap_or_else(|| s.len())
    }
}

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        // Renamed from "Input" to "Prompt" - removed SHIFT+ENTER help text
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![Span::styled(
                " Prompt ",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )]));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 1 || inner.width < 1 {
            return;
        }

        // Split inner area for attachments/badges if context files present
        let (badge_area, content_area) = if !self.context_files.is_empty() && inner.height > 1 {
            (
                Some(Rect {
                    x: inner.x,
                    y: inner.y,
                    width: inner.width,
                    height: 1,
                }),
                Rect {
                    x: inner.x,
                    y: inner.y + 1,
                    width: inner.width,
                    height: inner.height.saturating_sub(1),
                },
            )
        } else {
            (None, inner)
        };

        // Render context file badges with cross button (following web/ui/mocks design)
        if let Some(badge_rect) = badge_area {
            let mut badge_spans = vec![];
            for (i, file) in self.context_files.iter().take(5).enumerate() {
                if i > 0 {
                    badge_spans.push(Span::raw(" "));
                }
                // Extract filename from path
                let filename = file.split('/').next_back().unwrap_or(file);
                // Truncate long filenames with ellipsis
                let display_name = if filename.len() > 15 {
                    format!(
                        "{}...",
                        crate::core::truncate_at_char_boundary(filename, 12)
                    )
                } else {
                    filename.to_string()
                };
                // Badge with file icon and cross button for removal
                badge_spans.push(Span::styled(
                    format!(" 📄 {} ", display_name),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_code),
                ));
                badge_spans.push(Span::styled(
                    "✕",
                    Style::default().fg(self.theme.red).bg(self.theme.bg_code),
                ));
            }
            if self.context_files.len() > 5 {
                badge_spans.push(Span::raw(" "));
                badge_spans.push(Span::styled(
                    format!(" +{} more ", self.context_files.len() - 5),
                    Style::default()
                        .fg(self.theme.text_muted)
                        .bg(self.theme.bg_code),
                ));
            }
            let badge_line = Line::from(badge_spans);
            Paragraph::new(badge_line).render(badge_rect, buf);
        }

        // Use content_area for the actual input
        let inner = content_area;
        if inner.height < 1 || inner.width < 1 {
            return;
        }

        let content_width = inner.width as usize;
        let cursor_visible = self.focused && get_cursor_visible();

        // Render content or placeholder with wrapping
        if self.content.is_empty() {
            let mut spans = vec![Span::styled(
                self.placeholder,
                Style::default().fg(self.theme.text_muted),
            )];
            // Show blinking cursor at start when empty and focused
            if cursor_visible {
                spans.insert(
                    0,
                    Span::styled(
                        " ",
                        Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                    ),
                );
            }
            let placeholder_line = Line::from(spans);
            let paragraph = Paragraph::new(placeholder_line);
            paragraph.render(inner, buf);
        } else {
            // Use text wrapping for long lines
            let (visual_lines, cursor_visual_line, cursor_visual_col) =
                self.wrap_text_with_cursor(content_width);

            // Calculate scroll offset to keep cursor visible
            let max_visible_lines = inner.height as usize;
            let visible_start = if cursor_visual_line >= max_visible_lines {
                cursor_visual_line.saturating_sub(max_visible_lines - 1)
            } else {
                0
            };
            let visible_end = visible_start + max_visible_lines;

            // Build display lines with cursor rendering
            let mut display_lines: Vec<Line> = vec![];

            let selection = self.selection;
            for (line_idx, line_content) in visual_lines.iter().enumerate() {
                if line_idx < visible_start || line_idx >= visible_end {
                    continue;
                }

                let mut spans = vec![];
                let line_text = &line_content.text;
                let line_start = line_content.start_char;
                let line_end = line_start + line_text.chars().count();

                if line_idx == cursor_visual_line && self.focused {
                    // This line contains the cursor
                    let chars: Vec<char> = line_text.chars().collect();

                    if cursor_visual_col >= chars.len() {
                        // Cursor at end of line
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(self.theme.text_primary),
                        ));
                        if cursor_visible {
                            spans.push(Span::styled(
                                " ",
                                Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                            ));
                        }
                    } else {
                        // Cursor in middle of line
                        let before: String = chars[..cursor_visual_col].iter().collect();
                        let cursor_char = chars[cursor_visual_col];
                        let after: String = chars[cursor_visual_col + 1..].iter().collect();

                        if !before.is_empty() {
                            spans.push(Span::styled(
                                before,
                                Style::default().fg(self.theme.text_primary),
                            ));
                        }

                        if cursor_visible {
                            spans.push(Span::styled(
                                cursor_char.to_string(),
                                Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                            ));
                        } else {
                            spans.push(Span::styled(
                                cursor_char.to_string(),
                                Style::default().fg(self.theme.text_primary),
                            ));
                        }

                        if !after.is_empty() {
                            spans.push(Span::styled(
                                after,
                                Style::default().fg(self.theme.text_primary),
                            ));
                        }
                    }
                } else {
                    // Regular line without cursor
                    if let Some((sel_start, sel_end)) = selection {
                        if sel_start < line_end && sel_end > line_start {
                            let local_start = sel_start.saturating_sub(line_start);
                            let local_end = sel_end
                                .saturating_sub(line_start)
                                .min(line_text.chars().count());
                            let start_byte = Self::char_to_byte(line_text, local_start);
                            let end_byte = Self::char_to_byte(line_text, local_end);
                            let (before, rest) = line_text.split_at(start_byte);
                            let (selected, after) =
                                rest.split_at(end_byte.saturating_sub(start_byte));

                            if !before.is_empty() {
                                spans.push(Span::styled(
                                    before.to_string(),
                                    Style::default().fg(self.theme.text_primary),
                                ));
                            }
                            if !selected.is_empty() {
                                spans.push(Span::styled(
                                    selected.to_string(),
                                    Style::default().fg(self.theme.bg_main).bg(self.theme.blue),
                                ));
                            }
                            if !after.is_empty() {
                                spans.push(Span::styled(
                                    after.to_string(),
                                    Style::default().fg(self.theme.text_primary),
                                ));
                            }
                        } else {
                            spans.push(Span::styled(
                                line_text.to_string(),
                                Style::default().fg(self.theme.text_primary),
                            ));
                        }
                    } else {
                        spans.push(Span::styled(
                            line_text.to_string(),
                            Style::default().fg(self.theme.text_primary),
                        ));
                    }
                }

                display_lines.push(Line::from(spans));
            }

            // Handle case where content ends with newline (cursor on new empty line)
            if self.content.ends_with('\n') && cursor_visual_line >= visual_lines.len() {
                if cursor_visible {
                    display_lines.push(Line::from(vec![Span::styled(
                        " ",
                        Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                    )]));
                } else {
                    display_lines.push(Line::from(""));
                }
            }

            let paragraph = Paragraph::new(display_lines);
            paragraph.render(inner, buf);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_input_renders_placeholder_when_empty() {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let input = InputWidget::new("", 0, &theme).focused(true);
                f.render_widget(input, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..40)
            .map(|x| buffer.cell((x, 1)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Type a message"));
    }

    #[test]
    fn test_input_renders_content() {
        let backend = TestBackend::new(40, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let input = InputWidget::new("Hello world", 5, &theme).focused(true);
                f.render_widget(input, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..40)
            .map(|x| buffer.cell((x, 1)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Hello"));
    }
}
