//! Command Autocomplete Widget
//!
//! Provides intellisense-style autocomplete for slash commands.
//! Shows a dropdown when typing '/' with matching commands.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::tui_new::theme::Theme;

/// Available slash commands
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlashCommand {
    Diff,
    Help,
    Model,
    Provider,
    Theme,
    Clear,
    Compact,
    Think,
    Thinking,
    Tools,
    Policy,
    Sessions,
    New,
    Quit,
    Plan,
    Ask,
    Build,
}

impl SlashCommand {
    /// Get all available commands
    pub fn all() -> Vec<Self> {
        vec![
            Self::Diff,
            Self::Help,
            Self::Model,
            Self::Provider,
            Self::Theme,
            Self::Clear,
            Self::Compact,
            Self::Think,
            Self::Thinking,
            Self::Tools,
            Self::Policy,
            Self::Sessions,
            Self::New,
            Self::Quit,
            Self::Plan,
            Self::Ask,
            Self::Build,
        ]
    }

    /// Get command name (without slash)
    pub fn name(&self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Help => "help",
            Self::Model => "model",
            Self::Provider => "provider",
            Self::Theme => "theme",
            Self::Clear => "clear",
            Self::Compact => "compact",
            Self::Think => "think",
            Self::Thinking => "thinking",
            Self::Tools => "tools",
            Self::Policy => "policy",
            Self::Sessions => "sessions",
            Self::New => "new",
            Self::Quit => "quit",
            Self::Plan => "plan",
            Self::Ask => "ask",
            Self::Build => "build",
        }
    }

    /// Get command description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Diff => "Toggle diff view (auto/inline/split)",
            Self::Help => "Show help and keyboard shortcuts",
            Self::Model => "Open model picker",
            Self::Provider => "Open provider picker",
            Self::Theme => "Open theme picker",
            Self::Clear => "Clear conversation history",
            Self::Compact => "Compact context to free up space",
            Self::Think => "Toggle model thinking level (off/low/medium/high)",
            Self::Thinking => "Toggle think tool for structured reasoning",
            Self::Tools => "Show available tools",
            Self::Policy => "View approval/denial patterns",
            Self::Sessions => "Open sessions list",
            Self::New => "Create a new session",
            Self::Quit => "Quit the application",
            Self::Plan => "Switch to Plan mode",
            Self::Ask => "Switch to Ask mode",
            Self::Build => "Switch to Build mode",
        }
    }

    /// Get command icon
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Diff => "🔀",
            Self::Help => "❓",
            Self::Model => "🤖",
            Self::Provider => "🔌",
            Self::Theme => "🎨",
            Self::Clear => "🗑️",
            Self::Compact => "📦",
            Self::Think => "🧠",
            Self::Thinking => "💭",
            Self::Tools => "🔧",
            Self::Policy => "🔒",
            Self::Sessions => "🗂️",
            Self::New => "🆕",
            Self::Quit => "🚪",
            Self::Plan => "📋",
            Self::Ask => "💬",
            Self::Build => "🔨",
        }
    }

    /// Find command by name (partial match)
    pub fn find_matches(input: &str) -> Vec<Self> {
        let input_lower = input.to_lowercase();
        Self::all()
            .into_iter()
            .filter(|cmd| cmd.name().starts_with(&input_lower))
            .collect()
    }

    /// Get exact match if input matches a command exactly
    pub fn exact_match(input: &str) -> Option<Self> {
        let input_lower = input.to_lowercase();
        Self::all()
            .into_iter()
            .find(|cmd| cmd.name() == input_lower)
    }
}

/// State for slash command autocomplete
#[derive(Debug, Clone, Default)]
pub struct AutocompleteState {
    /// Whether autocomplete is active
    pub active: bool,
    /// Current input after the slash
    pub filter: String,
    /// Selected index in the dropdown
    pub selected: usize,
    /// Filtered commands
    pub matches: Vec<SlashCommand>,
    /// Scroll offset for viewport
    pub scroll_offset: usize,
}

impl AutocompleteState {
    /// Create new autocomplete state
    pub fn new() -> Self {
        Self::default()
    }

