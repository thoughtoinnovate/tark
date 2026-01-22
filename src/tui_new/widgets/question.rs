//! Question Widget - Interactive questionnaires from the agent
//!
//! UX Design Principles:
//! - Clear visual hierarchy: Question → Options/Input → Submit
//! - Keyboard hints in footer (like approval modal)
//! - Full-width selection highlight with ▶ marker
//! - Dynamic sizing based on content
//! - Themed colors matching the rest of the UI
//!
//! Reference: web/ui/mocks/src/app/components/Terminal.tsx @ratatui-behavior annotations
//! Features: 10_questions_multiple_choice, 11_questions_single_choice, 12_questions_free_text

use crate::tui_new::theme::Theme;
use ratatui::prelude::*;
use ratatui::symbols::border;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap};
use std::collections::HashSet;

/// Type of question being asked
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionType {
    /// Multiple choice with checkboxes (can select multiple)
    MultipleChoice,
    /// Single choice with radio buttons (select one)
    SingleChoice,
    /// Free text input
    FreeText,
}

impl QuestionType {
    /// Get the display label for this question type
    pub fn label(&self) -> &'static str {
        match self {
            QuestionType::MultipleChoice => "Multiple Choice",
            QuestionType::SingleChoice => "Single Choice",
            QuestionType::FreeText => "Free Text",
        }
    }

    /// Get the icon for this question type
    pub fn icon(&self) -> &'static str {
        match self {
            QuestionType::MultipleChoice => "☑",
            QuestionType::SingleChoice => "○",
            QuestionType::FreeText => "✎",
        }
    }
}

/// Question option
#[derive(Debug, Clone)]
pub struct QuestionOption {
    pub text: String,
    pub value: String,
}

/// Question widget for agent interactions
#[derive(Debug, Clone)]
pub struct QuestionWidget {
    /// Type of question
    pub question_type: QuestionType,
    /// Question text
    pub text: String,
    /// Available options (for multiple/single choice)
    pub options: Vec<QuestionOption>,
    /// Selected option indices (for multiple choice)
    pub selected: HashSet<usize>,
    /// Focused option index (options.len() = "Other" option)
    pub focused_index: usize,
    /// Free text answer (for free text questions)
    pub free_text_answer: String,
    /// Whether the question has been answered
    pub answered: bool,
    /// Whether to show "Other" option for choice questions
    pub allow_other: bool,
    /// Text for "Other" option when selected
    pub other_text: String,
    /// Whether "Other" is currently selected
    pub other_selected: bool,
    /// Current question index (0-based, for multi-question)
    pub current_index: usize,
    /// Total number of questions (for multi-question)
    pub total_questions: usize,
    /// Questionnaire title (for multi-question)
    pub title: String,
    /// Whether currently in free text edit mode (for FreeText questions)
    pub is_editing_free_text: bool,
    /// Whether currently in "Other" text edit mode (for choice questions)
    pub is_editing_other_text: bool,
}

impl QuestionWidget {
    /// Create a new multiple choice question (with "Other" option)
    pub fn multiple_choice(text: String, options: Vec<QuestionOption>) -> Self {
        Self {
            question_type: QuestionType::MultipleChoice,
            text,
            options,
            selected: HashSet::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            allow_other: true, // Enable "Other" by default for multiple choice
            other_text: String::new(),
            other_selected: false,
            current_index: 0,
            total_questions: 1,
            title: String::new(),
            is_editing_free_text: false,
            is_editing_other_text: false,
        }
    }

    /// Create a new single choice question (with "Other" option)
    pub fn single_choice(text: String, options: Vec<QuestionOption>) -> Self {
        Self {
            question_type: QuestionType::SingleChoice,
            text,
            options,
            selected: HashSet::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            allow_other: true, // Enable "Other" by default for single choice
            other_text: String::new(),
            other_selected: false,
            current_index: 0,
            total_questions: 1,
            title: String::new(),
            is_editing_free_text: false,
            is_editing_other_text: false,
        }
    }

    /// Create a new free text question
    pub fn free_text(text: String) -> Self {
        Self {
            question_type: QuestionType::FreeText,
            text,
            options: Vec::new(),
            selected: HashSet::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            allow_other: false,
            other_text: String::new(),
            other_selected: false,
            current_index: 0,
            total_questions: 1,
            title: String::new(),
            is_editing_free_text: false,
            is_editing_other_text: false,
        }
    }

    /// Check if this is part of a multi-question questionnaire
    pub fn is_multi_question(&self) -> bool {
        self.total_questions > 1
    }

