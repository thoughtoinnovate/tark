//! Status Bar Widget
//!
//! Displays agent mode, model, thinking toggle, queue, and help button
//! Feature: 02_status_bar.feature

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui_new::app::{AgentMode, BuildMode};
use crate::tui_new::theme::Theme;

/// Status bar widget
pub struct StatusBar<'a> {
    /// Current agent mode
    agent_mode: AgentMode,
    /// Current build mode (only shown in Build agent mode)
    build_mode: BuildMode,
    /// Current model name
    model_name: &'a str,
    /// Current provider name
    provider_name: &'a str,
    /// Whether thinking mode is enabled
    thinking_enabled: bool,
    /// Task queue count
    queue_count: usize,
    /// Whether agent is processing
    is_processing: bool,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    /// Create a new status bar
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            agent_mode: AgentMode::Build,
            build_mode: BuildMode::Balanced,
            model_name: "claude-sonnet-4",
            provider_name: "Anthropic",
            thinking_enabled: true,
            queue_count: 0,
            is_processing: false,
            theme,
        }
    }

    /// Set agent mode
    pub fn agent_mode(mut self, mode: AgentMode) -> Self {
        self.agent_mode = mode;
        self
    }

    /// Set build mode
    pub fn build_mode(mut self, mode: BuildMode) -> Self {
        self.build_mode = mode;
        self
    }

    /// Set model name
    pub fn model(mut self, name: &'a str) -> Self {
        self.model_name = name;
        self
    }

    /// Set provider name
    pub fn provider(mut self, name: &'a str) -> Self {
        self.provider_name = name;
        self
    }

    /// Set thinking mode
    pub fn thinking(mut self, enabled: bool) -> Self {
        self.thinking_enabled = enabled;
        self
    }

    /// Set queue count
    pub fn queue(mut self, count: usize) -> Self {
        self.queue_count = count;
        self
    }

    /// Set processing state
    pub fn processing(mut self, is_processing: bool) -> Self {
        self.is_processing = is_processing;
        self
    }

    /// Get agent mode display string
    fn agent_mode_str(&self) -> &'static str {
        match self.agent_mode {
            AgentMode::Build => "Build",
            AgentMode::Plan => "Plan",
            AgentMode::Ask => "Ask",
        }
    }

    /// Get agent mode icon
    fn agent_mode_icon(&self) -> &'static str {
        match self.agent_mode {
            AgentMode::Build => "🔨",
            AgentMode::Plan => "📋",
            AgentMode::Ask => "💬",
        }
    }

    /// Get build mode display string
    fn build_mode_str(&self) -> &'static str {
        match self.build_mode {
            BuildMode::Careful => "Careful",
            BuildMode::Balanced => "Balanced",
            BuildMode::Manual => "Manual",
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 1 {
            return;
        }

        // Layout: [Mode] [BuildMode?] [Model/Provider] [Working?] [Queue?] [Thinking] [Help]
        let chunks = Layout::horizontal([
            Constraint::Length(12), // Agent mode
            Constraint::Length(12), // Build mode (conditional)
            Constraint::Min(20),    // Model/Provider
            Constraint::Length(12), // Working indicator
            Constraint::Length(8),  // Queue
            Constraint::Length(4),  // Thinking
            Constraint::Length(3),  // Help
        ])
        .split(area);

        // Agent mode
        let mode_text = Line::from(vec![
            Span::styled(
                format!("{} ", self.agent_mode_icon()),
                Style::default().fg(self.theme.cyan),
            ),
            Span::styled(
                self.agent_mode_str(),
                Style::default().fg(self.theme.text_primary),
            ),
            Span::styled(" ▼", Style::default().fg(self.theme.text_muted)),
        ]);
        Paragraph::new(mode_text).render(chunks[0], buf);

        // Build mode (only in Build agent mode)
        if self.agent_mode == AgentMode::Build {
            let build_text = Line::from(vec![
                Span::styled(
                    self.build_mode_str(),
                    Style::default().fg(self.theme.text_secondary),
                ),
                Span::styled(" ▼", Style::default().fg(self.theme.text_muted)),
            ]);
            Paragraph::new(build_text).render(chunks[1], buf);
        }

        // Model/Provider
        let model_text = Line::from(vec![
            Span::styled(
                self.provider_name,
                Style::default().fg(self.theme.text_muted),
            ),
            Span::styled(" / ", Style::default().fg(self.theme.text_muted)),
            Span::styled(
                self.model_name,
                Style::default().fg(self.theme.text_primary),
            ),
            Span::styled(" ▼", Style::default().fg(self.theme.text_muted)),
        ]);
        Paragraph::new(model_text).render(chunks[2], buf);

        // Working indicator
        if self.is_processing {
            let working_text = Line::from(vec![
                Span::styled("● ", Style::default().fg(self.theme.green)),
                Span::styled("Working...", Style::default().fg(self.theme.text_secondary)),
            ]);
            Paragraph::new(working_text).render(chunks[3], buf);
        }

        // Queue indicator
        if self.queue_count > 0 {
            let queue_text = Line::from(vec![
                Span::styled("📋 ", Style::default().fg(self.theme.yellow)),
                Span::styled(
                    self.queue_count.to_string(),
                    Style::default().fg(self.theme.text_primary),
                ),
            ]);
            Paragraph::new(queue_text).render(chunks[4], buf);
        }

        // Thinking toggle
        let thinking_color = if self.thinking_enabled {
            self.theme.yellow
        } else {
            self.theme.text_muted
        };
        let thinking_text = Line::from(Span::styled("🧠", Style::default().fg(thinking_color)));
        Paragraph::new(thinking_text).render(chunks[5], buf);

        // Help button
        let help_text = Line::from(Span::styled(
            "?",
            Style::default().fg(self.theme.text_muted),
        ));
        Paragraph::new(help_text).render(chunks[6], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_status_bar_renders_agent_mode() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let status = StatusBar::new(&theme).agent_mode(AgentMode::Build);
                f.render_widget(status, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Build"));
    }

    #[test]
    fn test_status_bar_shows_thinking_icon() {
        let backend = TestBackend::new(80, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let status = StatusBar::new(&theme).thinking(true);
                f.render_widget(status, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let content: String = (0..80)
            .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("🧠"));
    }
}
