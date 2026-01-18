//! Header Widget
//!
//! Displays agent name, icon, and current path
//! Feature: 01_terminal_layout.feature - Terminal header displays correct information

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui_new::config::AppConfig;
use crate::tui_new::theme::Theme;

/// Header widget showing agent name and path
pub struct Header<'a> {
    /// Application config
    config: &'a AppConfig,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> Header<'a> {
    /// Create a new header widget
    pub fn new(config: &'a AppConfig, theme: &'a Theme) -> Self {
        Self { config, theme }
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Format: "🖥 Tark Terminal  ~/path/to/project"
        let header_text = Line::from(vec![
            Span::styled(
                format!("{} ", self.config.header_icon),
                Style::default().fg(self.theme.cyan),
            ),
            Span::styled(
                &self.config.agent_name,
                Style::default().fg(self.theme.text_primary),
            ),
            Span::styled("  ", Style::default()),
            Span::styled(
                &self.config.default_path,
                Style::default().fg(self.theme.text_muted),
            ),
        ]);

        let paragraph = Paragraph::new(header_text);
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_header_renders_agent_name() {
        let backend = TestBackend::new(60, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let config = AppConfig::default();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let header = Header::new(&config, &theme);
                f.render_widget(header, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..60)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Tark Terminal"));
    }
}