    /// Get progress string (e.g., "1/3")
    pub fn progress(&self) -> String {
        format!("{}/{}", self.current_index + 1, self.total_questions)
    }

    /// Get total number of selectable items (options + "Other" if enabled)
    pub fn total_items(&self) -> usize {
        self.options.len() + if self.allow_other { 1 } else { 0 }
    }

    /// Check if focused on "Other" option
    pub fn is_focused_on_other(&self) -> bool {
        self.allow_other && self.focused_index == self.options.len()
    }

    /// Move focus to next option
    pub fn focus_next(&mut self) {
        if self.question_type == QuestionType::FreeText {
            return; // No navigation for free text
        }
        if self.focused_index + 1 < self.total_items() {
            self.focused_index += 1;
        }
    }

    /// Move focus to previous option
    pub fn focus_prev(&mut self) {
        if self.question_type == QuestionType::FreeText {
            return; // No navigation for free text
        }
        if self.focused_index > 0 {
            self.focused_index -= 1;
        }
    }

    /// Toggle selection of focused option
    pub fn toggle_selection(&mut self) {
        match self.question_type {
            QuestionType::MultipleChoice => {
                if self.is_focused_on_other() {
                    // Toggle "Other" selection
                    self.other_selected = !self.other_selected;
                } else if self.selected.contains(&self.focused_index) {
                    self.selected.remove(&self.focused_index);
                } else {
                    self.selected.insert(self.focused_index);
                }
            }
            QuestionType::SingleChoice => {
                self.selected.clear();
                self.other_selected = false;
                if self.is_focused_on_other() {
                    self.other_selected = true;
                } else {
                    self.selected.insert(self.focused_index);
                }
            }
            QuestionType::FreeText => {
                // Not applicable for free text
            }
        }
    }

    /// Submit the question answer
    pub fn submit(&mut self) -> Vec<String> {
        self.answered = true;
        match self.question_type {
            QuestionType::MultipleChoice | QuestionType::SingleChoice => {
                let mut values: Vec<String> = self
                    .selected
                    .iter()
                    .filter_map(|&idx| self.options.get(idx).map(|opt| opt.value.clone()))
                    .collect();
                // Add "Other" text if selected and not empty
                if self.other_selected && !self.other_text.trim().is_empty() {
                    values.push(format!("other:{}", self.other_text.trim()));
                }
                values
            }
            QuestionType::FreeText => vec![self.free_text_answer.clone()],
        }
    }

    /// Maximum words allowed in "Other" text input
    const MAX_OTHER_WORDS: usize = 10;

    /// Count words in a string
    fn word_count(s: &str) -> usize {
        s.split_whitespace().count()
    }

    /// Insert text (for free text questions or "Other" input)
    pub fn insert_text(&mut self, text: &str) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.push_str(text);
        } else if self.is_focused_on_other() && self.other_selected {
            // Allow typing in "Other" field when it's selected (max 10 words)
            let potential = format!("{}{}", self.other_text, text);
            if Self::word_count(&potential) <= Self::MAX_OTHER_WORDS {
                self.other_text.push_str(text);
            }
        }
    }

    /// Insert a single character
    pub fn insert_char(&mut self, c: char) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.push(c);
        } else if self.is_focused_on_other() && self.other_selected {
            // Check word limit for "Other" input (max 10 words)
            let potential = format!("{}{}", self.other_text, c);
            if Self::word_count(&potential) <= Self::MAX_OTHER_WORDS {
                self.other_text.push(c);
            }
        }
    }

    /// Delete last character (for free text questions or "Other" input)
    pub fn backspace(&mut self) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.pop();
        } else if self.is_focused_on_other() && self.other_selected {
            self.other_text.pop();
        }
    }

    /// Check if submit is allowed
    pub fn can_submit(&self) -> bool {
        match self.question_type {
            QuestionType::MultipleChoice => {
                !self.selected.is_empty()
                    || (self.other_selected && !self.other_text.trim().is_empty())
            }
            QuestionType::SingleChoice => {
                !self.selected.is_empty()
                    || (self.other_selected && !self.other_text.trim().is_empty())
            }
            QuestionType::FreeText => !self.free_text_answer.trim().is_empty(),
        }
    }

    /// Check if currently editing "Other" text
    /// Check if currently editing "Other" text
    /// Returns true only if focused on "Other", it's selected, AND in edit mode
    pub fn is_editing_other(&self) -> bool {
        self.is_focused_on_other() && self.other_selected && self.is_editing_other_text
    }
}

/// Themed question renderer
pub struct ThemedQuestion<'a> {
    question: &'a QuestionWidget,
    theme: &'a Theme,
}

