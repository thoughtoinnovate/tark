//! Sidebar Widget - Context panels for session info, files, tasks, git
//!
//! Reference: web/ui/mocks/src/app/components/Sidebar.tsx
//! Feature: 13_sidebar.feature

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

/// Sidebar panel type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SidebarPanel {
    Session,
    Context,
    Tasks,
    GitChanges,
}

/// Sidebar widget
#[derive(Debug)]
pub struct Sidebar {
    /// Whether sidebar is visible
    pub visible: bool,
    /// Active panel
    pub active_panel: SidebarPanel,
    /// Scroll offset
    pub scroll_offset: usize,
    /// Session info
    pub session_info: SessionInfo,
    /// Context files
    pub context_files: Vec<String>,
    /// Tasks
    pub tasks: Vec<String>,
    /// Git changes
    pub git_changes: Vec<String>,
}

/// Session information
#[derive(Debug, Default)]
pub struct SessionInfo {
    pub session_id: String,
    pub total_cost: f64,
    pub total_tokens: usize,
    pub provider: String,
    pub model: String,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            visible: false,
            active_panel: SidebarPanel::Session,
            scroll_offset: 0,
            session_info: SessionInfo::default(),
            context_files: Vec::new(),
            tasks: Vec::new(),
            git_changes: Vec::new(),
        }
    }
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset += 1;
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
    }

    pub fn next_panel(&mut self) {
        self.active_panel = match self.active_panel {
            SidebarPanel::Session => SidebarPanel::Context,
            SidebarPanel::Context => SidebarPanel::Tasks,
            SidebarPanel::Tasks => SidebarPanel::GitChanges,
            SidebarPanel::GitChanges => SidebarPanel::Session,
        };
    }

    pub fn prev_panel(&mut self) {
        self.active_panel = match self.active_panel {
            SidebarPanel::Session => SidebarPanel::GitChanges,
            SidebarPanel::Context => SidebarPanel::Session,
            SidebarPanel::Tasks => SidebarPanel::Context,
            SidebarPanel::GitChanges => SidebarPanel::Tasks,
        };
    }
}

impl Widget for &Sidebar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.visible {
            return;
        }

        // Split into panels
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(6), // Session
                Constraint::Min(5),    // Context/Tasks/Git (active)
                Constraint::Length(1), // Help line
            ])
            .split(area);

        // Session panel
        let session_text = [
            format!("Session: {}", self.session_info.session_id),
            format!("Provider: {}", self.session_info.provider),
            format!("Model: {}", self.session_info.model),
            format!("Cost: ${:.4}", self.session_info.total_cost),
            format!("Tokens: {}", self.session_info.total_tokens),
        ];
        let session_para = Paragraph::new(session_text.join("\n"))
            .block(Block::default().borders(Borders::ALL).title("📊 Session"));
        session_para.render(chunks[0], buf);

        // Active panel content
        match self.active_panel {
            SidebarPanel::Session => {
                // Already shown above
            }
            SidebarPanel::Context => {
                let items: Vec<ListItem> = self
                    .context_files
                    .iter()
                    .map(|f| ListItem::new(format!("📄 {}", f)))
                    .collect();
                let list = List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("📁 Context Files"),
                );
                Widget::render(list, chunks[1], buf);
            }
            SidebarPanel::Tasks => {
                let items: Vec<ListItem> = self
                    .tasks
                    .iter()
                    .map(|t| ListItem::new(format!("☐ {}", t)))
                    .collect();
                let list =
                    List::new(items).block(Block::default().borders(Borders::ALL).title("✓ Tasks"));
                Widget::render(list, chunks[1], buf);
            }
            SidebarPanel::GitChanges => {
                let items: Vec<ListItem> = self
                    .git_changes
                    .iter()
                    .map(|g| ListItem::new(format!("M {}", g)))
                    .collect();
                let list = List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("🔀 Git Changes"),
                );
                Widget::render(list, chunks[1], buf);
            }
        }

        // Help line
        let help = Paragraph::new("Tab: Next Panel | Ctrl+B: Toggle");
        help.render(chunks[2], buf);
    }
}