    /// Activate autocomplete with initial filter
    pub fn activate(&mut self, filter: &str) {
        self.active = true;
        self.filter = filter.to_string();
        self.matches = SlashCommand::find_matches(filter);
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Deactivate autocomplete
    pub fn deactivate(&mut self) {
        self.active = false;
        self.filter.clear();
        self.matches.clear();
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Update filter and refresh matches
    pub fn update_filter(&mut self, filter: &str) {
        self.filter = filter.to_string();
        self.matches = SlashCommand::find_matches(filter);
        self.scroll_offset = 0; // Reset scroll when filter changes
                                // Keep selected in bounds
        if !self.matches.is_empty() && self.selected >= self.matches.len() {
            self.selected = self.matches.len() - 1;
        }
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
            // Auto-scroll up if selection goes above viewport
            if self.selected < self.scroll_offset {
                self.scroll_offset = self.selected;
            }
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        // Visible count: 8 items fit in the dropdown (10 height - 2 for borders)
        let visible_count = 8;
        if !self.matches.is_empty() && self.selected < self.matches.len() - 1 {
            self.selected += 1;
            // Auto-scroll down if selection goes below viewport
            if self.selected >= self.scroll_offset + visible_count {
                self.scroll_offset = self.selected - visible_count + 1;
            }
        }
    }

    /// Get currently selected command
    pub fn selected_command(&self) -> Option<SlashCommand> {
        self.matches.get(self.selected).copied()
    }

    /// Check if there's exactly one match (for auto-submit)
    pub fn has_unique_match(&self) -> bool {
        self.matches.len() == 1
    }

    /// Get the completion text for TAB
    pub fn get_completion(&self) -> Option<String> {
        self.selected_command()
            .map(|cmd| format!("/{}", cmd.name()))
    }
}

/// Command autocomplete dropdown widget
pub struct CommandAutocomplete<'a> {
    theme: &'a Theme,
    state: &'a AutocompleteState,
}

impl<'a> CommandAutocomplete<'a> {
    pub fn new(theme: &'a Theme, state: &'a AutocompleteState) -> Self {
        Self { theme, state }
    }
}

impl Widget for CommandAutocomplete<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.active || self.state.matches.is_empty() {
            return;
        }

        // Calculate dropdown position (above the input area)
        let dropdown_height = (self.state.matches.len() as u16 + 2).min(10); // +2 for borders
        let dropdown_width = 35u16.min(area.width);

        // Position dropdown above the cursor area
        let dropdown_area = Rect {
            x: area.x + 1,
            y: area.y.saturating_sub(dropdown_height),
            width: dropdown_width,
            height: dropdown_height,
        };

