//! Policy Manager Modal
//!
//! View and manage approval/denial patterns for tools.

use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Widget},
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
        // Center the modal
        let modal_width = area.width.min(80);
        let modal_height = area.height.min(30);
        let modal_x = (area.width.saturating_sub(modal_width)) / 2;
        let modal_y = (area.height.saturating_sub(modal_height)) / 2;

        let modal_area = Rect {
            x: area.x + modal_x,
            y: area.y + modal_y,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        for y in modal_area.top()..modal_area.bottom() {
            for x in modal_area.left()..modal_area.right() {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(Style::default().bg(self.theme.bg_dark));
                }
            }
        }

        // Create block
        let block = Block::default()
            .title(" Policy Manager ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border))
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Split into sections
        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header
                Constraint::Min(5),    // Approvals
                Constraint::Length(1), // Separator
                Constraint::Min(5),    // Denials
                Constraint::Length(3), // Help text
            ])
            .split(inner);

        // Header
        let header = Paragraph::new("Session Approval Patterns")
            .style(
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center);
        header.render(chunks[0], buf);

        // Approvals section
        let approval_items: Vec<ListItem> = self
            .modal
            .approvals
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let content = format!(
                    "{}: {} ({}) {}",
                    entry.tool,
                    entry.pattern,
                    entry.match_type,
                    entry.description.as_deref().unwrap_or("")
                );

                let style = if self.modal.in_approvals && i == self.modal.selected_index {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", i + 1),
                        Style::default().fg(self.theme.cyan),
                    ),
                    Span::styled(content, style),
                ]))
            })
            .collect();

        let approvals_title = format!(" Approvals ({}) ", self.modal.approvals.len());
        let approvals_list = List::new(approval_items)
            .block(
                Block::default()
                    .title(approvals_title)
                    .borders(Borders::ALL)
                    .border_style(if self.modal.in_approvals {
                        Style::default().fg(self.theme.cyan)
                    } else {
                        Style::default().fg(self.theme.border)
                    }),
            )
            .style(Style::default().bg(self.theme.bg_dark));

        approvals_list.render(chunks[1], buf);

        // Separator
        let separator = Paragraph::new("─".repeat(inner.width as usize))
            .style(Style::default().fg(self.theme.border));
        separator.render(chunks[2], buf);

        // Denials section
        let denial_items: Vec<ListItem> = self
            .modal
            .denials
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let content = format!(
                    "{}: {} ({}) {}",
                    entry.tool,
                    entry.pattern,
                    entry.match_type,
                    entry.description.as_deref().unwrap_or("")
                );

                let style = if !self.modal.in_approvals && i == self.modal.selected_index {
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.selection_bg)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}] ", i + 1), Style::default().fg(self.theme.red)),
                    Span::styled(content, style),
                ]))
            })
            .collect();

        let denials_title = format!(" Denials ({}) ", self.modal.denials.len());
        let denials_list = List::new(denial_items)
            .block(
                Block::default()
                    .title(denials_title)
                    .borders(Borders::ALL)
                    .border_style(if !self.modal.in_approvals {
                        Style::default().fg(self.theme.red)
                    } else {
                        Style::default().fg(self.theme.border)
                    }),
            )
            .style(Style::default().bg(self.theme.bg_dark));

        denials_list.render(chunks[3], buf);

        // Help text
        let help_lines = vec![
            Line::from(vec![
                Span::styled("↑/↓", Style::default().fg(self.theme.cyan)),
                Span::raw(": Navigate  "),
                Span::styled("d", Style::default().fg(self.theme.cyan)),
                Span::raw(": Delete  "),
                Span::styled("Esc", Style::default().fg(self.theme.cyan)),
                Span::raw(": Close"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Note: Session patterns from policy.db and .tark/sessions/. Persistent: ~/.config/tark/policy/",
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            )),
        ];

        let help = Paragraph::new(help_lines)
            .alignment(Alignment::Center)
            .style(Style::default().fg(self.theme.text_secondary));

        help.render(chunks[4], buf);
    }
}
