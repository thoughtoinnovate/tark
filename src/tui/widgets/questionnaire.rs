//! Questionnaire widget for user interaction
//!
//! Renders a modal popup for answering structured questions from the agent.
//! Supports single-select, multi-select, and free-text question types.

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};
use std::collections::{HashMap, HashSet};
use tokio::sync::oneshot;

use crate::tools::questionnaire::{
    AnswerValue, OptionItem, Question, QuestionType, Questionnaire, UserResponse,
};

// ============================================================================
// Questionnaire State
// ============================================================================

/// State for the active questionnaire popup
#[derive(Debug, Default)]
pub struct QuestionnaireState {
    /// Whether the popup is visible
    active: bool,

    /// The questionnaire data
    data: Option<Questionnaire>,

    /// Current question index (0-based)
    current_question_index: usize,

    /// Cursor position within current question's options
    cursor_index: usize,

    /// Collected answers for all questions
    answers: HashMap<String, AnswerValue>,

    /// Text buffer for free-text input
    text_buffer: String,

    /// Selected options for multi-select (current question only)
    selected_options: HashSet<String>,

    /// Channel to send response back to the tool
    response_tx: Option<oneshot::Sender<UserResponse>>,

    /// Validation error message (if any)
    error_message: Option<String>,
}

impl QuestionnaireState {
    /// Create a new questionnaire state
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the questionnaire is active/visible
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Open a new questionnaire
    pub fn open(&mut self, data: Questionnaire, response_tx: oneshot::Sender<UserResponse>) {
        self.active = true;
        self.data = Some(data);
        self.current_question_index = 0;
        self.cursor_index = 0;
        self.answers.clear();
        self.text_buffer.clear();
        self.selected_options.clear();
        self.response_tx = Some(response_tx);
        self.error_message = None;

        // Initialize with defaults for current question
        self.load_current_question_state();
    }