        // Clear and render dropdown
        Clear.render(dropdown_area, buf);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.cyan))
            .style(Style::default().bg(self.theme.bg_dark))
            .title(Span::styled(
                " Commands ",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(dropdown_area);
        block.render(dropdown_area, buf);

        // Render command options with viewport scrolling
        let mut lines: Vec<Line> = Vec::new();

        // Calculate visible range
        let visible_height = inner.height as usize;
        let visible_end = (self.state.scroll_offset + visible_height).min(self.state.matches.len());

        // Scroll-up indicator
        if self.state.scroll_offset > 0 {
            lines.push(Line::from(Span::styled(
                format!("  ▲ {} more above", self.state.scroll_offset),
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            )));
        }

        // Render visible commands
        for i in self.state.scroll_offset..visible_end {
            let cmd = &self.state.matches[i];
            let is_selected = i == self.state.selected;

            let icon = cmd.icon();
            let name = format!("/{}", cmd.name());

            let style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .bg(Color::Rgb(45, 60, 83))
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let prefix = if is_selected { "▸ " } else { "  " };

            lines.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(self.theme.text_muted),
                ),
                Span::styled(name, style),
            ]));
        }

        // Scroll-down indicator
        let remaining = self.state.matches.len().saturating_sub(visible_end);
        if remaining > 0 {
            lines.push(Line::from(Span::styled(
                format!("  ▼ {} more below", remaining),
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            )));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slash_command_find_matches() {
        let matches = SlashCommand::find_matches("th");
        assert_eq!(matches.len(), 3); // theme, think, thinking
        assert!(matches.contains(&SlashCommand::Theme));
        assert!(matches.contains(&SlashCommand::Think));
        assert!(matches.contains(&SlashCommand::Thinking));
    }

    #[test]
    fn test_slash_command_find_matches_c() {
        let matches = SlashCommand::find_matches("c");
        assert_eq!(matches.len(), 2); // clear, compact
        assert!(matches.contains(&SlashCommand::Clear));
        assert!(matches.contains(&SlashCommand::Compact));
    }

    #[test]
    fn test_slash_command_exact_match() {
        assert_eq!(SlashCommand::exact_match("help"), Some(SlashCommand::Help));
        assert_eq!(SlashCommand::exact_match("hel"), None);
        assert_eq!(
            SlashCommand::exact_match("compact"),
            Some(SlashCommand::Compact)
        );
    }

    #[test]
    fn test_autocomplete_state() {
        let mut state = AutocompleteState::new();
        state.activate("m");
        assert!(state.active);
        assert_eq!(state.matches.len(), 1); // model
        assert_eq!(state.selected_command(), Some(SlashCommand::Model));
    }

    #[test]
    fn test_compact_command_properties() {
        assert_eq!(SlashCommand::Compact.name(), "compact");
        assert_eq!(
            SlashCommand::Compact.description(),
            "Compact context to free up space"
        );
        assert_eq!(SlashCommand::Compact.icon(), "📦");
    }

    #[test]
    fn test_policy_command_properties() {
        assert_eq!(SlashCommand::Policy.name(), "policy");
        assert_eq!(
            SlashCommand::Policy.description(),
            "View approval/denial patterns"
        );
        assert_eq!(SlashCommand::Policy.icon(), "🔒");
    }

    #[test]
    fn test_autocomplete_scroll_offset_on_activate() {
        let mut state = AutocompleteState::new();
        state.scroll_offset = 5; // Set a non-zero value
        state.activate("h");
        assert_eq!(state.scroll_offset, 0); // Should reset to 0
    }

    #[test]
    fn test_autocomplete_scroll_offset_on_update_filter() {
        let mut state = AutocompleteState::new();
        state.activate("");
        state.scroll_offset = 5;
        state.update_filter("p");
        assert_eq!(state.scroll_offset, 0); // Should reset to 0
    }

    #[test]
    fn test_autocomplete_move_down_scrolls_viewport() {
        let mut state = AutocompleteState::new();
        state.activate(""); // Get all commands

        // Move down 8 times (visible count)
        for _ in 0..8 {
            state.move_down();
        }

        // Should be at index 8, scroll_offset should be 1 now (with 17 total commands)
        assert_eq!(state.selected, 8);
        assert_eq!(state.scroll_offset, 1);

        // Move down one more time - should increase scroll
        state.move_down();
        assert_eq!(state.selected, 9);
        assert_eq!(state.scroll_offset, 2); // selected (9) - visible_count (8) + 1 = 2
    }

    #[test]
    fn test_autocomplete_move_up_scrolls_viewport() {
        let mut state = AutocompleteState::new();
        state.activate(""); // Get all commands

        // Move to the end
        while state.selected < state.matches.len() - 1 {
            state.move_down();
        }

        let max_selected = state.selected;
        let final_scroll = state.scroll_offset;

        // Move up should maintain scroll until we go above viewport
        state.move_up();
        assert_eq!(state.selected, max_selected - 1);

        // Keep moving up until scroll changes
        while state.scroll_offset == final_scroll && state.selected > 0 {
            state.move_up();
        }

        // Scroll should have decreased
        assert!(state.scroll_offset < final_scroll);
    }

    #[test]
    fn test_slash_command_find_matches_includes_policy() {
        let matches = SlashCommand::find_matches("p");
        assert!(matches.contains(&SlashCommand::Policy));
        assert!(matches.contains(&SlashCommand::Provider));
        assert!(matches.contains(&SlashCommand::Plan));
    }
}
