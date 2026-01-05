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

// ============================================================================
// Enhanced Panel Data Structures (Requirements 3.1, 3.2, 4.1, 5.1, 6.1)
// ============================================================================

/// Data for the enhanced panel display
#[derive(Debug, Clone, Default)]
pub struct EnhancedPanelData {
    /// Session information
    pub session: SessionInfo,
    /// Context usage information
    pub context: ContextInfo,
    /// Active tasks
    pub tasks: Vec<TaskItem>,
    /// Modified files
    pub files: Vec<FileItem>,
}

/// Cost breakdown entry for a specific model/provider combination
#[derive(Debug, Clone, Default)]
pub struct CostBreakdownEntry {
    pub provider: String,
    pub model: String,
    pub cost: f64,
}

/// Session information displayed in the panel
#[derive(Debug, Clone, Default)]
pub struct SessionInfo {
    /// Session name
    pub name: String,
    /// Model name (e.g., "gpt-4o", "claude-3")
    pub model: String,
    /// Provider name (e.g., "openai", "anthropic")
    pub provider: String,
    /// Total cost in USD
    pub cost: f64,
    /// Active LSP languages (e.g., ["rust", "lua"])
    /// Cost breakdown by model/provider
    pub cost_breakdown: Vec<CostBreakdownEntry>,
    pub lsp_languages: Vec<String>,
}

/// Context usage information displayed in the panel
#[derive(Debug, Clone, Default)]
pub struct ContextInfo {
    /// Number of tokens used
    pub used_tokens: u32,
    /// Maximum tokens available
    pub max_tokens: u32,
    /// Usage percentage (0.0 to 100.0)
    pub usage_percent: f32,
    /// Whether current usage exceeds model limit (e.g., after switching to smaller model)
    pub over_limit: bool,
}

/// A task item displayed in the Tasks section
#[derive(Debug, Clone)]
pub struct TaskItem {
    /// Task description
    pub description: String,
    /// Task status
    pub status: TaskStatus,
}

impl Default for TaskItem {
    fn default() -> Self {
        Self {
            description: String::new(),
            status: TaskStatus::Pending,
        }
    }
}

/// A file item displayed in the Modified Files section
#[derive(Debug, Clone)]
pub struct FileItem {
    /// Relative path to the file
    pub path: String,
    /// Whether the file has been modified (always true for files in this list)
    pub modified: bool,
}

impl Default for FileItem {
    fn default() -> Self {
        Self {
            path: String::new(),
            modified: true,
        }
    }
}

// ============================================================================
// Panel Section State (Requirements 3.6, 3.7, 4.7, 4.8, 5.6-5.9, 6.5-6.8)
// ============================================================================

/// Enum representing the different panel sections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnhancedPanelSection {
    #[default]
    Session,
    Context,
    Tasks,
    Files,
}

/// State for panel sections with accordion behavior and scroll support
#[derive(Debug, Clone)]
pub struct PanelSectionState {
    /// Whether the Session section is expanded
    pub session_expanded: bool,
    /// Whether the Context section is expanded
    pub context_expanded: bool,
    /// Whether the Tasks section is expanded
    pub tasks_expanded: bool,
    /// Whether the Files section is expanded
    pub files_expanded: bool,
    /// Scroll offset for the Tasks section
    pub tasks_scroll: usize,
    /// Scroll offset for the Files section
    pub files_scroll: usize,
    /// Currently focused section (for keyboard navigation)
    pub focused_section: EnhancedPanelSection,
    /// Whether the cost breakdown accordion is expanded
    pub cost_breakdown_expanded: bool,
}

impl Default for PanelSectionState {
    fn default() -> Self {
        Self::new()
    }
}

impl PanelSectionState {
    /// Create a new PanelSectionState with all sections expanded
    pub fn new() -> Self {
        Self {
            session_expanded: true,
            context_expanded: true,
            tasks_expanded: true,
            files_expanded: true,
            tasks_scroll: 0,
            files_scroll: 0,
            focused_section: EnhancedPanelSection::Session,
            cost_breakdown_expanded: false,
        }
    }

    /// Toggle the focused section's expanded state
    pub fn toggle_focused(&mut self) {
        match self.focused_section {
            EnhancedPanelSection::Session => self.session_expanded = !self.session_expanded,
            EnhancedPanelSection::Context => self.context_expanded = !self.context_expanded,
            EnhancedPanelSection::Tasks => self.tasks_expanded = !self.tasks_expanded,
            EnhancedPanelSection::Files => self.files_expanded = !self.files_expanded,
        }
    }

    /// Move focus to the next section
    pub fn focus_next(&mut self) {
        self.focused_section = match self.focused_section {
            EnhancedPanelSection::Session => EnhancedPanelSection::Context,
            EnhancedPanelSection::Context => EnhancedPanelSection::Tasks,
            EnhancedPanelSection::Tasks => EnhancedPanelSection::Files,
            EnhancedPanelSection::Files => EnhancedPanelSection::Session,
        };
    }

    /// Move focus to the previous section
    pub fn focus_prev(&mut self) {
        self.focused_section = match self.focused_section {
            EnhancedPanelSection::Session => EnhancedPanelSection::Files,
            EnhancedPanelSection::Context => EnhancedPanelSection::Session,
            EnhancedPanelSection::Tasks => EnhancedPanelSection::Context,
            EnhancedPanelSection::Files => EnhancedPanelSection::Tasks,
        };
    }

    /// Scroll down in the focused scrollable section
    pub fn scroll_down(&mut self, max_tasks: usize, max_files: usize) {
        match self.focused_section {
            EnhancedPanelSection::Tasks if self.tasks_expanded => {
                if self.tasks_scroll < max_tasks.saturating_sub(1) {
                    self.tasks_scroll += 1;
                }
            }
            EnhancedPanelSection::Files if self.files_expanded => {
                if self.files_scroll < max_files.saturating_sub(1) {
                    self.files_scroll += 1;
                }
            }
            _ => {}
        }
    }

