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
    /// Whether currently editing free text (for FreeText questions)
    /// User must press Enter to start editing, then Enter again to submit
    pub is_editing_free_text: bool,
    /// Whether currently editing "Other" text (for choice questions)
    /// User must press Enter to start editing after selecting Other, then Enter again to submit
    pub is_editing_other_text: bool,
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
            is_editing_free_text: false,
            is_editing_other_text: false,
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
            is_editing_free_text: false,
            is_editing_other_text: false,
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
            is_editing_free_text: false,
            is_editing_other_text: false,
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
    /// For FreeText questions, only works when in edit mode (is_editing_free_text is true)
    /// For "Other" option, only works when in edit mode (is_editing_other_text is true)
    pub fn insert_char(&mut self, ch: char) {
        if self.question_type == QuestionType::FreeText {
            // Only insert if in edit mode
            if self.is_editing_free_text {
                self.free_text_answer.push(ch);
            }
        } else if self.is_focused_on_other() && self.other_selected && self.is_editing_other_text {
            // Only insert if in "Other" edit mode
            self.other_text.push(ch);
        }
    }

    /// Remove last character from free text answer or "Other" text
    /// For FreeText questions, only works when in edit mode (is_editing_free_text is true)
    /// For "Other" option, only works when in edit mode (is_editing_other_text is true)
    pub fn backspace(&mut self) {
        if self.question_type == QuestionType::FreeText {
            // Only backspace if in edit mode
            if self.is_editing_free_text {
                self.free_text_answer.pop();
            }
        } else if self.is_focused_on_other() && self.other_selected && self.is_editing_other_text {
            // Only backspace if in "Other" edit mode
            self.other_text.pop();
        }
    }

    /// Check if currently editing "Other" text
    /// Returns true only if focused on "Other", it's selected, AND in edit mode
    pub fn is_editing_other(&self) -> bool {
        self.is_focused_on_other() && self.other_selected && self.is_editing_other_text
    }

    /// Start editing free text (for FreeText questions)
    /// Called when user presses Enter on a FreeText question that isn't yet in edit mode
    pub fn start_editing_free_text(&mut self) {
        if self.question_type == QuestionType::FreeText {
            self.is_editing_free_text = true;
        }
    }

    /// Stop editing free text (for FreeText questions)
    /// Called when user presses Escape while editing
    pub fn stop_editing_free_text(&mut self) {
        self.is_editing_free_text = false;
    }

    /// Check if currently in free text edit mode
    pub fn is_in_free_text_edit_mode(&self) -> bool {
        self.question_type == QuestionType::FreeText && self.is_editing_free_text
    }

    /// Start editing "Other" text (for choice questions)
    /// Called when user presses Enter on "Other" option that is selected but not in edit mode
    pub fn start_editing_other_text(&mut self) {
        if self.is_focused_on_other() && self.other_selected {
            self.is_editing_other_text = true;
        }
    }

    /// Stop editing "Other" text (for choice questions)
    /// Called when user presses Escape while editing "Other" text
    pub fn stop_editing_other_text(&mut self) {
        self.is_editing_other_text = false;
    }

    /// Check if currently in "Other" text edit mode
    pub fn is_in_other_edit_mode(&self) -> bool {
        self.is_focused_on_other() && self.other_selected && self.is_editing_other_text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test: start_editing_free_text only works for FreeText questions
    #[test]
    fn test_start_editing_free_text_only_for_freetext() {
        // FreeText question should enter edit mode
        let mut q = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::FreeText,
            vec![],
        );
        assert!(!q.is_editing_free_text);
        q.start_editing_free_text();
        assert!(q.is_editing_free_text);

        // SingleChoice question should NOT enter edit mode
        let mut q2 = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::SingleChoice,
            vec![],
        );
        assert!(!q2.is_editing_free_text);
        q2.start_editing_free_text();
        assert!(!q2.is_editing_free_text); // Should still be false
    }

    /// Test: stop_editing_free_text resets edit mode
    #[test]
    fn test_stop_editing_free_text() {
        let mut q = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::FreeText,
            vec![],
        );
        q.start_editing_free_text();
        assert!(q.is_editing_free_text);
        q.stop_editing_free_text();
        assert!(!q.is_editing_free_text);
    }

    /// Test: is_in_free_text_edit_mode checks both conditions
    #[test]
    fn test_is_in_free_text_edit_mode() {
        let mut q = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::FreeText,
            vec![],
        );
        assert!(!q.is_in_free_text_edit_mode()); // Not editing yet
        q.start_editing_free_text();
        assert!(q.is_in_free_text_edit_mode()); // Now editing

        // SingleChoice should never be in free text edit mode
        let mut q2 = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::SingleChoice,
            vec![],
        );
        q2.is_editing_free_text = true; // Force the flag
        assert!(!q2.is_in_free_text_edit_mode()); // Still false because not FreeText
    }

    /// Test: insert_char only works when in edit mode for FreeText
    #[test]
    fn test_insert_char_requires_edit_mode() {
        let mut q = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::FreeText,
            vec![],
        );

        // Not in edit mode - insert should do nothing
        q.insert_char('a');
        assert_eq!(q.free_text_answer, "");

        // Enter edit mode
        q.start_editing_free_text();
        q.insert_char('a');
        q.insert_char('b');
        assert_eq!(q.free_text_answer, "ab");
    }

    /// Test: backspace only works when in edit mode for FreeText
    #[test]
    fn test_backspace_requires_edit_mode() {
        let mut q = QuestionnaireState::new(
            "test".to_string(),
            "Question?".to_string(),
            QuestionType::FreeText,
            vec![],
        );

        // Pre-populate some text (simulating previous edit)
        q.is_editing_free_text = true;
        q.insert_char('a');
        q.insert_char('b');
        q.is_editing_free_text = false;

        // Not in edit mode - backspace should do nothing
        q.backspace();
        assert_eq!(q.free_text_answer, "ab");

        // Enter edit mode
        q.start_editing_free_text();
        q.backspace();
        assert_eq!(q.free_text_answer, "a");
    }

    // ========== "Other" Edit Mode Tests ==========

    /// Helper to create a MultipleChoice question with "Other" option
    fn create_multi_choice_with_other() -> QuestionnaireState {
        QuestionnaireState::new(
            "test".to_string(),
            "Select options".to_string(),
            QuestionType::MultipleChoice,
            vec![
                QuestionOption {
                    text: "Option A".to_string(),
                    value: "a".to_string(),
                },
                QuestionOption {
                    text: "Option B".to_string(),
                    value: "b".to_string(),
                },
            ],
        )
    }

    /// Test: start_editing_other_text only works when focused on Other and it's selected
    #[test]
    fn test_start_editing_other_requires_focus_and_selection() {
        let mut q = create_multi_choice_with_other();

        // Not focused on Other, not selected - should not enter edit mode
        assert!(!q.is_editing_other_text);
        q.start_editing_other_text();
        assert!(!q.is_editing_other_text);

        // Focus on Other (index 2 for 2 options)
        q.focused_index = 2;
        assert!(q.is_focused_on_other());

        // Focused but not selected - should not enter edit mode
        q.start_editing_other_text();
        assert!(!q.is_editing_other_text);

        // Select Other
        q.other_selected = true;

        // Now focused and selected - should enter edit mode
        q.start_editing_other_text();
        assert!(q.is_editing_other_text);
    }

    /// Test: stop_editing_other_text resets edit mode
    #[test]
    fn test_stop_editing_other_text() {
        let mut q = create_multi_choice_with_other();
        q.focused_index = 2;
        q.other_selected = true;
        q.start_editing_other_text();
        assert!(q.is_editing_other_text);

        q.stop_editing_other_text();
        assert!(!q.is_editing_other_text);
    }

    /// Test: is_editing_other requires all three conditions
    #[test]
    fn test_is_editing_other_requires_all_conditions() {
        let mut q = create_multi_choice_with_other();

        // Initially false
        assert!(!q.is_editing_other());

        // Focus on Other
        q.focused_index = 2;
        assert!(!q.is_editing_other()); // Not selected, not editing

        // Select Other
        q.other_selected = true;
        assert!(!q.is_editing_other()); // Not in edit mode yet

        // Enter edit mode
        q.start_editing_other_text();
        assert!(q.is_editing_other()); // Now all conditions met
    }

    /// Test: insert_char for "Other" only works in edit mode
    #[test]
    fn test_other_insert_char_requires_edit_mode() {
        let mut q = create_multi_choice_with_other();
        q.focused_index = 2;
        q.other_selected = true;

        // Not in edit mode - insert should do nothing
        q.insert_char('x');
        assert_eq!(q.other_text, "");

        // Enter edit mode
        q.start_editing_other_text();
        q.insert_char('x');
        q.insert_char('y');
        assert_eq!(q.other_text, "xy");
    }

    /// Test: backspace for "Other" only works in edit mode
    #[test]
    fn test_other_backspace_requires_edit_mode() {
        let mut q = create_multi_choice_with_other();
        q.focused_index = 2;
        q.other_selected = true;

        // Pre-populate text
        q.is_editing_other_text = true;
        q.insert_char('a');
        q.insert_char('b');
        q.is_editing_other_text = false;

        // Not in edit mode - backspace should do nothing
        q.backspace();
        assert_eq!(q.other_text, "ab");

        // Enter edit mode
        q.start_editing_other_text();
        q.backspace();
        assert_eq!(q.other_text, "a");
    }

    /// Test: is_in_other_edit_mode helper
    #[test]
    fn test_is_in_other_edit_mode() {
        let mut q = create_multi_choice_with_other();

        assert!(!q.is_in_other_edit_mode());

        q.focused_index = 2;
        q.other_selected = true;
        assert!(!q.is_in_other_edit_mode());

        q.start_editing_other_text();
        assert!(q.is_in_other_edit_mode());
    }
}
