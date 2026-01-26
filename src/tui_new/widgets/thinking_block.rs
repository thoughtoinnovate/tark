//! Thinking block widget for displaying structured reasoning
//!
//! Renders the thinking history from the ThinkTool as a formatted block

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

/// Widget that renders thinking history
pub struct ThinkingBlockWidget<'a> {
    thoughts: &'a [crate::tools::builtin::Thought],
    collapsed: bool,
    focused: bool,
    theme: &'a super::super::theme::Theme,
}

impl<'a> ThinkingBlockWidget<'a> {
    /// Create a new thinking block widget
    pub fn new(
        thoughts: &'a [crate::tools::builtin::Thought],
        theme: &'a super::super::theme::Theme,
    ) -> Self {
        Self {
            thoughts,
            collapsed: false,
            focused: false,
            theme,
        }
    }

    /// Set collapsed state
    pub fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }

    /// Set focused state
    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Build the header line
    fn header_line(&self) -> Line<'a> {
        let current = self.thoughts.last();
        let progress = if let Some(thought) = current {
            format!(" ({}/{})", thought.thought_number, thought.total_thoughts)
        } else {
            String::new()
        };

        let header_style = if self.focused {
            Style::default()
                .fg(self.theme.thinking_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.thinking_fg)
        };

        Line::from(vec![
            Span::styled("🧠 ", header_style),
            Span::styled("Thinking...", header_style),
            Span::styled(progress, header_style.add_modifier(Modifier::DIM)),
        ])
    }

    /// Build the content lines
    fn content_lines(&self) -> Vec<Line<'a>> {
        if self.collapsed {
            return vec![];
        }

        let mut lines = Vec::new();

        for thought in self.thoughts {
            // Thought number and content
            let number_style = Style::default()
                .fg(self.theme.thinking_fg)
                .add_modifier(Modifier::BOLD);
            let content_style = Style::default().fg(Color::Gray);

            lines.push(Line::from(vec![
                Span::styled(format!("{}. ", thought.thought_number), number_style),
                Span::styled(&thought.thought, content_style),
            ]));

            // Metadata line (thought_type and confidence)
            let mut metadata_spans = Vec::new();
            if let Some(ref thought_type) = thought.thought_type {
                let type_color = match thought_type.as_str() {
                    "hypothesis" => Color::Cyan,
                    "analysis" => Color::Blue,
                    "plan" => Color::Green,
                    "decision" => Color::Yellow,
                    "reflection" => Color::Magenta,
                    _ => Color::Gray,
                };
                metadata_spans.push(Span::styled(
                    format!("   [{}]", thought_type),
                    Style::default()
                        .fg(type_color)
                        .add_modifier(Modifier::ITALIC),
                ));
            }

            if let Some(confidence) = thought.confidence {
                let confidence_pct = (confidence * 100.0) as u8;
                let confidence_style = if confidence >= 0.8 {
                    Style::default().fg(Color::Green)
                } else if confidence >= 0.5 {
                    Style::default().fg(Color::Yellow)
                } else {
                    Style::default().fg(Color::Red)
                };
                metadata_spans.push(Span::styled(" ", Style::default()));
                metadata_spans.push(Span::styled(
                    format!("confidence: {}%", confidence_pct),
                    confidence_style.add_modifier(Modifier::DIM),
                ));
            }

            if !metadata_spans.is_empty() {
                lines.push(Line::from(metadata_spans));
            }

            // Add spacing between thoughts
            lines.push(Line::from(""));
        }

        // Show "thinking..." indicator if more thoughts are coming
        if let Some(last) = self.thoughts.last() {
            if last.next_thought_needed {
                lines.push(Line::from(vec![Span::styled(
                    "   ⋯ Thinking...",
                    Style::default()
                        .fg(self.theme.thinking_fg)
                        .add_modifier(Modifier::DIM | Modifier::ITALIC),
                )]));
            }
        }

        lines
    }
}

impl Widget for ThinkingBlockWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build header
        let header = self.header_line();

        // Build content
        let content = self.content_lines();

        // Combine header and content
        let mut all_lines = vec![header];
        if !self.collapsed {
            all_lines.push(Line::from("")); // Spacer
            all_lines.extend(content);
        }

        // Create block with dashed border
        let border_style = if self.focused {
            Style::default()
                .fg(self.theme.thinking_fg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(border_style)
            .style(Style::default().bg(Color::Black));

        // Render paragraph with wrapping
        let paragraph = Paragraph::new(all_lines)
            .block(block)
            .wrap(Wrap { trim: false });

        paragraph.render(area, buf);
    }
}