    /// Scroll up in the focused scrollable section
    pub fn scroll_up(&mut self) {
        match self.focused_section {
            EnhancedPanelSection::Tasks if self.tasks_expanded => {
                self.tasks_scroll = self.tasks_scroll.saturating_sub(1);
            }
            EnhancedPanelSection::Files if self.files_expanded => {
                self.files_scroll = self.files_scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    /// Check if the given section is expanded
    pub fn is_expanded(&self, section: EnhancedPanelSection) -> bool {
        match section {
            EnhancedPanelSection::Session => self.session_expanded,
            EnhancedPanelSection::Context => self.context_expanded,
            EnhancedPanelSection::Tasks => self.tasks_expanded,
            EnhancedPanelSection::Files => self.files_expanded,
        }
    }

    /// Set the expanded state for a specific section
    pub fn set_expanded(&mut self, section: EnhancedPanelSection, expanded: bool) {
        match section {
            EnhancedPanelSection::Session => self.session_expanded = expanded,
            EnhancedPanelSection::Context => self.context_expanded = expanded,
            EnhancedPanelSection::Tasks => self.tasks_expanded = expanded,
            EnhancedPanelSection::Files => self.files_expanded = expanded,
        }
    }
}

// ============================================================================
// Original Panel Types (kept for backward compatibility)
// ============================================================================

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

    // ========================================================================
    // Enhanced Panel Data Structure Tests
    // ========================================================================

    #[test]
    fn test_enhanced_panel_data_default() {
        let data = EnhancedPanelData::default();
        assert!(data.session.name.is_empty());
        assert!(data.session.model.is_empty());
        assert!(data.session.provider.is_empty());
        assert_eq!(data.session.cost, 0.0);
        assert!(data.session.lsp_languages.is_empty());
        assert_eq!(data.context.used_tokens, 0);
        assert_eq!(data.context.max_tokens, 0);
        assert_eq!(data.context.usage_percent, 0.0);
        assert!(data.tasks.is_empty());
        assert!(data.files.is_empty());
    }

    #[test]
    fn test_session_info_default() {
        let session = SessionInfo::default();
        assert!(session.name.is_empty());
        assert!(session.model.is_empty());
        assert!(session.provider.is_empty());
        assert_eq!(session.cost, 0.0);
        assert!(session.lsp_languages.is_empty());
    }

    #[test]
    fn test_context_info_default() {
        let context = ContextInfo::default();
        assert_eq!(context.used_tokens, 0);
        assert_eq!(context.max_tokens, 0);
        assert_eq!(context.usage_percent, 0.0);
    }

    #[test]
    fn test_task_item_default() {
        let task = TaskItem::default();
        assert!(task.description.is_empty());
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[test]
    fn test_file_item_default() {
        let file = FileItem::default();
        assert!(file.path.is_empty());
        assert!(file.modified);
    }

    #[test]
    fn test_panel_section_state_new() {
        let state = PanelSectionState::new();
        assert!(state.session_expanded);
        assert!(state.context_expanded);
        assert!(state.tasks_expanded);
        assert!(state.files_expanded);
        assert_eq!(state.tasks_scroll, 0);
        assert_eq!(state.files_scroll, 0);
        assert_eq!(state.focused_section, EnhancedPanelSection::Session);
    }

    #[test]
    fn test_panel_section_state_toggle_focused() {
        let mut state = PanelSectionState::new();

        // Toggle Session (default focused)
        assert!(state.session_expanded);
        state.toggle_focused();
        assert!(!state.session_expanded);
        state.toggle_focused();
        assert!(state.session_expanded);

        // Move to Context and toggle
        state.focus_next();
        assert_eq!(state.focused_section, EnhancedPanelSection::Context);
        state.toggle_focused();
        assert!(!state.context_expanded);
    }

    #[test]
    fn test_panel_section_state_focus_navigation() {
        let mut state = PanelSectionState::new();

        // Forward navigation
        assert_eq!(state.focused_section, EnhancedPanelSection::Session);
        state.focus_next();
        assert_eq!(state.focused_section, EnhancedPanelSection::Context);
        state.focus_next();
        assert_eq!(state.focused_section, EnhancedPanelSection::Tasks);
        state.focus_next();
        assert_eq!(state.focused_section, EnhancedPanelSection::Files);
        state.focus_next();
        assert_eq!(state.focused_section, EnhancedPanelSection::Session); // Wraps around

        // Backward navigation
        state.focus_prev();
        assert_eq!(state.focused_section, EnhancedPanelSection::Files);
        state.focus_prev();
        assert_eq!(state.focused_section, EnhancedPanelSection::Tasks);
        state.focus_prev();
        assert_eq!(state.focused_section, EnhancedPanelSection::Context);
        state.focus_prev();
        assert_eq!(state.focused_section, EnhancedPanelSection::Session);
    }

    #[test]
    fn test_panel_section_state_scroll() {
        let mut state = PanelSectionState::new();

        // Focus on Tasks section
        state.focused_section = EnhancedPanelSection::Tasks;

        // Scroll down
        state.scroll_down(10, 5);
        assert_eq!(state.tasks_scroll, 1);
        state.scroll_down(10, 5);
        assert_eq!(state.tasks_scroll, 2);

        // Scroll up
        state.scroll_up();
        assert_eq!(state.tasks_scroll, 1);
        state.scroll_up();
        assert_eq!(state.tasks_scroll, 0);
        state.scroll_up(); // Should not go negative
        assert_eq!(state.tasks_scroll, 0);

        // Focus on Files section
        state.focused_section = EnhancedPanelSection::Files;
        state.scroll_down(10, 5);
        assert_eq!(state.files_scroll, 1);
        state.scroll_up();
        assert_eq!(state.files_scroll, 0);
    }

    #[test]
    fn test_panel_section_state_scroll_bounds() {
        let mut state = PanelSectionState::new();
        state.focused_section = EnhancedPanelSection::Tasks;

        // Try to scroll beyond max
        for _ in 0..20 {
            state.scroll_down(5, 3);
        }
        assert_eq!(state.tasks_scroll, 4); // max_tasks - 1 = 5 - 1 = 4

        // Scroll when collapsed should not change offset
        state.tasks_expanded = false;
        let prev_scroll = state.tasks_scroll;
        state.scroll_down(10, 5);
        assert_eq!(state.tasks_scroll, prev_scroll);
    }

    #[test]
    fn test_panel_section_state_is_expanded() {
        let state = PanelSectionState::new();
        assert!(state.is_expanded(EnhancedPanelSection::Session));
        assert!(state.is_expanded(EnhancedPanelSection::Context));
        assert!(state.is_expanded(EnhancedPanelSection::Tasks));
        assert!(state.is_expanded(EnhancedPanelSection::Files));
    }

    #[test]
    fn test_panel_section_state_set_expanded() {
        let mut state = PanelSectionState::new();

        state.set_expanded(EnhancedPanelSection::Session, false);
        assert!(!state.session_expanded);

        state.set_expanded(EnhancedPanelSection::Context, false);
        assert!(!state.context_expanded);

        state.set_expanded(EnhancedPanelSection::Tasks, false);
        assert!(!state.tasks_expanded);

        state.set_expanded(EnhancedPanelSection::Files, false);
        assert!(!state.files_expanded);
    }

    // ========================================================================
    // Progress Bar Rendering Tests (Requirements 4.2, 4.3, 4.4, 4.5, 4.6)
    // ========================================================================

    #[test]
    fn test_get_progress_bar_color_green() {
        // Usage < 80% should be green
        assert_eq!(get_progress_bar_color(0.0), Color::Green);
        assert_eq!(get_progress_bar_color(50.0), Color::Green);
        assert_eq!(get_progress_bar_color(79.9), Color::Green);
    }

    #[test]
    fn test_get_progress_bar_color_yellow() {
        // Usage 80-95% should be yellow
        assert_eq!(get_progress_bar_color(80.1), Color::Yellow);
        assert_eq!(get_progress_bar_color(90.0), Color::Yellow);
        assert_eq!(get_progress_bar_color(95.0), Color::Yellow);
    }

    #[test]
    fn test_get_progress_bar_color_red() {
        // Usage > 95% should be red
        assert_eq!(get_progress_bar_color(95.1), Color::Red);
        assert_eq!(get_progress_bar_color(99.0), Color::Red);
        assert_eq!(get_progress_bar_color(100.0), Color::Red);
    }

    #[test]
    fn test_format_with_thousands_separator() {
        assert_eq!(format_with_thousands_separator(0), "0");
        assert_eq!(format_with_thousands_separator(999), "999");
        assert_eq!(format_with_thousands_separator(1000), "1,000");
        assert_eq!(format_with_thousands_separator(12000), "12,000");
        assert_eq!(format_with_thousands_separator(128000), "128,000");
        assert_eq!(format_with_thousands_separator(1000000), "1,000,000");
    }

    #[test]
    fn test_render_progress_bar_empty() {
        let context = ContextInfo {
            used_tokens: 0,
            max_tokens: 128000,
            usage_percent: 0.0,
            over_limit: false,
        };
        let result = render_progress_bar(&context, None);

        assert_eq!(result.bar, "[░░░░░░░░░░░░░░░░░░░░]");
        assert_eq!(result.color, Color::Green);
        assert_eq!(result.token_display, "0 / 128,000 tokens");
    }

    #[test]
    fn test_render_progress_bar_half() {
        let context = ContextInfo {
            used_tokens: 64000,
            max_tokens: 128000,
            usage_percent: 50.0,
            over_limit: false,
        };
        let result = render_progress_bar(&context, None);

        assert_eq!(result.bar, "[██████████░░░░░░░░░░]");
        assert_eq!(result.color, Color::Green);
        assert_eq!(result.token_display, "64,000 / 128,000 tokens");
    }

    #[test]
    fn test_render_progress_bar_full() {
        let context = ContextInfo {
            used_tokens: 128000,
            max_tokens: 128000,
            usage_percent: 100.0,
            over_limit: true,
        };
        let result = render_progress_bar(&context, None);

        assert_eq!(result.bar, "[████████████████████]");
        assert_eq!(result.color, Color::Red);
        assert_eq!(result.token_display, "128,000 / 128,000 tokens");
    }

    #[test]
    fn test_render_progress_bar_yellow_threshold() {
        let context = ContextInfo {
            used_tokens: 108800,
            max_tokens: 128000,
            usage_percent: 85.0,
            over_limit: false,
        };
        let result = render_progress_bar(&context, None);

        assert_eq!(result.color, Color::Yellow);
    }

    #[test]
    fn test_render_progress_bar_custom_config() {
        let context = ContextInfo {
            used_tokens: 5000,
            max_tokens: 10000,
            usage_percent: 50.0,
            over_limit: false,
        };
        let config = ProgressBarConfig {
            width: 10,
            filled_char: '#',
            empty_char: '-',
        };
        let result = render_progress_bar(&context, Some(config));

        assert_eq!(result.bar, "[#####-----]");
        assert_eq!(result.token_display, "5,000 / 10,000 tokens");
    }

    #[test]
    fn test_context_info_render_progress_bar() {
        let context = ContextInfo {
            used_tokens: 12000,
            max_tokens: 128000,
            usage_percent: 9.375,
            over_limit: false,
        };
        let result = context.render_progress_bar();

        // 9.375% of 20 = 1.875, rounds to 2
        assert_eq!(result.bar, "[██░░░░░░░░░░░░░░░░░░]");
        assert_eq!(result.color, Color::Green);
    }

    #[test]
    fn test_context_info_format_percent() {
        let context = ContextInfo {
            used_tokens: 12000,
            max_tokens: 128000,
            usage_percent: 9.375,
            over_limit: false,
        };
        assert_eq!(context.format_percent(), "9%");

        let context2 = ContextInfo {
            used_tokens: 64000,
            max_tokens: 128000,
            usage_percent: 50.0,
            over_limit: false,
        };
        assert_eq!(context2.format_percent(), "50%");
    }

    #[test]
    fn test_render_progress_bar_clamps_percentage() {
        // Test that percentage is clamped to 0-100 range
        let context_over = ContextInfo {
            used_tokens: 150000,
            max_tokens: 128000,
            usage_percent: 117.0, // Over 100%
            over_limit: true,     // Context exceeds model limit
        };
        let result = render_progress_bar(&context_over, None);
        assert_eq!(result.bar, "[████████████████████]"); // Should be full, not overflow

        let context_under = ContextInfo {
            used_tokens: 0,
            max_tokens: 128000,
            usage_percent: -10.0, // Negative
            over_limit: false,
        };
        let result = render_progress_bar(&context_under, None);
        assert_eq!(result.bar, "[░░░░░░░░░░░░░░░░░░░░]"); // Should be empty, not underflow
    }

    #[test]
    fn test_context_over_limit_warning() {
        // Scenario: switched from 200K model to 16K model with 100K tokens in context
        let context = ContextInfo {
            used_tokens: 100000,
            max_tokens: 16000,
            usage_percent: 625.0, // 100K / 16K = 625%
            over_limit: true,
        };
        assert!(context.over_limit);
        // When rendered, this should show warning indicator
    }
}

// ============================================================================
// Panel Data Provider (Requirements 3.2, 3.3, 3.4, 4.2, 4.4, 5.2, 5.3, 5.4, 6.2, 6.3)
// ============================================================================

use crate::tui::agent_bridge::AgentBridge;
use crate::tui::editor_bridge::EditorState;
use crate::tui::plan_manager::PlanManager;
use crate::tui::usage_manager::UsageManager;
use std::collections::HashSet;

/// Collects data for the enhanced panel from various sources
///
/// This provider bridges the gap between the TUI components (AgentBridge,
/// UsageManager, PlanManager, EditorState) and the panel display data.
pub struct PanelDataProvider<'a> {
    /// Reference to the agent bridge for session info
    agent_bridge: Option<&'a AgentBridge>,
    /// Reference to the usage manager for context info
    usage_manager: Option<&'a UsageManager>,
    /// Reference to the plan manager for tasks and files
    plan_manager: Option<&'a PlanManager>,
    /// Reference to the editor state for LSP languages and modified files
    editor_state: Option<&'a EditorState>,
}

impl<'a> PanelDataProvider<'a> {
    /// Create a new PanelDataProvider with no sources
    pub fn new() -> Self {
        Self {
            agent_bridge: None,
            usage_manager: None,
            plan_manager: None,
            editor_state: None,
        }
    }

    /// Set the agent bridge reference
    pub fn with_agent_bridge(mut self, bridge: &'a AgentBridge) -> Self {
        self.agent_bridge = Some(bridge);
        self
    }

    /// Set the usage manager reference
    pub fn with_usage_manager(mut self, manager: &'a UsageManager) -> Self {
        self.usage_manager = Some(manager);
        self
    }

    /// Set the plan manager reference
    pub fn with_plan_manager(mut self, manager: &'a PlanManager) -> Self {
        self.plan_manager = Some(manager);
        self
    }

    /// Set the editor state reference
    pub fn with_editor_state(mut self, state: &'a EditorState) -> Self {
        self.editor_state = Some(state);
        self
    }

    /// Collect all panel data from the configured sources
    pub fn collect(&self) -> EnhancedPanelData {
        EnhancedPanelData {
            session: self.collect_session_info(),
            context: self.collect_context_info(),
            tasks: self.collect_tasks(),
            files: self.collect_files(),
        }
    }

    /// Collect session information from AgentBridge
    fn collect_session_info(&self) -> SessionInfo {
        if let Some(bridge) = self.agent_bridge {
            SessionInfo {
                name: bridge.session_name().to_string(),
                model: bridge.model_name().to_string(),
                provider: bridge.provider_name().to_string(),
                cost: bridge.total_cost(),
                lsp_languages: self.collect_lsp_languages(),
                cost_breakdown: Vec::new(),
            }
        } else {
            SessionInfo::default()
        }
    }

    /// Collect LSP languages from EditorState
    fn collect_lsp_languages(&self) -> Vec<String> {
        if let Some(state) = self.editor_state {
            // Extract unique filetypes from buffers
            state
                .buffers
                .iter()
                .filter_map(|b| b.filetype.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Collect context usage information from UsageManager
    fn collect_context_info(&self) -> ContextInfo {
        if let Some(usage) = self.usage_manager {
            let max = usage.max_tokens_for_model();
            let used = usage.total_tokens();
            let percent = usage.context_usage_percent(max) * 100.0; // Convert to percentage
            let over_limit = used > max || percent >= 100.0;
            ContextInfo {
                used_tokens: used,
                max_tokens: max,
                usage_percent: percent,
                over_limit,
            }
        } else {
            ContextInfo::default()
        }
    }

    /// Collect tasks from PlanManager
    fn collect_tasks(&self) -> Vec<TaskItem> {
        if let Some(plan) = self.plan_manager {
            plan.get_panel_tasks()
                .into_iter()
                .map(|t| TaskItem {
                    description: t.description,
                    status: match t.status {
                        crate::storage::TaskStatus::Pending => TaskStatus::Pending,
                        crate::storage::TaskStatus::InProgress => TaskStatus::Running,
                        crate::storage::TaskStatus::Completed => TaskStatus::Completed,
                        crate::storage::TaskStatus::Skipped => TaskStatus::Skipped,
                        crate::storage::TaskStatus::Failed => TaskStatus::Failed,
                    },
                })
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Collect modified files from PlanManager and EditorState
    fn collect_files(&self) -> Vec<FileItem> {
        let mut files: Vec<FileItem> = Vec::new();
        let mut seen_paths: HashSet<String> = HashSet::new();

        // From plan manager
        if let Some(plan) = self.plan_manager {
            for path in plan.modified_files() {
                if seen_paths.insert(path.clone()) {
                    files.push(FileItem {
                        path: path.clone(),
                        modified: true,
                    });
                }
            }
        }

        // From editor state
        if let Some(state) = self.editor_state {
            for path in &state.modified_files {
                if seen_paths.insert(path.clone()) {
                    files.push(FileItem {
                        path: path.clone(),
                        modified: true,
                    });
                }
            }
        }

        files
    }
}

impl Default for PanelDataProvider<'_> {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Progress Bar Rendering (Requirements 4.2, 4.3, 4.4, 4.5, 4.6)
// ============================================================================

/// Progress bar configuration
pub struct ProgressBarConfig {
    /// Total width of the progress bar in characters
    pub width: u16,
    /// Character for filled portion
    pub filled_char: char,
    /// Character for empty portion
    pub empty_char: char,
}

impl Default for ProgressBarConfig {
    fn default() -> Self {
        Self {
            width: 20,
            filled_char: '█',
            empty_char: '░',
        }
    }
}

/// Result of rendering a progress bar
#[derive(Debug, Clone, PartialEq)]
pub struct ProgressBarResult {
    /// The rendered progress bar string (e.g., "[████████░░░░░░░░░░░░]")
    pub bar: String,
    /// The color to use based on usage percentage
    pub color: Color,
    /// Formatted token count string (e.g., "12,000 / 128,000 tokens")
    pub token_display: String,
}

/// Get the color for a progress bar based on usage percentage
///
/// - Green (normal) for usage < 80%
/// - Yellow for usage 80-95%
/// - Red for usage > 95%
pub fn get_progress_bar_color(usage_percent: f32) -> Color {
    if usage_percent > 95.0 {
        Color::Red
    } else if usage_percent > 80.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

/// Format a number with thousands separators (e.g., 12000 -> "12,000")
#[allow(clippy::manual_is_multiple_of)] // is_multiple_of is unstable in stable Rust
pub fn format_with_thousands_separator(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();

    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }

    result
}

/// Render a progress bar for context usage
///
/// # Arguments
/// * `context` - The context info containing usage data
/// * `config` - Optional configuration for the progress bar
///
/// # Returns
/// A `ProgressBarResult` containing the rendered bar, color, and token display
pub fn render_progress_bar(
    context: &ContextInfo,
    config: Option<ProgressBarConfig>,
) -> ProgressBarResult {
    let config = config.unwrap_or_default();

    // Calculate filled/empty block ratio from percentage
    // Clamp percentage to 0-100 range
    let percent = context.usage_percent.clamp(0.0, 100.0);
    let filled_count = ((percent / 100.0) * config.width as f32).round() as u16;
    let empty_count = config.width.saturating_sub(filled_count);

    // Build the progress bar string
    let filled: String = std::iter::repeat_n(config.filled_char, filled_count as usize).collect();
    let empty: String = std::iter::repeat_n(config.empty_char, empty_count as usize).collect();
    let bar = format!("[{}{}]", filled, empty);

    // Get color based on thresholds
    let color = get_progress_bar_color(percent);

    // Format token counts with thousands separator
    let used_str = format_with_thousands_separator(context.used_tokens);
    let max_str = format_with_thousands_separator(context.max_tokens);
    let token_display = format!("{} / {} tokens", used_str, max_str);

    ProgressBarResult {
        bar,
        color,
        token_display,
    }
}

impl ContextInfo {
    /// Render the context info as a progress bar with token counts
    ///
    /// Returns a tuple of (progress_bar_string, color, token_display_string)
    pub fn render_progress_bar(&self) -> ProgressBarResult {
        render_progress_bar(self, None)
    }

    /// Render the context info with custom configuration
    pub fn render_progress_bar_with_config(&self, config: ProgressBarConfig) -> ProgressBarResult {
        render_progress_bar(self, Some(config))
    }

    /// Format the usage percentage as a string (e.g., "45%")
    pub fn format_percent(&self) -> String {
        format!("{}%", self.usage_percent.round() as u32)
    }
}

// ============================================================================
// Session Info Rendering Helper
// ============================================================================

impl SessionInfo {
    /// Format the session info as a single line: "name | model | provider"
    pub fn format_header(&self) -> String {
        format!("{} | {} | {}", self.name, self.model, self.provider)
    }

    /// Format the cost as USD string: "$X.XXXX"
    pub fn format_cost(&self) -> String {
        format!("Cost: ${:.4}", self.cost)
    }

    /// Format LSP languages as a comma-separated string
    pub fn format_lsp(&self) -> Option<String> {
        if self.lsp_languages.is_empty() {
            None
        } else {
            Some(format!("LSP: {}", self.lsp_languages.join(", ")))
        }
    }
}

// ============================================================================
// Task Status Rendering Helper
// ============================================================================

impl TaskItem {
    /// Get the display indicator for this task's status
    /// - Running: ● (filled circle)
    /// - Pending: ○ (empty circle)
    /// - Completed: ✓ (checkmark)
    /// - Failed: ✗ (cross)
    /// - Skipped: ⊘ (circle with slash)
    pub fn status_indicator(&self) -> &'static str {
        self.status.icon()
    }

    /// Format the task for display: "indicator description"
    pub fn format_display(&self) -> String {
        format!("{} {}", self.status_indicator(), self.description)
    }
}

// ============================================================================
// Enhanced Panel Rendering (Requirements 3.1, 3.6, 3.7, 4.1, 4.7, 4.8, 5.1, 5.6-5.9, 6.1, 6.5-6.8)
// ============================================================================

/// Result of rendering the enhanced panel
#[derive(Debug, Clone)]
pub struct EnhancedPanelRenderResult {
    /// Lines to render in the panel
    pub lines: Vec<Line<'static>>,
    /// Total height needed (for scrolling calculations)
    pub total_height: usize,
}

/// Render a scrollbar for a section
///
/// Returns the scrollbar characters to display on the right edge
fn render_scrollbar(
    scroll_offset: usize,
    visible_items: usize,
    total_items: usize,
    available_height: usize,
) -> Vec<char> {
    if total_items <= visible_items || available_height == 0 {
        // No scrollbar needed
        return vec![' '; available_height];
    }

    let mut scrollbar = vec!['░'; available_height];

    // Calculate thumb position and size
    let thumb_size = std::cmp::max(1, (available_height * visible_items) / total_items);
    let max_scroll = total_items.saturating_sub(visible_items);
    let thumb_pos = if max_scroll > 0 {
        (scroll_offset * (available_height.saturating_sub(thumb_size))) / max_scroll
    } else {
        0
    };

    // Draw the thumb
    for item in scrollbar
        .iter_mut()
        .take((thumb_pos + thumb_size).min(available_height))
        .skip(thumb_pos)
    {
        *item = '█';
    }

    // Add arrows at top and bottom if there's content above/below
    if scroll_offset > 0 && !scrollbar.is_empty() {
        scrollbar[0] = '▲';
    }
    if scroll_offset < max_scroll && scrollbar.len() > 1 {
        let last = scrollbar.len() - 1;
        scrollbar[last] = '▼';
    }

    scrollbar
}

/// Render a section header with accordion indicator
fn render_section_header(
    title: &str,
    icon: &str,
    expanded: bool,
    is_focused: bool,
    width: u16,
) -> Line<'static> {
    let expand_icon = if expanded { "▼" } else { "▶" };

    let header_style = if is_focused {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD)
    };

    // Build the header: "╭── ▼ 📊 Title ──────────────╮"
    let header_text = format!("{} {} {}", expand_icon, icon, title);
    let padding_len = (width as usize).saturating_sub(header_text.len() + 6); // 6 for "╭── " and " ─╮"
    let padding = "─".repeat(padding_len);

    Line::from(vec![
        Span::styled("╭── ", Style::default().fg(Color::DarkGray)),
        Span::styled(header_text, header_style),
        Span::styled(
            format!(" {}╮", padding),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

/// Render a section footer
fn render_section_footer(width: u16) -> Line<'static> {
    let padding = "─".repeat((width as usize).saturating_sub(2));
    Line::from(Span::styled(
        format!("╰{}╯", padding),
        Style::default().fg(Color::DarkGray),
    ))
}

/// Render the Session section content
fn render_session_content(session: &SessionInfo, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let content_width = (width as usize).saturating_sub(4); // Account for borders

    // Helper to truncate text if needed
    let truncate = |s: &str| -> String {
        if s.len() > content_width.saturating_sub(3) {
            format!("{}…", &s[..content_width.saturating_sub(4)])
        } else {
            s.to_string()
        }
    };

    // Session name line
    let name_display = if session.name.is_empty() {
        "New Session".to_string()
    } else {
        session.name.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled("📛 ", Style::default()),
        Span::styled(truncate(&name_display), Style::default().fg(Color::Cyan)),
    ]));

    // Model line
    let model_display = if session.model.is_empty() {
        "default".to_string()
    } else {
        session.model.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled("🤖 ", Style::default()),
        Span::styled(
            truncate(&model_display),
            Style::default().fg(Color::Magenta),
        ),
    ]));

    // Provider line
    let provider_display = if session.provider.is_empty() {
        "none".to_string()
    } else {
        session.provider.clone()
    };
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled("🏢 ", Style::default()),
        Span::styled(
            truncate(&provider_display),
            Style::default().fg(Color::Blue),
        ),
    ]));

