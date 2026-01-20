//! Markdown rendering for TUI messages
//!
//! Converts markdown text to styled ratatui Lines/Spans

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui_new::theme::Theme;

fn wrap_lines(lines: Vec<Line<'static>>, max_width: usize) -> Vec<Line<'static>> {
    if max_width == 0 {
        return lines;
    }

    let mut wrapped: Vec<Line<'static>> = Vec::new();

    for line in lines {
        if line.spans.is_empty() {
            wrapped.push(Line::from(""));
            continue;
        }

        let mut current_spans: Vec<Span<'static>> = Vec::new();
        let mut current_width = 0usize;

        for span in line.spans {
            let content = span.content.to_string();
            let style = span.style;

            for ch in content.chars() {
                if current_width >= max_width {
                    wrapped.push(Line::from(std::mem::take(&mut current_spans)));
                    current_width = 0;
                }
                current_spans.push(Span::styled(ch.to_string(), style));
                current_width += 1;
            }
        }

        wrapped.push(Line::from(current_spans));
    }

    wrapped
}

/// Render markdown text to styled ratatui Lines
///
/// Supports:
/// - **Bold** text
/// - *Italic* text
/// - `Inline code`
/// - Code blocks with language hints
/// - Bullet lists
/// - Links (shown as underlined text)
pub fn render_markdown(text: &str, theme: &Theme, max_width: usize) -> Vec<Line<'static>> {
    let parser = Parser::new(text);
    let mut lines = Vec::new();
    let mut current_spans: Vec<Span> = Vec::new();
    let mut style_stack: Vec<Style> = vec![Style::default().fg(theme.text_primary)];
    let mut in_code_block = false;
    let mut code_block_lang = String::new();

    for event in parser {
        match event {
            Event::Text(text) => {
                let style = style_stack.last().copied().unwrap_or_default();
                if in_code_block {
                    // Code block content
                    for line in text.lines() {
                        lines.push(Line::from(vec![
                            Span::raw("  "),
                            Span::styled(
                                line.to_string(),
                                Style::default().fg(theme.text_primary).bg(theme.bg_code),
                            ),
                        ]));
                    }
                } else {
                    current_spans.push(Span::styled(text.to_string(), style));
                }
            }
            Event::Code(code) => {
                // Inline code: different background
                current_spans.push(Span::styled(
                    format!("`{}`", code),
                    Style::default().fg(theme.cyan).bg(theme.bg_code),
                ));
            }
            Event::Start(Tag::Strong) => {
                let current_style = style_stack.last().copied().unwrap_or_default();
                style_stack.push(current_style.add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Strong) => {
                style_stack.pop();
            }
            Event::Start(Tag::Emphasis) => {
                let current_style = style_stack.last().copied().unwrap_or_default();
                style_stack.push(current_style.add_modifier(Modifier::ITALIC));
            }
            Event::End(TagEnd::Emphasis) => {
                style_stack.pop();
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                in_code_block = true;
                // Code block header
                if let CodeBlockKind::Fenced(lang) = kind {
                    code_block_lang = lang.to_string();
                    lines.push(Line::from(Span::styled(
                        format!("```{}", lang),
                        Style::default().fg(theme.text_muted),
                    )));
                } else {
                    lines.push(Line::from(Span::styled(
                        "```",
                        Style::default().fg(theme.text_muted),
                    )));
                }
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                code_block_lang.clear();
                lines.push(Line::from(Span::styled(
                    "```",
                    Style::default().fg(theme.text_muted),
                )));
            }
            Event::Start(Tag::List(_)) => {
                // handled by Item
            }
            Event::End(TagEnd::List(_)) => {
                // Add spacing after list
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
            }
            Event::Start(Tag::Item) => {
                current_spans.push(Span::styled("• ", Style::default().fg(theme.cyan)));
            }
            Event::End(TagEnd::Item) => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
            }
            Event::Start(Tag::Link { .. }) => {
                let current_style = style_stack.last().copied().unwrap_or_default();
                style_stack.push(
                    current_style
                        .fg(theme.cyan)
                        .add_modifier(Modifier::UNDERLINED),
                );
            }
            Event::End(TagEnd::Link) => {
                style_stack.pop();
            }
            Event::Start(Tag::Heading { .. }) => {
                let current_style = style_stack.last().copied().unwrap_or_default();
                style_stack.push(current_style.fg(theme.cyan).add_modifier(Modifier::BOLD));
            }
            Event::End(TagEnd::Heading { .. }) => {
                style_stack.pop();
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
                lines.push(Line::from("")); // Empty line after heading
            }
            Event::SoftBreak => {
                current_spans.push(Span::raw(" "));
            }
            Event::HardBreak => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
            }
            Event::End(TagEnd::Paragraph) => {
                if !current_spans.is_empty() {
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
                lines.push(Line::from("")); // Empty line after paragraph
            }
            Event::InlineHtml(html) => {
                // Render inline HTML (like <thinking>) as plain text instead of stripping it
                let style = style_stack.last().copied().unwrap_or_default();
                current_spans.push(Span::styled(html.to_string(), style));
            }
            Event::Html(html) => {
                // Render block-level HTML as plain text instead of stripping it
                let style = style_stack.last().copied().unwrap_or_default();
                for line in html.lines() {
                    current_spans.push(Span::styled(line.to_string(), style));
                    lines.push(Line::from(std::mem::take(&mut current_spans)));
                }
            }
            _ => {}
        }
    }

    if !current_spans.is_empty() {
        lines.push(Line::from(current_spans));
    }

    // Remove trailing empty lines
    while lines.last().is_some_and(|l| l.spans.is_empty()) {
        lines.pop();
    }

    wrap_lines(lines, max_width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui_new::theme::Theme;

    #[test]
    fn test_html_tags_preserved_as_text() {
        let theme = Theme::default();
        let text = "I don't have the <thinking> feature";
        let lines = render_markdown(text, &theme, 100);

        // Collect all text content from the rendered lines
        let rendered: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        // The <thinking> tag should be preserved, not stripped
        assert!(
            rendered.contains("<thinking>"),
            "Expected '<thinking>' to be preserved in output, got: {}",
            rendered
        );
    }

    #[test]
    fn test_html_tags_in_complex_message() {
        let theme = Theme::default();
        let text = "Show your thinking in <thinking> tags please";
        let lines = render_markdown(text, &theme, 100);

        let rendered: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();

        assert!(
            rendered.contains("<thinking>"),
            "Expected '<thinking>' to be preserved, got: {}",
            rendered
        );
    }
}
