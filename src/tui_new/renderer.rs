//! TUI Renderer - Implements UiRenderer trait for terminal display
//!
//! This module provides the rendering implementation for the TUI,
//! separate from business logic which is handled by AppService.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::time::Duration;

use crate::ui_backend::UiRenderer;
use crate::ui_backend::{
    AppEvent, Command, FocusedComponent, MessageRole as UiMessageRole, ModalType, SharedState,
};

use super::modals::{ApprovalModal, DeviceFlowModal, PluginModal, ToolsModal, TrustModal};
use super::theme::Theme;
use super::widgets::{
    FilePickerModal, GitChange, GitStatus, Header, HelpModal, InputWidget, MessageArea,
    ModelPickerModal, ProviderPickerModal, QuestionOption, QuestionWidget, SessionInfo, Sidebar,
    StatusBar, Task, TaskStatus, TerminalFrame, ThemePickerModal,
};

/// Click target for hit testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClickTarget {
    Header,
    Messages,
    Input,
    StatusBar,
    Sidebar,
    Modal,
    Outside,
}

/// TUI Renderer implementation
pub struct TuiRenderer<B: Backend> {
    /// Terminal instance
    terminal: Terminal<B>,
    /// Current theme (cached for rendering)
    theme: Theme,
}

impl<B: Backend> TuiRenderer<B> {
    /// Create a new TUI renderer
    pub fn new(terminal: Terminal<B>) -> Self {
        Self {
            terminal,
            theme: Theme::default(),
        }
    }

    /// Get reference to terminal (for testing)
    pub fn terminal(&self) -> &Terminal<B> {
        &self.terminal
    }

    /// Get mutable reference to terminal
    pub fn terminal_mut(&mut self) -> &mut Terminal<B> {
        &mut self.terminal
    }

    /// Convert keyboard event to command
    fn key_to_command(key: event::KeyEvent, state: &SharedState) -> Option<Command> {
        match (key.code, key.modifiers) {
            // Application control
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                // If LLM is processing, send interrupt instead of quit
                if state.llm_processing() {
                    Some(Command::Interrupt)
                } else {
                    Some(Command::Quit)
                }
            }
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(Command::Quit),
            (KeyCode::Char('?'), _) => Some(Command::ToggleHelp),

            // Focus management
            (KeyCode::Tab, KeyModifiers::NONE) => {
                if state.active_modal().is_none() {
                    Some(Command::FocusNext)
                } else {
                    None
                }
            }
            // SHIFT+TAB cycles agent mode (handle both BackTab and Tab+SHIFT for cross-terminal compatibility)
            (KeyCode::BackTab, KeyModifiers::SHIFT) | (KeyCode::Tab, KeyModifiers::SHIFT) => {
                Some(Command::CycleAgentMode)
            }

            // Mode cycling
            (KeyCode::Char('m'), KeyModifiers::CONTROL) => Some(Command::CycleBuildMode),

            // Approval mode (Alt+A or Shift+A) - only in Build mode
            (KeyCode::Char('a'), KeyModifiers::ALT) | (KeyCode::Char('A'), KeyModifiers::SHIFT) => {
                if state.agent_mode() == crate::ui_backend::AgentMode::Build {
                    Some(Command::OpenTrustLevelSelector)
                } else {
                    None
                }
            }

            // UI toggles
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Command::ToggleSidebar),
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => Some(Command::ToggleThinking),

