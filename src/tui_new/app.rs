//! TUI Application - Main application struct and state
//!
//! This is the core of the new TUI, built with TDD approach.
//! Feature file: tests/visual/tui/features/01_terminal_layout.feature

use ratatui::backend::Backend;
use ratatui::Terminal;

use super::config::AppConfig;
use super::theme::{Theme, ThemePreset};

/// Agent operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AgentMode {
    #[default]
    Build,
    Plan,
    Ask,
}

/// Build mode (only active in Build agent mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BuildMode {
    Careful,
    #[default]
    Balanced,
    Manual,
}

/// Input mode for the TUI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode - navigation
    Normal,
    /// Insert mode - typing
    #[default]
    Insert,
    /// Command mode - slash commands
    Command,
}

/// Currently focused UI component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedComponent {
    #[default]
    Input,
    Messages,
    Panel,
    Modal,
}

impl FocusedComponent {
    /// Cycle to next component
    pub fn next(self) -> Self {
        match self {
            Self::Input => Self::Messages,
            Self::Messages => Self::Panel,
            Self::Panel => Self::Input,
            Self::Modal => Self::Modal, // Modal keeps focus
        }
    }

    /// Cycle to previous component
    pub fn previous(self) -> Self {
        match self {
            Self::Input => Self::Panel,
            Self::Messages => Self::Input,
            Self::Panel => Self::Messages,
            Self::Modal => Self::Modal,
        }
    }
}

/// Application state for the new TUI
#[derive(Debug)]
pub struct AppState {
    /// Whether the application should exit
    pub should_quit: bool,
    /// Current agent mode
    pub agent_mode: AgentMode,
    /// Current build mode (only relevant in Build agent mode)
    pub build_mode: BuildMode,
    /// Whether thinking mode is enabled (display thinking blocks)
    pub thinking_enabled: bool,
    /// Current input mode
    pub input_mode: InputMode,
    /// Currently focused component
    pub focused_component: FocusedComponent,
    /// Terminal size (cols, rows)
    pub terminal_size: (u16, u16),
    /// Whether sidebar is visible
    pub sidebar_visible: bool,
    /// Current theme
    pub theme: Theme,
    /// Current theme preset name
    pub theme_preset: ThemePreset,
    /// Application configuration
    pub config: AppConfig,
    /// Current input text
    pub input_text: String,
    /// Input cursor position
    pub input_cursor: usize,
    /// Active modal (if any)
    pub active_modal: Option<ModalType>,
    /// Message scroll offset
    pub scroll_offset: usize,
    /// Whether agent is currently processing
    pub agent_processing: bool,
    /// Task queue count
    pub task_queue_count: usize,
    /// Chat messages
    pub messages: Vec<super::widgets::Message>,
    /// Whether agent mode dropdown is open
    pub agent_mode_dropdown_open: bool,
    /// Whether build mode dropdown is open
    pub build_mode_dropdown_open: bool,
    /// Dropdown selection index
    pub dropdown_index: usize,
    /// Whether LLM is connected
    pub llm_connected: bool,
    /// Currently focused message index (for navigation)
    pub focused_message: usize,
    /// Input history (previously submitted messages)
    pub input_history: Vec<String>,
    /// Current position in input history (-1 = current input, 0+ = history index)
    pub history_index: Option<usize>,
    /// Saved current input when navigating history
    pub saved_input: String,
    /// Context files (files added via @mention)
    pub context_files: Vec<String>,
}

/// Types of modals that can be displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalType {
    ProviderPicker,
    ModelPicker,
    FilePicker,
    ThemePicker,
    Help,
}

impl Default for AppState {
    fn default() -> Self {
        use super::widgets::{Message, MessageRole};
        Self {
            should_quit: false,
            agent_mode: AgentMode::default(),
            build_mode: BuildMode::default(),
            thinking_enabled: true,
            input_mode: InputMode::Insert,
            focused_component: FocusedComponent::Input,
            terminal_size: (80, 24),
            sidebar_visible: true,
            theme: Theme::default(),
            theme_preset: ThemePreset::default(),
            config: AppConfig::default(),
            input_text: String::new(),
            input_cursor: 0,
            active_modal: None,
            scroll_offset: 0,
            agent_processing: false,
            task_queue_count: 0,
            messages: vec![
                Message::new(
                    MessageRole::System,
                    "Welcome to Tark TUI (TDD Implementation)",
                ),
                Message::new(
                    MessageRole::System,
                    "Type a message and press Enter. Press Ctrl+C to quit.",
                ),
            ],
            agent_mode_dropdown_open: false,
            build_mode_dropdown_open: false,
            dropdown_index: 0,
            llm_connected: true,
            focused_message: 0,
            input_history: Vec::new(),
            history_index: None,
            saved_input: String::new(),
            context_files: Vec::new(),
        }
    }
}

