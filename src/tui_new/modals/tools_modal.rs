//! Tools Viewer Modal
//!
//! Displays available tools for the current agent mode with risk levels

use crate::core::types::AgentMode;
use crate::tools::RiskLevel;
use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Tool information for display
pub struct ToolDisplay {
    pub name: String,
    pub description: String,
    pub risk_level: RiskLevel,
}

/// Tools viewer modal
pub struct ToolsModal<'a> {
    theme: &'a Theme,
    tools: Vec<ToolDisplay>,
    agent_mode: AgentMode,
    selected: usize,
}

impl<'a> ToolsModal<'a> {
    pub fn new(theme: &'a Theme, agent_mode: AgentMode) -> Self {
        Self {
            theme,
            tools: Vec::new(),
            agent_mode,
            selected: 0,
        }
    }

    pub fn tools(mut self, tools: Vec<ToolDisplay>) -> Self {
        self.tools = tools;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    fn risk_icon(&self, level: RiskLevel) -> &'static str {
        match level {
            RiskLevel::ReadOnly => "✓",
            RiskLevel::Write => "✎",
            RiskLevel::Risky => "⚠",
            RiskLevel::Dangerous => "⚡",
        }
    }

    fn risk_color(&self, level: RiskLevel) -> ratatui::style::Color {
        match level {
            RiskLevel::ReadOnly => self.theme.green,
            RiskLevel::Write => self.theme.yellow,
            RiskLevel::Risky => self.theme.red,
            RiskLevel::Dangerous => self.theme.red,
        }
    }
}

impl Widget for ToolsModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(70);
        let modal_height = area.height.min(25);
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
                format!("Available Tools - {:?} Mode", self.agent_mode),
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.cyan))
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
            // Legend
            Line::from(vec![
                Span::styled("Risk: ", Style::default().fg(self.theme.text_muted)),
                Span::styled("✓ ", Style::default().fg(self.theme.green)),
                Span::styled("Safe  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("✎ ", Style::default().fg(self.theme.yellow)),
                Span::styled("Write  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("⚠ ", Style::default().fg(self.theme.red)),
                Span::styled("Risky  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("⚡ ", Style::default().fg(self.theme.red)),
                Span::styled("Dangerous", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        if self.tools.is_empty() {
            content.push(Line::from(Span::styled(
                "  No tools available for this mode",
                Style::default().fg(self.theme.text_muted),
            )));
        } else {
            for (i, tool) in self.tools.iter().enumerate().take(15) {
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

                let risk_icon = self.risk_icon(tool.risk_level);
                let risk_color = self.risk_color(tool.risk_level);

                // Tool name with risk indicator
                content.push(Line::from(vec![
                    Span::raw(prefix),
                    Span::styled(risk_icon, Style::default().fg(risk_color)),
                    Span::raw(" "),
                    Span::styled(&tool.name, name_style),
                ]));

                // Description
                content.push(Line::from(vec![
                    Span::raw("   "),
                    Span::styled(
                        &tool.description,
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]));

                if i < self.tools.len().min(15) - 1 {
                    content.push(Line::from(""));
                }
            }

            if self.tools.len() > 15 {
                content.push(Line::from(Span::styled(
                    format!("  ... {} more tools", self.tools.len() - 15),
                    Style::default().fg(self.theme.text_muted),
                )));
            }
        }

        Paragraph::new(content).render(inner, buf);
    }
}
