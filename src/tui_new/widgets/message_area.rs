//! Message Area Widget
//!
//! Displays chat messages with different styles for each role
//! Feature: 03_message_display.feature

#![allow(clippy::useless_format)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Widget},
};

use crate::tui_new::theme::Theme;
use crate::tui_new::widgets::question::QuestionWidget;
use ratatui::style::Color;

/// Dim a color by a factor (0.0 = black, 1.0 = original color)
fn dim_color(color: Color, factor: f32) -> Color {
    match color {
        Color::Rgb(r, g, b) => Color::Rgb(
            (r as f32 * factor) as u8,
            (g as f32 * factor) as u8,
            (b as f32 * factor) as u8,
        ),
        _ => color,
    }
}

/// Wrap text to fit within a given width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut result = Vec::new();

    for line in text.lines() {
        if line.is_empty() {
            result.push(String::new());
            continue;
        }

        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= max_width {
            result.push(line.to_string());
        } else {
            // Wrap long lines
            let mut start = 0;
            while start < chars.len() {
                let end = (start + max_width).min(chars.len());
                let chunk: String = chars[start..end].iter().collect();
                result.push(chunk);
                start = end;
            }
        }
    }

    // Ensure at least one line
    if result.is_empty() {
        result.push(String::new());
    }

    result
}

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
    /// Streaming content (assistant is typing)
    streaming_content: Option<String>,
    /// Streaming thinking content
    streaming_thinking: Option<String>,
    /// Agent name for display
    agent_name: &'a str,
    /// Index of currently focused message
    focused_message_index: usize,
}

