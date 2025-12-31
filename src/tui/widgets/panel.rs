//! Panel widget for displaying tasks, notifications, and files
//!
//! Provides a collapsible side panel with multiple sections
//! and vim-style navigation support.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph, Widget},
};

/// Status of a task item
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

impl TaskStatus {
    /// Get the display icon for this status
    pub fn icon(&self) -> &'static str {
        match self {
            TaskStatus::Pending => "○",
            TaskStatus::Running => "●",
            TaskStatus::Completed => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Skipped => "⊘",
        }
    }

    /// Get the color for this status
    pub fn color(&self) -> Color {
        match self {
            TaskStatus::Pending => Color::DarkGray,
            TaskStatus::Running => Color::Yellow,
            TaskStatus::Completed => Color::Green,
            TaskStatus::Failed => Color::Red,
            TaskStatus::Skipped => Color::Gray,
        }
    }
}

/// An item in a panel section
#[derive(Debug, Clone)]
pub enum SectionItem {
    /// A task with status
    Task { name: String, status: TaskStatus },
    /// A notification message
    Notification {
        message: String,
        level: NotificationLevel,
    },
    /// A file path
    File { path: String, modified: bool },
}

impl SectionItem {
    /// Create a new task item
    pub fn task(name: impl Into<String>, status: TaskStatus) -> Self {
        Self::Task {
            name: name.into(),
            status,
        }
    }

    /// Create a new notification item
    pub fn notification(message: impl Into<String>, level: NotificationLevel) -> Self {
        Self::Notification {
            message: message.into(),
            level,
        }
    }

    /// Create a new file item
    pub fn file(path: impl Into<String>, modified: bool) -> Self {
        Self::File {
            path: path.into(),
            modified,
        }
    }

    /// Get the display text for this item
    fn display_text(&self) -> String {
        match self {
            SectionItem::Task { name, status } => {
                format!("{} {}", status.icon(), name)
            }
            SectionItem::Notification { message, level } => {
                format!("{} {}", level.icon(), message)
            }
            SectionItem::File { path, modified } => {
                let indicator = if *modified { "●" } else { " " };
                format!("{} {}", indicator, path)
            }
        }
    }

    /// Get the style for this item
    fn style(&self, is_selected: bool) -> Style {
        let base_style = match self {
            SectionItem::Task { status, .. } => Style::default().fg(status.color()),
            SectionItem::Notification { level, .. } => Style::default().fg(level.color()),
            SectionItem::File { modified, .. } => {
                if *modified {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::White)
                }
            }
        };

        if is_selected {
            base_style.add_modifier(Modifier::REVERSED)
        } else {
            base_style
        }
    }
}

/// Notification level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationLevel {
    #[default]
    Info,
    Warning,
    Error,
    Success,
}

impl NotificationLevel {
    /// Get the display icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            NotificationLevel::Info => "ℹ",
            NotificationLevel::Warning => "⚠",
            NotificationLevel::Error => "✗",
            NotificationLevel::Success => "✓",
        }
    }

    /// Get the color for this level
    pub fn color(&self) -> Color {
        match self {
            NotificationLevel::Info => Color::Cyan,
            NotificationLevel::Warning => Color::Yellow,
            NotificationLevel::Error => Color::Red,
            NotificationLevel::Success => Color::Green,
        }
    }
}

/// A section in the panel
#[derive(Debug, Clone)]
pub struct PanelSection {
    /// Section title
    pub title: String,
    /// Items in this section
    pub items: Vec<SectionItem>,
    /// Whether the section is expanded
    pub expanded: bool,
}

impl PanelSection {
    /// Create a new panel section
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            items: Vec::new(),
            expanded: true,
        }
    }

    /// Add an item to the section
    pub fn with_item(mut self, item: SectionItem) -> Self {
        self.items.push(item);
        self
    }

    /// Add multiple items to the section
    pub fn with_items(mut self, items: impl IntoIterator<Item = SectionItem>) -> Self {
        self.items.extend(items);
        self
    }

    /// Set the expanded state
    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// Toggle the expanded state
    pub fn toggle(&mut self) {
        self.expanded = !self.expanded;
    }

    /// Get the number of items
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Check if the section is empty
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the visible line count (header + items if expanded)
    pub fn visible_lines(&self) -> usize {
        if self.expanded {
            1 + self.items.len()
        } else {
            1
        }
    }
}

