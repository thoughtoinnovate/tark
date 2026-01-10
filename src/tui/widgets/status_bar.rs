//! Status bar widget for displaying mode, model, provider, and usage info
//!
//! Shows the current state of the chat application in a compact status line.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tools::TrustLevel;
use crate::tui::AgentMode;

/// Status bar state
#[derive(Debug, Clone)]
pub struct StatusBar {
    /// Current agent mode
    pub mode: AgentMode,
    /// Current trust level
    pub trust_level: TrustLevel,
    /// Current model name
    pub model: String,
    /// Current provider name
    pub provider: String,
    /// Context usage (0.0 - 1.0)
    pub context_usage: f32,
    /// Current token count
    pub tokens_used: u32,
    /// Maximum tokens
    pub tokens_max: u32,
    /// Session cost in dollars
    pub cost: f64,
    /// Whether connected to editor (Neovim)
    pub editor_connected: bool,
    /// Current session name
    pub session_name: Option<String>,
    /// Whether thinking mode is enabled
    pub thinking_mode: bool,
    /// Active plan progress (completed, total)
    pub plan_progress: Option<(usize, usize)>,
    /// Contextual keybinding hints (shown based on current context)
    pub keybind_hints: String,
}

impl Default for StatusBar {
    fn default() -> Self {
        Self {
            mode: AgentMode::Build,
            trust_level: TrustLevel::default(),
            model: "gpt-4o".to_string(),
            provider: "OpenAI".to_string(),
            context_usage: 0.0,
            tokens_used: 0,
            tokens_max: 128_000,
            cost: 0.0,
            editor_connected: false,
            session_name: None,
            thinking_mode: false,
            plan_progress: None,
            keybind_hints: String::new(),
        }
    }
}

impl StatusBar {
    /// Create a new status bar with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the agent mode
    pub fn with_mode(mut self, mode: AgentMode) -> Self {
        self.mode = mode;
        self
    }

    /// Set the trust level
    pub fn with_trust_level(mut self, level: TrustLevel) -> Self {
        self.trust_level = level;
        self
    }

    /// Set the model name
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the provider name
    pub fn with_provider(mut self, provider: impl Into<String>) -> Self {
        self.provider = provider.into();
        self
    }

    /// Set the context usage
    pub fn with_context_usage(mut self, used: u32, max: u32) -> Self {
        self.tokens_used = used;
        self.tokens_max = max;
        self.context_usage = if max > 0 {
            used as f32 / max as f32
        } else {
            0.0
        };
        self
    }

    /// Set the cost
    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost = cost;
        self
    }

    /// Set editor connection status
    pub fn with_editor_connected(mut self, connected: bool) -> Self {
        self.editor_connected = connected;
        self
    }

    /// Set session name
    pub fn with_session_name(mut self, name: Option<String>) -> Self {
        self.session_name = name;
        self
    }

    /// Set thinking mode
    pub fn with_thinking_mode(mut self, enabled: bool) -> Self {
        self.thinking_mode = enabled;
        self
    }

    /// Set plan progress
    pub fn with_plan_progress(mut self, progress: Option<(usize, usize)>) -> Self {
        self.plan_progress = progress;
        self
    }

    /// Update the mode
    pub fn set_mode(&mut self, mode: AgentMode) {
        self.mode = mode;
    }

    /// Update the trust level
    pub fn set_trust_level(&mut self, level: TrustLevel) {
        self.trust_level = level;
    }

    /// Update the model
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    /// Update the provider
    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = provider.into();
    }

    /// Update context usage
    pub fn set_context_usage(&mut self, used: u32, max: u32) {
        self.tokens_used = used;
        self.tokens_max = max;
        self.context_usage = if max > 0 {
            used as f32 / max as f32
        } else {
            0.0
        };
    }

    /// Update cost
    pub fn set_cost(&mut self, cost: f64) {
        self.cost = cost;
    }

    /// Update editor connection status
    pub fn set_editor_connected(&mut self, connected: bool) {
        self.editor_connected = connected;
    }

    /// Update plan progress
    pub fn set_plan_progress(&mut self, progress: Option<(usize, usize)>) {
        self.plan_progress = progress;
    }

    /// Set contextual keybinding hints
    pub fn set_keybind_hints(&mut self, hints: impl Into<String>) {
        self.keybind_hints = hints.into();
    }
}