    /// Cancel the questionnaire and close
    pub fn cancel(&mut self) {
        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(UserResponse::cancelled());
        }
        self.close();
    }

    /// Submit all answers and close
    pub fn submit(&mut self) {
        // Save current question's answer first
        self.save_current_answer();

        if let Some(tx) = self.response_tx.take() {
            let _ = tx.send(UserResponse::with_answers(self.answers.clone()));
        }
        self.close();
    }

    /// Close the popup and reset state
    fn close(&mut self) {
        self.active = false;
        self.data = None;
        self.current_question_index = 0;
        self.cursor_index = 0;
        self.answers.clear();
        self.text_buffer.clear();
        self.selected_options.clear();
        self.response_tx = None;
        self.error_message = None;
    }

    /// Get the current question (if any)
    pub fn current_question(&self) -> Option<&Question> {
        self.data
            .as_ref()
            .and_then(|d| d.questions.get(self.current_question_index))
    }

    /// Get the questionnaire data
    pub fn questionnaire(&self) -> Option<&Questionnaire> {
        self.data.as_ref()
    }

    /// Get current question index
    pub fn current_index(&self) -> usize {
        self.current_question_index
    }

    /// Get total number of questions
    pub fn total_questions(&self) -> usize {
        self.data.as_ref().map(|d| d.questions.len()).unwrap_or(0)
    }

    /// Get cursor index within current question
    pub fn cursor_index(&self) -> usize {
        self.cursor_index
    }

    /// Get the text buffer content
    pub fn text_buffer(&self) -> &str {
        &self.text_buffer
    }

    /// Check if an option is selected (for multi-select)
    pub fn is_option_selected(&self, value: &str) -> bool {
        self.selected_options.contains(value)
    }

    /// Get selected value for single-select
    pub fn single_select_value(&self) -> Option<&str> {
        if let Some(q) = self.current_question() {
            if let QuestionType::SingleSelect { options, .. } = &q.kind {
                return options.get(self.cursor_index).map(|o| o.value.as_str());
            }
        }
        None
    }

    /// Get error message
    pub fn error_message(&self) -> Option<&str> {
        self.error_message.as_deref()
    }

    /// Load state for the current question (defaults, previous answers)
    fn load_current_question_state(&mut self) {
        self.cursor_index = 0;
        self.text_buffer.clear();
        self.selected_options.clear();
        self.error_message = None;

        let Some(question) = self.current_question().cloned() else {
            return;
        };

        // Check if we have a previous answer for this question
        if let Some(answer) = self.answers.get(&question.id) {
            match (&question.kind, answer) {
                (QuestionType::SingleSelect { options, .. }, AnswerValue::Single(val)) => {
                    // Find the index of the previously selected option
                    if let Some(idx) = options.iter().position(|o| &o.value == val) {
                        self.cursor_index = idx;
                    }
                }
                (QuestionType::MultiSelect { .. }, AnswerValue::Multi(vals)) => {
                    self.selected_options = vals.iter().cloned().collect();
                }
                (QuestionType::FreeText { .. }, AnswerValue::Single(val)) => {
                    self.text_buffer = val.clone();
                }
                _ => {}
            }
        } else {
            // Apply defaults
            match &question.kind {
                QuestionType::SingleSelect { options, default } => {
                    if let Some(default_val) = default {
                        if let Some(idx) = options.iter().position(|o| &o.value == default_val) {
                            self.cursor_index = idx;
                        }
                    }
                }
                QuestionType::FreeText { default, .. } => {
                    if let Some(default_val) = default {
                        self.text_buffer = default_val.clone();
                    }
                }
                QuestionType::MultiSelect { .. } => {}
            }
        }
    }

    /// Save the current question's answer
    fn save_current_answer(&mut self) {
        let Some(question) = self.current_question().cloned() else {
            return;
        };

        let answer = match &question.kind {
            QuestionType::SingleSelect { options, .. } => options
                .get(self.cursor_index)
                .map(|o| AnswerValue::Single(o.value.clone())),
            QuestionType::MultiSelect { .. } => Some(AnswerValue::Multi(
                self.selected_options.iter().cloned().collect(),
            )),
            QuestionType::FreeText { .. } => Some(AnswerValue::Single(self.text_buffer.clone())),
        };

        if let Some(answer) = answer {
            self.answers.insert(question.id.clone(), answer);
        }
    }

    /// Validate current answer and return error message if invalid
    fn validate_current(&self) -> Option<String> {
        let question = self.current_question()?;

        match &question.kind {
            QuestionType::SingleSelect { options, .. } => {
                if options.is_empty() {
                    return Some("No options available".to_string());
                }
            }
            QuestionType::MultiSelect { validation, .. } => {
                if let Some(v) = validation {
                    let count = self.selected_options.len();
                    if let Some(min) = v.min_selections {
                        if count < min {
                            return Some(format!("Please select at least {} option(s)", min));
                        }
                    }
                    if let Some(max) = v.max_selections {
                        if count > max {
                            return Some(format!("Please select at most {} option(s)", max));
                        }
                    }
                }
            }
            QuestionType::FreeText { validation, .. } => {
                if let Some(v) = validation {
                    let len = self.text_buffer.len();
                    if v.required && self.text_buffer.trim().is_empty() {
                        return Some("This field is required".to_string());
                    }
                    if let Some(min) = v.min_length {
                        if len < min {
                            return Some(format!("Minimum {} characters required", min));
                        }
                    }
                    if let Some(max) = v.max_length {
                        if len > max {
                            return Some(format!("Maximum {} characters allowed", max));
                        }
                    }
                    if let Some(pattern) = &v.regex {
                        if let Ok(re) = regex::Regex::new(pattern) {
                            if !re.is_match(&self.text_buffer) {
                                return Some("Invalid format".to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Move to the next question or submit if on the last one
    pub fn next_question(&mut self) {
        // Validate current answer
        if let Some(error) = self.validate_current() {
            self.error_message = Some(error);
            return;
        }

        // Save current answer
        self.save_current_answer();

        // Move to next or submit
        if self.current_question_index + 1 >= self.total_questions() {
            self.submit();
        } else {
            self.current_question_index += 1;
            self.load_current_question_state();
        }
    }

    /// Move to the previous question
    pub fn prev_question(&mut self) {
        if self.current_question_index > 0 {
            self.save_current_answer();
            self.current_question_index -= 1;
            self.load_current_question_state();
        }
    }

    /// Move cursor up within current question
    pub fn cursor_up(&mut self) {
        let max = self.current_option_count();
        if max > 0 && self.cursor_index > 0 {
            self.cursor_index -= 1;
            self.error_message = None;
        }
    }

    /// Move cursor down within current question
    pub fn cursor_down(&mut self) {
        let max = self.current_option_count();
        if max > 0 && self.cursor_index + 1 < max {
            self.cursor_index += 1;
            self.error_message = None;
        }
    }

    /// Get the number of options in the current question
    fn current_option_count(&self) -> usize {
        self.current_question()
            .map(|q| match &q.kind {
                QuestionType::SingleSelect { options, .. } => options.len(),
                QuestionType::MultiSelect { options, .. } => options.len(),
                QuestionType::FreeText { .. } => 0,
            })
            .unwrap_or(0)
    }

    /// Toggle selection for multi-select
    pub fn toggle_selection(&mut self) {
        // Extract the option value to toggle without borrowing self
        let option_value = {
            let Some(question) = self.current_question() else {
                return;
            };

            if let QuestionType::MultiSelect { options, .. } = &question.kind {
                options.get(self.cursor_index).map(|o| o.value.clone())
            } else {
                None
            }
        };

        // Now toggle the selection
        if let Some(value) = option_value {
            if self.selected_options.contains(&value) {
                self.selected_options.remove(&value);
            } else {
                self.selected_options.insert(value);
            }
            self.error_message = None;
        }
    }

    /// Handle text input for free-text questions
    pub fn handle_char(&mut self, c: char) {
        let Some(question) = self.current_question() else {
            return;
        };

        if matches!(question.kind, QuestionType::FreeText { .. }) {
            self.text_buffer.push(c);
            self.error_message = None;
        }
    }

    /// Handle backspace for free-text questions
    pub fn handle_backspace(&mut self) {
        let Some(question) = self.current_question() else {
            return;
        };

        if matches!(question.kind, QuestionType::FreeText { .. }) {
            self.text_buffer.pop();
            self.error_message = None;
        }
    }

    /// Handle a key event
    ///
    /// Returns true if the event was consumed
    ///
    /// Key handling differs based on question type:
    /// - Free-text: ALL character keys go to text input (no vim bindings)
    /// - Single/Multi-select: j/k navigate, space toggles/selects
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.active {
            return false;
        }

        // Check if current question is free-text (changes key handling)
        let is_free_text = self
            .current_question()
            .map(|q| matches!(q.kind, QuestionType::FreeText { .. }))
            .unwrap_or(false);

        // For free-text questions, handle character input FIRST
        // This allows typing j, k, g, l, etc. without vim interference
        if is_free_text {
            match key.code {
                KeyCode::Esc => {
                    self.cancel();
                    return true;
                }
                KeyCode::Enter => {
                    self.next_question();
                    return true;
                }
                KeyCode::Tab => {
                    if key.modifiers.contains(KeyModifiers::SHIFT) {
                        self.prev_question();
                    } else {
                        self.next_question();
                    }
                    return true;
                }
                KeyCode::BackTab => {
                    self.prev_question();
                    return true;
                }
                KeyCode::Backspace => {
                    self.handle_backspace();
                    return true;
                }
                KeyCode::Left => {
                    // Could add cursor movement within text in the future
                    return true;
                }
                KeyCode::Right => {
                    // Could add cursor movement within text in the future
                    return true;
                }
                KeyCode::Char(c) => {
                    // ALL characters go to text input for free-text
                    self.handle_char(c);
                    return true;
                }
                _ => return true,
            }
        }

        // For single-select and multi-select questions
        match key.code {
            KeyCode::Esc => {
                self.cancel();
                true
            }
            KeyCode::Enter => {
                self.next_question();
                true
            }
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.prev_question();
                } else {
                    self.next_question();
                }
                true
            }
            KeyCode::BackTab => {
                self.prev_question();
                true
            }
            // Vim-style navigation (only for select questions)
            KeyCode::Up | KeyCode::Char('k') => {
                self.cursor_up();
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cursor_down();
                true
            }
            KeyCode::Char(' ') => {
                if let Some(q) = self.current_question() {
                    match q.kind {
                        QuestionType::MultiSelect { .. } => {
                            self.toggle_selection();
                        }
                        QuestionType::SingleSelect { .. } => {
                            // Space selects and proceeds
                            self.next_question();
                        }
                        QuestionType::FreeText { .. } => {
                            // Handled above, but just in case
                            self.handle_char(' ');
                        }
                    }
                }
                true
            }
            KeyCode::Char('g') => {
                // Go to first option (like vim gg)
                self.cursor_index = 0;
                self.error_message = None;
                true
            }
            KeyCode::Char('G') => {
                // Go to last option (like vim G)
                let max = self.current_option_count();
                if max > 0 {
                    self.cursor_index = max - 1;
                    self.error_message = None;
                }
                true
            }
            _ => true, // Consume all other keys when active
        }
    }
}

// ============================================================================
// Questionnaire Widget
// ============================================================================

/// Widget for rendering the questionnaire popup
pub struct QuestionnaireWidget<'a> {
    state: &'a QuestionnaireState,
}

impl<'a> QuestionnaireWidget<'a> {
    /// Create a new questionnaire widget
    pub fn new(state: &'a QuestionnaireState) -> Self {
        Self { state }
    }

    /// Calculate the popup area (centered)
    fn calculate_area(&self, area: Rect) -> Rect {
        let width = (area.width * 60 / 100).clamp(40, 70);
        let height = (area.height * 70 / 100).clamp(12, 25);

        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        Rect::new(x, y, width, height)
    }

    /// Render options for single-select
    fn render_single_select(&self, options: &[OptionItem], inner: Rect, buf: &mut Buffer) {
        let mut y = inner.y;

        for (i, option) in options.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            let is_selected = i == self.state.cursor_index;

            // Cursor indicator
            let cursor = if is_selected { "▶ " } else { "  " };

            // Radio button
            let radio = if is_selected { "(●) " } else { "( ) " };

            let line = format!("{}{}{}", cursor, radio, option.label);

            let style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(inner.x, y, &line, style);
            y += 1;
        }
    }

    /// Render options for multi-select
    fn render_multi_select(&self, options: &[OptionItem], inner: Rect, buf: &mut Buffer) {
        let mut y = inner.y;

        for (i, option) in options.iter().enumerate() {
            if y >= inner.y + inner.height {
                break;
            }

            let is_cursor = i == self.state.cursor_index;
            let is_checked = self.state.is_option_selected(&option.value);

            // Cursor indicator
            let cursor = if is_cursor { "▶ " } else { "  " };

            // Checkbox
            let checkbox = if is_checked { "[x] " } else { "[ ] " };

            let line = format!("{}{}{}", cursor, checkbox, option.label);

            let style = if is_cursor {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else if is_checked {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::White)
            };

            buf.set_string(inner.x, y, &line, style);
            y += 1;
        }
    }

    /// Render free-text input
    fn render_free_text(&self, placeholder: Option<&str>, inner: Rect, buf: &mut Buffer) {
        let text = self.state.text_buffer();

        let display_text = if text.is_empty() {
            placeholder.unwrap_or("Type your answer...")
        } else {
            text
        };

        let style = if text.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        // Draw input box
        let input_line = format!("▶ {}_", display_text);
        buf.set_string(inner.x, inner.y, &input_line, style);
    }

    /// Render keybind hints
    fn render_hints(&self, question: &Question, area: Rect, buf: &mut Buffer) {
        let hints = match &question.kind {
            QuestionType::SingleSelect { .. } => {
                "[j/k/↑↓] Navigate  [g/G] First/Last  [Enter/Space] Select  [Esc] Cancel"
            }
            QuestionType::MultiSelect { .. } => {
                "[j/k/↑↓] Navigate  [Space] Toggle  [Enter] Next  [Esc] Cancel"
            }
            // Free-text: no vim keys mentioned since they type normally
            QuestionType::FreeText { .. } => "Type your answer  [Enter] Submit  [Esc] Cancel",
        };

        let nav_hints = if self.state.current_question_index > 0 {
            format!("[Tab] Next  [Shift+Tab] Back  {}", hints)
        } else {
            hints.to_string()
        };

        let style = Style::default().fg(Color::DarkGray);
        buf.set_string(area.x + 1, area.y, &nav_hints, style);
    }
}

impl Widget for QuestionnaireWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.is_active() {
            return;
        }

        let Some(questionnaire) = self.state.questionnaire() else {
            return;
        };

        let Some(question) = self.state.current_question() else {
            return;
        };

        let popup_area = self.calculate_area(area);

        // Clear background
        Clear.render(popup_area, buf);

        // Progress indicator
        let progress = format!(
            "[{}/{}]",
            self.state.current_index() + 1,
            self.state.total_questions()
        );

        // Title with progress
        let title = format!(" {} {} ", questionnaire.title, progress);

        // Draw border
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(popup_area);
        block.render(popup_area, buf);

        if inner.height < 4 {
            return;
        }

        // Layout: question text + options + hints
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Question text
                Constraint::Min(3),    // Options/input
                Constraint::Length(1), // Error message
                Constraint::Length(1), // Hints
            ])
            .split(inner);

        // Render question text
        let question_text = Paragraph::new(question.text.clone())
            .style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .wrap(Wrap { trim: true });
        question_text.render(chunks[0], buf);

        // Render question-specific content
        match &question.kind {
            QuestionType::SingleSelect { options, .. } => {
                self.render_single_select(options, chunks[1], buf);
            }
            QuestionType::MultiSelect { options, .. } => {
                self.render_multi_select(options, chunks[1], buf);
            }
            QuestionType::FreeText { placeholder, .. } => {
                self.render_free_text(placeholder.as_deref(), chunks[1], buf);
            }
        }

        // Render error message if any
        if let Some(error) = self.state.error_message() {
            let error_style = Style::default().fg(Color::Red);
            buf.set_string(chunks[2].x, chunks[2].y, error, error_style);
        }

        // Render hints
        self.render_hints(question, chunks[3], buf);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::questionnaire::{MultiSelectValidation, TextValidation};

    fn create_test_questionnaire() -> Questionnaire {
        Questionnaire {
            title: "Test".to_string(),
            description: None,
            questions: vec![
                Question {
                    id: "q1".to_string(),
                    text: "Single select?".to_string(),
                    kind: QuestionType::SingleSelect {
                        options: vec![
                            OptionItem {
                                value: "a".to_string(),
                                label: "Option A".to_string(),
                            },
                            OptionItem {
                                value: "b".to_string(),
                                label: "Option B".to_string(),
                            },
                        ],
                        default: None,
                    },
                },
                Question {
                    id: "q2".to_string(),
                    text: "Multi select?".to_string(),
                    kind: QuestionType::MultiSelect {
                        options: vec![
                            OptionItem {
                                value: "x".to_string(),
                                label: "Option X".to_string(),
                            },
                            OptionItem {
                                value: "y".to_string(),
                                label: "Option Y".to_string(),
                            },
                        ],
                        validation: None,
                    },
                },
                Question {
                    id: "q3".to_string(),
                    text: "Free text?".to_string(),
                    kind: QuestionType::FreeText {
                        default: None,
                        placeholder: Some("Enter text...".to_string()),
                        validation: None,
                    },
                },
            ],
            submit_label: "Submit".to_string(),
        }
    }

    #[test]
    fn test_questionnaire_state_new() {
        let state = QuestionnaireState::new();
        assert!(!state.is_active());
        assert!(state.questionnaire().is_none());
    }

    #[test]
    fn test_questionnaire_state_open() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        assert!(state.is_active());
        assert!(state.questionnaire().is_some());
        assert_eq!(state.current_index(), 0);
        assert_eq!(state.total_questions(), 3);
    }

    #[test]
    fn test_questionnaire_navigation() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // Initial state
        assert_eq!(state.cursor_index(), 0);

        // Cursor navigation
        state.cursor_down();
        assert_eq!(state.cursor_index(), 1);

        state.cursor_up();
        assert_eq!(state.cursor_index(), 0);

        // Can't go above 0
        state.cursor_up();
        assert_eq!(state.cursor_index(), 0);
    }

    #[test]
    fn test_questionnaire_next_prev_question() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        assert_eq!(state.current_index(), 0);

        state.next_question();
        assert_eq!(state.current_index(), 1);

        state.next_question();
        assert_eq!(state.current_index(), 2);

        state.prev_question();
        assert_eq!(state.current_index(), 1);
    }

    #[test]
    fn test_questionnaire_multi_select_toggle() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // Go to multi-select question
        state.next_question();
        assert_eq!(state.current_index(), 1);

        // Toggle first option
        assert!(!state.is_option_selected("x"));
        state.toggle_selection();
        assert!(state.is_option_selected("x"));

        // Toggle again
        state.toggle_selection();
        assert!(!state.is_option_selected("x"));
    }

    #[test]
    fn test_questionnaire_free_text() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // Go to free-text question
        state.next_question();
        state.next_question();
        assert_eq!(state.current_index(), 2);

        // Type some text
        state.handle_char('H');
        state.handle_char('e');
        state.handle_char('l');
        state.handle_char('l');
        state.handle_char('o');
        assert_eq!(state.text_buffer(), "Hello");

        // Backspace
        state.handle_backspace();
        assert_eq!(state.text_buffer(), "Hell");
    }

    #[test]
    fn test_questionnaire_cancel() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, mut rx) = oneshot::channel();

        state.open(q, tx);
        assert!(state.is_active());

        state.cancel();
        assert!(!state.is_active());

        // Check response
        let response = rx.try_recv().unwrap();
        assert!(response.cancelled);
    }

    #[test]
    fn test_questionnaire_validation_multi_select() {
        let mut state = QuestionnaireState::new();
        let q = Questionnaire {
            title: "Test".to_string(),
            description: None,
            questions: vec![Question {
                id: "q1".to_string(),
                text: "Select at least 1".to_string(),
                kind: QuestionType::MultiSelect {
                    options: vec![
                        OptionItem {
                            value: "a".to_string(),
                            label: "A".to_string(),
                        },
                        OptionItem {
                            value: "b".to_string(),
                            label: "B".to_string(),
                        },
                    ],
                    validation: Some(MultiSelectValidation {
                        min_selections: Some(1),
                        max_selections: None,
                    }),
                },
            }],
            submit_label: "Submit".to_string(),
        };
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // Try to proceed without selection - should show error
        state.next_question();
        assert!(state.error_message().is_some());
        assert!(state.is_active()); // Still active

        // Select one and proceed
        state.toggle_selection();
        state.next_question();
        assert!(!state.is_active()); // Submitted
    }

    #[test]
    fn test_questionnaire_validation_free_text_required() {
        let mut state = QuestionnaireState::new();
        let q = Questionnaire {
            title: "Test".to_string(),
            description: None,
            questions: vec![Question {
                id: "q1".to_string(),
                text: "Required field".to_string(),
                kind: QuestionType::FreeText {
                    default: None,
                    placeholder: None,
                    validation: Some(TextValidation {
                        required: true,
                        min_length: None,
                        max_length: None,
                        regex: None,
                    }),
                },
            }],
            submit_label: "Submit".to_string(),
        };
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // Try to proceed without text - should show error
        state.next_question();
        assert!(state.error_message().is_some());
        assert!(state.is_active());

        // Enter text and proceed
        state.handle_char('X');
        state.next_question();
        assert!(!state.is_active()); // Submitted
    }

    #[test]
    fn test_questionnaire_key_handling() {
        let mut state = QuestionnaireState::new();
        let q = create_test_questionnaire();
        let (tx, _rx) = oneshot::channel();

        state.open(q, tx);

        // j moves down
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(consumed);
        assert_eq!(state.cursor_index(), 1);

        // k moves up
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE));
        assert!(consumed);
        assert_eq!(state.cursor_index(), 0);

        // Down arrow also works
        let consumed = state.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE));
        assert!(consumed);
        assert_eq!(state.cursor_index(), 1);
    }
}
