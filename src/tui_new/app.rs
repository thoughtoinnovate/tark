//! TUI Application - Main application struct and state
//!
//! This is the core of the new TUI, built with TDD approach.
//! Feature file: tests/visual/tui/features/01_terminal_layout.feature

#![allow(clippy::collapsible_if)]

use ratatui::backend::Backend;
use ratatui::Terminal;
use std::path::PathBuf;

use super::config::AppConfig;
use super::theme::{Theme, ThemePreset};

// Re-export types from ui_backend
pub use crate::ui_backend::{AgentMode, BuildMode};

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

// Re-export from ui_backend
pub use crate::ui_backend::FocusedComponent;

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
    /// Theme picker selected index
    pub theme_picker_selected: usize,
    /// Theme picker search filter
    pub theme_picker_filter: String,
    /// Original theme before preview (for canceling)
    pub theme_before_preview: Option<ThemePreset>,
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
    /// Sidebar state
    pub sidebar_selected_panel: usize,
    pub sidebar_selected_item: Option<usize>,
    pub sidebar_expanded_panels: [bool; 4],
}

// Re-export from ui_backend
pub use crate::ui_backend::ModalType;

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
            theme_picker_selected: 0,
            theme_picker_filter: String::new(),
            theme_before_preview: None,
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
            sidebar_selected_panel: 0,
            sidebar_selected_item: None,
            sidebar_expanded_panels: [true, true, true, true],
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

    /// Switch to a new theme
    pub fn set_theme(&mut self, preset: ThemePreset) {
        self.theme_preset = preset;
        self.theme = Theme::from_preset(preset);
    }

    /// Cycle to next theme
    pub fn next_theme(&mut self) {
        let all_themes = ThemePreset::all();
        let current_idx = all_themes
            .iter()
            .position(|&t| t == self.theme_preset)
            .unwrap_or(0);
        let next_idx = (current_idx + 1) % all_themes.len();
        self.set_theme(all_themes[next_idx]);
    }

    /// Cycle to previous theme
    pub fn prev_theme(&mut self) {
        let all_themes = ThemePreset::all();
        let current_idx = all_themes
            .iter()
            .position(|&t| t == self.theme_preset)
            .unwrap_or(0);
        let prev_idx = if current_idx == 0 {
            all_themes.len() - 1
        } else {
            current_idx - 1
        };
        self.set_theme(all_themes[prev_idx]);
    }

    /// Preview theme (temporary, for theme picker navigation)
    pub fn preview_theme(&mut self, preset: ThemePreset) {
        self.theme = Theme::from_preset(preset);
        // Don't update theme_preset, just the visual theme
    }

    /// Apply theme permanently (called when confirming selection)
    pub fn apply_theme(&mut self, preset: ThemePreset) {
        self.theme_preset = preset;
        self.theme = Theme::from_preset(preset);
        self.theme_before_preview = None;
    }

    /// Get filtered themes based on search query
    pub fn get_filtered_themes(&self) -> Vec<ThemePreset> {
        let all_themes = ThemePreset::all();
        if self.theme_picker_filter.is_empty() {
            all_themes
        } else {
            let filter_lower = self.theme_picker_filter.to_lowercase();
            all_themes
                .into_iter()
                .filter(|t| t.display_name().to_lowercase().contains(&filter_lower))
                .collect()
        }
    }

    // ==== Sidebar Navigation Methods ====

    /// Navigate to next panel in sidebar
    pub fn sidebar_next_panel(&mut self) {
        self.sidebar_selected_panel = (self.sidebar_selected_panel + 1) % 4;
        self.sidebar_selected_item = None;
    }

    /// Navigate to previous panel in sidebar
    pub fn sidebar_prev_panel(&mut self) {
        self.sidebar_selected_panel = if self.sidebar_selected_panel == 0 {
            3
        } else {
            self.sidebar_selected_panel - 1
        };
        self.sidebar_selected_item = None;
    }

    /// Navigate to next item within current panel
    pub fn sidebar_next_item(&mut self) {
        if !self.sidebar_expanded_panels[self.sidebar_selected_panel] {
            return;
        }

        let max_items = match self.sidebar_selected_panel {
            0 => 2, // Session: cost info + tokens
            1 => self.context_files.len(),
            2 => 8, // Tasks (from mock data)
            3 => 5, // Git changes (from mock data)
            _ => 0,
        };

        if let Some(item) = self.sidebar_selected_item {
            if item + 1 < max_items {
                self.sidebar_selected_item = Some(item + 1);
            }
        } else if max_items > 0 {
            self.sidebar_selected_item = Some(0);
        }
    }

    /// Navigate to previous item within current panel
    pub fn sidebar_prev_item(&mut self) {
        if let Some(item) = self.sidebar_selected_item {
            if item > 0 {
                self.sidebar_selected_item = Some(item - 1);
            } else {
                self.sidebar_selected_item = None;
            }
        }
    }

    /// Enter into selected panel (expand and select first item)
    pub fn sidebar_enter_panel(&mut self) {
        if !self.sidebar_expanded_panels[self.sidebar_selected_panel] {
            // Expand collapsed panel
            self.sidebar_expanded_panels[self.sidebar_selected_panel] = true;
        } else if self.sidebar_selected_item.is_none() {
            // Enter into panel (select first item)
            self.sidebar_selected_item = Some(0);
        } else {
            // Toggle panel
            self.sidebar_expanded_panels[self.sidebar_selected_panel] =
                !self.sidebar_expanded_panels[self.sidebar_selected_panel];
            self.sidebar_selected_item = None;
        }
    }

    /// Exit from panel items back to panel header
    pub fn sidebar_exit_panel(&mut self) {
        if self.sidebar_selected_item.is_some() {
            self.sidebar_selected_item = None;
        } else {
            // Collapse panel
            self.sidebar_expanded_panels[self.sidebar_selected_panel] = false;
        }
    }

    /// Toggle sidebar panel expansion
    pub fn sidebar_toggle_panel(&mut self, panel_idx: usize) {
        if panel_idx < 4 {
            self.sidebar_expanded_panels[panel_idx] = !self.sidebar_expanded_panels[panel_idx];
            if !self.sidebar_expanded_panels[panel_idx] {
                self.sidebar_selected_item = None;
            }
        }
    }

    /// Set build mode
    pub fn set_build_mode(&mut self, mode: BuildMode) {
        self.build_mode = mode;
    }

    /// Open a modal
    pub fn open_modal(&mut self, modal: ModalType) {
        self.active_modal = Some(modal);
        self.focused_component = FocusedComponent::Modal;

        // Initialize modal state
        if modal == ModalType::ThemePicker {
            let all_themes = ThemePreset::all();
            self.theme_picker_selected = all_themes
                .iter()
                .position(|&t| t == self.theme_preset)
                .unwrap_or(0);
            self.theme_picker_filter.clear();
            // Save current theme for preview/cancel
            self.theme_before_preview = Some(self.theme_preset);
        }
    }

    /// Close the active modal
    pub fn close_modal(&mut self) {
        // Restore original theme if canceling theme picker
        if self.active_modal == Some(ModalType::ThemePicker) {
            if let Some(original_theme) = self.theme_before_preview {
                self.set_theme(original_theme);
            }
            self.theme_before_preview = None;
        }

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
                // Regular message - add user message
                self.messages
                    .push(Message::new(MessageRole::User, text.clone()));

                // Add simulated agent response for demo
                self.messages.push(Message::new(
                    MessageRole::Agent,
                    format!("You said: {}", text),
                ));
            }

            // Auto-scroll to bottom (scroll to show last message)
            if self.messages.len() > 5 {
                self.scroll_offset = self.messages.len().saturating_sub(5);
            } else {
                self.scroll_offset = 0;
            }
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
    /// Working directory for file operations
    working_dir: PathBuf,
    /// Current provider name
    current_provider: Option<String>,
    /// Current model name
    current_model: Option<String>,
}

