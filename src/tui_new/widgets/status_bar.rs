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

/// Clickable section of the status bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StatusSection {
    AgentMode,
    BuildMode,
    Thinking,
    ThinkingTool,
    Provider,
    Help,
}

/// Screen x-ranges of the clickable status bar sections (P2.5).
///
/// Recorded during render from the exact spans drawn, so clicks resolve
/// against real (variable-width, emoji-bearing) content instead of hardcoded
/// column estimates.
#[derive(Debug, Default, Clone)]
pub struct StatusSectionMap {
    /// Agent mode area (cycles agent mode).
    pub agent_mode: Option<Rect>,
    /// Build mode area (cycles build mode; only in Build agent mode).
    pub build_mode: Option<Rect>,
    /// Thinking `[🧠]` toggle.
    pub thinking: Option<Rect>,
    /// Thinking-tool `[💭]` indicator.
    pub thinking_tool: Option<Rect>,
    /// Provider/model area (opens provider picker).
    pub provider: Option<Rect>,
    /// Help `[?]` indicator.
    pub help: Option<Rect>,
}

impl<'a> StatusBar<'a> {
    /// Render while recording clickable section geometry into `map` (P2.5).
    pub fn render_with_map(self, area: Rect, buf: &mut Buffer, map: &mut StatusSectionMap) {
        if area.height < 1 {
            return;
        }

        // Build the complete status bar as tagged sections (left to right):
        // Build ▼  🟢 Balanced ▼  [🧠] [💭]  ≡ 7    ● Model Provider  [?]
        let mut sections: Vec<(Option<StatusSection>, Span)> = vec![];

        // 1. Agent mode (Build ▼)
        let tag = Some(StatusSection::AgentMode);
        sections.push((
            tag,
            Span::styled(
                self.agent_mode_str(),
                Style::default().fg(self.theme.yellow),
            ),
        ));
        sections.push((
            tag,
            Span::styled(" ▼ ", Style::default().fg(self.theme.text_muted)),
        ));

        // 2. Build mode (only if Build agent mode)
        if self.agent_mode == AgentMode::Build {
            let tag = Some(StatusSection::BuildMode);
            sections.push((tag, Span::raw(" ")));
            sections.push((
                tag,
                Span::styled("🟢 ", Style::default().fg(self.theme.green)),
            ));
            sections.push((
                tag,
                Span::styled(self.build_mode_str(), Style::default().fg(self.theme.green)),
            ));
            sections.push((
                tag,
                Span::styled(" ▼", Style::default().fg(self.theme.text_muted)),
            ));
        }

        // 3. Indicators section (thinking brain + thinking tool + queue)
        sections.push((None, Span::raw("  ")));

        // Model-level thinking (brain)
        let brain_border = if self.thinking_enabled {
            self.theme.yellow
        } else {
            self.theme.text_muted
        };
        let brain_fg = if self.thinking_enabled {
            self.theme.text_primary
        } else {
            self.theme.text_muted
        };
        let tag = Some(StatusSection::Thinking);
        sections.push((tag, Span::styled("[", Style::default().fg(brain_border))));
        sections.push((tag, Span::styled("🧠", Style::default().fg(brain_fg))));
        sections.push((tag, Span::styled("]", Style::default().fg(brain_border))));

        // Thinking tool (thought bubble)
        sections.push((None, Span::raw(" ")));
        let tool_border = if self.thinking_tool_enabled {
            self.theme.cyan
        } else {
            self.theme.text_muted
        };
        let tool_fg = if self.thinking_tool_enabled {
            self.theme.text_primary
        } else {
            self.theme.text_muted
        };
        let tag = Some(StatusSection::ThinkingTool);
        sections.push((tag, Span::styled("[", Style::default().fg(tool_border))));
        sections.push((tag, Span::styled("💭", Style::default().fg(tool_fg))));
        sections.push((tag, Span::styled("]", Style::default().fg(tool_border))));

        if self.queue_count > 0 {
            sections.push((None, Span::raw("  ")));
            sections.push((
                None,
                Span::styled("≡ ", Style::default().fg(self.theme.text_muted)),
            ));
            sections.push((
                None,
                Span::styled(
                    self.queue_count.to_string(),
                    Style::default().fg(self.theme.text_primary),
                ),
            ));
        }

        // Calculate right section width for alignment
        let model_provider_text = format!(
            "● {} {}  ?",
            self.model_name,
            self.provider_name.to_uppercase()
        );
        let left_width: usize = sections.iter().map(|(_, s)| s.width()).sum();
        let total_width = area.width as usize;
        let right_width = model_provider_text.len();

        if total_width > left_width + right_width {
            let padding = total_width - left_width - right_width;
            sections.push((None, Span::raw(" ".repeat(padding))));
        }

        // 4. Model/Provider (right-aligned) with connection indicator
        // Connection dot: green if connected, red if not
        let connection_dot_color = if self.llm_connected {
            self.theme.green
        } else {
            self.theme.red
        };
        let tag = Some(StatusSection::Provider);
        sections.push((
            tag,
            Span::styled("● ", Style::default().fg(connection_dot_color)),
        ));
        sections.push((
            tag,
            Span::styled(
                self.model_name,
                Style::default().fg(self.theme.text_primary),
            ),
        ));
        sections.push((tag, Span::raw(" ")));
        sections.push((
            tag,
            Span::styled(
                self.provider_name.to_uppercase(),
                Style::default().fg(self.theme.text_muted),
            ),
        ));

        // 5. Help button (Ctrl+? to open)
        sections.push((None, Span::raw("  ")));
        let tag = Some(StatusSection::Help);
        sections.push((
            tag,
            Span::styled("[?]", Style::default().fg(self.theme.text_muted)),
        ));

        // Record each tagged section's screen x-range from the exact spans.
        *map = StatusSectionMap::default();
        let mut x = area.x;
        let mut open: Option<(StatusSection, u16)> = None;
        let flush = |map: &mut StatusSectionMap, section: StatusSection, start: u16, end: u16| {
            let rect = Rect::new(start, area.y, end.saturating_sub(start), 1);
            match section {
                StatusSection::AgentMode => map.agent_mode = Some(rect),
                StatusSection::BuildMode => map.build_mode = Some(rect),
                StatusSection::Thinking => map.thinking = Some(rect),
                StatusSection::ThinkingTool => map.thinking_tool = Some(rect),
                StatusSection::Provider => map.provider = Some(rect),
                StatusSection::Help => map.help = Some(rect),
            }
        };
        for (section, span) in &sections {
            let w = span.width() as u16;
            match (*section, open) {
                (Some(s), None) => open = Some((s, x)),
                (Some(s), Some((o, start))) if s != o => {
                    flush(map, o, start, x);
                    open = Some((s, x));
                }
                (None, Some((o, start))) => {
                    flush(map, o, start, x);
                    open = None;
                }
                _ => {}
            }
            x = x.saturating_add(w);
        }
        if let Some((o, start)) = open {
            flush(map, o, start, x);
        }

        let spans: Vec<Span> = sections.into_iter().map(|(_, s)| s).collect();
        let line = Line::from(spans);
        Paragraph::new(line).render(area, buf);
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut discard = StatusSectionMap::default();
        self.render_with_map(area, buf, &mut discard);
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
    fn section_map_matches_rendered_columns() {
        use ratatui::layout::Rect;
        let backend = TestBackend::new(100, 1);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();
        let mut map = StatusSectionMap::default();

        terminal
            .draw(|f| {
                let status = StatusBar::new(&theme).agent_mode(AgentMode::Build);
                status.render_with_map(f.area(), f.buffer_mut(), &mut map);
            })
            .unwrap();

        let end = |r: &Rect| r.x + r.width;
        let agent = map.agent_mode.expect("agent");
        let build = map.build_mode.expect("build");
        let thinking = map.thinking.expect("thinking");
        let tool = map.thinking_tool.expect("tool");
        let provider = map.provider.expect("provider");
        let help = map.help.expect("help");

        // Sections tile left-to-right with no overlaps.
        assert_eq!(agent.x, 0);
        assert_eq!(build.x, end(&agent));
        assert_eq!(thinking.x, end(&build) + 2);
        assert_eq!(tool.x, end(&thinking) + 1);
        assert!(provider.x > end(&tool));
        assert_eq!(end(&provider), help.x - 2);
        assert_eq!(end(&help), 100);
        assert_eq!(help.width, 3); // "[?]"

        // Buffer text under each recorded range matches the section.
        let buffer = terminal.backend().buffer();
        let text_at = |r: &Rect| -> String {
            (r.x..r.x + r.width)
                .map(|x| buffer.cell((x, 0)).unwrap().symbol().to_string())
                .collect()
        };
        assert!(text_at(&agent).contains("Build"));
        assert!(text_at(&thinking).contains("🧠"));
        assert!(text_at(&help).contains("[?]"));
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
