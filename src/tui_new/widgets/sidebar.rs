//! Sidebar Widget - Context panels for session info, files, tasks, git
//!
//! Reference: web/ui/mocks/src/app/components/Sidebar.tsx
//! Feature: 13_sidebar.feature
//! Baseline: screenshots/11-sidebar-panel.png

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::theme::Theme;

/// Sidebar panel type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    Session,
    Context,
    Tasks,
    GitChanges,
}

impl SidebarPanel {
    /// Get display name for panel
    pub fn display_name(&self) -> &'static str {
        match self {
            SidebarPanel::Session => "Session",
            SidebarPanel::Context => "Context",
            SidebarPanel::Tasks => "Tasks",
            SidebarPanel::GitChanges => "Git Changes",
        }
    }

    /// Get icon for panel
    pub fn icon(&self) -> &'static str {
        match self {
            SidebarPanel::Session => "📊",
            SidebarPanel::Context => "📂",
            SidebarPanel::Tasks => "✓",
            SidebarPanel::GitChanges => "⎇",
        }
    }
}

/// Task status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskStatus {
    Active,
    Queued,
    Completed,
}

/// Task item
#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    pub status: TaskStatus,
}

/// Git change status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
}

/// Git change item
#[derive(Debug, Clone)]
pub struct GitChange {
    pub file: String,
    pub status: GitStatus,
    pub additions: usize,
    pub deletions: usize,
}

/// Sidebar widget
#[derive(Debug)]
pub struct Sidebar<'a> {
    /// Whether sidebar is visible
    pub visible: bool,
    /// Session info
    pub session_info: SessionInfo,
    /// Context files
    pub context_files: Vec<String>,
    /// Token usage
    pub tokens_used: usize,
    pub tokens_total: usize,
    /// Tasks
    pub tasks: Vec<Task>,
    /// Git changes
    pub git_changes: Vec<GitChange>,
    /// Theme
    pub theme: &'a Theme,
    /// Current theme name for display
    pub current_theme_name: String,
    /// Which panels are expanded (Session, Context, Tasks, GitChanges)
    pub expanded_panels: [bool; 4],
    /// Currently selected panel index
    pub selected_panel: usize,
    /// Selected item within panel (None = panel header selected)
    pub selected_item: Option<usize>,
    /// Whether sidebar has focus
    pub focused: bool,
    /// Current Vim mode
    pub vim_mode: crate::ui_backend::VimMode,
}

/// Session information
#[derive(Debug, Default)]
pub struct SessionInfo {
    pub branch: String,
    pub total_cost: f64,
    pub model_count: usize,
}

// Note: No Default impl for Sidebar since it requires a 'a lifetime reference to Theme

impl<'a> Sidebar<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            visible: true,
            session_info: SessionInfo::default(),
            context_files: Vec::new(),
            tokens_used: 0,
            tokens_total: 1_000_000,
            tasks: Vec::new(),
            git_changes: Vec::new(),
            theme,
            current_theme_name: "Catppuccin Mocha".to_string(),
            expanded_panels: [true, true, true, true], // All expanded by default
            selected_panel: 0,
            selected_item: None,
            focused: false,
            vim_mode: crate::ui_backend::VimMode::Insert,
        }
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    pub fn theme_name(mut self, name: String) -> Self {
        self.current_theme_name = name;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn expanded(mut self, panel: SidebarPanel, expanded: bool) -> Self {
        let idx = panel as usize;
        self.expanded_panels[idx] = expanded;
        self
    }

    pub fn selected_panel(mut self, panel_idx: usize) -> Self {
        self.selected_panel = panel_idx;
        self
    }

    pub fn vim_mode(mut self, mode: crate::ui_backend::VimMode) -> Self {
        self.vim_mode = mode;
        self
    }

    /// Toggle panel expansion
    pub fn toggle_panel(&mut self, panel_idx: usize) {
        if panel_idx < 4 {
            self.expanded_panels[panel_idx] = !self.expanded_panels[panel_idx];
        }
    }

    /// Navigate to next panel
    pub fn next_panel(&mut self) {
        self.selected_panel = (self.selected_panel + 1) % 4;
        self.selected_item = None;
    }

    /// Navigate to previous panel
    pub fn prev_panel(&mut self) {
        self.selected_panel = if self.selected_panel == 0 {
            3
        } else {
            self.selected_panel - 1
        };
        self.selected_item = None;
    }

    /// Navigate down within current panel
    pub fn next_item(&mut self) {
        if !self.expanded_panels[self.selected_panel] {
            return; // Can't navigate inside collapsed panel
        }

        let max_items = match self.selected_panel {
            0 => self.session_info.branch.len(), // Simplified
            1 => self.context_files.len(),
            2 => self.tasks.len(),
            3 => self.git_changes.len(),
            _ => 0,
        };

        if let Some(item) = self.selected_item {
            if item + 1 < max_items {
                self.selected_item = Some(item + 1);
            }
        } else if max_items > 0 {
            self.selected_item = Some(0);
        }
    }

    /// Navigate up within current panel
    pub fn prev_item(&mut self) {
        if let Some(item) = self.selected_item {
            if item > 0 {
                self.selected_item = Some(item - 1);
            } else {
                self.selected_item = None; // Back to panel header
            }
        }
    }

    /// Enter into selected panel (expand and select first item)
    pub fn enter_panel(&mut self) {
        if !self.expanded_panels[self.selected_panel] {
            self.expanded_panels[self.selected_panel] = true;
        }
        self.selected_item = Some(0);
    }

    /// Exit from panel items back to panel header
    pub fn exit_panel(&mut self) {
        self.selected_item = None;
    }

    pub fn session_info(mut self, info: SessionInfo) -> Self {
        self.session_info = info;
        self
    }

    pub fn context_files(mut self, files: Vec<String>) -> Self {
        self.context_files = files;
        self
    }

    pub fn tokens(mut self, used: usize, total: usize) -> Self {
        self.tokens_used = used;
        self.tokens_total = total;
        self
    }

    pub fn tasks(mut self, tasks: Vec<Task>) -> Self {
        self.tasks = tasks;
        self
    }

    pub fn git_changes(mut self, changes: Vec<GitChange>) -> Self {
        self.git_changes = changes;
        self
    }
}

