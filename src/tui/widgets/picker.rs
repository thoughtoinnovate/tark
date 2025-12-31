//! Picker widget for selecting items from a list
//!
//! Provides a modal picker UI for selecting sessions, providers, models, etc.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

/// A selectable item in the picker
#[derive(Debug, Clone)]
pub struct PickerItem {
    /// Unique identifier
    pub id: String,
    /// Display label
    pub label: String,
    /// Optional description
    pub description: Option<String>,
    /// Optional icon/indicator
    pub icon: Option<String>,
    /// Whether this item is currently active/selected
    pub is_active: bool,
}

impl PickerItem {
    /// Create a new picker item
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            icon: None,
            is_active: false,
        }
    }

    /// Set the description
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Set the icon
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Set the active state
    pub fn with_active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
}

/// Picker widget state
#[derive(Debug, Default)]
pub struct Picker {
    /// Title of the picker
    title: String,
    /// Items to select from
    items: Vec<PickerItem>,
    /// Currently highlighted index
    selected_index: usize,
    /// Whether the picker is visible
    visible: bool,
    /// Filter text for searching
    filter: String,
    /// Filtered item indices
    filtered_indices: Vec<usize>,
}

impl Picker {
    /// Create a new picker
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            selected_index: 0,
            visible: false,
            filter: String::new(),
            filtered_indices: Vec::new(),
        }
    }

    /// Set the items
    pub fn with_items(mut self, items: Vec<PickerItem>) -> Self {
        self.items = items;
        self.update_filter();
        self
    }

    /// Add an item
    pub fn add_item(&mut self, item: PickerItem) {
        self.items.push(item);
        self.update_filter();
    }

    /// Set items
    pub fn set_items(&mut self, items: Vec<PickerItem>) {
        self.items = items;
        self.selected_index = 0;
        self.update_filter();
    }

    /// Clear all items
    pub fn clear_items(&mut self) {
        self.items.clear();
        self.filtered_indices.clear();
        self.selected_index = 0;
    }

    /// Show the picker
    pub fn show(&mut self) {
        self.visible = true;
        self.selected_index = 0;
        self.filter.clear();
        self.update_filter();
    }

    /// Hide the picker
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the title
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Set the title
    pub fn set_title(&mut self, title: impl Into<String>) {
        self.title = title.into();
    }

    /// Get the filter text
    pub fn filter(&self) -> &str {
        &self.filter
    }

    /// Set the filter text
    pub fn set_filter(&mut self, filter: impl Into<String>) {
        self.filter = filter.into();
        self.update_filter();
    }

    /// Add a character to the filter
    pub fn filter_push(&mut self, c: char) {
        self.filter.push(c);
        self.update_filter();
    }

    /// Remove the last character from the filter
    pub fn filter_pop(&mut self) {
        self.filter.pop();
        self.update_filter();
    }

    /// Clear the filter
    pub fn filter_clear(&mut self) {
        self.filter.clear();
        self.update_filter();
    }

    /// Update filtered indices based on current filter
    fn update_filter(&mut self) {
        if self.filter.is_empty() {
            self.filtered_indices = (0..self.items.len()).collect();
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.filtered_indices = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| {
                    item.label.to_lowercase().contains(&filter_lower)
                        || item
                            .description
                            .as_ref()
                            .map(|d| d.to_lowercase().contains(&filter_lower))
                            .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .collect();
        }

        // Reset selection if out of bounds
        if self.selected_index >= self.filtered_indices.len() {
            self.selected_index = 0;
        }
    }

    /// Get the number of visible items
    pub fn visible_count(&self) -> usize {
        self.filtered_indices.len()
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

    /// Select first item
    pub fn select_first(&mut self) {
        self.selected_index = 0;
    }

    /// Select last item
    pub fn select_last(&mut self) {
        if !self.filtered_indices.is_empty() {
            self.selected_index = self.filtered_indices.len() - 1;
        }
    }

    /// Get the currently selected item
    pub fn selected_item(&self) -> Option<&PickerItem> {
        self.filtered_indices
            .get(self.selected_index)
            .and_then(|&idx| self.items.get(idx))
    }

    /// Get the ID of the currently selected item
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_item().map(|item| item.id.as_str())
    }

    /// Confirm selection and return the selected item ID
    pub fn confirm(&mut self) -> Option<String> {
        let id = self.selected_id().map(|s| s.to_string());
        self.hide();
        id
    }

    /// Cancel and hide the picker
    pub fn cancel(&mut self) {
        self.hide();
    }
}

