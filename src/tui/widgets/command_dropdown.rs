//! Command completion dropdown widget
//!
//! Provides a visual dropdown for slash command autocompletion,
//! similar to IDE intellisense.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use super::super::commands::Command;

/// Command dropdown item with name and description
#[derive(Debug, Clone)]
pub struct CommandDropdownItem {
    /// Command name (without /)
    pub name: String,
    /// Command description
    pub description: String,
    /// Full usage string
    pub usage: String,
}

impl CommandDropdownItem {
    /// Create from a Command
    pub fn from_command(cmd: &Command) -> Self {
        Self {
            name: cmd.name.clone(),
            description: cmd.description.clone(),
            usage: cmd.usage.clone(),
        }
    }
}

/// Command dropdown state
#[derive(Debug, Default, Clone)]
pub struct CommandDropdown {
    /// Available commands to show
    items: Vec<CommandDropdownItem>,
    /// Currently selected index
    selected_index: usize,
    /// Whether the dropdown is visible
    visible: bool,
    /// Filter prefix (what user typed after /)
    filter: String,
}

impl CommandDropdown {
    /// Create a new command dropdown
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the items to display
    pub fn set_items(&mut self, items: Vec<CommandDropdownItem>) {
        self.items = items;
        self.selected_index = 0;
    }

    /// Set the filter prefix
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
    }

    /// Get the current filter
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Show the dropdown
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the dropdown
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = self
                .selected_index
                .checked_sub(1)
                .unwrap_or(self.items.len() - 1);
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.items.len();
        }
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&CommandDropdownItem> {
        self.items.get(self.selected_index)
    }

    /// Get the selected command name (with /)
    pub fn selected_command(&self) -> Option<String> {
        self.selected_item().map(|item| format!("/{}", item.name))
    }

    /// Reset dropdown state
    pub fn reset(&mut self) {
        self.items.clear();
        self.selected_index = 0;
        self.visible = false;
        self.filter.clear();
    }
}

/// Renderable command dropdown widget
pub struct CommandDropdownWidget<'a> {
    dropdown: &'a CommandDropdown,
    /// Cursor position (where to place the dropdown)
    cursor_area: Rect,
}

impl<'a> CommandDropdownWidget<'a> {
    /// Create a new command dropdown widget
    pub fn new(dropdown: &'a CommandDropdown, cursor_area: Rect) -> Self {
        Self {
            dropdown,
            cursor_area,
        }
    }

    /// Calculate the dropdown area (below the cursor)
    fn calculate_area(&self, screen: Rect) -> Rect {
        let max_items = 10;
        let height = (self.dropdown.items.len() as u16).min(max_items) + 2; // +2 for borders
        let width = 60u16.min(screen.width.saturating_sub(4));

        // Position below cursor, but ensure it fits on screen
        let x = self.cursor_area.x.min(screen.width.saturating_sub(width));
        let mut y = self.cursor_area.y + self.cursor_area.height;

        // If dropdown would go off bottom of screen, position it above cursor instead
        if y + height > screen.height {
            y = self.cursor_area.y.saturating_sub(height);
        }

        Rect::new(x, y, width, height)
    }
}

impl Widget for CommandDropdownWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.dropdown.visible || self.dropdown.items.is_empty() {
            return;
        }

        let dropdown_area = self.calculate_area(area);

        // Clear the background
        Clear.render(dropdown_area, buf);

        // Draw border with title
        let title = if !self.dropdown.filter.is_empty() {
            format!(" Commands (/{}) ", self.dropdown.filter)
        } else {
            " Commands ".to_string()
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(dropdown_area);
        block.render(dropdown_area, buf);

        if inner.height == 0 {
            return;
        }

        // Render items
        let visible_items = (inner.height as usize).min(self.dropdown.items.len());
        let start_idx = if self.dropdown.selected_index >= visible_items {
            self.dropdown
                .selected_index
                .saturating_sub(visible_items - 1)
        } else {
            0
        };

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (idx, item) in self
            .dropdown
            .items
            .iter()
            .skip(start_idx)
            .take(visible_items)
            .enumerate()
        {
            let actual_idx = start_idx + idx;
            let is_selected = actual_idx == self.dropdown.selected_index;

            let mut spans = Vec::new();

            // Selection indicator
            if is_selected {
                spans.push(Span::styled(
                    "▶ ",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                spans.push(Span::raw("  "));
            }

            // Command name
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(format!("/{}", item.name), name_style));

            // Description
            if !item.description.is_empty() {
                let desc_style = if is_selected {
                    Style::default().fg(Color::Gray)
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                spans.push(Span::styled(format!(" - {}", item.description), desc_style));
            }

            lines.push(Line::from(spans));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_dropdown_new() {
        let dropdown = CommandDropdown::new();
        assert!(!dropdown.is_visible());
        assert!(dropdown.is_empty());
    }

    #[test]
    fn test_set_items() {
        let mut dropdown = CommandDropdown::new();
        let items = vec![
            CommandDropdownItem {
                name: "help".to_string(),
                description: "Show help".to_string(),
                usage: "/help".to_string(),
            },
            CommandDropdownItem {
                name: "clear".to_string(),
                description: "Clear messages".to_string(),
                usage: "/clear".to_string(),
            },
        ];
        dropdown.set_items(items);
        assert_eq!(dropdown.len(), 2);
    }

    #[test]
    fn test_navigation() {
        let mut dropdown = CommandDropdown::new();
        let items = vec![
            CommandDropdownItem {
                name: "help".to_string(),
                description: "Show help".to_string(),
                usage: "/help".to_string(),
            },
            CommandDropdownItem {
                name: "clear".to_string(),
                description: "Clear messages".to_string(),
                usage: "/clear".to_string(),
            },
        ];
        dropdown.set_items(items);
        dropdown.show();

        assert_eq!(dropdown.selected_item().unwrap().name, "help");

        dropdown.select_next();
        assert_eq!(dropdown.selected_item().unwrap().name, "clear");

        dropdown.select_next(); // Wraps around
        assert_eq!(dropdown.selected_item().unwrap().name, "help");

        dropdown.select_previous(); // Wraps around
        assert_eq!(dropdown.selected_item().unwrap().name, "clear");
    }

    #[test]
    fn test_selected_command() {
        let mut dropdown = CommandDropdown::new();
        let items = vec![CommandDropdownItem {
            name: "help".to_string(),
            description: "Show help".to_string(),
            usage: "/help".to_string(),
        }];
        dropdown.set_items(items);
        dropdown.show();

        assert_eq!(dropdown.selected_command(), Some("/help".to_string()));
    }
}
