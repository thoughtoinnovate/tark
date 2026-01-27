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
use unicode_width::UnicodeWidthStr;

use crate::tui_new::config::AppConfig;
use crate::tui_new::theme::Theme;

/// Header widget showing agent name and path
pub struct Header<'a> {
    /// Application config
    config: &'a AppConfig,
    /// Theme for styling
    theme: &'a Theme,
    /// Optional remote status indicator
    remote: Option<RemoteIndicator>,
}

impl<'a> Header<'a> {
    /// Create a new header widget
    pub fn new(config: &'a AppConfig, theme: &'a Theme, remote: Option<RemoteIndicator>) -> Self {
        Self {
            config,
            theme,
            remote,
        }
    }
}

#[derive(Clone)]
pub struct RemoteIndicator {
    pub label: String,
    pub status_color: ratatui::style::Color,
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Format: "Tark  ~/path/to/project" + optional right-aligned remote indicator
        let agent_label = if self.remote.is_some() {
            format!("📡 {}", self.config.agent_name)
        } else {
            self.config.agent_name.clone()
        };
        let left_text = format!("{}  {}", agent_label, self.config.default_path);

        let right_text = self
            .remote
            .as_ref()
            .map(|r| format!("⏺ {}", r.label))
            .unwrap_or_default();

        let left_width = left_text.width();
        let right_width = right_text.width();
        let total_width = area.width as usize;
        let padding = if right_width > 0 && total_width > left_width + right_width {
            total_width - left_width - right_width
        } else {
            1
        };

        let mut spans = vec![
            Span::styled(agent_label, Style::default().fg(self.theme.text_primary)),
            Span::styled("  ", Style::default()),
            Span::styled(
                &self.config.default_path,
                Style::default().fg(self.theme.text_muted),
            ),
        ];

        if !right_text.is_empty() {
            spans.push(Span::raw(" ".repeat(padding)));
            let color = self
                .remote
                .as_ref()
                .map(|r| r.status_color)
                .unwrap_or(self.theme.text_muted);
            spans.push(Span::styled("⏺ ", Style::default().fg(color)));
            spans.push(Span::styled(
                self.remote
                    .as_ref()
                    .map(|r| r.label.clone())
                    .unwrap_or_default(),
                Style::default().fg(self.theme.text_muted),
            ));
        }

        let header_text = Line::from(spans);

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
                let header = Header::new(&config, &theme, None);
                f.render_widget(header, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..60)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Tark"));
    }
}
