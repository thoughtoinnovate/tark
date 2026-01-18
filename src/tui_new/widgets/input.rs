//! Input Widget
//!
//! Text input area with cursor and multi-line support
//! Feature: 04_input_area.feature

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
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
            .title(" Input ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 1 || inner.width < 1 {
            return;
        }

        // Render content or placeholder
        let text = if self.content.is_empty() {
            Line::from(Span::styled(
                self.placeholder,
                Style::default().fg(self.theme.text_muted),
            ))
        } else {
            // Split content at cursor for rendering
            let (before, after) = self.content.split_at(self.cursor.min(self.content.len()));
            let cursor_char = after.chars().next().unwrap_or(' ');
            let after_cursor = if after.len() > cursor_char.len_utf8() {
                &after[cursor_char.len_utf8()..]
            } else {
                ""
            };

            let mut spans = vec![Span::styled(
                before,
                Style::default().fg(self.theme.text_primary),
            )];

            if self.focused {
                // Show cursor
                spans.push(Span::styled(
                    cursor_char.to_string(),
                    Style::default()
                        .fg(self.theme.bg_main)
                        .bg(self.theme.text_primary)
                        .add_modifier(Modifier::SLOW_BLINK),
                ));
            } else {
                spans.push(Span::styled(
                    cursor_char.to_string(),
                    Style::default().fg(self.theme.text_primary),
                ));
            }

            spans.push(Span::styled(
                after_cursor,
                Style::default().fg(self.theme.text_primary),
            ));

            Line::from(spans)
        };

        let paragraph = Paragraph::new(text);
        paragraph.render(inner, buf);
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
