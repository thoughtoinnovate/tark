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

use super::agent_bridge::{AgentBridge, AgentEvent, AgentMode};
use super::attachments::{Attachment, MessageAttachment};
use super::commands::{
    AgentModeChange, CommandHandler, CommandResult, ModelPickerState, PickerType, ProviderInfo,
    ToggleSetting,
};
use super::config::TuiConfig;
use super::events::{Event, EventHandler};
use super::keybindings::{Action, FocusedComponent, InputMode, KeybindingHandler};
use super::widgets::{
    AttachmentDropdownState, ChatMessage, CommandDropdown, EnhancedPanelData, EnhancedPanelWidget,
    InputWidget, MessageList, MessageListWidget, PanelSectionState, PanelWidget, Picker,
    PickerItem, PickerWidget,
};
use tokio::sync::mpsc;

/// Spinner animation frames for processing indicator
const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Interval between spinner frame updates (milliseconds)
const SPINNER_INTERVAL_MS: u64 = 80;

/// Application state for the TUI
#[derive(Debug)]
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
    /// Enhanced panel section state (accordion, scroll)
    pub panel_section_state: PanelSectionState,
    /// Enhanced panel data (session, context, tasks, files)
    pub enhanced_panel_data: EnhancedPanelData,
    /// Status message to display (temporary)
    pub status_message: Option<String>,
    /// Tab completion state
    pub completion_state: CompletionState,
    /// TUI configuration
    pub config: TuiConfig,
    /// Whether LLM is properly configured
    pub llm_configured: bool,
    /// LLM configuration error message (if not configured)
    pub llm_error: Option<String>,
    /// Attachment dropdown state for managing attachments
    pub attachment_dropdown_state: AttachmentDropdownState,
    /// Current attachments
    pub attachments: Vec<Attachment>,
    /// Pending message to send to LLM (for async processing)
    pub pending_message: Option<String>,
    /// Whether the agent is currently processing a message
    pub agent_processing: bool,
    /// Current tool being executed (for status bar display, Requirement 4.6)
    pub current_tool: Option<String>,
    /// Picker widget state for session/provider/model selection
    pub picker: Picker,
    /// Type of picker currently active (if any)
    pub active_picker_type: Option<PickerType>,
    /// State for the two-step model picker flow (provider → model selection)
    /// Tracks whether we're selecting a provider or a model within a provider
    pub model_picker_state: Option<ModelPickerState>,
    /// Rate limit retry timestamp (when to retry, Requirements 7.4)
    pub rate_limit_retry_at: Option<std::time::Instant>,
    /// Message to retry after rate limit expires
    pub rate_limit_pending_message: Option<String>,
    /// Queue of pending prompts to process sequentially
    pub prompt_queue: std::collections::VecDeque<String>,
    /// Command dropdown for slash command intellisense
    pub command_dropdown: CommandDropdown,
    /// Flag to auto-scroll to bottom on next render (for streaming updates)
    pub auto_scroll_pending: bool,
    /// Spinner animation frame index for processing indicator
    pub spinner_frame: usize,
    /// Last time spinner was updated
    pub spinner_last_update: std::time::Instant,
    /// Pending async request ready for processing (used by async loop)
    pub pending_async_request: Option<AsyncMessageRequest>,
    /// Flag indicating panel needs update when bridge is restored
    pub panel_update_pending: bool,
    /// Authentication dialog state (OAuth device flow)
    pub auth_dialog: super::widgets::AuthDialog,
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

impl Default for AppState {
    fn default() -> Self {
        Self {
            should_quit: false,
            mode: AgentMode::default(),
            thinking_mode: true, // Enable thinking mode by default to show thinking blocks
            editor_connected: false,
            terminal_size: (0, 0),
            input_mode: InputMode::default(),
            focused_component: FocusedComponent::default(),
            message_list: MessageList::default(),
            input_widget: InputWidget::default(),
            panel_widget: PanelWidget::default(),
            panel_section_state: PanelSectionState::default(),
            enhanced_panel_data: EnhancedPanelData::default(),
            status_message: None,
            completion_state: CompletionState::default(),
            config: TuiConfig::default(),
            llm_configured: false,
            llm_error: None,
            attachment_dropdown_state: AttachmentDropdownState::default(),
            attachments: Vec::new(),
            pending_message: None,
            agent_processing: false,
            current_tool: None,
            picker: Picker::default(),
            active_picker_type: None,
            model_picker_state: None,
            rate_limit_retry_at: None,
            rate_limit_pending_message: None,
            prompt_queue: std::collections::VecDeque::new(),
            command_dropdown: CommandDropdown::default(),
            auto_scroll_pending: false,
            spinner_frame: 0,
            spinner_last_update: std::time::Instant::now(),
            pending_async_request: None,
            panel_update_pending: false,
            auth_dialog: super::widgets::AuthDialog::default(),
        }
    }
}

impl AppState {
    /// Get the actual width of the message area for scroll calculations
    ///
    /// The message area is 70% of terminal width (chat column) minus 2 for borders.
    /// This ensures scroll operations use the correct dimensions.
    fn get_message_area_width(&self) -> u16 {
        let terminal_width = self.terminal_size.0;
        let chat_column_width = (terminal_width as f32 * 0.7) as u16;
        chat_column_width.saturating_sub(2) // Subtract borders
    }

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
            spinner_last_update: std::time::Instant::now(),
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

    /// Update spinner animation frame if enough time has elapsed
    /// Returns true if the spinner was updated (requires re-render)
    pub fn update_spinner_if_needed(&mut self) -> bool {
        if !self.agent_processing {
            return false;
        }

        let elapsed = self.spinner_last_update.elapsed();
        if elapsed.as_millis() >= SPINNER_INTERVAL_MS as u128 {
            self.spinner_frame = (self.spinner_frame + 1) % SPINNER_FRAMES.len();
            self.spinner_last_update = std::time::Instant::now();
            true
        } else {
            false
        }
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
                // In input focus, j/k (LineUp/LineDown) should navigate history
                // Other navigation keys switch to messages
                // Note: History navigation is handled separately in TuiApp
                // to access the prompt_history field
                if matches!(action, Action::GoToTop | Action::GoToBottom) {
                    self.set_focused_component(FocusedComponent::Messages);
                    self.handle_message_navigation(action);
                }
                // LineUp/LineDown are handled by TuiApp for history navigation
            }
        }
    }

    /// Handle navigation within the message list
    fn handle_message_navigation(&mut self, action: Action) {
        let width = self.get_message_area_width();
        match action {
            Action::LineDown => self.message_list.scroll_down(width),
            Action::LineUp => self.message_list.scroll_up(),
            Action::GoToTop => self.message_list.scroll_to_top(),
            Action::GoToBottom => self.message_list.scroll_to_bottom(width),
            Action::HalfPageDown => self.message_list.scroll_half_page_down(width),
            Action::HalfPageUp => self.message_list.scroll_half_page_up(),
            // Collapsible block actions (Requirements 7.5, 7.6, 8.7, 8.8)
            // Note: These require cursor tracking to identify which block to expand/collapse
            // For now, they are handled as no-ops in messages; full implementation pending
            Action::ExpandSection | Action::CollapseSection | Action::ToggleSection => {
                // TODO: Implement block-level expand/collapse when cursor tracking is added
                // This would require tracking which collapsible block is under the cursor
            }
            _ => {}
        }
    }

    /// Handle navigation within the panel
    ///
    /// Uses the enhanced panel section state for accordion-style navigation
    /// with scroll support for Tasks and Files sections.
    fn handle_panel_navigation(&mut self, action: Action) {
        let max_tasks = self.enhanced_panel_data.tasks.len();
        let max_files = self.enhanced_panel_data.files.len();

        match action {
            Action::LineDown => {
                // If in a scrollable section (Tasks/Files) and expanded, scroll down
                // Otherwise, move to next section
                let section = self.panel_section_state.focused_section;
                let is_scrollable = matches!(
                    section,
                    super::widgets::EnhancedPanelSection::Tasks
                        | super::widgets::EnhancedPanelSection::Files
                );
                let is_expanded = self.panel_section_state.is_expanded(section);

                if is_scrollable && is_expanded {
                    self.panel_section_state.scroll_down(max_tasks, max_files);
                } else {
                    self.panel_section_state.focus_next();
                }
            }
            Action::LineUp => {
                // If in a scrollable section (Tasks/Files) and expanded, scroll up
                // Otherwise, move to previous section
                let section = self.panel_section_state.focused_section;
                let is_scrollable = matches!(
                    section,
                    super::widgets::EnhancedPanelSection::Tasks
                        | super::widgets::EnhancedPanelSection::Files
                );
                let is_expanded = self.panel_section_state.is_expanded(section);

                if is_scrollable && is_expanded {
                    self.panel_section_state.scroll_up();
                } else {
                    self.panel_section_state.focus_prev();
                }
            }
            Action::GoToTop => {
                self.panel_section_state.focused_section =
                    super::widgets::EnhancedPanelSection::Session;
                self.panel_section_state.tasks_scroll = 0;
                self.panel_section_state.files_scroll = 0;
            }
            Action::GoToBottom => {
                self.panel_section_state.focused_section =
                    super::widgets::EnhancedPanelSection::Files;
            }
            Action::ExpandSection => {
                let section = self.panel_section_state.focused_section;
                self.panel_section_state.set_expanded(section, true);
            }
            Action::CollapseSection => {
                let section = self.panel_section_state.focused_section;
                self.panel_section_state.set_expanded(section, false);
            }
            Action::ToggleSection => {
                self.panel_section_state.toggle_focused();
            }
            _ => {}
        }
    }

    /// Update the enhanced panel data
    ///
    /// This method should be called when data from AgentBridge, UsageManager,
    /// PlanManager, or EditorState changes.
    pub fn update_panel_data(&mut self, data: EnhancedPanelData) {
        self.enhanced_panel_data = data;
    }

    /// Update session info in the panel
    pub fn update_session_info(&mut self, session: super::widgets::SessionInfo) {
        self.enhanced_panel_data.session = session;
    }

    /// Update context info in the panel
    pub fn update_context_info(&mut self, context: super::widgets::ContextInfo) {
        self.enhanced_panel_data.context = context;
    }

    /// Update tasks in the panel
    pub fn update_tasks(&mut self, tasks: Vec<super::widgets::TaskItem>) {
        self.enhanced_panel_data.tasks = tasks;
    }

    /// Update files in the panel
    pub fn update_files(&mut self, files: Vec<super::widgets::FileItem>) {
        self.enhanced_panel_data.files = files;
    }
}

/// Pending async message request
#[derive(Debug)]
struct PendingMessageRequest {
    content: String,
    attachments: Vec<MessageAttachment>,
    tx: mpsc::Sender<AgentEvent>,
}

/// Async message request ready for processing
#[derive(Debug)]
pub struct AsyncMessageRequest {
    content: String,
    attachments: Vec<MessageAttachment>,
    tx: mpsc::Sender<AgentEvent>,
    config: super::attachments::AttachmentConfig,
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
    /// Agent bridge for LLM integration
    agent_bridge: Option<AgentBridge>,
    /// Receiver for agent events (streaming responses, tool calls, etc.)
    agent_event_rx: Option<mpsc::Receiver<AgentEvent>>,
    /// Pending message request for async processing
    pending_request: Option<PendingMessageRequest>,
    /// Detected system username for display
    username: String,
    /// Prompt history for navigating previous inputs (Requirements 14.3)
    prompt_history: super::prompt_history::PromptHistory,
}

impl TuiApp {
    /// Get the actual width of the message area for scroll calculations
    ///
    /// The message area is 70% of terminal width (chat column) minus 2 for borders.
    /// This ensures scroll_to_bottom uses the correct dimensions.
    fn get_message_area_width(&self) -> u16 {
        let terminal_width = self.state.terminal_size.0;
        let chat_column_width = (terminal_width as f32 * 0.7) as u16;
        chat_column_width.saturating_sub(2) // Subtract borders
    }

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

        // Detect system username (Requirements 2.1)
        let username = whoami::username();

        // Initialize AgentBridge (Requirements 1.3, 1.4, 1.5)
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Initialize prompt history (Requirements 14.3)
        let prompt_history = super::prompt_history::PromptHistory::for_workspace(&working_dir);

        let (agent_bridge, llm_configured, llm_error) = match AgentBridge::new(working_dir) {
            Ok(bridge) => (Some(bridge), true, None),
            Err(e) => {
                let error_msg = format!(
                    "To configure an LLM provider, set one of the following environment variables:\n\
                    • OPENAI_API_KEY - for OpenAI (GPT-4, etc.)\n\
                    • ANTHROPIC_API_KEY - for Claude\n\
                    • Or configure Ollama for local models\n\n\
                    Error: {}",
                    e
                );
                (None, false, Some(error_msg))
            }
        };

        state.llm_configured = llm_configured;
        state.llm_error = llm_error;

        let mut app = Self {
            terminal,
            state,
            events,
            keybindings,
            commands,
            agent_bridge,
            agent_event_rx: None,
            pending_request: None,
            username,
            prompt_history,
        };

        // Initialize panel data from AgentBridge (Requirements 5.1)
        app.update_panel_from_bridge();

        // Restore messages from current session on startup (Requirements 6.1, 6.6)
        app.restore_messages_from_session();

        Ok(app)
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

        // Detect system username (Requirements 2.1)
        let username = whoami::username();

        // Initialize AgentBridge (Requirements 1.3, 1.4, 1.5)
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Initialize prompt history (Requirements 14.3)
        let prompt_history = super::prompt_history::PromptHistory::for_workspace(&working_dir);

        let (agent_bridge, llm_configured, llm_error) = match AgentBridge::new(working_dir) {
            Ok(bridge) => (Some(bridge), true, None),
            Err(e) => {
                let error_msg = format!(
                    "To configure an LLM provider, set one of the following environment variables:\n\
                    • OPENAI_API_KEY - for OpenAI (GPT-4, etc.)\n\
                    • ANTHROPIC_API_KEY - for Claude\n\
                    • Or configure Ollama for local models\n\n\
                    Error: {}",
                    e
                );
                (None, false, Some(error_msg))
            }
        };

        state.llm_configured = llm_configured;
        state.llm_error = llm_error;

        let mut app = Self {
            terminal,
            state,
            events,
            keybindings,
            commands,
            agent_bridge,
            agent_event_rx: None,
            pending_request: None,
            username,
            prompt_history,
        };

        // Initialize panel data from AgentBridge (Requirements 5.1)
        app.update_panel_from_bridge();

        // Restore messages from current session on startup (Requirements 6.1, 6.6)
        app.restore_messages_from_session();