impl<'a> ThemedQuestion<'a> {
    pub fn new(question: &'a QuestionWidget, theme: &'a Theme) -> Self {
        Self { question, theme }
    }

    /// Render an option line with full-width highlight
    fn render_option_line(
        &self,
        idx: usize,
        opt: &QuestionOption,
        is_focused: bool,
        is_selected: bool,
        width: u16,
    ) -> Line<'static> {
        // Selection indicator
        let marker = if is_focused { " ▶ " } else { "   " };

        // Checkbox/radio appearance
        let checkbox = match self.question.question_type {
            QuestionType::MultipleChoice => {
                if is_selected {
                    "☑"
                } else {
                    "☐"
                }
            }
            QuestionType::SingleChoice => {
                if is_selected {
                    "●"
                } else {
                    "○"
                }
            }
            QuestionType::FreeText => "",
        };

        // Letter prefix
        let letter = (b'a' + idx as u8) as char;
        let option_text = format!("{} {}) {}", checkbox, letter, opt.text);

        // Colors based on selection state
        let (fg, bg) = if is_focused {
            (self.theme.bg_dark, self.theme.question_fg)
        } else if is_selected {
            (self.theme.question_fg, Color::Reset)
        } else {
            (self.theme.text_primary, Color::Reset)
        };

        // Calculate padding for full-width highlight
        let content_len = marker.chars().count() + option_text.chars().count();
        let padding = " ".repeat((width as usize).saturating_sub(content_len + 1));

        Line::from(vec![
            Span::styled(
                marker.to_string(),
                Style::default()
                    .fg(if is_focused {
                        self.theme.bg_dark
                    } else {
                        self.theme.yellow
                    })
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(option_text, Style::default().fg(fg).bg(bg)),
            Span::styled(padding, Style::default().bg(bg)),
        ])
    }
}

