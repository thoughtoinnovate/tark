//! Input Widget
//!
//! Text input area with cursor and multi-line support
//! Feature: 04_input_area.feature

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui_new::theme::Theme;

/// Input widget for user text entry
pub struct InputWidget<'a> {
    /// Current input text
    content: &'a str,
    /// Cursor position
    cursor: usize,
    /// Whether the input is focused
    focused: bool,
    /// Theme for styling
    theme: &'a Theme,
    /// Placeholder text
    placeholder: &'a str,
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
}

impl Widget for InputWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Line::from(vec![
                Span::styled(" Input ", Style::default().fg(self.theme.text_primary)),
                Span::styled(
                    "(Shift+Enter for newline) ",
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 1 || inner.width < 1 {
            return;
        }

        // Render content or placeholder with wrapping
        if self.content.is_empty() {
            let placeholder_line = Line::from(Span::styled(
                self.placeholder,
                Style::default().fg(self.theme.text_muted),
            ));
            let paragraph = Paragraph::new(placeholder_line);
            paragraph.render(inner, buf);
        } else {
            // Split content at cursor for rendering
            let (before, after) = self.content.split_at(self.cursor.min(self.content.len()));
            let cursor_char = after.chars().next().unwrap_or(' ');
            let after_cursor = if after.len() > cursor_char.len_utf8() {
                &after[cursor_char.len_utf8()..]
            } else {
                ""
            };

            // Build text with cursor, then wrap
            let mut display_text = before.to_string();
            if self.focused {
                // Add visible cursor
                display_text.push(cursor_char);
            } else {
                display_text.push(cursor_char);
            }
            display_text.push_str(after_cursor);

            // Wrap text to fit width
            let width = inner.width as usize;
            let mut lines: Vec<Line> = vec![];
            let mut current_line = String::new();

            for ch in display_text.chars() {
                if ch == '\n' {
                    // Explicit newline
                    lines.push(Line::from(Span::styled(
                        current_line.clone(),
                        Style::default().fg(self.theme.text_primary),
                    )));
                    current_line.clear();
                } else if current_line.len() >= width {
                    // Auto-wrap at width
                    lines.push(Line::from(Span::styled(
                        current_line.clone(),
                        Style::default().fg(self.theme.text_primary),
                    )));
                    current_line.clear();
                    current_line.push(ch);
                } else {
                    current_line.push(ch);
                }
            }

            // Add remaining line
            if !current_line.is_empty() {
                lines.push(Line::from(Span::styled(
                    current_line,
                    Style::default().fg(self.theme.text_primary),
                )));
            }

            // Apply cursor styling if focused
            if self.focused && !lines.is_empty() {
                // Find cursor position in wrapped lines
                let chars_before_cursor = before.len();
                let mut char_count = 0;

                for (_line_idx, line) in lines.iter_mut().enumerate() {
                    let line_len = line.width();
                    if char_count <= chars_before_cursor
                        && chars_before_cursor < char_count + line_len
                    {
                        // Cursor is on this line - re-style with cursor highlight
                        let _cursor_pos_in_line = chars_before_cursor - char_count;
                        // For simplicity, just add a blinking cursor at end of line for now
                        break;
                    }
                    char_count += line_len;
                }
            }

            let paragraph = Paragraph::new(lines).wrap(ratatui::widgets::Wrap { trim: false });
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
