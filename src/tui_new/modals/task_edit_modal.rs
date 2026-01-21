//! Task Edit Modal
//!
//! Modal for editing a queued task's content before it's processed.

use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// Modal for editing a queued task
pub struct TaskEditModal<'a> {
    theme: &'a Theme,
    content: &'a str,
    cursor_position: usize,
}

impl<'a> TaskEditModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            content: "",
            cursor_position: 0,
        }
    }

    pub fn content(mut self, content: &'a str) -> Self {
        self.content = content;
        self
    }

    pub fn cursor_position(mut self, pos: usize) -> Self {
        self.cursor_position = pos;
        self
    }
}

impl Widget for TaskEditModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal - make it wider for comfortable editing
        let modal_width = area.width.clamp(50, 70);
        let modal_height = area.height.clamp(10, 14);
        let modal_area = Rect {
            x: (area.width.saturating_sub(modal_width)) / 2,
            y: (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        Clear.render(modal_area, buf);

        // Modal border with title
        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "Edit Queued Task",
                Style::default()
                    .fg(self.theme.cyan)
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
        let mut lines: Vec<Line> = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Edit the task message before it's sent:",
                Style::default().fg(self.theme.text_muted),
            )]),
            Line::from(""),
        ];

        // Input area with border
        let input_height = inner.height.saturating_sub(7); // Reserve space for hints
        let input_width = inner.width.saturating_sub(4);

        // Add input box indicator
        lines.push(Line::from(vec![Span::styled(
            format!("╭{}╮", "─".repeat(input_width as usize)),
            Style::default().fg(self.theme.border),
        )]));

        // Wrap content and show with cursor
        let content_with_cursor = if self.cursor_position <= self.content.len() {
            let (before, after) = self.content.split_at(self.cursor_position);
            format!("{}│{}", before, after)
        } else {
            format!("{}│", self.content)
        };

        // Split content into lines that fit the width
        let max_line_width = input_width.saturating_sub(2) as usize;
        let wrapped_lines: Vec<&str> = content_with_cursor
            .lines()
            .flat_map(|line| {
                if line.len() <= max_line_width {
                    vec![line]
                } else {
                    // Simple word wrap
                    let mut result = Vec::new();
                    let mut current = line;
                    while current.len() > max_line_width {
                        let split_pos = current[..max_line_width]
                            .rfind(|c: char| c.is_whitespace())
                            .unwrap_or(max_line_width);
                        result.push(&current[..split_pos]);
                        current = current[split_pos..].trim_start();
                    }
                    if !current.is_empty() {
                        result.push(current);
                    }
                    result
                }
            })
            .collect();

        // Show content lines (up to input_height)
        for i in 0..input_height as usize {
            let line_content = wrapped_lines.get(i).unwrap_or(&"");
            let padded = format!(
                "│ {:<width$} │",
                line_content,
                width = (input_width - 2) as usize
            );
            lines.push(Line::from(vec![Span::styled(
                padded,
                Style::default().fg(self.theme.text_primary),
            )]));
        }

        // Close input box
        lines.push(Line::from(vec![Span::styled(
            format!("╰{}╯", "─".repeat(input_width as usize)),
            Style::default().fg(self.theme.border),
        )]));

        lines.push(Line::from(""));

        // Navigation hints
        lines.push(Line::from(vec![
            Span::styled("Enter", Style::default().fg(self.theme.green)),
            Span::styled(" Save  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Esc", Style::default().fg(self.theme.yellow)),
            Span::styled(" Cancel  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Ctrl+A", Style::default().fg(self.theme.cyan)),
            Span::styled(" Select All", Style::default().fg(self.theme.text_muted)),
        ]));

        Paragraph::new(lines).render(inner, buf);
    }
}

/// Confirmation modal for deleting a queued task
pub struct TaskDeleteConfirmModal<'a> {
    theme: &'a Theme,
    task_preview: &'a str,
    selected: usize, // 0 = Cancel, 1 = Delete
}

impl<'a> TaskDeleteConfirmModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            task_preview: "",
            selected: 0,
        }
    }

    pub fn task_preview(mut self, preview: &'a str) -> Self {
        self.task_preview = preview;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for TaskDeleteConfirmModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Center the modal
        let modal_width = area.width.min(50);
        let modal_height = area.height.min(11);
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
                "Delete Task?",
                Style::default()
                    .fg(self.theme.red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.red))
            .title(title)
            .title_alignment(ratatui::layout::Alignment::Center)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Truncate task preview if too long
        let max_preview_len = (inner.width - 6) as usize;
        let preview = if self.task_preview.len() > max_preview_len {
            format!("{}...", &self.task_preview[..max_preview_len - 3])
        } else {
            self.task_preview.to_string()
        };

        // Build content
        let mut content: Vec<Line> = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "Remove this task from the queue?",
                Style::default().fg(self.theme.text_primary),
            )]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  \"", Style::default().fg(self.theme.text_muted)),
                Span::styled(preview, Style::default().fg(self.theme.text_secondary)),
                Span::styled("\"", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        // Options
        let options = [
            ("Cancel", self.theme.text_muted),
            ("Delete", self.theme.red),
        ];

        let mut option_spans = Vec::new();
        option_spans.push(Span::raw("  "));

        for (i, (name, color)) in options.iter().enumerate() {
            let is_selected = i == self.selected;

            if i > 0 {
                option_spans.push(Span::raw("    "));
            }

            let style = if is_selected {
                Style::default()
                    .fg(*color)
                    .add_modifier(Modifier::BOLD)
                    .bg(ratatui::style::Color::Rgb(45, 60, 83))
            } else {
                Style::default().fg(self.theme.text_muted)
            };

            let prefix = if is_selected { "▸ " } else { "  " };
            option_spans.push(Span::styled(format!("{}[{}]", prefix, name), style));
        }

        content.push(Line::from(option_spans));
        content.push(Line::from(""));

        // Navigation hints
        content.push(Line::from(vec![
            Span::styled("←→", Style::default().fg(self.theme.cyan)),
            Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Enter", Style::default().fg(self.theme.green)),
            Span::styled(" Confirm  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("y/n", Style::default().fg(self.theme.yellow)),
            Span::styled(" Quick", Style::default().fg(self.theme.text_muted)),
        ]));

        Paragraph::new(content).render(inner, buf);
    }
}
