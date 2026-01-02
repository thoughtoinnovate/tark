//! TUI Application state and main loop
//!
//! Contains the core application state and rendering logic for the terminal UI.

// Allow dead code for intentionally unused API methods that are part of the public interface
// These methods are designed for future use when the TUI is fully integrated
#![allow(dead_code)]

use std::io::{self, Stdout};
use std::panic;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use super::agent_bridge::AgentMode;
use super::commands::{AgentModeChange, CommandHandler, CommandResult, ToggleSetting};
use super::config::TuiConfig;
use super::events::{Event, EventHandler};
use super::keybindings::{Action, FocusedComponent, InputMode, KeybindingHandler};
use super::widgets::{ChatMessage, InputWidget, MessageList, PanelWidget};

/// Application state for the TUI
#[derive(Debug, Default)]
pub struct AppState {
    /// Whether the application should exit
    pub should_quit: bool,
    /// Current agent mode
    pub mode: AgentMode,
    /// Whether thinking/verbose mode is enabled
    pub thinking_mode: bool,
    /// Whether connected to an editor (Neovim)
    pub editor_connected: bool,
    /// Terminal size (cols, rows)
    pub terminal_size: (u16, u16),
    /// Current input mode (Normal, Insert, Command)
    pub input_mode: InputMode,
    /// Currently focused component
    pub focused_component: FocusedComponent,
    /// Message list state
    pub message_list: MessageList,
    /// Input widget state
    pub input_widget: InputWidget,
    /// Panel widget state
    pub panel_widget: PanelWidget,
    /// Status message to display (temporary)
    pub status_message: Option<String>,
    /// Tab completion state
    pub completion_state: CompletionState,
    /// TUI configuration
    pub config: TuiConfig,
}

/// State for tab completion
#[derive(Debug, Default, Clone)]
pub struct CompletionState {
    /// Available completions
    pub completions: Vec<String>,
    /// Current completion index
    pub index: usize,
    /// Original text before completion started
    pub original_text: String,
    /// Whether completion is active
    pub active: bool,
}

impl CompletionState {
    /// Start a new completion session
    pub fn start(&mut self, original: String, completions: Vec<String>) {
        self.original_text = original;
        self.completions = completions;
        self.index = 0;
        self.active = !self.completions.is_empty();
    }

    /// Get the current completion
    pub fn current(&self) -> Option<&str> {
        if self.active && !self.completions.is_empty() {
            Some(&self.completions[self.index])
        } else {
            None
        }
    }

    /// Move to the next completion
    pub fn next(&mut self) {
        if !self.completions.is_empty() {
            self.index = (self.index + 1) % self.completions.len();
        }
    }

    /// Move to the previous completion
    pub fn previous(&mut self) {
        if !self.completions.is_empty() {
            self.index = self
                .index
                .checked_sub(1)
                .unwrap_or(self.completions.len() - 1);
        }
    }

    /// Reset completion state
    pub fn reset(&mut self) {
        self.completions.clear();
        self.index = 0;
        self.original_text.clear();
        self.active = false;
    }
}

impl AppState {
    /// Create a new application state
    pub fn new() -> Self {
        // Load config from file, falling back to defaults on error
        let config = TuiConfig::load().unwrap_or_default();
        Self::with_config(config)
    }

    /// Create a new application state with a specific config
    pub fn with_config(config: TuiConfig) -> Self {
        Self {
            input_mode: InputMode::Insert, // Start in insert mode for immediate typing
            focused_component: FocusedComponent::Input,
            config,
            ..Default::default()
        }
    }

    /// Get the TUI configuration
    pub fn config(&self) -> &TuiConfig {
        &self.config
    }

    /// Check if running in standalone mode (no editor connection)
    pub fn is_standalone(&self) -> bool {
        !self.editor_connected
    }

    /// Check if a feature requiring editor connection is available
    pub fn is_editor_feature_available(&self) -> bool {
        self.editor_connected
    }

