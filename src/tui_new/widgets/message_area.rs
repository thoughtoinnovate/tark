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
use std::time::{Duration, Instant};

/// Cursor blink interval
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

/// Tool loading blink interval (faster for visual feedback)
const TOOL_BLINK_INTERVAL: Duration = Duration::from_millis(400);

/// Global cursor blink state for messages (shared across renders)
static mut MSG_LAST_BLINK: Option<Instant> = None;
static mut MSG_CURSOR_VISIBLE: bool = true;

/// Global tool loading blink state (shared across renders)
static mut TOOL_LAST_BLINK: Option<Instant> = None;
static mut TOOL_INDICATOR_VISIBLE: bool = true;

/// Get current cursor visibility state for messages (blinks every 530ms)
fn get_message_cursor_visible() -> bool {
    unsafe {
        let now = Instant::now();
        if let Some(last) = MSG_LAST_BLINK {
            if now.duration_since(last) >= CURSOR_BLINK_INTERVAL {
                MSG_CURSOR_VISIBLE = !MSG_CURSOR_VISIBLE;
                MSG_LAST_BLINK = Some(now);
            }
        } else {
            MSG_LAST_BLINK = Some(now);
            MSG_CURSOR_VISIBLE = true;
        }
        MSG_CURSOR_VISIBLE
    }
}

/// Get current tool loading indicator visibility state (blinks every 400ms)
fn get_tool_indicator_visible() -> bool {
    unsafe {
        let now = Instant::now();
        if let Some(last) = TOOL_LAST_BLINK {
            if now.duration_since(last) >= TOOL_BLINK_INTERVAL {
                TOOL_INDICATOR_VISIBLE = !TOOL_INDICATOR_VISIBLE;
                TOOL_LAST_BLINK = Some(now);
            }
        } else {
            TOOL_LAST_BLINK = Some(now);
            TOOL_INDICATOR_VISIBLE = true;
        }
        TOOL_INDICATOR_VISIBLE
    }
}

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
    /// Timestamp of the message (HH:MM:SS)
    pub timestamp: String,
    /// Optional question widget for interactive questions
    pub question: Option<QuestionWidget>,
    /// Original tool arguments (for rich rendering of tools like think)
    pub tool_args: Option<serde_json::Value>,
}

impl Message {
    /// Create a new message
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
            collapsed: false,
            timestamp: String::new(),
            question: None,
            tool_args: None,
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
            timestamp: String::new(),
            question: Some(question),
            tool_args: None,
        }
    }
}

/// A group of consecutive tools with the same risk level
#[derive(Debug, Clone)]
pub struct ToolGroup {
    /// Risk level group name (Exploration, Changes, Commands, Destructive)
    pub risk_group: String,
    /// Message indices of tools in this group
    pub tool_indices: Vec<usize>,
    /// Starting message index (first tool in group)
    pub start_index: usize,
    /// Whether all tools in the group are completed
    pub all_completed: bool,
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
    /// Pre-rendered streaming lines (for incremental rendering optimization)
    /// Uses 'static lifetime since these come from a cache and are cloned
    streaming_lines: Option<Vec<Line<'static>>>,
    /// Pre-rendered thinking lines (for incremental rendering optimization)
    thinking_lines: Option<Vec<Line<'static>>>,
    /// Whether LLM is currently processing (shows placeholder before streaming starts)
    is_processing: bool,
    /// Agent name for display
    agent_name: &'a str,
    /// Index of currently focused message
    focused_message_index: usize,
    /// Sub-index for hierarchical navigation within tool groups
    focused_sub_index: Option<usize>,
    /// Vim mode
    vim_mode: crate::ui_backend::VimMode,
    /// Set of collapsed tool group start indices
    collapsed_tool_groups: &'a std::collections::HashSet<usize>,
}

/// Static empty hashset for default collapsed_tool_groups
static EMPTY_HASHSET: std::sync::LazyLock<std::collections::HashSet<usize>> =
    std::sync::LazyLock::new(std::collections::HashSet::new);

/// Parse risk group from tool message content
/// Format: "status|name|risk_group|content" or legacy "status|name|content"
pub fn parse_tool_risk_group(content: &str) -> &str {
    let parts: Vec<&str> = content.splitn(4, '|').collect();
    if parts.len() >= 4 {
        // New format: status|name|risk_group|content
        parts[2]
    } else {
        // Legacy format: status|name|content - default to Exploration
        "Exploration"
    }
}