impl Widget for ThemedQuestion<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate dynamic modal size
        let options_height = if self.question.question_type == QuestionType::FreeText {
            3 // Input box height
        } else {
            // Include "Other" option if enabled
            let other_height = if self.question.allow_other { 1 } else { 0 };
            (self.question.options.len() + other_height).max(1) as u16
        };

        let base_height: u16 = 8; // Title + question + submit + footer
        let modal_height = (base_height + options_height).min(area.height.saturating_sub(2));
        let modal_width = area.width.min(65);

        let modal_area = Rect {
            x: (area.width.saturating_sub(modal_width)) / 2,
            y: (area.height.saturating_sub(modal_height)) / 2,
            width: modal_width,
            height: modal_height,
        };

        // Clear background
        Clear.render(modal_area, buf);

        // Title with question type indicator and progress
        let title_text = if self.question.is_multi_question() {
            // Show progress for multi-question questionnaires
            if self.question.title.is_empty() {
                format!(
                    "❓ {} ({})",
                    self.question.question_type.label(),
                    self.question.progress()
                )
            } else {
                format!("❓ {} ({})", self.question.title, self.question.progress())
            }
        } else {
            format!("❓ {}", self.question.question_type.label())
        };

        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title_text,
                Style::default()
                    .fg(self.theme.question_fg)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]);

        // Footer with keyboard hints based on question type and state
        let footer = if self.question.is_editing_other() {
            // When editing "Other" text, show typing hint
            Line::from(vec![
                Span::styled("Type ", Style::default().fg(self.theme.cyan)),
                Span::styled("your answer  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter ", Style::default().fg(self.theme.green)),
                Span::styled("submit  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc ", Style::default().fg(self.theme.yellow)),
                Span::styled("exit edit ", Style::default().fg(self.theme.text_muted)),
            ])
        } else if self.question.is_focused_on_other() && self.question.other_selected {
            // "Other" is selected but not in edit mode - show edit prompt
            Line::from(vec![
                Span::styled("Enter ", Style::default().fg(self.theme.green)),
                Span::styled("edit  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("↑↓ ", Style::default().fg(self.theme.yellow)),
                Span::styled("navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc ", Style::default().fg(self.theme.red)),
                Span::styled("cancel ", Style::default().fg(self.theme.text_muted)),
            ])
        } else {
            match self.question.question_type {
                QuestionType::FreeText => {
                    if self.question.is_editing_free_text {
                        // Editing mode: show typing hints
                        Line::from(vec![
                            Span::styled("Type ", Style::default().fg(self.theme.cyan)),
                            Span::styled(
                                "your answer  ",
                                Style::default().fg(self.theme.text_muted),
                            ),
                            Span::styled("Enter ", Style::default().fg(self.theme.green)),
                            Span::styled("submit  ", Style::default().fg(self.theme.text_muted)),
                            Span::styled("Esc ", Style::default().fg(self.theme.yellow)),
                            Span::styled("exit edit ", Style::default().fg(self.theme.text_muted)),
                        ])
                    } else {
                        // Not editing: show edit prompt
                        Line::from(vec![
                            Span::styled("Enter ", Style::default().fg(self.theme.green)),
                            Span::styled("edit  ", Style::default().fg(self.theme.text_muted)),
                            Span::styled("Esc ", Style::default().fg(self.theme.red)),
                            Span::styled("cancel ", Style::default().fg(self.theme.text_muted)),
                        ])
                    }
                }
                QuestionType::SingleChoice => Line::from(vec![
                    // Single choice: arrow keys auto-select (no Space needed)
                    Span::styled("↑↓ ", Style::default().fg(self.theme.yellow)),
                    Span::styled("select  ", Style::default().fg(self.theme.text_muted)),
                    Span::styled("Enter ", Style::default().fg(self.theme.green)),
                    Span::styled("submit  ", Style::default().fg(self.theme.text_muted)),
                    Span::styled("Esc ", Style::default().fg(self.theme.red)),
                    Span::styled("cancel ", Style::default().fg(self.theme.text_muted)),
                ]),
                QuestionType::MultipleChoice => Line::from(vec![
                    // Multiple choice: Space to toggle checkboxes
                    Span::styled("↑↓ ", Style::default().fg(self.theme.yellow)),
                    Span::styled("navigate  ", Style::default().fg(self.theme.text_muted)),
                    Span::styled("Space ", Style::default().fg(self.theme.cyan)),
                    Span::styled("toggle  ", Style::default().fg(self.theme.text_muted)),
                    Span::styled("Enter ", Style::default().fg(self.theme.green)),
                    Span::styled("submit  ", Style::default().fg(self.theme.text_muted)),
                    Span::styled("Esc ", Style::default().fg(self.theme.red)),
                    Span::styled("cancel ", Style::default().fg(self.theme.text_muted)),
                ]),
            }
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.question_fg))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_bottom(footer)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout: Question text, Options/Input, Submit button
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Question text
                Constraint::Min(3),    // Options or input
                Constraint::Length(2), // Submit button
            ])
            .split(inner);

        // Question text
        let question_text = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", self.question.text),
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            )),
        ])
        .wrap(Wrap { trim: true });
        question_text.render(chunks[0], buf);

        // Options or free text input
        let content_width = inner.width.saturating_sub(2);
        match self.question.question_type {
            QuestionType::MultipleChoice | QuestionType::SingleChoice => {
                let mut option_lines: Vec<Line> = Vec::new();

                // Render regular options
                for (idx, opt) in self.question.options.iter().enumerate() {
                    let is_focused = idx == self.question.focused_index;
                    let is_selected = self.question.selected.contains(&idx);
                    option_lines.push(self.render_option_line(
                        idx,
                        opt,
                        is_focused,
                        is_selected,
                        content_width,
                    ));
                }

                // Render "Other" option if enabled
                if self.question.allow_other {
                    let other_idx = self.question.options.len();
                    let is_focused = self.question.is_focused_on_other();
                    let is_selected = self.question.other_selected;

                    // Selection indicator
                    let marker = if is_focused { " ▶ " } else { "   " };

                    // Checkbox/radio appearance
                    let checkbox = match self.question.question_type {
                        QuestionType::MultipleChoice => {
                            if is_selected {
                                "☑"
                            } else {
                                "☐"
                            }
                        }
                        QuestionType::SingleChoice => {
                            if is_selected {
                                "●"
                            } else {
                                "○"
                            }
                        }
                        _ => "",
                    };

                    // Letter prefix for "Other"
                    let letter = (b'a' + other_idx as u8) as char;

                    // Colors based on selection state
                    let (fg, bg) = if is_focused {
                        (self.theme.bg_dark, self.theme.question_fg)
                    } else if is_selected {
                        (self.theme.question_fg, Color::Reset)
                    } else {
                        (self.theme.text_muted, Color::Reset)
                    };

                    // Build "Other" prefix
                    let prefix = format!("{} {}) Other: ", checkbox, letter);
                    let prefix_len = marker.chars().count() + prefix.chars().count();

                    // Available width for the input text (account for cursor)
                    let input_width = (content_width as usize).saturating_sub(prefix_len + 2); // +2 for cursor and padding

                    // Check if in edit mode for "Other"
                    let is_editing_other = self.question.is_editing_other_text;

                    // Get the input text to display
                    let input_text = if is_selected && is_editing_other {
                        // In edit mode: show actual text or empty
                        if self.question.other_text.is_empty() {
                            "".to_string()
                        } else {
                            self.question.other_text.clone()
                        }
                    } else if is_selected && !self.question.other_text.is_empty() {
                        // Selected but not editing, with text: show the text (dimmed)
                        self.question.other_text.clone()
                    } else if is_selected {
                        // Selected but not editing, empty: show hint
                        "Press Enter to edit...".to_string()
                    } else {
                        "...".to_string() // Collapsed state
                    };

                    // Wrap text if needed (for display, we show it inline but truncate)
                    let display_text = if input_text.len() > input_width && input_width > 3 {
                        format!("{}...", &input_text[..input_width.saturating_sub(3)])
                    } else {
                        input_text
                    };

                    let full_content = format!("{}{}", prefix, display_text);
                    let content_len = marker.chars().count() + full_content.chars().count();
                    let padding =
                        " ".repeat((content_width as usize).saturating_sub(content_len + 1));

                    option_lines.push(Line::from(vec![
                        Span::styled(
                            marker.to_string(),
                            Style::default()
                                .fg(if is_focused {
                                    self.theme.bg_dark
                                } else {
                                    self.theme.yellow
                                })
                                .bg(bg)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(full_content, Style::default().fg(fg).bg(bg)),
                        // Add cursor only when in edit mode
                        if is_selected && is_focused && is_editing_other {
                            Span::styled(
                                "▌",
                                Style::default()
                                    .fg(if is_focused {
                                        self.theme.bg_dark
                                    } else {
                                        self.theme.question_fg
                                    })
                                    .bg(bg)
                                    .add_modifier(Modifier::SLOW_BLINK),
                            )
                        } else {
                            Span::raw("")
                        },
                        Span::styled(padding, Style::default().bg(bg)),
                    ]));
                }

                Paragraph::new(option_lines).render(chunks[1], buf);
            }
            QuestionType::FreeText => {
                // Styled input area - different appearance based on edit mode
                let is_editing = self.question.is_editing_free_text;

                let (display_text, cursor) = if is_editing {
                    // Editing mode: show answer with blinking cursor
                    let text = if self.question.free_text_answer.is_empty() {
                        Span::styled("", Style::default().fg(self.theme.text_primary))
                    } else {
                        Span::styled(
                            self.question.free_text_answer.clone(),
                            Style::default().fg(self.theme.text_primary),
                        )
                    };
                    let cursor = Span::styled(
                        "▌",
                        Style::default()
                            .fg(self.theme.question_fg)
                            .add_modifier(Modifier::SLOW_BLINK),
                    );
                    (text, cursor)
                } else {
                    // Not editing: show placeholder or existing text (dimmed), no cursor
                    let text = if self.question.free_text_answer.is_empty() {
                        Span::styled(
                            "Press Enter to edit...",
                            Style::default()
                                .fg(self.theme.text_muted)
                                .add_modifier(Modifier::ITALIC),
                        )
                    } else {
                        // Show existing text but dimmed to indicate not in edit mode
                        Span::styled(
                            self.question.free_text_answer.clone(),
                            Style::default().fg(self.theme.text_muted),
                        )
                    };
                    let cursor = Span::raw(""); // No cursor when not editing
                    (text, cursor)
                };

                let input_line = Line::from(vec![Span::raw("  "), display_text, cursor]);

                // Underline color changes based on edit mode
                let underline_color = if is_editing {
                    self.theme.question_fg
                } else {
                    self.theme.text_muted
                };

                // Simple underline-style input
                let input_para = Paragraph::new(vec![
                    Line::from(""),
                    input_line,
                    Line::from(Span::styled(
                        format!("  {}", "─".repeat((content_width - 4) as usize)),
                        Style::default().fg(underline_color),
                    )),
                ]);
                input_para.render(chunks[1], buf);
            }
        }

        // Submit button
        let can_submit = self.question.can_submit();
        let (button_fg, button_bg) = if can_submit {
            (self.theme.bg_dark, self.theme.green)
        } else {
            (self.theme.text_muted, self.theme.bg_code)
        };

        let button_text = if can_submit {
            "[ ▶ Submit ]"
        } else {
            "[ Submit ]"
        };

        // Center the button
        let button_width = button_text.len();
        let padding = (content_width as usize).saturating_sub(button_width) / 2;
        let button_line = format!("{}{}", " ".repeat(padding), button_text);

        let submit_para = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                button_line,
                Style::default()
                    .fg(button_fg)
                    .bg(button_bg)
                    .add_modifier(Modifier::BOLD),
            )),
        ]);
        submit_para.render(chunks[2], buf);
    }
}