    // Cost line
    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled("💰 ", Style::default()),
        Span::styled(
            format!("${:.4}", session.cost),
            Style::default().fg(Color::Yellow),
        ),
    ]));

    // LSP languages (if any)
    if !session.lsp_languages.is_empty() {
        let lsp_str = session.lsp_languages.join(", ");
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("🔧 ", Style::default()),
            Span::styled(truncate(&lsp_str), Style::default().fg(Color::Green)),
        ]));
    }

    lines
}

/// Render the Context section content
fn render_context_content(context: &ContextInfo, _width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    // Progress bar with warning indicator if over limit
    let progress = context.render_progress_bar();
    let warning = if context.over_limit { "⚠ " } else { "" };
    let bar_color = if context.over_limit {
        Color::Red
    } else {
        progress.color
    };

    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(warning.to_string(), Style::default().fg(Color::Yellow)),
        Span::styled(
            format!("{} {}%", progress.bar, context.usage_percent.round() as u32),
            Style::default().fg(bar_color),
        ),
        Span::raw(" "),
    ]));

    // Token counts with warning message if over limit
    if context.over_limit {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "⚠ Context exceeds model limit!",
                Style::default().fg(Color::Yellow),
            ),
            Span::raw(" "),
        ]));
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                "Auto-summarize on next prompt",
                Style::default().fg(Color::DarkGray),
            ),
            Span::raw(" "),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("│ ", Style::default().fg(Color::DarkGray)),
        Span::styled(progress.token_display, Style::default().fg(Color::White)),
        Span::raw(" "),
    ]));

    lines
}