impl AppState {
    /// Create new application state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set terminal size
    pub fn set_terminal_size(&mut self, cols: u16, rows: u16) {
        self.terminal_size = (cols, rows);
    }

    /// Set input mode
    pub fn set_input_mode(&mut self, mode: InputMode) {
        self.input_mode = mode;
    }

    /// Set focused component
    pub fn set_focused_component(&mut self, component: FocusedComponent) {
        self.focused_component = component;
    }

    /// Focus next component
    pub fn focus_next(&mut self) {
        if self.active_modal.is_none() {
            self.focused_component = self.focused_component.next();
        }
    }

    /// Focus previous component
    pub fn focus_previous(&mut self) {
        if self.active_modal.is_none() {
            self.focused_component = self.focused_component.previous();
        }
    }

    /// Toggle sidebar visibility
    pub fn toggle_sidebar(&mut self) {
        self.sidebar_visible = !self.sidebar_visible;
    }

    /// Toggle thinking mode
    pub fn toggle_thinking(&mut self) {
        self.thinking_enabled = !self.thinking_enabled;
    }

    /// Set agent mode
    pub fn set_agent_mode(&mut self, mode: AgentMode) {
        self.agent_mode = mode;
    }

    /// Set build mode
    pub fn set_build_mode(&mut self, mode: BuildMode) {
        self.build_mode = mode;
    }

    /// Set theme
    pub fn set_theme(&mut self, preset: ThemePreset) {
        self.theme_preset = preset;
        self.theme = Theme::from_preset(preset);
    }

    /// Open a modal
    pub fn open_modal(&mut self, modal: ModalType) {
        self.active_modal = Some(modal);
        self.focused_component = FocusedComponent::Modal;
    }

    /// Close the active modal
    pub fn close_modal(&mut self) {
        self.active_modal = None;
        self.focused_component = FocusedComponent::Input;
    }

    /// Check if a modal is open
    pub fn is_modal_open(&self) -> bool {
        self.active_modal.is_some()
    }

