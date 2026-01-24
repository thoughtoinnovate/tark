//! Status Bar Widget
//!
//! Displays agent mode, model, thinking toggle, queue, and help button
//! Feature: 02_status_bar.feature

#![allow(clippy::vec_init_then_push)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
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
    /// Whether thinking mode is enabled (model-level extended thinking)
    thinking_enabled: bool,
    /// Whether thinking tool is enabled (structured reasoning)
    thinking_tool_enabled: bool,
    /// Task queue count
    queue_count: usize,
    /// Whether agent is processing
    is_processing: bool,
    /// Whether LLM is connected (green dot) or disconnected (red dot)
    llm_connected: bool,
    /// Theme for styling
    theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    /// Create a new status bar
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            agent_mode: AgentMode::Build,
            build_mode: BuildMode::Balanced,
            model_name: "tark_llm",
            provider_name: "tark_sim",
            thinking_enabled: true,
            thinking_tool_enabled: false,
            queue_count: 0,
            is_processing: false,
            llm_connected: false,
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

    /// Set thinking mode (model-level extended thinking)
    pub fn thinking(mut self, enabled: bool) -> Self {
        self.thinking_enabled = enabled;
        self
    }

    /// Set thinking tool (structured reasoning)
    pub fn thinking_tool(mut self, enabled: bool) -> Self {
        self.thinking_tool_enabled = enabled;
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

    /// Set LLM connection state
    pub fn connected(mut self, connected: bool) -> Self {
        self.llm_connected = connected;
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

        // Build the complete status bar as a single line (left to right):
        // agent • Build ▼  🟢 Balanced ▼  [🧠] [💭]  ≡ 7    ● Working...    • Model Provider  [?]
        let mut spans = vec![];

        // 1. Agent label (small, muted)
        spans.push(Span::styled(
            "agent ",
            Style::default().fg(self.theme.text_muted),
        ));

        // 2. Agent mode (• Build ▼)
        spans.push(Span::styled("• ", Style::default().fg(self.theme.yellow)));
        spans.push(Span::styled(
            self.agent_mode_str(),
            Style::default().fg(self.theme.yellow),
        ));
        spans.push(Span::styled(
            " ▼ ",
            Style::default().fg(self.theme.text_muted),
        ));

        // 3. Build mode (only if Build agent mode)
        if self.agent_mode == AgentMode::Build {
            spans.push(Span::raw(" "));
            spans.push(Span::styled("🟢 ", Style::default().fg(self.theme.green)));
            spans.push(Span::styled(
                self.build_mode_str(),
                Style::default().fg(self.theme.green),
            ));
            spans.push(Span::styled(
                " ▼",
                Style::default().fg(self.theme.text_muted),
            ));
        }

        // 4. Indicators section (thinking brain + thinking tool + queue)
        spans.push(Span::raw("  "));

        // Model-level thinking (brain)
        if self.thinking_enabled {
            // Thinking enabled: brain with golden/yellow border styling
            spans.push(Span::styled("[", Style::default().fg(self.theme.yellow)));
            spans.push(Span::styled(
                "🧠",
                Style::default().fg(self.theme.text_primary),
            ));
            spans.push(Span::styled("]", Style::default().fg(self.theme.yellow)));
        } else {
            // Thinking disabled: brain with muted border matching theme background
            spans.push(Span::styled(
                "[",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                "🧠",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                "]",
                Style::default().fg(self.theme.text_muted),
            ));
        }

        // Thinking tool (thought bubble)
        spans.push(Span::raw(" "));
        if self.thinking_tool_enabled {
            // Thinking tool enabled: thought bubble with cyan border
            spans.push(Span::styled("[", Style::default().fg(self.theme.cyan)));
            spans.push(Span::styled(
                "💭",
                Style::default().fg(self.theme.text_primary),
            ));
            spans.push(Span::styled("]", Style::default().fg(self.theme.cyan)));
        } else {
            // Thinking tool disabled: thought bubble with muted border
            spans.push(Span::styled(
                "[",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                "💭",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                "]",
                Style::default().fg(self.theme.text_muted),
            ));
        }

        if self.queue_count > 0 {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                "≡ ",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                self.queue_count.to_string(),
                Style::default().fg(self.theme.text_primary),
            ));
        }

        // 5. Working indicator (with padding for centering)
        if self.is_processing {
            spans.push(Span::raw("    "));
            spans.push(Span::styled("● ", Style::default().fg(self.theme.green)));
            spans.push(Span::styled(
                "Working...",
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        // Calculate right section width for alignment
        let model_provider_text = format!(
            "● {} {}  ?",
            self.model_name,
            self.provider_name.to_uppercase()
        );
        let left_width: usize = spans.iter().map(|s| s.width()).sum();
        let total_width = area.width as usize;
        let right_width = model_provider_text.len();

        if total_width > left_width + right_width {
            let padding = total_width - left_width - right_width;
            spans.push(Span::raw(" ".repeat(padding)));
        }

        // 6. Model/Provider (right-aligned) with connection indicator
        // Connection dot: green if connected, red if not
        let connection_dot_color = if self.llm_connected {
            self.theme.green
        } else {
            self.theme.red
        };
        spans.push(Span::styled(
            "● ",
            Style::default().fg(connection_dot_color),
        ));
        spans.push(Span::styled(
            self.model_name,
            Style::default().fg(self.theme.text_primary),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            self.provider_name.to_uppercase(),
            Style::default().fg(self.theme.text_muted),
        ));

        // 7. Help button (Ctrl+? to open)
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "[?]",
            Style::default().fg(self.theme.text_muted),
        ));

        let line = Line::from(spans);
        Paragraph::new(line).render(area, buf);
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
