//! File dropdown widget for @files feature
//!
//! Provides a visual dropdown for file selection with fuzzy search,
//! similar to IDE file pickers.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::collections::HashSet;
use std::path::PathBuf;

/// File dropdown item with path and display info
#[derive(Debug, Clone)]
pub struct FileDropdownItem {
    /// Full path to the file
    pub path: PathBuf,
    /// Display name (relative or shortened path)
    pub display_name: String,
    /// Whether this is a directory
    pub is_directory: bool,
    /// File icon based on extension
    pub icon: &'static str,
}

impl FileDropdownItem {
    /// Create a new file dropdown item
    pub fn new(path: PathBuf, display_name: String, is_directory: bool) -> Self {
        let icon = if is_directory {
            "📁"
        } else {
            Self::get_file_icon(&display_name)
        };

        Self {
            path,
            display_name,
            is_directory,
            icon,
        }
    }

    /// Get icon for file based on extension
    fn get_file_icon(filename: &str) -> &'static str {
        if let Some(ext) = filename.rsplit('.').next() {
            match ext {
                "rs" => "🦀",
                "py" => "🐍",
                "js" | "ts" => "📜",
                "json" | "toml" | "yaml" | "yml" => "⚙️",
                "md" => "📝",
                "txt" => "📄",
                "png" | "jpg" | "jpeg" | "gif" | "webp" => "🖼️",
                _ => "📄",
            }
        } else {
            "📄"
        }
    }
}

/// File dropdown state
#[derive(Debug, Default, Clone)]
pub struct FileDropdown {
    /// Available files to show
    items: Vec<FileDropdownItem>,
    /// Filtered item indices
    filtered_indices: Vec<usize>,
    /// Currently selected index (into filtered_indices)
    selected_index: usize,
    /// Whether the dropdown is visible
    visible: bool,
    /// Filter text (what user typed after @)
    filter: String,
    /// Whether in multi-select mode
    multi_select_mode: bool,
    /// Selected file indices (in multi-select mode)
    selected_files: HashSet<usize>,
}

impl FileDropdown {
    /// Create a new file dropdown
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the items to display
    pub fn set_items(&mut self, items: Vec<FileDropdownItem>) {
        self.items = items;
        self.update_filtered_indices();
    }

    /// Set the filter text
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
        self.update_filtered_indices();
        self.selected_index = 0;
    }

    /// Get the current filter
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Update filtered indices based on current filter
    fn update_filtered_indices(&mut self) {
        if self.filter.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.filtered_indices = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.display_name.to_lowercase().contains(&filter_lower)
                        || item
                            .path
                            .to_string_lossy()
                            .to_lowercase()
                            .contains(&filter_lower)
                })
                .map(|(idx, _)| idx)
                .collect();
        }
    }

    /// Show the dropdown
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the dropdown
    pub fn hide(&mut self) {
        self.visible = false;
        self.filter.clear();
        self.selected_files.clear();
        self.multi_select_mode = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the number of filtered items
    pub fn len(&self) -> usize {
        self.filtered_indices.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.filtered_indices.is_empty()
    }

    /// Move selection up
    pub fn select_previous(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = self
                .selected_index
                .checked_sub(1)
                .unwrap_or(self.filtered_indices.len() - 1);
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = (self.selected_index + 1) % self.filtered_indices.len();
        }
    }

    /// Toggle multi-select mode
    pub fn toggle_multi_select_mode(&mut self) {
        self.multi_select_mode = !self.multi_select_mode;
    }

    /// Toggle selection of current item (multi-select mode)
    pub fn toggle_current(&mut self) {
        if let Some(&item_idx) = self.filtered_indices.get(self.selected_index) {
            if self.selected_files.contains(&item_idx) {
                self.selected_files.remove(&item_idx);
            } else {
                self.selected_files.insert(item_idx);
            }
        }
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&FileDropdownItem> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.items.get(idx))
    }

    /// Get the selected file path
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.selected_item().map(|item| item.path.clone())
    }

    /// Get all selected file paths (in multi-select mode)
    pub fn selected_paths(&self) -> Vec<PathBuf> {
        if self.multi_select_mode {
            self.selected_files
                .iter()
                .filter_map(|&idx| self.items.get(idx).map(|item| item.path.clone()))
                .collect()
        } else {
            self.selected_path().into_iter().collect()
        }
    }

    /// Confirm selection and return selected path(s)
    pub fn confirm(&mut self) -> Vec<PathBuf> {
        let paths = self.selected_paths();
        self.hide();
        paths
    }

    /// Reset dropdown state
    pub fn reset(&mut self) {
        self.items.clear();
        self.filtered_indices.clear();
        self.selected_index = 0;
        self.visible = false;
        self.filter.clear();
        self.selected_files.clear();
        self.multi_select_mode = false;
    }
}

