//! Tools Viewer Modal
//!
//! Displays available tools for the current agent mode with risk levels

use crate::core::types::AgentMode;
use crate::tools::{RiskLevel, ToolCategory};
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
    pub category: ToolCategory,
}

impl From<crate::ui_backend::tool_execution::ToolInfo> for ToolDisplay {
    fn from(info: crate::ui_backend::tool_execution::ToolInfo) -> Self {
        Self {
            name: info.name,
            description: info.description,
            risk_level: info.risk_level,
            category: info.category,
        }
    }
}

/// Tools viewer modal
pub struct ToolsModal<'a> {
    theme: &'a Theme,
    tools: Vec<ToolDisplay>,
    agent_mode: AgentMode,
    selected: usize,
    scroll_offset: usize,
    is_external: bool,
}

impl<'a> ToolsModal<'a> {
    pub fn new(theme: &'a Theme, agent_mode: AgentMode) -> Self {
        Self {
            theme,
            tools: Vec::new(),
            agent_mode,
            selected: 0,
            scroll_offset: 0,
            is_external: false,
        }
    }

    pub fn tools(mut self, tools: Vec<crate::ui_backend::tool_execution::ToolInfo>) -> Self {
        self.tools = tools.into_iter().map(ToolDisplay::from).collect();
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    pub fn external(mut self, external: bool) -> Self {
        self.is_external = external;
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

        // Calculate viewport
        let header_lines = 4; // nav hints + legend + blanks
        let lines_per_tool = 3; // name + description + blank
        let inner_height = modal_height.saturating_sub(2) as usize; // minus borders
        let content_height = inner_height.saturating_sub(header_lines);
        let max_visible_tools = content_height / lines_per_tool;

        // Dynamic title based on tool type
        let title_text = if self.tools.is_empty() {
            if self.is_external {
                "External Tools".to_string()
            } else {
                "Internal Tools".to_string()
            }
        } else if self.is_external {
            format!(
                "External Tools [{}/{}]",
                self.selected + 1,
                self.tools.len()
            )
        } else {
            format!(
                "Internal Tools [{}/{}]",
                self.selected + 1,
                self.tools.len()
            )
        };

        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title_text,
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
                "  No tools available",
                Style::default().fg(self.theme.text_muted),
            )));
        } else {
            // Scroll-up indicator
            if self.scroll_offset > 0 {
                content.push(Line::from(Span::styled(
                    format!("  ▲ {} more above", self.scroll_offset),
                    Style::default().fg(self.theme.text_muted),
                )));
                content.push(Line::from(""));
            }

            // Render visible tools
            let visible_end = (self.scroll_offset + max_visible_tools).min(self.tools.len());
            for i in self.scroll_offset..visible_end {
                let tool = &self.tools[i];
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

                if i < visible_end - 1 {
                    content.push(Line::from(""));
                }
            }

            // Scroll-down indicator
            let remaining = self.tools.len().saturating_sub(visible_end);
            if remaining > 0 {
                content.push(Line::from(""));
                content.push(Line::from(Span::styled(
                    format!("  ▼ {} more below", remaining),
                    Style::default().fg(self.theme.text_muted),
                )));
            }
        }

        Paragraph::new(content).render(inner, buf);
    }
}
