//! Terminal Frame Widget
//!
//! The main container with rounded borders (╭─╮╰─╯)
//! Feature: 01_terminal_layout.feature - Borders render correctly with Unicode box drawing

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    symbols::border,
    widgets::{Block, Borders, Widget},
};

use crate::tui_new::theme::Theme;

/// Terminal frame with rounded borders
pub struct TerminalFrame<'a> {
    /// Title for the frame
    title: Option<&'a str>,
    /// Theme for styling
    theme: &'a Theme,
    /// Whether this frame is focused
    focused: bool,
}

impl<'a> TerminalFrame<'a> {
    /// Create a new terminal frame
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            title: None,
            theme,
            focused: false,
        }
    }

    /// Set the title
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    /// Set focused state
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Create the block with rounded borders
    pub fn block(&self) -> Block<'a> {
        let border_color = if self.focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(border_color));

        if let Some(title) = self.title {
            block = block.title(title);
        }

        block
    }
}

impl Widget for TerminalFrame<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.block().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_terminal_frame_renders_rounded_borders() {
        let backend = TestBackend::new(10, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let frame = TerminalFrame::new(&theme);
                f.render_widget(frame, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Check corners are rounded
        assert_eq!(buffer.cell((0, 0)).unwrap().symbol(), "╭");
        assert_eq!(buffer.cell((9, 0)).unwrap().symbol(), "╮");
        assert_eq!(buffer.cell((0, 4)).unwrap().symbol(), "╰");
        assert_eq!(buffer.cell((9, 4)).unwrap().symbol(), "╯");
    }
}