/// Render the Tasks section content with scrollbar
fn render_tasks_content(
    tasks: &[TaskItem],
    scroll_offset: usize,
    visible_height: usize,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if tasks.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("No active tasks", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
        ]));
        return lines;
    }

    // Calculate scrollbar
    let scrollbar = render_scrollbar(scroll_offset, visible_height, tasks.len(), visible_height);
    let needs_scrollbar = tasks.len() > visible_height;

    // Render visible tasks
    let visible_tasks = tasks.iter().skip(scroll_offset).take(visible_height);
    for (i, task) in visible_tasks.enumerate() {
        let display = task.format_display();
        let content_width = if needs_scrollbar {
            (width as usize).saturating_sub(5) // Account for borders and scrollbar
        } else {
            (width as usize).saturating_sub(4)
        };

        let truncated = if display.len() > content_width {
            format!("{}…", &display[..content_width.saturating_sub(1)])
        } else {
            display
        };

        let scrollbar_char = if needs_scrollbar && i < scrollbar.len() {
            scrollbar[i].to_string()
        } else {
            String::new()
        };

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(truncated, Style::default().fg(task.status.color())),
            Span::raw(" "),
            Span::styled(scrollbar_char, Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines
}

/// Render the Files section content with scrollbar
fn render_files_content(
    files: &[FileItem],
    scroll_offset: usize,
    visible_height: usize,
    width: u16,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    if files.is_empty() {
        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled("No modified files", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
        ]));
        return lines;
    }

    // Calculate scrollbar
    let scrollbar = render_scrollbar(scroll_offset, visible_height, files.len(), visible_height);
    let needs_scrollbar = files.len() > visible_height;

    // Render visible files
    let visible_files = files.iter().skip(scroll_offset).take(visible_height);
    for (i, file) in visible_files.enumerate() {
        let indicator = if file.modified { "●" } else { " " };
        let display = format!("{} {}", indicator, file.path);
        let content_width = if needs_scrollbar {
            (width as usize).saturating_sub(5)
        } else {
            (width as usize).saturating_sub(4)
        };

        let truncated = if display.len() > content_width {
            format!("{}…", &display[..content_width.saturating_sub(1)])
        } else {
            display
        };

        let scrollbar_char = if needs_scrollbar && i < scrollbar.len() {
            scrollbar[i].to_string()
        } else {
            String::new()
        };

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::DarkGray)),
            Span::styled(truncated, Style::default().fg(Color::Yellow)),
            Span::raw(" "),
            Span::styled(scrollbar_char, Style::default().fg(Color::DarkGray)),
        ]));
    }

    lines
}