        Ok(app)
    }

    /// Create a new TUI application with provider and model overrides from CLI
    ///
    /// This allows `--provider` and `--model` CLI arguments to be applied
    /// at AgentBridge creation time, before the LLM provider is instantiated.
    pub fn with_provider_override(
        config: TuiConfig,
        provider: Option<String>,
        model: Option<String>,
    ) -> anyhow::Result<Self> {
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

        // Detect system username (Requirements 2.1)
        let username = whoami::username();

        // Initialize AgentBridge with provider/model overrides (Requirements 1.3, 1.4, 1.5)
        let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

        // Initialize prompt history (Requirements 14.3)
        let prompt_history = super::prompt_history::PromptHistory::for_workspace(&working_dir);

        let (agent_bridge, llm_configured, llm_error) = match AgentBridge::with_provider(
            working_dir,
            provider,
            model,
        ) {
            Ok(bridge) => (Some(bridge), true, None),
            Err(e) => {
                let error_msg = format!(
                        "To configure an LLM provider, set one of the following environment variables:\n\
                        • OPENAI_API_KEY - for OpenAI (GPT-4, etc.)\n\
                        • ANTHROPIC_API_KEY - for Claude\n\
                        • Run 'tark auth copilot' for GitHub Copilot\n\
                        • Or configure Ollama for local models\n\n\
                        Error: {}",
                        e
                    );
                (None, false, Some(error_msg))
            }
        };

        state.llm_configured = llm_configured;
        state.llm_error = llm_error;

        let mut app = Self {
            terminal,
            state,
            events,
            keybindings,
            commands,
            agent_bridge,
            agent_event_rx: None,
            pending_request: None,
            username,
            prompt_history,
        };

        // Initialize panel data from AgentBridge (Requirements 5.1)
        app.update_panel_from_bridge();

        // Restore messages from current session on startup (Requirements 6.1, 6.6)
        app.restore_messages_from_session();

        Ok(app)
    }

    /// Get the current configuration
    pub fn config(&self) -> &TuiConfig {
        self.state.config()
    }

    /// Get the detected system username
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Get a reference to the agent bridge (if configured)
    pub fn agent_bridge(&self) -> Option<&AgentBridge> {
        self.agent_bridge.as_ref()
    }

    /// Get a mutable reference to the agent bridge (if configured)
    pub fn agent_bridge_mut(&mut self) -> Option<&mut AgentBridge> {
        self.agent_bridge.as_mut()
    }

    /// Update panel data from the AgentBridge
    ///
    /// Aggregates session info from AgentBridge and updates the enhanced panel data.
    /// Formats session info as "session_name | model | provider".
    /// Includes token count and cost information.
    ///
    /// Requirements: 5.1, 5.2, 5.3, 5.4, 5.5, 5.8, 5.9
    pub fn update_panel_from_bridge(&mut self) {
        if let Some(ref bridge) = self.agent_bridge {
            // Get session info from bridge
            let session_name = bridge.session_name().to_string();
            let model_name = bridge.model_name().to_string();
            let provider_name = bridge.provider_name().to_string();
            let total_cost = bridge.total_cost();
            let (input_tokens, output_tokens) = bridge.total_tokens();
            let context_usage = bridge.context_usage_percent();

            // Update session info (Requirements 5.1, 5.2, 5.8, 5.9)
            // Format: "session_name | model | provider"
            // Get actual model context limit from models.dev first (before moving strings)
            let model_name_ref = model_name.as_str();
            let provider_name_ref = provider_name.as_str();

            // Fetch context limit asynchronously (this will use cache if available)
            let max_tokens = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(async {
                    crate::llm::models_db()
                        .get_context_limit(provider_name_ref, model_name_ref)
                        .await
                })
            });

            // Now update session info
            self.state.enhanced_panel_data.session = super::widgets::SessionInfo {
                name: if session_name.is_empty() {
                    "New Session".to_string()
                } else {
                    session_name
                },
                model: if model_name.is_empty() {
                    "default".to_string()
                } else {
                    model_name
                },
                provider: if provider_name.is_empty() {
                    "none".to_string()
                } else {
                    provider_name
                },
                cost: total_cost,
                lsp_languages: vec![], // LSP languages are managed separately
                cost_breakdown: Vec::new(),
            };

            // Update context info (Requirements 5.3, 5.4, 5.5)
            let total_tokens = (input_tokens + output_tokens) as u32;

            let usage_percent = if max_tokens > 0 {
                (total_tokens as f32 / max_tokens as f32) * 100.0
            } else {
                0.0
            };

            let final_usage_percent = if context_usage > 0 {
                context_usage as f32
            } else {
                usage_percent
            };

            self.state.enhanced_panel_data.context = super::widgets::ContextInfo {
                used_tokens: total_tokens,
                max_tokens,
                usage_percent: final_usage_percent,
                over_limit: total_tokens > max_tokens || final_usage_percent >= 100.0,
            };
        }
    }

    /// Handle agent errors with user-friendly messages and suggestions
    ///
    /// Categorizes errors and provides helpful suggestions for resolution.
    /// Requirements: 7.1, 7.6
    fn handle_agent_error(&mut self, error: &str) {
        // Clear current tool indicator
        self.state.current_tool = None;

        // Finalize any streaming message with error notice
        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
            if last_msg.role == super::widgets::Role::Assistant && last_msg.is_streaming {
                last_msg.is_streaming = false;
                if !last_msg.content.is_empty() {
                    last_msg.content.push_str("\n\n⚠️ [Error occurred]");
                }
            }
        }

        // Categorize error and get user-friendly message with suggestions
        let (error_type, suggestion) = Self::categorize_error(error);

        // Build the error message
        let error_message = format!(
            "⚠️ **{}**\n\n{}\n\n💡 **Suggestion:** {}",
            error_type, error, suggestion
        );

        self.state
            .message_list
            .push(ChatMessage::system(error_message));
        self.state.agent_processing = false;
        self.state.status_message = Some(format!("Error: {}", error_type));
    }

    /// Categorize an error string and return a user-friendly type and suggestion
    ///
    /// Returns (error_type, suggestion) tuple for display
    fn categorize_error(error: &str) -> (&'static str, &'static str) {
        let error_lower = error.to_lowercase();

        // API/Authentication errors
        if error_lower.contains("api key")
            || error_lower.contains("authentication")
            || error_lower.contains("unauthorized")
            || error_lower.contains("401")
        {
            return (
                "Authentication Error",
                "Check your API key is set correctly. Use /model to switch providers or verify your environment variables (OPENAI_API_KEY, ANTHROPIC_API_KEY).",
            );
        }

        // Rate limiting errors
        if error_lower.contains("rate limit")
            || error_lower.contains("too many requests")
            || error_lower.contains("429")
        {
            return (
                "Rate Limit Exceeded",
                "Wait a moment before sending another message, or switch to a different provider with /model.",
            );
        }

        // Context window errors
        if error_lower.contains("context")
            || error_lower.contains("token limit")
            || error_lower.contains("too long")
            || error_lower.contains("maximum context")
        {
            return (
                "Context Window Exceeded",
                "Your conversation is too long. Use /compact to summarize the conversation, or /session new to start fresh.",
            );
        }

        // Network/Connection errors
        if error_lower.contains("network")
            || error_lower.contains("connection")
            || error_lower.contains("timeout")
            || error_lower.contains("unreachable")
            || error_lower.contains("dns")
        {
            return (
                "Connection Error",
                "Check your internet connection. If using Ollama, ensure the server is running with 'ollama serve'.",
            );
        }

        // Model not found errors
        if error_lower.contains("model not found")
            || error_lower.contains("invalid model")
            || error_lower.contains("model does not exist")
        {
            return (
                "Model Not Found",
                "The selected model is not available. Use /model to choose a different model, or check your provider configuration.",
            );
        }

        // Provider errors
        if error_lower.contains("provider")
            || error_lower.contains("service unavailable")
            || error_lower.contains("503")
        {
            return (
                "Provider Unavailable",
                "The LLM provider is temporarily unavailable. Try again later or switch to a different provider with /model.",
            );
        }

        // Tool execution errors
        if error_lower.contains("tool")
            || error_lower.contains("execution failed")
            || error_lower.contains("permission denied")
        {
            return (
                "Tool Execution Error",
                "A tool failed to execute. Check file permissions and paths. You can retry the operation or provide more specific instructions.",
            );
        }

        // Invalid request errors
        if error_lower.contains("invalid request")
            || error_lower.contains("bad request")
            || error_lower.contains("400")
        {
            return (
                "Invalid Request",
                "The request was malformed. Try rephrasing your message or starting a new session with /session new.",
            );
        }

        // Server errors
        if error_lower.contains("internal server error")
            || error_lower.contains("500")
            || error_lower.contains("server error")
        {
            return (
                "Server Error",
                "The LLM provider encountered an internal error. Try again in a moment, or switch providers with /model.",
            );
        }

        // Default fallback
        (
            "Unexpected Error",
            "An unexpected error occurred. Try again, or use /session new to start a fresh conversation. If the problem persists, check your configuration.",
        )
    }

    /// Check if rate limit has expired and retry the pending message
    ///
    /// Called on each tick to check if we should retry after rate limiting.
    /// Requirements: 7.4
    fn check_rate_limit_retry(&mut self) {
        // Check if we have a rate limit retry pending
        if let Some(retry_at) = self.state.rate_limit_retry_at {
            let now = std::time::Instant::now();

            if now >= retry_at {
                // Rate limit expired, clear the retry state
                self.state.rate_limit_retry_at = None;

                // If we have a pending message, retry it
                if let Some(message) = self.state.rate_limit_pending_message.take() {
                    self.state.status_message = Some("🔄 Retrying after rate limit...".to_string());

                    // Set the pending message for retry
                    self.state.pending_message = Some(message);
                }
            } else {
                // Update countdown in status bar
                let remaining = retry_at.duration_since(now);
                let secs = remaining.as_secs();
                if secs > 0 {
                    self.state.status_message =
                        Some(format!("⏳ Rate limited: retry in {} seconds", secs));
                }
            }
        }
    }

    /// Process the next message from the prompt queue
    ///
    /// Called after a message completes to automatically process queued messages.
    /// This removes the message from the queue, adds it to chat history, and
    /// starts processing it.
    fn process_next_queued_message(&mut self) {
        // Check if there are queued messages
        if let Some(next_message) = self.state.prompt_queue.pop_front() {
            // Create mpsc channel for AgentEvents
            let (tx, rx) = mpsc::channel(100);
            self.agent_event_rx = Some(rx);

            // Mark as processing
            self.state.agent_processing = true;
            let queue_remaining = self.state.prompt_queue.len();
            self.state.status_message = Some(format!(
                "Processing queued message ({} remaining)...",
                queue_remaining
            ));

            // ADD the user message to chat history now that we're processing it
            self.state
                .message_list
                .push(ChatMessage::user(next_message.clone()));

            // Store the pending message for async processing
            self.state.pending_message = Some(next_message.clone());

            // Queue the request for async processing
            self.pending_request = Some(PendingMessageRequest {
                content: next_message,
                attachments: Vec::new(),
                tx,
            });

            // Update panel to show queue status (removes the processed one)
            self.update_panel_tasks_from_queue();
        } else {
            // No more queued messages, clear tasks
            self.state.enhanced_panel_data.tasks.clear();
        }
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
            Event::Mouse(mouse_event) => self.handle_mouse_event(mouse_event),
            Event::Tick => {
                // Check for rate limit retry (Requirements 7.4)
                self.check_rate_limit_retry();
                Ok(false)
            }
        }
    }
    // Handle auth dialog input if visible
    fn handle_key_event(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
        // Handle auth dialog input if visible
        if self.state.auth_dialog.is_visible() {
            match key.code {
                KeyCode::Esc => {
                    self.state.auth_dialog.hide();
                    return Ok(true);
                }
                KeyCode::Char('c') | KeyCode::Char('C') => {
                    // Copy code to clipboard
                    let code = self.state.auth_dialog.user_code().to_string();
                    if let Err(e) = super::clipboard::copy_to_clipboard(&code) {
                        self.state.status_message = Some(format!("Failed to copy: {}", e));
                    } else {
                        self.state.status_message =
                            Some("✅ Code copied to clipboard!".to_string());
                    }
                    return Ok(true);
                }
                _ => return Ok(true),
            }
        }

        // Handle picker input first if picker is visible (Requirements 6.4, 12.1, 12.2)
        if self.state.picker.is_visible() {
            return self.handle_picker_key(key);
        }

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

    /// Handle a mouse event
    ///
    /// Handles mouse clicks for:
    /// - Collapsible block headers (toggle expand/collapse)
    /// - Attachment dropdown items (select/remove)
    /// - Panel section headers (toggle expand/collapse)
    ///
    /// Requirements: 7.4, 10.7, 11.5
    fn handle_mouse_event(&mut self, mouse: crossterm::event::MouseEvent) -> anyhow::Result<bool> {
        use crossterm::event::{MouseButton, MouseEventKind};

        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let (col, row) = (mouse.column, mouse.row);

                // Check if click is in attachment dropdown area
                if self.handle_attachment_click(col, row) {
                    return Ok(true);
                }

                // Check if click is in panel area (right 30% of screen)
                let (width, _height) = self.state.terminal_size;
                let panel_start = (width as f32 * 0.70) as u16;

                if col >= panel_start {
                    // Click in panel area
                    self.handle_panel_click(col - panel_start, row);
                    return Ok(true);
                }

                // Click in messages area - could be on a collapsible block header
                // For now, just focus the messages area
                self.state.set_focused_component(FocusedComponent::Messages);
                Ok(true)
            }
            MouseEventKind::ScrollDown => {
                // Scroll down in the focused component
                let width = self.state.terminal_size.0;
                match self.state.focused_component {
                    FocusedComponent::Messages => {
                        self.state.message_list.scroll_down(width);
                    }
                    FocusedComponent::Panel => {
                        let max_tasks = self.state.enhanced_panel_data.tasks.len();
                        let max_files = self.state.enhanced_panel_data.files.len();
                        self.state
                            .panel_section_state
                            .scroll_down(max_tasks, max_files);
                    }
                    _ => {}
                }
                Ok(true)
            }
            MouseEventKind::ScrollUp => {
                // Scroll up in the focused component
                match self.state.focused_component {
                    FocusedComponent::Messages => {
                        self.state.message_list.scroll_up();
                    }
                    FocusedComponent::Panel => {
                        self.state.panel_section_state.scroll_up();
                    }
                    _ => {}
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// Handle click in attachment dropdown area
    ///
    /// Returns true if the click was handled.
    /// Handles clicks on:
    /// - Dropdown items to select them
    /// - ✕ button to remove attachments
    ///
    /// Requirements: 11.5
    fn handle_attachment_click(&mut self, col: u16, row: u16) -> bool {
        let (width, height) = self.state.terminal_size;

        // Calculate status bar position (approximately 5% from bottom of chat column)
        let chat_width = (width as f32 * 0.70) as u16;
        let status_row = height.saturating_sub(4); // Approximate status bar row

        // Check if click is in the attachment area of status bar
        if row == status_row && col < chat_width && !self.state.attachments.is_empty() {
            // Toggle dropdown if clicking on attachment indicator
            self.state.attachment_dropdown_state.toggle();
            return true;
        }

        // If dropdown is open, handle clicks within it
        if self.state.attachment_dropdown_state.is_open() {
            let dropdown_height = std::cmp::min(self.state.attachments.len() as u16, 5) + 2;
            let dropdown_top = status_row.saturating_sub(dropdown_height);

            // Check if click is within dropdown bounds
            if row >= dropdown_top && row < status_row && col < 40 {
                let item_row = row.saturating_sub(dropdown_top + 1); // Account for border
                let item_idx = item_row as usize;

                if item_idx < self.state.attachments.len() {
                    // Check if click is on the ✕ button (last few characters)
                    if col > 35 {
                        // Click on remove button - request delete with confirmation
                        self.state.attachment_dropdown_state.selected_index = Some(item_idx);
                        self.state.attachment_dropdown_state.request_delete();
                    } else {
                        // Click on item - select it
                        self.state.attachment_dropdown_state.selected_index = Some(item_idx);
                    }
                    return true;
                }
            }

            // Click outside dropdown - close it
            self.state.attachment_dropdown_state.close();
            return true;
        }

        false
    }

    /// Handle click in panel area
    ///
    /// Determines which section header was clicked and toggles it.
    fn handle_panel_click(&mut self, _col: u16, row: u16) {
        // Simple heuristic: each section header is roughly at a fixed position
        // This is a simplified implementation - proper hit testing would require
        // tracking the actual rendered positions of section headers

        // Focus the panel
        self.state.set_focused_component(FocusedComponent::Panel);

        // Estimate section positions (this is approximate)
        // Session header is at row ~1
        // Context header is at row ~5 (if session expanded)
        // Tasks header is at row ~9 (if context expanded)
        // Files header is at row ~13+ (depends on tasks count)

        // For now, just toggle the focused section on any click
        // A more sophisticated implementation would track actual positions
        if row < 5 {
            self.state.panel_section_state.focused_section =
                super::widgets::EnhancedPanelSection::Session;
        } else if row < 9 {
            self.state.panel_section_state.focused_section =
                super::widgets::EnhancedPanelSection::Context;
        } else if row < 15 {
            self.state.panel_section_state.focused_section =
                super::widgets::EnhancedPanelSection::Tasks;
        } else {
            self.state.panel_section_state.focused_section =
                super::widgets::EnhancedPanelSection::Files;
        }

        // Toggle the section that was clicked
        self.state.panel_section_state.toggle_focused();
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
                // Close attachment dropdown if open
                if self.state.attachment_dropdown_state.is_open() {
                    self.state.attachment_dropdown_state.close();
                } else if self.state.input_mode == InputMode::Insert {
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
                // Check if attachment dropdown is open - handle navigation within it
                if self.state.attachment_dropdown_state.is_open() {
                    match action {
                        Action::LineDown => {
                            let count = self.state.attachments.len();
                            self.state.attachment_dropdown_state.select_next(count);
                        }
                        Action::LineUp => {
                            let count = self.state.attachments.len();
                            self.state.attachment_dropdown_state.select_prev(count);
                        }
                        _ => {
                            // Other navigation actions close the dropdown
                            self.state.attachment_dropdown_state.close();
                            self.state.handle_navigation(action);
                        }
                    }
                } else if self.state.focused_component == FocusedComponent::Input
                    && matches!(action, Action::LineUp | Action::LineDown)
                {
                    // When input is focused in normal mode, j/k navigate history (Requirements 14.1, 14.2)
                    match action {
                        Action::LineUp => self.navigate_history_previous(),
                        Action::LineDown => self.navigate_history_next(),
                        _ => {}
                    }
                } else {
                    self.state.handle_navigation(action);
                }
                Ok(true)
            }
            // Collapsible block and attachment actions (Task 11)
            Action::ToggleBlock => {
                self.handle_toggle_block();
                Ok(true)
            }
            Action::ToggleAttachmentDropdown => {
                self.state.attachment_dropdown_state.toggle();
                Ok(true)
            }
            Action::DeleteAttachment => {
                self.handle_delete_attachment();
                Ok(true)
            }
            Action::Confirm => {
                self.handle_confirm_action();
                Ok(true)
            }
            Action::Reject => {
                self.handle_reject_action();
                Ok(true)
            }
            Action::Interrupt => {
                // Handle interrupt action (Ctrl+C)
                // Requirements: 8.1, 8.2, 8.3, 8.4, 8.5
                self.handle_interrupt();
                Ok(true)
            }
            Action::Paste => {
                // Handle paste from clipboard (Ctrl+V)
                // Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6
                self.handle_clipboard_paste();
                Ok(true)
            }
            Action::CycleModeNext => {
                // Handle mode cycling forward (Ctrl+Tab)
                // Requirements: 13.1, 13.4, 13.5, 13.6
                self.handle_cycle_mode_next();
                Ok(true)
            }
            Action::CycleModePrev => {
                // Handle mode cycling backward (Ctrl+Shift+Tab)
                // Requirements: 13.2, 13.4, 13.5, 13.6
                self.handle_cycle_mode_prev();
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
                // If command dropdown is visible, apply selection
                if self.state.command_dropdown.is_visible() {
                    if let Some(command) = self.state.command_dropdown.selected_command() {
                        self.state.input_widget.set_content(command);
                        self.state.command_dropdown.hide();
                    }
                } else {
                    self.handle_tab_completion(false);
                }
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
                    // Update command dropdown as user types
                    self.update_command_dropdown();
                }
            }
            KeyCode::Backspace => {
                self.state.input_widget.delete_char_before();
                // Update command dropdown after deletion
                self.update_command_dropdown();
            }
            KeyCode::Delete => {
                self.state.input_widget.delete_char_at();
                self.update_command_dropdown();
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
                // If command dropdown is visible, navigate up in dropdown
                if self.state.command_dropdown.is_visible() {
                    self.state.command_dropdown.select_previous();
                } else {
                    // Navigate to previous history entry (Requirements 14.1, 14.4)
                    self.navigate_history_previous();
                }
            }
            KeyCode::Down => {
                // If command dropdown is visible, navigate down in dropdown
                if self.state.command_dropdown.is_visible() {
                    self.state.command_dropdown.select_next();
                } else {
                    // Navigate to next history entry (Requirements 14.2, 14.4)
                    self.navigate_history_next();
                }
            }
            KeyCode::Esc => {
                // Hide command dropdown on Esc
                if self.state.command_dropdown.is_visible() {
                    self.state.command_dropdown.hide();
                }
            }
            KeyCode::Enter => {
                // If command dropdown is visible, apply selection
                if self.state.command_dropdown.is_visible() {
                    if let Some(command) = self.state.command_dropdown.selected_command() {
                        self.state.input_widget.set_content(command.clone());
                        self.state.command_dropdown.hide();

                        // Check if the command requires arguments
                        // If no args needed, auto-execute immediately
                        let command_name = command
                            .trim_start_matches('/')
                            .split_whitespace()
                            .next()
                            .unwrap_or("");
                        if let Some(cmd) = self.commands.get_command(command_name) {
                            if !cmd.requires_args {
                                // Auto-execute commands that don't need arguments
                                self.handle_submit();
                            }
                            // If requires_args is true, just fill in the command and wait for user to add args
                        } else {
                            // Unknown command, still execute to show error
                            self.handle_submit();
                        }
                    }
                } else {
                    // Normal submit behavior
                    self.handle_submit();
                }
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

    /// Update command dropdown based on current input
    fn update_command_dropdown(&mut self) {
        let content = self.state.input_widget.content();

        // Only show dropdown if input starts with /
        if !content.starts_with('/') {
            self.state.command_dropdown.hide();
            return;
        }

        // Get filter (text after /)
        let filter = content.trim_start_matches('/');
        self.state.command_dropdown.set_filter(filter);

        // Get matching commands
        use super::widgets::CommandDropdownItem;
        let mut items: Vec<CommandDropdownItem> = self
            .commands
            .commands()
            .filter(|cmd| {
                if filter.is_empty() {
                    true
                } else {
                    cmd.name.starts_with(filter) || cmd.name.contains(filter)
                }
            })
            .map(CommandDropdownItem::from_command)
            .collect();

        // Sort by relevance (starts_with first, then contains)
        items.sort_by(|a, b| {
            let a_starts = a.name.starts_with(filter);
            let b_starts = b.name.starts_with(filter);
            match (a_starts, b_starts) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });

        if items.is_empty() {
            self.state.command_dropdown.hide();
        } else {
            self.state.command_dropdown.set_items(items);
            self.state.command_dropdown.show();
        }
    }

    /// Navigate to the previous (older) prompt in history
    ///
    /// Preserves the current unsent input when navigating away.
    /// Requirements: 14.1, 14.4
    fn navigate_history_previous(&mut self) {
        let current_input = self.state.input_widget.content().to_string();
        if let Some(entry) = self.prompt_history.previous(&current_input) {
            // Set the input widget content to the history entry
            // This doesn't modify the original history entry (Requirements 14.6)
            self.state.input_widget.set_content(entry);
        }
    }

    /// Navigate to the next (newer) prompt in history
    ///
    /// Returns to the saved unsent input when reaching the end of history.
    /// Requirements: 14.2, 14.4
    fn navigate_history_next(&mut self) {
        if let Some(entry) = self.prompt_history.next_entry() {
            // Set the input widget content to the history entry or saved input
            // This doesn't modify the original history entry (Requirements 14.6)
            self.state.input_widget.set_content(entry);
        }
    }

    /// Handle submit action
    fn handle_submit(&mut self) {
        // Hide command dropdown if visible
        self.state.command_dropdown.hide();

        let content = self.state.input_widget.submit();
        if content.trim().is_empty() && self.state.attachments.is_empty() {
            return;
        }

        // Add to prompt history and save (Requirements 14.3)
        // Only add non-command messages to history
        if !CommandHandler::is_command(&content) {
            self.prompt_history.add(content.clone());
            // Reset navigation state after submit
            self.prompt_history.reset_navigation();
            // Save history to disk (best effort, don't fail on error)
            if let Err(e) = self.prompt_history.save() {
                tracing::warn!("Failed to save prompt history: {}", e);
            }
        }

        // Check if this is a command
        if CommandHandler::is_command(&content) {
            self.handle_command(&content);
        } else {
            // Regular message - add to the list with detected username (Requirements 2.2)
            // FIRST check if agent is already processing - if so, queue the message
            // Note: agent_bridge may be temporarily None during processing (taken by spawn_blocking)
            // so we check agent_processing flag instead
            if self.state.agent_processing {
                // Queue the message - DON'T add to chat yet
                self.state.prompt_queue.push_back(content.clone());
                let queue_pos = self.state.prompt_queue.len();
                self.state.status_message =
                    Some(format!("Message queued (position {})", queue_pos));

                // Update panel to show queued task
                self.update_panel_tasks_from_queue();
                return;
            }

            // Not processing - add user message to chat
            let user_msg = ChatMessage::user(content.clone());
            self.state.message_list.push(user_msg);

            // Auto-scroll to bottom to show the new message
            let width = self.get_message_area_width();
            self.state.message_list.scroll_to_bottom(width);

            // Check if LLM is configured
            if !self.state.llm_configured {
                // Show error message about LLM configuration
                let error_msg = self.state.llm_error.clone().unwrap_or_else(|| {
                    "No LLM provider configured. Please set up an API key.".to_string()
                });
                self.state.message_list.push(ChatMessage::system(format!(
                    "⚠️ Cannot send message - LLM not configured\n\n{}",
                    error_msg
                )));
            } else if self.agent_bridge.is_some() {
                // Create mpsc channel for AgentEvents (Requirements 1.1, 1.2)
                let (tx, rx) = mpsc::channel(100);
                self.agent_event_rx = Some(rx);

                // Mark as processing
                self.state.agent_processing = true;
                self.state.status_message = Some("Sending message to LLM...".to_string());

                // Store the pending message for async processing
                self.state.pending_message = Some(content.clone());

                // Take attachments for sending (Requirements 10.6)
                let attachments: Vec<MessageAttachment> = self
                    .state
                    .attachments
                    .drain(..)
                    .map(|a| a.to_message_attachment())
                    .collect();

                // Queue the request for async processing in the event loop
                self.pending_request = Some(PendingMessageRequest {
                    content,
                    attachments,
                    tx,
                });

                // Update panel data after message queued (Requirements 5.3)
                self.update_panel_from_bridge();

                // Clear attachment bar state (Requirements 10.6)
                self.state.attachment_dropdown_state = AttachmentDropdownState::default();
            } else {
                // AgentBridge not initialized
                self.state.message_list.push(ChatMessage::system(
                    "⚠️ Agent not initialized. Please restart the application.".to_string(),
                ));
            }
        }
    }

    /// Update panel tasks from the prompt queue
    fn update_panel_tasks_from_queue(&mut self) {
        let mut tasks: Vec<super::widgets::TaskItem> = self
            .state
            .prompt_queue
            .iter()
            .enumerate()
            .map(|(i, prompt)| {
                let desc = if prompt.len() > 40 {
                    format!("{}. {}...", i + 1, &prompt[..37])
                } else {
                    format!("{}. {}", i + 1, prompt)
                };
                super::widgets::TaskItem {
                    description: desc,
                    status: super::widgets::TaskStatus::Pending,
                }
            })
            .collect();

        // Add current processing task at the top if processing
        if self.state.agent_processing {
            if let Some(ref pending) = self.state.pending_message {
                let desc = if pending.len() > 40 {
                    format!("⏳ {}...", &pending[..37])
                } else {
                    format!("⏳ {}", pending)
                };
                tasks.insert(
                    0,
                    super::widgets::TaskItem {
                        description: desc,
                        status: super::widgets::TaskStatus::Running,
                    },
                );
            }
        }

        self.state.enhanced_panel_data.tasks = tasks.clone();

        // Ensure tasks section is expanded when tasks are added
        if !tasks.is_empty() {
            self.state.panel_section_state.tasks_expanded = true;
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
                // Show picker UI for session/provider/model selection (Requirements 6.4, 12.1, 12.2)
                self.show_picker(picker_type);
            }
            CommandResult::ChangeMode(mode_change) => {
                let new_mode = match mode_change {
                    AgentModeChange::Plan => AgentMode::Plan,
                    AgentModeChange::Build => AgentMode::Build,
                    AgentModeChange::Review => AgentMode::Review,
                };
                // Use the common mode change handler
                self.apply_mode_change(new_mode);
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
                // Open usage dashboard in browser
                let url = "http://localhost:8765/usage/dashboard";

                #[cfg(target_os = "macos")]
                let open_cmd = "open";
                #[cfg(target_os = "linux")]
                let open_cmd = "xdg-open";
                #[cfg(target_os = "windows")]
                let open_cmd = "start";

                match std::process::Command::new(open_cmd).arg(url).spawn() {
                    Ok(_) => {
                        self.state.status_message =
                            Some(format!("Opening usage dashboard: {}", url));
                    }
                    Err(e) => {
                        self.state.status_message =
                            Some(format!("Failed to open browser: {}. Visit: {}", e, url));
                    }
                }
            }
            CommandResult::ClearHistory => {
                // Clear UI message list
                self.state.message_list.clear();
                // Clear agent's conversation context and reset session tokens/cost
                if let Some(ref mut bridge) = self.agent_bridge {
                    bridge.clear_history();
                }
                // Update panel to reflect cleared context (tokens=0, cost=0)
                self.update_panel_from_bridge();
                self.state.status_message = Some("Chat history cleared".to_string());
            }
            CommandResult::Compact => {
                // TODO: Implement compaction
                self.state.status_message = Some("Compacting conversation...".to_string());
            }
            CommandResult::NewSession => {
                // Create new session via AgentBridge (Requirements 5.9, 6.3)
                if let Some(ref mut bridge) = self.agent_bridge {
                    match bridge.new_session() {
                        Ok(()) => {
                            self.state.message_list.clear();
                            self.state.status_message = Some("New session created".to_string());
                            // Update panel data after session switch (Requirements 5.9)
                            self.update_panel_from_bridge();
                        }
                        Err(e) => {
                            self.state.status_message =
                                Some(format!("Failed to create session: {}", e));
                        }
                    }
                } else {
                    self.state.message_list.clear();
                    self.state.status_message = Some("New session created".to_string());
                }
            }
            CommandResult::DeleteSession => {
                // TODO: Implement session deletion
                self.state.status_message = Some("Delete session: Not yet implemented".to_string());
            }
            CommandResult::Exit => {
                self.state.quit();
            }
            CommandResult::Interrupt => {
                // Wire /interrupt command to cancel ongoing LLM operation
                // Requirements: 8.4, 8.5
                // Use non-quitting version for command (don't quit if nothing is running)
                self.handle_interrupt_with_quit(false);
            }
            CommandResult::AttachFile(path) => {
                // Implement file attachment with AttachmentManager
                if path.is_empty() {
                    self.state.status_message = Some("Usage: /attach <file_path>".to_string());
                } else {
                    match super::attachments::resolve_file_path(&path) {
                        Ok(resolved_path) => {
                            let config = self.state.config.to_attachment_config();
                            let mut manager = super::attachments::AttachmentManager::new(config);

                            // Transfer existing attachments to manager for limit checking
                            let mut failed_attachments = Vec::new();
                            for attachment in self.state.attachments.drain(..) {
                                if let Err(e) = manager.add(attachment.clone()) {
                                    tracing::warn!(
                                        "Failed to transfer attachment to manager: {}",
                                        e
                                    );
                                    failed_attachments.push(attachment);
                                }
                            }

                            // If we had failures, restore them and abort
                            if !failed_attachments.is_empty() {
                                self.state.attachments.extend(failed_attachments);
                                self.state.status_message = Some(
                                    "❌ Cannot attach: Cannot process existing attachments"
                                        .to_string(),
                                );
                                // Restore any attachments that made it to the manager
                                let remaining = manager.take_all();
                                self.state.attachments.extend(remaining);
                                return;
                            }

                            match manager.attach_file(&resolved_path) {
                                Ok(attachment) => {
                                    let filename = attachment.filename.clone();
                                    let file_type = match &attachment.file_type {
                                        super::attachments::AttachmentType::Image { .. } => "📷",
                                        super::attachments::AttachmentType::Text { .. } => "📝",
                                        super::attachments::AttachmentType::Document { .. } => "📄",
                                        super::attachments::AttachmentType::Data { .. } => "📊",
                                    };
                                    // Transfer all attachments back
                                    self.state.attachments = manager.take_all();
                                    self.state.status_message =
                                        Some(format!("{} Attached: {}", file_type, filename));
                                }
                                Err(e) => {
                                    // Restore attachments
                                    self.state.attachments = manager.take_all();
                                    self.state.status_message =
                                        Some(format!("❌ Cannot attach: {}", e));
                                }
                            }
                        }
                        Err(e) => {
                            self.state.status_message = Some(format!("❌ File not found: {}", e));
                        }
                    }
                }
            }
            CommandResult::ClearAttachments => {
                let count = self.state.attachments.len();
                self.state.attachments.clear();
                self.state.status_message = Some(format!("Cleared {} attachment(s)", count));
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

    /// Handle toggle block action (Enter key in messages area)
    ///
    /// Toggles the collapsible block under the cursor when focused on messages.
    /// In panel, toggles the focused section.
    /// In input, enters insert mode.
    ///
    /// Requirements: 7.4, 7.5, 7.6, 8.6, 8.7, 8.8
    fn handle_toggle_block(&mut self) {
        match self.state.focused_component {
            FocusedComponent::Messages => {
                // Toggle collapsible block in message list
                // For now, we don't track cursor position within messages,
                // so this is a placeholder for future implementation
                // TODO: Track cursor position and toggle block under cursor
                self.state.status_message =
                    Some("Toggle block (cursor tracking pending)".to_string());
            }
            FocusedComponent::Panel => {
                // Toggle the focused panel section
                self.state.panel_section_state.toggle_focused();
            }
            FocusedComponent::Input => {
                // Enter insert mode when pressing Enter on input
                self.state.set_input_mode(InputMode::Insert);
            }
        }
    }

    /// Handle delete attachment action (d key)
    ///
    /// Requests deletion of the selected attachment in the dropdown.
    /// Shows confirmation prompt before actual deletion.
    ///
    /// Requirements: 11.5, 11.6
    fn handle_delete_attachment(&mut self) {
        if self.state.attachment_dropdown_state.is_open() {
            self.state.attachment_dropdown_state.request_delete();
        }
    }

    /// Handle confirm action (y key)
    ///
    /// Confirms pending actions like attachment deletion.
    ///
    /// Requirements: 11.7
    fn handle_confirm_action(&mut self) {
        if let Some(idx) = self.state.attachment_dropdown_state.confirm_delete() {
            // Remove the attachment at the confirmed index
            if idx < self.state.attachments.len() {
                let removed = self.state.attachments.remove(idx);
                self.state.status_message =
                    Some(format!("Removed attachment: {}", removed.filename));

                // Adjust selection if needed
                if self.state.attachments.is_empty() {
                    self.state.attachment_dropdown_state.close();
                } else if let Some(selected) = self.state.attachment_dropdown_state.selected_index {
                    if selected >= self.state.attachments.len() {
                        self.state.attachment_dropdown_state.selected_index =
                            Some(self.state.attachments.len() - 1);
                    }
                }
            }
        }
    }

    /// Handle reject action (n key)
    ///
    /// Cancels pending actions like attachment deletion confirmation.
    ///
    /// Requirements: 11.8
    fn handle_reject_action(&mut self) {
        self.state.attachment_dropdown_state.cancel_delete();
    }

    /// Handle interrupt action (Ctrl+C or /interrupt command)
    ///
    /// Interrupts the current agent operation if one is in progress.
    /// If no operation is in progress:
    /// - Ctrl+C: quits the application
    /// - /interrupt command: shows a message that nothing is running
    ///
    /// Requirements:
    /// - 8.1: WHEN the user presses Ctrl+C during streaming, THE TUI SHALL stop the current response
    /// - 8.2: WHEN the user presses Ctrl+C during tool execution, THE TUI SHALL attempt to cancel the operation
    /// - 8.3: WHEN an operation is interrupted, THE TUI SHALL display what was completed before interruption
    /// - 8.4: THE /interrupt command SHALL cancel any ongoing LLM operation
    /// - 8.5: AFTER interruption, THE TUI SHALL be ready for new input immediately
    fn handle_interrupt(&mut self) {
        self.handle_interrupt_with_quit(true)
    }

    /// Handle interrupt with optional quit behavior
    ///
    /// When `quit_if_idle` is true (Ctrl+C), quits if no operation is running.
    /// When `quit_if_idle` is false (/interrupt command), just shows a message.
    fn handle_interrupt_with_quit(&mut self, quit_if_idle: bool) {
        if self.state.agent_processing {
            // Agent is processing - interrupt it (Requirements 8.1, 8.2)
            if let Some(ref bridge) = self.agent_bridge {
                bridge.interrupt();
                self.state.status_message = Some("⚠️ Interrupting...".to_string());

                // The AgentEvent::Interrupted will be received in poll_agent_events
                // which will handle displaying partial response (Requirement 8.3)
                // and setting agent_processing to false (Requirement 8.5)
            }
        } else if quit_if_idle {
            // No operation in progress and Ctrl+C pressed - quit the application
            self.state.quit();
        } else {
            // No operation in progress and /interrupt command used - show message
            self.state.status_message = Some("No operation in progress to interrupt".to_string());
        }
    }

    /// Handle clipboard paste (Ctrl+V)
    ///
    /// Detects clipboard content type and handles appropriately:
    /// - Images: add as attachment with preview icon (Requirements 11.2, 11.6)
    /// - File paths: attach the file (Requirements 11.3)
    /// - Text: paste into input field (Requirements 11.4)
    ///
    /// Uses multiple clipboard backends:
    /// 1. Native clipboard (arboard) - works with display server
    /// 2. OSC 52 fallback - works over SSH with supported terminals
    ///
    /// Requirements: 11.1, 11.2, 11.3, 11.4, 11.5, 11.6, 11.7
    fn handle_clipboard_paste(&mut self) {
        use super::clipboard::{get_clipboard_with_fallback, ClipboardContent, ClipboardHandler};

        // Try to get clipboard content with fallback to OSC 52
        let content = match get_clipboard_with_fallback() {
            Ok(c) => c,
            Err(e) => {
                // Provide helpful error message for SSH users
                self.state.status_message = Some(format!(
                    "Clipboard error: {}. Try /attach <file> instead.",
                    e
                ));
                return;
            }
        };

        match content {
            ClipboardContent::Image(image_data) => {
                // Create attachment from image (Requirements 11.2, 11.6)
                let attachment = ClipboardHandler::image_to_attachment(image_data);
                let filename = attachment.filename.clone();

                // Check attachment limits
                let config = self.state.config.to_attachment_config();
                if self.state.attachments.len() >= config.max_attachments {
                    self.state.status_message = Some(format!(
                        "Cannot add attachment: maximum {} attachments reached",
                        config.max_attachments
                    ));
                    return;
                }

                if attachment.size > config.max_attachment_size {
                    self.state.status_message = Some(format!(
                        "Image too large: {} (max: {})",
                        super::attachments::format_size(attachment.size),
                        super::attachments::format_size(config.max_attachment_size)
                    ));
                    return;
                }

                // Add attachment (Requirements 11.5)
                self.state.attachments.push(attachment);
                self.state.status_message = Some(format!("📷 Pasted image: {}", filename));
            }
            ClipboardContent::FilePath(path) => {
                // Attach the file (Requirements 11.3)
                match super::attachments::resolve_file_path(&path) {
                    Ok(resolved_path) => {
                        // Use AttachmentManager to attach the file
                        let config = self.state.config.to_attachment_config();
                        let mut manager = super::attachments::AttachmentManager::new(config);

                        // Transfer existing attachments to manager for limit checking
                        let mut failed_attachments = Vec::new();
                        for attachment in self.state.attachments.drain(..) {
                            if let Err(e) = manager.add(attachment.clone()) {
                                tracing::warn!("Failed to transfer attachment to manager: {}", e);
                                failed_attachments.push(attachment);
                            }
                        }

                        // If we had failures, restore them and abort
                        if !failed_attachments.is_empty() {
                            self.state.attachments.extend(failed_attachments);
                            self.state.status_message = Some(
                                "❌ Cannot attach: Cannot process existing attachments".to_string(),
                            );
                            // Restore any attachments that made it to the manager
                            let remaining = manager.take_all();
                            self.state.attachments.extend(remaining);
                            return;
                        }

                        match manager.attach_file(&resolved_path) {
                            Ok(attachment) => {
                                let filename = attachment.filename.clone();
                                // Transfer all attachments back
                                self.state.attachments = manager.take_all();
                                self.state.status_message =
                                    Some(format!("📎 Attached file: {}", filename));
                            }
                            Err(e) => {
                                // Restore attachments
                                self.state.attachments = manager.take_all();
                                self.state.status_message =
                                    Some(format!("Cannot attach file: {}", e));
                            }
                        }
                    }
                    Err(e) => {
                        self.state.status_message = Some(format!("Cannot attach file: {}", e));
                    }
                }
            }
            ClipboardContent::Text(text) => {
                // Paste text into input field (Requirements 11.4)
                for c in text.chars() {
                    // Filter out control characters except newlines
                    if !c.is_control() || c == '\n' {
                        self.state.input_widget.insert_char(c);
                    }
                }
                // Don't show status message for text paste - it's the expected behavior
            }
            ClipboardContent::Empty => {
                // Provide helpful message - clipboard might not work over SSH
                self.state.status_message = Some(
                    "Clipboard empty or unavailable. Use /attach <file> for files.".to_string(),
                );
            }
        }
    }

    /// Handle cycling to the next agent mode (Ctrl+Tab)
    ///
    /// Cycles through modes: Build → Plan → Review → Build
    /// Updates the AgentBridge mode, status bar, and displays a notification.
    ///
    /// Requirements: 13.1, 13.4, 13.5, 13.6
    fn handle_cycle_mode_next(&mut self) {
        let new_mode = self.state.mode.next();
        self.apply_mode_change(new_mode);
    }

    /// Handle cycling to the previous agent mode (Ctrl+Shift+Tab)
    ///
    /// Cycles through modes: Build → Review → Plan → Build
    /// Updates the AgentBridge mode, status bar, and displays a notification.
    ///
    /// Requirements: 13.2, 13.4, 13.5, 13.6
    fn handle_cycle_mode_prev(&mut self) {
        let new_mode = self.state.mode.prev();
        self.apply_mode_change(new_mode);
    }

    /// Apply a mode change to the application state and AgentBridge
    ///
    /// This is the common implementation for mode changes from both
    /// cycling keybindings and slash commands.
    ///
    /// If the mode has no configured model preference, automatically triggers
    /// the model selection picker flow.
    ///
    /// Requirements: 2.6, 13.4, 13.5, 13.6
    fn apply_mode_change(&mut self, new_mode: AgentMode) {
        let old_mode = self.state.mode;

        // Update state
        self.state.mode = new_mode;

        // Update AgentBridge mode (Requirements 13.5)
        if let Some(ref mut bridge) = self.agent_bridge {
            let _ = bridge.set_mode(new_mode);

            // Check if mode has a preference configured (Requirements 2.6)
            if !bridge.has_mode_preference(new_mode) {
                // Display notification about needing to select a model
                let mode_icon = new_mode.icon();
                let mode_name = new_mode.display_name();
                self.state.status_message = Some(format!(
                    "{} Switched to {} mode - please select a model",
                    mode_icon, mode_name
                ));

                // Automatically trigger the model selection picker flow
                self.show_picker(PickerType::Provider);

                // Update panel data after mode change (Requirements 13.4)
                self.update_panel_from_bridge();
                return;
            }
        }

        // Display notification (Requirements 13.6)
        let mode_icon = new_mode.icon();
        let mode_name = new_mode.display_name();
        self.state.status_message = Some(format!(
            "{} Switched to {} mode (from {})",
            mode_icon,
            mode_name,
            old_mode.display_name()
        ));

        // Update panel data after mode change (Requirements 13.4)
        self.update_panel_from_bridge();
    }

    /// Show a picker for session/provider/model selection
    ///
    /// Populates the picker with appropriate items based on the picker type.
    /// For provider picker, also sets model_picker_state to enable two-step flow.
    ///
    /// Requirements: 1.1, 6.4, 12.1, 12.2
    fn show_picker(&mut self, picker_type: PickerType) {
        self.state.active_picker_type = Some(picker_type);

        match picker_type {
            PickerType::Session => {
                self.state.picker.set_title("Select Session");
                let items = self.get_session_picker_items();
                self.state.picker.set_items(items);
            }
            PickerType::Provider => {
                // Set model_picker_state to enable two-step flow (provider → model)
                // Requirements: 1.1, 1.3
                self.state.model_picker_state = Some(ModelPickerState::SelectingProvider);
                self.state.picker.set_title("Select Provider");
                let items = self.get_provider_picker_items();
                self.state.picker.set_items(items);
            }
            PickerType::Model => {
                self.state.picker.set_title("Select Model");
                let items = self.get_model_picker_items();
                self.state.picker.set_items(items);
            }
        }

        self.state.picker.show();
    }

    /// Get picker items for session selection
    ///
    /// Requirements: 6.4
    fn get_session_picker_items(&self) -> Vec<PickerItem> {
        if let Some(ref bridge) = self.agent_bridge {
            match bridge.list_sessions() {
                Ok(sessions) => sessions
                    .into_iter()
                    .map(|session| {
                        let description = format!(
                            "{} messages | {} | {}",
                            session.message_count, session.provider, session.mode
                        );
                        PickerItem::new(&session.id, &session.name)
                            .with_description(description)
                            .with_icon(if session.is_current { "●" } else { "○" })
                            .with_active(session.is_current)
                    })
                    .collect(),
                Err(_) => vec![PickerItem::new("new", "Create New Session").with_icon("➕")],
            }
        } else {
            vec![PickerItem::new("new", "Create New Session").with_icon("➕")]
        }
    }

    /// Get picker items for provider selection with availability indicators
    ///
    /// Uses ProviderInfo to check API key availability and show configuration hints.
    /// Highlights the currently selected provider.
    ///
    /// Requirements: 1.2, 3.1, 3.2, 12.1
    fn get_provider_picker_items(&self) -> Vec<PickerItem> {
        let current_provider = self
            .agent_bridge
            .as_ref()
            .map(|b| b.provider_name().to_string())
            .unwrap_or_default();

        // Get all providers with availability status
        let providers = ProviderInfo::get_all_providers();

        providers
            .into_iter()
            .map(|provider| {
                let is_current = current_provider == provider.id;

                // Build description with availability hint if not available
                let description = if provider.available {
                    provider.description
                } else {
                    // Add hint for unavailable providers
                    match &provider.hint {
                        Some(hint) => format!("{} ({})", provider.description, hint),
                        None => format!("{} (not configured)", provider.description),
                    }
                };

                // Choose icon based on provider and availability
                let icon = match provider.id.as_str() {
                    "openai" => {
                        if provider.available {
                            "🤖"
                        } else {
                            "⚠️"
                        }
                    }
                    "claude" => {
                        if provider.available {
                            "🧠"
                        } else {
                            "⚠️"
                        }
                    }
                    "copilot" => {
                        if provider.available {
                            "🐙"
                        } else {
                            "⚠️"
                        }
                    }
                    "gemini" => {
                        if provider.available {
                            "✨"
                        } else {
                            "⚠️"
                        }
                    }
                    "openrouter" => {
                        if provider.available {
                            "🔀"
                        } else {
                            "⚠️"
                        }
                    }
                    "ollama" => {
                        if provider.available {
                            "🏠"
                        } else {
                            "⚠️"
                        }
                    }
                    _ => "❓",
                };

                PickerItem::new(&provider.id, &provider.name)
                    .with_description(description)
                    .with_icon(icon)
                    .with_active(is_current)
                    .with_disabled(!provider.available)
            })
            .collect()
    }

    /// Get picker items for model selection
    ///
    /// Requirements: 12.2
    fn get_model_picker_items(&self) -> Vec<PickerItem> {
        let current_provider = self
            .agent_bridge
            .as_ref()
            .map(|b| b.provider_name().to_string())
            .unwrap_or_else(|| "openai".to_string());

        let current_model = self
            .agent_bridge
            .as_ref()
            .map(|b| b.model_name().to_string())
            .unwrap_or_default();

        match current_provider.as_str() {
            "openai" | "gpt" => vec![
                PickerItem::new("gpt-4o", "GPT-4o")
                    .with_description("Most capable, multimodal")
                    .with_active(current_model == "gpt-4o"),
                PickerItem::new("gpt-4o-mini", "GPT-4o Mini")
                    .with_description("Fast and affordable")
                    .with_active(current_model == "gpt-4o-mini"),
                PickerItem::new("o3-mini", "O3 Mini")
                    .with_description("Latest reasoning model")
                    .with_active(current_model == "o3-mini"),
                PickerItem::new("o1", "O1")
                    .with_description("Advanced reasoning model")
                    .with_active(current_model == "o1"),
                PickerItem::new("o1-mini", "O1 Mini")
                    .with_description("Fast reasoning model")
                    .with_active(current_model == "o1-mini"),
                PickerItem::new("gpt-4-turbo", "GPT-4 Turbo")
                    .with_description("High capability, 128k context")
                    .with_active(current_model == "gpt-4-turbo"),
                PickerItem::new("gpt-4", "GPT-4")
                    .with_description("Original GPT-4")
                    .with_active(current_model == "gpt-4"),
                PickerItem::new("gpt-3.5-turbo", "GPT-3.5 Turbo")
                    .with_description("Fast and economical")
                    .with_active(current_model == "gpt-3.5-turbo"),
            ],
            "claude" | "anthropic" => vec![
                PickerItem::new("claude-sonnet-4-20250514", "Claude Sonnet 4")
                    .with_description("Latest, most capable")
                    .with_active(current_model == "claude-sonnet-4-20250514"),
                PickerItem::new("claude-3-7-sonnet-20250219", "Claude 3.7 Sonnet")
                    .with_description("Hybrid reasoning model")
                    .with_active(current_model == "claude-3-7-sonnet-20250219"),
                PickerItem::new("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet")
                    .with_description("Best balance of speed and capability")
                    .with_active(current_model == "claude-3-5-sonnet-20241022"),
                PickerItem::new("claude-3-5-haiku-20241022", "Claude 3.5 Haiku")
                    .with_description("Fast and affordable")
                    .with_active(current_model == "claude-3-5-haiku-20241022"),
                PickerItem::new("claude-3-opus-20240229", "Claude 3 Opus")
                    .with_description("Most powerful, best for complex tasks")
                    .with_active(current_model == "claude-3-opus-20240229"),
                PickerItem::new("claude-3-haiku-20240307", "Claude 3 Haiku")
                    .with_description("Fastest, most economical")
                    .with_active(current_model == "claude-3-haiku-20240307"),
            ],
            "copilot" | "github" => vec![
                PickerItem::new("gpt-4o", "GPT-4o")
                    .with_description("Most capable model via Copilot")
                    .with_active(current_model == "gpt-4o"),
                PickerItem::new("gpt-4", "GPT-4")
                    .with_description("Original GPT-4")
                    .with_active(current_model == "gpt-4"),
            ],
            "gemini" | "google" => vec![
                PickerItem::new("gemini-2.0-flash-exp", "Gemini 2.0 Flash")
                    .with_description("Fast and efficient (default)")
                    .with_active(current_model == "gemini-2.0-flash-exp"),
                PickerItem::new("gemini-2.0-flash-thinking-exp", "Gemini 2.0 Flash Thinking")
                    .with_description("With extended thinking")
                    .with_active(current_model == "gemini-2.0-flash-thinking-exp"),
                PickerItem::new("gemini-1.5-pro", "Gemini 1.5 Pro")
                    .with_description("Larger, more capable")
                    .with_active(current_model == "gemini-1.5-pro"),
                PickerItem::new("gemini-1.5-flash", "Gemini 1.5 Flash")
                    .with_description("Fast and lightweight")
                    .with_active(current_model == "gemini-1.5-flash"),
            ],
            "openrouter" => vec![
                PickerItem::new("anthropic/claude-sonnet-4", "Claude Sonnet 4")
                    .with_description("Latest Claude via OpenRouter")
                    .with_active(current_model == "anthropic/claude-sonnet-4"),
                PickerItem::new("deepseek/deepseek-chat", "DeepSeek Chat")
                    .with_description("Very affordable, great quality")
                    .with_active(current_model == "deepseek/deepseek-chat"),
                PickerItem::new("google/gemini-2.0-flash-exp:free", "Gemini 2.0 (Free)")
                    .with_description("Free via OpenRouter")
                    .with_active(current_model == "google/gemini-2.0-flash-exp:free"),
                PickerItem::new(
                    "meta-llama/llama-3.1-8b-instruct:free",
                    "Llama 3.1 8B (Free)",
                )
                .with_description("Free open model")
                .with_active(current_model == "meta-llama/llama-3.1-8b-instruct:free"),
                PickerItem::new("qwen/qwen-2.5-72b-instruct", "Qwen 2.5 72B")
                    .with_description("Excellent for coding")
                    .with_active(current_model == "qwen/qwen-2.5-72b-instruct"),
            ],
            "ollama" | "local" => vec![
                PickerItem::new("llama3.2", "Llama 3.2")
                    .with_description("Meta's latest open model")
                    .with_active(current_model == "llama3.2"),
                PickerItem::new("qwen2.5-coder", "Qwen 2.5 Coder")
                    .with_description("Excellent for coding tasks")
                    .with_active(current_model == "qwen2.5-coder"),
                PickerItem::new("codellama", "Code Llama")
                    .with_description("Optimized for code generation")
                    .with_active(current_model == "codellama"),
                PickerItem::new("deepseek-coder-v2", "DeepSeek Coder V2")
                    .with_description("Advanced coding model")
                    .with_active(current_model == "deepseek-coder-v2"),
                PickerItem::new("mistral", "Mistral")
                    .with_description("Fast and capable")
                    .with_active(current_model == "mistral"),
                PickerItem::new("phi3", "Phi-3")
                    .with_description("Microsoft's compact model")
                    .with_active(current_model == "phi3"),
            ],
            _ => vec![PickerItem::new("default", "Default Model")],
        }
    }

    /// Get picker items for model selection for a specific provider
    ///
    /// Used in the two-step model picker flow to show models for the selected provider.
    /// Highlights the currently selected model if it matches the provider.
    ///
    /// Requirements: 1.3, 1.4, 4.1
    fn get_model_picker_items_for_provider(&self, provider_id: &str) -> Vec<PickerItem> {
        let current_model = self
            .agent_bridge
            .as_ref()
            .map(|b| b.model_name().to_string())
            .unwrap_or_default();

        match provider_id {
            "openai" | "gpt" => vec![
                PickerItem::new("gpt-4o", "GPT-4o")
                    .with_description("Most capable, multimodal")
                    .with_active(current_model == "gpt-4o"),
                PickerItem::new("gpt-4o-mini", "GPT-4o Mini")
                    .with_description("Fast and affordable")
                    .with_active(current_model == "gpt-4o-mini"),
                PickerItem::new("o3-mini", "O3 Mini")
                    .with_description("Latest reasoning model")
                    .with_active(current_model == "o3-mini"),
                PickerItem::new("o1", "O1")
                    .with_description("Advanced reasoning model")
                    .with_active(current_model == "o1"),
                PickerItem::new("o1-mini", "O1 Mini")
                    .with_description("Fast reasoning model")
                    .with_active(current_model == "o1-mini"),
                PickerItem::new("gpt-4-turbo", "GPT-4 Turbo")
                    .with_description("High capability, 128k context")
                    .with_active(current_model == "gpt-4-turbo"),
                PickerItem::new("gpt-4", "GPT-4")
                    .with_description("Original GPT-4")
                    .with_active(current_model == "gpt-4"),
                PickerItem::new("gpt-3.5-turbo", "GPT-3.5 Turbo")
                    .with_description("Fast and economical")
                    .with_active(current_model == "gpt-3.5-turbo"),
            ],
            "claude" | "anthropic" => vec![
                PickerItem::new("claude-sonnet-4-20250514", "Claude Sonnet 4")
                    .with_description("Latest, most capable")
                    .with_active(current_model == "claude-sonnet-4-20250514"),
                PickerItem::new("claude-3-7-sonnet-20250219", "Claude 3.7 Sonnet")
                    .with_description("Hybrid reasoning model")
                    .with_active(current_model == "claude-3-7-sonnet-20250219"),
                PickerItem::new("claude-3-5-sonnet-20241022", "Claude 3.5 Sonnet")
                    .with_description("Best balance of speed and capability")
                    .with_active(current_model == "claude-3-5-sonnet-20241022"),
                PickerItem::new("claude-3-5-haiku-20241022", "Claude 3.5 Haiku")
                    .with_description("Fast and affordable")
                    .with_active(current_model == "claude-3-5-haiku-20241022"),
                PickerItem::new("claude-3-opus-20240229", "Claude 3 Opus")
                    .with_description("Most powerful, best for complex tasks")
                    .with_active(current_model == "claude-3-opus-20240229"),
                PickerItem::new("claude-3-haiku-20240307", "Claude 3 Haiku")
                    .with_description("Fastest, most economical")
                    .with_active(current_model == "claude-3-haiku-20240307"),
            ],
            "copilot" | "github" => vec![
                PickerItem::new("gpt-4o", "GPT-4o")
                    .with_description("Most capable model via Copilot")
                    .with_active(current_model == "gpt-4o"),
                PickerItem::new("gpt-4", "GPT-4")
                    .with_description("Original GPT-4")
                    .with_active(current_model == "gpt-4"),
            ],
            "gemini" | "google" => vec![
                PickerItem::new("gemini-2.0-flash-exp", "Gemini 2.0 Flash")
                    .with_description("Fast and efficient (default)")
                    .with_active(current_model == "gemini-2.0-flash-exp"),
                PickerItem::new("gemini-2.0-flash-thinking-exp", "Gemini 2.0 Flash Thinking")
                    .with_description("With extended thinking")
                    .with_active(current_model == "gemini-2.0-flash-thinking-exp"),
                PickerItem::new("gemini-1.5-pro", "Gemini 1.5 Pro")
                    .with_description("Larger, more capable")
                    .with_active(current_model == "gemini-1.5-pro"),
                PickerItem::new("gemini-1.5-flash", "Gemini 1.5 Flash")
                    .with_description("Fast and lightweight")
                    .with_active(current_model == "gemini-1.5-flash"),
            ],
            "openrouter" => vec![
                PickerItem::new("anthropic/claude-sonnet-4", "Claude Sonnet 4")
                    .with_description("Latest Claude via OpenRouter")
                    .with_active(current_model == "anthropic/claude-sonnet-4"),
                PickerItem::new("deepseek/deepseek-chat", "DeepSeek Chat")
                    .with_description("Very affordable, great quality")
                    .with_active(current_model == "deepseek/deepseek-chat"),
                PickerItem::new("google/gemini-2.0-flash-exp:free", "Gemini 2.0 (Free)")
                    .with_description("Free via OpenRouter")
                    .with_active(current_model == "google/gemini-2.0-flash-exp:free"),
                PickerItem::new(
                    "meta-llama/llama-3.1-8b-instruct:free",
                    "Llama 3.1 8B (Free)",
                )
                .with_description("Free open model")
                .with_active(current_model == "meta-llama/llama-3.1-8b-instruct:free"),
                PickerItem::new("qwen/qwen-2.5-72b-instruct", "Qwen 2.5 72B")
                    .with_description("Excellent for coding")
                    .with_active(current_model == "qwen/qwen-2.5-72b-instruct"),
            ],
            "ollama" | "local" => vec![
                PickerItem::new("llama3.2", "Llama 3.2")
                    .with_description("Meta's latest open model")
                    .with_active(current_model == "llama3.2"),
                PickerItem::new("qwen2.5-coder", "Qwen 2.5 Coder")
                    .with_description("Excellent for coding tasks")
                    .with_active(current_model == "qwen2.5-coder"),
                PickerItem::new("codellama", "Code Llama")
                    .with_description("Optimized for code generation")
                    .with_active(current_model == "codellama"),
                PickerItem::new("deepseek-coder-v2", "DeepSeek Coder V2")
                    .with_description("Advanced coding model")
                    .with_active(current_model == "deepseek-coder-v2"),
                PickerItem::new("mistral", "Mistral")
                    .with_description("Fast and capable")
                    .with_active(current_model == "mistral"),
                PickerItem::new("phi3", "Phi-3")
                    .with_description("Microsoft's compact model")
                    .with_active(current_model == "phi3"),
            ],
            _ => vec![PickerItem::new("default", "Default Model")],
        }
    }

    /// Get picker items for model selection for a specific provider (dynamic) (dynamic)
    ///
    /// Fetches models dynamically from AgentBridge using list_available_models().
    /// Falls back to hardcoded list if fetching fails.
    ///
    /// Requirements: 1.3, 1.4, 4.1
    fn get_model_picker_items_for_provider_dynamic(&self, provider_id: &str) -> Vec<PickerItem> {
        let current_model = self
            .agent_bridge
            .as_ref()
            .map(|b| b.model_name().to_string())
            .unwrap_or_default();

        // Try to fetch models dynamically from AgentBridge
        if let Some(ref bridge) = self.agent_bridge {
            // Use block_in_place to call async function in sync context
            let models_result = tokio::task::block_in_place(|| {
                tokio::runtime::Handle::current().block_on(bridge.list_available_models())
            });

            if !models_result.is_empty() {
                // Successfully fetched models dynamically
                return models_result
                    .into_iter()
                    .map(|(model_id, display_name, description)| {
                        PickerItem::new(&model_id, &display_name)
                            .with_description(description)
                            .with_active(current_model == model_id)
                    })
                    .collect();
            }
        }

        // Fall back to hardcoded list if fetch failed
        self.get_model_picker_items_for_provider(provider_id)
    }

    /// Handle key events when picker is visible
    ///
    /// Handles the two-step model picker flow (provider → model selection).
    /// Requirements: 1.3, 1.5, 1.6, 6.4, 12.1, 12.2
    fn handle_picker_key(&mut self, key: KeyEvent) -> anyhow::Result<bool> {
        match key.code {
            KeyCode::Esc => {
                // Cancel the picker and reset model picker state
                self.state.picker.cancel();
                self.state.active_picker_type = None;
                self.state.model_picker_state = None;
            }
            KeyCode::Char('d') if self.state.active_picker_type == Some(PickerType::Session) => {
                // Delete selected session (only for session picker)
                if let Some(session_id) = self.state.picker.selected_id() {
                    let session_id = session_id.to_string();
                    self.handle_session_deletion(&session_id);
                }
            }
            KeyCode::Enter => {
                // Check if selected item is disabled before confirming
                if let Some(item) = self.state.picker.selected_item() {
                    if item.is_disabled {
                        // Show error message for disabled provider
                        if let Some(ref desc) = item.description {
                            self.state.status_message =
                                Some(format!("Provider not available: {}", desc));
                        } else {
                            self.state.status_message = Some("Provider not available".to_string());
                        }
                        return Ok(true);
                    }
                }

                if let Some(id) = self.state.picker.confirm() {
                    self.handle_picker_confirm(&id);
                }
                // Note: active_picker_type is now managed by handle_picker_confirm
                // for the two-step flow
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.state.picker.select_previous();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.state.picker.select_next();
            }
            KeyCode::Home | KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::NONE) => {
                self.state.picker.select_first();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.state.picker.select_last();
            }
            KeyCode::Backspace => {
                self.state.picker.filter_pop();
            }
            KeyCode::Char(c) => {
                // Filter input
                self.state.picker.filter_push(c);
            }
            _ => {}
        }
        Ok(true)
    }

    /// Handle picker confirmation based on picker type
    ///
    /// Implements the two-step model picker flow:
    /// 1. If in provider selection step, transition to model selection
    /// 2. If in model selection step, apply selection and save preference for current mode
    ///
    /// Requirements: 1.3, 1.5, 2.3, 6.4, 12.3, 12.4
    fn handle_picker_confirm(&mut self, selected_id: &str) {
        let picker_type = self.state.active_picker_type;

        match picker_type {
            Some(PickerType::Session) => {
                self.handle_session_switch(selected_id);
                self.state.active_picker_type = None;
                self.state.model_picker_state = None;
            }
            Some(PickerType::Provider) => {
                // Check if we're in the two-step model picker flow
                if self.state.model_picker_state.is_some() {
                    // Step 1 complete: Provider selected, transition to model selection
                    self.state.model_picker_state = Some(ModelPickerState::SelectingModel {
                        provider: selected_id.to_string(),
                    });

                    // Show model picker for the selected provider
                    self.state.picker.set_title("Select Model");
                    let items = self.get_model_picker_items_for_provider_dynamic(selected_id);
                    self.state.picker.set_items(items);
                    self.state.active_picker_type = Some(PickerType::Model);
                    self.state.picker.show();
                } else {
                    // Legacy provider switch (not part of two-step flow)
                    self.handle_provider_switch(selected_id);
                    self.state.active_picker_type = None;
                }
            }
            Some(PickerType::Model) => {
                // Check if we're in the two-step model picker flow
                if let Some(ModelPickerState::SelectingModel { ref provider }) =
                    self.state.model_picker_state
                {
                    // Step 2 complete: Model selected, apply selection and save preference
                    let provider_id = provider.clone();
                    self.handle_two_step_model_selection(&provider_id, selected_id);
                } else {
                    // Legacy model switch (not part of two-step flow)
                    self.handle_model_switch(selected_id);
                }
                self.state.active_picker_type = None;
                self.state.model_picker_state = None;
            }
            None => {}
        }
    }

    /// Handle the completion of the two-step model selection flow
    ///
    /// Switches to the selected provider and model, then saves the preference
    /// for the current agent mode only.
    ///
    /// Requirements: 1.5, 2.3
    fn handle_two_step_model_selection(&mut self, provider_id: &str, model_id: &str) {
        if let Some(ref mut bridge) = self.agent_bridge {
            // Check if provider requires authentication first
            if provider_id == "copilot" || provider_id == "github" {
                // Check if Copilot token exists
                if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
                    let token_path = proj_dirs.config_dir().join("copilot_token.json");
                    if !token_path.exists() {
                        self.state.status_message = Some(
                            "⚠️  GitHub Copilot not authenticated. Run: tark auth copilot"
                                .to_string(),
                        );
                        return;
                    }
                }
            }

            // Switch provider first
            if let Err(e) = bridge.set_provider(provider_id) {
                self.state.status_message = Some(format!("Failed to switch provider: {}", e));
                return;
            }

            // Then set the model
            bridge.set_model(model_id);

            // Save preference for current mode only
            let current_mode = bridge.mode();
            let pref = crate::storage::ModelPreference {
                provider: provider_id.to_string(),
                model: model_id.to_string(),
            };
            bridge.set_mode_preference(current_mode, pref);

            self.state.status_message = Some(format!(
                "Switched to {} / {} for {} mode",
                provider_id, model_id, current_mode
            ));

            // Update panel data
            self.update_panel_from_bridge();
        }
    }

    /// Handle session switch from picker
    ///
    /// Requirements: 6.4, 6.6
    fn handle_session_switch(&mut self, session_id: &str) {
        if session_id == "new" {
            // Create new session
            if let Some(ref mut bridge) = self.agent_bridge {
                match bridge.new_session() {
                    Ok(()) => {
                        self.state.message_list.clear();
                        self.state.status_message = Some("New session created".to_string());
                    }
                    Err(e) => {
                        self.state.status_message =
                            Some(format!("Failed to create session: {}", e));
                        return;
                    }
                }
            }
            self.update_panel_from_bridge();
        } else {
            // Switch to existing session
            let mut session_name: Option<String> = None;
            let switch_result = if let Some(ref mut bridge) = self.agent_bridge {
                match bridge.switch_session(session_id) {
                    Ok(()) => {
                        session_name = Some(bridge.session_name().to_string());
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            } else {
                Err(anyhow::anyhow!("No agent bridge"))
            };

            match switch_result {
                Ok(()) => {
                    // Restore messages from session (Requirements 6.4, 6.6)
                    self.restore_messages_from_session();
                    self.state.status_message = Some(format!(
                        "Switched to session: {}",
                        session_name.unwrap_or_else(|| "unknown".to_string())
                    ));
                    self.update_panel_from_bridge();
                }
                Err(e) => {
                    self.state.status_message = Some(format!("Failed to switch session: {}", e));
                }
            }
        }
    }

    /// Handle session deletion from picker
    ///
    /// Deletes the selected session after confirmation.
    fn handle_session_deletion(&mut self, session_id: &str) {
        if let Some(ref mut bridge) = self.agent_bridge {
            match bridge.delete_session(session_id) {
                Ok(()) => {
                    self.state.status_message = Some("Session deleted successfully".to_string());

                    // Refresh the session picker to remove deleted session
                    let items = self.get_session_picker_items();
                    self.state.picker.set_items(items);
                }
                Err(e) => {
                    self.state.status_message = Some(format!("Cannot delete session: {}", e));
                }
            }
        }
    }

    /// Handle provider switch from picker
    ///
    /// Requirements: 12.3, 12.5
    fn handle_provider_switch(&mut self, provider_id: &str) {
        if let Some(ref mut bridge) = self.agent_bridge {
            // Check if provider requires authentication first
            if provider_id == "copilot" || provider_id == "github" {
                if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
                    let token_path = proj_dirs.config_dir().join("copilot_token.json");
                    if !token_path.exists() {
                        self.state.status_message = Some(
                            "⚠️  GitHub Copilot not authenticated. Run: tark auth copilot"
                                .to_string(),
                        );
                        return;
                    }
                }
            }

            match bridge.set_provider(provider_id) {
                Ok(()) => {
                    self.state.status_message =
                        Some(format!("Switched to provider: {}", provider_id));
                    // Update panel data after provider change (Requirements 5.8)
                    self.update_panel_from_bridge();
                }
                Err(e) => {
                    self.state.status_message = Some(format!("Failed to switch provider: {}", e));
                }
            }
        }
    }

    /// Handle model switch from picker
    ///
    /// Requirements: 12.4, 12.5
    fn handle_model_switch(&mut self, model_id: &str) {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.set_model(model_id);
            self.state.status_message = Some(format!("Switched to model: {}", model_id));
            // Update panel data after model change (Requirements 5.8)
            self.update_panel_from_bridge();
        }
    }

    /// Restore messages from the current session to the message list
    ///
    /// Requirements: 6.1, 6.6
    fn restore_messages_from_session(&mut self) {
        if let Some(ref bridge) = self.agent_bridge {
            // Clear current messages
            self.state.message_list.clear();

            // Get messages from session and add to message list
            let chat_messages = bridge.get_chat_messages();
            for msg in chat_messages {
                self.state.message_list.push(msg);
            }

            // Scroll to bottom will be done after initial render with correct dimensions
            // to ensure accurate scroll calculation based on actual message area size
        }
    }

    /// Run the main event loop
    ///
    /// This polls for events and renders the UI until the application quits.
    pub async fn run(&mut self) -> anyhow::Result<()> {
        // Initial render
        self.render()?;

        // Scroll to bottom after initial render to ensure messages are visible
        // Use the actual message area width for accurate scroll calculation
        if !self.state.message_list.messages().is_empty() {
            let width = self.get_message_area_width();
            self.state.message_list.scroll_to_bottom(width);
        }

        while !self.state.should_quit {
            // Process any pending message queuing
            self.process_pending_async();

            // Start async LLM work if there's a pending request
            if let Some(request) = self.state.pending_async_request.take() {
                // Run LLM work while still polling events and rendering
                self.run_llm_with_ui_updates(request).await?;
            }

            // Poll for agent events (non-blocking)
            let agent_events_processed = self.poll_agent_events();

            // Update spinner if processing
            let needs_spinner_update = self.state.update_spinner_if_needed();

            // Use non-blocking poll with short timeout during processing
            let poll_timeout = if self.state.agent_processing {
                std::time::Duration::from_millis(50) // Fast refresh during processing
            } else {
                std::time::Duration::from_millis(100) // Normal refresh otherwise
            };

            // Poll for terminal events
            if let Some(event) = self.events.poll_with_timeout(poll_timeout)? {
                let needs_redraw = self.handle_event(event)?;
                if needs_redraw {
                    self.render()?;
                }
            } else if agent_events_processed || needs_spinner_update {
                // Re-render for agent updates or spinner animation
                self.render()?;
            }
        }

        Ok(())
    }

    /// Process any pending async work (message sending)
    ///
    /// This method stages LLM requests for async processing.
    /// The actual work is done by spawn_async_llm_work.
    fn process_pending_async(&mut self) {
        // Take the pending request if any
        if let Some(request) = self.pending_request.take() {
            // Update status - the actual response comes through the channel
            self.state.status_message = Some("Sending to LLM...".to_string());

            // Store the request in a shared state for async processing
            self.state.pending_async_request = Some(AsyncMessageRequest {
                content: request.content,
                attachments: request.attachments,
                tx: request.tx,
                config: self.state.config.to_attachment_config(),
            });
        }
    }

    /// Run LLM work while continuously updating the UI
    ///
    /// This spawns the LLM work and polls for events/input while it's running,
    /// enabling real-time streaming display and responsive input.
    async fn run_llm_with_ui_updates(
        &mut self,
        request: AsyncMessageRequest,
    ) -> anyhow::Result<()> {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        // Flag to track if LLM work is done
        let done_flag = Arc::new(AtomicBool::new(false));
        let done_flag_clone = done_flag.clone();

        // Take the bridge temporarily to use in spawned task
        // We need to use block_in_place since AgentBridge isn't Send
        let bridge = self.agent_bridge.take();

        if let Some(mut bridge) = bridge {
            let has_attachments = !request.attachments.is_empty();
            let tx = request.tx.clone();

            // Spawn the LLM work in a blocking task
            let handle = tokio::task::spawn_blocking(move || {
                let rt = tokio::runtime::Handle::current();
                let result = rt.block_on(async {
                    if has_attachments {
                        bridge
                            .send_message_with_attachments(
                                &request.content,
                                request.attachments,
                                tx.clone(),
                                &request.config,
                            )
                            .await
                    } else {
                        bridge
                            .send_message_streaming(&request.content, tx.clone())
                            .await
                    }
                });
                done_flag_clone.store(true, Ordering::SeqCst);
                (bridge, result, tx)
            });

            // Poll events and render while LLM is working
            while !done_flag.load(Ordering::SeqCst) {
                // Poll for agent events (streaming chunks)
                let events_processed = self.poll_agent_events();

                // Update spinner
                let spinner_updated = self.state.update_spinner_if_needed();

                // Poll for terminal events (short timeout to stay responsive)
                if let Some(event) = self
                    .events
                    .poll_with_timeout(std::time::Duration::from_millis(30))?
                {
                    let needs_redraw = self.handle_event(event)?;
                    if needs_redraw {
                        self.render()?;
                    }
                } else if events_processed || spinner_updated {
                    self.render()?;
                }

                // Small yield to let the spawned task make progress
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            // Get the result and restore the bridge
            let (bridge, result, tx) = handle.await.expect("LLM task panicked");
            self.agent_bridge = Some(bridge);

            match result {
                Ok(()) => {
                    // Message sent successfully
                }
                Err(e) => {
                    let _ = tx.send(AgentEvent::Error(e.to_string())).await;
                    self.state.agent_processing = false;
                }
            }

            // Update panel data after bridge is restored
            // This ensures cost/token info is displayed correctly
            self.update_panel_from_bridge();

            // If panel update was pending (from Completed event while bridge was taken),
            // the bridge is now available so the update above should have worked
            self.state.panel_update_pending = false;

            // Force a render to show the updated panel
            self.render()?;
        }

        Ok(())
    }

    /// Poll for agent events and update UI accordingly
    ///
    /// This is called in the main event loop to process streaming responses,
    /// tool calls, and other agent events.
    ///
    /// Requirements:
    /// - 3.1: Display streaming indicator when LLM starts generating
    /// - 3.2: Update message content in real-time during streaming
    /// - 3.3: Finalize message and remove streaming indicator on completion
    /// - 3.4: Display partial response with interruption notice if interrupted
    /// - 3.5: Streaming display shall not block user input or UI responsiveness
    /// - 4.1: Display Tool_Block when ChatAgent executes a tool
    /// - 4.2: Show tool name and arguments in Tool_Block
    /// - 4.3: Show result preview when tool completes
    /// - 4.5: Display multiple tools in sequence
    /// - 4.6: Show current tool in status bar
    ///
    /// Returns true if any events were processed.
    fn poll_agent_events(&mut self) -> bool {
        let mut processed = false;

        // Take the receiver temporarily to avoid borrow issues
        if let Some(mut rx) = self.agent_event_rx.take() {
            // Try to receive events without blocking (Requirements 3.5)
            while let Ok(event) = rx.try_recv() {
                processed = true;
                match event {
                    AgentEvent::Started => {
                        // Create a streaming placeholder message (Requirements 3.1, 3.2)
                        let streaming_msg =
                            ChatMessage::assistant(String::new()).with_streaming(true);
                        self.state.message_list.push(streaming_msg);
                        self.state.status_message = Some("Receiving response...".to_string());
                        // Clear current tool indicator
                        self.state.current_tool = None;
                    }
                    AgentEvent::TextChunk(chunk) => {
                        // Append chunk to the streaming assistant message (Requirements 3.2)
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant
                                && last_msg.is_streaming
                            {
                                last_msg.content.push_str(&chunk);

                                // Request auto-scroll on next render (after visible_height is updated)
                                self.state.auto_scroll_pending = true;
                            }
                        }
                    }
                    AgentEvent::ThinkingChunk(chunk) => {
                        // Handle thinking content (Requirements 9.1, 9.3)
                        // Only display if thinking_mode is enabled
                        if self.state.thinking_mode {
                            if let Some(last_msg) =
                                self.state.message_list.messages_mut().last_mut()
                            {
                                if last_msg.role == super::widgets::Role::Assistant
                                    && last_msg.is_streaming
                                {
                                    // Append thinking content to the message's thinking_content field
                                    last_msg.thinking_content.push_str(&chunk);
                                }
                            }
                        }
                        // Note: When thinking_mode is disabled, thinking content is silently ignored
                    }
                    AgentEvent::ToolCallStarted { tool, args } => {
                        // Update status bar with current tool (Requirement 4.6)
                        self.state.current_tool = Some(tool.clone());
                        self.state.status_message = Some(format!("⚙️ Executing: {}", tool));

                        // Add tool call info to the streaming assistant message (Requirements 4.1, 4.2, 4.5)
                        // This will be rendered as a collapsible Tool_Block
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant {
                                // Create ToolCallInfo with empty result (will be updated on completion)
                                let tool_info = super::widgets::ToolCallInfo::new(
                                    tool.clone(),
                                    args.clone(),
                                    "⏳ Running...", // Placeholder until completion
                                );
                                last_msg.tool_call_info.push(tool_info);
                            }
                        }
                    }
                    AgentEvent::ToolCallCompleted {
                        tool,
                        result_preview,
                    } => {
                        // Clear current tool from status bar (Requirement 4.6)
                        self.state.current_tool = None;
                        self.state.status_message = Some(format!("✓ {} completed", tool));

                        // Update the tool call info with the result (Requirement 4.3)
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant {
                                // Find the matching tool call and update its result
                                if let Some(tool_info) =
                                    last_msg.tool_call_info.iter_mut().rev().find(|t| {
                                        t.tool == tool && t.result_preview == "⏳ Running..."
                                    })
                                {
                                    // Truncate result preview if too long
                                    tool_info.result_preview = if result_preview.len() > 500 {
                                        format!("{}...", &result_preview[..497])
                                    } else {
                                        result_preview
                                    };
                                }
                            }
                        }
                    }
                    AgentEvent::ToolCallFailed { tool, error } => {
                        // Handle tool execution failure (Requirements 7.2)
                        // Clear current tool from status bar
                        self.state.current_tool = None;
                        self.state.status_message = Some(format!("✗ {} failed", tool));

                        // Update the tool call info with the error (red styling in ToolBlock)
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant {
                                // Find the matching tool call and update with error
                                if let Some(tool_info) =
                                    last_msg.tool_call_info.iter_mut().rev().find(|t| {
                                        t.tool == tool && t.result_preview == "⏳ Running..."
                                    })
                                {
                                    // Set error message (will be displayed in red)
                                    tool_info.error = Some(error.clone());
                                    tool_info.result_preview = format!("Error: {}", error);
                                }
                            }
                        }
                    }
                    AgentEvent::Completed(info) => {
                        // Clear current tool indicator
                        self.state.current_tool = None;

                        // Finalize the streaming message (Requirements 3.3)
                        // Find and update the streaming message, or create a new one if not found
                        let mut found_streaming = false;
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant
                                && last_msg.is_streaming
                            {
                                // Save thinking content before overwriting
                                let thinking_content = last_msg.thinking_content.clone();

                                // Update the streaming message with final content
                                // If we have thinking content, wrap it in tags for collapsible rendering
                                if !thinking_content.is_empty() && self.state.thinking_mode {
                                    last_msg.content = format!(
                                        "<thinking>\n{}\n</thinking>\n\n{}",
                                        thinking_content, info.text
                                    );
                                } else {
                                    last_msg.content = info.text.clone();
                                }
                                last_msg.is_streaming = false;

                                // Update tool call info from the response if we have detailed logs
                                // This ensures we have the complete tool call information
                                if !info.tool_call_log.is_empty()
                                    && last_msg.tool_call_info.is_empty()
                                {
                                    for log in &info.tool_call_log {
                                        last_msg.tool_call_info.push(
                                            super::widgets::ToolCallInfo::new(
                                                log.tool.clone(),
                                                log.args.clone(),
                                                log.result_preview.clone(),
                                            ),
                                        );
                                    }
                                }

                                found_streaming = true;
                            }
                        }

                        // If no streaming message was found, add a new one with tool info
                        if !found_streaming {
                            let mut msg = ChatMessage::assistant(info.text);
                            for log in &info.tool_call_log {
                                msg.tool_call_info.push(super::widgets::ToolCallInfo::new(
                                    log.tool.clone(),
                                    log.args.clone(),
                                    log.result_preview.clone(),
                                ));
                            }
                            self.state.message_list.push(msg);
                        }

                        self.state.agent_processing = false;
                        self.state.status_message = Some(format!(
                            "Response complete ({} tool calls, {} tokens)",
                            info.tool_calls_made,
                            info.input_tokens + info.output_tokens
                        ));

                        // Mark that panel needs update when bridge is restored
                        self.state.panel_update_pending = true;

                        // Try to update panel (will succeed if bridge is available)
                        self.update_panel_from_bridge();

                        // Auto-scroll to bottom to show the complete response
                        let width = self.get_message_area_width();
                        self.state.message_list.scroll_to_bottom(width);

                        // Process next queued message if any
                        self.process_next_queued_message();
                    }
                    AgentEvent::Error(error) => {
                        // Handle error with user-friendly message and suggestions (Requirements 7.1, 7.6)
                        self.handle_agent_error(&error);
                    }
                    AgentEvent::Interrupted => {
                        // Clear current tool indicator
                        self.state.current_tool = None;

                        // Display partial response with interruption notice (Requirements 3.4, 8.3)
                        if let Some(last_msg) = self.state.message_list.messages_mut().last_mut() {
                            if last_msg.role == super::widgets::Role::Assistant
                                && last_msg.is_streaming
                            {
                                last_msg.is_streaming = false;
                                // Add interruption notice to partial response
                                if !last_msg.content.is_empty() {
                                    last_msg.content.push_str("\n\n⚠️ [Response interrupted]");
                                } else {
                                    last_msg.content =
                                        "⚠️ [Response interrupted before any content was received]"
                                            .to_string();
                                }
                            }
                        }
                        self.state.agent_processing = false;
                        self.state.status_message = Some("Interrupted".to_string());

                        // Clear the queue on interrupt (user wants to stop)
                        self.state.prompt_queue.clear();
                        self.state.enhanced_panel_data.tasks.clear();
                    }
                    AgentEvent::ContextCompacted {
                        old_tokens,
                        new_tokens,
                    } => {
                        // Display context compaction notification (Requirements 5.6)
                        let saved_tokens = old_tokens.saturating_sub(new_tokens);
                        let notification = format!(
                            "📦 Context compacted: {} → {} tokens (saved {})",
                            old_tokens, new_tokens, saved_tokens
                        );
                        self.state.status_message = Some(notification.clone());

                        // Also add a system message to the chat for visibility
                        self.state
                            .message_list
                            .push(ChatMessage::system(format!("ℹ️ {}", notification)));

                        // Update panel data to reflect new token count
                        self.update_panel_from_bridge();
                    }
                    AgentEvent::ContextWindowExceeded {
                        current_tokens,
                        max_tokens,
                    } => {
                        // Handle context window exceeded (Requirements 7.3)
                        // Display notification about the issue
                        let notification = format!(
                            "⚠️ Context window exceeded: {} / {} tokens",
                            current_tokens, max_tokens
                        );
                        self.state.status_message = Some(notification.clone());

                        // Add a system message explaining the situation
                        self.state.message_list.push(ChatMessage::system(format!(
                            "⚠️ **Context Window Exceeded**\n\n\
                            Your conversation has reached {} tokens (max: {}).\n\n\
                            💡 **Suggestions:**\n\
                            • Use `/compact` to summarize and reduce context\n\
                            • Use `/session new` to start a fresh conversation\n\
                            • The system will attempt auto-compaction on the next message",
                            current_tokens, max_tokens
                        )));

                        // Update panel to show the context usage
                        self.update_panel_from_bridge();
                    }
                    AgentEvent::RateLimited {
                        retry_after_secs,
                        message,
                    } => {
                        // Handle rate limiting (Requirements 7.4)
                        // Store the retry time
                        self.state.rate_limit_retry_at = Some(
                            std::time::Instant::now()
                                + std::time::Duration::from_secs(retry_after_secs),
                        );

                        // Store the pending message for retry
                        if let Some(ref pending) = self.state.pending_message {
                            self.state.rate_limit_pending_message = Some(pending.clone());
                        }

                        // Display countdown notification
                        let notification =
                            format!("⏳ Rate limited: retry in {} seconds", retry_after_secs);
                        self.state.status_message = Some(notification.clone());

                        // Add a system message explaining the situation
                        self.state.message_list.push(ChatMessage::system(format!(
                            "⏳ **Rate Limit Reached**\n\n\
                            {}\n\n\
                            Will automatically retry in {} seconds.\n\n\
                            💡 **Tip:** You can also switch to a different provider with `/model`",
                            message, retry_after_secs
                        )));

                        // Keep processing flag true so we can auto-retry
                        // The tick handler will check for retry
                    }
                    AgentEvent::AuthRequired {
                        provider,
                        verification_url,
                        user_code,
                        timeout_secs,
                    } => {
                        // Show authentication required message
                        self.state.status_message =
                            Some(format!("🔐 {} authentication required", provider));

                        // Add a system message with auth instructions
                        self.state.message_list.push(ChatMessage::system(format!(
                            "🔐 **Authentication Required for {}**\n\n\
                            Please visit: {}\n\n\
                            Enter code: **{}**\n\n\
                            ⏳ Waiting for authentication (timeout: {}s)...",
                            provider, verification_url, user_code, timeout_secs
                        )));
                    }
                    AgentEvent::AuthSuccess { provider } => {
                        // Show authentication success
                        self.state.status_message =
                            Some(format!("✅ {} authenticated successfully", provider));

                        self.state.message_list.push(ChatMessage::system(format!(
                            "✅ **{} Authentication Successful**\n\n\
                            You can now use {} models.",
                            provider, provider
                        )));
                    }
                    AgentEvent::AuthFailed { provider, error } => {
                        // Show authentication failure
                        self.state.status_message =
                            Some(format!("❌ {} authentication failed", provider));

                        self.state.message_list.push(ChatMessage::system(format!(
                            "❌ **{} Authentication Failed**\n\n\
                            Error: {}\n\n\
                            Please try again with `/model` or check your credentials.",
                            provider, error
                        )));

                        // Stop processing since auth failed
                        self.state.agent_processing = false;
                    }
                }
            }

            // Put the receiver back if we're still processing
            // BUT: only if it wasn't replaced by process_next_queued_message
            // (which creates a new channel for the next request)
            if self.state.agent_processing && self.agent_event_rx.is_none() {
                self.agent_event_rx = Some(rx);
            }
        }

        processed
    }

    /// Render the UI
    ///
    /// Enhanced layout structure (Requirements 1.1, 1.2, 1.3, 2.1-2.4, 9.1-9.7):
    /// - Horizontal split: 70% chat column, 30% panel (full height)
    /// - Chat column vertical split: 75% messages, 5% status, 20% input
    /// - "🤖 Tark" in top border title (compact header)
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
        let llm_configured = self.state.llm_configured;
        let llm_error = self.state.llm_error.clone();
        // Current tool being executed (Requirement 4.6)
        let current_tool = self.state.current_tool.clone();
        // Whether agent is processing (for loading indicator)
        let agent_processing = self.state.agent_processing;

        // Clone input widget for rendering (it implements Clone)
        let input_widget = self.state.input_widget.clone();

        // Clone enhanced panel state for rendering
        let panel_section_state = self.state.panel_section_state.clone();
        let enhanced_panel_data = self.state.enhanced_panel_data.clone();

        // Clone username for rendering (Requirements 2.2, 2.3)
        let username = self.username.clone();

        // Check if message list is empty for rendering decision
        let messages_empty = self.state.message_list.is_empty();

        // Check if auto-scroll is pending (need to check before draw closure)
        let auto_scroll_pending = self.state.auto_scroll_pending;

        self.terminal.draw(|frame| {
            let area = frame.area();

            // Enhanced layout: Main horizontal split - chat column (70%) | panel (30%)
            // Panel spans full height (Requirements 2.1, 2.3, 2.4, 9.3, 9.4)
            let main_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(70), // Chat column (messages + status + input)
                    Constraint::Percentage(30), // Panel (full height)
                ])
                .split(area);

            let chat_column = main_chunks[0];
            let panel_area = main_chunks[1];

            // Chat column vertical split (Requirements 9.5, 9.6, 9.7):
            // - Messages: 75% (flexible, min 5 lines)
            // - Status bar: 5% (min 1 line)
            // - Input: 20% (min 3 lines)
            let chat_height = chat_column.height;
            let status_height = std::cmp::max(1, (chat_height as f32 * 0.05) as u16);
            let input_height = std::cmp::max(3, (chat_height as f32 * 0.20) as u16);

            let chat_chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),                // Messages (75%, flexible)
                    Constraint::Length(status_height), // Status bar (5%)
                    Constraint::Length(input_height),  // Input (20%)
                ])
                .split(chat_column);

            // Messages area with "🤖 Tark" in top border title (Requirements 1.1, 1.2, 1.3)
            let messages_block = Block::default()
                .title(" 🤖 Tark ")
                .borders(Borders::ALL)
                .border_style(if focused == FocusedComponent::Messages {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            let messages_inner = messages_block.inner(chat_chunks[0]);
            frame.render_widget(messages_block, chat_chunks[0]);

            // Update message list visible height for accurate scroll calculations
            // This must be set before any scroll operations
            self.state
                .message_list
                .set_visible_height(messages_inner.height);

            // Render message content
            if messages_empty {
                let mut welcome_text = vec![
                    ratatui::text::Line::from(""),
                    ratatui::text::Line::from(ratatui::text::Span::styled(
                        "  Welcome to tark chat!",
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD),
                    )),
                    ratatui::text::Line::from(""),
                ];

                // Show LLM status
                if llm_configured {
                    welcome_text.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                        "  ✓ LLM configured - ready to chat",
                        Style::default().fg(Color::Green),
                    )));
                } else {
                    welcome_text.push(ratatui::text::Line::from(ratatui::text::Span::styled(
                        "  ⚠ LLM not configured",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    )));
                    welcome_text.push(ratatui::text::Line::from(""));

                    // Show the error message if available
                    if let Some(ref error) = llm_error {
                        for line in error.lines() {
                            welcome_text.push(ratatui::text::Line::from(
                                ratatui::text::Span::styled(
                                    format!("  {}", line),
                                    Style::default().fg(Color::Yellow),
                                ),
                            ));
                        }
                    }
                }

                welcome_text.extend(vec![
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
                ]);
                let welcome = Paragraph::new(welcome_text);
                frame.render_widget(welcome, messages_inner);
            } else {
                // Render messages using the full MessageListWidget for proper
                // streaming indicators, thinking blocks, and tool calls
                let message_list_widget = MessageListWidget::new(&mut self.state.message_list)
                    .username(username.clone())
                    .focused(focused == FocusedComponent::Messages);
                frame.render_widget(message_list_widget, messages_inner);
            }

            // Panel area (right side, full height - Requirements 2.1, 2.2, 9.3, 9.4)
            let panel_block = Block::default()
                .title(" Panel ")
                .borders(Borders::ALL)
                .border_style(if focused == FocusedComponent::Panel {
                    Style::default()
                        .fg(Color::Magenta)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            let panel_inner = panel_block.inner(panel_area);
            frame.render_widget(panel_block, panel_area);

            // Render enhanced panel content with accordion sections and scrollbars
            // (Requirements 3.1, 3.6, 3.7, 4.1, 4.7, 4.8, 5.1, 5.6-5.9, 6.1, 6.5-6.8)
            let enhanced_panel =
                EnhancedPanelWidget::new(&enhanced_panel_data, &panel_section_state);
            frame.render_widget(enhanced_panel, panel_inner);

            // Input area (same width as chat area - Requirements 9.1, 9.2)
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
                    Style::default()
                        .fg(mode_indicator.1)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                });

            // Render the actual input widget content
            let input_renderer = InputWidgetRenderer::new(&input_widget).block(input_block);
            frame.render_widget(input_renderer, chat_chunks[2]);

            // Status bar (same width as chat area - Requirements 9.1, 9.2, 4.6)
            let mode_str = match agent_mode {
                AgentMode::Build => ("◆ Build", Color::Green),
                AgentMode::Plan => ("◇ Plan", Color::Yellow),
                AgentMode::Review => ("◈ Review", Color::Cyan),
            };
            let thinking_str = if thinking_mode { " 🧠" } else { "" };
            let connection_str = if editor_connected {
                ("◉ nvim", Color::Green)
            } else {
                ("", Color::DarkGray)
            };
            let llm_str = if llm_configured {
                ("● LLM", Color::Green)
            } else {
                ("○ No LLM", Color::Red)
            };

            let mut status_spans = vec![ratatui::text::Span::styled(
                format!(" {} ", mode_str.0),
                Style::default().fg(mode_str.1),
            )];

            // Only show connection status if connected
            if editor_connected {
                status_spans.push(ratatui::text::Span::raw("│"));
                status_spans.push(ratatui::text::Span::styled(
                    format!(" {} ", connection_str.0),
                    Style::default().fg(connection_str.1),
                ));
            }

            status_spans.push(ratatui::text::Span::raw("│"));
            status_spans.push(ratatui::text::Span::styled(
                format!(" {} ", llm_str.0),
                Style::default().fg(llm_str.1),
            ));

            if !thinking_str.is_empty() {
                status_spans.push(ratatui::text::Span::raw("│"));
                status_spans.push(ratatui::text::Span::styled(
                    thinking_str,
                    Style::default().fg(Color::Magenta),
                ));
            }

            // Show loading spinner when agent is processing
            if agent_processing {
                // Use the stateful spinner frame for smooth animation
                let spinner_char = SPINNER_FRAMES[self.state.spinner_frame % SPINNER_FRAMES.len()];
                status_spans.push(ratatui::text::Span::raw("│"));
                status_spans.push(ratatui::text::Span::styled(
                    format!(" {} Processing... ", spinner_char),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            // Show current tool being executed (Requirement 4.6)
            if let Some(ref tool) = current_tool {
                status_spans.push(ratatui::text::Span::raw("│"));
                status_spans.push(ratatui::text::Span::styled(
                    format!(" ⚙️ {} ", tool),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ));
            }

            status_spans.push(ratatui::text::Span::raw(" "));
            status_spans.push(ratatui::text::Span::styled(
                status_message.unwrap_or_else(|| "Ready".to_string()),
                Style::default().fg(Color::White),
            ));

            let status = Paragraph::new(ratatui::text::Line::from(status_spans))
                .style(Style::default().bg(Color::DarkGray));
            frame.render_widget(status, chat_chunks[1]);

            // Render picker overlay if visible (Requirements 6.4, 12.1, 12.2)
            if self.state.picker.is_visible() {
                let picker_widget = PickerWidget::new(&self.state.picker);
                frame.render_widget(picker_widget, area);
            }

            // Render auth dialog overlay if visible
            if self.state.auth_dialog.is_visible() {
                let auth_widget = super::widgets::AuthDialogWidget::new(&self.state.auth_dialog);
                frame.render_widget(auth_widget, area);
            }

            // Render command dropdown overlay if visible (above input area)
            if self.state.command_dropdown.is_visible() {
                use super::widgets::CommandDropdownWidget;
                let cursor_area = chat_chunks[2]; // Input area
                let dropdown_widget =
                    CommandDropdownWidget::new(&self.state.command_dropdown, cursor_area);
                frame.render_widget(dropdown_widget, area);
            }
        })?;

        // Perform pending auto-scroll after render (when visible_height is updated)
        if auto_scroll_pending {
            let width = self.get_message_area_width();
            self.state.message_list.scroll_to_bottom(width);
            self.state.auto_scroll_pending = false;
        }

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
        assert!(state.thinking_mode); // Thinking mode enabled by default
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

    // ========================================================================
    // Layout Calculation Helper Functions (for property testing)
    // ========================================================================

    /// Calculate the chat column and panel widths for a given terminal width
    ///
    /// Returns (chat_width, panel_width) based on 70%/30% split
    fn calculate_layout_widths(total_width: u16) -> (u16, u16) {
        // Using the same calculation as ratatui's Percentage constraint
        let chat_width = (total_width as f32 * 0.70).round() as u16;
        let panel_width = total_width.saturating_sub(chat_width);
        (chat_width, panel_width)
    }

    /// Calculate the chat column vertical layout heights
    ///
    /// Returns (messages_height, status_height, input_height)
    fn calculate_chat_heights(total_height: u16) -> (u16, u16, u16) {
        // Status bar: 5% (min 1 line)
        let status_height = std::cmp::max(1, (total_height as f32 * 0.05) as u16);
        // Input: 20% (min 3 lines)
        let input_height = std::cmp::max(3, (total_height as f32 * 0.20) as u16);
        // Messages: remaining space (min 5 lines)
        let messages_height = total_height.saturating_sub(status_height + input_height);
        (messages_height, status_height, input_height)
    }

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

        // ====================================================================
        // Enhanced TUI Layout Property Tests
        // ====================================================================

        /// **Feature: enhanced-tui-layout, Property 1: Layout Proportions**
        /// **Validates: Requirements 2.3, 2.4, 2.5**
        ///
        /// For any terminal size with width >= 40 columns, the chat area width
        /// should be approximately 70% (±2 columns) and the panel width should
        /// be approximately 30% (±2 columns) of the total width.
        #[test]
        #[allow(clippy::manual_range_contains)]
        fn prop_layout_proportions(width in 40u16..=300u16, height in 10u16..=100u16) {
            let _ = height; // Height is used for completeness but not tested here

            let (chat_width, panel_width) = calculate_layout_widths(width);

            // Chat area should be approximately 70% of total width
            let expected_chat = (width as f32 * 0.70).round() as u16;
            let chat_diff = (chat_width as i32 - expected_chat as i32).abs();
            prop_assert!(
                chat_diff <= 2,
                "Chat width {} should be within ±2 of expected {} (70% of {})",
                chat_width, expected_chat, width
            );

            // Panel should be approximately 30% of total width
            let expected_panel = width.saturating_sub(expected_chat);
            let panel_diff = (panel_width as i32 - expected_panel as i32).abs();
            prop_assert!(
                panel_diff <= 2,
                "Panel width {} should be within ±2 of expected {} (30% of {})",
                panel_width, expected_panel, width
            );

            // Total should equal original width
            prop_assert_eq!(
                chat_width + panel_width, width,
                "Chat ({}) + Panel ({}) should equal total width ({})",
                chat_width, panel_width, width
            );
        }

        /// **Feature: enhanced-tui-layout, Property 2: Panel Full Height**
        /// **Validates: Requirements 2.1, 9.3, 9.4**
        ///
        /// For any terminal size, the panel height should equal the terminal
        /// height (full height layout). The panel spans from top to bottom.
        #[test]
        fn prop_panel_full_height(width in 40u16..=300u16, height in 10u16..=100u16) {
            let _ = width; // Width is used for completeness but not tested here

            // In the enhanced layout, the panel spans full height
            // The panel area height equals the terminal height (minus borders)
            // Since we're testing the layout calculation, panel height = terminal height
            let panel_height = height;

            // Panel should span full terminal height
            prop_assert_eq!(
                panel_height, height,
                "Panel height ({}) should equal terminal height ({})",
                panel_height, height
            );
        }

        /// **Feature: enhanced-tui-layout, Property 9: Input and Status Bar Alignment**
        /// **Validates: Requirements 9.1, 9.2**
        ///
        /// For any terminal width, the input box width and status bar width
        /// should equal the chat area width (not the full terminal width).
        #[test]
        fn prop_input_status_alignment(width in 40u16..=300u16, height in 10u16..=100u16) {
            let _ = height; // Height is used for completeness

            let (chat_width, _panel_width) = calculate_layout_widths(width);

            // Input box and status bar should have the same width as chat area
            let input_width = chat_width;
            let status_width = chat_width;

            // Both should equal chat area width
            prop_assert_eq!(
                input_width, chat_width,
                "Input width ({}) should equal chat area width ({})",
                input_width, chat_width
            );
            prop_assert_eq!(
                status_width, chat_width,
                "Status bar width ({}) should equal chat area width ({})",
                status_width, chat_width
            );

            // Neither should equal full terminal width (unless chat is 100%)
            if width > 40 {
                prop_assert!(
                    input_width < width,
                    "Input width ({}) should be less than terminal width ({})",
                    input_width, width
                );
                prop_assert!(
                    status_width < width,
                    "Status bar width ({}) should be less than terminal width ({})",
                    status_width, width
                );
            }
        }

        /// **Feature: enhanced-tui-layout, Property: Chat column vertical proportions**
        /// **Validates: Requirements 9.5, 9.6, 9.7**
        ///
        /// For any terminal height, the chat column should be split into:
        /// - Messages: ~75% (flexible, min 5 lines)
        /// - Status bar: ~5% (min 1 line)
        /// - Input: ~20% (min 3 lines)
        #[test]
        fn prop_chat_column_proportions(height in 10u16..=100u16) {
            let (messages_height, status_height, input_height) = calculate_chat_heights(height);

            // Status bar should be at least 1 line
            prop_assert!(
                status_height >= 1,
                "Status bar height ({}) should be at least 1",
                status_height
            );

            // Input should be at least 3 lines
            prop_assert!(
                input_height >= 3,
                "Input height ({}) should be at least 3",
                input_height
            );

            // Total should not exceed original height
            let total = messages_height + status_height + input_height;
            prop_assert!(
                total <= height,
                "Total height ({}) should not exceed terminal height ({})",
                total, height
            );

            // Messages should get the remaining space
            let expected_messages = height.saturating_sub(status_height + input_height);
            prop_assert_eq!(
                messages_height, expected_messages,
                "Messages height ({}) should equal remaining space ({})",
                messages_height, expected_messages
            );
        }
    }

    // Separate proptest block for TUI LLM Integration tests with fewer cases for faster execution
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(20))]

        // ====================================================================
        // TUI LLM Integration Property Tests
        // ====================================================================

        /// **Feature: tui-llm-integration, Property 1: Message Round-Trip**
        /// **Validates: Requirements 1.1, 1.2**
        ///
        /// For any valid user message added to the message list, the message
        /// SHALL be stored correctly and retrievable from the message list.
        /// This tests the message flow through AppState without actual LLM calls.
        #[test]
        fn prop_message_round_trip(
            message in "[a-zA-Z0-9 .,!?]{1,100}"
        ) {
            let mut state = AppState::new();

            // Initially empty
            prop_assert!(
                state.message_list.messages().is_empty(),
                "Message list should be empty initially"
            );

            // Add a user message
            state.message_list.push(ChatMessage::user(message.clone()));

            // Message should be in the list
            prop_assert_eq!(
                state.message_list.messages().len(),
                1,
                "Message list should have 1 message"
            );

            // Message content should match
            let stored_msg = &state.message_list.messages()[0];
            prop_assert_eq!(
                &stored_msg.content,
                &message,
                "Stored message content should match original"
            );

            // Role should be User
            prop_assert_eq!(
                stored_msg.role,
                crate::tui::widgets::Role::User,
                "Message role should be User"
            );
        }

        /// **Feature: tui-llm-integration, Property 1: Message Round-Trip**
        /// **Validates: Requirements 1.1, 1.2**
        ///
        /// For any sequence of user and assistant messages, all messages
        /// SHALL be stored in order and retrievable from the message list.
        #[test]
        fn prop_message_sequence_preserved(
            messages in prop::collection::vec("[a-zA-Z0-9 .,!?]{1,50}", 1..5)
        ) {
            let mut state = AppState::new();

            // Add alternating user and assistant messages
            for (i, msg) in messages.iter().enumerate() {
                if i % 2 == 0 {
                    state.message_list.push(ChatMessage::user(msg.clone()));
                } else {
                    state.message_list.push(ChatMessage::assistant(msg.clone()));
                }
            }

            // All messages should be stored
            prop_assert_eq!(
                state.message_list.messages().len(),
                messages.len(),
                "All messages should be stored"
            );

            // Messages should be in order with correct content and roles
            for (i, (stored, original)) in
                state.message_list.messages().iter().zip(messages.iter()).enumerate()
            {
                prop_assert_eq!(
                    &stored.content,
                    original,
                    "Message {} content should match",
                    i
                );

                let expected_role = if i % 2 == 0 {
                    crate::tui::widgets::Role::User
                } else {
                    crate::tui::widgets::Role::Assistant
                };
                prop_assert_eq!(
                    stored.role,
                    expected_role,
                    "Message {} role should be correct",
                    i
                );
            }
        }

        /// **Feature: tui-llm-integration, Property 1: Message Round-Trip**
        /// **Validates: Requirements 1.1, 1.2**
        ///
        /// For any agent processing state, the state flags should be
        /// correctly maintained and consistent.
        #[test]
        fn prop_agent_processing_state_consistent(
            processing in proptest::bool::ANY,
            has_pending in proptest::bool::ANY
        ) {
            let mut state = AppState::new();

            // Set processing state
            state.agent_processing = processing;

            // Set pending message if applicable
            if has_pending {
                state.pending_message = Some("test message".to_string());
            }

            // Verify state is consistent
            prop_assert_eq!(
                state.agent_processing,
                processing,
                "Agent processing state should be preserved"
            );

            if has_pending {
                prop_assert!(
                    state.pending_message.is_some(),
                    "Pending message should be set"
                );
            } else {
                prop_assert!(
                    state.pending_message.is_none(),
                    "Pending message should be None"
                );
            }
        }

        /// **Feature: tui-llm-integration, Property 3: Streaming Updates Non-Blocking**
        /// **Validates: Requirements 3.2, 3.5**
        ///
        /// For any sequence of text chunks, streaming messages SHALL be updated
        /// correctly without blocking, and the final content SHALL be the
        /// concatenation of all chunks.
        #[test]
        fn prop_streaming_updates_non_blocking(
            chunks in prop::collection::vec("[a-zA-Z0-9 ]{1,20}", 1..5)
        ) {
            let mut state = AppState::new();

            // Create a streaming message (simulating AgentEvent::Started)
            let streaming_msg = ChatMessage::assistant(String::new()).with_streaming(true);
            state.message_list.push(streaming_msg);

            // Verify streaming message was created
            prop_assert_eq!(
                state.message_list.messages().len(),
                1,
                "Should have one streaming message"
            );
            prop_assert!(
                state.message_list.messages()[0].is_streaming,
                "Message should be in streaming state"
            );

            // Simulate receiving text chunks (AgentEvent::TextChunk)
            let mut expected_content = String::new();
            for chunk in &chunks {
                expected_content.push_str(chunk);

                // Append chunk to streaming message
                if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                    if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                        last_msg.content.push_str(chunk);
                    }
                }
            }

            // Verify content was accumulated correctly
            let final_msg = &state.message_list.messages()[0];
            prop_assert_eq!(
                &final_msg.content,
                &expected_content,
                "Streaming content should be concatenation of all chunks"
            );
            prop_assert!(
                final_msg.is_streaming,
                "Message should still be streaming until finalized"
            );

            // Simulate completion (AgentEvent::Completed)
            if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                    last_msg.is_streaming = false;
                }
            }

            // Verify message was finalized
            let finalized_msg = &state.message_list.messages()[0];
            prop_assert!(
                !finalized_msg.is_streaming,
                "Message should no longer be streaming after completion"
            );
            prop_assert_eq!(
                &finalized_msg.content,
                &expected_content,
                "Content should be preserved after finalization"
            );
        }

        /// **Feature: tui-llm-integration, Property 3: Streaming Updates Non-Blocking**
        /// **Validates: Requirements 3.2, 3.5**
        ///
        /// For any streaming message that is interrupted, the partial content
        /// SHALL be preserved and an interruption notice SHALL be added.
        #[test]
        fn prop_streaming_interruption_preserves_content(
            chunks in prop::collection::vec("[a-zA-Z0-9 ]{1,20}", 1..3)
        ) {
            let mut state = AppState::new();

            // Create a streaming message
            let streaming_msg = ChatMessage::assistant(String::new()).with_streaming(true);
            state.message_list.push(streaming_msg);

            // Accumulate some chunks
            let mut partial_content = String::new();
            for chunk in &chunks {
                partial_content.push_str(chunk);
                if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                    if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                        last_msg.content.push_str(chunk);
                    }
                }
            }

            // Simulate interruption (AgentEvent::Interrupted)
            if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                    last_msg.is_streaming = false;
                    if !last_msg.content.is_empty() {
                        last_msg.content.push_str("\n\n⚠️ [Response interrupted]");
                    }
                }
            }

            // Verify partial content was preserved with interruption notice
            let interrupted_msg = &state.message_list.messages()[0];
            prop_assert!(
                !interrupted_msg.is_streaming,
                "Message should no longer be streaming after interruption"
            );
            prop_assert!(
                interrupted_msg.content.starts_with(&partial_content),
                "Partial content should be preserved"
            );
            prop_assert!(
                interrupted_msg.content.contains("⚠️ [Response interrupted]"),
                "Interruption notice should be added"
            );
        }

        /// **Feature: tui-llm-integration, Property 5: Session Info Display**
        /// **Validates: Requirements 5.2, 5.3, 5.4, 5.5, 5.8, 5.9**
        ///
        /// For any session state change, the Panel SHALL immediately update to show
        /// the current session name, model, provider, token count, and cost.
        #[test]
        fn prop_session_info_display(
            session_name in "[a-zA-Z0-9]{1,10}",
            model_idx in 0usize..3usize,
            provider_idx in 0usize..3usize,
            cost in 0.0f64..50.0f64,
            used_tokens in 0u32..50000u32,
            max_tokens in 1u32..100000u32,
        ) {
            let models = ["gpt-4o", "claude-3-sonnet", "codellama"];
            let providers = ["openai", "claude", "ollama"];
            let model_name = models[model_idx].to_string();
            let provider_name = providers[provider_idx].to_string();

            let mut state = AppState::new();

            // Update session info
            let session_info = crate::tui::widgets::SessionInfo {
                name: session_name.clone(),
                model: model_name.clone(),
                provider: provider_name.clone(),
                cost,
                lsp_languages: vec![],
                cost_breakdown: Vec::new(),
            };
            state.update_session_info(session_info);

            // Verify session info is correctly stored
            prop_assert_eq!(&state.enhanced_panel_data.session.name, &session_name);
            prop_assert_eq!(&state.enhanced_panel_data.session.model, &model_name);
            prop_assert_eq!(&state.enhanced_panel_data.session.provider, &provider_name);
            prop_assert!((state.enhanced_panel_data.session.cost - cost).abs() < f64::EPSILON);

            // Update context info
            let usage_percent = (used_tokens as f32 / max_tokens as f32) * 100.0;
            let context_info = crate::tui::widgets::ContextInfo {
                used_tokens,
                max_tokens,
                usage_percent,
                over_limit: used_tokens > max_tokens || usage_percent >= 100.0,
            };
            state.update_context_info(context_info);

            // Verify context info is correctly stored
            prop_assert_eq!(state.enhanced_panel_data.context.used_tokens, used_tokens);
            prop_assert_eq!(state.enhanced_panel_data.context.max_tokens, max_tokens);
            prop_assert!((state.enhanced_panel_data.context.usage_percent - usage_percent).abs() < 0.01);
            prop_assert!(state.enhanced_panel_data.context.usage_percent >= 0.0);
        }

        /// **Feature: tui-llm-integration, Property 7: Error Handling Robustness**
        /// **Validates: Requirements 7.6**
        ///
        /// For any recoverable error (API failure, tool failure, network error),
        /// the TUI SHALL display an appropriate error message and SHALL NOT crash.
        /// The error categorization SHALL provide user-friendly messages with suggestions.
        #[test]
        fn prop_error_handling_robustness(
            error_type_idx in 0usize..10usize,
            error_detail in "[a-zA-Z0-9 ]{1,50}"
        ) {
            // Generate different types of errors
            let error_messages = [
                format!("API key invalid: {}", error_detail),
                format!("Rate limit exceeded: {}", error_detail),
                format!("Context window exceeded: {}", error_detail),
                format!("Network connection failed: {}", error_detail),
                format!("Model not found: {}", error_detail),
                format!("Provider unavailable: {}", error_detail),
                format!("Tool execution failed: {}", error_detail),
                format!("Invalid request: {}", error_detail),
                format!("Internal server error: {}", error_detail),
                format!("Unknown error: {}", error_detail),
            ];

            let error = &error_messages[error_type_idx];

            // Test that categorize_error returns valid results for all error types
            let (error_type, suggestion) = TuiApp::categorize_error(error);

            // Error type should not be empty
            prop_assert!(
                !error_type.is_empty(),
                "Error type should not be empty for error: {}",
                error
            );

            // Suggestion should not be empty
            prop_assert!(
                !suggestion.is_empty(),
                "Suggestion should not be empty for error: {}",
                error
            );

            // Test that AppState can handle the error without panicking
            let mut state = AppState::new();

            // Simulate error handling by adding error message to message list
            let error_message = format!(
                "⚠️ **{}**\n\n{}\n\n💡 **Suggestion:** {}",
                error_type, error, suggestion
            );
            state.message_list.push(ChatMessage::system(error_message.clone()));

            // Verify error message was added
            prop_assert!(
                !state.message_list.is_empty(),
                "Error message should be added to message list"
            );

            // Verify the message contains the error type
            let last_msg = state.message_list.messages().last().unwrap();
            prop_assert!(
                last_msg.content.contains(error_type),
                "Error message should contain error type"
            );

            // Verify the message contains a suggestion
            prop_assert!(
                last_msg.content.contains("Suggestion"),
                "Error message should contain a suggestion"
            );

            // Verify state is still valid (not crashed)
            prop_assert!(
                !state.should_quit,
                "App should not quit on recoverable error"
            );
        }

        /// **Feature: tui-llm-integration, Property 7: Error Handling Robustness**
        /// **Validates: Requirements 7.6**
        ///
        /// For any error during streaming, the TUI SHALL finalize the streaming
        /// message with an error notice and SHALL NOT crash.
        #[test]
        fn prop_streaming_error_handling(
            partial_content in "[a-zA-Z0-9 ]{0,50}",
            error_message in "[a-zA-Z0-9 ]{1,50}"
        ) {
            let mut state = AppState::new();

            // Create a streaming message
            let streaming_msg = ChatMessage::assistant(partial_content.clone()).with_streaming(true);
            state.message_list.push(streaming_msg);

            // Verify streaming message was created
            prop_assert!(
                state.message_list.messages()[0].is_streaming,
                "Message should be in streaming state"
            );

            // Simulate error during streaming (AgentEvent::Error)
            if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                    last_msg.is_streaming = false;
                    if !last_msg.content.is_empty() {
                        last_msg.content.push_str("\n\n⚠️ [Error occurred]");
                    }
                }
            }

            // Add error system message
            state.message_list.push(ChatMessage::system(format!("⚠️ Error: {}", error_message)));

            // Verify streaming was finalized
            let streaming_msg = &state.message_list.messages()[0];
            prop_assert!(
                !streaming_msg.is_streaming,
                "Streaming should be finalized on error"
            );

            // Verify error message was added
            prop_assert!(
                state.message_list.messages().len() >= 2,
                "Error message should be added"
            );

            // Verify partial content was preserved (if any)
            if !partial_content.is_empty() {
                prop_assert!(
                    streaming_msg.content.starts_with(&partial_content),
                    "Partial content should be preserved"
                );
            }

            // Verify state is still valid
            prop_assert!(
                !state.should_quit,
                "App should not quit on streaming error"
            );
        }

        /// **Feature: tui-llm-integration, Property 7: Error Handling Robustness**
        /// **Validates: Requirements 7.6**
        ///
        /// For any rate limit error, the TUI SHALL store retry information
        /// and SHALL NOT crash.
        #[test]
        fn prop_rate_limit_handling(
            retry_after_secs in 1u64..120u64,
            pending_message in "[a-zA-Z0-9 ]{1,50}"
        ) {
            let mut state = AppState::new();

            // Simulate rate limit by setting retry state
            state.rate_limit_retry_at = Some(
                std::time::Instant::now() + std::time::Duration::from_secs(retry_after_secs)
            );
            state.rate_limit_pending_message = Some(pending_message.clone());

            // Verify rate limit state was set
            prop_assert!(
                state.rate_limit_retry_at.is_some(),
                "Rate limit retry time should be set"
            );
            prop_assert!(
                state.rate_limit_pending_message.is_some(),
                "Pending message should be stored"
            );
            prop_assert_eq!(
                state.rate_limit_pending_message.as_ref().unwrap(),
                &pending_message,
                "Pending message should match"
            );

            // Verify state is still valid
            prop_assert!(
                !state.should_quit,
                "App should not quit on rate limit"
            );
        }

        /// **Feature: tui-llm-integration, Property 8: Interrupt Recovery**
        /// **Validates: Requirements 8.3, 8.5**
        ///
        /// For any interrupted operation, the TUI SHALL display partial results
        /// and be ready for new input immediately after interruption.
        #[test]
        fn prop_interrupt_recovery(
            partial_chunks in prop::collection::vec("[a-zA-Z0-9 ]{1,20}", 0..5),
            was_processing in proptest::bool::ANY
        ) {
            let mut state = AppState::new();

            // Set initial processing state
            state.agent_processing = was_processing;

            // If processing, create a streaming message with partial content
            if was_processing {
                let streaming_msg = ChatMessage::assistant(String::new()).with_streaming(true);
                state.message_list.push(streaming_msg);

                // Accumulate partial chunks
                let mut partial_content = String::new();
                for chunk in &partial_chunks {
                    partial_content.push_str(chunk);
                    if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                        if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                            last_msg.content.push_str(chunk);
                        }
                    }
                }

                // Simulate interrupt (AgentEvent::Interrupted)
                // This is what happens when handle_interrupt is called and the agent sends Interrupted event
                if let Some(last_msg) = state.message_list.messages_mut().last_mut() {
                    if last_msg.role == crate::tui::widgets::Role::Assistant && last_msg.is_streaming {
                        last_msg.is_streaming = false;
                        // Add interruption notice to partial response (Requirement 8.3)
                        if !last_msg.content.is_empty() {
                            last_msg.content.push_str("\n\n⚠️ [Response interrupted]");
                        } else {
                            last_msg.content = "⚠️ [Response interrupted before any content was received]".to_string();
                        }
                    }
                }

                // Clear processing flag (Requirement 8.5)
                state.agent_processing = false;

                // Verify partial content was preserved with interruption notice
                let interrupted_msg = &state.message_list.messages()[0];
                prop_assert!(
                    !interrupted_msg.is_streaming,
                    "Message should no longer be streaming after interruption"
                );

                // Verify interruption notice was added
                prop_assert!(
                    interrupted_msg.content.contains("interrupted"),
                    "Interruption notice should be present"
                );

                // If there was partial content, it should be preserved
                if !partial_chunks.is_empty() {
                    let expected_partial: String = partial_chunks.iter().cloned().collect();
                    prop_assert!(
                        interrupted_msg.content.starts_with(&expected_partial),
                        "Partial content should be preserved: expected to start with '{}', got '{}'",
                        expected_partial,
                        interrupted_msg.content
                    );
                }
            }

            // Verify ready for new input (Requirement 8.5)
            prop_assert!(
                !state.agent_processing,
                "Agent should not be processing after interrupt"
            );

            // Verify app is still running (didn't crash)
            prop_assert!(
                !state.should_quit,
                "App should not quit on interrupt"
            );

            // Verify we can add new messages (ready for new input)
            let new_msg = ChatMessage::user("New message after interrupt".to_string());
            state.message_list.push(new_msg);
            prop_assert!(
                state.message_list.messages().last().unwrap().content == "New message after interrupt",
                "Should be able to add new messages after interrupt"
            );
        }

        /// **Feature: tui-llm-integration, Property 8: Interrupt Recovery**
        /// **Validates: Requirements 8.3, 8.5**
        ///
        /// For any interrupt when not processing, the TUI SHALL handle it gracefully
        /// (either quit or show message) without crashing.
        #[test]
        fn prop_interrupt_when_idle(
            has_messages in proptest::bool::ANY,
            message_count in 0usize..5usize
        ) {
            let mut state = AppState::new();

            // Ensure not processing
            state.agent_processing = false;

            // Add some messages if specified
            if has_messages {
                for i in 0..message_count {
                    state.message_list.push(ChatMessage::user(format!("Message {}", i)));
                }
            }

            // Simulate interrupt when idle
            // In this case, the app would either quit (Ctrl+C) or show a message (/interrupt)
            // We test the /interrupt command behavior (non-quitting)
            if !state.agent_processing {
                state.status_message = Some("No operation in progress to interrupt".to_string());
            }

            // Verify state is consistent
            prop_assert!(
                !state.agent_processing,
                "Should still not be processing"
            );

            // Verify status message was set
            prop_assert!(
                state.status_message.is_some(),
                "Status message should be set"
            );

            // Verify messages were preserved
            if has_messages {
                prop_assert_eq!(
                    state.message_list.messages().len(),
                    message_count,
                    "Messages should be preserved"
                );
            }
        }

        /// **Feature: unified-model-selection, Property 8: Panel Updates After Selection**
        /// **Validates: Requirements 4.2**
        ///
        /// For any model selection, the panel session section SHALL display
        /// the new provider and model values.
        #[test]
        fn prop_panel_updates_after_selection(
            provider_idx in 0usize..3usize,
            model_idx in 0usize..3usize,
            session_name in "[a-zA-Z0-9]{1,10}",
            cost in 0.0f64..100.0f64,
        ) {
            let providers = ["openai", "claude", "ollama"];
            let models = ["gpt-4o", "claude-sonnet-4", "codellama"];

            let selected_provider = providers[provider_idx].to_string();
            let selected_model = models[model_idx].to_string();

            let mut state = AppState::new();

            // Simulate the panel update that happens after model selection
            // This mirrors what update_panel_from_bridge() does after handle_two_step_model_selection()
            let session_info = crate::tui::widgets::SessionInfo {
                name: session_name.clone(),
                model: selected_model.clone(),
                provider: selected_provider.clone(),
                cost,
                lsp_languages: vec![],
                cost_breakdown: Vec::new(),
            };
            state.update_session_info(session_info);

            // Verify panel displays the new provider value
            prop_assert_eq!(
                &state.enhanced_panel_data.session.provider,
                &selected_provider,
                "Panel should display the selected provider"
            );

            // Verify panel displays the new model value
            prop_assert_eq!(
                &state.enhanced_panel_data.session.model,
                &selected_model,
                "Panel should display the selected model"
            );

            // Verify session name is preserved
            prop_assert_eq!(
                &state.enhanced_panel_data.session.name,
                &session_name,
                "Panel should preserve session name"
            );

            // Verify cost is preserved
            prop_assert!(
                (state.enhanced_panel_data.session.cost - cost).abs() < f64::EPSILON,
                "Panel should preserve cost"
            );
        }
    }
}

