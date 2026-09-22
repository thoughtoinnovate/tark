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

/// Sidebar panel type.
///
/// Discriminant order matches `SharedState::sidebar_selected_panel`
/// (plan §C1): Subagents sits between Tasks and Todo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    Session,
    Context,
    Tasks,
    Subagents,
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
            SidebarPanel::Subagents => "Subagents",
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
            SidebarPanel::Subagents => "⑂",
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

/// Clickable geometry recorded during the last sidebar render (P2.4).
///
/// Hit-testing must resolve clicks against what was actually drawn: panel
/// heights and item rows depend on live content (task counts, file lists),
/// so formula-based estimates drift. The renderer keeps the latest map and
/// resolves sidebar clicks from it.
#[derive(Debug, Default, Clone)]
pub struct SidebarClickMap {
    /// VIM header strip (not clickable; clicks here resolve to nothing).
    pub header: Option<Rect>,
    /// Panel header+content areas: 0=Session, 1=Context, 2=Tasks,
    /// 3=Subagents, 4=Todo, 5=Git, 6=Plugins.
    pub panels: [Option<Rect>; 7],
    /// Theme footer strip (panel index 7 in selection terms).
    pub footer: Option<Rect>,
    /// Visible clickable item rows per panel: `items[p][i]` is the screen row
    /// of the `i`-th clickable item in panel `p` (accounts for panel scroll).
    /// For the Tasks panel (`items[2]`), position `i` is the display task
    /// index in render order (active, completed, queued). Subagents
    /// (`items[3]`) registers title rows only — muted preview lines below
    /// each row are display-only, so positions stay 1:1 with `selected_item`.
    pub items: [Vec<Rect>; 7],
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
    /// Subagents (lightweight child runs, plan §C1)
    pub subagents: Vec<crate::ui_backend::SubagentInfo>,
    pub subagent_queued: Vec<crate::ui_backend::QueuedSubagent>,
    pub subagent_effective: usize,
    pub subagent_auto: bool,
    pub subagent_other_sessions: usize,
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
    /// Which panels are expanded (Session, Context, Tasks, Subagents, Todo,
    /// GitChanges, Plugins; Theme footer exempt). Indices match
    /// `SharedState::sidebar_selected_panel` (plan §C1).
    pub expanded_panels: [bool; 7],
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
    pub panel_scrolls: [usize; 7],
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
            subagents: Vec::new(),
            subagent_queued: Vec::new(),
            subagent_effective: 5,
            subagent_auto: true,
            subagent_other_sessions: 0,
            todos: Vec::new(),
            git_changes: Vec::new(),
            plugin_widgets: Vec::new(),
            git_branch: String::new(),
            theme,
            theme_preset: ThemePreset::default(),
            current_theme_name: "Catppuccin Mocha".to_string(),
            expanded_panels: [true, true, false, true, false, false, false],
            selected_panel: 0,
            selected_item: None,
            focused: false,
            vim_mode: crate::ui_backend::VimMode::Insert,
            scroll_offset: 0,
            panel_scrolls: [0, 0, 0, 0, 0, 0, 0],
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
        if idx < 7 {
            self.expanded_panels[idx] = expanded;
        }
        self
    }

    pub fn subagents(
        mut self,
        subagents: Vec<crate::ui_backend::SubagentInfo>,
        queued: Vec<crate::ui_backend::QueuedSubagent>,
        effective: usize,
        auto: bool,
        other_sessions: usize,
    ) -> Self {
        self.subagents = subagents;
        self.subagent_queued = queued;
        self.subagent_effective = effective;
        self.subagent_auto = auto;
        self.subagent_other_sessions = other_sessions;
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

    pub fn panel_scrolls(mut self, scrolls: [usize; 7]) -> Self {
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
        // 8 panels: Session, Context, Tasks, Subagents, Todo, GitChanges, Plugins, Theme
        self.selected_panel = (self.selected_panel + 1) % 8;
        self.selected_item = None;
    }

    /// Navigate to previous panel
    pub fn prev_panel(&mut self) {
        self.selected_panel = if self.selected_panel == 0 {
            7
        } else {
            self.selected_panel - 1
        };
        self.selected_item = None;
    }

    /// Navigate down within current panel
    pub fn next_item(&mut self) {
        // Theme panel (index 7) doesn't have items to navigate
        if self.selected_panel == 7 {
            return;
        }

        if !self.expanded_panels[self.selected_panel] {
            return; // Can't navigate inside collapsed panel
        }

        let max_items = match self.selected_panel {
            0 => 3 + self.session_info.model_costs.len(), // name line + cost line + tokens line + per-model lines
            1 => self.context_files.len(),
            2 => self.tasks.len(),
            3 => self.subagents.len() + self.subagent_queued.len(), // Subagents panel
            4 => self.todos.len(),                                  // Todo panel
            5 => self.git_changes.len(),
            6 => self.plugin_widgets.len(),
            7 => 0, // Theme panel has no items
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
        // Theme footer (index 7) has no expansion flag or items.
        if self.selected_panel >= self.expanded_panels.len() {
            return;
        }
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

    /// Resolve the effective scroll offset for a panel, shared by rendering
    /// and click-map recording so both always agree (P2.4).
    fn panel_scroll_pos(
        total_lines: usize,
        visible_height: usize,
        scroll: usize,
        selected_line: Option<usize>,
    ) -> usize {
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
        scroll_pos
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
        let scroll_pos = Self::panel_scroll_pos(total_lines, visible_height, scroll, selected_line);

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

impl<'a> Sidebar<'a> {
    /// Render while recording clickable geometry into `map` (P2.4).
    ///
    /// Panel heights and item rows depend on live content, so hit-testing
    /// must use these recorded rects instead of estimated heights.
    pub fn render_with_map(self, area: Rect, buf: &mut Buffer, map: &mut SidebarClickMap) {
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
        let mut subagents_lines: Vec<Line> = vec![];
        let mut todo_lines: Vec<Line> = vec![];
        let mut git_lines: Vec<Line> = vec![];
        let mut plugin_lines: Vec<Line> = vec![];
        let mut panel_item_lines: [Vec<usize>; 7] = std::array::from_fn(|_| Vec::new());

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
                Self::push_line(&mut session_lines, &mut all_lines, Line::from(name_spans));
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

        // ======== SUBAGENTS SECTION (plan §C1) ========
        // Panel index 3, between Tasks and Todo. Rows: active children then
        // queued spawns. `panel_item_lines[3]` drives j/k + click mapping.
        let subagents_expanded = self.expanded_panels[3];
        let subagents_selected = self.selected_panel == 3 && self.selected_item.is_none();
        let chevron = if subagents_expanded { "▼" } else { "▶" };
        let running_count = self
            .subagents
            .iter()
            .filter(|s| {
                matches!(
                    s.status,
                    crate::ui_backend::SubagentStatus::Running
                        | crate::ui_backend::SubagentStatus::WaitingInput
                )
            })
            .count();
        let total_count = self.subagents.len() + self.subagent_queued.len();

        let subagents_header_color = if running_count > 0 {
            self.theme.yellow // Amber while work is running
        } else if self.focused && subagents_selected {
            self.theme.cyan
        } else {
            self.theme.text_primary
        };
        let subagents_header_style = if self.focused && subagents_selected {
            Style::default()
                .fg(subagents_header_color)
                .add_modifier(Modifier::BOLD)
                .bg(self.theme.selection_bg)
        } else {
            Style::default()
                .fg(subagents_header_color)
                .add_modifier(Modifier::BOLD)
        };
        let sub_badge_color = if running_count > 0 {
            self.theme.yellow
        } else {
            self.theme.blue
        };
        // Badge shows n/effective + mode marker (• auto, F manual).
        let mode_marker = if self.subagent_auto { "•" } else { "F" };
        Self::push_line(
            &mut subagents_lines,
            &mut all_lines,
            Line::from(vec![
                Span::styled(
                    format!("{} ", chevron),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled("⑂ Subagents", subagents_header_style),
                Span::raw(" "),
                Span::styled(
                    format!("{}/{}{}", total_count, self.subagent_effective, mode_marker),
                    Style::default()
                        .fg(self.theme.bg_main)
                        .bg(sub_badge_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
        );

        if subagents_expanded {
            let mut sub_idx = 0;
            for agent in self.subagents.iter() {
                let item_selected =
                    self.focused && self.selected_panel == 3 && self.selected_item == Some(sub_idx);
                let (glyph, glyph_color) = match agent.status {
                    crate::ui_backend::SubagentStatus::Running
                    | crate::ui_backend::SubagentStatus::WaitingInput => ("●", self.theme.green),
                    crate::ui_backend::SubagentStatus::Queued => ("○", self.theme.text_muted),
                    crate::ui_backend::SubagentStatus::Completed => ("✓", self.theme.green),
                    crate::ui_backend::SubagentStatus::Failed => ("✗", self.theme.red),
                    crate::ui_backend::SubagentStatus::Killed => ("⊗", self.theme.text_muted),
                };
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };
                // Title + model·effort (* = override) + unread badge.
                let model = if agent.overridden {
                    format!("{}·{} *", agent.model, agent.effort)
                } else {
                    format!("{}·{}", agent.model, agent.effort)
                };
                let mut title = format!("{} {}", agent.title, model);
                if agent.unread > 0 {
                    title = format!("{} ●{}", title, agent.unread);
                }
                let truncated = Self::truncate_text(&title, content_width, 4);
                let line_idx = subagents_lines.len();
                panel_item_lines[3].push(line_idx);
                Self::push_line(
                    &mut subagents_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::styled(
                            format!("  {} ", glyph),
                            Style::default()
                                .fg(glyph_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(truncated, item_style),
                    ]),
                );
                // Muted preview line below the row.
                Self::push_line(
                    &mut subagents_lines,
                    &mut all_lines,
                    Line::from(vec![Span::styled(
                        format!("    {}", agent.preview),
                        Style::default().fg(self.theme.text_muted),
                    )]),
                );
                sub_idx += 1;
            }

            for queued in self.subagent_queued.iter() {
                let item_selected =
                    self.focused && self.selected_panel == 3 && self.selected_item == Some(sub_idx);
                let item_style = if item_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_muted)
                };
                let truncated = Self::truncate_text(&queued.title, content_width, 4);
                let line_idx = subagents_lines.len();
                panel_item_lines[3].push(line_idx);
                Self::push_line(
                    &mut subagents_lines,
                    &mut all_lines,
                    Line::from(vec![
                        Span::styled("  ○ ", Style::default().fg(self.theme.text_muted)),
                        Span::styled(truncated, item_style),
                        Span::styled(
                            format!(" #{}", queued.position),
                            Style::default().fg(self.theme.text_muted),
                        ),
                    ]),
                );
                sub_idx += 1;
            }

            if self.subagent_other_sessions > 0 {
                Self::push_line(
                    &mut subagents_lines,
                    &mut all_lines,
                    Line::from(vec![Span::styled(
                        format!(
                            "    (+{} other ⏳ — switch session to view)",
                            self.subagent_other_sessions
                        ),
                        Style::default().fg(self.theme.text_muted),
                    )]),
                );
            }
        }
        Self::push_line(&mut subagents_lines, &mut all_lines, Line::from(""));

        // ======== TODO SECTION ========
        let todo_expanded = self.expanded_panels[4];
        let todo_selected = self.selected_panel == 4 && self.selected_item.is_none();
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
                    self.focused && self.selected_panel == 4 && self.selected_item == Some(i);

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
                panel_item_lines[4].push(line_idx);
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
        let git_expanded = self.expanded_panels[5];
        let git_selected = self.selected_panel == 5 && self.selected_item.is_none();
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
                    self.focused && self.selected_panel == 5 && self.selected_item == Some(i);
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
                panel_item_lines[5].push(line_idx);
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
        let plugins_expanded = self.expanded_panels[6];
        let plugins_selected = self.selected_panel == 6 && self.selected_item.is_none();
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
                        self.focused && self.selected_panel == 6 && self.selected_item == Some(i);
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
                    panel_item_lines[6].push(line_idx);
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
        let footer_selected = self.focused && self.selected_panel == 7;
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
        let subagents_height = if subagents_expanded {
            subagents_lines.len() as u16
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
            Constraint::Length(subagents_height),
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
        let subagents_area = panel_chunks[4];
        let todo_area = panel_chunks[5];
        let git_area = panel_chunks[6];
        let plugins_area = panel_chunks[7];
        let footer_area = panel_chunks[8];

        let mut panel_selected_lines: [Option<usize>; 7] =
            [None, None, None, None, None, None, None];
        if self.focused {
            if let Some(selected_item) = self.selected_item {
                let panel_idx = self.selected_panel;
                if panel_idx < 7 {
                    let indices = &panel_item_lines[panel_idx];
                    panel_selected_lines[panel_idx] = indices
                        .get(selected_item)
                        .copied()
                        .or_else(|| indices.last().copied());
                }
            }
        }

        let panel_has_focus = self.focused
            && self.selected_panel < 7
            && (self.selected_item.is_some() || self.expanded_panels[self.selected_panel]);

        // Content lengths before the line vecs move into render_panel calls.
        let panel_line_counts = [
            session_lines.len(),
            context_lines.len(),
            tasks_lines.len(),
            subagents_lines.len(),
            todo_lines.len(),
            git_lines.len(),
            plugin_lines.len(),
        ];

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
            subagents_area,
            subagents_lines,
            self.panel_scrolls[3],
            panel_selected_lines[3],
            panel_has_focus && self.selected_panel == 3,
            buf,
        );
        self.render_panel(
            todo_area,
            todo_lines,
            self.panel_scrolls[4],
            panel_selected_lines[4],
            panel_has_focus && self.selected_panel == 4,
            buf,
        );
        self.render_panel(
            git_area,
            git_lines,
            self.panel_scrolls[5],
            panel_selected_lines[5],
            panel_has_focus && self.selected_panel == 5,
            buf,
        );
        self.render_panel(
            plugins_area,
            plugin_lines,
            self.panel_scrolls[6],
            panel_selected_lines[6],
            panel_has_focus && self.selected_panel == 6,
            buf,
        );
        Paragraph::new(footer_lines).render(footer_area, buf);

        // Record clickable geometry for hit-testing (P2.4). Item rows apply
        // the same panel scroll offset the renderer used, so clicks land on
        // the exact visible row. `panel_item_lines[p]` is built in display
        // order, so rows stop at the first off-screen line.
        *map = SidebarClickMap::default();
        map.header = Some(header_area);
        map.footer = Some(footer_area);
        let panel_areas = [
            session_area,
            context_area,
            tasks_area,
            subagents_area,
            todo_area,
            git_area,
            plugins_area,
        ];
        for (p, panel_area) in panel_areas.iter().enumerate() {
            if panel_area.height == 0 || panel_area.width == 0 {
                continue;
            }
            map.panels[p] = Some(*panel_area);
            let scroll_pos = Self::panel_scroll_pos(
                panel_line_counts[p],
                panel_area.height as usize,
                self.panel_scrolls[p],
                panel_selected_lines[p],
            );
            for &line_idx in &panel_item_lines[p] {
                if line_idx < scroll_pos {
                    continue;
                }
                let y = panel_area.y.saturating_add((line_idx - scroll_pos) as u16);
                if y >= panel_area.y.saturating_add(panel_area.height) {
                    break;
                }
                map.items[p].push(Rect {
                    x: panel_area.x,
                    y,
                    width: panel_area.width,
                    height: 1,
                });
            }
        }

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

impl Widget for Sidebar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut discard = SidebarClickMap::default();
        self.render_with_map(area, buf, &mut discard);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn test_sidebar<'a>(theme: &'a Theme) -> Sidebar<'a> {
        Sidebar::new(theme)
            .expanded(SidebarPanel::Tasks, true)
            .tasks(vec![
                Task {
                    name: "active one".to_string(),
                    status: TaskStatus::Active,
                },
                Task {
                    name: "done one".to_string(),
                    status: TaskStatus::Completed,
                },
                Task {
                    name: "queued one".to_string(),
                    status: TaskStatus::Queued,
                },
            ])
    }

    fn render_map(terminal: &mut Terminal<TestBackend>, sidebar: Sidebar) -> SidebarClickMap {
        let mut map = SidebarClickMap::default();
        terminal
            .draw(|frame| {
                sidebar.render_with_map(frame.area(), frame.buffer_mut(), &mut map);
            })
            .expect("draw");
        map
    }

    #[test]
    fn click_map_records_variable_height_task_rows() {
        // Regression test for the "click one line above" bug: active tasks
        // render a 2-line block (name + status label) while completed and
        // queued tasks render 1 line, so uniform row math drifts.
        let backend = TestBackend::new(40, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let map = render_map(&mut terminal, test_sidebar(&theme));

        let tasks_rect = map.panels[2].expect("tasks panel rect");
        assert_eq!(map.items[2].len(), 3);
        // Row offsets within the panel: header(0), active(1), label(2),
        // completed(3), QUEUED(4), queued(5).
        let rows: Vec<u16> = map.items[2].iter().map(|r| r.y - tasks_rect.y).collect();
        assert_eq!(rows, vec![1, 3, 5]);
        // Every row is exactly one line tall and spans the panel width.
        for row in &map.items[2] {
            assert_eq!(row.height, 1);
            assert_eq!(row.x, tasks_rect.x);
            assert_eq!(row.width, tasks_rect.width);
        }
    }

    #[test]
    fn click_map_panel_rects_are_contiguous() {
        // Tall area so no panel is clipped to zero height.
        let backend = TestBackend::new(40, 200);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let map = render_map(&mut terminal, test_sidebar(&theme));

        // Panels stack without gaps or overlaps.
        let header = map.header.expect("header");
        let mut cursor = header.y + header.height;
        for panel in map.panels.iter().flatten() {
            assert_eq!(panel.y, cursor);
            cursor += panel.height;
        }
        assert_eq!(map.footer.expect("footer").y, cursor);
    }

    #[test]
    fn click_map_collapsed_panel_has_no_items() {
        let backend = TestBackend::new(40, 40);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let sidebar = test_sidebar(&theme).expanded(SidebarPanel::Tasks, false);
        let map = render_map(&mut terminal, sidebar);

        let tasks_rect = map.panels[2].expect("tasks panel rect");
        assert_eq!(tasks_rect.height, 1); // header only
        assert!(map.items[2].is_empty());
    }

    fn test_agent(
        title: &str,
        status: crate::ui_backend::SubagentStatus,
    ) -> crate::ui_backend::SubagentInfo {
        crate::ui_backend::SubagentInfo {
            id: format!("s:sub:{title}"),
            parent_session: "s".to_string(),
            title: title.to_string(),
            status,
            provider: "openrouter".to_string(),
            model: "sonnet".to_string(),
            effort: "med".to_string(),
            overridden: false,
            preview: "rg…".to_string(),
            log_tail: vec!["✓ rg → 14 hits".to_string()],
            unread: 0,
            elapsed_s: 12,
            tools_used: 4,
            tools_cap: 5,
        }
    }

    #[test]
    fn subagents_section_renders_between_tasks_and_todo() {
        // Tall area so panels are not clipped.
        let backend = TestBackend::new(40, 200);
        let mut terminal = Terminal::new(backend).expect("terminal");
        let theme = Theme::default();
        let sidebar = Sidebar::new(&theme).subagents(
            vec![
                test_agent("explore-auth", crate::ui_backend::SubagentStatus::Running),
                test_agent("check-policy", crate::ui_backend::SubagentStatus::Completed),
            ],
            vec![crate::ui_backend::QueuedSubagent {
                id: "s:sub:q".to_string(),
                title: "write-migr".to_string(),
                position: 1,
            }],
            5,
            true,
            0,
        );
        let map = render_map(&mut terminal, sidebar);

        // Panel index 3 sits strictly between Tasks (2) and Todo (4).
        let tasks = map.panels[2].expect("tasks rect");
        let subs = map.panels[3].expect("subagents rect");
        let todo = map.panels[4].expect("todo rect");
        assert!(subs.y >= tasks.y + tasks.height);
        assert!(todo.y >= subs.y + subs.height);
        // Two active rows + one queued row registered 1:1 (previews excluded).
        assert_eq!(map.items[3].len(), 3);
    }

    #[test]
    fn subagents_panel_cycles_at_index_three() {
        let theme = Theme::default();
        let mut sidebar = Sidebar::new(&theme);
        // Session(0) → Context(1) → Tasks(2) → Subagents(3).
        for _ in 0..3 {
            sidebar.next_panel();
        }
        assert_eq!(sidebar.selected_panel, 3);
        // Full cycle covers all 8 panels and wraps.
        for _ in 0..5 {
            sidebar.next_panel();
        }
        assert_eq!(sidebar.selected_panel, 0);
        sidebar.prev_panel();
        assert_eq!(sidebar.selected_panel, 7);
    }

    #[test]
    fn subagents_next_item_counts_rows_plus_queue() {
        let theme = Theme::default();
        let mut sidebar = Sidebar::new(&theme).subagents(
            vec![test_agent("a", crate::ui_backend::SubagentStatus::Running)],
            vec![crate::ui_backend::QueuedSubagent {
                id: "s:sub:q".to_string(),
                title: "b".to_string(),
                position: 1,
            }],
            5,
            true,
            0,
        );
        sidebar.selected_panel = 3;
        sidebar.next_item();
        assert_eq!(sidebar.selected_item, Some(0));
        sidebar.next_item();
        assert_eq!(sidebar.selected_item, Some(1));
        // Only two navigable rows — clamped.
        sidebar.next_item();
        assert_eq!(sidebar.selected_item, Some(1));
    }
}