/// Render the enhanced panel with all sections
///
/// This function renders the full enhanced panel with:
/// - Session section (collapsible)
/// - Context section with progress bar (collapsible)
/// - Tasks section with scrollbar (collapsible)
/// - Files section with scrollbar (collapsible)
///
/// # Arguments
/// * `data` - The panel data to render
/// * `state` - The panel section state (expanded/collapsed, scroll offsets)
/// * `area` - The area to render into
/// * `buf` - The buffer to render into
///
/// # Requirements
/// - 3.1, 3.6, 3.7: Session section with accordion
/// - 4.1, 4.7, 4.8: Context section with accordion
/// - 5.1, 5.6-5.9: Tasks section with accordion and scrollbar
/// - 6.1, 6.5-6.8: Files section with accordion and scrollbar
pub fn render_enhanced_panel(
    data: &EnhancedPanelData,
    state: &PanelSectionState,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.width < 10 || area.height < 5 {
        return; // Too small to render
    }

    let width = area.width;
    let mut lines: Vec<Line<'static>> = Vec::new();

    // Calculate available height for scrollable sections
    // Each section needs: 1 header + content + 1 footer (when expanded)
    // When collapsed: just 1 header line
    let session_height = if state.session_expanded {
        3 + if data.session.lsp_languages.is_empty() {
            0
        } else {
            1
        }
    } else {
        1
    };
    let context_height = if state.context_expanded { 4 } else { 1 };

    // Calculate remaining height for tasks and files
    let fixed_height = session_height + context_height + 2; // +2 for section footers
    let remaining_height = (area.height as usize).saturating_sub(fixed_height);
    let tasks_visible_height = if state.tasks_expanded {
        remaining_height / 2
    } else {
        0
    };
    let files_visible_height = if state.files_expanded {
        remaining_height.saturating_sub(tasks_visible_height)
    } else {
        0
    };

    // ========== Session Section ==========
    lines.push(render_section_header(
        "Session",
        "ℹ️",
        state.session_expanded,
        state.focused_section == EnhancedPanelSection::Session,
        width,
    ));

    if state.session_expanded {
        lines.extend(render_session_content(&data.session, width));
        lines.push(render_section_footer(width));
    }

    // ========== Context Section ==========
    lines.push(render_section_header(
        "Context",
        "📊",
        state.context_expanded,
        state.focused_section == EnhancedPanelSection::Context,
        width,
    ));

    if state.context_expanded {
        lines.extend(render_context_content(&data.context, width));
        lines.push(render_section_footer(width));
    }

    // ========== Tasks Section ==========
    lines.push(render_section_header(
        "Tasks",
        "⏱️",
        state.tasks_expanded,
        state.focused_section == EnhancedPanelSection::Tasks,
        width,
    ));

    if state.tasks_expanded {
        let visible_height = std::cmp::max(1, tasks_visible_height.saturating_sub(2)); // -2 for header/footer
        lines.extend(render_tasks_content(
            &data.tasks,
            state.tasks_scroll,
            visible_height,
            width,
        ));
        lines.push(render_section_footer(width));
    }

    // ========== Files Section ==========
    lines.push(render_section_header(
        "Modified Files",
        "📁",
        state.files_expanded,
        state.focused_section == EnhancedPanelSection::Files,
        width,
    ));

    if state.files_expanded {
        let visible_height = std::cmp::max(1, files_visible_height.saturating_sub(2)); // -2 for header/footer
        lines.extend(render_files_content(
            &data.files,
            state.files_scroll,
            visible_height,
            width,
        ));
        lines.push(render_section_footer(width));
    }

    // Render all lines to the buffer
    let paragraph = Paragraph::new(lines);
    paragraph.render(area, buf);
}