impl<B: Backend> TuiApp<B> {
    /// Create a new TUI application
    pub fn new(terminal: Terminal<B>) -> Self {
        Self::with_working_dir(
            terminal,
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        )
    }

    /// Create a new TUI application with specified working directory
    pub fn with_working_dir(terminal: Terminal<B>, working_dir: PathBuf) -> Self {
        let mut state = AppState::new();
        // LLM connection will be managed by AppService
        state.llm_connected = false;

        Self {
            terminal,
            state,
            working_dir,
            current_provider: None,
            current_model: None,
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
            FilePickerModal, GitChange, GitStatus, Header, HelpModal, InputWidget, MessageArea,
            ModelPickerModal, ProviderPickerModal, SessionInfo, Sidebar, StatusBar, Task,
            TaskStatus, TerminalFrame, ThemePickerModal,
        };
        use crate::core::context_tracker::ContextBreakdown;
        use ratatui::layout::{Constraint, Direction, Layout};

        let state = &self.state;
        let theme = &state.theme;
        let config = &state.config;
        let messages = &state.messages;
        let active_modal = state.active_modal;
        let sidebar_visible = state.sidebar_visible;
        let context_files = state.context_files.clone();

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

            // Horizontal split: Main Terminal | Sidebar (when visible)
            let (main_area, sidebar_area) = if sidebar_visible && inner.width > 80 {
                // Only show sidebar if terminal is wide enough
                let horizontal_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Min(60),    // Main terminal area
                        Constraint::Length(35), // Sidebar panel
                    ])
                    .split(inner);
                (horizontal_chunks[0], Some(horizontal_chunks[1]))
            } else {
                (inner, None)
            };

            // Vertical layout: Header | Messages | Input | Status
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Header
                    Constraint::Min(5),    // Message area
                    Constraint::Length(5), // Input area (increased for multi-line)
                    Constraint::Length(1), // Status bar
                ])
                .split(main_area);

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
                .queue(if state.task_queue_count > 0 {
                    state.task_queue_count
                } else {
                    7
                }) // Show 7 for demo
                .processing(state.agent_processing);
            frame.render_widget(status, chunks[3]);

            // Render sidebar if visible
            if let Some(sidebar_rect) = sidebar_area {
                let is_sidebar_focused = state.focused_component == FocusedComponent::Panel;
                let current_theme_name = state.theme_preset.display_name().to_string();

                // Demo context breakdown
                let demo_breakdown = ContextBreakdown::new(
                    1500,    // system_prompt
                    12000,   // conversation_history
                    800,     // tool_schemas
                    500,     // attachments
                    128_000, // max_tokens
                );

                let mut sidebar = Sidebar::new(theme)
                    .visible(true)
                    .theme_name(current_theme_name)
                    .theme_preset(state.theme_preset)
                    .focused(is_sidebar_focused)
                    .selected_panel(state.sidebar_selected_panel)
                    .session_info(SessionInfo {
                        name: "Session".to_string(),
                        total_cost: 0.015,
                        model_count: 3,
                        model_costs: vec![],
                        total_tokens: 0,
                        model_tokens: vec![],
                    })
                    .context_files(context_files.clone())
                    .context_breakdown(demo_breakdown)
                    .tasks(vec![
                        Task {
                            name: "Understanding the codebase architecture".to_string(),
                            status: TaskStatus::Active,
                        },
                        Task {
                            name: "Which is the most complex component?".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Refactor the gaming class structure".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Optimize database queries".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Fix authentication bug".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Update documentation".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Review pull requests".to_string(),
                            status: TaskStatus::Queued,
                        },
                        Task {
                            name: "Implement dark mode toggle".to_string(),
                            status: TaskStatus::Queued,
                        },
                    ]);

                // Set expanded state for each panel
                sidebar.expanded_panels = state.sidebar_expanded_panels;
                sidebar.selected_item = state.sidebar_selected_item;

                let sidebar = sidebar
                    .git_changes(vec![
                        GitChange {
                            file: "src/components/Sidebar.tsx".to_string(),
                            status: GitStatus::Modified,
                            additions: 45,
                            deletions: 12,
                        },
                        GitChange {
                            file: "src/utils/helpers.ts".to_string(),
                            status: GitStatus::Added,
                            additions: 0,
                            deletions: 0,
                        },
                        GitChange {
                            file: "public/legacy-logo.svg".to_string(),
                            status: GitStatus::Deleted,
                            additions: 0,
                            deletions: 0,
                        },
                        GitChange {
                            file: "src/styles/globals.css".to_string(),
                            status: GitStatus::Modified,
                            additions: 10,
                            deletions: 5,
                        },
                        GitChange {
                            file: "README.md".to_string(),
                            status: GitStatus::Modified,
                            additions: 2,
                            deletions: 1,
                        },
                    ])
                    .git_branch("main".to_string());
                frame.render_widget(sidebar, sidebar_rect);
            }

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
                        let picker = ThemePickerModal::new(
                            theme,
                            state.theme_preset,
                            &state.theme_picker_filter,
                        )
                        .selected(state.theme_picker_selected);
                        frame.render_widget(picker, area);
                    }
                    ModalType::ModelPicker => {
                        let picker = ModelPickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                    ModalType::SessionPicker => {
                        // Session picker handled in tui_new renderer
                    }
                    ModalType::FilePicker => {
                        let picker = FilePickerModal::new(theme);
                        frame.render_widget(picker, area);
                    }
                    ModalType::Approval => {
                        // Approval modal not implemented in old app.rs (using tui_new)
                    }
                    ModalType::TrustLevel => {
                        // TrustLevel modal not implemented in old app.rs (using tui_new)
                    }
                    ModalType::Tools => {
                        // Tools modal not implemented in old app.rs (using tui_new)
                    }
                    ModalType::Plugin => {
                        // Plugin modal not implemented in old app.rs (using tui_new)
                    }
                    ModalType::DeviceFlow => {
                        // DeviceFlow modal not implemented in old app.rs (using tui_new)
                    }
                    ModalType::SessionSwitchConfirm => {
                        // SessionSwitchConfirm modal handled in tui_new renderer
                    }
                    ModalType::TaskEdit => {
                        // TaskEdit modal handled in tui_new renderer
                    }
                    ModalType::TaskDeleteConfirm => {
                        // TaskDeleteConfirm modal handled in tui_new renderer
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
                                if !self.state.is_modal_open() {
                                    self.state.focus_next();
                                }
                            }
                            // Backtab to cycle focus backwards
                            (KeyCode::BackTab, _) => {
                                if !self.state.is_modal_open() {
                                    self.state.focus_previous();
                                }
                            }
                            // Vim keys (j/k) for sidebar navigation
                            (KeyCode::Char('j'), KeyModifiers::NONE) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    if self.state.sidebar_selected_item.is_some() {
                                        // Navigate within panel
                                        self.state.sidebar_next_item();
                                    } else {
                                        // Navigate between panels
                                        self.state.sidebar_next_panel();
                                    }
                                }
                            }
                            (KeyCode::Char('k'), KeyModifiers::NONE) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    if self.state.sidebar_selected_item.is_some() {
                                        // Navigate within panel
                                        self.state.sidebar_prev_item();
                                    } else {
                                        // Navigate between panels
                                        self.state.sidebar_prev_panel();
                                    }
                                }
                            }
                            // Enter to toggle/enter panels
                            (KeyCode::Char('l'), KeyModifiers::NONE) | (KeyCode::Right, _) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    self.state.sidebar_enter_panel();
                                }
                            }
                            // Escape or h to exit panel
                            (KeyCode::Char('h'), KeyModifiers::NONE)
                            | (KeyCode::Left, _)
                            | (KeyCode::Char('-'), KeyModifiers::NONE) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    self.state.sidebar_exit_panel();
                                }
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
                                if self.state.active_modal == Some(ModalType::ThemePicker) {
                                    // Typing in theme picker search
                                    self.state.theme_picker_filter.push(c);
                                    // Reset selection when filter changes
                                    self.state.theme_picker_selected = 0;
                                } else if matches!(
                                    self.state.focused_component,
                                    FocusedComponent::Input
                                ) {
                                    self.state.insert_char(c);
                                    // @ triggers file picker
                                    if c == '@' {
                                        self.state.open_modal(ModalType::FilePicker);
                                    }
                                }
                            }
                            // Backspace
                            (KeyCode::Backspace, _) => {
                                if self.state.active_modal == Some(ModalType::ThemePicker) {
                                    // Delete from theme picker search
                                    self.state.theme_picker_filter.pop();
                                    self.state.theme_picker_selected = 0;
                                } else if matches!(
                                    self.state.focused_component,
                                    FocusedComponent::Input
                                ) {
                                    self.state.delete_char_before();
                                }
                            }
                            // Shift+Enter to insert newline in input
                            (KeyCode::Enter, KeyModifiers::SHIFT) => {
                                if matches!(self.state.focused_component, FocusedComponent::Input) {
                                    self.state.insert_char('\n');
                                }
                            }
                            // Enter to submit or select in modal
                            (KeyCode::Enter, KeyModifiers::NONE) => {
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
                                        Some(ModalType::SessionPicker) => {
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::FilePicker) => {
                                            // Select file and close
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::ThemePicker) => {
                                            // Apply selected theme permanently and close
                                            let filtered_themes = self.state.get_filtered_themes();
                                            if let Some(&selected_theme) = filtered_themes
                                                .get(self.state.theme_picker_selected)
                                            {
                                                self.state.apply_theme(selected_theme);
                                            }
                                            self.state.active_modal = None;
                                            self.state.focused_component = FocusedComponent::Input;
                                        }
                                        Some(ModalType::Help) => {
                                            // Close help
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::Approval) => {
                                            // Approval handled via Y/N keys
                                        }
                                        Some(ModalType::TrustLevel) => {
                                            // TrustLevel handled via Up/Down/Enter keys
                                        }
                                        Some(ModalType::Tools)
                                        | Some(ModalType::Plugin)
                                        | Some(ModalType::DeviceFlow) => {
                                            // Close these modals
                                            self.state.close_modal();
                                        }
                                        Some(ModalType::SessionSwitchConfirm) => {
                                            // SessionSwitchConfirm handled in controller
                                        }
                                        Some(ModalType::TaskEdit) => {
                                            // TaskEdit handled in controller
                                        }
                                        Some(ModalType::TaskDeleteConfirm) => {
                                            // TaskDeleteConfirm handled in controller
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
                            // Arrow keys for navigation
                            (KeyCode::Up, _) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    // Sidebar navigation
                                    if self.state.sidebar_selected_item.is_some() {
                                        self.state.sidebar_prev_item();
                                    } else {
                                        self.state.sidebar_prev_panel();
                                    }
                                } else if self.state.active_modal == Some(ModalType::ThemePicker) {
                                    if self.state.theme_picker_selected > 0 {
                                        self.state.theme_picker_selected -= 1;
                                        // Live preview the theme
                                        let filtered_themes = self.state.get_filtered_themes();
                                        if let Some(&theme) =
                                            filtered_themes.get(self.state.theme_picker_selected)
                                        {
                                            self.state.preview_theme(theme);
                                        }
                                    }
                                }
                            }
                            (KeyCode::Down, _) => {
                                if self.state.focused_component == FocusedComponent::Panel {
                                    // Sidebar navigation
                                    if self.state.sidebar_selected_item.is_some() {
                                        self.state.sidebar_next_item();
                                    } else {
                                        self.state.sidebar_next_panel();
                                    }
                                } else if self.state.active_modal == Some(ModalType::ThemePicker) {
                                    let filtered_themes = self.state.get_filtered_themes();
                                    if self.state.theme_picker_selected + 1 < filtered_themes.len()
                                    {
                                        self.state.theme_picker_selected += 1;
                                        // Live preview the theme
                                        if let Some(&theme) =
                                            filtered_themes.get(self.state.theme_picker_selected)
                                        {
                                            self.state.preview_theme(theme);
                                        }
                                    }
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
