//! Context-aware help popup widget
//!
//! Shows keybinding help based on current focus and input mode.
//! Press ? to toggle, Esc or ? again to close.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

use crate::tui::keybindings::{FocusedComponent, InputMode};

/// Help popup state
#[derive(Debug, Default, Clone)]
pub struct HelpPopupState {
    /// Whether the popup is visible
    pub visible: bool,
}

impl HelpPopupState {
    /// Toggle visibility
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Show the popup
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Hide the popup
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }
}

/// Help popup widget
pub struct HelpPopup<'a> {
    focused_component: FocusedComponent,
    input_mode: InputMode,
    /// Optional custom title
    title: Option<&'a str>,
}

impl<'a> HelpPopup<'a> {
    /// Create a new help popup for the given context
    pub fn new(focused_component: FocusedComponent, input_mode: InputMode) -> Self {
        Self {
            focused_component,
            input_mode,
            title: None,
        }
    }

    /// Set a custom title
    pub fn title(mut self, title: &'a str) -> Self {
        self.title = Some(title);
        self
    }

    /// Get the title based on context
    fn get_title(&self) -> String {
        if let Some(t) = self.title {
            return t.to_string();
        }

        let component = match self.focused_component {
            FocusedComponent::Messages => "Messages",
            FocusedComponent::Input => "Input",
            FocusedComponent::Panel => "Panel",
        };

        let mode = match self.input_mode {
            InputMode::Normal => "Normal",
            InputMode::Insert => "Insert",
            InputMode::Visual => "Visual",
            InputMode::Command => "Command",
        };

        format!("Help: {} ({})", component, mode)
    }

    /// Get help content for Messages in Normal mode
    fn messages_normal_help() -> Vec<Line<'static>> {
        vec![
            Line::from(vec![Span::styled(
                "NAVIGATION",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  j / k           Move down / up"),
            Line::from("  gg / G          Go to top / bottom"),
            Line::from("  Ctrl+d / u      Half page down / up"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "BLOCKS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Enter           Toggle block at cursor"),
            Line::from("  n / N           Focus next / prev block"),
            Line::from("  zo / zc         Expand / collapse at cursor"),
            Line::from("  zO / zC         Expand / collapse all"),
            Line::from("  [ / ]           Scroll within focused block"),
            Line::from("  Esc             Clear block focus"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "MODE",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  i               Enter insert mode"),
            Line::from("  v               Enter visual mode"),
            Line::from("  Tab             Focus next component"),
            Line::from("  Shift+Tab       Cycle agent mode"),
        ]
    }

    /// Get help content for Input in Insert mode
    fn input_insert_help() -> Vec<Line<'static>> {
        vec![
            Line::from(vec![Span::styled(
                "EDITING",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Type            Normal text input"),
            Line::from("  Backspace       Delete character"),
            Line::from("  Ctrl+V          Paste from clipboard"),
            Line::from("  Shift+Enter     Insert newline"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "COMMANDS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  /command        Slash commands (autocomplete)"),
            Line::from("  @filename       Attach file to context"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "SUBMIT",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Enter           Send message"),
            Line::from("  Esc             Exit insert mode"),
            Line::from("  Ctrl+C          Interrupt streaming"),
        ]
    }

    /// Get help content for Input in Normal mode
    fn input_normal_help() -> Vec<Line<'static>> {
        vec![
            Line::from(vec![Span::styled(
                "INPUT",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  i               Enter insert mode"),
            Line::from("  Tab             Focus next component"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "AGENT MODE",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Shift+Tab       Cycle: Build -> Plan -> Ask"),
            Line::from("  /plan           Switch to Plan mode"),
            Line::from("  /build          Switch to Build mode"),
            Line::from("  /ask            Switch to Ask mode"),
        ]
    }

    /// Get help content for Panel
    fn panel_help() -> Vec<Line<'static>> {
        vec![
            Line::from(vec![Span::styled(
                "NAVIGATION",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  j / k           Move down / up"),
            Line::from("  Enter           Drill into section"),
            Line::from("  -               Go back to parent"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "SECTIONS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  zo / zc         Expand / collapse section"),
            Line::from("  c               Toggle cost breakdown"),
            Line::from(""),
            Line::from(vec![Span::styled(
                "FOCUS",
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )]),
            Line::from("  Tab             Focus next component"),
            Line::from("  Shift+Tab       Cycle agent mode"),
        ]
    }

    /// Get help content based on current context
    fn get_content(&self) -> Vec<Line<'static>> {
        match (self.focused_component, self.input_mode) {
            (FocusedComponent::Messages, InputMode::Normal) => Self::messages_normal_help(),
            (FocusedComponent::Messages, InputMode::Visual) => {
                let mut lines = Self::messages_normal_help();
                lines.insert(
                    0,
                    Line::from(vec![Span::styled(
                        "VISUAL MODE - Selection active",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    )]),
                );
                lines.insert(1, Line::from("  y               Copy selection"));
                lines.insert(2, Line::from("  Esc             Exit visual mode"));
                lines.insert(3, Line::from(""));
                lines
            }
            (FocusedComponent::Input, InputMode::Insert) => Self::input_insert_help(),
            (FocusedComponent::Input, _) => Self::input_normal_help(),
            (FocusedComponent::Panel, _) => Self::panel_help(),
            (FocusedComponent::Messages, _) => Self::messages_normal_help(),
        }
    }
}

impl Widget for HelpPopup<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate popup size (centered, 60% width, up to 70% height)
        let popup_width = (area.width as f32 * 0.6).min(50.0) as u16;
        let content = self.get_content();
        let popup_height = (content.len() as u16 + 6).min((area.height as f32 * 0.7) as u16);

        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area behind the popup
        Clear.render(popup_area, buf);

        // Create the block with title
        let title = self.get_title();
        let block = Block::default()
            .title(title)
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(Style::default().bg(Color::Black));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        // Render content
        let mut all_lines = content;

        // Add footer
        all_lines.push(Line::from(""));
        all_lines.push(Line::from(vec![Span::styled(
            "Press ? or Esc to close",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )]));

        let paragraph = Paragraph::new(all_lines)
            .alignment(Alignment::Left)
            .wrap(Wrap { trim: false });

        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_popup_state() {
        let mut state = HelpPopupState::default();
        assert!(!state.is_visible());

        state.toggle();
        assert!(state.is_visible());

        state.toggle();
        assert!(!state.is_visible());

        state.show();
        assert!(state.is_visible());

        state.hide();
        assert!(!state.is_visible());
    }

    #[test]
    fn test_help_popup_title() {
        let popup = HelpPopup::new(FocusedComponent::Messages, InputMode::Normal);
        assert_eq!(popup.get_title(), "Help: Messages (Normal)");

        let popup = HelpPopup::new(FocusedComponent::Input, InputMode::Insert);
        assert_eq!(popup.get_title(), "Help: Input (Insert)");

        let popup = HelpPopup::new(FocusedComponent::Panel, InputMode::Normal);
        assert_eq!(popup.get_title(), "Help: Panel (Normal)");
    }

    #[test]
    fn test_help_popup_content_not_empty() {
        let popup = HelpPopup::new(FocusedComponent::Messages, InputMode::Normal);
        let content = popup.get_content();
        assert!(!content.is_empty());

        let popup = HelpPopup::new(FocusedComponent::Input, InputMode::Insert);
        let content = popup.get_content();
        assert!(!content.is_empty());

        let popup = HelpPopup::new(FocusedComponent::Panel, InputMode::Normal);
        let content = popup.get_content();
        assert!(!content.is_empty());
    }
}
