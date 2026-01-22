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
    Help,
    Model,
    Provider,
    Theme,
    Clear,
    Compact,
    Think,
    Tools,
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
            Self::Help,
            Self::Model,
            Self::Provider,
            Self::Theme,
            Self::Clear,
            Self::Compact,
            Self::Think,
            Self::Tools,
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
            Self::Help => "help",
            Self::Model => "model",
            Self::Provider => "provider",
            Self::Theme => "theme",
            Self::Clear => "clear",
            Self::Compact => "compact",
            Self::Think => "think",
            Self::Tools => "tools",
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
            Self::Help => "Show help and keyboard shortcuts",
            Self::Model => "Open model picker",
            Self::Provider => "Open provider picker",
            Self::Theme => "Open theme picker",
            Self::Clear => "Clear conversation history",
            Self::Compact => "Compact context to free up space",
            Self::Think => "Toggle thinking mode",
            Self::Tools => "Show available tools",
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
            Self::Help => "❓",
            Self::Model => "🤖",
            Self::Provider => "🔌",
            Self::Theme => "🎨",
            Self::Clear => "🗑️",
            Self::Compact => "📦",
            Self::Think => "🧠",
            Self::Tools => "🔧",
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
    }

    /// Deactivate autocomplete
    pub fn deactivate(&mut self) {
        self.active = false;
        self.filter.clear();
        self.matches.clear();
        self.selected = 0;
    }

    /// Update filter and refresh matches
    pub fn update_filter(&mut self, filter: &str) {
        self.filter = filter.to_string();
        self.matches = SlashCommand::find_matches(filter);
        // Keep selected in bounds
        if !self.matches.is_empty() && self.selected >= self.matches.len() {
            self.selected = self.matches.len() - 1;
        }
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        if !self.matches.is_empty() && self.selected < self.matches.len() - 1 {
            self.selected += 1;
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

        // Render command options
        let mut lines: Vec<Line> = Vec::new();

        for (i, cmd) in self.state.matches.iter().enumerate() {
            let is_selected = i == self.state.selected;

            let icon = cmd.icon();
            let name = format!("/{}", cmd.name());
            let desc = cmd.description();

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

            // Show description for selected item
            if is_selected && inner.height > self.state.matches.len() as u16 + 1 {
                lines.push(Line::from(Span::styled(
                    format!("    {}", desc),
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::DIM),
                )));
            }
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
        assert_eq!(matches.len(), 2); // theme, think
        assert!(matches.contains(&SlashCommand::Theme));
        assert!(matches.contains(&SlashCommand::Think));
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
}
