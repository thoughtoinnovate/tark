//! Session Switch Confirmation Modal
//!
//! Modal shown when user tries to switch sessions while the agent is still processing.
//! Gives options to either wait for the agent to finish or abort and switch immediately.

use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Confirmation dialog for session switching while agent is processing
pub struct SessionSwitchConfirmModal<'a> {
    theme: &'a Theme,
    selected: usize, // 0 = Wait, 1 = Abort & Switch
}

impl<'a> SessionSwitchConfirmModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme, selected: 0 }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for SessionSwitchConfirmModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(55);
        let modal_height = area.height.min(12);
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
                "Agent Processing",
                Style::default()
                    .fg(self.theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.yellow))
            .title(title)
            .title_alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Build content
        let mut content: Vec<Line> = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "The agent is still responding.",
                Style::default().fg(self.theme.text_primary),
            )]),
            Line::from(vec![Span::styled(
                "What would you like to do?",
                Style::default().fg(self.theme.text_muted),
            )]),
            Line::from(""),
        ];

        // Options
        let options = [
            ("⏳", "Wait", "Let the agent finish first"),
            ("⏹️", "Abort & Switch", "Stop the agent and switch now"),
        ];

        for (i, (icon, name, desc)) in options.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let name_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
                    .bg(ratatui::style::Color::Rgb(45, 60, 83))
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            // Option line
            content.push(Line::from(vec![
                Span::raw(prefix),
                Span::raw(*icon),
                Span::raw(" "),
                Span::styled(*name, name_style),
                Span::styled(
                    format!("  {}", desc),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));
        }

        content.push(Line::from(""));

        // Navigation hints
        content.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
            Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Enter", Style::default().fg(self.theme.green)),
            Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Esc", Style::default().fg(self.theme.yellow)),
            Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
        ]));

        Paragraph::new(content).render(inner, buf);
    }
}