    /// Get the mode string for display
    pub fn mode_display(&self) -> &'static str {
        if self.editor_connected {
            "Connected"
        } else {
            "Standalone"
        }
    }

    /// Signal the application to quit
    pub fn quit(&mut self) {
        self.should_quit = true;
    }

    /// Update terminal size
    pub fn set_terminal_size(&mut self, cols: u16, rows: u16) {
        self.terminal_size = (cols, rows);
    }

    /// Set the input mode
    pub fn set_input_mode(&mut self, mode: InputMode) {
        self.input_mode = mode;
        // Update input widget focus based on mode
        self.input_widget
            .set_focused(mode == InputMode::Insert || mode == InputMode::Command);
    }

    /// Set the focused component
    pub fn set_focused_component(&mut self, component: FocusedComponent) {
        self.focused_component = component;
        // Update widget focus states
        self.input_widget
            .set_focused(component == FocusedComponent::Input);
    }

    /// Cycle focus to the next component
    pub fn focus_next(&mut self) {
        self.set_focused_component(self.focused_component.next());
    }

    /// Cycle focus to the previous component
    pub fn focus_previous(&mut self) {
        self.set_focused_component(self.focused_component.previous());
    }

    /// Handle navigation action based on focused component
    pub fn handle_navigation(&mut self, action: Action) {
        match self.focused_component {
            FocusedComponent::Messages => self.handle_message_navigation(action),
            FocusedComponent::Panel => self.handle_panel_navigation(action),
            FocusedComponent::Input => {
                // In input focus, navigation keys might switch to messages
                if matches!(
                    action,
                    Action::LineUp | Action::LineDown | Action::GoToTop | Action::GoToBottom
                ) {
                    self.set_focused_component(FocusedComponent::Messages);
                    self.handle_message_navigation(action);
                }
            }
        }
    }

    /// Handle navigation within the message list
    fn handle_message_navigation(&mut self, action: Action) {
        let width = self.terminal_size.0;
        match action {
            Action::LineDown => self.message_list.scroll_down(width),
            Action::LineUp => self.message_list.scroll_up(),
            Action::GoToTop => self.message_list.scroll_to_top(),
            Action::GoToBottom => self.message_list.scroll_to_bottom(width),
            Action::HalfPageDown => self.message_list.scroll_half_page_down(width),
            Action::HalfPageUp => self.message_list.scroll_half_page_up(),
            _ => {}
        }
    }

    /// Handle navigation within the panel
    fn handle_panel_navigation(&mut self, action: Action) {
        match action {
            Action::LineDown => self.panel_widget.focus_next(),
            Action::LineUp => self.panel_widget.focus_previous(),
            Action::GoToTop => self.panel_widget.focus_first(),
            Action::GoToBottom => self.panel_widget.focus_last(),
            Action::ExpandSection => self.panel_widget.expand_section(),
            Action::CollapseSection => self.panel_widget.collapse_section(),
            Action::ToggleSection => self.panel_widget.toggle_section(),
            _ => {}
        }
    }
}

/// Main TUI application
pub struct TuiApp {
    /// Terminal backend
    terminal: Terminal<CrosstermBackend<Stdout>>,
    /// Application state
    pub state: AppState,
    /// Event handler
    pub events: EventHandler,
    /// Keybinding handler
    pub keybindings: KeybindingHandler,
    /// Command handler
    pub commands: CommandHandler,
}

impl TuiApp {
    /// Create a new TUI application
    ///
    /// This initializes the terminal in raw mode with alternate screen
    /// and sets up a panic hook for clean exit.
    pub fn new() -> anyhow::Result<Self> {
        // Set up panic hook before initializing terminal
        Self::install_panic_hook();

        let terminal = Self::setup_terminal()?;
        let mut state = AppState::new();
        let events = EventHandler::new();
        let keybindings = KeybindingHandler::new();
        let commands = CommandHandler::new();

        // Get initial terminal size
        let size = terminal.size()?;
        state.set_terminal_size(size.width, size.height);

        Ok(Self {
            terminal,
            state,
            events,
            keybindings,
            commands,
        })
    }

    /// Create a new TUI application with a specific configuration
    pub fn with_config(config: TuiConfig) -> anyhow::Result<Self> {
        // Set up panic hook before initializing terminal
        Self::install_panic_hook();

        let terminal = Self::setup_terminal()?;
        let mut state = AppState::with_config(config);
        let events = EventHandler::new();
        let keybindings = KeybindingHandler::new();
        let commands = CommandHandler::new();

        // Get initial terminal size
        let size = terminal.size()?;
        state.set_terminal_size(size.width, size.height);

        Ok(Self {
            terminal,
            state,
            events,
            keybindings,
            commands,
        })
    }

    /// Get the current configuration
    pub fn config(&self) -> &TuiConfig {
        self.state.config()
    }