/// Panel widget state
#[derive(Debug, Default)]
pub struct PanelWidget {
    /// Sections in the panel
    sections: Vec<PanelSection>,
    /// Currently focused section index
    focused_section: usize,
    /// Currently focused item index within the section
    focused_item: Option<usize>,
    /// Whether the panel is visible
    visible: bool,
    /// Panel width
    width: u16,
}

impl PanelWidget {
    /// Create a new panel widget
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            focused_section: 0,
            focused_item: None,
            visible: true,
            width: 30,
        }
    }

    /// Add a section to the panel
    pub fn with_section(mut self, section: PanelSection) -> Self {
        self.sections.push(section);
        self
    }

    /// Set the panel width
    pub fn with_width(mut self, width: u16) -> Self {
        self.width = width;
        self
    }

    /// Set visibility
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Get the sections
    pub fn sections(&self) -> &[PanelSection] {
        &self.sections
    }

    /// Get mutable sections
    pub fn sections_mut(&mut self) -> &mut Vec<PanelSection> {
        &mut self.sections
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Toggle visibility
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    /// Get the panel width
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Get the focused section index
    pub fn focused_section(&self) -> usize {
        self.focused_section
    }

    /// Get the focused item index
    pub fn focused_item(&self) -> Option<usize> {
        self.focused_item
    }

    /// Move focus to the next item (j key)
    pub fn focus_next(&mut self) {
        if self.sections.is_empty() {
            return;
        }

        let section = &self.sections[self.focused_section];

        match self.focused_item {
            None => {
                // Focus first item in current section if expanded
                if section.expanded && !section.items.is_empty() {
                    self.focused_item = Some(0);
                } else {
                    // Move to next section
                    self.focus_next_section();
                }
            }
            Some(idx) => {
                if idx + 1 < section.items.len() {
                    // Move to next item in section
                    self.focused_item = Some(idx + 1);
                } else {
                    // Move to next section
                    self.focus_next_section();
                }
            }
        }
    }

    /// Move focus to the previous item (k key)
    pub fn focus_previous(&mut self) {
        if self.sections.is_empty() {
            return;
        }

        match self.focused_item {
            None => {
                // Move to previous section's last item
                self.focus_previous_section();
            }
            Some(0) => {
                // Move to section header
                self.focused_item = None;
            }
            Some(idx) => {
                // Move to previous item
                self.focused_item = Some(idx - 1);
            }
        }
    }

    /// Move focus to the next section
    fn focus_next_section(&mut self) {
        if self.focused_section + 1 < self.sections.len() {
            self.focused_section += 1;
            self.focused_item = None;
        }
    }

    /// Move focus to the previous section
    fn focus_previous_section(&mut self) {
        if self.focused_section > 0 {
            self.focused_section -= 1;
            let section = &self.sections[self.focused_section];
            if section.expanded && !section.items.is_empty() {
                self.focused_item = Some(section.items.len() - 1);
            } else {
                self.focused_item = None;
            }
        }
    }

    /// Toggle the current section's expanded state (zo/zc keys)
    pub fn toggle_section(&mut self) {
        if let Some(section) = self.sections.get_mut(self.focused_section) {
            section.toggle();
            // Clear item focus if collapsing
            if !section.expanded {
                self.focused_item = None;
            }
        }
    }

    /// Expand the current section (zo key)
    pub fn expand_section(&mut self) {
        if let Some(section) = self.sections.get_mut(self.focused_section) {
            section.expanded = true;
        }
    }

    /// Collapse the current section (zc key)
    pub fn collapse_section(&mut self) {
        if let Some(section) = self.sections.get_mut(self.focused_section) {
            section.expanded = false;
            self.focused_item = None;
        }
    }

    /// Focus the first item (gg key)
    pub fn focus_first(&mut self) {
        self.focused_section = 0;
        self.focused_item = None;
    }

    /// Focus the last item (G key)
    pub fn focus_last(&mut self) {
        if self.sections.is_empty() {
            return;
        }
        self.focused_section = self.sections.len() - 1;
        let section = &self.sections[self.focused_section];
        if section.expanded && !section.items.is_empty() {
            self.focused_item = Some(section.items.len() - 1);
        } else {
            self.focused_item = None;
        }
    }

    /// Get the currently focused item
    pub fn get_focused_item(&self) -> Option<&SectionItem> {
        self.focused_item.and_then(|idx| {
            self.sections
                .get(self.focused_section)
                .and_then(|s| s.items.get(idx))
        })
    }

    /// Add a section
    pub fn add_section(&mut self, section: PanelSection) {
        self.sections.push(section);
    }

    /// Clear all sections
    pub fn clear(&mut self) {
        self.sections.clear();
        self.focused_section = 0;
        self.focused_item = None;
    }
}

