//! Plugin Management Modal
//!
//! Modal for viewing and managing installed plugins

use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Plugin information for display
#[derive(Debug, Clone)]
pub struct PluginDisplay {
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub description: String,
}

/// Plugin management modal
pub struct PluginModal<'a> {
    theme: &'a Theme,
    plugins: Vec<PluginDisplay>,
    selected: usize,
}

impl<'a> PluginModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            plugins: Vec::new(),
            selected: 0,
        }
    }

    pub fn plugins(mut self, plugins: Vec<PluginDisplay>) -> Self {
        self.plugins = plugins;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for PluginModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(65);
        let modal_height = area.height.min(20);
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
                "Plugin Management",
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
                Span::styled("Esc", Style::default().fg(self.theme.yellow)),
                Span::styled(" Close", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        if self.plugins.is_empty() {
            content.push(Line::from(Span::styled(
                "  No plugins installed",
                Style::default().fg(self.theme.text_muted),
            )));
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                "  Plugins can be installed in ~/.config/tark/plugins/",
                Style::default().fg(self.theme.text_muted),
            )));
        } else {
            for (i, plugin) in self.plugins.iter().enumerate() {
                let is_selected = i == self.selected;
                let prefix = if is_selected { "▸ " } else { "  " };

                let name_style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(ratatui::style::Color::Rgb(45, 60, 83))
                } else {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD)
                };

                let status_icon = if plugin.enabled { "✓" } else { "○" };
                let status_color = if plugin.enabled {
                    self.theme.green
                } else {
                    self.theme.text_muted
                };

                // Plugin name with status
                content.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(status_icon, Style::default().fg(status_color)),
                    Span::raw(" "),
                    Span::styled(&plugin.name, name_style),
                    Span::styled(
                        format!(" v{}", plugin.version),
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]));

                // Description
                if !plugin.description.is_empty() {
                    content.push(Line::from(vec![
                        Span::raw("   "),
                        Span::styled(
                            &plugin.description,
                            Style::default().fg(self.theme.text_secondary),
                        ),
                    ]));
                }

                if i < self.plugins.len() - 1 {
                    content.push(Line::from(""));
                }
            }
        }

        Paragraph::new(content).render(inner, buf);
    }
}
