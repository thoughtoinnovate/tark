//! Plan picker widget for managing execution plans
//!
//! Provides a popup UI for viewing, switching, archiving, and exporting plans.
//! Accessible from all modes via the `/plans` command.
//!
//! # Features
//!
//! - **Tab navigation**: Switch between Active and Archived tabs
//! - **Plan switching**: Set a different plan as current (works in all modes)
//! - **Confirmation dialog**: When agent is processing, shows confirmation before switch
//! - **Archive/Export**: Archive plans or export as markdown
//!
//! # Keybindings
//!
//! - `Enter`: Switch (active tab) or View (archived tab)
//! - `Tab`: Toggle between Active/Archived tabs
//! - `a`: Archive the selected plan
//! - `e`: Export the selected plan as markdown
//! - `j/k` or `↓/↑`: Navigate up/down
//! - `Esc`: Close the picker
//!
//! # Confirmation Dialog
//!
//! When the agent is actively processing and the user tries to switch plans:
//! 1. A confirmation dialog appears
//! 2. Press `y` to confirm: stops agent processing and switches
//! 3. Press `n` or `Esc` to cancel: returns to picker

#![allow(dead_code)]

use crate::storage::{PlanMeta, PlanStatus};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Tabs, Widget},
};

/// Tab in the plan picker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlanPickerTab {
    /// Active/draft plans
    #[default]
    Active,
    /// Archived plans
    Archived,
}

impl PlanPickerTab {
    /// Get the display name
    pub fn label(&self) -> &'static str {
        match self {
            PlanPickerTab::Active => "Active",
            PlanPickerTab::Archived => "Archived",
        }
    }

    /// Toggle to the other tab
    pub fn toggle(&self) -> Self {
        match self {
            PlanPickerTab::Active => PlanPickerTab::Archived,
            PlanPickerTab::Archived => PlanPickerTab::Active,
        }
    }
}

/// Action that can be performed on a plan
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlanAction {
    /// View plan details
    View(String),
    /// Set as active plan
    Switch(String),
    /// Archive the plan
    Archive(String),
    /// Export as markdown
    Export(String),
    /// Close the picker
    Close,
}

/// State for the plan picker widget
#[derive(Debug, Default)]
pub struct PlanPickerState {
    /// Active plans list
    active_plans: Vec<PlanMeta>,
    /// Archived plans list
    archived_plans: Vec<PlanMeta>,
    /// Current tab
    current_tab: PlanPickerTab,
    /// Selected index in current list
    selected_index: usize,
    /// Whether the picker is visible
    visible: bool,
    /// Current plan ID (for highlighting)
    current_plan_id: Option<String>,
    /// Pending switch confirmation (plan ID and title)
    /// When set, shows a confirmation dialog before switching
    pending_switch: Option<(String, String)>,
}

impl PlanPickerState {
    /// Create a new plan picker state
    pub fn new() -> Self {
        Self::default()
    }

    /// Show the picker with plans
    ///
    /// Switch action is available in ALL modes (Ask, Plan, Build) for active plans.
    pub fn show(
        &mut self,
        active_plans: Vec<PlanMeta>,
        archived_plans: Vec<PlanMeta>,
        current_plan_id: Option<String>,
    ) {
        self.active_plans = active_plans;
        self.archived_plans = archived_plans;
        self.current_plan_id = current_plan_id;
        self.current_tab = PlanPickerTab::Active;
        self.selected_index = 0;
        self.visible = true;
    }

    /// Hide the picker
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the current list based on tab
    fn current_list(&self) -> &[PlanMeta] {
        match self.current_tab {
            PlanPickerTab::Active => &self.active_plans,
            PlanPickerTab::Archived => &self.archived_plans,
        }
    }

