//! Questionnaire types for ask_user tool support

use serde::{Deserialize, Serialize};

/// A single question's answer (for multi-question support)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsweredQuestion {
    pub question_id: String,
    pub question_text: String,
    pub answer_display: String,
    pub answer_value: serde_json::Value,
}

/// Questionnaire state for interactive questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionnaireState {
    /// Questionnaire title
    pub title: String,
    /// The question ID (used in response mapping)
    pub question_id: String,
    /// The question text
    pub question: String,
    /// Question type
    pub question_type: QuestionType,
    /// Available options (for choice questions)
    pub options: Vec<QuestionOption>,
    /// Selected option indices
    pub selected: Vec<usize>,
    /// Focused option index (options.len() = "Other" option)
    pub focused_index: usize,
    /// Free text answer (for free text questions)
    pub free_text_answer: String,
    /// Whether the question has been answered
    pub answered: bool,
    /// Tool call ID this questionnaire responds to
    pub tool_call_id: Option<String>,
    /// Whether to show "Other" option for choice questions
    pub allow_other: bool,
    /// Text entered for "Other" option
    pub other_text: String,
    /// Whether "Other" option is currently selected
    pub other_selected: bool,
    /// Index of current question (for multi-question)
    pub current_question_index: usize,
    /// Total number of questions
    pub total_questions: usize,
    /// Answers collected so far (for multi-question)
    pub collected_answers: Vec<AnsweredQuestion>,
}

/// Type of question
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuestionType {
    SingleChoice,
    MultipleChoice,
    FreeText,
}

/// A question option for choice-based questions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub text: String,
    pub value: String,
}

impl Default for QuestionnaireState {
    fn default() -> Self {
        Self {
            title: String::new(),
            question_id: String::new(),
            question: String::new(),
            question_type: QuestionType::SingleChoice,
            options: Vec::new(),
            selected: Vec::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            tool_call_id: None,
            allow_other: true, // Enable "Other" by default for choice questions
            other_text: String::new(),
            other_selected: false,
            current_question_index: 0,
            total_questions: 1,
            collected_answers: Vec::new(),
        }
    }
}

impl QuestionnaireState {
    /// Create a new questionnaire for a single question
    pub fn new(
        question_id: String,
        question: String,
        question_type: QuestionType,
        options: Vec<QuestionOption>,
    ) -> Self {
        let allow_other = !matches!(question_type, QuestionType::FreeText);
        Self {
            title: String::new(),
            question_id,
            question,
            question_type,
            options,
            selected: Vec::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            tool_call_id: None,
            allow_other,
            other_text: String::new(),
            other_selected: false,
            current_question_index: 0,
            total_questions: 1,
            collected_answers: Vec::new(),
        }
    }

    /// Create a new multi-question questionnaire
    pub fn new_multi(
        title: String,
        question_id: String,
        question: String,
        question_type: QuestionType,
        options: Vec<QuestionOption>,
        current_index: usize,
        total: usize,
    ) -> Self {
        let allow_other = !matches!(question_type, QuestionType::FreeText);
        Self {
            title,
            question_id,
            question,
            question_type,
            options,
            selected: Vec::new(),
            focused_index: 0,
            free_text_answer: String::new(),
            answered: false,
            tool_call_id: None,
            allow_other,
            other_text: String::new(),
            other_selected: false,
            current_question_index: current_index,
            total_questions: total,
            collected_answers: Vec::new(),
        }
    }

    /// Check if this is the last question
    pub fn is_last_question(&self) -> bool {
        self.current_question_index + 1 >= self.total_questions
    }

    /// Get progress string (e.g., "1/3")
    pub fn progress(&self) -> String {
        format!(
            "{}/{}",
            self.current_question_index + 1,
            self.total_questions
        )
    }

    /// Get the current answer display text
    pub fn get_answer_display(&self) -> String {
        match self.question_type {
            QuestionType::SingleChoice => {
                if self.other_selected && !self.other_text.trim().is_empty() {
                    format!("Other: {}", self.other_text.trim())
                } else {
                    self.selected
                        .first()
                        .and_then(|idx| self.options.get(*idx))
                        .map(|opt| opt.text.clone())
                        .unwrap_or_default()
                }
            }
            QuestionType::MultipleChoice => {
                let mut labels: Vec<String> = self
                    .selected
                    .iter()
                    .filter_map(|idx| self.options.get(*idx).map(|opt| opt.text.clone()))
                    .collect();
                if self.other_selected && !self.other_text.trim().is_empty() {
                    labels.push(format!("Other: {}", self.other_text.trim()));
                }
                labels.join(", ")
            }
            QuestionType::FreeText => self.free_text_answer.clone(),
        }
    }