impl Widget for Sidebar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.visible {
            return;
        }

        // Main border with focus indicator
        let border_style = if self.focused {
            Style::default().fg(self.theme.border_focused)
        } else {
            Style::default().fg(self.theme.border)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(Span::styled(
                " Panel ",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        block.render(area, buf);

        // Build all content lines
        let mut all_lines: Vec<Line> = vec![];

        // Header section: theme selector and close button
        all_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled("Theme: ", Style::default().fg(self.theme.text_muted)),
            Span::styled(
                &self.current_theme_name,
                Style::default().fg(self.theme.cyan),
            ),
            Span::raw("  "),
            Span::styled("⟩", Style::default().fg(self.theme.text_muted)),
        ]));
        all_lines.push(Line::from(""));

        // VIM mode indicator
        use crate::ui_backend::VimMode;
        let vim_str = match self.vim_mode {
            VimMode::Insert => "INSERT",
            VimMode::Normal => "NORMAL",
            VimMode::Visual => "VISUAL",
            VimMode::Command => "COMMAND",
        };
        let vim_color = match self.vim_mode {
            VimMode::Insert => self.theme.green,
            VimMode::Normal => self.theme.blue,
            VimMode::Visual => self.theme.purple,
            VimMode::Command => self.theme.yellow,
        };
        all_lines.push(Line::from(vec![
            Span::styled("  VIM: ", Style::default().fg(self.theme.text_muted)),
            Span::styled(vim_str, Style::default().fg(vim_color)),
        ]));
        all_lines.push(Line::from(""));

        // ======== SESSION SECTION ========
        let session_expanded = self.expanded_panels[0];
        let session_selected = self.selected_panel == 0 && self.selected_item.is_none();
        let chevron = if session_expanded { "▼" } else { "▶" };

        let session_header_style = if self.focused && session_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(45, 60, 83))
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        all_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", chevron),
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled("Session", session_header_style),
        ]));

        if session_expanded {
            all_lines.push(Line::from(vec![
                Span::raw("  ⎇ "),
                Span::styled(
                    &self.session_info.branch,
                    Style::default().fg(self.theme.blue),
                ),
            ]));
            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "${:.3} ({} models)",
                        self.session_info.total_cost, self.session_info.model_count
                    ),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));
        }
        all_lines.push(Line::from(""));

        // ======== CONTEXT SECTION ========
        let context_expanded = self.expanded_panels[1];
        let context_selected = self.selected_panel == 1 && self.selected_item.is_none();
        let chevron = if context_expanded { "▼" } else { "▶" };
        let context_count = self.context_files.len();

        let context_header_style = if self.focused && context_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(45, 60, 83))
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        all_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", chevron),
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled("Context", context_header_style),
            Span::raw("  "),
            Span::styled(
                format!("{:.1}k", self.tokens_used as f32 / 1000.0),
                Style::default().fg(self.theme.blue),
            ),
        ]));

        if context_expanded {
            // Calculate usage percentage
            let usage_percent = if self.tokens_total > 0 {
                self.tokens_used as f32 / self.tokens_total as f32 * 100.0
            } else {
                0.0
            };

            let usage_color = if usage_percent < 50.0 {
                self.theme.green
            } else if usage_percent < 80.0 {
                self.theme.yellow
            } else {
                self.theme.red
            };

            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} / {} tokens", self.tokens_used, self.tokens_total),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));
            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("Usage: {:.1}%", usage_percent),
                    Style::default()
                        .fg(usage_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("LOADED FILES ({})", context_count),
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            for (i, file) in self.context_files.iter().enumerate().take(4) {
                let item_selected =
                    self.focused && self.selected_panel == 1 && self.selected_item == Some(i);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(Color::Rgb(45, 60, 83))
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                all_lines.push(Line::from(vec![
                    Span::raw("  📄 "),
                    Span::styled(file, item_style),
                ]));
            }
            if context_count > 4 {
                all_lines.push(Line::from(vec![Span::styled(
                    format!("  ... and {} more", context_count - 4),
                    Style::default().fg(self.theme.text_muted),
                )]));
            }
        }
        all_lines.push(Line::from(""));

        // ======== TASKS SECTION ========
        let tasks_expanded = self.expanded_panels[2];
        let tasks_selected = self.selected_panel == 2 && self.selected_item.is_none();
        let chevron = if tasks_expanded { "▼" } else { "▶" };
        let task_count = self.tasks.len();
        let active_tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Active)
            .collect();
        let queued_tasks: Vec<_> = self
            .tasks
            .iter()
            .filter(|t| t.status == TaskStatus::Queued)
            .collect();

        let tasks_header_style = if self.focused && tasks_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(45, 60, 83))
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        all_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", chevron),
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled("Tasks", tasks_header_style),
            Span::raw(" "),
            Span::styled(
                format!("{}", task_count),
                Style::default()
                    .fg(self.theme.bg_main)
                    .bg(self.theme.blue)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if tasks_expanded {
            let mut task_idx = 0;
            for task in active_tasks.iter().take(1) {
                let item_selected = self.focused
                    && self.selected_panel == 2
                    && self.selected_item == Some(task_idx);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(Color::Rgb(45, 60, 83))
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                all_lines.push(Line::from(vec![
                    Span::styled("  ● ", Style::default().fg(self.theme.green)),
                    Span::styled(&task.name, item_style),
                    Span::raw(" "),
                    Span::styled("Active", Style::default().fg(self.theme.text_muted)),
                ]));
                task_idx += 1;
            }

            if !queued_tasks.is_empty() {
                all_lines.push(Line::from(vec![Span::styled(
                    "  QUEUED",
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                )]));

                for task in queued_tasks.iter().take(6) {
                    let item_selected = self.focused
                        && self.selected_panel == 2
                        && self.selected_item == Some(task_idx);
                    let item_style = if item_selected {
                        Style::default()
                            .fg(self.theme.cyan)
                            .bg(Color::Rgb(45, 60, 83))
                    } else {
                        Style::default().fg(self.theme.text_primary)
                    };

                    all_lines.push(Line::from(vec![
                        Span::styled("  ○ ", Style::default().fg(self.theme.text_muted)),
                        Span::styled(&task.name, item_style),
                    ]));
                    task_idx += 1;
                }
            }
        }
        all_lines.push(Line::from(""));

        // ======== GIT CHANGES SECTION ========
        let git_expanded = self.expanded_panels[3];
        let git_selected = self.selected_panel == 3 && self.selected_item.is_none();
        let chevron = if git_expanded { "▼" } else { "▶" };
        let git_count = self.git_changes.len();
        let modified_count = self
            .git_changes
            .iter()
            .filter(|g| g.status == GitStatus::Modified)
            .count();
        let added_count = self
            .git_changes
            .iter()
            .filter(|g| g.status == GitStatus::Added)
            .count();
        let deleted_count = self
            .git_changes
            .iter()
            .filter(|g| g.status == GitStatus::Deleted)
            .count();

        let git_header_style = if self.focused && git_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(Color::Rgb(45, 60, 83))
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        all_lines.push(Line::from(vec![
            Span::styled(
                format!("{} ", chevron),
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled("Git Changes", git_header_style),
            Span::raw(" "),
            Span::styled(
                format!("{}", git_count),
                Style::default()
                    .fg(self.theme.bg_main)
                    .bg(self.theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        if git_expanded {
            all_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!(
                        "{} Mod | {} New | {} Del",
                        modified_count, added_count, deleted_count
                    ),
                    Style::default().fg(self.theme.text_muted),
                ),
            ]));

            // Show up to 8 files instead of 4
            for (i, change) in self.git_changes.iter().enumerate().take(8) {
                let item_selected =
                    self.focused && self.selected_panel == 3 && self.selected_item == Some(i);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(Color::Rgb(45, 60, 83))
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let status_text = match change.status {
                    GitStatus::Modified => {
                        format!("+{} -{}", change.additions, change.deletions)
                    }
                    GitStatus::Added => "NEW".to_string(),
                    GitStatus::Deleted => "DEL".to_string(),
                };

                let status_color = match change.status {
                    GitStatus::Modified => self.theme.yellow,
                    GitStatus::Added => self.theme.green,
                    GitStatus::Deleted => self.theme.red,
                };

                all_lines.push(Line::from(vec![
                    Span::raw("  📄 "),
                    Span::styled(&change.file, item_style),
                    Span::raw(" "),
                    Span::styled(status_text, Style::default().fg(status_color)),
                ]));
            }

            // Show "...and N more" if there are more files
            if git_count > 8 {
                all_lines.push(Line::from(vec![Span::styled(
                    format!("  ... and {} more", git_count - 8),
                    Style::default().fg(self.theme.text_muted),
                )]));
            }
        }

        // Render final paragraph
        let paragraph = Paragraph::new(all_lines);
        paragraph.render(inner, buf);
    }
}
