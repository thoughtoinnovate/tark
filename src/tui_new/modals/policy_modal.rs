//! Policy Manager Modal
//!
//! View and manage approval/denial patterns for tools.

use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Pattern entry for display
#[derive(Debug, Clone)]
pub struct PolicyPatternEntry {
    pub id: i64,
    pub tool: String,
    pub pattern: String,
    pub match_type: String,
    pub is_denial: bool,
    pub description: Option<String>,
}

/// Policy manager modal state
#[derive(Debug, Clone)]
pub struct PolicyModal {
    pub approvals: Vec<PolicyPatternEntry>,
    pub denials: Vec<PolicyPatternEntry>,
    pub selected_index: usize,
    pub in_approvals: bool, // true = approvals section, false = denials section
}

impl PolicyModal {
    pub fn new(approvals: Vec<PolicyPatternEntry>, denials: Vec<PolicyPatternEntry>) -> Self {
        Self {
            approvals,
            denials,
            selected_index: 0,
            in_approvals: true,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        } else if self.in_approvals && !self.denials.is_empty() {
            // Move from approvals to denials
            self.in_approvals = false;
            self.selected_index = self.denials.len().saturating_sub(1);
        }
    }

    pub fn move_down(&mut self) {
        let max = if self.in_approvals {
            self.approvals.len().saturating_sub(1)
        } else {
            self.denials.len().saturating_sub(1)
        };

        if self.selected_index < max {
            self.selected_index += 1;
        } else if !self.in_approvals && !self.approvals.is_empty() {
            // Move from denials to approvals
            self.in_approvals = true;
            self.selected_index = 0;
        }
    }

    pub fn get_selected_pattern_id(&self) -> Option<i64> {
        if self.in_approvals {
            self.approvals.get(self.selected_index).map(|p| p.id)
        } else {
            self.denials.get(self.selected_index).map(|p| p.id)
        }
    }

    pub fn remove_selected(&mut self) {
        if self.in_approvals {
            if self.selected_index < self.approvals.len() {
                self.approvals.remove(self.selected_index);
                if self.selected_index >= self.approvals.len() && self.selected_index > 0 {
                    self.selected_index -= 1;
                }
            }
        } else if self.selected_index < self.denials.len() {
            self.denials.remove(self.selected_index);
            if self.selected_index >= self.denials.len() && self.selected_index > 0 {
                self.selected_index -= 1;
            }
        }
    }
}

pub struct PolicyModalWidget<'a> {
    modal: &'a PolicyModal,
    theme: &'a Theme,
}

impl<'a> PolicyModalWidget<'a> {
    pub fn new(modal: &'a PolicyModal, theme: &'a Theme) -> Self {
        Self { modal, theme }
    }
}

impl Widget for PolicyModalWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate modal dimensions (70% height like SessionPicker)
        let popup_width = area.width * 70 / 100;
        let popup_height = area.height * 70 / 100;

        let x = (area.width.saturating_sub(popup_width)) / 2;
        let y = (area.height.saturating_sub(popup_height)) / 2;

        let modal_area = Rect::new(x, y, popup_width, popup_height);

        // Clear the modal area
        Clear.render(modal_area, buf);

        // Render modal with rounded corners (like SessionPicker)
        let block = Block::default()
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    "Policy",
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]))
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.cyan))
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Build content like SessionPicker
        let mut content: Vec<Line> = vec![];

        // Navigation hints
        content.push(Line::from(vec![
            Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
            Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("d", Style::default().fg(self.theme.red)),
            Span::styled(" Delete  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Esc", Style::default().fg(self.theme.yellow)),
            Span::styled(" Close", Style::default().fg(self.theme.text_muted)),
        ]));
        content.push(Line::from(""));

        // Approvals section header
        content.push(Line::from(vec![Span::styled(
            format!("  Approvals ({})", self.modal.approvals.len()),
            Style::default()
                .fg(self.theme.green)
                .add_modifier(Modifier::BOLD),
        )]));
        content.push(Line::from(""));

        // Approvals list
        if self.modal.approvals.is_empty() {
            content.push(Line::from(vec![Span::styled(
                "    No approval patterns",
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            )]));
        } else {
            for (i, entry) in self.modal.approvals.iter().enumerate() {
                let is_selected = self.modal.in_approvals && i == self.modal.selected_index;
                let prefix = if is_selected { "▸ " } else { "  " };

                let row_style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let mut spans = vec![Span::styled(prefix, row_style)];
                spans.push(Span::styled("✓ ", Style::default().fg(self.theme.green)));
                spans.push(Span::styled(
                    format!("{}: {}", entry.tool, entry.pattern),
                    row_style,
                ));

                content.push(Line::from(spans));

                // Show details on selected item
                if is_selected {
                    let detail = format!(
                        "    {} · {}",
                        entry.match_type,
                        entry.description.as_deref().unwrap_or("No description")
                    );
                    content.push(Line::from(vec![Span::styled(
                        detail,
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM),
                    )]));
                }
            }
        }

        content.push(Line::from(""));

        // Denials section header
        content.push(Line::from(vec![Span::styled(
            format!("  Denials ({})", self.modal.denials.len()),
            Style::default()
                .fg(self.theme.red)
                .add_modifier(Modifier::BOLD),
        )]));
        content.push(Line::from(""));

        // Denials list
        if self.modal.denials.is_empty() {
            content.push(Line::from(vec![Span::styled(
                "    No denial patterns",
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            )]));
        } else {
            for (i, entry) in self.modal.denials.iter().enumerate() {
                let is_selected = !self.modal.in_approvals && i == self.modal.selected_index;
                let prefix = if is_selected { "▸ " } else { "  " };

                let row_style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let mut spans = vec![Span::styled(prefix, row_style)];
                spans.push(Span::styled("✗ ", Style::default().fg(self.theme.red)));
                spans.push(Span::styled(
                    format!("{}: {}", entry.tool, entry.pattern),
                    row_style,
                ));

                content.push(Line::from(spans));

                // Show details on selected item
                if is_selected {
                    let detail = format!(
                        "    {} · {}",
                        entry.match_type,
                        entry.description.as_deref().unwrap_or("No description")
                    );
                    content.push(Line::from(vec![Span::styled(
                        detail,
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM),
                    )]));
                }
            }
        }

        // Footer note
        content.push(Line::from(""));
        content.push(Line::from(vec![Span::styled(
            "  Session patterns from policy.db and .tark/sessions/",
            Style::default()
                .fg(self.theme.text_muted)
                .add_modifier(Modifier::DIM),
        )]));

        // Render content
        let paragraph = Paragraph::new(content).style(Style::default().fg(self.theme.text_primary));
        paragraph.render(inner, buf);
    }
}