/// Renderable file dropdown widget
pub struct FileDropdownWidget<'a> {
    dropdown: &'a FileDropdown,
    /// Cursor area for positioning
    cursor_area: Rect,
}

impl<'a> FileDropdownWidget<'a> {
    /// Create a new file dropdown widget
    pub fn new(dropdown: &'a FileDropdown, cursor_area: Rect) -> Self {
        Self {
            dropdown,
            cursor_area,
        }
    }

    /// Calculate the area for the dropdown (above or below cursor)
    fn calculate_area(&self, area: Rect) -> Rect {
        let width = 50.min(area.width.saturating_sub(4));
        let max_height = 10.min(self.dropdown.filtered_indices.len() as u16 + 3);

        // Position above cursor if there's room, otherwise below
        let x = self.cursor_area.x.min(area.width.saturating_sub(width));
        let y = if self.cursor_area.y > max_height {
            self.cursor_area.y.saturating_sub(max_height)
        } else {
            (self.cursor_area.y + 1).min(area.height.saturating_sub(max_height))
        };

        Rect::new(x, y, width, max_height)
    }
}

impl Widget for FileDropdownWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.dropdown.visible {
            return;
        }

        let dropdown_area = self.calculate_area(area);

        // Clear the background
        Clear.render(dropdown_area, buf);

        // Draw border
        let title = if self.dropdown.multi_select_mode {
            " Select Files (Space to toggle) "
        } else {
            " Select File "
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(dropdown_area);
        block.render(dropdown_area, buf);

        if inner.height < 1 {
            return;
        }

        // Render items
        if self.dropdown.filtered_indices.is_empty() {
            let no_items = Paragraph::new("No matching files")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            no_items.render(inner, buf);
            return;
        }

        let mut lines: Vec<Line<'static>> = Vec::new();
        let visible_count = (inner.height as usize).min(self.dropdown.filtered_indices.len());

        for (display_idx, &item_idx) in self
            .dropdown
            .filtered_indices
            .iter()
            .take(visible_count)
            .enumerate()
        {
            let item = &self.dropdown.items[item_idx];
            let is_selected = display_idx == self.dropdown.selected_index;
            let is_marked = self.dropdown.selected_files.contains(&item_idx);

            let mut spans = Vec::new();

            // Selection indicator
            let indicator = if is_selected {
                "▶ "
            } else if is_marked && self.dropdown.multi_select_mode {
                "✓ "
            } else {
                "  "
            };

            spans.push(Span::styled(
                indicator.to_string(),
                Style::default().fg(if is_selected {
                    Color::Cyan
                } else if is_marked {
                    Color::Green
                } else {
                    Color::DarkGray
                }),
            ));

            // Icon
            spans.push(Span::styled(format!("{} ", item.icon), Style::default()));

            // Display name
            let name_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            spans.push(Span::styled(item.display_name.clone(), name_style));

            lines.push(Line::from(spans));
        }

        // Show count if there are more items
        if self.dropdown.filtered_indices.len() > visible_count {
            let remaining = self.dropdown.filtered_indices.len() - visible_count;
            lines.push(Line::from(vec![Span::styled(
                format!("  ... {} more files", remaining),
                Style::default().fg(Color::DarkGray),
            )]));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_dropdown_new() {
        let dropdown = FileDropdown::new();
        assert!(!dropdown.is_visible());
        assert!(dropdown.is_empty());
    }

    #[test]
    fn test_set_filter() {
        let mut dropdown = FileDropdown::new();
        let items = vec![
            FileDropdownItem::new(
                PathBuf::from("src/main.rs"),
                "src/main.rs".to_string(),
                false,
            ),
            FileDropdownItem::new(PathBuf::from("src/lib.rs"), "src/lib.rs".to_string(), false),
        ];
        dropdown.set_items(items);

        assert_eq!(dropdown.len(), 2);

        dropdown.set_filter("main");
        assert_eq!(dropdown.len(), 1);
    }

    #[test]
    fn test_multi_select() {
        let mut dropdown = FileDropdown::new();
        let items = vec![
            FileDropdownItem::new(PathBuf::from("file1.txt"), "file1.txt".to_string(), false),
            FileDropdownItem::new(PathBuf::from("file2.txt"), "file2.txt".to_string(), false),
        ];
        dropdown.set_items(items);
        dropdown.show();

        dropdown.toggle_multi_select_mode();
        assert!(dropdown.multi_select_mode);

        dropdown.toggle_current(); // Select first
        dropdown.select_next();
        dropdown.toggle_current(); // Select second

        let paths = dropdown.selected_paths();
        assert_eq!(paths.len(), 2);
    }
}