            // Vim keybindings for messages panel and sidebar (must be before general char handler)
            (KeyCode::Char('j'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                match state.focused_component() {
                    FocusedComponent::Messages if state.vim_mode() == VimMode::Normal => {
                        Some(Command::ScrollDown)
                    }
                    FocusedComponent::Panel if state.vim_mode() == VimMode::Normal => {
                        Some(Command::SidebarDown)
                    }
                    _ => None,
                }
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                match state.focused_component() {
                    FocusedComponent::Messages if state.vim_mode() == VimMode::Normal => {
                        Some(Command::ScrollUp)
                    }
                    FocusedComponent::Panel if state.vim_mode() == VimMode::Normal => {
                        Some(Command::SidebarUp)
                    }
                    _ => None,
                }
            }
            (KeyCode::Char('y'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    Some(Command::YankMessage)
                } else {
                    None
                }
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                use crate::ui_backend::VimMode;
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    // Scroll to bottom
                    let msg_count = state.message_count();
                    if msg_count > 0 {
                        state.set_messages_scroll_offset(msg_count.saturating_sub(1));
                    }
                    None
                } else {
                    None
                }
            }
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    // Scroll to top
                    state.set_messages_scroll_offset(0);
                    None
                } else {
                    None
                }
            }

            // Escape to close modal or switch to Normal mode
            (KeyCode::Esc, _) => {
                if state.active_modal().is_some() {
                    Some(Command::CloseModal)
                } else {
                    use crate::ui_backend::VimMode;
                    // Switch to Normal mode when in Input or Messages
                    match state.focused_component() {
                        FocusedComponent::Input | FocusedComponent::Messages => {
                            Some(Command::SetVimMode(VimMode::Normal))
                        }
                        _ => Some(Command::ClearInput),
                    }
                }
            }

            // Enter handling
            (KeyCode::Enter, KeyModifiers::SHIFT) => {
                if matches!(state.focused_component(), FocusedComponent::Input) {
                    Some(Command::InsertNewline)
                } else {
                    None
                }
            }
            (KeyCode::Enter, KeyModifiers::NONE) => {
                if state.active_questionnaire().is_some() {
                    Some(Command::QuestionSubmit)
                } else if let Some(modal) = state.active_modal() {
                    match modal {
                        ModalType::TrustLevel => {
                            let selected = state.trust_level_selected();
                            let level = crate::tools::TrustLevel::from_index(selected);
                            Some(Command::SetTrustLevel(level))
                        }
                        ModalType::FilePicker => Some(Command::FilePickerSelect),
                        _ => Some(Command::ConfirmModal),
                    }
                } else if matches!(state.focused_component(), FocusedComponent::Input) {
                    let text = state.input_text();
                    Some(Command::SendMessage(text))
                } else {
                    None
                }
            }

            // Text editing (only in input focus)
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                // Questionnaire has priority
                if state.active_questionnaire().is_some() && c == ' ' {
                    Some(Command::QuestionToggle)
                } else {
                    match state.active_modal() {
                        Some(ModalType::Approval) => {
                            // Handle approval actions
                            match c.to_ascii_lowercase() {
                                'y' => Some(Command::ApproveOperation),
                                'n' => Some(Command::DenyOperation),
                                _ => None,
                            }
                        }
                        Some(ModalType::FilePicker) => {
                            // Update file picker filter
                            let current_filter = state.file_picker_filter();
                            Some(Command::UpdateFilePickerFilter(format!(
                                "{}{}",
                                current_filter, c
                            )))
                        }
                        Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker) => Some(Command::ModalFilter(c.to_string())),
                        _ => {
                            use crate::ui_backend::VimMode;

                            let vim_mode = state.vim_mode();
                            let focused = state.focused_component();

                            // Vim mode switching commands (work in Normal mode)
                            if vim_mode == VimMode::Normal && focused == FocusedComponent::Input {
                                match c {
                                    'i' => return Some(Command::SetVimMode(VimMode::Insert)),
                                    'v' => return Some(Command::SetVimMode(VimMode::Visual)),
                                    'a' => {
                                        // Enter insert mode after cursor
                                        return Some(Command::SetVimMode(VimMode::Insert));
                                    }
                                    _ => {}
                                }
                            }

                            // Text editing only works in Insert mode
                            if vim_mode == VimMode::Insert && focused == FocusedComponent::Input {
                                Some(Command::InsertChar(c))
                            } else {
                                None
                            }
                        }
                    }
                }
            }

            // Backspace
            (KeyCode::Backspace, _) => {
                use crate::ui_backend::VimMode;

                match state.active_modal() {
                    Some(ModalType::FilePicker) => {
                        let current_filter = state.file_picker_filter();
                        if !current_filter.is_empty() {
                            let mut filter = current_filter;
                            filter.pop();
                            Some(Command::UpdateFilePickerFilter(filter))
                        } else {
                            None
                        }
                    }
                    Some(ModalType::ThemePicker)
                    | Some(ModalType::ProviderPicker)
                    | Some(ModalType::ModelPicker) => Some(Command::ModalFilter(String::new())), // Signal to pop
                    _ => {
                        // Only allow backspace in Insert mode
                        if state.vim_mode() == VimMode::Insert
                            && matches!(state.focused_component(), FocusedComponent::Input)
                        {
                            Some(Command::DeleteCharBefore)
                        } else {
                            None
                        }
                    }
                }
            }

            // Cursor movement
            (KeyCode::Left, KeyModifiers::NONE) => Some(Command::CursorLeft),
            (KeyCode::Right, KeyModifiers::NONE) => Some(Command::CursorRight),
            (KeyCode::Home, _) => Some(Command::CursorToLineStart),
            (KeyCode::End, _) => Some(Command::CursorToLineEnd),
            (KeyCode::Left, KeyModifiers::CONTROL) => Some(Command::CursorWordBackward),
            (KeyCode::Right, KeyModifiers::CONTROL) => Some(Command::CursorWordForward),

            // Arrow key navigation (context-dependent)
            (KeyCode::Up, _) => {
                if state.active_questionnaire().is_some() {
                    Some(Command::QuestionUp)
                } else {
                    match state.active_modal() {
                        Some(ModalType::Tools) => {
                            let selected = state.tools_selected();
                            if selected > 0 {
                                state.set_tools_selected(selected - 1);
                            }
                            None
                        }
                        Some(ModalType::TrustLevel) => {
                            let selected = state.trust_level_selected();
                            if selected > 0 {
                                state.set_trust_level_selected(selected - 1);
                            }
                            None
                        }
                        Some(ModalType::FilePicker) => Some(Command::FilePickerUp),
                        Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker) => Some(Command::ModalUp),
                        _ if matches!(state.focused_component(), FocusedComponent::Panel) => None,
                        _ => Some(Command::HistoryPrevious),
                    }
                }
            }
            (KeyCode::Down, _) => {
                if state.active_questionnaire().is_some() {
                    Some(Command::QuestionDown)
                } else {
                    match state.active_modal() {
                        Some(ModalType::Tools) => {
                            let selected = state.tools_selected();
                            // Limit navigation based on actual tool count (handled in service)
                            state.set_tools_selected(selected + 1);
                            None
                        }
                        Some(ModalType::TrustLevel) => {
                            let selected = state.trust_level_selected();
                            if selected < 2 {
                                // 0=Manual, 1=Balanced, 2=Careful
                                state.set_trust_level_selected(selected + 1);
                            }
                            None
                        }
                        Some(ModalType::FilePicker) => Some(Command::FilePickerDown),
                        Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker) => Some(Command::ModalDown),
                        _ if matches!(state.focused_component(), FocusedComponent::Panel) => None,
                        _ => Some(Command::HistoryNext),
                    }
                }
            }

            _ => None,
        }
    }

    /// Convert mouse event to command
    fn mouse_to_command(&self, mouse: MouseEvent, state: &SharedState) -> Option<Command> {
        // Debug log mouse events
        tracing::debug!(
            "Mouse event: kind={:?}, col={}, row={}",
            mouse.kind,
            mouse.column,
            mouse.row
        );

        match mouse.kind {
            MouseEventKind::ScrollDown => {
                tracing::debug!("Scroll down event detected");
                // Route scroll to the correct component
                if state.active_modal().is_some() {
                    Some(Command::ModalDown)
                } else {
                    match state.focused_component() {
                        FocusedComponent::Panel => Some(Command::SidebarDown),
                        _ => Some(Command::ScrollDown),
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                tracing::debug!("Scroll up event detected");
                // Route scroll to the correct component
                if state.active_modal().is_some() {
                    Some(Command::ModalUp)
                } else {
                    match state.focused_component() {
                        FocusedComponent::Panel => Some(Command::SidebarUp),
                        _ => Some(Command::ScrollUp),
                    }
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                tracing::debug!("Left click event detected");
                // Handle clicks - delegate to hit testing
                self.handle_mouse_click(mouse.column, mouse.row, state)
            }
            _ => {
                tracing::debug!("Unhandled mouse event: {:?}", mouse.kind);
                None
            }
        }
    }

    /// Handle mouse click and determine which component was clicked
    fn handle_mouse_click(&self, col: u16, row: u16, state: &SharedState) -> Option<Command> {
        let target = self.hit_test(col, row, state);

        match target {
            ClickTarget::Messages => Some(Command::FocusMessages),
            ClickTarget::Input => Some(Command::FocusInput),
            ClickTarget::Sidebar => {
                // Determine which sidebar panel was clicked based on row
                let panel_idx = self.get_clicked_sidebar_panel(row, state);
                if let Some(idx) = panel_idx {
                    // Toggle the clicked panel
                    Some(Command::ToggleSidebarPanel(idx))
                } else {
                    // Just focus sidebar if we can't determine panel
                    Some(Command::FocusPanel)
                }
            }
            ClickTarget::StatusBar => {
                // Determine which section of status bar was clicked
                self.hit_test_status_bar(col, state)
            }
            ClickTarget::Modal => {
                // Modal clicks will select items - handled in modal navigation
                None
            }
            ClickTarget::Outside if state.active_modal().is_some() => {
                // Click outside modal closes it
                Some(Command::CloseModal)
            }
            _ => None,
        }
    }

    /// Determine which sidebar panel was clicked based on row position
    fn get_clicked_sidebar_panel(&self, row: u16, _state: &SharedState) -> Option<usize> {
        let size = self.terminal.size().unwrap_or_default();

        // Sidebar starts at inner_y (which is 1 for the border)
        let inner_y = 1u16;
        let inner_height = size.height.saturating_sub(2);

        // Calculate approximate panel positions
        // Sidebar layout: each panel has a header line plus content
        // We'll approximate: if clicked in top 25% -> panel 0, next 25% -> panel 1, etc.
        if row < inner_y {
            return None;
        }

        let relative_row = row - inner_y;
        let quarter = inner_height / 4;

        if relative_row < quarter {
            Some(0) // Session panel
        } else if relative_row < quarter * 2 {
            Some(1) // Context panel
        } else if relative_row < quarter * 3 {
            Some(2) // Tasks panel
        } else if relative_row < inner_height {
            Some(3) // Git changes panel
        } else {
            None
        }
    }

    /// Hit test status bar to determine which icon/section was clicked
    fn hit_test_status_bar(&self, col: u16, state: &SharedState) -> Option<Command> {
        // Status bar layout (approximate positions):
        // "agent • Build ▼  🟢 Balanced ▼  🧠  ≡ 7  VIM(INSERT)    ● Working...    • Model Provider  ⊙"
        //  0-15: Agent mode area
        //  16-35: Build mode area
        //  36-39: Thinking icon (🧠)
        //  40-50: Queue indicator
        //  51-65: Vim mode
        //  Right side (-30 to end): Provider/Model

        let size = self.terminal.size().unwrap_or_default();
        let status_width = size.width.saturating_sub(2); // Account for borders

        // Relative position from start of status bar
        let rel_col = col.saturating_sub(1); // Account for left border

        // Agent mode area (0-15)
        if rel_col < 15 {
            return Some(Command::CycleAgentMode);
        }

        // Build mode area (16-35) - only if in Build agent mode
        if (16..35).contains(&rel_col) && state.agent_mode() == crate::ui_backend::AgentMode::Build
        {
            return Some(Command::CycleBuildMode);
        }

        // Thinking icon (36-40)
        if (36..40).contains(&rel_col) {
            return Some(Command::ToggleThinking);
        }

        // Provider/Model on right (last 30 chars)
        if rel_col > status_width.saturating_sub(30) {
            return Some(Command::OpenProviderPicker);
        }

        // Default: no action
        None
    }

    /// Perform hit testing to determine which component was clicked
    fn hit_test(&self, col: u16, row: u16, state: &SharedState) -> ClickTarget {
        let size = self.terminal.size().unwrap_or_default();
        let area = size;

        // Account for border frame (1px on each side)
        if col == 0 || row == 0 || col >= area.width - 1 || row >= area.height - 1 {
            return ClickTarget::Outside;
        }

        // Inner area (inside frame borders)
        let inner_x = 1u16;
        let inner_y = 1u16;
        let inner_width = area.width.saturating_sub(2);
        let inner_height = area.height.saturating_sub(2);

        // Check if modal is active
        if state.active_modal().is_some() {
            // Modal is centered, occupying ~60% width and height
            let modal_width = inner_width * 60 / 100;
            let modal_height = inner_height * 60 / 100;
            let modal_x = inner_x + (inner_width.saturating_sub(modal_width)) / 2;
            let modal_y = inner_y + (inner_height.saturating_sub(modal_height)) / 2;

            if col >= modal_x
                && col < modal_x + modal_width
                && row >= modal_y
                && row < modal_y + modal_height
            {
                return ClickTarget::Modal;
            } else {
                return ClickTarget::Outside;
            }
        }

        // Check if sidebar is visible
        let sidebar_visible = state.sidebar_visible();
        let (main_x, main_width, sidebar_x, sidebar_width) = if sidebar_visible && inner_width > 80
        {
            // Sidebar is 35 chars wide on the right
            let main_w = inner_width.saturating_sub(35);
            let sidebar_w = 35;
            let sidebar_start = inner_x + main_w;
            (inner_x, main_w, Some(sidebar_start), Some(sidebar_w))
        } else {
            (inner_x, inner_width, None, None)
        };

        // Check if click is in sidebar
        if let (Some(sb_x), Some(sb_w)) = (sidebar_x, sidebar_width) {
            if col >= sb_x && col < sb_x + sb_w && row >= inner_y && row < inner_y + inner_height {
                return ClickTarget::Sidebar;
            }
        }

        // Main area vertical layout: Header(2) | Messages(Min 5) | Input(5) | Status(1)
        let header_height = 2u16;
        let input_height = 5u16;
        let status_height = 1u16;

        let header_y = inner_y;
        let messages_y = header_y + header_height;
        let status_y = inner_y + inner_height - status_height;
        let input_y = status_y.saturating_sub(input_height);

        // Determine which vertical section was clicked
        if col >= main_x && col < main_x + main_width {
            if row >= header_y && row < messages_y {
                return ClickTarget::Header;
            } else if row >= messages_y && row < input_y {
                return ClickTarget::Messages;
            } else if row >= input_y && row < status_y {
                return ClickTarget::Input;
            } else if row >= status_y && row < status_y + status_height {
                return ClickTarget::StatusBar;
            }
        }

        ClickTarget::Outside
    }
}

impl<B: Backend> UiRenderer for TuiRenderer<B> {
    fn render(&mut self, state: &SharedState) -> Result<()> {
        use ratatui::layout::{Constraint, Direction, Layout};

        // Update theme if changed
        let theme_preset = state.theme();
        self.theme = Theme::from_preset(theme_preset);

        let theme = &self.theme;
        let messages = state.messages();
        let active_modal = state.active_modal();
        let sidebar_visible = state.sidebar_visible();
        let context_files = state.context_files();
        let input_text = state.input_text();
        let input_cursor = state.input_cursor();
        let focused_component = state.focused_component();
        let agent_mode = state.agent_mode();
        let build_mode = state.build_mode();
        let thinking_enabled = state.thinking_enabled();
        let llm_processing = state.llm_processing();
        let current_provider = state.current_provider();
        let current_model = state.current_model();

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
                    Constraint::Length(5), // Input area
                    Constraint::Length(1), // Status bar
                ])
                .split(main_area);

            // Render header
            let config = super::config::AppConfig::default();
            let header = Header::new(&config, theme);
            frame.render_widget(header, chunks[0]);

            // Render message area
            let message_widgets: Vec<_> = messages
                .iter()
                .map(|m| super::widgets::Message {
                    role: match m.role {
                        UiMessageRole::User => super::widgets::MessageRole::User,
                        UiMessageRole::Assistant => super::widgets::MessageRole::Agent,
                        UiMessageRole::System => super::widgets::MessageRole::System,
                    },
                    content: m.content.clone(),
                    collapsed: m.collapsed,
                    question: None, // TODO: map questions if needed
                })
                .collect();

            let streaming_content = state.streaming_content();
            let streaming_thinking = if thinking_enabled {
                state.streaming_thinking()
            } else {
                None
            };

            let message_area = MessageArea::new(&message_widgets, theme)
                .agent_name(&config.agent_name_short)
                .focused(matches!(focused_component, FocusedComponent::Messages))
                .focused_index(state.focused_message())
                .streaming_content(streaming_content)
                .streaming_thinking(streaming_thinking);
            frame.render_widget(message_area, chunks[1]);

            // Render input area
            let context_file_paths: Vec<String> =
                context_files.iter().map(|f| f.path.clone()).collect();
            let input = InputWidget::new(&input_text, input_cursor, theme)
                .focused(matches!(focused_component, FocusedComponent::Input))
                .context_files(context_file_paths);
            frame.render_widget(input, chunks[2]);

            // Render status bar
            let vim_mode = state.vim_mode();
            let queued_count = state.queued_message_count();

            let mut status = StatusBar::new(theme)
                .agent_mode(agent_mode)
                .build_mode(build_mode)
                .thinking(thinking_enabled)
                .queue(queued_count) // Show actual queue count
                .processing(llm_processing)
                .vim_mode(vim_mode);

            // Set provider and model if available
            if let Some(ref provider) = current_provider {
                status = status.provider(provider);
            }
            if let Some(ref model) = current_model {
                status = status.model(model);
            }

            frame.render_widget(status, chunks[3]);

            // Render sidebar if visible
            if let Some(sidebar_rect) = sidebar_area {
                let is_sidebar_focused = focused_component == FocusedComponent::Panel;
                let current_theme_name = theme_preset.display_name().to_string();

                // Get real session info from state
                let session_info = state
                    .session()
                    .map(|s| SessionInfo {
                        branch: s.branch,
                        total_cost: s.total_cost,
                        model_count: s.model_count,
                    })
                    .unwrap_or_else(|| SessionInfo {
                        branch: "main".to_string(),
                        total_cost: 0.0,
                        model_count: 0,
                    });

                // Get real tasks from state
                let tasks_widget: Vec<Task> = state
                    .tasks()
                    .iter()
                    .map(|t| Task {
                        name: t.name.clone(),
                        status: match t.status {
                            crate::ui_backend::TaskStatus::Queued => TaskStatus::Queued,
                            crate::ui_backend::TaskStatus::Active => TaskStatus::Active,
                            crate::ui_backend::TaskStatus::Completed => TaskStatus::Completed,
                            crate::ui_backend::TaskStatus::Failed => TaskStatus::Queued, // Map failed to queued for display
                        },
                    })
                    .collect();

                // Get real git changes from state
                let git_changes_widget: Vec<GitChange> = state
                    .git_changes()
                    .iter()
                    .map(|g| GitChange {
                        file: g.file.clone(),
                        status: match g.status {
                            crate::ui_backend::GitStatus::Modified => GitStatus::Modified,
                            crate::ui_backend::GitStatus::Added => GitStatus::Added,
                            crate::ui_backend::GitStatus::Deleted => GitStatus::Deleted,
                            crate::ui_backend::GitStatus::Renamed => GitStatus::Modified, // Map renamed to modified for display
                            crate::ui_backend::GitStatus::Untracked => GitStatus::Added, // Map untracked to added for display
                        },
                        additions: g.additions,
                        deletions: g.deletions,
                    })
                    .collect();

                let mut sidebar = Sidebar::new(theme)
                    .visible(true)
                    .theme_name(current_theme_name)
                    .focused(is_sidebar_focused)
                    .selected_panel(state.sidebar_selected_panel())
                    .vim_mode(vim_mode)
                    .session_info(session_info)
                    .context_files(
                        context_files
                            .iter()
                            .map(|f| {
                                if f.token_count > 0 {
                                    format!("{} ({} tokens)", f.path, f.token_count)
                                } else {
                                    f.path.clone()
                                }
                            })
                            .collect(),
                    )
                    .tokens(state.tokens_used(), state.tokens_total())
                    .tasks(tasks_widget)
                    .git_changes(git_changes_widget);

                sidebar.expanded_panels = state.sidebar_expanded_panels();
                sidebar.selected_item = state.sidebar_selected_item();

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
                        let providers_data: Vec<_> = state
                            .available_providers()
                            .iter()
                            .map(|p| {
                                (
                                    p.name.clone(),
                                    p.icon.clone(),
                                    p.description.clone(),
                                    p.configured,
                                )
                            })
                            .collect();
                        let picker = ProviderPickerModal::new(theme)
                            .providers(providers_data)
                            .selected(state.provider_picker_selected())
                            .filter(state.provider_picker_filter());
                        frame.render_widget(picker, area);
                    }
                    ModalType::ThemePicker => {
                        let filter = state.theme_picker_filter();
                        let picker = ThemePickerModal::new(theme, theme_preset, &filter)
                            .selected(state.theme_picker_selected());
                        frame.render_widget(picker, area);
                    }
                    ModalType::ModelPicker => {
                        let current_model = state.current_model();
                        let models_data: Vec<_> = state
                            .available_models()
                            .iter()
                            .map(|m| {
                                let is_current = current_model
                                    .as_ref()
                                    .map(|cm| cm == &m.id)
                                    .unwrap_or(false);
                                (m.name.clone(), m.description.clone(), is_current)
                            })
                            .collect();
                        let picker = ModelPickerModal::new(theme)
                            .models(models_data)
                            .selected(state.model_picker_selected())
                            .filter(state.model_picker_filter());
                        frame.render_widget(picker, area);
                    }
                    ModalType::FilePicker => {
                        let files = state.file_picker_files();
                        let filter = state.file_picker_filter();
                        let selected = state.file_picker_selected();
                        let picker = FilePickerModal::new(theme)
                            .files(&files)
                            .filter(&filter)
                            .selected(selected);
                        frame.render_widget(picker, area);
                    }
                    ModalType::Approval => {
                        if let Some(approval) = state.pending_approval() {
                            let modal = ApprovalModal::new(theme, &approval);
                            frame.render_widget(modal, area);
                        }
                    }
                    ModalType::TrustLevel => {
                        let current_level = state.trust_level();
                        let selected = state.trust_level_selected();
                        let modal = TrustModal::new(theme, current_level).selected(selected);
                        frame.render_widget(modal, area);
                    }
                    ModalType::Tools => {
                        // Get tools from service (via controller helper or store in state)
                        // For now, create empty modal - tools should be cached in state
                        let modal =
                            ToolsModal::new(theme, agent_mode).selected(state.tools_selected());
                        frame.render_widget(modal, area);
                    }
                    ModalType::Plugin => {
                        let modal = PluginModal::new(theme);
                        frame.render_widget(modal, area);
                    }
                    ModalType::DeviceFlow => {
                        if let Some(session) = state.device_flow_session() {
                            let modal = DeviceFlowModal::new(theme, &session);
                            frame.render_widget(modal, area);
                        }
                    }
                }
            }

            // Render questionnaire if active (on top of modals)
            if let Some(q) = state.active_questionnaire() {
                use super::widgets::QuestionType as WidgetQuestionType;
                use crate::ui_backend::questionnaire::QuestionType as StateQuestionType;

                // Convert QuestionnaireState to QuestionWidget
                let widget_question_type = match q.question_type {
                    StateQuestionType::SingleChoice => WidgetQuestionType::SingleChoice,
                    StateQuestionType::MultipleChoice => WidgetQuestionType::MultipleChoice,
                    StateQuestionType::FreeText => WidgetQuestionType::FreeText,
                };

                let options: Vec<QuestionOption> = q
                    .options
                    .iter()
                    .map(|opt| QuestionOption {
                        text: opt.text.clone(),
                        value: opt.value.clone(),
                    })
                    .collect();

                let question_widget = QuestionWidget {
                    question_type: widget_question_type,
                    text: q.question.clone(),
                    options,
                    selected: q.selected.iter().copied().collect(),
                    focused_index: 0,
                    free_text_answer: q.free_text_answer.clone(),
                    answered: q.answered,
                };

                // Center the questionnaire
                let question_width = area.width.min(70);
                let question_height = area.height.min(20);
                let question_area = Rect {
                    x: (area.width.saturating_sub(question_width)) / 2,
                    y: (area.height.saturating_sub(question_height)) / 2,
                    width: question_width,
                    height: question_height,
                };

                frame.render_widget(&question_widget, question_area);
            }
        })?;

        Ok(())
    }

    fn poll_input(&mut self, state: &SharedState) -> Result<Option<Command>> {
        // Non-blocking poll with short timeout
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    return Ok(Self::key_to_command(key, state));
                }
                Event::Mouse(mouse) => {
                    return Ok(self.mouse_to_command(mouse, state));
                }
                Event::Resize(_, _) => {
                    // Terminal resize handled automatically by ratatui
                }
                _ => {}
            }
        }
        Ok(None)
    }

    fn handle_event(&mut self, event: &AppEvent, _state: &SharedState) -> Result<()> {
        // Renderer no longer accumulates streaming text.
        // All accumulation happens in BFF layer (SharedState.streaming_content).
        // Events are only used to trigger UI refresh.
        match event {
            AppEvent::LlmTextChunk(_)
            | AppEvent::LlmThinkingChunk(_)
            | AppEvent::LlmCompleted { .. }
            | AppEvent::LlmError(_) => {
                // Just trigger a render cycle - state is already updated by BFF
            }
            _ => {}
        }
        Ok(())
    }

    fn get_size(&self) -> (u16, u16) {
        let size = self.terminal.size().unwrap_or_default();
        (size.width, size.height)
    }

    fn should_quit(&self, state: &SharedState) -> bool {
        state.should_quit()
    }
}