    /// Move selection up
    pub fn select_up(&mut self) {
        let len = self.current_list().len();
        if len > 0 && self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Move selection down
    pub fn select_down(&mut self) {
        let len = self.current_list().len();
        if len > 0 && self.selected_index < len - 1 {
            self.selected_index += 1;
        }
    }

    /// Switch tabs
    pub fn toggle_tab(&mut self) {
        self.current_tab = self.current_tab.toggle();
        self.selected_index = 0;
    }

    /// Get selected plan ID
    pub fn selected_plan_id(&self) -> Option<&str> {
        let list = self.current_list();
        list.get(self.selected_index).map(|p| p.id.as_str())
    }

    /// Handle Enter key - return action
    ///
    /// On Active tab: Switch to selected plan (works in all modes: Ask/Plan/Build)
    /// On Archive tab: View the archived plan
    pub fn confirm(&self) -> Option<PlanAction> {
        let list = self.current_list();
        list.get(self.selected_index).map(|p| {
            if self.current_tab == PlanPickerTab::Active {
                // Allow switching in ALL modes (Ask, Plan, Build)
                PlanAction::Switch(p.id.clone())
            } else {
                // Archive tab - view only
                PlanAction::View(p.id.clone())
            }
        })
    }

    /// Handle 'a' key - archive
    pub fn archive(&self) -> Option<PlanAction> {
        if self.current_tab == PlanPickerTab::Active {
            self.selected_plan_id()
                .map(|id| PlanAction::Archive(id.to_string()))
        } else {
            None
        }
    }

    /// Handle 'e' key - export
    pub fn export(&self) -> Option<PlanAction> {
        self.selected_plan_id()
            .map(|id| PlanAction::Export(id.to_string()))
    }

    /// Check if in confirmation mode (agent was processing during switch attempt)
    pub fn is_confirming(&self) -> bool {
        self.pending_switch.is_some()
    }

    /// Request switch confirmation (called when agent is processing)
    ///
    /// Stores the plan ID and title for the confirmation dialog.
    pub fn request_switch_confirmation(&mut self, plan_id: String, plan_title: String) {
        self.pending_switch = Some((plan_id, plan_title));
    }

    /// Confirm the pending switch (user pressed 'y')
    ///
    /// Returns the switch action and clears the pending state.
    pub fn confirm_pending_switch(&mut self) -> Option<PlanAction> {
        self.pending_switch
            .take()
            .map(|(id, _)| PlanAction::Switch(id))
    }

    /// Cancel the pending switch (user pressed 'n' or Esc)
    pub fn cancel_pending_switch(&mut self) {
        self.pending_switch = None;
    }

    /// Get the pending switch info (for rendering)
    pub fn pending_switch_info(&self) -> Option<&(String, String)> {
        self.pending_switch.as_ref()
    }
}

/// Plan picker widget for rendering
pub struct PlanPickerWidget<'a> {
    state: &'a PlanPickerState,
}

impl<'a> PlanPickerWidget<'a> {
    /// Create a new plan picker widget
    pub fn new(state: &'a PlanPickerState) -> Self {
        Self { state }
    }

    /// Get status icon for a plan
    fn status_icon(status: PlanStatus) -> &'static str {
        match status {
            PlanStatus::Draft => "📝",
            PlanStatus::Active => "▶️",
            PlanStatus::Paused => "⏸️",
            PlanStatus::Completed => "✅",
            PlanStatus::Abandoned => "❌",
        }
    }

    /// Render a plan item
    fn render_plan_item(&self, plan: &PlanMeta, is_selected: bool, is_current: bool) -> Line<'a> {
        let icon = Self::status_icon(plan.status);
        let progress = format!("{}/{}", plan.progress.0, plan.progress.1);

        let mut spans = vec![
            Span::raw(if is_selected { "▸ " } else { "  " }),
            Span::raw(icon),
            Span::raw(" "),
        ];

        // Title
        let title_style = if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default().fg(Color::Green)
        } else {
            Style::default().fg(Color::White)
        };
        spans.push(Span::styled(plan.title.clone(), title_style));

        // Progress
        spans.push(Span::styled(
            format!(" ({})", progress),
            Style::default().fg(Color::DarkGray),
        ));

        // Current indicator
        if is_current {
            spans.push(Span::styled(
                " [current]",
                Style::default().fg(Color::Green),
            ));
        }

        Line::from(spans)
    }

    /// Render confirmation dialog when agent is processing
    fn render_confirmation(&self, area: Rect, buf: &mut Buffer, plan_title: &str) {
        // Smaller dialog for confirmation
        let popup_width = 50.min(area.width.saturating_sub(4));
        let popup_height = 8;
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area
        Clear.render(popup_area, buf);

        // Outer block with warning style
        let block = Block::default()
            .title(" ⚠️ Confirm Switch ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Layout: message + options
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Warning message
                Constraint::Length(2), // Plan title
                Constraint::Min(1),    // Spacer
                Constraint::Length(1), // Options
            ])
            .split(inner);

        // Warning message
        let warning = Paragraph::new("Agent is currently processing.")
            .style(Style::default().fg(Color::Yellow))
            .alignment(Alignment::Center);
        warning.render(chunks[0], buf);

        // Plan title
        let title_msg = format!("Switch to \"{}\"?", plan_title);
        let title = Paragraph::new(title_msg)
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center);
        title.render(chunks[1], buf);

        // Options
        let options = Paragraph::new("Press [y] to confirm and stop agent, [n] or Esc to cancel")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        options.render(chunks[3], buf);
    }
}