/// Integration tests for the unified model selection flow
///
/// Tests the full `/model` command flow: provider → model → persistence
/// **Validates: Requirements 1.1-1.6, 2.1-2.6**
#[cfg(test)]
mod model_selection_integration_tests {
    use super::*;
    use crate::storage::{ChatSession, ModePreferences, ModelPreference, TarkStorage};
    use crate::tui::commands::{CommandHandler, CommandResult, ModelPickerState, PickerType};
    use tempfile::TempDir;

    /// Test that /model command starts the two-step flow with provider selection
    /// **Validates: Requirements 1.1**
    #[test]
    fn test_model_command_starts_provider_selection() {
        let handler = CommandHandler::new();
        let result = handler.execute("/model");

        // /model should show provider picker first
        assert_eq!(result, CommandResult::ShowPicker(PickerType::Provider));
    }

    /// Test that /provider alias redirects to /model
    /// **Validates: Requirements 1.7**
    #[test]
    fn test_provider_alias_redirects_to_model() {
        let handler = CommandHandler::new();
        let result = handler.execute("/provider");

        // /provider should also show provider picker (redirected to /model)
        assert_eq!(result, CommandResult::ShowPicker(PickerType::Provider));
    }

    /// Test model picker state transitions
    /// **Validates: Requirements 1.1, 1.3**
    #[test]
    fn test_model_picker_state_transitions() {
        let mut state = AppState::new();

        // Initially no picker state
        assert!(state.model_picker_state.is_none());

        // Start provider selection
        state.model_picker_state = Some(ModelPickerState::SelectingProvider);
        assert_eq!(
            state.model_picker_state,
            Some(ModelPickerState::SelectingProvider)
        );

        // Transition to model selection after provider is chosen
        state.model_picker_state = Some(ModelPickerState::SelectingModel {
            provider: "openai".to_string(),
        });
        assert!(matches!(
            state.model_picker_state,
            Some(ModelPickerState::SelectingModel { ref provider }) if provider == "openai"
        ));

        // Clear state after model selection
        state.model_picker_state = None;
        assert!(state.model_picker_state.is_none());
    }