/// Renderable panel widget
pub struct PanelWidgetRenderer<'a> {
    panel: &'a PanelWidget,
    block: Option<Block<'a>>,
}

impl<'a> PanelWidgetRenderer<'a> {
    /// Create a new panel widget renderer
    pub fn new(panel: &'a PanelWidget) -> Self {
        Self { panel, block: None }
    }

    /// Set the block for the widget
    pub fn block(mut self, block: Block<'a>) -> Self {
        self.block = Some(block);
        self
    }
}

impl Widget for PanelWidgetRenderer<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.panel.visible || area.width == 0 || area.height == 0 {
            return;
        }

        // Calculate inner area
        let inner = if let Some(ref block) = self.block {
            let inner = block.inner(area);
            block.clone().render(area, buf);
            inner
        } else {
            area
        };

        if self.panel.sections.is_empty() {
            let empty_text = Paragraph::new("No items").style(Style::default().fg(Color::DarkGray));
            empty_text.render(inner, buf);
            return;
        }

        let mut lines: Vec<Line<'static>> = Vec::new();

        for (section_idx, section) in self.panel.sections.iter().enumerate() {
            let is_section_focused =
                section_idx == self.panel.focused_section && self.panel.focused_item.is_none();

            // Section header
            let expand_icon = if section.expanded { "▼" } else { "▶" };
            let header_style = if is_section_focused {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            };

            let count_str = format!(" ({})", section.items.len());
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", expand_icon), header_style),
                Span::styled(section.title.clone(), header_style),
                Span::styled(count_str, Style::default().fg(Color::DarkGray)),
            ]));

            // Section items (if expanded)
            if section.expanded {
                for (item_idx, item) in section.items.iter().enumerate() {
                    let is_item_focused = section_idx == self.panel.focused_section
                        && self.panel.focused_item == Some(item_idx);

                    let item_style = item.style(is_item_focused);
                    let text = format!("  {}", item.display_text());
                    lines.push(Line::from(Span::styled(text, item_style)));
                }
            }

            // Add spacing between sections
            if section_idx < self.panel.sections.len() - 1 {
                lines.push(Line::from(""));
            }
        }

        // Render lines
        let visible_lines: Vec<Line<'static>> =
            lines.into_iter().take(inner.height as usize).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_status_icon() {
        assert_eq!(TaskStatus::Pending.icon(), "○");
        assert_eq!(TaskStatus::Running.icon(), "●");
        assert_eq!(TaskStatus::Completed.icon(), "✓");
        assert_eq!(TaskStatus::Failed.icon(), "✗");
        assert_eq!(TaskStatus::Skipped.icon(), "⊘");
    }

    #[test]
    fn test_notification_level_icon() {
        assert_eq!(NotificationLevel::Info.icon(), "ℹ");
        assert_eq!(NotificationLevel::Warning.icon(), "⚠");
        assert_eq!(NotificationLevel::Error.icon(), "✗");
        assert_eq!(NotificationLevel::Success.icon(), "✓");
    }

    #[test]
    fn test_section_item_creation() {
        let task = SectionItem::task("Build project", TaskStatus::Running);
        let notification = SectionItem::notification("Build complete", NotificationLevel::Success);
        let file = SectionItem::file("src/main.rs", true);

        match task {
            SectionItem::Task { name, status } => {
                assert_eq!(name, "Build project");
                assert_eq!(status, TaskStatus::Running);
            }
            _ => panic!("Expected Task"),
        }

        match notification {
            SectionItem::Notification { message, level } => {
                assert_eq!(message, "Build complete");
                assert_eq!(level, NotificationLevel::Success);
            }
            _ => panic!("Expected Notification"),
        }

        match file {
            SectionItem::File { path, modified } => {
                assert_eq!(path, "src/main.rs");
                assert!(modified);
            }
            _ => panic!("Expected File"),
        }
    }

    #[test]
    fn test_panel_section() {
        let section = PanelSection::new("Tasks")
            .with_item(SectionItem::task("Task 1", TaskStatus::Pending))
            .with_item(SectionItem::task("Task 2", TaskStatus::Running))
            .with_expanded(true);

        assert_eq!(section.title, "Tasks");
        assert_eq!(section.len(), 2);
        assert!(section.expanded);
        assert_eq!(section.visible_lines(), 3); // header + 2 items
    }

    #[test]
    fn test_panel_section_toggle() {
        let mut section =
            PanelSection::new("Tasks").with_item(SectionItem::task("Task 1", TaskStatus::Pending));

        assert!(section.expanded);
        section.toggle();
        assert!(!section.expanded);
        assert_eq!(section.visible_lines(), 1); // header only
    }

    #[test]
    fn test_panel_widget_navigation() {
        let mut panel = PanelWidget::new()
            .with_section(
                PanelSection::new("Tasks")
                    .with_item(SectionItem::task("Task 1", TaskStatus::Pending))
                    .with_item(SectionItem::task("Task 2", TaskStatus::Running)),
            )
            .with_section(
                PanelSection::new("Files").with_item(SectionItem::file("main.rs", false)),
            );

        // Initial state
        assert_eq!(panel.focused_section(), 0);
        assert_eq!(panel.focused_item(), None);

        // Navigate down
        panel.focus_next();
        assert_eq!(panel.focused_section(), 0);
        assert_eq!(panel.focused_item(), Some(0));

        panel.focus_next();
        assert_eq!(panel.focused_item(), Some(1));

        panel.focus_next();
        assert_eq!(panel.focused_section(), 1);
        assert_eq!(panel.focused_item(), None);

        // Navigate up
        panel.focus_previous();
        assert_eq!(panel.focused_section(), 0);
        assert_eq!(panel.focused_item(), Some(1));

        // First/Last
        panel.focus_first();
        assert_eq!(panel.focused_section(), 0);
        assert_eq!(panel.focused_item(), None);

        panel.focus_last();
        assert_eq!(panel.focused_section(), 1);
        assert_eq!(panel.focused_item(), Some(0));
    }

    #[test]
    fn test_panel_widget_toggle_section() {
        let mut panel = PanelWidget::new().with_section(
            PanelSection::new("Tasks").with_item(SectionItem::task("Task 1", TaskStatus::Pending)),
        );

        assert!(panel.sections[0].expanded);
        panel.toggle_section();
        assert!(!panel.sections[0].expanded);
    }

    #[test]
    fn test_panel_widget_visibility() {
        let mut panel = PanelWidget::new();
        assert!(panel.is_visible());

        panel.toggle_visible();
        assert!(!panel.is_visible());

        panel.toggle_visible();
        assert!(panel.is_visible());
    }

    #[test]
    fn test_panel_widget_get_focused_item() {
        let mut panel = PanelWidget::new().with_section(
            PanelSection::new("Tasks").with_item(SectionItem::task("Task 1", TaskStatus::Pending)),
        );

        // No item focused initially
        assert!(panel.get_focused_item().is_none());

        // Focus first item
        panel.focus_next();
        let item = panel.get_focused_item();
        assert!(item.is_some());
        match item.unwrap() {
            SectionItem::Task { name, .. } => assert_eq!(name, "Task 1"),
            _ => panic!("Expected Task"),
        }
    }
}