impl AgentMode {
    /// Get the display icon for this mode
    pub fn icon(&self) -> &'static str {
        match self {
            AgentMode::Build => "◆",
            AgentMode::Plan => "◇",
            AgentMode::Ask => "❓",
        }
    }

    /// Get the display name for this mode
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentMode::Build => "Build",
            AgentMode::Plan => "Plan",
            AgentMode::Ask => "Ask",
        }
    }

    /// Get the color for this mode
    pub fn color(&self) -> Color {
        match self {
            AgentMode::Build => Color::Green,
            AgentMode::Plan => Color::Yellow,
            AgentMode::Ask => Color::Cyan,
        }
    }
}

/// Renderable status bar widget
pub struct StatusBarWidget<'a> {
    status: &'a StatusBar,
    style: Style,
}

impl<'a> StatusBarWidget<'a> {
    /// Create a new status bar widget
    pub fn new(status: &'a StatusBar) -> Self {
        Self {
            status,
            style: Style::default().bg(Color::DarkGray),
        }
    }

    /// Set the style for the widget
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Format token count for display (e.g., "12K/128K")
    fn format_tokens(used: u32, max: u32) -> String {
        let format_num = |n: u32| -> String {
            if n >= 1_000_000 {
                format!("{}M", n / 1_000_000)
            } else if n >= 1_000 {
                format!("{}K", n / 1_000)
            } else {
                n.to_string()
            }
        };
        format!("{}/{}", format_num(used), format_num(max))
    }

    /// Get color for context usage percentage
    fn usage_color(usage: f32) -> Color {
        if usage >= 0.9 {
            Color::Red
        } else if usage >= 0.8 {
            Color::Yellow
        } else if usage >= 0.5 {
            Color::White
        } else {
            Color::Green
        }
    }
}