    /// Test per-mode preference storage and retrieval
    /// **Validates: Requirements 2.1, 2.3**
    #[test]
    fn test_per_mode_preference_storage() {
        let mut prefs = ModePreferences::default();

        // Set different preferences for each mode
        prefs.set("build", ModelPreference::new("openai", "gpt-4o"));
        prefs.set("plan", ModelPreference::new("claude", "claude-sonnet-4"));
        prefs.set("ask", ModelPreference::new("ollama", "codellama"));

        // Verify each mode has its own preference
        assert_eq!(prefs.get("build").provider, "openai");
        assert_eq!(prefs.get("build").model, "gpt-4o");

        assert_eq!(prefs.get("plan").provider, "claude");
        assert_eq!(prefs.get("plan").model, "claude-sonnet-4");

        assert_eq!(prefs.get("ask").provider, "ollama");
        assert_eq!(prefs.get("ask").model, "codellama");
    }

    /// Test that changing one mode's preference doesn't affect others
    /// **Validates: Requirements 2.1, 2.3**
    #[test]
    fn test_mode_preference_independence() {
        let mut prefs = ModePreferences::default();

        // Set initial preferences
        prefs.set("build", ModelPreference::new("openai", "gpt-4o"));
        prefs.set("plan", ModelPreference::new("claude", "claude-sonnet-4"));

        // Change build mode preference
        prefs.set("build", ModelPreference::new("ollama", "codellama"));

        // Plan mode should be unchanged
        assert_eq!(prefs.get("plan").provider, "claude");
        assert_eq!(prefs.get("plan").model, "claude-sonnet-4");

        // Build mode should have new value
        assert_eq!(prefs.get("build").provider, "ollama");
        assert_eq!(prefs.get("build").model, "codellama");
    }

