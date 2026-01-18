//! Message Area Widget
//!
//! Displays chat messages with different styles for each role
//! Feature: 03_message_display.feature

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
};

use crate::tui_new::theme::Theme;
use crate::tui_new::widgets::question::QuestionWidget;

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
    /// Optional question widget for interactive questions
    pub question: Option<QuestionWidget>,
}

impl Message {
    /// Create a new message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            collapsed: false,
            question: None,
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

    /// Create a question message
    pub fn question(question: QuestionWidget) -> Self {
        let content = question.text.clone();
        Self {
            role: MessageRole::Question,
            content,
            collapsed: false,
            question: Some(question),
        }
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

    /// Get role label for message
    fn role_label(&self, role: MessageRole) -> &'static str {
        match role {
            MessageRole::System => "System",
            MessageRole::User => "You",
            MessageRole::Agent => "Innodrupe",
            MessageRole::Tool => "Tool",
            MessageRole::Thinking => "Thinking",
            MessageRole::Question => "Question",
            MessageRole::Command => "Command",
        }
    }

    /// Get foreground color for message role
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

    /// Get background color for message bubble (user and agent only)
    fn role_bg_color(&self, role: MessageRole) -> Option<ratatui::style::Color> {
        match role {
            // User: blue tint (darker shade of blue)
            MessageRole::User => Some(Color::Rgb(45, 60, 83)),
            // Agent: green tint (darker shade of green)
            MessageRole::Agent => Some(Color::Rgb(45, 75, 55)),
            // Thinking: gray background
            MessageRole::Thinking => Some(Color::Rgb(55, 55, 65)),
            // Others: no background
            _ => None,
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
            let label = self.role_label(msg.role);
            let fg_color = self.role_color(msg.role);
            let bg_color = self.role_bg_color(msg.role);

            // Handle question messages specially
            if msg.role == MessageRole::Question {
                if let Some(ref question) = msg.question {
                    if question.answered {
                        // Answered state: show "✓ Answered: <selections>"
                        let answer_text = match question.question_type {
                            crate::tui_new::widgets::question::QuestionType::MultipleChoice
                            | crate::tui_new::widgets::question::QuestionType::SingleChoice => {
                                let selections: Vec<String> = question
                                    .selected
                                    .iter()
                                    .map(|&idx| question.options[idx].text.clone())
                                    .collect();
                                selections.join(", ")
                            }
                            crate::tui_new::widgets::question::QuestionType::FreeText => {
                                question.free_text_answer.clone()
                            }
                        };

                        lines.push(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                            Span::styled(
                                format!("{}", msg.content),
                                Style::default().fg(self.theme.text_primary),
                            ),
                        ]));
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled("✓ Answered: ", Style::default().fg(self.theme.green)),
                            Span::styled(
                                answer_text,
                                Style::default()
                                    .fg(self.theme.text_primary)
                                    .bg(Color::Rgb(45, 75, 55)),
                            ),
                        ]));
                    } else {
                        // Unanswered: show question header (full widget would need more space)
                        lines.push(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                            Span::styled(
                                &msg.content,
                                Style::default().fg(self.theme.text_primary),
                            ),
                        ]));
                        // Add options preview
                        for (idx, opt) in question.options.iter().enumerate() {
                            let checkbox = if question.selected.contains(&idx) {
                                "●"
                            } else {
                                "○"
                            };
                            lines.push(Line::from(vec![
                                Span::raw("  "),
                                Span::styled(
                                    format!("{} ", checkbox),
                                    Style::default().fg(fg_color),
                                ),
                                Span::styled(
                                    &opt.text,
                                    Style::default().fg(self.theme.text_secondary),
                                ),
                            ]));
                        }
                    }
                } else {
                    // Fallback if no question widget attached
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                        Span::styled(&msg.content, Style::default().fg(self.theme.text_primary)),
                    ]));
                }
                lines.push(Line::from(""));
                continue;
            }

            // Check if this message type is collapsible
            let is_collapsible = matches!(msg.role, MessageRole::Thinking | MessageRole::Tool);

            if is_collapsible {
                // Collapsible message with chevron indicator
                let chevron = if msg.collapsed { "▶" } else { "▼" };

                let header_line = Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                    Span::styled(
                        format!("{} {} ", chevron, label),
                        Style::default().fg(self.theme.text_primary),
                    ),
                ]);
                lines.push(header_line);

                // Show content only if not collapsed
                if !msg.collapsed {
                    if let Some(bg) = bg_color {
                        lines.push(Line::from(Span::styled(
                            &msg.content,
                            Style::default().fg(self.theme.text_secondary).bg(bg),
                        )));
                    } else {
                        lines.push(Line::from(Span::styled(
                            &msg.content,
                            Style::default().fg(self.theme.text_secondary),
                        )));
                    }
                }
            } else {
                // Regular message with role icon, label, and content
                let mut spans = vec![
                    // Icon with colored background for user/agent
                    Span::styled(
                        format!("{} ", icon),
                        if bg_color.is_some() {
                            Style::default().fg(fg_color).bg(fg_color)
                        } else {
                            Style::default().fg(fg_color)
                        },
                    ),
                    // Role label
                    Span::styled(
                        format!("{}", label),
                        Style::default().fg(self.theme.text_primary),
                    ),
                    Span::raw(" "),
                ];

                // Add message content with bubble background for user/agent
                if let Some(bg) = bg_color {
                    spans.push(Span::styled(
                        &msg.content,
                        Style::default().fg(self.theme.text_primary).bg(bg),
                    ));
                } else {
                    spans.push(Span::styled(
                        &msg.content,
                        Style::default().fg(self.theme.text_primary),
                    ));
                }

                let line = Line::from(spans);
                lines.push(line);
            }

            // Add empty line between messages
            lines.push(Line::from(""));
        }

        // Store total line count before filtering
        let total_lines = lines.len();

        // Apply scroll offset (multiply by 2 since each message produces 2 lines: content + empty)
        let line_offset = self.scroll_offset * 2;
        let visible_lines: Vec<Line> = lines.into_iter().skip(line_offset).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);

        // Always render scrollbar when content exceeds viewport
        if total_lines > inner.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(self.theme.text_muted))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state =
                ScrollbarState::new(total_lines.saturating_sub(inner.height as usize))
                    .position(line_offset);

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
