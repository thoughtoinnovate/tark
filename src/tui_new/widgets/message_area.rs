//! Message Area Widget
//!
//! Displays chat messages with different styles for each role
//! Feature: 03_message_display.feature

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
};

use crate::tui_new::theme::Theme;

/// Message role/type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageRole {
    /// System messages (cyan)
    System,
    /// User messages (blue bubble)
    User,
    /// Agent/assistant messages (green bubble)
    Agent,
    /// Tool execution messages
    Tool,
    /// Thinking/reasoning blocks
    Thinking,
    /// Question prompts
    Question,
    /// Command execution
    Command,
}

/// A single message in the chat
#[derive(Debug, Clone)]
pub struct Message {
    /// Message role
    pub role: MessageRole,
    /// Message content
    pub content: String,
    /// Whether this message is collapsed (for thinking/tool)
    pub collapsed: bool,
}

impl Message {
    /// Create a new message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            collapsed: false,
        }
    }

    /// Create a system message
    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }

    /// Create a user message
    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    /// Create an agent message
    pub fn agent(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Agent, content)
    }
}

/// Message area widget displaying chat history
pub struct MessageArea<'a> {
    /// Messages to display
    messages: &'a [Message],
    /// Scroll offset
    scroll_offset: usize,
    /// Theme for styling
    theme: &'a Theme,
    /// Whether this area is focused
    focused: bool,
}

impl<'a> MessageArea<'a> {
    /// Create a new message area
    pub fn new(messages: &'a [Message], theme: &'a Theme) -> Self {
        Self {
            messages,
            scroll_offset: 0,
            theme,
            focused: false,
        }
    }

    /// Set scroll offset
    pub fn scroll(mut self, offset: usize) -> Self {
        self.scroll_offset = offset;
        self
    }

    /// Set focused state
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Get icon for message role
    fn role_icon(&self, role: MessageRole) -> &'static str {
        match role {
            MessageRole::System => "●",
            MessageRole::User => "👤",
            MessageRole::Agent => "🤖",
            MessageRole::Tool => "🔧",
            MessageRole::Thinking => "🧠",
            MessageRole::Question => "❓",
            MessageRole::Command => "$",
        }
    }

    /// Get color for message role
    fn role_color(&self, role: MessageRole) -> ratatui::style::Color {
        match role {
            MessageRole::System => self.theme.system_fg,
            MessageRole::User => self.theme.user_bubble,
            MessageRole::Agent => self.theme.agent_bubble,
            MessageRole::Tool => self.theme.tool_fg,
            MessageRole::Thinking => self.theme.thinking_fg,
            MessageRole::Question => self.theme.question_fg,
            MessageRole::Command => self.theme.command_fg,
        }
    }
}

impl Widget for MessageArea<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let border_color = if self.focused {
            self.theme.border_focused
        } else {
            self.theme.border
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" Messages ");

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 1 || inner.width < 1 {
            return;
        }

        // Build lines from messages
        let mut lines: Vec<Line> = Vec::new();

        for msg in self.messages.iter() {
            let icon = self.role_icon(msg.role);
            let color = self.role_color(msg.role);

            // Add message with icon
            let line = Line::from(vec![
                Span::styled(format!("{} ", icon), Style::default().fg(color)),
                Span::styled(&msg.content, Style::default().fg(self.theme.text_primary)),
            ]);
            lines.push(line);

            // Add empty line between messages
            lines.push(Line::from(""));
        }

        // Apply scroll offset (multiply by 2 since each message produces 2 lines: content + empty)
        let line_offset = self.scroll_offset * 2;
        let visible_lines: Vec<Line> = lines.into_iter().skip(line_offset).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);

        // Render scrollbar if needed
        if self.messages.len() > inner.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight);
            let mut scrollbar_state =
                ScrollbarState::new(self.messages.len()).position(self.scroll_offset);

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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_message_area_renders_messages() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let messages = vec![
            Message::system("Welcome to Tark"),
            Message::user("Hello!"),
            Message::agent("Hi there!"),
        ];

        terminal
            .draw(|f| {
                let area = MessageArea::new(&messages, &theme);
                f.render_widget(area, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Check that content is rendered (simplified check)
        let content: String = (0..60)
            .map(|x| buffer.cell((x, 1)).unwrap().symbol().to_string())
            .collect();

        assert!(content.contains("Welcome"));
    }
}