impl Widget for PlanPickerWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Check if we're in confirmation mode
        if let Some((_, plan_title)) = &self.state.pending_switch {
            self.render_confirmation(area, buf, plan_title);
            return;
        }

        // Calculate popup area (centered, 60% width, 50% height)
        let popup_width = (area.width as f32 * 0.6) as u16;
        let popup_height = (area.height as f32 * 0.5) as u16;
        let popup_x = (area.width - popup_width) / 2;
        let popup_y = (area.height - popup_height) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area
        Clear.render(popup_area, buf);

        // Outer block
        let block = Block::default()
            .title(" Plans ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Layout: tabs + content + help
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Tabs
                Constraint::Min(3),    // Content
                Constraint::Length(2), // Help
            ])
            .split(inner);

        // Tabs
        let tabs = Tabs::new(vec!["Active", "Archived"])
            .select(match self.state.current_tab {
                PlanPickerTab::Active => 0,
                PlanPickerTab::Archived => 1,
            })
            .style(Style::default().fg(Color::White))
            .highlight_style(
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(" | ");
        tabs.render(chunks[0], buf);

        // Content
        let list = self.state.current_list();
        if list.is_empty() {
            let msg = match self.state.current_tab {
                PlanPickerTab::Active => "No active plans",
                PlanPickerTab::Archived => "No archived plans",
            };
            let empty_msg = Paragraph::new(msg)
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            empty_msg.render(chunks[1], buf);
        } else {
            // Render plan items
            let max_visible = chunks[1].height as usize;
            let start = if self.state.selected_index >= max_visible {
                self.state.selected_index - max_visible + 1
            } else {
                0
            };

            for (i, plan) in list.iter().skip(start).take(max_visible).enumerate() {
                let is_selected = i + start == self.state.selected_index;
                let is_current = self
                    .state
                    .current_plan_id
                    .as_ref()
                    .map(|id| id == &plan.id)
                    .unwrap_or(false);

                let line = self.render_plan_item(plan, is_selected, is_current);
                let line_area = Rect::new(chunks[1].x, chunks[1].y + i as u16, chunks[1].width, 1);

                let style = if is_selected {
                    Style::default().bg(Color::DarkGray)
                } else {
                    Style::default()
                };

                Paragraph::new(line).style(style).render(line_area, buf);
            }
        }

        // Help footer - Switch is available in ALL modes for active plans
        let help_text = match self.state.current_tab {
            PlanPickerTab::Active => {
                "Enter: Switch | a: Archive | e: Export | Tab: Switch tab | Esc: Close"
            }
            PlanPickerTab::Archived => "Enter: View | e: Export | Tab: Switch tab | Esc: Close",
        };
        let help = Paragraph::new(help_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        help.render(chunks[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_test_plan(id: &str, title: &str, status: PlanStatus) -> PlanMeta {
        PlanMeta {
            id: id.to_string(),
            title: title.to_string(),
            status,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            task_count: 3,
            progress: (1, 3),
        }
    }

    #[test]
    fn test_plan_picker_navigation() {
        let mut state = PlanPickerState::new();
        let plans = vec![
            make_test_plan("plan1", "Plan 1", PlanStatus::Active),
            make_test_plan("plan2", "Plan 2", PlanStatus::Draft),
            make_test_plan("plan3", "Plan 3", PlanStatus::Completed),
        ];

        state.show(plans.clone(), vec![], None);

        assert!(state.is_visible());
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.selected_plan_id(), Some("plan1"));

        state.select_down();
        assert_eq!(state.selected_index, 1);
        assert_eq!(state.selected_plan_id(), Some("plan2"));

        state.select_up();
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn test_plan_picker_tab_toggle() {
        let mut state = PlanPickerState::new();
        let active = vec![make_test_plan("plan1", "Active Plan", PlanStatus::Active)];
        let archived = vec![make_test_plan(
            "arch1",
            "Archived Plan",
            PlanStatus::Completed,
        )];

        state.show(active, archived, None);

        assert_eq!(state.current_tab, PlanPickerTab::Active);
        assert_eq!(state.selected_plan_id(), Some("plan1"));

        state.toggle_tab();
        assert_eq!(state.current_tab, PlanPickerTab::Archived);
        assert_eq!(state.selected_plan_id(), Some("arch1"));
    }

    #[test]
    fn test_plan_picker_actions() {
        let mut state = PlanPickerState::new();
        let plans = vec![make_test_plan("plan1", "Plan 1", PlanStatus::Active)];

        // Active tab - confirm always switches (works in all modes now)
        state.show(plans.clone(), vec![], None);
        assert_eq!(
            state.confirm(),
            Some(PlanAction::Switch("plan1".to_string()))
        );

        // Archived tab - confirm views (can't switch to completed plans)
        let archived = vec![make_test_plan(
            "arch1",
            "Archived Plan",
            PlanStatus::Completed,
        )];
        state.show(vec![], archived, None);
        state.toggle_tab(); // Switch to archived tab
        assert_eq!(state.confirm(), Some(PlanAction::View("arch1".to_string())));
    }

    #[test]
    fn test_plan_picker_confirmation_dialog() {
        let mut state = PlanPickerState::new();
        let plans = vec![make_test_plan("plan1", "Test Plan", PlanStatus::Active)];
        state.show(plans, vec![], None);

        // Initially not in confirmation mode
        assert!(!state.is_confirming());
        assert!(state.pending_switch_info().is_none());

        // Request confirmation (simulates agent is processing)
        state.request_switch_confirmation("plan1".to_string(), "Test Plan".to_string());

        // Now in confirmation mode
        assert!(state.is_confirming());
        assert!(state.pending_switch_info().is_some());
        let (id, title) = state.pending_switch_info().unwrap();
        assert_eq!(id, "plan1");
        assert_eq!(title, "Test Plan");

        // Cancel confirmation
        state.cancel_pending_switch();
        assert!(!state.is_confirming());
    }

    #[test]
    fn test_plan_picker_confirm_pending_switch() {
        let mut state = PlanPickerState::new();
        let plans = vec![make_test_plan("plan1", "Test Plan", PlanStatus::Active)];
        state.show(plans, vec![], None);

        // Request and confirm
        state.request_switch_confirmation("plan1".to_string(), "Test Plan".to_string());
        let action = state.confirm_pending_switch();

        // Should return switch action and clear pending state
        assert_eq!(action, Some(PlanAction::Switch("plan1".to_string())));
        assert!(!state.is_confirming());
    }

    #[test]
    fn test_plan_picker_archive_action() {
        let mut state = PlanPickerState::new();
        let plans = vec![make_test_plan("plan1", "Plan 1", PlanStatus::Active)];
        state.show(plans, vec![], None);

        // Archive action available on active tab
        assert_eq!(
            state.archive(),
            Some(PlanAction::Archive("plan1".to_string()))
        );

        // Archive action not available on archived tab
        let archived = vec![make_test_plan("arch1", "Archived", PlanStatus::Completed)];
        state.show(vec![], archived, None);
        state.toggle_tab();
        assert_eq!(state.archive(), None);
    }

    #[test]
    fn test_plan_picker_export_action() {
        let mut state = PlanPickerState::new();
        let plans = vec![make_test_plan("plan1", "Plan 1", PlanStatus::Active)];
        state.show(plans, vec![], None);

        // Export available on any tab
        assert_eq!(
            state.export(),
            Some(PlanAction::Export("plan1".to_string()))
        );
    }

    #[test]
    fn test_plan_picker_current_plan_highlighting() {
        let mut state = PlanPickerState::new();
        let plans = vec![
            make_test_plan("plan1", "Plan 1", PlanStatus::Active),
            make_test_plan("plan2", "Plan 2", PlanStatus::Active),
        ];

        // Show with plan2 as current
        state.show(plans, vec![], Some("plan2".to_string()));

        // Current plan ID should be stored for highlighting
        assert_eq!(state.current_plan_id, Some("plan2".to_string()));
    }
}