/// Widget wrapper for the enhanced panel
pub struct EnhancedPanelWidget<'a> {
    data: &'a EnhancedPanelData,
    state: &'a PanelSectionState,
}

impl<'a> EnhancedPanelWidget<'a> {
    /// Create a new enhanced panel widget
    pub fn new(data: &'a EnhancedPanelData, state: &'a PanelSectionState) -> Self {
        Self { data, state }
    }
}

impl Widget for EnhancedPanelWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        render_enhanced_panel(self.data, self.state, area, buf);
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    // Strategy for generating EnhancedPanelSection
    fn enhanced_panel_section_strategy() -> impl Strategy<Value = EnhancedPanelSection> {
        prop_oneof![
            Just(EnhancedPanelSection::Session),
            Just(EnhancedPanelSection::Context),
            Just(EnhancedPanelSection::Tasks),
            Just(EnhancedPanelSection::Files),
        ]
    }

    // Strategy for generating non-empty strings (for session info)
    fn non_empty_string_strategy() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_-]{1,50}"
    }

    // Strategy for generating TaskStatus
    fn task_status_strategy() -> impl Strategy<Value = TaskStatus> {
        prop_oneof![
            Just(TaskStatus::Pending),
            Just(TaskStatus::Running),
            Just(TaskStatus::Completed),
            Just(TaskStatus::Failed),
            Just(TaskStatus::Skipped),
        ]
    }

    // Feature: enhanced-tui-layout, Property 12: Panel Section Accordion Toggle
    // **Validates: Requirements 5.6, 6.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]
        #[test]
        fn prop_panel_section_accordion_toggle_roundtrip(
            initial_session in any::<bool>(),
            initial_context in any::<bool>(),
            initial_tasks in any::<bool>(),
            initial_files in any::<bool>(),
            section in enhanced_panel_section_strategy()
        ) {
            let mut state = PanelSectionState {
                session_expanded: initial_session,
                context_expanded: initial_context,
                tasks_expanded: initial_tasks,
                files_expanded: initial_files,
                tasks_scroll: 0,
                files_scroll: 0,
                focused_section: section,
            };

            // Get initial expanded state for the focused section
            let initial_expanded = state.is_expanded(section);

            // Toggle twice should return to original state
            state.toggle_focused();
            state.toggle_focused();

            prop_assert_eq!(state.is_expanded(section), initial_expanded);
        }
    }

    // Feature: enhanced-tui-layout, Property 13: Panel Section Scroll Bounds
    // **Validates: Requirements 5.6, 6.5**
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]
        #[test]
        fn prop_panel_section_scroll_bounds(
            max_tasks in 0usize..50,
            max_files in 0usize..50,
            scroll_ops in proptest::collection::vec(any::<bool>(), 0..20)
        ) {
            let mut state = PanelSectionState::new();

            // Test Tasks section scroll bounds
            state.focused_section = EnhancedPanelSection::Tasks;
            for scroll_down in &scroll_ops {
                if *scroll_down {
                    state.scroll_down(max_tasks, max_files);
                } else {
                    state.scroll_up();
                }
            }
            // Scroll offset should never be negative (usize guarantees this)
            // Scroll offset should be <= max_tasks - 1 (or 0 if max_tasks is 0)
            let max_scroll_tasks = max_tasks.saturating_sub(1);
            prop_assert!(state.tasks_scroll <= max_scroll_tasks);

            // Test Files section scroll bounds
            state.focused_section = EnhancedPanelSection::Files;
            state.files_scroll = 0; // Reset
            for scroll_down in &scroll_ops {
                if *scroll_down {
                    state.scroll_down(max_tasks, max_files);
                } else {
                    state.scroll_up();
                }
            }
            let max_scroll_files = max_files.saturating_sub(1);
            prop_assert!(state.files_scroll <= max_scroll_files);
        }
    }

    // Feature: enhanced-tui-layout, Property 3: Session Info Rendering
    // **Validates: Requirements 3.2, 3.3, 3.4**
    //
    // For any valid SessionInfo with non-empty name, model, and provider,
    // the rendered session section should contain all three values and
    // the cost formatted as "$X.XXXX".
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_session_info_rendering(
            name in non_empty_string_strategy(),
            model in non_empty_string_strategy(),
            provider in non_empty_string_strategy(),
            cost in 0.0f64..1000.0f64,
            lsp_languages in proptest::collection::vec("[a-z]{2,10}", 0..5)
        ) {
            let session = SessionInfo {
                name: name.clone(),
                model: model.clone(),
                provider: provider.clone(),
                cost,
                lsp_languages: lsp_languages.clone(),
            };

            // Test format_header contains all three values
            let header = session.format_header();
            prop_assert!(
                header.contains(&name),
                "Header '{}' should contain name '{}'", header, name
            );
            prop_assert!(
                header.contains(&model),
                "Header '{}' should contain model '{}'", header, model
            );
            prop_assert!(
                header.contains(&provider),
                "Header '{}' should contain provider '{}'", header, provider
            );

            // Test format_cost produces correct format "$X.XXXX"
            let cost_str = session.format_cost();
            prop_assert!(
                cost_str.starts_with("Cost: $"),
                "Cost string '{}' should start with 'Cost: $'", cost_str
            );
            // Verify the cost value is formatted with 4 decimal places
            let expected_cost = format!("Cost: ${:.4}", cost);
            prop_assert_eq!(
                cost_str, expected_cost,
                "Cost string should be formatted as '$X.XXXX'"
            );

            // Test format_lsp
            let lsp_str = session.format_lsp();
            if lsp_languages.is_empty() {
                prop_assert!(
                    lsp_str.is_none(),
                    "LSP string should be None when no languages"
                );
            } else {
                prop_assert!(
                    lsp_str.is_some(),
                    "LSP string should be Some when languages exist"
                );
                let lsp = lsp_str.unwrap();
                prop_assert!(
                    lsp.starts_with("LSP: "),
                    "LSP string '{}' should start with 'LSP: '", lsp
                );
                // Each language should be in the output
                for lang in &lsp_languages {
                    prop_assert!(
                        lsp.contains(lang),
                        "LSP string '{}' should contain language '{}'", lsp, lang
                    );
                }
            }
        }
    }

    // Feature: enhanced-tui-layout, Property 5: Task Status Indicators
    // **Validates: Requirements 5.2, 5.3, 5.4**
    //
    // For any TaskItem, the rendered indicator should be:
    // - ● for Running
    // - ○ for Pending
    // - ✓ for Completed
    // - ✗ for Failed
    // - ⊘ for Skipped
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_task_status_indicators(
            description in "[a-zA-Z0-9 ]{1,100}",
            status in task_status_strategy()
        ) {
            let task = TaskItem {
                description: description.clone(),
                status,
            };

            // Verify the correct indicator is returned
            let indicator = task.status_indicator();
            let expected_indicator = match status {
                TaskStatus::Running => "●",
                TaskStatus::Pending => "○",
                TaskStatus::Completed => "✓",
                TaskStatus::Failed => "✗",
                TaskStatus::Skipped => "⊘",
            };
            prop_assert_eq!(
                indicator, expected_indicator,
                "Status {:?} should have indicator '{}', got '{}'",
                status, expected_indicator, indicator
            );

            // Verify format_display contains both indicator and description
            let display = task.format_display();
            prop_assert!(
                display.contains(indicator),
                "Display '{}' should contain indicator '{}'", display, indicator
            );
            prop_assert!(
                display.contains(&description),
                "Display '{}' should contain description '{}'", display, description
            );

            // Verify format is "indicator description"
            let expected_display = format!("{} {}", indicator, description);
            prop_assert_eq!(
                display, expected_display,
                "Display should be 'indicator description'"
            );
        }
    }

    // Feature: enhanced-tui-layout, Property 4: Progress Bar Rendering
    // **Validates: Requirements 4.2, 4.3, 4.4, 4.5, 4.6**
    //
    // For any ContextInfo with usage_percent in [0, 100], the progress bar should have
    // filled blocks proportional to usage_percent, and the color should be:
    // - Green (normal) for usage < 80%
    // - Yellow for usage 80-95%
    // - Red for usage > 95%
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_progress_bar_rendering(
            used_tokens in 0u32..200000u32,
            max_tokens in 1u32..200000u32,
            usage_percent in 0.0f32..=100.0f32
        ) {
            let context = ContextInfo {
                used_tokens,
                max_tokens,
                usage_percent,
                over_limit: used_tokens > max_tokens || usage_percent >= 100.0,
            };

            let result = render_progress_bar(&context, None);
            let config = ProgressBarConfig::default();

            // Property 1: Color should match thresholds
            let expected_color = if usage_percent > 95.0 {
                Color::Red
            } else if usage_percent > 80.0 {
                Color::Yellow
            } else {
                Color::Green
            };
            prop_assert_eq!(
                result.color, expected_color,
                "Usage {}% should have color {:?}, got {:?}",
                usage_percent, expected_color, result.color
            );

            // Property 2: Progress bar should have correct total width
            // Bar format is "[filled_chars + empty_chars]" so total length = width + 2 (for brackets)
            let expected_bar_len = config.width as usize + 2;
            prop_assert_eq!(
                result.bar.chars().count(), expected_bar_len,
                "Progress bar '{}' should have {} chars, got {}",
                result.bar, expected_bar_len, result.bar.chars().count()
            );

            // Property 3: Progress bar should start with '[' and end with ']'
            prop_assert!(
                result.bar.starts_with('[') && result.bar.ends_with(']'),
                "Progress bar '{}' should be wrapped in brackets", result.bar
            );

            // Property 4: Filled blocks should be proportional to usage_percent
            let clamped_percent = usage_percent.clamp(0.0, 100.0);
            let expected_filled = ((clamped_percent / 100.0) * config.width as f32).round() as usize;
            let filled_count = result.bar.chars().filter(|&c| c == config.filled_char).count();
            prop_assert_eq!(
                filled_count, expected_filled,
                "Progress bar should have {} filled blocks for {}%, got {}",
                expected_filled, usage_percent, filled_count
            );

            // Property 5: Empty blocks should fill the remaining space
            let expected_empty = config.width as usize - expected_filled;
            let empty_count = result.bar.chars().filter(|&c| c == config.empty_char).count();
            prop_assert_eq!(
                empty_count, expected_empty,
                "Progress bar should have {} empty blocks, got {}",
                expected_empty, empty_count
            );

            // Property 6: Token display should contain formatted token counts
            let used_str = format_with_thousands_separator(used_tokens);
            let max_str = format_with_thousands_separator(max_tokens);
            prop_assert!(
                result.token_display.contains(&used_str),
                "Token display '{}' should contain used tokens '{}'",
                result.token_display, used_str
            );
            prop_assert!(
                result.token_display.contains(&max_str),
                "Token display '{}' should contain max tokens '{}'",
                result.token_display, max_str
            );
            prop_assert!(
                result.token_display.ends_with(" tokens"),
                "Token display '{}' should end with ' tokens'",
                result.token_display
            );
        }
    }
}