impl<'a> MessageArea<'a> {
    /// Create a new message area
    pub fn new(messages: &'a [Message], theme: &'a Theme) -> Self {
        Self {
            messages,
            scroll_offset: 0,
            theme,
            focused: false,
            streaming_content: None,
            streaming_thinking: None,
            agent_name: "Tark", // Default
            focused_message_index: 0,
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

    /// Set streaming content
    pub fn streaming_content(mut self, content: Option<String>) -> Self {
        self.streaming_content = content;
        self
    }

    /// Set streaming thinking
    pub fn streaming_thinking(mut self, thinking: Option<String>) -> Self {
        self.streaming_thinking = thinking;
        self
    }

    /// Set agent name
    pub fn agent_name(mut self, name: &'a str) -> Self {
        self.agent_name = name;
        self
    }

    /// Set focused message index
    pub fn focused_index(mut self, index: usize) -> Self {
        self.focused_message_index = index;
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
    fn role_label(&self, role: MessageRole) -> &str {
        match role {
            MessageRole::System => "System",
            MessageRole::User => "You",
            MessageRole::Agent => self.agent_name, // Use configurable name
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

    /// Get background color for message bubble
    fn role_bg_color(&self, role: MessageRole) -> ratatui::style::Color {
        match role {
            MessageRole::User => self.theme.user_bubble_bg,
            MessageRole::Agent => self.theme.agent_bubble_bg,
            MessageRole::Thinking => self.theme.thinking_bubble_bg,
            _ => self.theme.bg_dark,
        }
    }

    /// Get border color for message bubble
    fn role_border_color(&self, role: MessageRole) -> ratatui::style::Color {
        match role {
            MessageRole::User => self.theme.blue,
            MessageRole::Agent => self.theme.green,
            _ => self.theme.border,
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

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            let icon = self.role_icon(msg.role);
            let label = self.role_label(msg.role);
            let fg_color = self.role_color(msg.role);
            let bg_color = self.role_bg_color(msg.role);

            // Check if this message is focused
            let is_focused_msg = self.focused && msg_idx == self.focused_message_index;

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
                                    .bg(self.theme.agent_bubble_bg),
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
                    lines.push(Line::from(Span::styled(
                        &msg.content,
                        Style::default().fg(self.theme.text_secondary).bg(bg_color),
                    )));
                }
            } else {
                // Regular message with role icon, label, and content
                // For User and Agent messages, create a bubble effect with background
                if matches!(msg.role, MessageRole::User | MessageRole::Agent) {
                    let bg = bg_color;
                    let border_fg = self.role_border_color(msg.role);
                    let glow_color = dim_color(border_fg, 0.5);

                    // Fixed bubble width for consistent appearance
                    let bubble_content_width: usize = 56;

                    // Header line: icon with label (outside bubble)
                    let mut header_spans = vec![];

                    // Add cursor indicator if this message is focused
                    if is_focused_msg {
                        header_spans.push(Span::styled("> ", Style::default().fg(self.theme.cyan)));
                    } else {
                        header_spans.push(Span::raw(" "));
                    }

                    header_spans.push(Span::styled(
                        format!("{} ", icon),
                        Style::default().fg(border_fg),
                    ));
                    header_spans.push(Span::styled(
                        label,
                        Style::default().fg(self.theme.text_secondary),
                    ));

                    lines.push(Line::from(header_spans));

                    // Top border with rounded corners and glow
                    let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(top_border, Style::default().fg(glow_color)),
                    ]));

                    // Wrap content into lines that fit the bubble
                    let wrapped_lines = wrap_text(&msg.content, bubble_content_width - 2);

                    // Content lines with side borders and full background
                    for content_line in wrapped_lines {
                        // Pad content to fill the bubble width exactly
                        let char_count = content_line.chars().count();
                        let padding = bubble_content_width - 2 - char_count;
                        let padded = format!(" {}{} ", content_line, " ".repeat(padding));

                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled("│", Style::default().fg(glow_color)),
                            Span::styled(
                                padded,
                                Style::default().fg(self.theme.text_primary).bg(bg),
                            ),
                            Span::styled("│", Style::default().fg(glow_color)),
                        ]));
                    }

                    // Bottom border with rounded corners and glow
                    let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(bottom_border, Style::default().fg(glow_color)),
                    ]));
                } else {
                    // System, Tool, Command messages: simple format
                    let mut spans = vec![];

                    // Add cursor indicator if this message is focused
                    if is_focused_msg {
                        spans.push(Span::styled("> ", Style::default().fg(self.theme.cyan)));
                    }

                    spans.push(Span::styled(
                        format!("{} ", icon),
                        Style::default().fg(fg_color),
                    ));
                    spans.push(Span::styled(
                        label,
                        Style::default().fg(self.theme.text_muted),
                    ));
                    spans.push(Span::raw(" "));

                    spans.push(Span::styled(
                        &msg.content,
                        Style::default().fg(self.theme.text_primary),
                    ));

                    let line = Line::from(spans);
                    lines.push(line);
                }
            }

            // Add empty line between messages
            lines.push(Line::from(""));
        }

        // Add streaming content if present (assistant is typing)
        if let Some(ref content) = self.streaming_content {
            if !content.is_empty() {
                let icon = self.role_icon(MessageRole::Agent);
                let label = self.role_label(MessageRole::Agent);
                let border_fg = self.role_border_color(MessageRole::Agent);
                let glow_color = dim_color(border_fg, 0.5);
                let bg = self.role_bg_color(MessageRole::Agent);
                let bubble_content_width: usize = 56;

                // Header line
                let header_spans = vec![
                    Span::styled(format!(" {} ", icon), Style::default().fg(border_fg)),
                    Span::styled(label, Style::default().fg(self.theme.text_secondary)),
                    Span::styled(" (typing...)", Style::default().fg(self.theme.text_muted)),
                ];
                lines.push(Line::from(header_spans));

                // Top border
                let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(top_border, Style::default().fg(glow_color)),
                ]));

                // Wrap streaming content
                let wrapped_lines = wrap_text(content, bubble_content_width - 2);
                for content_line in wrapped_lines {
                    let char_count = content_line.chars().count();
                    let padding = bubble_content_width - 2 - char_count;
                    let padded = format!(" {}{} ", content_line, " ".repeat(padding));

                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled("│", Style::default().fg(glow_color)),
                        Span::styled(padded, Style::default().fg(self.theme.text_primary).bg(bg)),
                        Span::styled("│", Style::default().fg(glow_color)),
                    ]));
                }

                // Bottom border
                let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(bottom_border, Style::default().fg(glow_color)),
                ]));
                lines.push(Line::from(""));
            }
        }

        // Add streaming thinking if present and thinking mode enabled
        if let Some(ref thinking) = self.streaming_thinking {
            if !thinking.is_empty() {
                let icon = self.role_icon(MessageRole::Thinking);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", icon),
                        Style::default().fg(self.theme.thinking_fg),
                    ),
                    Span::styled("▼ Thinking ", Style::default().fg(self.theme.text_primary)),
                ]));
                lines.push(Line::from(Span::styled(
                    thinking,
                    Style::default()
                        .fg(self.theme.text_secondary)
                        .bg(self.theme.thinking_bubble_bg),
                )));
                lines.push(Line::from(""));
            }
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