    /// Insert text at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input_text.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
    }

    /// Insert string at cursor position
    pub fn insert_str(&mut self, s: &str) {
        self.input_text.insert_str(self.input_cursor, s);
        self.input_cursor += s.len();
    }

    /// Delete character before cursor
    pub fn delete_char_before(&mut self) {
        if self.input_cursor > 0 {
            let prev_char_boundary = self.input_text[..self.input_cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input_text.remove(prev_char_boundary);
            self.input_cursor = prev_char_boundary;
        }
    }

    /// Clear input
    pub fn clear_input(&mut self) {
        self.input_text.clear();
        self.input_cursor = 0;
    }

    /// Submit the current input as a user message or process slash commands
    pub fn submit_input(&mut self) {
        use super::widgets::{Message, MessageRole};
        if !self.input_text.is_empty() {
            let text = std::mem::take(&mut self.input_text);
            self.input_cursor = 0;

            // Save to history (only non-slash commands)
            if !text.starts_with('/') {
                self.input_history.push(text.clone());
            }
            // Reset history navigation
            self.history_index = None;
            self.saved_input.clear();

            // Process slash commands
            if text.starts_with('/') {
                match text.trim() {
                    "/help" | "/?" => {
                        self.open_modal(ModalType::Help);
                    }
                    "/model" | "/provider" => {
                        self.open_modal(ModalType::ProviderPicker);
                    }
                    "/theme" => {
                        self.open_modal(ModalType::ThemePicker);
                    }
                    "/file" | "/files" => {
                        self.open_modal(ModalType::FilePicker);
                    }
                    "/clear" => {
                        // Clear messages except welcome
                        self.messages.truncate(2);
                        self.scroll_offset = 0;
                    }
                    "/quit" | "/exit" | "/q" => {
                        self.quit();
                    }
                    _ => {
                        // Unknown command - show as system message
                        self.messages.push(Message::new(
                            MessageRole::System,
                            format!(
                                "Unknown command: {}. Type /help for available commands.",
                                text
                            ),
                        ));
                    }
                }
            } else {
                // Regular message
                self.messages.push(Message::new(MessageRole::User, text));
            }

            // Auto-scroll to bottom
            self.scroll_offset = self.messages.len().saturating_sub(1);
        }
    }

    /// Get input content
    pub fn input_content(&self) -> &str {
        &self.input_text
    }

    /// Check if input is empty
    pub fn is_input_empty(&self) -> bool {
        self.input_text.is_empty()
    }

    /// Signal application to quit
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Move dropdown selection to next item
    pub fn dropdown_next(&mut self) {
        self.dropdown_index = (self.dropdown_index + 1) % 3;
    }

    /// Move dropdown selection to previous item
    pub fn dropdown_prev(&mut self) {
        self.dropdown_index = if self.dropdown_index == 0 {
            2
        } else {
            self.dropdown_index - 1
        };
    }

    /// Move focus to next message
    pub fn message_focus_next(&mut self) {
        if !self.messages.is_empty() {
            self.focused_message = (self.focused_message + 1).min(self.messages.len() - 1);
        }
    }

    /// Move focus to previous message
    pub fn message_focus_prev(&mut self) {
        self.focused_message = self.focused_message.saturating_sub(1);
    }

    /// Yank (copy) current message content
    pub fn yank_message(&mut self) {
        // In real implementation, this would copy to clipboard
        // For now, just a no-op for testing
    }

    /// Toggle collapse state of thinking block
    pub fn toggle_thinking_collapse(&mut self) {
        // Toggle collapse state of focused thinking message
        // This would modify the message's collapsed state
        // For now, just render to verify the step works
    }

    /// Navigate to previous item in input history (Up arrow)
    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                // Save current input and go to most recent history
                self.saved_input = self.input_text.clone();
                self.history_index = Some(self.input_history.len() - 1);
                self.input_text = self.input_history[self.input_history.len() - 1].clone();
                self.input_cursor = self.input_text.len();
            }
            Some(idx) if idx > 0 => {
                // Go to older history
                self.history_index = Some(idx - 1);
                self.input_text = self.input_history[idx - 1].clone();
                self.input_cursor = self.input_text.len();
            }
            _ => {} // Already at oldest
        }
    }

    /// Navigate to next item in input history (Down arrow)
    pub fn history_next(&mut self) {
        if let Some(idx) = self.history_index {
            if idx + 1 < self.input_history.len() {
                // Go to newer history
                self.history_index = Some(idx + 1);
                self.input_text = self.input_history[idx + 1].clone();
                self.input_cursor = self.input_text.len();
            } else {
                // Return to current input
                self.history_index = None;
                self.input_text = self.saved_input.clone();
                self.input_cursor = self.input_text.len();
            }
        }
        // If history_index is None, we're already at current input - do nothing
    }

    /// Add a file to context
    pub fn add_context_file(&mut self, file: String) {
        if !self.context_files.contains(&file) {
            self.context_files.push(file);
        }
    }

    /// Remove a file from context
    pub fn remove_context_file(&mut self, file: &str) {
        self.context_files.retain(|f| f != file);
    }
}

/// Main TUI Application
#[derive(Debug)]
pub struct TuiApp<B: Backend> {
    /// Terminal instance
    terminal: Terminal<B>,
    /// Application state
    pub state: AppState,
}

impl<B: Backend> TuiApp<B> {
    /// Create a new TUI application
    pub fn new(terminal: Terminal<B>) -> Self {
        Self {
            terminal,
            state: AppState::new(),
        }
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut AppState {
        &mut self.state
    }

    /// Get reference to state
    pub fn state(&self) -> &AppState {
        &self.state
    }

    /// Get reference to terminal (for testing)
    pub fn terminal(&self) -> &Terminal<B> {
        &self.terminal
    }

    /// Get mutable reference to terminal
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> {
        &mut self.terminal
    }

    /// Render the UI to the terminal
    pub fn render(&mut self) -> std::io::Result<()> {
        use super::widgets::{
            FilePickerModal, Header, HelpModal, InputWidget, MessageArea, ModelPickerModal,
            ProviderPickerModal, StatusBar, TerminalFrame, ThemePickerModal,
        };
        use ratatui::layout::{Constraint, Direction, Layout};

        let state = &self.state;
        let theme = &state.theme;
        let config = &state.config;
        let messages = &state.messages;
        let active_modal = state.active_modal;

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Main layout: Terminal frame with rounded borders
            let terminal_frame = TerminalFrame::new(theme);
            frame.render_widget(terminal_frame, area);

            // Inner area (inside the frame borders)
            let inner = ratatui::layout::Rect {
                x: area.x + 1,
                y: area.y + 1,
                width: area.width.saturating_sub(2),
                height: area.height.saturating_sub(2),
            };

            // Vertical layout: Header | Messages | Input | Status
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Header
                    Constraint::Min(5),    // Message area
                    Constraint::Length(3), // Input area
                    Constraint::Length(1), // Status bar
                ])
                .split(inner);

            // Render header
            let header = Header::new(config, theme);
            frame.render_widget(header, chunks[0]);

