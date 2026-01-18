//! Question Widget - Interactive questionnaires from the agent
//!
//! Reference: web/ui/mocks/src/app/components/Terminal.tsx @ratatui-behavior annotations
//! Features: 10_questions_multiple_choice, 11_questions_single_choice, 12_questions_free_text

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};
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
    /// Focused option index
    pub focused_index: usize,
    /// Free text answer (for free text questions)
    pub free_text_answer: String,
    /// Whether the question has been answered
    pub answered: bool,
}

impl QuestionWidget {
    /// Create a new multiple choice question
    pub fn multiple_choice(text: String, options: Vec<QuestionOption>) -> Self {
        Self {
            question_type: QuestionType::MultipleChoice,
            text,
            options,
            selected: HashSet::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
        }
    }

    /// Create a new single choice question
    pub fn single_choice(text: String, options: Vec<QuestionOption>) -> Self {
        Self {
            question_type: QuestionType::SingleChoice,
            text,
            options,
            selected: HashSet::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
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
        }
    }

    /// Move focus to next option
    pub fn focus_next(&mut self) {
        if self.focused_index + 1 < self.options.len() {
            self.focused_index += 1;
        }
    }

    /// Move focus to previous option
    pub fn focus_prev(&mut self) {
        if self.focused_index > 0 {
            self.focused_index -= 1;
        }
    }

    /// Toggle selection of focused option
    pub fn toggle_selection(&mut self) {
        match self.question_type {
            QuestionType::MultipleChoice => {
                if self.selected.contains(&self.focused_index) {
                    self.selected.remove(&self.focused_index);
                } else {
                    self.selected.insert(self.focused_index);
                }
            }
            QuestionType::SingleChoice => {
                self.selected.clear();
                self.selected.insert(self.focused_index);
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
            QuestionType::MultipleChoice | QuestionType::SingleChoice => self
                .selected
                .iter()
                .map(|&idx| self.options[idx].value.clone())
                .collect(),
            QuestionType::FreeText => vec![self.free_text_answer.clone()],
        }
    }

    /// Insert text (for free text questions)
    pub fn insert_text(&mut self, text: &str) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.push_str(text);
        }
    }

    /// Delete last character (for free text questions)
    pub fn backspace(&mut self) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.pop();
        }
    }
}

impl Widget for &QuestionWidget {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Question header
        let question_text = format!("❓ {}", self.text);
        let question_para =
            Paragraph::new(question_text).style(Style::default().fg(Color::Cyan).bold());

        match self.question_type {
            QuestionType::MultipleChoice | QuestionType::SingleChoice => {
                // Render options as list
                let items: Vec<ListItem> = self
                    .options
                    .iter()
                    .enumerate()
                    .map(|(idx, opt)| {
                        let checkbox = if self.selected.contains(&idx) {
                            match self.question_type {
                                QuestionType::MultipleChoice => "☑",
                                QuestionType::SingleChoice => "◉",
                                _ => " ",
                            }
                        } else {
                            match self.question_type {
                                QuestionType::MultipleChoice => "☐",
                                QuestionType::SingleChoice => "○",
                                _ => " ",
                            }
                        };

                        let marker = if idx == self.focused_index {
                            "→"
                        } else {
                            " "
                        };

                        let line = format!("{} {} {}", marker, checkbox, opt.text);
                        let style = if idx == self.focused_index {
                            Style::default().bg(Color::DarkGray)
                        } else {
                            Style::default()
                        };

                        ListItem::new(line).style(style)
                    })
                    .collect();

                // Split area for question and options
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(2), Constraint::Min(1)])
                    .split(area);

                question_para.render(chunks[0], buf);

                let list =
                    List::new(items).block(Block::default().borders(Borders::ALL).title("Options"));
                Widget::render(list, chunks[1], buf);
            }
            QuestionType::FreeText => {
                // Render text input
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(2), Constraint::Length(3)])
                    .split(area);

                question_para.render(chunks[0], buf);

                let input_para = Paragraph::new(self.free_text_answer.as_str())
                    .block(Block::default().borders(Borders::ALL).title("Your Answer"));
                input_para.render(chunks[1], buf);
            }
        }
    }
}