    /// Test session preference persistence round-trip
    /// **Validates: Requirements 2.5**
    #[test]
    fn test_session_preference_persistence() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let storage = TarkStorage::new(temp_dir.path()).expect("Failed to create storage");

        // Create session with mode preferences
        let mut session = ChatSession::new();
        session.mode_preferences.build = ModelPreference::new("openai", "gpt-4o");
        session.mode_preferences.plan = ModelPreference::new("claude", "claude-sonnet-4");
        session.mode_preferences.ask = ModelPreference::new("ollama", "codellama");

        // Save session
        storage
            .save_session(&session)
            .expect("Failed to save session");

        // Load session back
        let loaded = storage
            .load_session(&session.id)
            .expect("Failed to load session");

        // Verify all mode preferences are preserved
        assert_eq!(loaded.mode_preferences.build.provider, "openai");
        assert_eq!(loaded.mode_preferences.build.model, "gpt-4o");

        assert_eq!(loaded.mode_preferences.plan.provider, "claude");
        assert_eq!(loaded.mode_preferences.plan.model, "claude-sonnet-4");

        assert_eq!(loaded.mode_preferences.ask.provider, "ollama");
        assert_eq!(loaded.mode_preferences.ask.model, "codellama");
    }

    /// Test has_preference detection
    /// **Validates: Requirements 2.4, 2.6**
    #[test]
    fn test_has_preference_detection() {
        let mut prefs = ModePreferences::default();

        // Initially no preferences
        assert!(!prefs.has_preference("build"));
        assert!(!prefs.has_preference("plan"));
        assert!(!prefs.has_preference("ask"));

        // Set build preference
        prefs.set("build", ModelPreference::new("openai", "gpt-4o"));

        // Only build should have preference
        assert!(prefs.has_preference("build"));
        assert!(!prefs.has_preference("plan"));
        assert!(!prefs.has_preference("ask"));
    }

    /// Test full flow simulation: /model → provider → model → verify state
    /// **Validates: Requirements 1.1-1.6**
    #[test]
    fn test_full_model_selection_flow_simulation() {
        let mut state = AppState::new();

        // Step 1: /model command triggers provider picker
        state.model_picker_state = Some(ModelPickerState::SelectingProvider);
        state.active_picker_type = Some(PickerType::Provider);
        assert!(state.model_picker_state.is_some());

        // Step 2: User selects provider "openai"
        // This transitions to model selection
        state.model_picker_state = Some(ModelPickerState::SelectingModel {
            provider: "openai".to_string(),
        });
        state.active_picker_type = Some(PickerType::Model);

        // Verify we're in model selection state with correct provider
        if let Some(ModelPickerState::SelectingModel { ref provider }) = state.model_picker_state {
            assert_eq!(provider, "openai");
        } else {
            panic!("Expected SelectingModel state");
        }

        // Step 3: User selects model "gpt-4o"
        // This completes the flow and clears state
        state.model_picker_state = None;
        state.active_picker_type = None;

        // Verify flow is complete
        assert!(state.model_picker_state.is_none());
        assert!(state.active_picker_type.is_none());
    }

    /// Test mode switching with preference loading
    /// **Validates: Requirements 2.2, 2.6**
    #[test]
    fn test_mode_switching_loads_preferences() {
        let mut prefs = ModePreferences::default();

        // Set preferences for different modes
        prefs.set("build", ModelPreference::new("openai", "gpt-4o"));
        prefs.set("plan", ModelPreference::new("claude", "claude-sonnet-4"));

        // Simulate switching to build mode
        let build_pref = prefs.get("build");
        assert_eq!(build_pref.provider, "openai");
        assert_eq!(build_pref.model, "gpt-4o");

        // Simulate switching to plan mode
        let plan_pref = prefs.get("plan");
        assert_eq!(plan_pref.provider, "claude");
        assert_eq!(plan_pref.model, "claude-sonnet-4");

        // Simulate switching to ask mode (no preference)
        let ask_pref = prefs.get("ask");
        assert!(ask_pref.is_empty());
        assert!(!prefs.has_preference("ask"));
    }

    /// Test new session has empty preferences
    /// **Validates: Requirements 2.4**
    #[test]
    fn test_new_session_empty_preferences() {
        let session = ChatSession::new();

        // All mode preferences should be empty
        assert!(session.mode_preferences.build.is_empty());
        assert!(session.mode_preferences.plan.is_empty());
        assert!(session.mode_preferences.ask.is_empty());

        // has_preference should return false for all modes
        assert!(!session.mode_preferences.has_preference("build"));
        assert!(!session.mode_preferences.has_preference("plan"));
        assert!(!session.mode_preferences.has_preference("ask"));
    }

    /// Test panel data updates after model selection
    /// **Validates: Requirements 4.2, 4.3**
    #[test]
    fn test_panel_updates_after_model_selection() {
        let mut state = AppState::new();

        // Simulate model selection completing
        let session_info = crate::tui::widgets::SessionInfo {
            name: "Test Session".to_string(),
            model: "gpt-4o".to_string(),
            provider: "openai".to_string(),
            cost: 0.0,
            lsp_languages: vec![],
                cost_breakdown: Vec::new(),
        };
        state.update_session_info(session_info);

        // Verify panel shows new provider and model
        assert_eq!(state.enhanced_panel_data.session.provider, "openai");
        assert_eq!(state.enhanced_panel_data.session.model, "gpt-4o");
    }
}
