//! Markdown rendering for TUI messages
//!
//! Converts markdown text to styled ratatui Lines/Spans.
//! Includes incremental rendering support for streaming content.

use pulldown_cmark::{CodeBlockKind, Event, Parser, Tag, TagEnd};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use crate::tui_new::theme::Theme;

/// Cache for incremental markdown rendering during streaming.
///
/// This struct enables efficient rendering of streaming content by:
/// - Caching already-rendered lines up to a "stable point" (paragraph break)
/// - Only re-parsing content from the stable point onward
///
/// This avoids O(n) re-parsing of the entire content on every frame.
#[derive(Debug, Clone, Default)]
pub struct StreamingMarkdownCache {
    /// Cached rendered lines (up to stable_byte_offset)
    cached_lines: Vec<Line<'static>>,
    /// Byte offset up to which we have stable, cached lines
    stable_byte_offset: usize,
    /// The max_width used for the cached lines (invalidates cache if changed)
    cached_max_width: usize,
}

impl StreamingMarkdownCache {
    /// Create a new empty cache
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear the cache (call when streaming completes or content changes)
    pub fn clear(&mut self) {
        self.cached_lines.clear();
        self.stable_byte_offset = 0;
        self.cached_max_width = 0;
    }

    /// Render markdown incrementally, using cached lines where possible.
    ///
    /// This finds the last "stable point" (double newline / paragraph break)
    /// in the cached region and only re-parses from there.
    pub fn render_incremental(
        &mut self,
        text: &str,
        theme: &Theme,
        max_width: usize,
    ) -> Vec<Line<'static>> {
        // If max_width changed, invalidate cache
        if max_width != self.cached_max_width {
            self.clear();
            self.cached_max_width = max_width;
        }

        // Find the last stable point (paragraph break) in the already-processed region
        // A paragraph break is "\n\n" which marks a clear boundary for markdown parsing
        let stable_point = find_last_stable_point(text, self.stable_byte_offset);

        // Ensure stable_point is at a valid char boundary before slicing
        if stable_point > self.stable_byte_offset
            && stable_point <= text.len()
            && text.is_char_boundary(stable_point)
        {
            // We have new stable content - render and cache it
            let stable_text = &text[..stable_point];
            let new_cached_lines = render_markdown(stable_text, theme, max_width);
            self.cached_lines = new_cached_lines;
            self.stable_byte_offset = stable_point;
        }

        // Now render the unstable tail (from stable_point to end)
        if self.stable_byte_offset < text.len() && text.is_char_boundary(self.stable_byte_offset) {
            let unstable_text = &text[self.stable_byte_offset..];
            let tail_lines = render_markdown(unstable_text, theme, max_width);

            // Combine cached + tail
            let mut result = self.cached_lines.clone();
            result.extend(tail_lines);
            result
        } else if self.stable_byte_offset >= text.len() {
            // All content is stable/cached
            self.cached_lines.clone()
        } else {
            // stable_byte_offset is not at a valid char boundary, re-render everything
            render_markdown(text, theme, max_width)
        }
    }
}

/// Find the last "stable point" in text where we can safely cache rendered lines.
///
/// A stable point is after a paragraph break ("\n\n") or at the start of the text.
/// This ensures that markdown constructs (bold, code, etc.) are complete.
fn find_last_stable_point(text: &str, min_offset: usize) -> usize {
    // Look for the last "\n\n" that is after min_offset but before the end
    // We want some buffer at the end to avoid re-parsing just for a few characters
    const MIN_TAIL_SIZE: usize = 100; // Keep at least 100 bytes unparsed for the "tail"

    if text.len() <= min_offset + MIN_TAIL_SIZE {
        return min_offset; // Not enough new content to bother updating cache
    }

    // Ensure min_offset is at a valid char boundary
    let safe_min_offset = if text.is_char_boundary(min_offset) {
        min_offset
    } else {
        // Find the next valid char boundary
        (min_offset..text.len())
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(text.len())
    };

    // Calculate end position and ensure it's at a valid char boundary
    let end_pos = text.len().saturating_sub(MIN_TAIL_SIZE);
    let safe_end_pos = if text.is_char_boundary(end_pos) {
        end_pos
    } else {
        // Find the previous valid char boundary
        (0..end_pos)
            .rev()
            .find(|&i| text.is_char_boundary(i))
            .unwrap_or(0)
    };

    // If we can't create a valid slice, return the original min_offset
    if safe_min_offset >= safe_end_pos {
        return min_offset;
    }

    let search_region = &text[safe_min_offset..safe_end_pos];

    // Find the last paragraph break in the search region
    if let Some(pos) = search_region.rfind("\n\n") {
        // Return the position after the paragraph break (start of next paragraph)
        safe_min_offset + pos + 2
    } else {
        // No paragraph break found, keep existing stable point
        min_offset
    }
}

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

    #[test]
    fn test_multibyte_characters_in_streaming() {
        let theme = Theme::default();
        // Text with curly apostrophe (3-byte UTF-8 character: U+2019)
        let text = "So far, we haven't covered any specific programming tasks";
        let mut cache = StreamingMarkdownCache::new();

        // Should not panic with multi-byte characters
        let lines = cache.render_incremental(text, &theme, 80);
        assert!(!lines.is_empty());

        // Verify the content is preserved
        let rendered: String = lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect();
        assert!(rendered.contains("haven't"));
    }

    #[test]
    fn test_streaming_cache_with_emoji() {
        let theme = Theme::default();
        // Text with emoji (4-byte UTF-8 character)
        let text =
            "Hello 👋 world! This is a longer message with emoji characters 🎉 to test streaming.";
        let mut cache = StreamingMarkdownCache::new();

        // Should not panic with multi-byte characters
        let lines = cache.render_incremental(text, &theme, 80);
        assert!(!lines.is_empty());
    }
}