/// Collect consecutive tool messages into groups by risk level
fn collect_tool_groups(messages: &[Message]) -> Vec<ToolGroup> {
    let mut groups: Vec<ToolGroup> = Vec::new();
    let mut current_group: Option<ToolGroup> = None;

    for (idx, msg) in messages.iter().enumerate() {
        if msg.role == MessageRole::Tool {
            let risk_group = parse_tool_risk_group(&msg.content);

            // Check if tool is completed (status is ✓ or ✗)
            let is_completed = msg.content.starts_with('✓') || msg.content.starts_with('✗');

            match &mut current_group {
                Some(group) if group.risk_group == risk_group => {
                    // Same risk group - add to current group
                    group.tool_indices.push(idx);
                    if !is_completed {
                        group.all_completed = false;
                    }
                }
                Some(group) => {
                    // Different risk group - save current and start new
                    groups.push(group.clone());
                    current_group = Some(ToolGroup {
                        risk_group: risk_group.to_string(),
                        tool_indices: vec![idx],
                        start_index: idx,
                        all_completed: is_completed,
                    });
                }
                None => {
                    // Start new group
                    current_group = Some(ToolGroup {
                        risk_group: risk_group.to_string(),
                        tool_indices: vec![idx],
                        start_index: idx,
                        all_completed: is_completed,
                    });
                }
            }
        } else {
            // Non-tool message - save any current group
            if let Some(group) = current_group.take() {
                groups.push(group);
            }
        }
    }

    // Don't forget the last group
    if let Some(group) = current_group {
        groups.push(group);
    }

    groups
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
            streaming_lines: None,
            thinking_lines: None,
            is_processing: false,
            agent_name: "Tark", // Default
            focused_message_index: 0,
            focused_sub_index: None,
            vim_mode: crate::ui_backend::VimMode::Insert,
            collapsed_tool_groups: &EMPTY_HASHSET,
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

    /// Set pre-rendered streaming lines (for incremental rendering optimization)
    ///
    /// When set, these lines are used instead of parsing streaming_content.
    /// This avoids O(n) markdown parsing on every frame during streaming.
    pub fn streaming_lines(mut self, lines: Option<Vec<Line<'static>>>) -> Self {
        self.streaming_lines = lines;
        self
    }

    /// Set pre-rendered thinking lines (for incremental rendering optimization)
    pub fn thinking_lines(mut self, lines: Option<Vec<Line<'static>>>) -> Self {
        self.thinking_lines = lines;
        self
    }

    /// Set processing state (shows placeholder while waiting for first chunk)
    pub fn processing(mut self, is_processing: bool) -> Self {
        self.is_processing = is_processing;
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

    /// Set focused sub-index for hierarchical navigation
    pub fn focused_sub_index(mut self, sub: Option<usize>) -> Self {
        self.focused_sub_index = sub;
        self
    }

    pub fn vim_mode(mut self, mode: crate::ui_backend::VimMode) -> Self {
        self.vim_mode = mode;
        self
    }

    /// Set collapsed tool groups
    pub fn collapsed_tool_groups(
        mut self,
        collapsed: &'a std::collections::HashSet<usize>,
    ) -> Self {
        self.collapsed_tool_groups = collapsed;
        self
    }

    pub fn metrics(&self, area: Rect) -> (usize, usize) {
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
        if inner.height < 1 || inner.width < 1 {
            return (0, 0);
        }

        let lines = self.build_lines(inner);
        (lines.len(), inner.height as usize)
    }

    fn build_lines(&self, inner: Rect) -> Vec<Line<'static>> {
        // Calculate dynamic bubble width: 90% of panel width (5% padding on each side)
        // Minimum 40 chars, maximum based on available width
        let available_width = inner.width.saturating_sub(4) as usize; // Account for "  " prefix and borders
        let bubble_content_width = ((available_width as f32 * 0.9) as usize)
            .max(40)
            .min(available_width);

        // Build lines from messages
        let mut lines: Vec<Line> = Vec::new();

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            let icon = self.role_icon(msg.role);
            let label = self.role_label(msg.role).to_string();
            let fg_color = self.role_color(msg.role);
            let _bg_color = self.role_bg_color(msg.role);

            // Check if this message is focused
            let is_focused_msg = self.focused && msg_idx == self.focused_message_index;
            let is_visual = self.focused && self.vim_mode == crate::ui_backend::VimMode::Visual;

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
                        // Unanswered: render question widget
                        lines.push(Line::from(vec![
                            Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                            Span::styled(
                                format!("{}", msg.content),
                                Style::default().fg(self.theme.text_primary),
                            ),
                        ]));
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
                                    Style::default().fg(self.theme.question_fg),
                                ),
                                Span::styled(
                                    opt.text.clone(),
                                    Style::default().fg(self.theme.text_primary),
                                ),
                            ]));
                        }
                    }
                    lines.push(Line::from(""));
                    continue;
                }
            }

            let mut spans = vec![
                Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                Span::styled(label.clone(), Style::default().fg(fg_color)),
                Span::raw(" "),
            ];

            // Add timestamp if present
            if !msg.timestamp.is_empty() {
                spans.push(Span::styled(
                    format!("{} ", msg.timestamp),
                    Style::default().fg(self.theme.text_muted),
                ));
            }

            // Add cursor indicator for focused message
            if is_focused_msg {
                let cursor_visible = get_message_cursor_visible();
                let cursor = if cursor_visible {
                    Span::styled("▮ ", Style::default().fg(self.theme.text_primary))
                } else {
                    Span::raw("  ")
                };
                spans.insert(0, cursor);
            } else if self.focused {
                spans.insert(0, Span::raw("  "));
            }

            // Standard message rendering (non-bubble)
            if msg.role == MessageRole::System {
                let mut system_spans = spans.clone();
                system_spans.push(Span::styled(
                    msg.content.clone(),
                    Style::default().fg(self.theme.text_primary),
                ));
                lines.push(Line::from(system_spans));
                lines.push(Line::from(""));
                continue;
            }

            // Bubble rendering for user/assistant
            if msg.role == MessageRole::User || msg.role == MessageRole::Agent {
                let glow_color = if msg.role == MessageRole::User {
                    self.theme.user_bubble
                } else {
                    self.theme.agent_bubble
                };

                let content = if msg.collapsed {
                    "..."
                } else {
                    msg.content.as_str()
                };

                // Header line
                let mut header_spans = spans.clone();
                if msg.role == MessageRole::User {
                    header_spans.push(Span::styled(
                        self.agent_name.to_string(),
                        Style::default().fg(self.theme.text_muted),
                    ));
                } else {
                    header_spans.push(Span::styled(
                        "Assistant",
                        Style::default().fg(self.theme.text_muted),
                    ));
                }
                lines.push(Line::from(header_spans));

                // Top border
                let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(top_border, Style::default().fg(glow_color)),
                ]));

                // Render streaming content as markdown (per chunk)
                let markdown_lines =
                    super::markdown::render_markdown(content, self.theme, bubble_content_width - 2);
                for md_line in markdown_lines {
                    let line_width = md_line.width();
                    let padding = bubble_content_width.saturating_sub(2 + line_width);
                    let mut line_spans = vec![
                        Span::raw("  "),
                        Span::styled("│", Style::default().fg(glow_color)),
                        Span::raw(" "),
                    ];
                    line_spans.extend(md_line.spans);
                    line_spans.push(Span::raw(" ".repeat(padding + 1)));
                    line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                    lines.push(Line::from(line_spans));
                }

                // Bottom border
                let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(bottom_border, Style::default().fg(glow_color)),
                ]));
                lines.push(Line::from(""));
                continue;
            }

            // Non-bubble message types
            let mut content_spans = spans.clone();

            if is_visual && is_focused_msg {
                content_spans.push(Span::styled(
                    msg.content.clone(),
                    Style::default()
                        .fg(self.theme.text_primary)
                        .bg(self.theme.bg_code),
                ));
            } else {
                // Calculate prefix width for indentation
                let mut prefix_width = 0;
                if is_focused_msg {
                    prefix_width += 2; // "▶ " or "  "
                }
                prefix_width += icon.chars().count() + 1; // "icon "
                prefix_width += label.chars().count();
                prefix_width += 1; // " "
                if !msg.timestamp.is_empty() {
                    prefix_width += msg.timestamp.chars().count() + 1; // "time "
                }

                let content_width = bubble_content_width.saturating_sub(prefix_width).max(20);
                let wrapped_content = wrap_text(&msg.content, content_width);

                for (i, line_content) in wrapped_content.into_iter().enumerate() {
                    if i == 0 {
                        let mut first_line_spans = content_spans.clone();
                        first_line_spans.push(Span::styled(
                            line_content,
                            Style::default().fg(self.theme.text_primary),
                        ));
                        lines.push(Line::from(first_line_spans));
                    } else {
                        let indented_spans = vec![
                            Span::raw(" ".repeat(prefix_width)),
                            Span::styled(
                                line_content,
                                Style::default().fg(self.theme.text_primary),
                            ),
                        ];
                        lines.push(Line::from(indented_spans));
                    }
                }
                lines.push(Line::from(""));
                continue;
            }

            lines.push(Line::from(content_spans));
            lines.push(Line::from(""));
        }

        // Add streaming content if present
        // Use pre-rendered lines if available (incremental rendering optimization),
        // otherwise fall back to parsing the raw content
        let has_streaming = self
            .streaming_content
            .as_ref()
            .is_some_and(|c| !c.is_empty())
            || self.streaming_lines.is_some();

        // Show a "thinking" placeholder while processing but no streaming content yet
        // This provides visual feedback during the waiting period before first chunk
        let show_thinking_placeholder = self.is_processing && !has_streaming;

        if show_thinking_placeholder {
            let icon = self.role_icon(MessageRole::Agent);
            let header_spans = vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(self.theme.agent_bubble),
                ),
                Span::styled("Assistant", Style::default().fg(self.theme.text_muted)),
            ];
            lines.push(Line::from(header_spans));

            let glow_color = self.theme.agent_bubble;
            let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(top_border, Style::default().fg(glow_color)),
            ]));

            // Animated thinking indicator using the blink state
            let indicator_visible = get_tool_indicator_visible();
            let thinking_text = if indicator_visible {
                "Thinking..."
            } else {
                "Thinking.  "
            };
            let padding = bubble_content_width.saturating_sub(2 + thinking_text.len());
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled("│", Style::default().fg(glow_color)),
                Span::raw(" "),
                Span::styled(thinking_text, Style::default().fg(self.theme.text_muted)),
                Span::raw(" ".repeat(padding + 1)),
                Span::styled("│", Style::default().fg(glow_color)),
            ]));

            let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(bottom_border, Style::default().fg(glow_color)),
            ]));
            lines.push(Line::from(""));
        } else if has_streaming {
            let icon = self.role_icon(MessageRole::Agent);
            let mut header_spans = vec![
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(self.theme.agent_bubble),
                ),
                Span::styled("Assistant", Style::default().fg(self.theme.text_muted)),
            ];

            if self.focused {
                let cursor_visible = get_message_cursor_visible();
                let cursor = if cursor_visible {
                    Span::styled("▮ ", Style::default().fg(self.theme.text_primary))
                } else {
                    Span::raw("  ")
                };
                header_spans.insert(0, cursor);
            } else {
                header_spans.insert(0, Span::raw("  "));
            }
            lines.push(Line::from(header_spans));

            let glow_color = self.theme.agent_bubble;
            let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(top_border, Style::default().fg(glow_color)),
            ]));

            // Use pre-rendered lines if available (incremental rendering),
            // otherwise parse the streaming content
            let markdown_lines: Vec<Line<'_>> = if let Some(ref pre_rendered) = self.streaming_lines
            {
                pre_rendered.clone()
            } else if let Some(ref content) = self.streaming_content {
                super::markdown::render_markdown(content, self.theme, bubble_content_width - 2)
            } else {
                vec![]
            };

            for md_line in markdown_lines {
                let line_width = md_line.width();
                let padding = bubble_content_width.saturating_sub(2 + line_width);
                let mut line_spans = vec![
                    Span::raw("  "),
                    Span::styled("│", Style::default().fg(glow_color)),
                    Span::raw(" "),
                ];
                line_spans.extend(md_line.spans);
                line_spans.push(Span::raw(" ".repeat(padding + 1)));
                line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                lines.push(Line::from(line_spans));
            }

            let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(bottom_border, Style::default().fg(glow_color)),
            ]));
            lines.push(Line::from(""));
        }

        // Add streaming thinking if present and thinking mode enabled
        // Use pre-rendered lines if available (incremental rendering optimization)
        let has_thinking = self
            .streaming_thinking
            .as_ref()
            .is_some_and(|t| !t.is_empty())
            || self.thinking_lines.is_some();

        if has_thinking {
            let icon = self.role_icon(MessageRole::Thinking);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(self.theme.thinking_fg),
                ),
                Span::styled("▼ Thinking ", Style::default().fg(self.theme.text_primary)),
            ]));

            // Use pre-rendered lines if available, otherwise use raw thinking content
            if let Some(ref pre_rendered) = self.thinking_lines {
                for line in pre_rendered {
                    lines.push(line.clone());
                }
            } else if let Some(ref thinking) = self.streaming_thinking {
                lines.push(Line::from(Span::styled(
                    thinking.clone(),
                    Style::default()
                        .fg(self.theme.text_secondary)
                        .bg(self.theme.thinking_bubble_bg),
                )));
            }
            lines.push(Line::from(""));
        }

        lines
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

        // Calculate dynamic bubble width: 90% of panel width (5% padding on each side)
        // Minimum 40 chars, maximum based on available width
        let available_width = inner.width.saturating_sub(4) as usize; // Account for "  " prefix and borders
        let bubble_content_width = ((available_width as f32 * 0.9) as usize)
            .max(40)
            .min(available_width);

        // Collect tool groups for collapsible group headers
        let tool_groups = collect_tool_groups(self.messages);

        // Build lines from messages
        let mut lines: Vec<Line> = Vec::new();

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            let icon = self.role_icon(msg.role);
            let label = self.role_label(msg.role);
            let fg_color = self.role_color(msg.role);
            let bg_color = self.role_bg_color(msg.role);

            // Check visual mode state
            let is_visual = self.focused && self.vim_mode == crate::ui_backend::VimMode::Visual;

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
                                    opt.text.clone(),
                                    Style::default().fg(self.theme.text_secondary),
                                ),
                            ]));
                        }
                    }
                } else {
                    // Fallback if no question widget attached
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                        Span::styled(
                            msg.content.clone(),
                            Style::default().fg(self.theme.text_primary),
                        ),
                    ]));
                }
                lines.push(Line::from(""));
                continue;
            }

            // Check if this message type is collapsible
            let is_collapsible = matches!(msg.role, MessageRole::Thinking | MessageRole::Tool);

            if is_collapsible {
                if msg.role == MessageRole::Tool {
                    // Detect if we need a separator before this tool sequence
                    let prev_role = if msg_idx > 0 {
                        Some(self.messages[msg_idx - 1].role)
                    } else {
                        None
                    };

                    // Find the group this tool belongs to
                    let tool_group = tool_groups
                        .iter()
                        .find(|g| g.tool_indices.contains(&msg_idx));

                    // Check if this is the first tool in a group with 2+ tools
                    let is_group_first = tool_group
                        .map(|g| g.tool_indices.len() >= 2 && g.tool_indices[0] == msg_idx)
                        .unwrap_or(false);

                    // Check if the group is collapsed
                    let group_is_collapsed = tool_group
                        .map(|g| self.collapsed_tool_groups.contains(&g.start_index))
                        .unwrap_or(false);

                    // Render group header if this is the first tool in a multi-tool group
                    if is_group_first {
                        if let Some(group) = tool_group {
                            // Add separator when transitioning to Tools (from User, Agent, or first message)
                            if prev_role == Some(MessageRole::Agent)
                                || prev_role == Some(MessageRole::User)
                                || (prev_role.is_none() && msg_idx == 0)
                            {
                                lines.push(Line::from("")); // Spacing before group
                            }

                            // Get color for risk level
                            let risk_color = match group.risk_group.as_str() {
                                "Exploration" => self.theme.green,
                                "Changes" => self.theme.blue,
                                "Commands" => self.theme.yellow,
                                "Destructive" => self.theme.red,
                                _ => self.theme.text_muted,
                            };

                            // Group header with collapse indicator
                            let collapse_icon = if group_is_collapsed { "▶" } else { "▼" };
                            let tool_count = group.tool_indices.len();
                            let status_text = if group.all_completed {
                                "completed"
                            } else {
                                "in progress"
                            };

                            let mut header_spans = vec![];

                            // Focus/position indicator for group header
                            // Only show cursor on group header when NOT navigating within group
                            let is_at_position = msg_idx == self.focused_message_index
                                && self.focused_sub_index.is_none();
                            if is_at_position {
                                if self.focused {
                                    // Focused panel: blinking cursor
                                    let cursor_visible = get_message_cursor_visible();
                                    if cursor_visible {
                                        header_spans.push(Span::styled(
                                            "▶ ",
                                            Style::default().fg(self.theme.cyan),
                                        ));
                                    } else {
                                        header_spans.push(Span::styled(
                                            "▷ ",
                                            Style::default().fg(self.theme.text_muted),
                                        ));
                                    }
                                } else {
                                    // Not focused: dim position indicator
                                    header_spans.push(Span::styled(
                                        "› ",
                                        Style::default().fg(self.theme.text_muted),
                                    ));
                                }
                            } else {
                                header_spans.push(Span::raw("  "));
                            }

                            header_spans.extend([
                                Span::styled(
                                    format!("{} ", collapse_icon),
                                    Style::default().fg(risk_color),
                                ),
                                Span::styled(
                                    format!("{}", group.risk_group),
                                    Style::default()
                                        .fg(risk_color)
                                        .add_modifier(ratatui::style::Modifier::BOLD),
                                ),
                                Span::styled(
                                    format!(" ({} tools)", tool_count),
                                    Style::default().fg(self.theme.text_muted),
                                ),
                                Span::styled(
                                    format!(" ─ {}", status_text),
                                    Style::default().fg(self.theme.text_muted),
                                ),
                            ]);

                            lines.push(Line::from(header_spans));
                        }
                    } else if prev_role == Some(MessageRole::Agent)
                        || prev_role == Some(MessageRole::User)
                        || (prev_role.is_none() && msg_idx == 0)
                    {
                        // Single tool (not in a multi-tool group) - add spacing
                        lines.push(Line::from("")); // Spacing
                    }

                    // Parse tool message content
                    // Format: "status|name|risk_group|content" or legacy "status|name|content"
                    let parts: Vec<&str> = msg.content.splitn(4, '|').collect();

                    let (status_icon, tool_name, content) = if parts.len() >= 4 {
                        // New format: status|name|risk_group|content
                        (parts[0], parts[1], parts[3].to_string())
                    } else if parts.len() == 3 {
                        // Legacy format: status|name|content
                        (parts[0], parts[1], parts[2].to_string())
                    } else if let Some(colon_idx) = msg.content.find(':') {
                        // Very old format: name: content
                        (
                            "🔧",
                            &msg.content[..colon_idx],
                            msg.content[colon_idx + 1..].trim().to_string(),
                        )
                    } else {
                        ("🔧", "tool", msg.content.clone())
                    };

                    // Check if tool is running
                    let is_running = status_icon == "⋯";

                    // Skip individual tool rendering if group is collapsed and tool is not running
                    if group_is_collapsed && !is_running {
                        continue;
                    }

                    // Determine tool position in group (for border characters)
                    let (is_first_in_group, is_last_in_group) = if let Some(group) = tool_group {
                        if group.tool_indices.len() >= 2 {
                            let pos = group
                                .tool_indices
                                .iter()
                                .position(|&i| i == msg_idx)
                                .unwrap_or(0);
                            (pos == 0, pos == group.tool_indices.len() - 1)
                        } else {
                            (true, true) // Single tool
                        }
                    } else {
                        (true, true) // Not in a group
                    };

                    // Determine border character
                    let border_char = if is_first_in_group && is_last_in_group {
                        "─" // Single tool
                    } else if is_first_in_group {
                        "╭" // First in group
                    } else if is_last_in_group {
                        "╰" // Last in group
                    } else {
                        "├" // Middle of group
                    };

                    // Determine colors based on status with blinking for running tools
                    let tool_visible = get_tool_indicator_visible();
                    let (status_color, status_display) = match status_icon {
                        "⋯" => {
                            // Running: blink between theme accent and muted color
                            let color = if tool_visible {
                                self.theme.yellow
                            } else {
                                self.theme.text_muted
                            };
                            let icon = if tool_visible { "●" } else { "○" };
                            (color, icon)
                        }
                        "✓" => (self.theme.green, "✓"), // Success - green tick
                        "✗" => (self.theme.red, "✗"),   // Failed - red cross
                        "?" => (self.theme.yellow, "?"), // Interrupted - yellow question mark (session was closed while running)
                        _ => (self.theme.tool_fg, "●"),  // Default
                    };

                    // Mark running tools as not collapsed so output shows during execution
                    let effective_collapsed = if is_running { false } else { msg.collapsed };

                    // Chevron for expand/collapse (shows navigation hint when focused)
                    let chevron = if effective_collapsed { "▶" } else { "▼" };

                    // Check if this tool message is at the current position
                    // If we're navigating within a group (focused_sub_index is Some),
                    // the cursor is at: group_start + sub_index
                    let is_at_position = if let Some(sub_idx) = self.focused_sub_index {
                        // In group navigation mode - cursor at group_start + sub_idx
                        msg_idx == self.focused_message_index + sub_idx
                    } else {
                        // Message level navigation - only show on group header (first tool)
                        msg_idx == self.focused_message_index && is_first_in_group
                    };

                    // Tool header: "╭ ● ▶ tool_name  description  (0.2s)"
                    let mut header_spans = vec![];

                    // Add border character for visual grouping
                    header_spans.push(Span::styled(
                        format!("{} ", border_char),
                        Style::default().fg(self.theme.text_muted),
                    ));

                    // Add focus/position indicator for tool messages
                    // Always show position, but dim when panel not focused
                    if is_at_position {
                        if self.focused {
                            // Focused panel: blinking cursor
                            let cursor_visible = get_message_cursor_visible();
                            let cursor = if cursor_visible {
                                Span::styled("▶ ", Style::default().fg(self.theme.cyan))
                            } else {
                                Span::styled("▷ ", Style::default().fg(self.theme.text_muted))
                            };
                            header_spans.push(cursor);
                        } else {
                            // Not focused: dim position indicator
                            header_spans.push(Span::styled(
                                "› ",
                                Style::default().fg(self.theme.text_muted),
                            ));
                        }
                    } else {
                        header_spans.push(Span::raw("  "));
                    }

                    // Extract elapsed time if present in format "content (Xs)" or "content (X.Ys)"
                    let (display_content, elapsed_time) =
                        if let Some(time_start) = content.rfind(" (") {
                            if content.ends_with(')') {
                                let time_part = &content[time_start..];
                                if time_part.contains("ms") || time_part.contains('s') {
                                    (
                                        content[..time_start].to_string(),
                                        Some(time_part.to_string()),
                                    )
                                } else {
                                    (content.clone(), None)
                                }
                            } else {
                                (content.clone(), None)
                            }
                        } else {
                            (content.clone(), None)
                        };

                    header_spans.extend([
                        Span::styled(
                            format!("{} ", status_display),
                            Style::default().fg(status_color),
                        ),
                        Span::styled(
                            format!("{} ", chevron),
                            Style::default().fg(self.theme.text_muted),
                        ),
                        Span::styled(
                            tool_name.to_string(),
                            Style::default()
                                .fg(self.theme.text_primary)
                                .add_modifier(ratatui::style::Modifier::BOLD),
                        ),
                        Span::raw("  "),
                        Span::styled(
                            if display_content.chars().count() > 50 && effective_collapsed {
                                let truncated: String = display_content.chars().take(47).collect();
                                format!("{}...", truncated)
                            } else if effective_collapsed {
                                display_content.clone()
                            } else {
                                String::new()
                            },
                            Style::default().fg(self.theme.text_muted),
                        ),
                    ]);

                    // Add elapsed time at the end if present
                    if let Some(time) = elapsed_time {
                        header_spans.push(Span::styled(
                            format!(" {}", time),
                            Style::default().fg(self.theme.text_muted),
                        ));
                    }
                    let header_line = Line::from(header_spans);
                    lines.push(header_line);

                    // Show content only if not collapsed
                    if !effective_collapsed {
                        // Special handling for "think" tool - render from tool_args
                        if tool_name == "think" {
                            // Parse THIS message's thought from its own args
                            if let Some(ref args) = msg.tool_args {
                                if let Ok(thought) = serde_json::from_value::<
                                    crate::tools::builtin::Thought,
                                >(args.clone())
                                {
                                    // Thought number and content
                                    lines.push(Line::from(vec![
                                        Span::styled(
                                            "│ ",
                                            Style::default().fg(self.theme.text_muted),
                                        ),
                                        Span::raw("   "),
                                        Span::styled(
                                            format!("{}. ", thought.thought_number),
                                            Style::default()
                                                .fg(self.theme.thinking_fg)
                                                .add_modifier(ratatui::style::Modifier::BOLD),
                                        ),
                                        Span::styled(
                                            thought.thought.clone(),
                                            Style::default().fg(ratatui::style::Color::Gray),
                                        ),
                                    ]));

                                    // Metadata line (thought_type and confidence)
                                    let mut has_metadata = false;
                                    let mut metadata_spans = vec![
                                        Span::styled(
                                            "│ ",
                                            Style::default().fg(self.theme.text_muted),
                                        ),
                                        Span::raw("      "),
                                    ];

                                    if let Some(ref thought_type) = thought.thought_type {
                                        has_metadata = true;
                                        let type_color = match thought_type.as_str() {
                                            "hypothesis" => ratatui::style::Color::Cyan,
                                            "analysis" => ratatui::style::Color::Blue,
                                            "plan" => ratatui::style::Color::Green,
                                            "decision" => ratatui::style::Color::Yellow,
                                            "reflection" => ratatui::style::Color::Magenta,
                                            _ => ratatui::style::Color::Gray,
                                        };
                                        metadata_spans.push(Span::styled(
                                            format!("[{}]", thought_type),
                                            Style::default()
                                                .fg(type_color)
                                                .add_modifier(ratatui::style::Modifier::ITALIC),
                                        ));
                                    }

                                    if let Some(confidence) = thought.confidence {
                                        has_metadata = true;
                                        let confidence_pct = (confidence * 100.0) as u8;
                                        let confidence_style = if confidence >= 0.8 {
                                            Style::default().fg(ratatui::style::Color::Green)
                                        } else if confidence >= 0.5 {
                                            Style::default().fg(ratatui::style::Color::Yellow)
                                        } else {
                                            Style::default().fg(ratatui::style::Color::Red)
                                        };
                                        if thought.thought_type.is_some() {
                                            metadata_spans.push(Span::raw(" "));
                                        }
                                        metadata_spans.push(Span::styled(
                                            format!("confidence: {}%", confidence_pct),
                                            confidence_style
                                                .add_modifier(ratatui::style::Modifier::DIM),
                                        ));
                                    }

                                    if has_metadata {
                                        lines.push(Line::from(metadata_spans));
                                    }

                                    // Add spacing after thought
                                    lines.push(Line::from(vec![Span::styled(
                                        "│ ",
                                        Style::default().fg(self.theme.text_muted),
                                    )]));

                                    // Show "thinking..." indicator if more thoughts are coming
                                    if thought.next_thought_needed {
                                        lines.push(Line::from(vec![
                                            Span::styled(
                                                "│ ",
                                                Style::default().fg(self.theme.text_muted),
                                            ),
                                            Span::raw("      "),
                                            Span::styled(
                                                "⋯ Thinking...",
                                                Style::default()
                                                    .fg(self.theme.thinking_fg)
                                                    .add_modifier(
                                                        ratatui::style::Modifier::DIM
                                                            | ratatui::style::Modifier::ITALIC,
                                                    ),
                                            ),
                                        ]));
                                    }
                                }
                            }
                        } else {
                            // Regular tool content rendering
                            let content_lines =
                                wrap_text(&display_content, available_width.saturating_sub(6));
                            for line in content_lines {
                                lines.push(Line::from(vec![
                                    Span::styled("│ ", Style::default().fg(self.theme.text_muted)), // Border continuation
                                    Span::raw("   "), // Indent
                                    Span::styled(
                                        line,
                                        Style::default().fg(self.theme.text_secondary),
                                    ),
                                ]));
                            }
                        }
                    }
                } else {
                    // Thinking messages - keep original collapsible behavior
                    let chevron = if msg.collapsed { "▶" } else { "▼" };
                    let header_line = Line::from(vec![
                        Span::styled(format!("{} ", icon), Style::default().fg(fg_color)),
                        Span::styled(
                            format!("{} {} ", chevron, label),
                            Style::default().fg(self.theme.text_primary),
                        ),
                    ]);
                    lines.push(header_line);

                    if !msg.collapsed {
                        lines.push(Line::from(Span::styled(
                            msg.content.clone(),
                            Style::default().fg(self.theme.text_secondary).bg(bg_color),
                        )));
                    }
                }
            } else {
                // Regular message with role icon, label, and content
                // For User and Agent messages, create a bubble effect with background
                if matches!(msg.role, MessageRole::User | MessageRole::Agent) {
                    let bg = bg_color;
                    let border_fg = self.role_border_color(msg.role);
                    let glow_color = dim_color(border_fg, 0.5);

                    // Header line: icon with label (outside bubble)
                    let mut header_spans = vec![];

                    // Add position indicator - always show, but dim when not focused
                    let is_at_position = msg_idx == self.focused_message_index;
                    if is_at_position {
                        if self.focused {
                            let cursor_visible = get_message_cursor_visible();
                            if is_visual {
                                header_spans.push(Span::styled(
                                    "▮ ",
                                    Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                                ));
                            } else if cursor_visible {
                                header_spans
                                    .push(Span::styled("▶ ", Style::default().fg(self.theme.cyan)));
                            } else {
                                header_spans.push(Span::styled(
                                    "▷ ",
                                    Style::default().fg(self.theme.text_muted),
                                ));
                            }
                        } else {
                            // Not focused: dim position indicator
                            header_spans.push(Span::styled(
                                "› ",
                                Style::default().fg(self.theme.text_muted),
                            ));
                        }
                    } else {
                        header_spans.push(Span::raw("  "));
                    }

                    header_spans.push(Span::styled(
                        format!("{} ", icon),
                        Style::default().fg(border_fg),
                    ));
                    header_spans.push(Span::styled(
                        label,
                        Style::default().fg(self.theme.text_secondary),
                    ));
                    if !msg.timestamp.is_empty() {
                        header_spans.push(Span::raw(" · "));
                        header_spans.push(Span::styled(
                            msg.timestamp.clone(),
                            Style::default().fg(self.theme.text_muted),
                        ));
                    }

                    lines.push(Line::from(header_spans));

                    // Top border with rounded corners and glow
                    let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
                    lines.push(Line::from(vec![
                        Span::raw("  "),
                        Span::styled(top_border, Style::default().fg(glow_color)),
                    ]));

                    // For Agent messages, use markdown rendering
                    if msg.role == MessageRole::Agent {
                        let markdown_lines = super::markdown::render_markdown(
                            &msg.content,
                            self.theme,
                            bubble_content_width - 2,
                        );

                        for md_line in markdown_lines {
                            let line_width = md_line.width();
                            let padding = bubble_content_width.saturating_sub(2 + line_width);
                            let mut line_spans = vec![
                                Span::raw("  "),
                                Span::styled("│", Style::default().fg(glow_color)),
                                Span::raw(" "),
                            ];
                            line_spans.extend(md_line.spans);
                            line_spans.push(Span::raw(" ".repeat(padding + 1)));
                            line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                            lines.push(Line::from(line_spans));
                        }
                    } else {
                        // Wrap content into lines that fit the bubble (for User messages)
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

                    // Add position indicator - always show, but dim when not focused
                    let is_at_position = msg_idx == self.focused_message_index;
                    if is_at_position {
                        if self.focused {
                            let cursor_visible = get_message_cursor_visible();
                            if is_visual {
                                spans.push(Span::styled(
                                    "▮ ",
                                    Style::default().fg(self.theme.bg_main).bg(self.theme.cyan),
                                ));
                            } else if cursor_visible {
                                spans
                                    .push(Span::styled("▶ ", Style::default().fg(self.theme.cyan)));
                            } else {
                                spans.push(Span::styled(
                                    "▷ ",
                                    Style::default().fg(self.theme.text_muted),
                                ));
                            }
                        } else {
                            // Not focused: dim position indicator
                            spans.push(Span::styled(
                                "› ",
                                Style::default().fg(self.theme.text_muted),
                            ));
                        }
                    } else {
                        spans.push(Span::raw("  "));
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
                    if !msg.timestamp.is_empty() {
                        spans.push(Span::styled(
                            format!("{} ", msg.timestamp),
                            Style::default().fg(self.theme.text_muted),
                        ));
                    }

                    // Calculate prefix width for indentation
                    let mut prefix_width = 2; // Always have "▶ " or "  " now
                    prefix_width += icon.chars().count() + 1; // "icon "
                    prefix_width += label.chars().count();
                    prefix_width += 1; // " "
                    if !msg.timestamp.is_empty() {
                        prefix_width += msg.timestamp.chars().count() + 1; // "time "
                    }

                    let content_width = bubble_content_width.saturating_sub(prefix_width).max(20);
                    let wrapped_content = wrap_text(&msg.content, content_width);

                    for (i, line_content) in wrapped_content.into_iter().enumerate() {
                        if i == 0 {
                            let mut first_line_spans = spans.clone();
                            first_line_spans.push(Span::styled(
                                line_content,
                                Style::default().fg(self.theme.text_primary),
                            ));
                            lines.push(Line::from(first_line_spans));
                        } else {
                            let indented_spans = vec![
                                Span::raw(" ".repeat(prefix_width)),
                                Span::styled(
                                    line_content,
                                    Style::default().fg(self.theme.text_primary),
                                ),
                            ];
                            lines.push(Line::from(indented_spans));
                        }
                    }
                }
            }

            // Add empty line between messages
            lines.push(Line::from(""));
        }

        // Add streaming thinking FIRST (before streaming content)
        // This represents reasoning from previous turn(s) that happened before current response
        let has_thinking = self
            .streaming_thinking
            .as_ref()
            .is_some_and(|t| !t.is_empty())
            || self.thinking_lines.is_some();

        if has_thinking {
            let icon = self.role_icon(MessageRole::Thinking);
            let border_fg = self.theme.thinking_fg;
            let glow_color = dim_color(border_fg, 0.5);

            // Header line
            let header_spans = vec![
                Span::styled(format!(" {} ", icon), Style::default().fg(border_fg)),
                Span::styled("Thinking", Style::default().fg(self.theme.text_secondary)),
                Span::styled(" (reasoning)", Style::default().fg(self.theme.text_muted)),
            ];
            lines.push(Line::from(header_spans));

            // Top border
            let top_border = format!("╭{}╮", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(top_border, Style::default().fg(glow_color)),
            ]));

            // Use pre-rendered lines if available, otherwise render raw thinking content
            if let Some(ref pre_rendered) = self.thinking_lines {
                for md_line in pre_rendered {
                    let line_width = md_line.width();
                    let padding = bubble_content_width.saturating_sub(2 + line_width);
                    let mut line_spans = vec![
                        Span::raw("  "),
                        Span::styled("│", Style::default().fg(glow_color)),
                        Span::raw(" "),
                    ];
                    line_spans.extend(md_line.spans.clone());
                    line_spans.push(Span::raw(" ".repeat(padding + 1)));
                    line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                    lines.push(Line::from(line_spans));
                }
            } else if let Some(ref thinking) = self.streaming_thinking {
                let markdown_lines = super::markdown::render_markdown(
                    thinking,
                    self.theme,
                    bubble_content_width - 2,
                );
                for md_line in markdown_lines {
                    let line_width = md_line.width();
                    let padding = bubble_content_width.saturating_sub(2 + line_width);
                    let mut line_spans = vec![
                        Span::raw("  "),
                        Span::styled("│", Style::default().fg(glow_color)),
                        Span::raw(" "),
                    ];
                    line_spans.extend(md_line.spans);
                    line_spans.push(Span::raw(" ".repeat(padding + 1)));
                    line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                    lines.push(Line::from(line_spans));
                }
            }

            // Bottom border
            let bottom_border = format!("╰{}╯", "─".repeat(bubble_content_width));
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(bottom_border, Style::default().fg(glow_color)),
            ]));
            lines.push(Line::from(""));
        }

        // Add streaming content if present (assistant is typing)
        if let Some(ref content) = self.streaming_content {
            if !content.is_empty() {
                let icon = self.role_icon(MessageRole::Agent);
                let label = self.role_label(MessageRole::Agent);
                let border_fg = self.role_border_color(MessageRole::Agent);
                let glow_color = dim_color(border_fg, 0.5);
                let _bg = self.role_bg_color(MessageRole::Agent);

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

                // Render streaming content as markdown (per chunk)
                let markdown_lines =
                    super::markdown::render_markdown(content, self.theme, bubble_content_width - 2);
                for md_line in markdown_lines {
                    let line_width = md_line.width();
                    let padding = bubble_content_width.saturating_sub(2 + line_width);
                    let mut line_spans = vec![
                        Span::raw("  "),
                        Span::styled("│", Style::default().fg(glow_color)),
                        Span::raw(" "),
                    ];
                    line_spans.extend(md_line.spans);
                    line_spans.push(Span::raw(" ".repeat(padding + 1)));
                    line_spans.push(Span::styled("│", Style::default().fg(glow_color)));
                    lines.push(Line::from(line_spans));
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

        // Store total line count before filtering
        let total_lines = lines.len();

        let max_offset = total_lines.saturating_sub(inner.height as usize);

        // Apply scroll offset (line-based)
        let line_offset = self.scroll_offset.min(max_offset);
        let visible_lines: Vec<Line> = lines.into_iter().skip(line_offset).collect();

        let paragraph = Paragraph::new(visible_lines);
        paragraph.render(inner, buf);

        // Always render scrollbar when content exceeds viewport
        if total_lines > inner.height as usize {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(self.theme.text_muted))
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));

            let mut scrollbar_state = ScrollbarState::new(max_offset).position(line_offset);

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

    #[test]
    fn test_agent_bubble_padding_fills_width() {
        let backend = TestBackend::new(60, 10);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let messages = vec![Message::agent("Hi")];

        terminal
            .draw(|f| {
                let area = MessageArea::new(&messages, &theme);
                f.render_widget(area, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        let area = Rect {
            x: 0,
            y: 0,
            width: 60,
            height: 10,
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default())
            .title(" Messages ");
        let inner = block.inner(area);

        let available_width = inner.width.saturating_sub(4) as usize;
        let bubble_content_width = ((available_width as f32 * 0.9) as usize)
            .max(40)
            .min(available_width);

        let expected_x = inner.x + 2 + 1 + bubble_content_width as u16;
        let expected_y = inner.y + 2;

        let cell = buffer.cell((expected_x, expected_y)).unwrap();
        assert_eq!(cell.symbol(), "│");
    }

    #[test]
    fn test_message_area_scroll_clamps_to_end() {
        let backend = TestBackend::new(50, 8);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        let long_text = "This is a long line that should wrap across multiple lines.";
        let messages = vec![Message::agent(long_text)];

        terminal
            .draw(|f| {
                let area = MessageArea::new(&messages, &theme).scroll(usize::MAX);
                f.render_widget(area, f.area());
            })
            .unwrap();
    }
}
