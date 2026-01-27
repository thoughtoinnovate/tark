//! Sidebar Widget - Context panels for session info, files, tasks, todos, git
//!
//! Reference: web/ui/mocks/src/app/components/Sidebar.tsx
//! Feature: 13_sidebar.feature
//! Baseline: screenshots/11-sidebar-panel.png

use ratatui::prelude::*;
use ratatui::symbols::border;
use ratatui::widgets::{Block, Borders, Paragraph};

use super::super::theme::Theme;
use crate::core::context_tracker::ContextBreakdown;
use crate::tools::builtin::{TodoItem, TodoStatus};
use crate::ui_backend::PluginWidgetInfo;
use crate::ui_backend::ThemePreset;
use serde_json::Value;

/// Sidebar panel type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    Session,
    Context,
    Tasks,
    Todo,
    GitChanges,
    Plugins,
    Theme,
}

impl SidebarPanel {
    /// Get display name for panel
    pub fn display_name(&self) -> &'static str {
        match self {
            SidebarPanel::Session => "Session",
            SidebarPanel::Context => "Context",
            SidebarPanel::Tasks => "Tasks",
            SidebarPanel::Todo => "Todo",
            SidebarPanel::GitChanges => "Git Changes",
            SidebarPanel::Plugins => "Plugins",
            SidebarPanel::Theme => "Theme",
        }
    }

    /// Get icon for panel
    pub fn icon(&self) -> &'static str {
        match self {
            SidebarPanel::Session => "📊",
            SidebarPanel::Context => "📂",
            SidebarPanel::Tasks => "✓",
            SidebarPanel::Todo => "📋",
            SidebarPanel::GitChanges => "⎇",
            SidebarPanel::Plugins => "🔌",
            SidebarPanel::Theme => "🎨",
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

/// Format token count for display (e.g., 1234 -> "1.2k", 123456 -> "123k")
fn format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f32 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f32 / 1_000.0)
    } else {
        tokens.to_string()
    }
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
    /// Detailed context breakdown by source
    pub context_breakdown: ContextBreakdown,
    /// Tasks (high-level queued tasks)
    pub tasks: Vec<Task>,
    /// Todos (agent's immediate work items)
    pub todos: Vec<TodoItem>,
    /// Git changes
    pub git_changes: Vec<GitChange>,
    /// Plugin widgets
    pub plugin_widgets: Vec<PluginWidgetInfo>,
    /// Current git branch
    pub git_branch: String,
    /// Theme
    pub theme: &'a Theme,
    /// Current theme preset (for icon display)
    pub theme_preset: ThemePreset,
    /// Current theme name for display
    pub current_theme_name: String,
    /// Which panels are expanded (Session, Context, Tasks, Todo, GitChanges, Plugins)
    pub expanded_panels: [bool; 6],
    /// Currently selected panel index
    pub selected_panel: usize,
    /// Selected item within panel (None = panel header selected)
    pub selected_item: Option<usize>,
    /// Whether sidebar has focus
    pub focused: bool,
    /// Current Vim mode
    pub vim_mode: crate::ui_backend::VimMode,
    /// Scroll offset for sidebar content
    pub scroll_offset: usize,
    /// Per-panel scroll offsets
    pub panel_scrolls: [usize; 6],
    /// Index of task being dragged for reordering (within queued tasks)
    pub dragging_task_index: Option<usize>,
    /// Target position for the dragged task
    pub drag_target_index: Option<usize>,
}

/// Session information
#[derive(Debug, Default)]
pub struct SessionInfo {
    pub name: String,
    pub is_remote: bool,
    pub total_cost: f64,
    pub model_count: usize,
    pub model_costs: Vec<(String, f64)>,
    pub total_tokens: usize,
    pub model_tokens: Vec<(String, usize)>,
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
            context_breakdown: ContextBreakdown::default(),
            tasks: Vec::new(),
            todos: Vec::new(),
            git_changes: Vec::new(),
            plugin_widgets: Vec::new(),
            git_branch: String::new(),
            theme,
            theme_preset: ThemePreset::default(),
            current_theme_name: "Catppuccin Mocha".to_string(),
            expanded_panels: [true, true, true, true, true, true], // All expanded by default
            selected_panel: 0,
            selected_item: None,
            focused: false,
            vim_mode: crate::ui_backend::VimMode::Insert,
            scroll_offset: 0,
            panel_scrolls: [0, 0, 0, 0, 0, 0],
            dragging_task_index: None,
            drag_target_index: None,
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