/// Renderable picker widget
pub struct PickerWidget<'a> {
    picker: &'a Picker,
}

impl<'a> PickerWidget<'a> {
    /// Create a new picker widget
    pub fn new(picker: &'a Picker) -> Self {
        Self { picker }
    }

    /// Calculate the area for the picker (centered modal)
    fn calculate_area(&self, area: Rect) -> Rect {
        let width = (area.width * 60 / 100).clamp(30, 60);
        let height = (self.picker.filtered_indices.len() as u16 + 4)
            .min(area.height * 80 / 100)
            .max(5);

        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        Rect::new(x, y, width, height)
    }
}

impl Widget for PickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.picker.visible {
            return;
        }

        let picker_area = self.calculate_area(area);

        // Clear the background
        Clear.render(picker_area, buf);

        // Draw border
        let block = Block::default()
            .title(format!(" {} ", self.picker.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(picker_area);
        block.render(picker_area, buf);

        if inner.height < 2 {
            return;
        }

        // Layout: filter input + items
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Filter input
                Constraint::Min(1),    // Items
            ])
            .split(inner);

        // Render filter input
        let filter_text = if self.picker.filter.is_empty() {
            "Type to filter...".to_string()
        } else {
            self.picker.filter.clone()
        };
        let filter_style = if self.picker.filter.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };
        let filter_line = Line::from(vec![
            Span::styled("> ", Style::default().fg(Color::Cyan)),
            Span::styled(filter_text, filter_style),
        ]);
        Paragraph::new(filter_line).render(chunks[0], buf);

        // Render items
        if self.picker.filtered_indices.is_empty() {
            let no_items = Paragraph::new("No matching items")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            no_items.render(chunks[1], buf);
            return;
        }

        let mut lines: Vec<Line<'static>> = Vec::new();
        for (display_idx, &item_idx) in self.picker.filtered_indices.iter().enumerate() {
            let item = &self.picker.items[item_idx];
            let is_selected = display_idx == self.picker.selected_index;

            let mut spans = Vec::new();

            // Selection indicator
            let indicator = if is_selected { "▶ " } else { "  " };
            spans.push(Span::styled(
                indicator.to_string(),
                Style::default().fg(Color::Cyan),
            ));

            // Icon if present
            if let Some(ref icon) = item.icon {
                spans.push(Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(Color::Yellow),
                ));
            }

            // Active indicator
            if item.is_active {
                spans.push(Span::styled("● ", Style::default().fg(Color::Green)));
            }

            // Label
            let label_style = if is_selected {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };
            spans.push(Span::styled(item.label.clone(), label_style));

            // Description if present
            if let Some(ref desc) = item.description {
                spans.push(Span::styled(
                    format!(" - {}", desc),
                    Style::default().fg(Color::DarkGray),
                ));
            }

            lines.push(Line::from(spans));
        }

        // Only show items that fit
        let visible_lines: Vec<Line<'static>> =
            lines.into_iter().take(chunks[1].height as usize).collect();

        let items_paragraph = Paragraph::new(visible_lines);
        items_paragraph.render(chunks[1], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picker_item_creation() {
        let item = PickerItem::new("id1", "Label 1")
            .with_description("Description")
            .with_icon("🔧")
            .with_active(true);

        assert_eq!(item.id, "id1");
        assert_eq!(item.label, "Label 1");
        assert_eq!(item.description, Some("Description".to_string()));
        assert_eq!(item.icon, Some("🔧".to_string()));
        assert!(item.is_active);
    }

    #[test]
    fn test_picker_creation() {
        let picker = Picker::new("Select Item").with_items(vec![
            PickerItem::new("1", "Item 1"),
            PickerItem::new("2", "Item 2"),
            PickerItem::new("3", "Item 3"),
        ]);

        assert_eq!(picker.title(), "Select Item");
        assert_eq!(picker.visible_count(), 3);
        assert!(!picker.is_visible());
    }

    #[test]
    fn test_picker_show_hide() {
        let mut picker = Picker::new("Test");
        assert!(!picker.is_visible());

        picker.show();
        assert!(picker.is_visible());

        picker.hide();
        assert!(!picker.is_visible());
    }

    #[test]
    fn test_picker_navigation() {
        let mut picker = Picker::new("Test").with_items(vec![
            PickerItem::new("1", "Item 1"),
            PickerItem::new("2", "Item 2"),
            PickerItem::new("3", "Item 3"),
        ]);

        picker.show();
        assert_eq!(picker.selected_id(), Some("1"));

        picker.select_next();
        assert_eq!(picker.selected_id(), Some("2"));

        picker.select_next();
        assert_eq!(picker.selected_id(), Some("3"));

        picker.select_next(); // Wraps around
        assert_eq!(picker.selected_id(), Some("1"));

        picker.select_previous(); // Wraps around
        assert_eq!(picker.selected_id(), Some("3"));

        picker.select_first();
        assert_eq!(picker.selected_id(), Some("1"));

        picker.select_last();
        assert_eq!(picker.selected_id(), Some("3"));
    }

    #[test]
    fn test_picker_filter() {
        let mut picker = Picker::new("Test").with_items(vec![
            PickerItem::new("1", "Apple"),
            PickerItem::new("2", "Banana"),
            PickerItem::new("3", "Cherry"),
            PickerItem::new("4", "Apricot"),
        ]);

        picker.show();
        assert_eq!(picker.visible_count(), 4);

        picker.set_filter("ap");
        assert_eq!(picker.visible_count(), 2); // Apple, Apricot

        picker.filter_clear();
        assert_eq!(picker.visible_count(), 4);

        picker.filter_push('b');
        assert_eq!(picker.visible_count(), 1); // Banana

        picker.filter_pop();
        assert_eq!(picker.visible_count(), 4);
    }

    #[test]
    fn test_picker_confirm() {
        let mut picker = Picker::new("Test").with_items(vec![
            PickerItem::new("id1", "Item 1"),
            PickerItem::new("id2", "Item 2"),
        ]);

        picker.show();
        picker.select_next();

        let selected = picker.confirm();
        assert_eq!(selected, Some("id2".to_string()));
        assert!(!picker.is_visible());
    }

    #[test]
    fn test_picker_cancel() {
        let mut picker = Picker::new("Test").with_items(vec![PickerItem::new("id1", "Item 1")]);

        picker.show();
        assert!(picker.is_visible());

        picker.cancel();
        assert!(!picker.is_visible());
    }

    #[test]
    fn test_picker_empty() {
        let mut picker = Picker::new("Test");
        picker.show();

        assert_eq!(picker.visible_count(), 0);
        assert!(picker.selected_item().is_none());
        assert!(picker.selected_id().is_none());
    }

    #[test]
    fn test_picker_filter_with_description() {
        let mut picker = Picker::new("Test").with_items(vec![
            PickerItem::new("1", "Item 1").with_description("First item"),
            PickerItem::new("2", "Item 2").with_description("Second item"),
        ]);

        picker.show();
        picker.set_filter("first");
        assert_eq!(picker.visible_count(), 1);
        assert_eq!(picker.selected_id(), Some("1"));
    }
}
