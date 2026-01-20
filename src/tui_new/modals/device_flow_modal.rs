//! Device Flow Auth Modal
//!
//! Displays OAuth device flow information for provider authentication

use crate::tui_new::theme::Theme;
use crate::ui_backend::DeviceFlowSession;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Device flow authentication modal
pub struct DeviceFlowModal<'a> {
    theme: &'a Theme,
    session: &'a DeviceFlowSession,
}

impl<'a> DeviceFlowModal<'a> {
    pub fn new(theme: &'a Theme, session: &'a DeviceFlowSession) -> Self {
        Self { theme, session }
    }
}

impl Widget for DeviceFlowModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(60);
        let modal_height = area.height.min(18);
        let modal_area = Rect {
            x: (area.width.saturating_sub(modal_width)) / 2,
            y: (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        Clear.render(modal_area, buf);

        // Modal border
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                format!("{} Authentication", self.session.provider),
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.blue))
            .title(title)
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Build content
        let content: Vec<Line> = vec![
            Line::from(Span::styled(
                "To authenticate, follow these steps:",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            // Step 1: Visit URL
            Line::from(vec![
                Span::styled("1. ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Visit this URL in your browser:",
                    Style::default().fg(self.theme.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    &self.session.verification_url,
                    Style::default()
                        .fg(self.theme.blue)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]),
            Line::from(""),
            // Step 2: Enter code
            Line::from(vec![
                Span::styled("2. ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Enter this code:",
                    Style::default().fg(self.theme.text_primary),
                ),
            ]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    &self.session.user_code,
                    Style::default()
                        .fg(self.theme.yellow)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.bg_code),
                ),
            ]),
            Line::from(""),
            // Step 3: Wait
            Line::from(vec![
                Span::styled("3. ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Waiting for authorization...",
                    Style::default().fg(self.theme.text_primary),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("⏳ ", Style::default().fg(self.theme.yellow)),
                Span::styled(
                    "Polling for completion",
                    Style::default().fg(self.theme.text_muted),
                ),
            ]),
            Line::from(""),
            // Cancel hint
            Line::from(vec![
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(self.theme.red)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
        ];

        Paragraph::new(content)
            .alignment(Alignment::Left)
            .render(inner, buf);
    }
}