    pub fn theme_preset(mut self, preset: ThemePreset) -> Self {
        self.theme_preset = preset;
        self
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    pub fn expanded(mut self, panel: SidebarPanel, expanded: bool) -> Self {
        let idx = panel as usize;
        if idx < 6 {
            self.expanded_panels[idx] = expanded;
        }
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

    pub fn scroll_offset(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    pub fn panel_scrolls(mut self, scrolls: [usize; 6]) -> Self {
        self.panel_scrolls = scrolls;
        self
    }

    /// Set todos for the todo panel
    pub fn todos(mut self, todos: Vec<TodoItem>) -> Self {
        self.todos = todos;
        self
    }

    /// Set drag state for task reordering
    pub fn drag_state(mut self, dragging: Option<usize>, target: Option<usize>) -> Self {
        self.dragging_task_index = dragging;
        self.drag_target_index = target;
        self
    }

    /// Toggle panel expansion
    pub fn toggle_panel(&mut self, panel_idx: usize) {
        if panel_idx < 6 {
            self.expanded_panels[panel_idx] = !self.expanded_panels[panel_idx];
        }
    }

    /// Navigate to next panel
    pub fn next_panel(&mut self) {
        self.selected_panel = (self.selected_panel + 1) % 7; // 7 panels: Session, Context, Tasks, Todo, GitChanges, Plugins, Theme
        self.selected_item = None;
    }

    /// Navigate to previous panel
    pub fn prev_panel(&mut self) {
        self.selected_panel = if self.selected_panel == 0 {
            6
        } else {
            self.selected_panel - 1
        };
        self.selected_item = None;
    }

    /// Navigate down within current panel
    pub fn next_item(&mut self) {
        // Theme panel (index 6) doesn't have items to navigate
        if self.selected_panel == 6 {
            return;
        }

        if !self.expanded_panels[self.selected_panel] {
            return; // Can't navigate inside collapsed panel
        }

        let max_items = match self.selected_panel {
            0 => 3 + self.session_info.model_costs.len(), // name line + cost line + tokens line + per-model lines
            1 => self.context_files.len(),
            2 => self.tasks.len(),
            3 => self.todos.len(), // Todo panel
            4 => self.git_changes.len(),
            5 => self.plugin_widgets.len(),
            6 => 0, // Theme panel has no items
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

    pub fn context_breakdown(mut self, breakdown: ContextBreakdown) -> Self {
        self.tokens_used = breakdown.total;
        self.tokens_total = breakdown.max_tokens;
        self.context_breakdown = breakdown;
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

    pub fn plugin_widgets(mut self, widgets: Vec<PluginWidgetInfo>) -> Self {
        self.plugin_widgets = widgets;
        self
    }

    pub fn git_branch(mut self, branch: String) -> Self {
        self.git_branch = branch;
        self
    }

    fn render_panel(
        &self,
        area: Rect,
        lines: Vec<Line>,
        scroll: usize,
        selected_line: Option<usize>,
        show_scrollbar: bool,
        buf: &mut Buffer,
    ) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        let total_lines = lines.len();
        let visible_height = area.height as usize;
        let max_scroll = total_lines.saturating_sub(visible_height);
        let mut scroll_pos = scroll.min(max_scroll);
        if let Some(selected_line) = selected_line {
            let selected_line = selected_line.min(total_lines.saturating_sub(1));
            if selected_line < scroll_pos {
                scroll_pos = selected_line;
            } else if selected_line >= scroll_pos.saturating_add(visible_height) {
                scroll_pos = selected_line.saturating_sub(visible_height.saturating_sub(1));
            }
        }

        let paragraph = Paragraph::new(lines).scroll((scroll_pos as u16, 0));
        paragraph.render(area, buf);

        if show_scrollbar && total_lines > visible_height {
            use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(self.theme.text_muted))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(max_scroll).position(scroll_pos);
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y,
                width: 1,
                height: area.height,
            };

            ratatui::widgets::StatefulWidget::render(
                scrollbar,
                scrollbar_area,
                buf,
                &mut scrollbar_state,
            );
        }
    }

    fn push_line<'b>(target: &mut Vec<Line<'b>>, all: &mut Vec<Line<'b>>, line: Line<'b>) {
        target.push(line.clone());
        all.push(line);
    }

    /// Truncate text to fit within available width, adding "..." if truncated
    /// Leaves 2% padding on the right edge
    fn truncate_text(text: &str, available_width: u16, prefix_len: usize) -> String {
        // Calculate max text width: available - prefix - 2% right padding (min 1 char)
        let right_padding = (available_width as usize * 2 / 100).max(1);
        let max_width = (available_width as usize)
            .saturating_sub(prefix_len)
            .saturating_sub(right_padding);

        if max_width < 4 {
            // Not enough space for meaningful text
            return String::new();
        }

        if text.chars().count() <= max_width {
            text.to_string()
        } else {
            // Truncate and add "..."
            let truncate_at = max_width.saturating_sub(3);
            let truncated: String = text.chars().take(truncate_at).collect();
            format!("{}...", truncated)
        }
    }

    fn flatten_json(value: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
        match value {
            Value::Object(map) => {
                for (key, val) in map {
                    let next = if prefix.is_empty() {
                        key.to_string()
                    } else {
                        format!("{}.{}", prefix, key)
                    };
                    Self::flatten_json(val, &next, out);
                }
            }
            Value::Array(list) => {
                let mut rendered = Vec::new();
                for item in list {
                    rendered.push(match item {
                        Value::String(s) => s.clone(),
                        _ => item.to_string(),
                    });
                }
                out.push((prefix.to_string(), rendered.join(", ")));
            }
            Value::Null => out.push((prefix.to_string(), "null".to_string())),
            Value::Bool(b) => out.push((prefix.to_string(), b.to_string())),
            Value::Number(n) => out.push((prefix.to_string(), n.to_string())),
            Value::String(s) => out.push((prefix.to_string(), s.clone())),
        }
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
            .border_set(border::ROUNDED)
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
        let mut header_lines: Vec<Line> = vec![];
        let mut session_lines: Vec<Line> = vec![];
        let mut context_lines: Vec<Line> = vec![];
        let mut tasks_lines: Vec<Line> = vec![];
        let mut todo_lines: Vec<Line> = vec![];
        let mut git_lines: Vec<Line> = vec![];
        let mut plugin_lines: Vec<Line> = vec![];
        let mut panel_item_lines: [Vec<usize>; 6] = std::array::from_fn(|_| Vec::new());

        // Available width for content (used for truncation)
        let content_width = inner.width;
        let mut footer_lines: Vec<Line> = vec![];

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
        Self::push_line(
            &mut header_lines,
            &mut all_lines,
            Line::from(vec![
                Span::styled("  VIM: ", Style::default().fg(self.theme.text_muted)),
                Span::styled(vim_str, Style::default().fg(vim_color)),
            ]),
        );
        Self::push_line(&mut header_lines, &mut all_lines, Line::from(""));

        // ======== SESSION SECTION ========
        let session_expanded = self.expanded_panels[0];
        let session_selected = self.selected_panel == 0 && self.selected_item.is_none();
        let chevron = if session_expanded { "▼" } else { "▶" };

        let session_header_style = if self.focused && session_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        Self::push_line(
            &mut session_lines,
            &mut all_lines,
            Line::from(vec![
                Span::styled(
                    format!("{} ", chevron),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled("Session", session_header_style),
            ]),
        );

        if session_expanded {
            let mut session_item_idx = 0usize;
            // Show session name
            if !self.session_info.name.is_empty() {
                let item_selected = self.focused
                    && self.selected_panel == 0
                    && self.selected_item == Some(session_item_idx);
                let name_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };
                let line_idx = session_lines.len();
                panel_item_lines[0].push(line_idx);
                let mut name_spans = vec![
                    Span::raw("  "),
                    Span::styled(&self.session_info.name, name_style),
                ];
                if self.session_info.is_remote {
                    name_spans.push(Span::raw(" "));
                    name_spans.push(Span::styled(
                        "📡",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
                Self::push_line(
                    &mut session_lines,
                    &mut all_lines,
                    Line::from(name_spans),
                );
                session_item_idx += 1;
            }
            let item_selected = self.focused
                && self.selected_panel == 0
                && self.selected_item == Some(session_item_idx);
            let cost_style = if item_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .bg(self.theme.selection_bg)
            } else {
                Style::default().fg(self.theme.text_muted)
            };
            let line_idx = session_lines.len();
            panel_item_lines[0].push(line_idx);
            Self::push_line(
                &mut session_lines,
                &mut all_lines,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!(
                            "${:.3} ({} models) ▼",
                            self.session_info.total_cost, self.session_info.model_count
                        ),
                        cost_style,
                    ),
                ]),
            );
            session_item_idx += 1;
            let item_selected = self.focused
                && self.selected_panel == 0
                && self.selected_item == Some(session_item_idx);
            let tokens_style = if item_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .bg(self.theme.selection_bg)
            } else {
                Style::default().fg(self.theme.text_muted)
            };
            let line_idx = session_lines.len();
            panel_item_lines[0].push(line_idx);
            Self::push_line(
                &mut session_lines,
                &mut all_lines,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{} tokens total", self.session_info.total_tokens),
                        tokens_style,
                    ),
                ]),
            );
            session_item_idx += 1;
            if !self.session_info.model_costs.is_empty() {
                for (model, cost) in &self.session_info.model_costs {
                    let model_tokens = self
                        .session_info
                        .model_tokens
                        .iter()
                        .find(|(name, _)| name == model)
                        .map(|(_, tokens)| *tokens)
                        .unwrap_or(0);
                    let item_selected = self.focused
                        && self.selected_panel == 0
                        && self.selected_item == Some(session_item_idx);
                    let model_style = if item_selected {
                        Style::default()
                            .fg(self.theme.cyan)
                            .bg(self.theme.selection_bg)
                    } else {
                        Style::default().fg(self.theme.text_secondary)
                    };
                    let cost_style = if item_selected {
                        Style::default()
                            .fg(self.theme.cyan)
                            .bg(self.theme.selection_bg)
                    } else {
                        Style::default().fg(self.theme.text_muted)
                    };
                    let line_idx = session_lines.len();
                    panel_item_lines[0].push(line_idx);
                    Self::push_line(
                        &mut session_lines,
                        &mut all_lines,
                        Line::from(vec![
                            Span::raw("    "),
                            Span::styled(model, model_style),
                            Span::raw(" "),
                            Span::styled(
                                format!("${:.3} · {} tok", cost, model_tokens),
                                cost_style,
                            ),
                        ]),
                    );
                    session_item_idx += 1;
                }
            }
        }
        Self::push_line(&mut session_lines, &mut all_lines, Line::from(""));

        // ======== CONTEXT SECTION ========
        let context_expanded = self.expanded_panels[1];
        let context_selected = self.selected_panel == 1 && self.selected_item.is_none();
        let chevron = if context_expanded { "▼" } else { "▶" };
        let context_count = self.context_files.len();

        let context_header_style = if self.focused && context_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        Self::push_line(
            &mut context_lines,
            &mut all_lines,
            Line::from(vec![
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
            ]),
        );

        if context_expanded {
            // Calculate usage percentage
            let usage_percent = if self.tokens_total > 0 {
                self.tokens_used as f32 / self.tokens_total as f32 * 100.0
            } else {
                0.0
            };

            let usage_color = if usage_percent < 60.0 {
                self.theme.green
            } else if usage_percent < 80.0 {
                self.theme.yellow
            } else {
                self.theme.red
            };

            // Token count line
            Self::push_line(
                &mut context_lines,
                &mut all_lines,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!(
                            "{} / {} tokens ({:.1}%)",
                            format_tokens(self.tokens_used),
                            format_tokens(self.tokens_total),
                            usage_percent
                        ),
                        Style::default().fg(usage_color),
                    ),
                ]),
            );

            // Visual progress bar
            let bar_width = 20usize; // Fixed width for consistent appearance
            let filled = ((bar_width as f32) * (usage_percent / 100.0)).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar_str = format!("  {}{}", "█".repeat(filled), "░".repeat(empty));

            Self::push_line(
                &mut context_lines,
                &mut all_lines,
                Line::from(Span::styled(bar_str, Style::default().fg(usage_color))),
            );

            // Breakdown by source (only show if we have meaningful data)
            let breakdown = &self.context_breakdown;
            if breakdown.total > 0 {
                Self::push_line(
                    &mut context_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled("Sys: ", Style::default().fg(self.theme.text_muted)),
                        Span::styled(
                            format_tokens(breakdown.system_prompt),
                            Style::default().fg(self.theme.cyan),
                        ),
                        Span::styled("  Hist: ", Style::default().fg(self.theme.text_muted)),
                        Span::styled(
                            format_tokens(breakdown.conversation_history),
                            Style::default().fg(self.theme.blue),
                        ),
                    ]),
                );

                // Second breakdown line for tools and files
                if breakdown.tool_schemas > 0 || breakdown.attachments > 0 {
                    Self::push_line(
                        &mut context_lines,
                        &mut all_lines,
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled("Tools: ", Style::default().fg(self.theme.text_muted)),
                            Span::styled(
                                format_tokens(breakdown.tool_schemas),
                                Style::default().fg(self.theme.purple),
                            ),
                            Span::styled("  Files: ", Style::default().fg(self.theme.text_muted)),
                            Span::styled(
                                format_tokens(breakdown.attachments),
                                Style::default().fg(self.theme.yellow),
                            ),
                        ]),
                    );
                }
            }

            Self::push_line(
                &mut context_lines,
                &mut all_lines,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("LOADED CONTEXT ({})", context_count),
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
            );

            for (i, file) in self.context_files.iter().enumerate() {
                let item_selected =
                    self.focused && self.selected_panel == 1 && self.selected_item == Some(i);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };
                let icon = if file.ends_with('/') { "📁" } else { "📄" };

                let line_idx = context_lines.len();
                panel_item_lines[1].push(line_idx);
                Self::push_line(
                    &mut context_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::raw(format!("  {} ", icon)),
                        Span::styled(file, item_style),
                    ]),
                );
            }
        }
        Self::push_line(&mut context_lines, &mut all_lines, Line::from(""));

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

        // Golden/amber header when there are active tasks
        let has_active_tasks = !active_tasks.is_empty();
        let tasks_header_color = if has_active_tasks {
            self.theme.yellow // Golden/amber for active tasks
        } else if self.focused && tasks_selected {
            self.theme.cyan
        } else {
            self.theme.text_primary
        };

        let tasks_header_style = if self.focused && tasks_selected {
            Style::default()
                .fg(tasks_header_color)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(tasks_header_color)
                .add_modifier(Modifier::BOLD)
        };

        // Badge color also reflects active state
        let badge_color = if has_active_tasks {
            self.theme.yellow
        } else {
            self.theme.blue
        };

        Self::push_line(
            &mut tasks_lines,
            &mut all_lines,
            Line::from(vec![
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
                        .bg(badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        );

        if tasks_expanded {
            let mut task_idx = 0;

            // Active tasks with filled green circle (●) and "Active" label below
            for task in active_tasks.iter() {
                let item_selected = self.focused
                    && self.selected_panel == 2
                    && self.selected_item == Some(task_idx);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    // Active tasks get highlight color
                    Style::default().fg(self.theme.green)
                };

                // Green filled circle (●) for active tasks
                // Truncate task name to fit within available width (prefix "  ● " = 4 chars)
                let truncated_name = Self::truncate_text(&task.name, content_width, 4);
                let line_idx = tasks_lines.len();
                panel_item_lines[2].push(line_idx);
                Self::push_line(
                    &mut tasks_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::styled(
                            "  ● ",
                            Style::default()
                                .fg(self.theme.green)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(truncated_name, item_style),
                    ]),
                );
                // "Active" label below the task name
                Self::push_line(
                    &mut tasks_lines,
                    &mut all_lines,
                    Line::from(vec![Span::styled(
                        "    Active",
                        Style::default().fg(self.theme.green),
                    )]),
                );
                task_idx += 1;
            }

            // Completed tasks with checkmark
            let completed_tasks: Vec<_> = self
                .tasks
                .iter()
                .filter(|t| t.status == TaskStatus::Completed)
                .collect();

            for task in completed_tasks.iter() {
                let item_selected = self.focused
                    && self.selected_panel == 2
                    && self.selected_item == Some(task_idx);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_muted)
                };

                // Truncate task name to fit within available width (prefix "  ✓ " = 4 chars)
                let truncated_name = Self::truncate_text(&task.name, content_width, 4);
                let line_idx = tasks_lines.len();
                panel_item_lines[2].push(line_idx);
                Self::push_line(
                    &mut tasks_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::styled("  ✓ ", Style::default().fg(self.theme.green)),
                        Span::styled(truncated_name, item_style),
                    ]),
                );
                task_idx += 1;
            }

            // Queued tasks with unchecked circle icon
            if !queued_tasks.is_empty() {
                Self::push_line(
                    &mut tasks_lines,
                    &mut all_lines,
                    Line::from(vec![Span::styled(
                        "  QUEUED",
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::BOLD),
                    )]),
                );

                let is_dragging = self.dragging_task_index.is_some();

                for (queue_idx, task) in queued_tasks.iter().enumerate() {
                    let item_selected = self.focused
                        && self.selected_panel == 2
                        && self.selected_item == Some(task_idx);

                    // Check if this item is being dragged
                    let is_dragged_item = self.dragging_task_index == Some(queue_idx);
                    // Check if this is the drop target
                    let is_drop_target = is_dragging
                        && self.drag_target_index == Some(queue_idx)
                        && self.dragging_task_index != Some(queue_idx);

                    let item_style = if is_dragged_item {
                        // Dragged item: dimmed with italic
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::ITALIC)
                    } else if is_drop_target {
                        // Drop target: highlighted with underline
                        Style::default()
                            .fg(self.theme.yellow)
                            .add_modifier(Modifier::UNDERLINED)
                    } else if item_selected {
                        Style::default()
                            .fg(self.theme.cyan)
                            .bg(self.theme.selection_bg)
                    } else {
                        Style::default().fg(self.theme.text_secondary)
                    };

                    // Queued tasks show:
                    // - Drag indicator (↕) when item is being dragged
                    // - Drop indicator (→) when this is the drop target
                    // - Selection indicator (▸) when selected
                    // - Gray empty circle (○)
                    // - Task name
                    // - Action icons (≡ x) only on selected row (not while dragging)
                    let prefix = if is_dragged_item {
                        " ↕"
                    } else if is_drop_target {
                        " →"
                    } else if item_selected {
                        " ▸"
                    } else {
                        "  "
                    };

                    // When selected (and not dragging), need extra space for action icons (≡ x = 4 chars)
                    let icon_space = if item_selected && !is_dragging { 5 } else { 0 };
                    let truncated_name =
                        Self::truncate_text(&task.name, content_width, 4 + icon_space);

                    let prefix_style = if is_dragged_item {
                        Style::default().fg(self.theme.yellow)
                    } else if is_drop_target {
                        Style::default()
                            .fg(self.theme.green)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.cyan)
                    };

                    let mut spans = vec![
                        Span::styled(prefix, prefix_style),
                        Span::styled("○ ", Style::default().fg(self.theme.text_muted)),
                        Span::styled(truncated_name, item_style),
                    ];

                    // Add action icons only on selected row (not while dragging)
                    if item_selected && !is_dragging {
                        spans.push(Span::styled(
                            " ≡",
                            Style::default().fg(self.theme.text_muted),
                        ));
                        spans.push(Span::styled(" x", Style::default().fg(self.theme.red)));
                    }

                    let line_idx = tasks_lines.len();
                    panel_item_lines[2].push(line_idx);
                    Self::push_line(&mut tasks_lines, &mut all_lines, Line::from(spans));
                    task_idx += 1;
                }
            }
        }
        Self::push_line(&mut tasks_lines, &mut all_lines, Line::from(""));

        // ======== TODO SECTION ========
        let todo_expanded = self.expanded_panels[3];
        let todo_selected = self.selected_panel == 3 && self.selected_item.is_none();
        let chevron = if todo_expanded { "▼" } else { "▶" };
        let todo_count = self.todos.len();
        let completed_todos = self
            .todos
            .iter()
            .filter(|t| t.status == TodoStatus::Completed)
            .count();

        // Calculate progress for badge color
        let todo_progress = if todo_count > 0 {
            (completed_todos as f32 / todo_count as f32) * 100.0
        } else {
            0.0
        };

        let todo_header_style = if self.focused && todo_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        // Badge color based on progress
        let todo_badge_color = if todo_progress < 33.0 {
            self.theme.red
        } else if todo_progress < 66.0 {
            self.theme.yellow
        } else {
            self.theme.green
        };

        Self::push_line(
            &mut todo_lines,
            &mut all_lines,
            Line::from(vec![
                Span::styled(
                    format!("{} ", chevron),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled("📋 Todo", todo_header_style),
                Span::raw(" "),
                Span::styled(
                    format!("{}/{}", completed_todos, todo_count),
                    Style::default()
                        .fg(self.theme.bg_main)
                        .bg(todo_badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        );

        if todo_expanded && !self.todos.is_empty() {
            // Progress bar
            let bar_width = 20usize;
            let filled = ((bar_width as f32) * (todo_progress / 100.0)).round() as usize;
            let empty = bar_width.saturating_sub(filled);
            let bar_str = format!("  {}{}", "█".repeat(filled), "░".repeat(empty));

            Self::push_line(
                &mut todo_lines,
                &mut all_lines,
                Line::from(Span::styled(bar_str, Style::default().fg(todo_badge_color))),
            );

            // Todo items
            for (i, item) in self.todos.iter().enumerate() {
                let item_selected =
                    self.focused && self.selected_panel == 3 && self.selected_item == Some(i);

                // Status icon and color
                let (icon, status_color) = match item.status {
                    TodoStatus::Pending => ("○", self.theme.text_muted),
                    TodoStatus::InProgress => ("●", self.theme.yellow),
                    TodoStatus::Completed => ("✓", self.theme.green),
                    TodoStatus::Cancelled => ("✗", self.theme.red),
                };

                let mut item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                // Strikethrough for cancelled items
                if item.status == TodoStatus::Cancelled {
                    item_style = item_style
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::CROSSED_OUT);
                }

                // Dim completed items
                if item.status == TodoStatus::Completed && !item_selected {
                    item_style = item_style.fg(self.theme.text_muted);
                }

                // Truncate content to fit
                let truncated_content = Self::truncate_text(&item.content, content_width, 4);

                let line_idx = todo_lines.len();
                panel_item_lines[3].push(line_idx);
                Self::push_line(
                    &mut todo_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::styled(format!("  {} ", icon), Style::default().fg(status_color)),
                        Span::styled(truncated_content, item_style),
                    ]),
                );
            }
        }
        Self::push_line(&mut todo_lines, &mut all_lines, Line::from(""));

        // ======== GIT CHANGES SECTION ========
        let git_expanded = self.expanded_panels[4];
        let git_selected = self.selected_panel == 4 && self.selected_item.is_none();
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
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        Self::push_line(
            &mut git_lines,
            &mut all_lines,
            Line::from(vec![
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
            ]),
        );

        if git_expanded {
            // Show current branch
            if !self.git_branch.is_empty() {
                Self::push_line(
                    &mut git_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled("⎇ ", Style::default().fg(self.theme.purple)),
                        Span::styled(&self.git_branch, Style::default().fg(self.theme.blue)),
                    ]),
                );
            }

            // Summary: modified | added | deleted
            Self::push_line(
                &mut git_lines,
                &mut all_lines,
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled("M", Style::default().fg(self.theme.yellow)),
                    Span::styled(
                        format!("{}", modified_count),
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::raw(" "),
                    Span::styled("A", Style::default().fg(self.theme.green)),
                    Span::styled(
                        format!("{}", added_count),
                        Style::default().fg(self.theme.text_muted),
                    ),
                    Span::raw(" "),
                    Span::styled("D", Style::default().fg(self.theme.red)),
                    Span::styled(
                        format!("{}", deleted_count),
                        Style::default().fg(self.theme.text_muted),
                    ),
                ]),
            );

            // Show all files with status icons per mock design
            for (i, change) in self.git_changes.iter().enumerate() {
                let item_selected =
                    self.focused && self.selected_panel == 4 && self.selected_item == Some(i);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                // Status icon: M (yellow), A (green), D (red) per mock design
                let (status_icon, status_color) = match change.status {
                    GitStatus::Modified => ("M", self.theme.yellow),
                    GitStatus::Added => ("A", self.theme.green),
                    GitStatus::Deleted => ("D", self.theme.red),
                };

                // Show +X -Y for modified files
                let diff_text = if change.status == GitStatus::Modified {
                    format!(" +{} -{}", change.additions, change.deletions)
                } else {
                    String::new()
                };

                let line_idx = git_lines.len();
                panel_item_lines[4].push(line_idx);
                Self::push_line(
                    &mut git_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{} ", status_icon),
                            Style::default()
                                .fg(status_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(&change.file, item_style),
                        Span::styled(diff_text, Style::default().fg(self.theme.text_muted)),
                    ]),
                );
            }

            // No truncation; rely on sidebar scroll
        }

        // ======== PLUGINS SECTION ========
        let plugins_expanded = self.expanded_panels[5];
        let plugins_selected = self.selected_panel == 5 && self.selected_item.is_none();
        let chevron = if plugins_expanded { "▼" } else { "▶" };

        let plugins_header_style = if self.focused && plugins_selected {
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(self.theme.text_primary)
                .add_modifier(Modifier::BOLD)
        };

        Self::push_line(
            &mut plugin_lines,
            &mut all_lines,
            Line::from(vec![
                Span::styled(
                    format!("{} ", chevron),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled("Plugins", plugins_header_style),
                Span::raw(" "),
                Span::styled(
                    format!("{}", self.plugin_widgets.len()),
                    Style::default()
                        .fg(self.theme.bg_main)
                        .bg(self.theme.purple)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        );

        if plugins_expanded {
            if self.plugin_widgets.is_empty() {
                Self::push_line(
                    &mut plugin_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled("(no widgets)", Style::default().fg(self.theme.text_muted)),
                    ]),
                );
            } else {
                for (i, widget) in self.plugin_widgets.iter().enumerate() {
                    let item_selected =
                        self.focused && self.selected_panel == 5 && self.selected_item == Some(i);
                    let item_style = if item_selected {
                        Style::default()
                            .fg(self.theme.cyan)
                            .bg(self.theme.selection_bg)
                    } else {
                        Style::default().fg(self.theme.text_primary)
                    };

                    let status = widget.status.as_deref().unwrap_or("unknown").to_string();
                    let status_color = if status.eq_ignore_ascii_case("connected") {
                        self.theme.green
                    } else if status.eq_ignore_ascii_case("disconnected") {
                        self.theme.red
                    } else {
                        self.theme.text_muted
                    };
                    let line_idx = plugin_lines.len();
                    panel_item_lines[5].push(line_idx);
                    Self::push_line(
                        &mut plugin_lines,
                        &mut all_lines,
                        Line::from(vec![
                            Span::raw("  "),
                            Span::styled("● ", Style::default().fg(status_color)),
                            Span::styled(&widget.plugin_id, item_style),
                            Span::raw(" "),
                            Span::styled(
                                format!("[{}]", status),
                                Style::default().fg(self.theme.text_muted),
                            ),
                        ]),
                    );

                    if let Some(err) = widget.error.as_ref() {
                        let truncated = Self::truncate_text(err, content_width, 4);
                        Self::push_line(
                            &mut plugin_lines,
                            &mut all_lines,
                            Line::from(vec![
                                Span::raw("    "),
                                Span::styled("error: ", Style::default().fg(self.theme.red)),
                                Span::styled(truncated, Style::default().fg(self.theme.text_muted)),
                            ]),
                        );
                        continue;
                    }

                    let mut fields = Vec::new();
                    Self::flatten_json(&widget.attributes, "", &mut fields);
                    for (key, value) in fields {
                        let key_text = Self::truncate_text(&key, content_width, 6);
                        let value_text =
                            Self::truncate_text(&value, content_width, 6 + key_text.len());
                        Self::push_line(
                            &mut plugin_lines,
                            &mut all_lines,
                            Line::from(vec![
                                Span::raw("    "),
                                Span::styled(
                                    format!("{}: ", key_text),
                                    Style::default().fg(self.theme.text_muted),
                                ),
                                Span::styled(
                                    value_text,
                                    Style::default().fg(self.theme.text_secondary),
                                ),
                            ]),
                        );
                    }
                }
            }
        }

        Self::push_line(&mut plugin_lines, &mut all_lines, Line::from(""));

        // Footer section: theme icon only, navigable
        let footer_selected = self.focused && self.selected_panel == 6;
        let footer_style = if footer_selected {
            Style::default()
                .fg(self.theme.cyan)
                .bg(self.theme.selection_bg)
        } else {
            Style::default().fg(self.theme.cyan)
        };

        // Get icon from current theme preset
        let theme_icon = self.theme_preset.icon();

        footer_lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(theme_icon, footer_style),
            Span::styled(" ▼", Style::default().fg(self.theme.text_muted)),
        ]));

        // Layout with dynamic panel heights based on actual content
        let session_height = if session_expanded {
            session_lines.len() as u16
        } else {
            1 // Just header when collapsed
        };
        let context_height = if context_expanded {
            context_lines.len() as u16
        } else {
            1
        };
        let tasks_height = if tasks_expanded {
            tasks_lines.len() as u16
        } else {
            1
        };
        let todo_height = if todo_expanded {
            todo_lines.len() as u16
        } else {
            1
        };
        let git_height = if git_expanded {
            git_lines.len() as u16
        } else {
            1
        };
        let plugins_height = if plugins_expanded {
            plugin_lines.len() as u16
        } else {
            1
        };

        let panel_constraints = [
            Constraint::Length(header_lines.len() as u16),
            Constraint::Length(session_height),
            Constraint::Length(context_height),
            Constraint::Length(tasks_height),
            Constraint::Length(todo_height),
            Constraint::Length(git_height),
            Constraint::Length(plugins_height),
            Constraint::Length(footer_lines.len() as u16),
        ];
        let panel_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(panel_constraints)
            .split(inner);

        let header_area = panel_chunks[0];
        let session_area = panel_chunks[1];
        let context_area = panel_chunks[2];
        let tasks_area = panel_chunks[3];
        let todo_area = panel_chunks[4];
        let git_area = panel_chunks[5];
        let plugins_area = panel_chunks[6];
        let footer_area = panel_chunks[7];

        let mut panel_selected_lines: [Option<usize>; 6] = [None, None, None, None, None, None];
        if self.focused {
            if let Some(selected_item) = self.selected_item {
                let panel_idx = self.selected_panel;
                if panel_idx < 6 {
                    let indices = &panel_item_lines[panel_idx];
                    panel_selected_lines[panel_idx] = indices
                        .get(selected_item)
                        .copied()
                        .or_else(|| indices.last().copied());
                }
            }
        }

        let panel_has_focus = self.focused
            && self.selected_panel < 6
            && (self.selected_item.is_some() || self.expanded_panels[self.selected_panel]);

        Paragraph::new(header_lines).render(header_area, buf);
        self.render_panel(
            session_area,
            session_lines,
            self.panel_scrolls[0],
            panel_selected_lines[0],
            panel_has_focus && self.selected_panel == 0,
            buf,
        );
        self.render_panel(
            context_area,
            context_lines,
            self.panel_scrolls[1],
            panel_selected_lines[1],
            panel_has_focus && self.selected_panel == 1,
            buf,
        );
        self.render_panel(
            tasks_area,
            tasks_lines,
            self.panel_scrolls[2],
            panel_selected_lines[2],
            panel_has_focus && self.selected_panel == 2,
            buf,
        );
        self.render_panel(
            todo_area,
            todo_lines,
            self.panel_scrolls[3],
            panel_selected_lines[3],
            panel_has_focus && self.selected_panel == 3,
            buf,
        );
        self.render_panel(
            git_area,
            git_lines,
            self.panel_scrolls[4],
            panel_selected_lines[4],
            panel_has_focus && self.selected_panel == 4,
            buf,
        );
        self.render_panel(
            plugins_area,
            plugin_lines,
            self.panel_scrolls[5],
            panel_selected_lines[5],
            panel_has_focus && self.selected_panel == 5,
            buf,
        );
        Paragraph::new(footer_lines).render(footer_area, buf);

        // Global scrollbar (overall sidebar)
        let total_lines = all_lines.len();
        let visible_height = inner.height as usize;
        if total_lines > visible_height && !panel_has_focus {
            use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

            let max = total_lines.saturating_sub(visible_height);
            let position = self.scroll_offset.min(max);
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(self.theme.text_muted))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(max).position(position);
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            ratatui::widgets::StatefulWidget::render(
                scrollbar,
                scrollbar_area,
                buf,
                &mut scrollbar_state,
            );
        }
    }
}