    /// Get total number of selectable items (options + "Other" if enabled)
    pub fn total_items(&self) -> usize {
        self.options.len() + if self.allow_other { 1 } else { 0 }
    }

    /// Check if focused on "Other" option
    pub fn is_focused_on_other(&self) -> bool {
        self.allow_other && self.focused_index == self.options.len()
    }

    /// Move focus to previous option
    /// For SingleChoice: auto-selects the focused option (no Space needed)
    pub fn focus_prev(&mut self) {
        if self.focused_index > 0 {
            self.focused_index -= 1;
            // Auto-select for single choice (radio button behavior)
            if self.question_type == QuestionType::SingleChoice {
                self.auto_select_focused();
            }
        }
    }

    /// Move focus to next option
    /// For SingleChoice: auto-selects the focused option (no Space needed)
    pub fn focus_next(&mut self) {
        if self.focused_index + 1 < self.total_items() {
            self.focused_index += 1;
            // Auto-select for single choice (radio button behavior)
            if self.question_type == QuestionType::SingleChoice {
                self.auto_select_focused();
            }
        }
    }

    /// Auto-select the currently focused option (for single choice)
    fn auto_select_focused(&mut self) {
        // Don't deselect "Other" if user has typed text in it - preserve their input
        if !self.other_text.trim().is_empty() {
            // Keep other_selected true, just clear regular selection
            self.selected.clear();
            return;
        }

        self.selected.clear();
        self.other_selected = false;

        if self.is_focused_on_other() {
            self.other_selected = true;
        } else {
            self.selected = vec![self.focused_index];
        }
    }

    /// Toggle selection for the focused option
    pub fn toggle_focused(&mut self) {
        if self.is_focused_on_other() {
            // Toggle "Other" selection
            match self.question_type {
                QuestionType::SingleChoice => {
                    self.selected.clear();
                    self.other_selected = !self.other_selected;
                }
                QuestionType::MultipleChoice => {
                    self.other_selected = !self.other_selected;
                }
                _ => {}
            }
        } else {
            self.toggle_option(self.focused_index);
        }
    }

    /// Toggle selection of an option
    pub fn toggle_option(&mut self, index: usize) {
        if index >= self.options.len() {
            return;
        }

        match self.question_type {
            QuestionType::SingleChoice => {
                // Single choice: replace selection
                self.selected = vec![index];
                self.other_selected = false; // Deselect "Other"
            }
            QuestionType::MultipleChoice => {
                // Multiple choice: toggle
                if let Some(pos) = self.selected.iter().position(|&i| i == index) {
                    self.selected.remove(pos);
                } else {
                    self.selected.push(index);
                }
            }
            QuestionType::FreeText => {
                // Free text doesn't use options
            }
        }
    }

    /// Mark as answered
    pub fn mark_answered(&mut self) {
        self.answered = true;
    }

    /// Get the answer as a JSON value
    pub fn get_answer(&self) -> serde_json::Value {
        match self.question_type {
            QuestionType::SingleChoice => {
                if self.other_selected && !self.other_text.trim().is_empty() {
                    serde_json::json!(format!("other:{}", self.other_text.trim()))
                } else if let Some(&idx) = self.selected.first() {
                    serde_json::json!(self.options[idx].value)
                } else {
                    serde_json::Value::Null
                }
            }
            QuestionType::MultipleChoice => {
                let mut values: Vec<String> = self
                    .selected
                    .iter()
                    .filter_map(|&idx| self.options.get(idx).map(|o| o.value.clone()))
                    .collect();
                if self.other_selected && !self.other_text.trim().is_empty() {
                    values.push(format!("other:{}", self.other_text.trim()));
                }
                serde_json::json!(values)
            }
            QuestionType::FreeText => serde_json::json!(self.free_text_answer),
        }
    }

    /// Insert a character into free text answer or "Other" text
    pub fn insert_char(&mut self, ch: char) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.push(ch);
        } else if self.is_focused_on_other() && self.other_selected {
            self.other_text.push(ch);
        }
    }

    /// Remove last character from free text answer or "Other" text
    pub fn backspace(&mut self) {
        if self.question_type == QuestionType::FreeText {
            self.free_text_answer.pop();
        } else if self.is_focused_on_other() && self.other_selected {
            self.other_text.pop();
        }
    }

    /// Check if currently editing "Other" text
    pub fn is_editing_other(&self) -> bool {
        self.is_focused_on_other() && self.other_selected
    }
}