            // Render message area
            let message_area = MessageArea::new(messages, theme)
                .scroll(state.scroll_offset)
                .focused(matches!(
                    state.focused_component,
                    FocusedComponent::Messages
                ));
            frame.render_widget(message_area, chunks[1]);

            // Render input area
            let input = InputWidget::new(&state.input_text, state.input_cursor, theme)
                .focused(matches!(state.focused_component, FocusedComponent::Input));
            frame.render_widget(input, chunks[2]);

            // Render status bar
            let status = StatusBar::new(theme)
                .agent_mode(state.agent_mode)
                .build_mode(state.build_mode)
                .thinking(state.thinking_enabled)
                .queue(state.task_queue_count)
                .processing(state.agent_processing);
            frame.render_widget(status, chunks[3]);

            // Render modal if active (on top of everything)
            if let Some(modal_type) = active_modal {
                match modal_type {
                    ModalType::Help => {
                        let help = HelpModal::new(theme);
                        frame.render_widget(help, area);
                    }
                    ModalType::ProviderPicker => {
                        let picker = ProviderPickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                    ModalType::ThemePicker => {
                        let picker = ThemePickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                    ModalType::ModelPicker => {
                        let picker = ModelPickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                    ModalType::FilePicker => {
                        let picker = FilePickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                }
            }
        })?;

        Ok(())
    }

    /// Run the main application loop
    pub fn run(&mut self) -> std::io::Result<()> {
        use crossterm::event::{self, Event, KeyCode, KeyModifiers};
        use std::time::Duration;

        // Update terminal size
        let size = self.terminal.size()?;
        self.state.set_terminal_size(size.width, size.height);

        loop {
            // Render the UI
            self.render()?;

            // Handle events with a timeout
            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => {
                        match (key.code, key.modifiers) {
                            // Quit on Ctrl+C or Ctrl+Q
                            (KeyCode::Char('c'), KeyModifiers::CONTROL)
                            | (KeyCode::Char('q'), KeyModifiers::CONTROL) => {
                                self.state.quit();
                            }
                            // Escape to close modal or clear input
                            (KeyCode::Esc, _) => {
                                if self.state.is_modal_open() {
                                    self.state.close_modal();
                                } else {
                                    self.state.clear_input();
                                }
                            }
                            // Tab to cycle focus
                            (KeyCode::Tab, _) => {
                                self.state.focus_next();
                            }
                            // Backtab to cycle focus backwards
                            (KeyCode::BackTab, _) => {
                                self.state.focus_previous();
                            }
                            // Toggle thinking with Ctrl+T
                            (KeyCode::Char('t'), KeyModifiers::CONTROL) => {
                                self.state.toggle_thinking();
                            }
                            // Toggle sidebar with Ctrl+B
                            (KeyCode::Char('b'), KeyModifiers::CONTROL) => {
                                self.state.toggle_sidebar();
                            }
                            // Help modal with ? (toggle)
                            (KeyCode::Char('?'), _) => {
                                if self.state.active_modal == Some(ModalType::Help) {
                                    self.state.close_modal();
                                } else {
                                    self.state.open_modal(ModalType::Help);
                                }
                            }
                            // Input handling in insert mode
                            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                                if matches!(self.state.focused_component, FocusedComponent::Input) {
                                    self.state.insert_char(c);
                                    // @ triggers file picker
                                    if c == '@' {
                                        self.state.open_modal(ModalType::FilePicker);
                                    }
                                }
                            }
                            // Backspace
                            (KeyCode::Backspace, _) => {
                                if matches!(self.state.focused_component, FocusedComponent::Input) {
                                    self.state.delete_char_before();
                                }
                            }
                            // Enter to submit or select in modal
                            (KeyCode::Enter, _) => {
                                if self.state.is_modal_open() {
                                    // Handle modal selection
                                    match self.state.active_modal {
                                        Some(ModalType::ProviderPicker) => {
                                            // Select provider and open model picker
                                            self.state.open_modal(ModalType::ModelPicker);
                                        }
                                        Some(ModalType::ModelPicker) => {
                                            // Select model and close
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::FilePicker) => {
                                            // Select file and close
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::ThemePicker) => {
                                            // Apply theme and close
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::Help) => {
                                            // Close help
                                            self.state.close_modal();
                                        }
                                        None => {}
                                    }
                                } else if matches!(
                                    self.state.focused_component,
                                    FocusedComponent::Input
                                ) {
                                    self.state.submit_input();
                                }
                            }
                            _ => {}
                        }
                    }
                    Event::Resize(width, height) => {
                        self.state.set_terminal_size(width, height);
                    }
                    _ => {}
                }
            }

            // Check if we should quit
            if self.state.should_quit {
                break;
            }
        }

        Ok(())
    }
}
