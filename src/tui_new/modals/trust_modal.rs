//! Trust Level Modal
//!
//! Modal for selecting tool execution trust level (Manual, Balanced, Careful)
//! Triggered by Ctrl+Shift+B in Build mode

use crate::tools::TrustLevel;
use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Trust level selector modal
pub struct TrustModal<'a> {
    theme: &'a Theme,
    current_level: TrustLevel,
    selected: usize,
}

impl<'a> TrustModal<'a> {
    pub fn new(theme: &'a Theme, current_level: TrustLevel) -> Self {
        Self {
            theme,
            current_level,
            selected: current_level.index(),
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    fn level_at_index(&self, index: usize) -> TrustLevel {
        TrustLevel::from_index(index)
    }

    fn level_icon(&self, level: TrustLevel) -> &'static str {
        match level {
            TrustLevel::Manual => "🛑",
            TrustLevel::Balanced => "⚖️",
            TrustLevel::Careful => "🛡️",
        }
    }

    fn level_description(&self, level: TrustLevel) -> &'static str {
        match level {
            TrustLevel::Manual => "Prompt for every operation",
            TrustLevel::Balanced => "Learn patterns, prompt for new operations",
            TrustLevel::Careful => "Conservative pattern matching",
        }
    }
}

impl Widget for TrustModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(60);
        let modal_height = area.height.min(15);
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
                "Trust Level",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.purple))
            .title(title)
            .title_alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Build content
        let mut content: Vec<Line> = vec![
            // Navigation hints
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.green)),
                Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.yellow)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        // Trust level options
        let levels = [
            TrustLevel::Manual,
            TrustLevel::Balanced,
            TrustLevel::Careful,
        ];

        for (i, level) in levels.iter().enumerate() {
            let is_selected = i == self.selected;
            let is_current = *level == self.current_level;
            let prefix = if is_selected { "▸ " } else { "  " };

            let status_icon = if is_current { " ✓" } else { "" };

            let name_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
                    .bg(self.theme.selection_bg)
            } else if is_current {
                Style::default()
                    .fg(self.theme.green)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            // Level name line
            content.push(Line::from(vec![
                Span::raw(prefix),
                Span::raw(self.level_icon(*level)),
                Span::raw(" "),
                Span::styled(format!("{:?}", level), name_style),
                Span::styled(status_icon, Style::default().fg(self.theme.green)),
            ]));

            // Description line
            content.push(Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    self.level_description(*level),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));

            if i < levels.len() - 1 {
                content.push(Line::from(""));
            }
        }

        Paragraph::new(content).render(inner, buf);
    }
}
