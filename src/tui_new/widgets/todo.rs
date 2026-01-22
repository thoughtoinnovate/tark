//! Todo Widget - displays session todo list with progress bar
//!
//! Shows a live-updating widget for the agent's todo list, including:
//! - Header with progress count
//! - Visual progress bar
//! - List of items with status icons
//!
//! Note: This widget is currently unused - sidebar.rs renders todos manually.
//! Kept for potential future use or standalone rendering.

use crate::tools::builtin::{TodoItem, TodoStatus};
use crate::tui_new::theme::Theme;
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Blink interval for in-progress indicator
const TODO_BLINK_INTERVAL_MS: u64 = 400;

/// Thread-safe global blink state for in-progress todos
static TODO_LAST_BLINK_MS: AtomicU64 = AtomicU64::new(0);
static TODO_INDICATOR_VISIBLE: AtomicBool = AtomicBool::new(true);

/// Get current timestamp in milliseconds
fn current_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Get current in-progress indicator visibility state (blinks every 400ms)
fn get_todo_indicator_visible() -> bool {
    let now = current_time_ms();
    let last = TODO_LAST_BLINK_MS.load(Ordering::Relaxed);

    if last == 0 || now.saturating_sub(last) >= TODO_BLINK_INTERVAL_MS {
        // Toggle visibility and update timestamp
        let current = TODO_INDICATOR_VISIBLE.load(Ordering::Relaxed);
        TODO_INDICATOR_VISIBLE.store(!current, Ordering::Relaxed);
        TODO_LAST_BLINK_MS.store(now, Ordering::Relaxed);
        !current
    } else {
        TODO_INDICATOR_VISIBLE.load(Ordering::Relaxed)
    }
}

/// Todo widget for displaying the session todo list
pub struct TodoWidget<'a> {
    items: &'a [TodoItem],
    theme: &'a Theme,
}

impl<'a> TodoWidget<'a> {
    /// Create a new todo widget
    pub fn new(items: &'a [TodoItem], theme: &'a Theme) -> Self {
        Self { items, theme }
    }

    /// Calculate progress statistics
    fn calculate_progress(&self) -> (usize, usize, f32) {
        let total = self.items.len();
        let completed = self
            .items
            .iter()
            .filter(|item| item.status == TodoStatus::Completed)
            .count();
        let percent = if total > 0 {
            (completed as f32 / total as f32) * 100.0
        } else {
            0.0
        };
        (completed, total, percent)
    }

    /// Get color for a todo status
    fn status_color(&self, status: TodoStatus) -> ratatui::style::Color {
        match status {
            TodoStatus::Pending => self.theme.text_muted,
            TodoStatus::InProgress => self.theme.yellow,
            TodoStatus::Completed => self.theme.green,
            TodoStatus::Cancelled => self.theme.red,
        }
    }

    /// Get status icon, with blinking for in-progress
    fn status_icon(&self, status: TodoStatus) -> &'static str {
        match status {
            TodoStatus::Pending => "○",
            TodoStatus::InProgress => {
                if get_todo_indicator_visible() {
                    "●"
                } else {
                    "○"
                }
            }
            TodoStatus::Completed => "✓",
            TodoStatus::Cancelled => "✗",
        }
    }
}

impl Widget for TodoWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.items.is_empty() {
            return;
        }

        let (completed, total, percent) = self.calculate_progress();

        // Build content lines
        let mut lines = vec![];

        // Header line: "📋 Todo" + progress count
        lines.push(Line::from(vec![
            Span::styled("📋 Todo ", Style::default().fg(self.theme.text_primary)),
            Span::raw(" ".repeat(area.width.saturating_sub(20) as usize)),
            Span::styled(
                format!("{}/{} done", completed, total),
                Style::default().fg(self.theme.text_muted),
            ),
        ]));

        // Progress bar
        let bar_width = area.width.saturating_sub(4) as usize; // Account for borders and padding
        let filled = ((bar_width as f32) * (percent / 100.0)).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let bar_str = format!("{}{}", "█".repeat(filled), "░".repeat(empty));

        let bar_color = if percent < 33.0 {
            self.theme.red
        } else if percent < 66.0 {
            self.theme.yellow
        } else {
            self.theme.green
        };

        lines.push(Line::from(Span::styled(
            bar_str,
            Style::default().fg(bar_color),
        )));
        lines.push(Line::from("")); // Spacing

        // Todo items
        for item in self.items {
            let icon = self.status_icon(item.status);
            let color = self.status_color(item.status);

            let mut item_style = Style::default().fg(self.theme.text_primary);

            // Strikethrough for cancelled items
            if item.status == TodoStatus::Cancelled {
                item_style = item_style
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::CROSSED_OUT);
            }

            // Dim completed items slightly
            if item.status == TodoStatus::Completed {
                item_style = item_style.fg(self.theme.text_secondary);
            }

            lines.push(Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&item.content, item_style),
            ]));
        }

        // Render with border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border));

        let inner = block.inner(area);
        block.render(area, buf);

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::builtin::TodoItem;

    #[test]
    fn test_calculate_progress() {
        let theme = Theme::default();
        let items = vec![
            TodoItem::with_status("t1", "Task 1", TodoStatus::Completed),
            TodoItem::with_status("t2", "Task 2", TodoStatus::Completed),
            TodoItem::with_status("t3", "Task 3", TodoStatus::InProgress),
            TodoItem::with_status("t4", "Task 4", TodoStatus::Pending),
        ];

        let widget = TodoWidget::new(&items, &theme);
        let (completed, total, percent) = widget.calculate_progress();

        assert_eq!(completed, 2);
        assert_eq!(total, 4);
        assert_eq!(percent, 50.0);
    }

    #[test]
    fn test_empty_progress() {
        let theme = Theme::default();
        let items = vec![];

        let widget = TodoWidget::new(&items, &theme);
        let (completed, total, percent) = widget.calculate_progress();

        assert_eq!(completed, 0);
        assert_eq!(total, 0);
        assert_eq!(percent, 0.0);
    }
}