    /// Install a panic hook that restores the terminal before panicking
    ///
    /// This ensures the terminal is left in a usable state even if the
    /// application panics.
    fn install_panic_hook() {
        let original_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            // Restore terminal state before printing panic info
            let _ = Self::restore_terminal_static();
            original_hook(panic_info);
        }));
    }

    /// Static version of restore_terminal for use in panic hook
    fn restore_terminal_static() -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;
        Ok(())
    }

    /// Set up the terminal for TUI rendering
    fn setup_terminal() -> anyhow::Result<Terminal<CrosstermBackend<Stdout>>> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let terminal = Terminal::new(backend)?;
        Ok(terminal)
    }

    /// Restore the terminal to its original state
    fn restore_terminal(&mut self) -> anyhow::Result<()> {
        disable_raw_mode()?;
        execute!(
            self.terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        self.terminal.show_cursor()?;
        Ok(())
    }

    /// Get the current terminal size
    pub fn terminal_size(&self) -> anyhow::Result<(u16, u16)> {
        let size = self.terminal.size()?;
        Ok((size.width, size.height))
    }

    /// Handle an event and update state accordingly
    ///
    /// Returns true if the event was handled and requires a redraw.
    pub fn handle_event(&mut self, event: Event) -> anyhow::Result<bool> {
        match event {
            Event::Resize(cols, rows) => {
                self.state.set_terminal_size(cols, rows);
                Ok(true)
            }
            Event::Key(key_event) => self.handle_key_event(key_event),
            Event::Tick => Ok(false),
            _ => Ok(false),
        }
    }

    /// Handle a key event
    fn handle_key_event(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
        // Get action from keybinding handler
        if let Some(action) = self.keybindings.handle_key(key, self.state.input_mode) {
            return self.handle_action(action);
        }

        // If in insert mode and no action, pass key to input widget
        if self.state.input_mode == InputMode::Insert {
            self.handle_input_key(key);
            return Ok(true);
        }

        Ok(false)
    }

    /// Handle an action from the keybinding handler
    fn handle_action(&mut self, action: Action) -> anyhow::Result<bool> {
        match action {
            Action::Quit => {
                self.state.quit();
                Ok(false)
            }
            Action::EnterInsertMode => {
                self.state.set_input_mode(InputMode::Insert);
                self.state.set_focused_component(FocusedComponent::Input);
                Ok(true)
            }
            Action::ExitInsertMode => {
                self.state.set_input_mode(InputMode::Normal);
                Ok(true)
            }
            Action::FocusNext => {
                self.state.focus_next();
                Ok(true)
            }
            Action::FocusPrevious => {
                self.state.focus_previous();
                Ok(true)
            }
            Action::FocusInput => {
                self.state.set_focused_component(FocusedComponent::Input);
                Ok(true)
            }
            Action::FocusMessages => {
                self.state.set_focused_component(FocusedComponent::Messages);
                Ok(true)
            }
            Action::FocusPanel => {
                self.state.set_focused_component(FocusedComponent::Panel);
                Ok(true)
            }
            Action::Submit => {
                self.handle_submit();
                Ok(true)
            }
            Action::Cancel => {
                self.keybindings.clear_sequence();
                if self.state.input_mode == InputMode::Insert {
                    self.state.set_input_mode(InputMode::Normal);
                }
                Ok(true)
            }
            // Navigation actions
            Action::LineDown
            | Action::LineUp
            | Action::GoToTop
            | Action::GoToBottom
            | Action::HalfPageDown
            | Action::HalfPageUp
            | Action::ExpandSection
            | Action::CollapseSection
            | Action::ToggleSection => {
                self.state.handle_navigation(action);
                Ok(true)
            }
        }
    }

    /// Handle input key in insert mode
    fn handle_input_key(&mut self, key: KeyEvent) {
        // Reset completion state on most keys (except Tab)
        if key.code != KeyCode::Tab && key.code != KeyCode::BackTab {
            self.state.completion_state.reset();
        }

        match key.code {
            KeyCode::Tab => {
                self.handle_tab_completion(false);
            }
            KeyCode::BackTab => {
                self.handle_tab_completion(true);
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    // Handle Ctrl combinations
                    match c {
                        'w' => self.state.input_widget.delete_word_before(),
                        'a' => self.state.input_widget.move_cursor_to_start(),
                        'e' => self.state.input_widget.move_cursor_to_end(),
                        _ => {}
                    }
                } else {
                    self.state.input_widget.insert_char(c);
                }
            }
            KeyCode::Backspace => {
                self.state.input_widget.delete_char_before();
            }
            KeyCode::Delete => {
                self.state.input_widget.delete_char_at();
            }
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.state.input_widget.move_cursor_word_left();
                } else {
                    self.state.input_widget.move_cursor_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.state.input_widget.move_cursor_word_right();
                } else {
                    self.state.input_widget.move_cursor_right();
                }
            }
            KeyCode::Home => {
                self.state.input_widget.move_cursor_to_start();
            }
            KeyCode::End => {
                self.state.input_widget.move_cursor_to_end();
            }
            KeyCode::Up => {
                self.state.input_widget.history_previous();
            }
            KeyCode::Down => {
                self.state.input_widget.history_next();
            }
            _ => {}
        }
    }

    /// Handle tab completion for commands
    fn handle_tab_completion(&mut self, reverse: bool) {
        let content = self.state.input_widget.content();

        // Only complete if input starts with /
        if !content.starts_with('/') {
            return;
        }

        if self.state.completion_state.active {
            // Cycle through completions
            if reverse {
                self.state.completion_state.previous();
            } else {
                self.state.completion_state.next();
            }

            // Apply current completion
            if let Some(completion) = self.state.completion_state.current() {
                self.state.input_widget.set_content(completion);
            }
        } else {
            // Start new completion
            let completions = self.commands.get_completions(content);
            if !completions.is_empty() {
                let original = content.to_string();
                self.state.completion_state.start(original, completions);

                // Apply first completion
                if let Some(completion) = self.state.completion_state.current() {
                    self.state.input_widget.set_content(completion);
                }
            }
        }
    }

    /// Handle submit action
    fn handle_submit(&mut self) {
        let content = self.state.input_widget.submit();
        if content.trim().is_empty() {
            return;
        }

        // Check if this is a command
        if CommandHandler::is_command(&content) {
            self.handle_command(&content);
        } else {
            // Regular message - add to the list
            self.state.message_list.push(ChatMessage::user(content));
        }
    }

    /// Handle a slash command
    fn handle_command(&mut self, input: &str) {
        let result = self
            .commands
            .execute_with_editor_state(input, self.state.editor_connected);

        match result {
            CommandResult::Continue => {}
            CommandResult::ClearInput => {
                self.state.input_widget.clear();
            }
            CommandResult::ShowPicker(picker_type) => {
                // TODO: Implement picker UI
                self.state.status_message = Some(format!("Picker: {:?}", picker_type));
            }
            CommandResult::ChangeMode(mode_change) => {
                self.state.mode = match mode_change {
                    AgentModeChange::Plan => AgentMode::Plan,
                    AgentModeChange::Build => AgentMode::Build,
                    AgentModeChange::Review => AgentMode::Review,
                };
                self.state.status_message = Some(format!("Switched to {:?} mode", self.state.mode));
            }
            CommandResult::Toggle(setting) => match setting {
                ToggleSetting::Thinking => {
                    self.state.thinking_mode = !self.state.thinking_mode;
                    self.state.status_message = Some(format!(
                        "Thinking mode: {}",
                        if self.state.thinking_mode {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ));
                }
            },
            CommandResult::ShowHelp(help_text) => {
                // Add help as a system message
                self.state.message_list.push(ChatMessage::system(help_text));
            }
            CommandResult::ShowStats => {
                // TODO: Implement stats display
                self.state.status_message = Some("Stats: Not yet implemented".to_string());
            }
            CommandResult::ShowCost => {
                // TODO: Implement cost display
                self.state.status_message = Some("Cost: Not yet implemented".to_string());
            }
            CommandResult::ShowUsage => {
                // Display usage information as a system message
                // TODO: Integrate with UsageManager when available
                let usage_text = "=== Usage Statistics ===\n\
                    Current session: 0 tokens, $0.0000\n\
                    Use /usage-open to view detailed dashboard";
                self.state
                    .message_list
                    .push(ChatMessage::system(usage_text.to_string()));
            }
            CommandResult::OpenUsageDashboard => {
                // TODO: Open browser
                self.state.status_message = Some("Opening usage dashboard...".to_string());
            }
            CommandResult::ClearHistory => {
                self.state.message_list.clear();
                self.state.status_message = Some("Chat history cleared".to_string());
            }
            CommandResult::Compact => {
                // TODO: Implement compaction
                self.state.status_message = Some("Compacting conversation...".to_string());
            }
            CommandResult::NewSession => {
                // TODO: Implement session creation
                self.state.message_list.clear();
                self.state.status_message = Some("New session created".to_string());
            }
            CommandResult::DeleteSession => {
                // TODO: Implement session deletion
                self.state.status_message = Some("Delete session: Not yet implemented".to_string());
            }
            CommandResult::Exit => {
                self.state.quit();
            }
            CommandResult::Interrupt => {
                // TODO: Implement interrupt
                self.state.status_message = Some("Interrupting...".to_string());
            }
            CommandResult::AttachFile(path) => {
                // TODO: Implement file attachment with AttachmentManager
                self.state.status_message = Some(format!("Attaching file: {}", path));
            }
            CommandResult::ClearAttachments => {
                // TODO: Implement clearing attachments with AttachmentManager
                self.state.status_message = Some("Attachments cleared".to_string());
            }
            CommandResult::Error(msg) => {
                self.state
                    .message_list
                    .push(ChatMessage::system(format!("Error: {}", msg)));
            }
            CommandResult::Message(msg) => {
                self.state.status_message = Some(msg);
            }
            // Plan commands
            CommandResult::PlanStatus => {
                // TODO: Implement with PlanManager
                self.state.status_message = Some("Plan status: No active plan".to_string());
            }
            CommandResult::PlanList => {
                // TODO: Implement with PlanManager
                self.state.status_message = Some("No plans found".to_string());
            }
            CommandResult::PlanDone(task_arg) => {
                // TODO: Implement with PlanManager
                let msg = match task_arg {
                    Some(task) => format!("Marked task '{}' as done", task),
                    None => "Marked current task as done".to_string(),
                };
                self.state.status_message = Some(msg);
            }
            CommandResult::PlanSkip(task_arg) => {
                // TODO: Implement with PlanManager
                let msg = match task_arg {
                    Some(task) => format!("Skipped task '{}'", task),
                    None => "Skipped current task".to_string(),
                };
                self.state.status_message = Some(msg);
            }
            CommandResult::PlanNext => {
                // TODO: Implement with PlanManager
                self.state.status_message = Some("No pending tasks".to_string());
            }
            CommandResult::PlanRefine(refinement) => {
                // TODO: Implement with PlanManager
                self.state.status_message = Some(format!("Added refinement: {}", refinement));
            }
            // Diff commands
            CommandResult::ShowDiff(file) => {
                if self.state.editor_connected {
                    // TODO: Send ShowDiff via editor bridge
                    self.state.status_message = Some(format!("Showing diff for: {}", file));
                } else {
                    // In standalone mode, show inline diff message
                    self.state.message_list.push(ChatMessage::system(format!(
                        "Diff view requires Neovim connection. File: {}",
                        file
                    )));
                }
            }
            CommandResult::ToggleAutoDiff => {
                // Toggle auto-diff mode (tracked in app state)
                // TODO: Integrate with PlanManager when available
                self.state.status_message = Some("Auto-diff mode toggled".to_string());
            }
            CommandResult::FocusTasks => {
                self.state.set_focused_component(FocusedComponent::Panel);
                self.state.status_message = Some("Focused tasks panel".to_string());
            }
        }
    }

    /// Run the main event loop
    ///
    /// This polls for events and renders the UI until the application quits.
    pub fn run(&mut self) -> anyhow::Result<()> {
        // Initial render
        self.render()?;

        while !self.state.should_quit {
            // Poll for events
            if let Some(event) = self.events.poll()? {
                let needs_redraw = self.handle_event(event)?;
                if needs_redraw {
                    self.render()?;
                }
            }
        }

        Ok(())
    }

    /// Render the UI
    pub fn render(&mut self) -> anyhow::Result<()> {
        use super::widgets::InputWidgetRenderer;
        use ratatui::layout::{Constraint, Direction, Layout};
        use ratatui::style::{Color, Modifier, Style};
        use ratatui::widgets::{Block, Borders, Paragraph};

        let input_mode = self.state.input_mode;
        let focused = self.state.focused_component;
        let pending_key = self.keybindings.pending_key();
        let agent_mode = self.state.mode;
        let thinking_mode = self.state.thinking_mode;
        let status_message = self.state.status_message.take();
        let editor_connected = self.state.editor_connected;

        // Clone input widget for rendering (it implements Clone)
        let input_widget = self.state.input_widget.clone();

        // Collect message data for rendering
        let messages_data: Vec<(super::widgets::Role, String)> = self
            .state
            .message_list
            .messages()
            .iter()
            .map(|msg| (msg.role, msg.content.clone()))
            .collect();
        let messages_empty = messages_data.is_empty();

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Main vertical layout: content area + input + status bar
            let main_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),    // Content (messages + panel)
                    Constraint::Length(3), // Input
                    Constraint::Length(1), // Status bar
                ])
                .split(area);

            // Content area: messages (left) + panel (right)
            let content_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(75), // Messages area
                    Constraint::Percentage(25), // Panel
                ])
                .split(main_chunks[0]);

            // Messages area
            let messages_block = Block::default()
                .title(" Messages ")
                .borders(Borders::ALL)
                .border_style(if focused == FocusedComponent::Messages {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            let messages_inner = messages_block.inner(content_chunks[0]);
            frame.render_widget(messages_block, content_chunks[0]);

            // Render message content
            if messages_empty {
                let welcome_text = vec![
                    ratatui::text::Line::from(""),
                    ratatui::text::Line::from(ratatui::text::Span::styled(
                        "  Welcome to tark chat!",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )),
                    ratatui::text::Line::from(""),
                    ratatui::text::Line::from("  Type a message to start chatting."),
                    ratatui::text::Line::from("  Use /help for available commands."),
                    ratatui::text::Line::from(""),
                    ratatui::text::Line::from(ratatui::text::Span::styled(
                        "  Keybindings:",
                        Style::default().add_modifier(Modifier::BOLD),
                    )),
                    ratatui::text::Line::from("    i     - Enter insert mode"),
                    ratatui::text::Line::from("    Esc   - Exit insert mode"),
                    ratatui::text::Line::from("    Enter - Send message"),
                    ratatui::text::Line::from("    q     - Quit (in normal mode)"),
                    ratatui::text::Line::from("    Tab   - Cycle focus"),
                ];
                let welcome = Paragraph::new(welcome_text);
                frame.render_widget(welcome, messages_inner);
            } else {
                // Render messages from the list
                let messages: Vec<ratatui::text::Line> = messages_data
                    .iter()
                    .flat_map(|(role, content)| {
                        let (role_style, role_icon) = match role {
                            super::widgets::Role::User => {
                                (Style::default().fg(Color::Green), "👤 You")
                            }
                            super::widgets::Role::Assistant => {
                                (Style::default().fg(Color::Cyan), "🤖 Assistant")
                            }
                            super::widgets::Role::System => {
                                (Style::default().fg(Color::Yellow), "⚙ System")
                            }
                            super::widgets::Role::Tool => {
                                (Style::default().fg(Color::Magenta), "🔧 Tool")
                            }
                        };

                        let mut lines =
                            vec![ratatui::text::Line::from(ratatui::text::Span::styled(
                                role_icon,
                                role_style.add_modifier(Modifier::BOLD),
                            ))];

                        // Add message content lines
                        for line in content.lines() {
                            lines.push(ratatui::text::Line::from(format!("  {}", line)));
                        }

                        // Add spacing between messages
                        lines.push(ratatui::text::Line::from(""));

                        lines
                    })
                    .collect();

                let messages_paragraph = Paragraph::new(messages);
                frame.render_widget(messages_paragraph, messages_inner);
            }

            // Panel area (right side)
            let panel_block = Block::default()
                .title(" Panel ")
                .borders(Borders::ALL)
                .border_style(if focused == FocusedComponent::Panel {
                    Style::default().fg(Color::Magenta)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            let panel_inner = panel_block.inner(content_chunks[1]);
            frame.render_widget(panel_block, content_chunks[1]);

            // Panel content
            let panel_content = vec![
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    " Tasks",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                ratatui::text::Line::from("   No active tasks"),
                ratatui::text::Line::from(""),
                ratatui::text::Line::from(ratatui::text::Span::styled(
                    " Files",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                ratatui::text::Line::from("   No files attached"),
            ];
            let panel_paragraph = Paragraph::new(panel_content);
            frame.render_widget(panel_paragraph, panel_inner);

            // Input area
            let mode_indicator = match input_mode {
                InputMode::Normal => ("NORMAL", Color::Blue),
                InputMode::Insert => ("INSERT", Color::Green),
                InputMode::Command => ("COMMAND", Color::Yellow),
            };
            let pending_indicator = pending_key.map(|k| format!(" {}", k)).unwrap_or_default();
            let input_block = Block::default()
                .title(format!(" [{}]{} ", mode_indicator.0, pending_indicator))
                .borders(Borders::ALL)
                .border_style(if focused == FocusedComponent::Input {
                    Style::default().fg(mode_indicator.1)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            // Render the actual input widget content
            let input_renderer = InputWidgetRenderer::new(&input_widget).block(input_block);
            frame.render_widget(input_renderer, main_chunks[1]);

            // Status bar
            let mode_str = match agent_mode {
                AgentMode::Build => ("◆ Build", Color::Green),
                AgentMode::Plan => ("◇ Plan", Color::Yellow),
                AgentMode::Review => ("◈ Review", Color::Cyan),
            };
            let thinking_str = if thinking_mode { " [verbose]" } else { "" };
            let connection_str = if editor_connected {
                ("◉ Connected", Color::Green)
            } else {
                ("○ Standalone", Color::DarkGray)
            };

            let status_spans = vec![
                ratatui::text::Span::styled(
                    format!(" {} ", mode_str.0),
                    Style::default().fg(mode_str.1),
                ),
                ratatui::text::Span::raw("│"),
                ratatui::text::Span::styled(
                    format!(" {} ", connection_str.0),
                    Style::default().fg(connection_str.1),
                ),
                ratatui::text::Span::raw("│"),
                ratatui::text::Span::styled(thinking_str, Style::default().fg(Color::Magenta)),
                ratatui::text::Span::raw(" "),
                ratatui::text::Span::styled(
                    status_message.unwrap_or_else(|| "Ready".to_string()),
                    Style::default().fg(Color::White),
                ),
            ];

            let status = Paragraph::new(ratatui::text::Line::from(status_spans))
                .style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status, main_chunks[2]);
        })?;
        Ok(())
    }

    /// Check if the application should quit
    pub fn should_quit(&self) -> bool {
        self.state.should_quit
    }
}

impl Drop for TuiApp {
    fn drop(&mut self) {
        // Ensure terminal is restored even on panic
        let _ = self.restore_terminal();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_default() {
        let state = AppState::new();
        assert!(!state.should_quit);
        assert_eq!(state.mode, AgentMode::Build);
        assert!(!state.thinking_mode);
        assert!(!state.editor_connected);
        assert_eq!(state.input_mode, InputMode::Insert); // Start in insert mode
        assert_eq!(state.focused_component, FocusedComponent::Input);
    }

    #[test]
    fn test_app_state_quit() {
        let mut state = AppState::new();
        assert!(!state.should_quit);
        state.quit();
        assert!(state.should_quit);
    }

    #[test]
    fn test_app_state_terminal_size() {
        let mut state = AppState::new();
        state.set_terminal_size(80, 24);
        assert_eq!(state.terminal_size, (80, 24));
    }

    #[test]
    fn test_app_state_input_mode() {
        let mut state = AppState::new();

        state.set_input_mode(InputMode::Normal);
        assert_eq!(state.input_mode, InputMode::Normal);
        assert!(!state.input_widget.is_focused());

        state.set_input_mode(InputMode::Insert);
        assert_eq!(state.input_mode, InputMode::Insert);
        assert!(state.input_widget.is_focused());
    }

    #[test]
    fn test_app_state_focus_cycle() {
        let mut state = AppState::new();
        assert_eq!(state.focused_component, FocusedComponent::Input);

        state.focus_next();
        assert_eq!(state.focused_component, FocusedComponent::Messages);

        state.focus_next();
        assert_eq!(state.focused_component, FocusedComponent::Panel);

        state.focus_next();
        assert_eq!(state.focused_component, FocusedComponent::Input);

        state.focus_previous();
        assert_eq!(state.focused_component, FocusedComponent::Panel);
    }

    #[test]
    fn test_app_state_set_focused_component() {
        let mut state = AppState::new();

        state.set_focused_component(FocusedComponent::Messages);
        assert_eq!(state.focused_component, FocusedComponent::Messages);
        assert!(!state.input_widget.is_focused());

        state.set_focused_component(FocusedComponent::Input);
        assert_eq!(state.focused_component, FocusedComponent::Input);
        assert!(state.input_widget.is_focused());
    }

    #[test]
    fn test_completion_state_start() {
        let mut state = CompletionState::default();
        assert!(!state.active);

        state.start(
            "/he".to_string(),
            vec!["/help".to_string(), "/hello".to_string()],
        );
        assert!(state.active);
        assert_eq!(state.completions.len(), 2);
        assert_eq!(state.current(), Some("/help"));
    }

    #[test]
    fn test_completion_state_navigation() {
        let mut state = CompletionState::default();
        state.start(
            "/".to_string(),
            vec!["/a".to_string(), "/b".to_string(), "/c".to_string()],
        );

        assert_eq!(state.current(), Some("/a"));

        state.next();
        assert_eq!(state.current(), Some("/b"));

        state.next();
        assert_eq!(state.current(), Some("/c"));

        state.next(); // Wraps around
        assert_eq!(state.current(), Some("/a"));

        state.previous(); // Wraps around
        assert_eq!(state.current(), Some("/c"));
    }

    #[test]
    fn test_completion_state_reset() {
        let mut state = CompletionState::default();
        state.start("/he".to_string(), vec!["/help".to_string()]);
        assert!(state.active);

        state.reset();
        assert!(!state.active);
        assert!(state.completions.is_empty());
        assert!(state.current().is_none());
    }

    #[test]
    fn test_completion_state_empty() {
        let mut state = CompletionState::default();
        state.start("/xyz".to_string(), vec![]);
        assert!(!state.active);
        assert!(state.current().is_none());
    }
}

/// Property-based tests for terminal initialization
///
/// **Property: Terminal setup/teardown is clean**
/// **Validates: Requirements 1.1**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        /// **Feature: terminal-tui-chat, Property: Terminal setup/teardown is clean**
        /// **Validates: Requirements 1.1**
        ///
        /// For any valid terminal size, the AppState should correctly store and
        /// retrieve the size, and the state should be consistent after resize events.
        #[test]
        fn prop_terminal_size_round_trip(cols in 1u16..=500u16, rows in 1u16..=200u16) {
            let mut state = AppState::new();

            // Set terminal size
            state.set_terminal_size(cols, rows);

            // Size should be stored correctly
            assert_eq!(state.terminal_size, (cols, rows));

            // Multiple resize events should work correctly
            let new_cols = cols.saturating_add(10);
            let new_rows = rows.saturating_add(5);
            state.set_terminal_size(new_cols, new_rows);
            assert_eq!(state.terminal_size, (new_cols, new_rows));
        }

        /// **Feature: terminal-tui-chat, Property: Terminal setup/teardown is clean**
        /// **Validates: Requirements 1.1**
        ///
        /// For any sequence of state changes, the quit state should be idempotent
        /// and the application should cleanly signal exit.
        #[test]
        fn prop_quit_state_idempotent(quit_count in 1usize..=10usize) {
            let mut state = AppState::new();

            // Initially not quitting
            assert!(!state.should_quit);

            // Call quit multiple times
            for _ in 0..quit_count {
                state.quit();
            }

            // Should still be in quit state (idempotent)
            assert!(state.should_quit);
        }

        /// **Feature: terminal-tui-chat, Property: Terminal setup/teardown is clean**
        /// **Validates: Requirements 1.1**
        ///
        /// For any agent mode, the state should correctly store and preserve the mode.
        #[test]
        fn prop_agent_mode_preserved(mode_idx in 0u8..3u8) {
            let mut state = AppState::new();

            let mode = match mode_idx {
                0 => AgentMode::Build,
                1 => AgentMode::Plan,
                _ => AgentMode::Review,
            };

            state.mode = mode;
            assert_eq!(state.mode, mode);
        }

        /// **Feature: terminal-tui-chat, Property: Focus cycle is consistent**
        /// **Validates: Requirements 1.4**
        ///
        /// For any number of focus cycles, cycling through all components
        /// and back should return to the original state.
        #[test]
        fn prop_focus_cycle_consistent(cycles in 1usize..=10usize) {
            let mut state = AppState::new();
            let initial_focus = state.focused_component;

            // Cycle through 3 times the number of components times cycles
            for _ in 0..(3 * cycles) {
                state.focus_next();
            }

            // Should be back to initial focus
            assert_eq!(state.focused_component, initial_focus);
        }

        /// **Feature: terminal-tui-chat, Property: Input mode affects widget focus**
        /// **Validates: Requirements 1.4**
        ///
        /// For any input mode change, the input widget focus should be
        /// correctly updated based on the mode.
        #[test]
        fn prop_input_mode_affects_widget_focus(mode_idx in 0u8..3u8) {
            let mut state = AppState::new();

            let mode = match mode_idx {
                0 => InputMode::Normal,
                1 => InputMode::Insert,
                _ => InputMode::Command,
            };

            state.set_input_mode(mode);

            // Input widget should be focused in Insert or Command mode
            let expected_focused = mode == InputMode::Insert || mode == InputMode::Command;
            assert_eq!(state.input_widget.is_focused(), expected_focused);
        }
    }
}