impl Widget for StatusBarWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width == 0 {
            return;
        }

        // Fill background
        for x in area.x..area.x + area.width {
            for y in area.y..area.y + area.height {
                if let Some(cell) = buf.cell_mut((x, y)) {
                    cell.set_style(self.style);
                }
            }
        }

        let mut spans = Vec::new();

        // Mode indicator with trust level (only in Build mode)
        let mode_style = Style::default()
            .fg(self.status.mode.color())
            .add_modifier(Modifier::BOLD);

        spans.push(Span::styled(
            format!(
                " {} {} ",
                self.status.mode.icon(),
                self.status.mode.display_name()
            ),
            mode_style,
        ));

        // Trust level indicator - only show in Build mode where it has effect
        if self.status.mode == AgentMode::Build {
            let trust_indicator = match self.status.trust_level {
                TrustLevel::Balanced => self.status.trust_level.icon().to_string(),
                TrustLevel::Careful => self.status.trust_level.label().to_string(),
                TrustLevel::Manual => self.status.trust_level.label().to_string(),
            };

            let trust_color = match self.status.trust_level {
                TrustLevel::Balanced => Color::DarkGray,
                TrustLevel::Careful => Color::Blue,
                TrustLevel::Manual => Color::Red,
            };

            spans.push(Span::styled(
                trust_indicator,
                Style::default().fg(trust_color),
            ));
            spans.push(Span::raw(" "));
        }

        // Separator
        spans.push(Span::styled("│", Style::default().fg(Color::Gray)));

        // Model
        spans.push(Span::styled(
            format!(" {} ", self.status.model),
            Style::default().fg(Color::White),
        ));

        // Plan progress (if active)
        if let Some((completed, total)) = self.status.plan_progress {
            spans.push(Span::styled("│", Style::default().fg(Color::Gray)));
            spans.push(Span::styled(
                format!(" Plan: {}/{} ", completed, total),
                Style::default().fg(Color::Magenta),
            ));
        }

        // Separator
        spans.push(Span::styled("│", Style::default().fg(Color::Gray)));

        // Provider
        spans.push(Span::styled(
            format!(" {} ", self.status.provider),
            Style::default().fg(Color::Cyan),
        ));

        // Separator
        spans.push(Span::styled("│", Style::default().fg(Color::Gray)));

        // Context usage
        let usage_pct = (self.status.context_usage * 100.0) as u32;
        let usage_color = Self::usage_color(self.status.context_usage);
        let tokens_str = Self::format_tokens(self.status.tokens_used, self.status.tokens_max);
        spans.push(Span::styled(
            format!(" [{}%] {} ", usage_pct, tokens_str),
            Style::default().fg(usage_color),
        ));

        // Separator
        spans.push(Span::styled("│", Style::default().fg(Color::Gray)));

        // Cost
        spans.push(Span::styled(
            format!(" ${:.4} ", self.status.cost),
            Style::default().fg(Color::Yellow),
        ));

        // Connection status (right-aligned) - only show when connected
        let connection_icon = if self.status.editor_connected {
            "●"
        } else {
            ""
        };
        let connection_color = if self.status.editor_connected {
            Color::Green
        } else {
            Color::DarkGray
        };
        let connection_text = if self.status.editor_connected {
            "nvim"
        } else {
            ""
        };

        // Thinking mode indicator
        if self.status.thinking_mode {
            spans.push(Span::styled("│", Style::default().fg(Color::Gray)));
            spans.push(Span::styled(" 🧠 ", Style::default().fg(Color::Magenta)));
        }

        // Calculate remaining space for right-aligned content
        let left_content_len: usize = spans.iter().map(|s| s.content.len()).sum();

        // Keybind hints (right side, before connection)
        let hints_content = if !self.status.keybind_hints.is_empty() {
            format!(" {} │", self.status.keybind_hints)
        } else {
            String::new()
        };
        let hints_len = hints_content.chars().count();

        let right_content = format!(" {} {} ", connection_icon, connection_text);
        let right_content_len = right_content.len();

        let padding = (area.width as usize)
            .saturating_sub(left_content_len)
            .saturating_sub(hints_len)
            .saturating_sub(right_content_len);

        if padding > 0 {
            spans.push(Span::raw(" ".repeat(padding)));
        }

        // Add keybind hints (dimmed, right-aligned)
        if !hints_content.is_empty() {
            spans.push(Span::styled(
                hints_content,
                Style::default().fg(Color::DarkGray),
            ));
        }

        spans.push(Span::styled(
            right_content,
            Style::default().fg(connection_color),
        ));

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_default() {
        let status = StatusBar::new();
        assert_eq!(status.mode, AgentMode::Build);
        assert_eq!(status.model, "gpt-4o");
        assert_eq!(status.provider, "OpenAI");
        assert_eq!(status.context_usage, 0.0);
        assert!(!status.editor_connected);
    }

    #[test]
    fn test_status_bar_builder() {
        let status = StatusBar::new()
            .with_mode(AgentMode::Plan)
            .with_model("claude-3-sonnet")
            .with_provider("Anthropic")
            .with_context_usage(50_000, 100_000)
            .with_cost(0.05)
            .with_editor_connected(true);

        assert_eq!(status.mode, AgentMode::Plan);
        assert_eq!(status.model, "claude-3-sonnet");
        assert_eq!(status.provider, "Anthropic");
        assert_eq!(status.context_usage, 0.5);
        assert_eq!(status.tokens_used, 50_000);
        assert_eq!(status.tokens_max, 100_000);
        assert_eq!(status.cost, 0.05);
        assert!(status.editor_connected);
    }

    #[test]
    fn test_status_bar_setters() {
        let mut status = StatusBar::new();

        status.set_mode(AgentMode::Ask);
        assert_eq!(status.mode, AgentMode::Ask);

        status.set_model("gpt-4");
        assert_eq!(status.model, "gpt-4");

        status.set_provider("OpenAI");
        assert_eq!(status.provider, "OpenAI");

        status.set_context_usage(10_000, 50_000);
        assert_eq!(status.context_usage, 0.2);

        status.set_cost(0.01);
        assert_eq!(status.cost, 0.01);

        status.set_editor_connected(true);
        assert!(status.editor_connected);
    }

    #[test]
    fn test_agent_mode_display() {
        assert_eq!(AgentMode::Build.display_name(), "Build");
        assert_eq!(AgentMode::Plan.display_name(), "Plan");
        assert_eq!(AgentMode::Ask.display_name(), "Ask");
    }

    #[test]
    fn test_agent_mode_icon() {
        assert_eq!(AgentMode::Build.icon(), "◆");
        assert_eq!(AgentMode::Plan.icon(), "◇");
        assert_eq!(AgentMode::Ask.icon(), "❓");
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(StatusBarWidget::format_tokens(500, 1000), "500/1K");
        assert_eq!(StatusBarWidget::format_tokens(5000, 128000), "5K/128K");
        assert_eq!(StatusBarWidget::format_tokens(1500000, 2000000), "1M/2M");
    }

    #[test]
    fn test_usage_color() {
        assert_eq!(StatusBarWidget::usage_color(0.1), Color::Green);
        assert_eq!(StatusBarWidget::usage_color(0.5), Color::White);
        assert_eq!(StatusBarWidget::usage_color(0.8), Color::Yellow);
        assert_eq!(StatusBarWidget::usage_color(0.95), Color::Red);
    }
}
