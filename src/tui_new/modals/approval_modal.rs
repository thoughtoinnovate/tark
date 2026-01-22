//! Approval Modal for Risky Operations
//!
//! Displays a modal when a risky operation needs user approval.
//! UX Design Principles:
//! - Clear visual hierarchy: Risk → Operation → Command → Actions
//! - Unified list: actions and patterns in one navigable list
//! - Distinct colors: Green (approve), Yellow (patterns), Red (deny)
//! - Prominent selected state with full-row highlight

use crate::tui_new::theme::Theme;
use crate::ui_backend::approval::{ApprovalCardState, RiskLevel};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// Approval modal widget
pub struct ApprovalModal<'a> {
    theme: &'a Theme,
    approval: &'a ApprovalCardState,
}

impl<'a> ApprovalModal<'a> {
    pub fn new(theme: &'a Theme, approval: &'a ApprovalCardState) -> Self {
        Self { theme, approval }
    }

    fn risk_color(&self) -> Color {
        match self.approval.risk_level {
            RiskLevel::Safe => self.theme.green,
            RiskLevel::Write => self.theme.yellow,
            RiskLevel::Risky => Color::Rgb(255, 150, 50), // Orange for risky
            RiskLevel::Dangerous => self.theme.red,
        }
    }

    fn risk_icon(&self) -> &'static str {
        match self.approval.risk_level {
            RiskLevel::Safe => "✓",
            RiskLevel::Write => "✎",
            RiskLevel::Risky => "⚠",
            RiskLevel::Dangerous => "⚡",
        }
    }

    fn risk_label(&self) -> &'static str {
        match self.approval.risk_level {
            RiskLevel::Safe => "SAFE",
            RiskLevel::Write => "WRITE",
            RiskLevel::Risky => "RISKY",
            RiskLevel::Dangerous => "DANGER",
        }
    }

    /// Render a unified list item (action or pattern) with full-width highlight
    #[allow(clippy::too_many_arguments)]
    fn render_item_line(
        &self,
        is_selected: bool,
        icon: &str,
        key: &str,
        label: &str,
        description: &str,
        color: Color,
        width: u16,
    ) -> Line<'static> {
        // Full-width highlight for selected item
        let bg = if is_selected { color } else { Color::Reset };
        let fg = if is_selected {
            self.theme.bg_dark
        } else {
            color
        };
        let desc_fg = if is_selected {
            self.theme.bg_dark
        } else {
            self.theme.text_secondary
        };

        // Selection indicator
        let marker = if is_selected { " ▶ " } else { "   " };

        // Build the button text
        let button_text = format!("[{} {}{}]", icon, key, label);
        let desc_text = format!(" {}", description);

        // Calculate padding to fill width
        let content_len =
            marker.chars().count() + button_text.chars().count() + desc_text.chars().count();
        let padding = " ".repeat((width as usize).saturating_sub(content_len + 1));

        Line::from(vec![
            Span::styled(
                marker.to_string(),
                Style::default()
                    .fg(if is_selected {
                        self.theme.bg_dark
                    } else {
                        self.theme.yellow
                    })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[{} ", icon),
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                key.to_string(),
                Style::default()
                    .fg(fg)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::styled(
                format!("{}]", label),
                Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Span::styled(desc_text, Style::default().fg(desc_fg).bg(bg)),
            Span::styled(padding, Style::default().bg(bg)),
        ])
    }
}

impl Widget for ApprovalModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate dynamic modal size based on content
        let pattern_count = self.approval.suggested_patterns.len().min(5);
        let has_files = !self.approval.affected_paths.is_empty();

        // Total items: RunOnce + AlwaysAllow + patterns + Skip
        let total_items = 2 + pattern_count + 1;

        // Base height + items + files section
        let items_height = total_items as u16 + 1; // +1 for spacer
        let files_height: u16 = if has_files {
            2 + self.approval.affected_paths.len().min(3) as u16
        } else {
            0
        };
        let base_height: u16 = 8; // Title, operation, command box, footer
        let modal_height = (base_height + items_height + files_height).min(area.height - 2);
        let modal_width = area.width.min(65);

        let modal_area = Rect {
            x: (area.width.saturating_sub(modal_width)) / 2,
            y: (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        Clear.render(modal_area, buf);

        // Modal border with prominent risk-colored title
        let risk_color = self.risk_color();
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                self.risk_icon(),
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                self.risk_label(),
                Style::default().fg(risk_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " - Approval Required ",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        // Footer with keyboard hints
        let footer = Line::from(vec![
            Span::styled(" ↑↓ ", Style::default().fg(self.theme.yellow)),
            Span::styled("navigate  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Enter ", Style::default().fg(self.theme.green)),
            Span::styled("select  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Esc ", Style::default().fg(self.theme.red)),
            Span::styled("cancel ", Style::default().fg(self.theme.text_muted)),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(risk_color))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_bottom(footer)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Build layout constraints
        let mut constraints = vec![
            Constraint::Length(2), // Operation name
            Constraint::Length(3), // Command box
        ];

        if has_files {
            constraints.push(Constraint::Length(
                1 + self.approval.affected_paths.len().min(3) as u16,
            ));
        }

        constraints.push(Constraint::Min(items_height)); // Unified action list

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut chunk_idx = 0;

        // Operation name with description
        let operation_lines = vec![
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(
                    &self.approval.operation,
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(Span::styled(
                format!("  {}", &self.approval.description),
                Style::default().fg(self.theme.text_secondary),
            )),
        ];
        Paragraph::new(operation_lines)
            .wrap(Wrap { trim: true })
            .render(chunks[chunk_idx], buf);
        chunk_idx += 1;

        // Command in a styled box
        let command_label = match self.approval.operation.as_str() {
            "delete_file" => " Path ",
            "write_file" | "patch_file" | "read_file" | "read_files" => " File ",
            _ => " Command ",
        };

        let command_block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.cyan))
            .title(Span::styled(
                command_label,
                Style::default().fg(self.theme.cyan),
            ));

        let command_inner = command_block.inner(chunks[chunk_idx]);
        command_block.render(chunks[chunk_idx], buf);

        let command_text = Paragraph::new(self.approval.command.as_str())
            .style(Style::default().fg(self.theme.cyan))
            .wrap(Wrap { trim: false });
        command_text.render(command_inner, buf);
        chunk_idx += 1;

        // Affected files (if any)
        if has_files {
            let mut file_lines = vec![Line::from(Span::styled(
                "  Affected files:",
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::ITALIC),
            ))];
            for path in self.approval.affected_paths.iter().take(3) {
                file_lines.push(Line::from(vec![
                    Span::styled("    📄 ", Style::default().fg(self.theme.blue)),
                    Span::styled(
                        path.to_string(),
                        Style::default().fg(self.theme.text_secondary),
                    ),
                ]));
            }
            if self.approval.affected_paths.len() > 3 {
                file_lines.push(Line::from(Span::styled(
                    format!("    ... +{} more", self.approval.affected_paths.len() - 3),
                    Style::default().fg(self.theme.text_muted),
                )));
            }
            Paragraph::new(file_lines).render(chunks[chunk_idx], buf);
            chunk_idx += 1;
        }

        // Unified action list with patterns inline
        let selected_index = self.approval.selected_index;
        let action_width = inner.width.saturating_sub(2);

        let mut items: Vec<Line> = vec![Line::from("")]; // Spacer

        // 0: Run Once
        items.push(self.render_item_line(
            selected_index == 0,
            "▶",
            "R",
            "un Once",
            "Execute this time only",
            self.theme.green,
            action_width,
        ));

        // 1: Always Allow
        items.push(self.render_item_line(
            selected_index == 1,
            "✓",
            "A",
            "lways Allow",
            "Remember this command",
            Color::Rgb(100, 200, 100),
            action_width,
        ));

        // 2+: Patterns (if any)
        for (idx, pattern) in self.approval.suggested_patterns.iter().take(5).enumerate() {
            let item_index = 2 + idx;
            items.push(self.render_item_line(
                selected_index == item_index,
                "✱",
                &format!("{}", idx + 1),
                "",
                &pattern.description,
                self.theme.yellow,
                action_width,
            ));
        }

        // Last: Skip
        let skip_index = 2 + pattern_count;
        items.push(self.render_item_line(
            selected_index == skip_index,
            "✗",
            "S",
            "kip",
            "Don't run this",
            self.theme.red,
            action_width,
        ));

        Paragraph::new(items).render(chunks[chunk_idx], buf);
    }
}
