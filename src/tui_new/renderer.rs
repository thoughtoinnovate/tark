//! TUI Renderer - Implements UiRenderer trait for terminal display
//!
//! This module provides the rendering implementation for the TUI,
//! separate from business logic which is handled by AppService.

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::backend::Backend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::ui_backend::UiRenderer;
use crate::ui_backend::{
    AppEvent, Command, FocusedComponent, Message, MessageRole as UiMessageRole, ModalType,
    SharedState, TaskStatus as StateTaskStatus,
};

use super::modals::{
    ApprovalModal, DeviceFlowModal, PluginModal, SessionSwitchConfirmModal, TaskDeleteConfirmModal,
    TaskEditModal, ToolsModal, TrustModal,
};
use super::theme::Theme;
use super::widgets::{
    FilePickerModal, FlashBar, FlashBarState, GitChange, GitStatus, Header, HelpModal, InputWidget,
    MessageArea, ModelPickerModal, ProviderPickerModal, QuestionOption, QuestionWidget,
    SessionInfo, SessionPickerModal, Sidebar, StatusBar, Task, TaskStatus, TerminalFrame,
    ThemePickerModal,
};

fn resolve_model_display_name(
    state: &SharedState,
    current_model: &Option<String>,
) -> Option<String> {
    current_model.as_ref().map(|model| {
        state
            .available_models()
            .iter()
            .find(|m| &m.id == model)
            .map(|m| m.name.clone())
            .unwrap_or_else(|| model.clone())
    })
}

fn build_status_message(state: &SharedState) -> (FlashBarState, Option<String>) {
    if let Some(retry_at) = state.rate_limit_retry_at() {
        let now = std::time::Instant::now();
        if retry_at > now {
            let remaining = retry_at.saturating_duration_since(now).as_secs().max(1);
            return (
                FlashBarState::Warning,
                Some(format!("Rate limited · retrying in {}s", remaining)),
            );
        }
    }

    let flash_state = state.flash_bar_state();
    let message = state.status_message();

    if message.is_some() {
        let resolved_state = match flash_state {
            FlashBarState::Error | FlashBarState::Warning => flash_state,
            FlashBarState::Idle | FlashBarState::Working => FlashBarState::Warning,
        };
        (resolved_state, message)
    } else if flash_state == FlashBarState::Idle {
        let idle_elapsed = state.idle_elapsed();
        if idle_elapsed >= Duration::from_secs(10) {
            (FlashBarState::Idle, None)
        } else if idle_elapsed >= Duration::from_secs(2) {
            (FlashBarState::Idle, Some("Ready".to_string()))
        } else {
            (FlashBarState::Idle, None)
        }
    } else {
        (flash_state, None)
    }
}

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
    /// Last time ESC was pressed (for double-ESC detection)
    last_esc_time: Option<Instant>,
    /// Working directory for git context
    working_dir: PathBuf,
    /// Cache for incremental markdown rendering during streaming
    streaming_markdown_cache: super::widgets::markdown::StreamingMarkdownCache,
    /// Cache for incremental thinking markdown rendering during streaming
    streaming_thinking_cache: super::widgets::markdown::StreamingMarkdownCache,
}

impl<B: Backend> TuiRenderer<B> {
    /// Create a new TUI renderer
    pub fn new(terminal: Terminal<B>, working_dir: PathBuf) -> Self {
        Self {
            terminal,
            theme: Theme::default(),
            last_esc_time: None,
            working_dir,
            streaming_markdown_cache: super::widgets::markdown::StreamingMarkdownCache::new(),
            streaming_thinking_cache: super::widgets::markdown::StreamingMarkdownCache::new(),
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

    fn handle_text_char(c: char, state: &SharedState) -> Option<Command> {
        if let Some(q) = state.active_questionnaire() {
            use crate::ui_backend::questionnaire::QuestionType;

            if q.question_type == QuestionType::FreeText {
                // Free text: only insert if in edit mode
                if q.is_editing_free_text {
                    state.questionnaire_insert_char(c);
                }
                // Block all typing when not in edit mode
                None
            } else if q.is_editing_other() {
                // "Other" is focused and selected: typing goes to other_text
                state.questionnaire_insert_char(c);
                None
            } else if c == ' ' && q.question_type == QuestionType::MultipleChoice {
                // Space only toggles for multiple choice (checkboxes)
                // Single choice auto-selects on navigation, no space needed
                Some(Command::QuestionToggle)
            } else {
                // Block other keys from going to prompt when questionnaire is active
                None
            }
        } else {
            match state.active_modal() {
                Some(ModalType::Approval) => {
                    // Handle approval actions
                    match c.to_ascii_lowercase() {
                        'r' => Some(Command::ApproveOperation),
                        'a' => Some(Command::ApproveAlways),
                        'p' => Some(Command::ApproveSession),
                        's' => Some(Command::DenyOperation),
                        _ => None,
                    }
                }
                Some(ModalType::TaskDeleteConfirm) => {
                    // Quick keys for delete confirmation
                    match c.to_ascii_lowercase() {
                        'y' => Some(Command::ConfirmDeleteTask),
                        'n' => Some(Command::CancelDeleteTask),
                        _ => None,
                    }
                }
                Some(ModalType::TaskEdit) => {
                    // In edit modal, characters go to the edit buffer
                    let mut content = state.editing_task_content();
                    content.push(c);
                    state.set_editing_task_content(content);
                    None
                }
                Some(ModalType::FilePicker) => {
                    // FilePicker is an overlay - pass through to input
                    // Update both file picker filter and input
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}{}", current_filter, c));

                    // Also insert into input if focused on input
                    if state.focused_component() == FocusedComponent::Input {
                        Some(Command::InsertChar(c))
                    } else {
                        None
                    }
                }
                Some(ModalType::ThemePicker)
                | Some(ModalType::ProviderPicker)
                | Some(ModalType::ModelPicker)
                | Some(ModalType::SessionPicker) => Some(Command::ModalFilter(c.to_string())),
                _ => {
                    use crate::ui_backend::VimMode;

                    let vim_mode = state.vim_mode();
                    let focused = state.focused_component();

                    // Handle pending operator in Normal mode (e.g., dd, dw)
                    if vim_mode == VimMode::Normal && focused == FocusedComponent::Input {
                        if let Some(op) = state.pending_operator() {
                            state.set_pending_operator(None);
                            match (op, c) {
                                ('d', 'd') => return Some(Command::DeleteLine),
                                ('d', 'w') => return Some(Command::DeleteWord),
                                _ => {}
                            }
                        }
                    }

                    // Vim mode commands (work in Normal mode for Input)
                    if vim_mode == VimMode::Normal && focused == FocusedComponent::Input {
                        match c {
                            // Mode switching
                            'i' => return Some(Command::SetVimMode(VimMode::Insert)),
                            'v' => return Some(Command::SetVimMode(VimMode::Visual)),
                            'a' => {
                                // Enter insert mode after cursor (move right then insert)
                                state.move_cursor_right();
                                return Some(Command::SetVimMode(VimMode::Insert));
                            }
                            'A' => {
                                // Enter insert mode at end of line
                                state.move_cursor_to_line_end();
                                return Some(Command::SetVimMode(VimMode::Insert));
                            }
                            'I' => {
                                // Enter insert mode at start of line
                                state.move_cursor_to_line_start();
                                return Some(Command::SetVimMode(VimMode::Insert));
                            }
                            // Operator-pending commands
                            'd' => {
                                state.set_pending_operator(Some('d'));
                                return None;
                            }
                            // Navigation
                            'h' => return Some(Command::CursorLeft),
                            'l' => return Some(Command::CursorRight),
                            'w' => return Some(Command::CursorWordForward),
                            'b' => return Some(Command::CursorWordBackward),
                            '0' => return Some(Command::CursorToLineStart),
                            '$' => return Some(Command::CursorToLineEnd),
                            '^' => return Some(Command::CursorToLineStart), // First non-whitespace (simplified)
                            // Deletion
                            'x' => return Some(Command::DeleteCharAfter),
                            'X' => return Some(Command::DeleteCharBefore),
                            // Other common commands
                            'o' => {
                                // Open new line below and enter insert mode
                                state.move_cursor_to_line_end();
                                state.insert_newline();
                                return Some(Command::SetVimMode(VimMode::Insert));
                            }
                            'O' => {
                                // Open new line above and enter insert mode
                                state.move_cursor_to_line_start();
                                state.insert_newline();
                                state.move_cursor_up();
                                return Some(Command::SetVimMode(VimMode::Insert));
                            }
                            _ => {}
                        }
                    }

                    // Visual mode selection + yank/delete
                    if vim_mode == VimMode::Visual && focused == FocusedComponent::Input {
                        match c {
                            'h' => return Some(Command::CursorLeft),
                            'l' => return Some(Command::CursorRight),
                            'w' => return Some(Command::CursorWordForward),
                            'b' => return Some(Command::CursorWordBackward),
                            '0' => return Some(Command::CursorToLineStart),
                            '$' => return Some(Command::CursorToLineEnd),
                            'y' => return Some(Command::YankSelection),
                            'd' => return Some(Command::DeleteSelection),
                            _ => {}
                        }
                    }

                    // Text editing only works in Insert mode
                    if vim_mode == VimMode::Insert && focused == FocusedComponent::Input {
                        // Detect '/' at start of input for slash commands
                        if c == '/' && state.input_text().is_empty() {
                            // Activate autocomplete for slash commands
                            state.activate_autocomplete("");
                        }
                        Some(Command::InsertChar(c))
                    } else {
                        None
                    }
                }
            }
        }
    }

    /// Convert keyboard event to command
    fn key_to_command(key: event::KeyEvent, state: &SharedState) -> Option<Command> {
        let force_quit_on_ctrl_c =
            std::env::var("TARK_FORCE_QUIT_ON_CTRL_C").is_ok_and(|v| v != "0");
        let vim_keys_enabled = state.is_vim_key_enabled();
        if key.code == KeyCode::Char('i')
            && key.modifiers == KeyModifiers::NONE
            && state.active_modal().is_none()
            && state.active_questionnaire().is_none()
            && state.focused_component() != FocusedComponent::Input
        {
            return Some(Command::FocusInput);
        }
        if !vim_keys_enabled
            && (key.modifiers == KeyModifiers::NONE || key.modifiers == KeyModifiers::SHIFT)
        {
            if let KeyCode::Char(c) = key.code {
                return Self::handle_text_char(c, state);
            }
        }

        match (key.code, key.modifiers) {
            // Application control
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                // If LLM is processing, send interrupt instead of quit
                if state.llm_processing() && !force_quit_on_ctrl_c {
                    Some(Command::Interrupt)
                } else {
                    Some(Command::Quit)
                }
            }
            (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(Command::Quit),
            // Ctrl+? opens help (allows ? to be typed normally in input)
            (KeyCode::Char('?'), KeyModifiers::CONTROL) => Some(Command::ToggleHelp),
            (KeyCode::Char('v'), KeyModifiers::CONTROL) => Some(Command::PasteClipboard),

            // Focus management / Autocomplete
            (KeyCode::Tab, KeyModifiers::NONE) => {
                // Check if autocomplete is active for slash commands
                if state.autocomplete_active() {
                    // Autocomplete will be handled by controller
                    return Some(Command::AutocompleteSelect);
                }
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
            (KeyCode::Char('M'), KeyModifiers::CONTROL) => Some(Command::CycleBuildMode),

            // Trust level cycling (Ctrl+Y) - works in all modes
            (KeyCode::Char('y'), KeyModifiers::CONTROL) => Some(Command::CycleTrustLevel),

            // UI toggles
            (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Command::ToggleSidebar),
            (KeyCode::Char('t'), KeyModifiers::CONTROL) => Some(Command::ToggleThinking),
            (KeyCode::Char('r'), KeyModifiers::CONTROL) => Some(Command::ToggleThinkingTool),

            // Vim keybindings for messages panel and sidebar
            // These should only consume the key if actually used, otherwise fall through to insert
            // IMPORTANT: Questionnaire takes priority over all these
            // IMPORTANT: Pickers with text filter need all chars including vim keys for typing
            (KeyCode::Char('j'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("j".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}j", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('j'));
                    }
                    return None;
                }
                // Questionnaire takes priority
                if let Some(q) = state.active_questionnaire() {
                    if q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText {
                        // For FreeText questions: only insert char if in edit mode
                        if q.is_editing_free_text {
                            state.questionnaire_insert_char('j');
                        }
                        // If not editing, do nothing (no navigation for FreeText)
                        return None;
                    }
                    return Some(Command::QuestionDown);
                }
                // Check if approval modal is active - j/k for navigation
                if state.active_modal() == Some(ModalType::Approval) {
                    state.approval_select_next();
                    return None;
                }
                // SessionSwitchConfirm modal navigation
                if state.active_modal() == Some(ModalType::SessionSwitchConfirm) {
                    return Some(Command::ModalDown);
                }
                // Trust/Tools/Plugin/Policy modals: j/k for navigation (selection only, no text input)
                if matches!(
                    state.active_modal(),
                    Some(ModalType::TrustLevel)
                        | Some(ModalType::Tools)
                        | Some(ModalType::Plugin)
                        | Some(ModalType::Policy)
                ) {
                    return Some(Command::ModalDown);
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::NextMessage)
                    }
                    (FocusedComponent::Panel, VimMode::Normal) => Some(Command::SidebarDown),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('j')),
                    _ => None,
                }
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("k".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}k", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('k'));
                    }
                    return None;
                }
                // Questionnaire takes priority
                if let Some(q) = state.active_questionnaire() {
                    if q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText {
                        // For FreeText questions: only insert char if in edit mode
                        if q.is_editing_free_text {
                            state.questionnaire_insert_char('k');
                        }
                        // If not editing, do nothing (no navigation for FreeText)
                        return None;
                    }
                    return Some(Command::QuestionUp);
                }
                // Check if approval modal is active - j/k for navigation
                if state.active_modal() == Some(ModalType::Approval) {
                    state.approval_select_prev();
                    return None;
                }
                // SessionSwitchConfirm modal navigation
                if state.active_modal() == Some(ModalType::SessionSwitchConfirm) {
                    return Some(Command::ModalUp);
                }
                // Trust/Tools/Plugin/Policy modals: j/k for navigation (selection only, no text input)
                if matches!(
                    state.active_modal(),
                    Some(ModalType::TrustLevel)
                        | Some(ModalType::Tools)
                        | Some(ModalType::Plugin)
                        | Some(ModalType::Policy)
                ) {
                    return Some(Command::ModalUp);
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::PrevMessage)
                    }
                    (FocusedComponent::Panel, VimMode::Normal) => Some(Command::SidebarUp),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('k')),
                    _ => None,
                }
            }
            (KeyCode::Char('l'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("l".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}l", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('l'));
                    }
                    return None;
                }
                // Questionnaire takes priority
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('l');
                    }
                    return None;
                }
                let is_tool_header = state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                    && state.focused_message_role() == Some(UiMessageRole::Tool)
                    && !state.is_in_group();
                match (state.focused_component(), state.vim_mode()) {
                    _ if is_tool_header => Some(Command::EnterGroup),
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorRight)
                    }
                    (FocusedComponent::Panel, VimMode::Normal) => Some(Command::SidebarEnter),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('l')),
                    _ => None,
                }
            }
            (KeyCode::Char('h'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("h".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}h", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('h'));
                    }
                    return None;
                }
                // Questionnaire takes priority
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('h');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorLeft)
                    }
                    (FocusedComponent::Panel, VimMode::Normal) => Some(Command::SidebarExit),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('h')),
                    _ => None,
                }
            }
            (KeyCode::Char('y'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("y".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}y", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('y'));
                    }
                    return None;
                }
                // Questionnaire takes priority - block y from going to input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('y');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal) => Some(Command::YankMessage),
                    (FocusedComponent::Messages, VimMode::Visual) => Some(Command::YankSelection),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('y')),
                    _ => None,
                }
            }
            (KeyCode::Char('v'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("v".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}v", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('v'));
                    }
                    return None;
                }
                // Questionnaire takes priority - block v from going to input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('v');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal) => {
                        Some(Command::SetVimMode(VimMode::Visual))
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('v')),
                    _ => None,
                }
            }
            (KeyCode::Char('-'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("-".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}-", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('-'));
                    }
                    return None;
                }
                if let Some(q) = state.active_questionnaire() {
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('-');
                    }
                    return None;
                }
                if state.focused_component() == FocusedComponent::Messages && state.is_in_group() {
                    return Some(Command::ExitGroup);
                }
                if state.focused_component() == FocusedComponent::Input
                    && state.vim_mode() == VimMode::Insert
                {
                    return Some(Command::InsertChar('-'));
                }
                None
            }
            (KeyCode::Char('w'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("w".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}w", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('w'));
                    }
                    return None;
                }
                if let Some(q) = state.active_questionnaire() {
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('w');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorWordForward)
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('w')),
                    _ => None,
                }
            }
            (KeyCode::Char('b'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("b".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}b", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('b'));
                    }
                    return None;
                }
                if let Some(q) = state.active_questionnaire() {
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('b');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorWordBackward)
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('b')),
                    _ => None,
                }
            }
            (KeyCode::Char('0'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("0".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}0", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('0'));
                    }
                    return None;
                }
                if let Some(q) = state.active_questionnaire() {
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('0');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorToLineStart)
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('0')),
                    _ => None,
                }
            }
            (KeyCode::Char('$'), KeyModifiers::SHIFT)
            | (KeyCode::Char('$'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("$".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}$", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('$'));
                    }
                    return None;
                }
                if let Some(q) = state.active_questionnaire() {
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('$');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal | VimMode::Visual) => {
                        Some(Command::CursorToLineEnd)
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('$')),
                    _ => None,
                }
            }
            // Task queue management: 'e' to edit selected task
            (KeyCode::Char('e'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pass through for pickers with text filter
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("e".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}e", current_filter));
                    return None;
                }
                // Questionnaire free text input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('e');
                    }
                    return None;
                }
                // Panel focused + Tasks panel + item selected -> edit task
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        // Only allow editing queued tasks (skip active tasks in the count)
                        // Active tasks are at the beginning, so we need to offset
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        // Queue index = item_idx - active_count - completed_count
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            return Some(Command::EditQueuedTask(queue_idx));
                        }
                    }
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('e')),
                    _ => None,
                }
            }
            // Task queue management: 'd' or 'x' to delete selected task
            (KeyCode::Char('d'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Policy modal: delete selected pattern
                if state.active_modal() == Some(ModalType::Policy) {
                    return Some(Command::DeletePolicyPattern);
                }
                // Pass through for pickers with text filter
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("d".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}d", current_filter));
                    return None;
                }
                // Questionnaire free text input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('d');
                    }
                    return None;
                }
                // Panel focused + Tasks panel + queued item selected -> delete task
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            return Some(Command::DeleteQueuedTask(queue_idx));
                        }
                    }
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Input, VimMode::Normal) => Some(Command::DeleteLine),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('d')),
                    _ => None,
                }
            }
            // Task queue management: 'x' also deletes (alternative to 'd')
            (KeyCode::Char('x'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pass through for pickers with text filter
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("x".to_string()));
                }
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}x", current_filter));
                    return None;
                }
                // Questionnaire free text input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('x');
                    }
                    return None;
                }
                // Panel focused + Tasks panel + queued item selected -> delete task
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            return Some(Command::DeleteQueuedTask(queue_idx));
                        }
                    }
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Input, VimMode::Normal) => Some(Command::DeleteCharAfter),
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('x')),
                    _ => None,
                }
            }
            // Task queue management: Shift+K to move task up
            (KeyCode::Char('K'), KeyModifiers::SHIFT) => {
                // Panel focused + Tasks panel + queued item selected -> move task up
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            if queue_idx > 0 {
                                return Some(Command::MoveTaskUp(queue_idx));
                            }
                        }
                    }
                }
                None
            }
            // Task queue management: Shift+J to move task down
            (KeyCode::Char('J'), KeyModifiers::SHIFT) => {
                // Panel focused + Tasks panel + queued item selected -> move task down
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        let queued_count = state.queued_message_count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            if queue_idx < queued_count.saturating_sub(1) {
                                return Some(Command::MoveTaskDown(queue_idx));
                            }
                        }
                    }
                }
                None
            }
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("g".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}g", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('g'));
                    }
                    return None;
                }
                // Questionnaire takes priority - block g from going to input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('g');
                    }
                    return None;
                }
                match (state.focused_component(), state.vim_mode()) {
                    (FocusedComponent::Messages, VimMode::Normal) => {
                        // Scroll to top
                        state.set_messages_scroll_offset(0);
                        None
                    }
                    (FocusedComponent::Input, VimMode::Insert) => Some(Command::InsertChar('g')),
                    _ => None,
                }
            }
            (KeyCode::Char('G'), KeyModifiers::SHIFT) => {
                use crate::ui_backend::VimMode;
                // Pickers with text filter: pass character through for typing
                // Note: FilePicker is handled separately as it has its own filter mechanism
                if matches!(
                    state.active_modal(),
                    Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                ) {
                    return Some(Command::ModalFilter("G".to_string()));
                }
                // FilePicker: update filter directly (it uses a different mechanism)
                if state.active_modal() == Some(ModalType::FilePicker) {
                    let current_filter = state.file_picker_filter();
                    state.set_file_picker_filter(format!("{}G", current_filter));
                    if state.focused_component() == FocusedComponent::Input {
                        return Some(Command::InsertChar('G'));
                    }
                    return None;
                }
                // Questionnaire takes priority - block G from going to input
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText: only insert if in edit mode
                    // For Other: only insert if editing other
                    if (q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        state.questionnaire_insert_char('G');
                    }
                    return None;
                }
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    // Scroll to bottom
                    state.scroll_to_bottom();
                    None
                } else {
                    None
                }
            }
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => {
                use crate::ui_backend::VimMode;
                // Ctrl+U is only for vim navigation in messages
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    // Page up (half page)
                    let current = state.messages_scroll_offset();
                    let total_lines = state.messages_total_lines();
                    let viewport_height = state.messages_viewport_height();
                    let max_offset = total_lines.saturating_sub(viewport_height);
                    let normalized = if current == usize::MAX {
                        max_offset
                    } else {
                        current
                    };
                    state.set_messages_scroll_offset(normalized.saturating_sub(10));
                }
                None
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                use crate::ui_backend::VimMode;
                // Ctrl+D is only for vim navigation in messages
                if state.focused_component() == FocusedComponent::Messages
                    && state.vim_mode() == VimMode::Normal
                {
                    // Page down (half page)
                    let current = state.messages_scroll_offset();
                    let total_lines = state.messages_total_lines();
                    let viewport_height = state.messages_viewport_height();
                    let max_offset = total_lines.saturating_sub(viewport_height);
                    let normalized = if current == usize::MAX {
                        max_offset
                    } else {
                        current
                    };
                    state.set_messages_scroll_offset(normalized.saturating_add(10).min(max_offset));
                }
                None
            }

            // Escape to close modal or switch to Normal mode
            (KeyCode::Esc, _) => {
                // Check if in questionnaire edit mode first (FreeText or "Other")
                if let Some(q) = state.active_questionnaire() {
                    if q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText
                        && q.is_editing_free_text
                    {
                        // Exit FreeText edit mode but keep the questionnaire open
                        return Some(Command::QuestionStopEdit);
                    }
                    if q.is_editing_other_text {
                        // Exit "Other" edit mode but keep the questionnaire open
                        return Some(Command::QuestionStopEdit);
                    }
                    // Not in edit mode - cancel the questionnaire
                    return Some(Command::QuestionCancel);
                }
                if let Some(modal) = state.active_modal() {
                    match modal {
                        ModalType::TaskEdit => Some(Command::CancelTaskEdit),
                        ModalType::TaskDeleteConfirm => Some(Command::CancelDeleteTask),
                        _ => Some(Command::CloseModal),
                    }
                } else {
                    // Check if navigating within a tool group
                    if matches!(state.focused_component(), FocusedComponent::Messages)
                        && state.is_in_group()
                    {
                        return Some(Command::ExitGroup);
                    }

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
                if let Some(q) = state.active_questionnaire() {
                    // For FreeText questions: Enter starts edit mode, or submits if already editing
                    if q.question_type == crate::ui_backend::questionnaire::QuestionType::FreeText {
                        if q.is_editing_free_text {
                            Some(Command::QuestionSubmit)
                        } else {
                            Some(Command::QuestionStartEdit)
                        }
                    } else if q.is_focused_on_other() && q.other_selected {
                        // "Other" is selected - check if editing
                        if q.is_editing_other_text {
                            Some(Command::QuestionSubmit)
                        } else {
                            Some(Command::QuestionStartEdit)
                        }
                    } else {
                        Some(Command::QuestionSubmit)
                    }
                } else if let Some(modal) = state.active_modal() {
                    match modal {
                        ModalType::TrustLevel => {
                            let selected = state.trust_level_selected();
                            let level = crate::tools::TrustLevel::from_index(selected);
                            Some(Command::SetTrustLevel(level))
                        }
                        ModalType::FilePicker => Some(Command::FilePickerSelect),
                        ModalType::Approval => {
                            // Execute the selected action from unified list
                            use crate::ui_backend::approval::ApprovalItem;
                            if let Some(approval) = state.pending_approval() {
                                match approval.get_selected_item() {
                                    ApprovalItem::RunOnce => Some(Command::ApproveOperation),
                                    ApprovalItem::AlwaysAllow => Some(Command::ApproveAlways),
                                    ApprovalItem::Pattern(_) => Some(Command::ApproveSession),
                                    ApprovalItem::Skip => Some(Command::DenyOperation),
                                }
                            } else {
                                Some(Command::ApproveOperation)
                            }
                        }
                        ModalType::TaskEdit => Some(Command::ConfirmTaskEdit),
                        ModalType::TaskDeleteConfirm => Some(Command::ConfirmDeleteTask),
                        _ => Some(Command::ConfirmModal),
                    }
                } else if matches!(state.focused_component(), FocusedComponent::Input) {
                    if state.autocomplete_active() {
                        Some(Command::AutocompleteConfirm)
                    } else {
                        let text = state.input_text();
                        Some(Command::SendMessage(text))
                    }
                } else if matches!(state.focused_component(), FocusedComponent::Messages) {
                    Some(Command::ToggleMessageCollapse)
                } else if matches!(state.focused_component(), FocusedComponent::Panel) {
                    Some(Command::SidebarSelect)
                } else {
                    None
                }
            }

            // Text editing (only in input focus)
            (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
                Self::handle_text_char(c, state)
            }

            // Backspace
            (KeyCode::Backspace, _) => {
                use crate::ui_backend::VimMode;

                if let Some(q) = state.active_questionnaire() {
                    use crate::ui_backend::questionnaire::QuestionType;

                    // For FreeText: only backspace if in edit mode
                    // For Other: only backspace if editing other
                    if (q.question_type == QuestionType::FreeText && q.is_editing_free_text)
                        || q.is_editing_other()
                    {
                        // Backspace in free text or "Other" input
                        state.questionnaire_backspace();
                        return None;
                    }
                    // Block backspace when not editing (for FreeText) or for choice questions
                    return None;
                }

                match state.active_modal() {
                    Some(ModalType::SessionPicker) if key.modifiers == KeyModifiers::SHIFT => {
                        Some(Command::DeleteSessionSelected)
                    }
                    Some(ModalType::FilePicker) => {
                        // FilePicker is an overlay - pass through to input
                        let current_filter = state.file_picker_filter();
                        if !current_filter.is_empty() {
                            let mut filter = current_filter;
                            filter.pop();
                            state.set_file_picker_filter(filter);
                        }

                        // Also delete from input if focused on input
                        if state.focused_component() == FocusedComponent::Input {
                            Some(Command::DeleteCharBefore)
                        } else {
                            None
                        }
                    }
                    Some(ModalType::ThemePicker)
                    | Some(ModalType::ProviderPicker)
                    | Some(ModalType::ModelPicker)
                    | Some(ModalType::SessionPicker) => Some(Command::ModalFilter(String::new())), // Signal to pop
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
            // Delete key
            (KeyCode::Delete, _) => {
                if state.active_modal() == Some(ModalType::SessionPicker) {
                    return Some(Command::DeleteSessionSelected);
                }
                None
            }

            // Cursor movement
            (KeyCode::Left, KeyModifiers::NONE) => match state.focused_component() {
                FocusedComponent::Messages => Some(Command::PrevMessage),
                FocusedComponent::Panel => Some(Command::SidebarExit),
                _ => Some(Command::CursorLeft),
            },
            (KeyCode::Right, KeyModifiers::NONE) => match state.focused_component() {
                FocusedComponent::Messages => {
                    if state.focused_message_role() == Some(UiMessageRole::Tool)
                        && !state.is_in_group()
                    {
                        Some(Command::EnterGroup)
                    } else {
                        Some(Command::NextMessage)
                    }
                }
                FocusedComponent::Panel => Some(Command::SidebarEnter),
                _ => Some(Command::CursorRight),
            },
            (KeyCode::Home, _) => Some(Command::CursorToLineStart),
            (KeyCode::End, _) => Some(Command::CursorToLineEnd),
            (KeyCode::Left, KeyModifiers::CONTROL) => Some(Command::CursorWordBackward),
            (KeyCode::Right, KeyModifiers::CONTROL) => Some(Command::CursorWordForward),

            // Task queue management: Shift+Up to move task up (same as Shift+K)
            (KeyCode::Up, KeyModifiers::SHIFT) => {
                // Panel focused + Tasks panel + queued item selected -> move task up
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            if queue_idx > 0 {
                                return Some(Command::MoveTaskUp(queue_idx));
                            }
                        }
                    }
                }
                None
            }
            // Task queue management: Shift+Down to move task down (same as Shift+J)
            (KeyCode::Down, KeyModifiers::SHIFT) => {
                // Panel focused + Tasks panel + queued item selected -> move task down
                if state.focused_component() == FocusedComponent::Panel
                    && state.sidebar_selected_panel() == 2
                {
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        let tasks = state.tasks();
                        let active_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Active)
                            .count();
                        let completed_count = tasks
                            .iter()
                            .filter(|t| t.status == StateTaskStatus::Completed)
                            .count();
                        let queued_count = state.queued_message_count();
                        if item_idx >= active_count + completed_count {
                            let queue_idx = item_idx - active_count - completed_count;
                            if queue_idx < queued_count.saturating_sub(1) {
                                return Some(Command::MoveTaskDown(queue_idx));
                            }
                        }
                    }
                }
                None
            }

            // Arrow key navigation (context-dependent)
            (KeyCode::Up, _) => {
                if state.active_questionnaire().is_some() {
                    Some(Command::QuestionUp)
                } else {
                    if state.autocomplete_active()
                        && matches!(state.focused_component(), FocusedComponent::Input)
                    {
                        return Some(Command::AutocompleteUp);
                    }
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
                        Some(ModalType::Approval) => {
                            state.approval_select_prev();
                            None
                        }
                        Some(ModalType::FilePicker) => Some(Command::FilePickerUp),
                        Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                        | Some(ModalType::SessionSwitchConfirm)
                        | Some(ModalType::Policy) => Some(Command::ModalUp),
                        _ if matches!(state.focused_component(), FocusedComponent::Panel) => {
                            Some(Command::SidebarUp)
                        }
                        _ if matches!(state.focused_component(), FocusedComponent::Messages) => {
                            Some(Command::PrevMessage)
                        }
                        _ => Some(Command::HistoryPrevious),
                    }
                }
            }
            (KeyCode::Down, _) => {
                if state.active_questionnaire().is_some() {
                    Some(Command::QuestionDown)
                } else {
                    if state.autocomplete_active()
                        && matches!(state.focused_component(), FocusedComponent::Input)
                    {
                        return Some(Command::AutocompleteDown);
                    }
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
                        Some(ModalType::Approval) => {
                            state.approval_select_next();
                            None
                        }
                        Some(ModalType::FilePicker) => Some(Command::FilePickerDown),
                        Some(ModalType::ThemePicker)
                        | Some(ModalType::ProviderPicker)
                        | Some(ModalType::ModelPicker)
                        | Some(ModalType::SessionPicker)
                        | Some(ModalType::SessionSwitchConfirm)
                        | Some(ModalType::Policy) => Some(Command::ModalDown),
                        _ if matches!(state.focused_component(), FocusedComponent::Panel) => {
                            Some(Command::SidebarDown)
                        }
                        _ if matches!(state.focused_component(), FocusedComponent::Messages) => {
                            Some(Command::NextMessage)
                        }
                        _ => Some(Command::HistoryNext),
                    }
                }
            }

            _ => None,
        }
    }

    fn handle_paste(state: &SharedState, text: String) -> Option<Command> {
        // Questionnaire takes priority for paste
        if let Some(q) = state.active_questionnaire() {
            use crate::ui_backend::questionnaire::QuestionType;
            // For FreeText: only paste if in edit mode
            // For Other: only paste if editing other
            if (q.question_type == QuestionType::FreeText && q.is_editing_free_text)
                || q.is_editing_other()
            {
                // Insert pasted text into questionnaire
                for c in text.chars() {
                    state.questionnaire_insert_char(c);
                }
            }
            // Block paste from going to input
            return None;
        }
        // Paste should insert full text without submitting
        if matches!(state.focused_component(), FocusedComponent::Input) {
            if state.active_modal().is_none() {
                let line_count = text.split('\n').count();
                if line_count > 1 {
                    let placeholder = format!("[pasted {} lines]", line_count);
                    state.add_paste_block(placeholder.clone(), text);
                    return Some(Command::InsertText(placeholder));
                }
            }
            return Some(Command::InsertText(text));
        }
        None
    }

    fn build_message_widgets(messages: &[Message]) -> Vec<super::widgets::Message> {
        messages
            .iter()
            .map(|m| super::widgets::Message {
                role: match m.role {
                    UiMessageRole::User => super::widgets::MessageRole::User,
                    UiMessageRole::Assistant => super::widgets::MessageRole::Agent,
                    UiMessageRole::System => super::widgets::MessageRole::System,
                    UiMessageRole::Tool => super::widgets::MessageRole::Tool,
                    UiMessageRole::Thinking => super::widgets::MessageRole::Thinking,
                },
                content: m.content.clone(),
                remote: m.remote,
                provider: m.provider.clone(),
                model: m.model.clone(),
                collapsed: m.collapsed,
                timestamp: m.timestamp.clone(),
                question: None,
                tool_args: m.tool_args.clone(),
            })
            .collect()
    }

    fn messages_area_rect(&self, state: &SharedState) -> Option<Rect> {
        let size = self.terminal.size().ok()?;
        if size.width < 3 || size.height < 3 {
            return None;
        }

        let inner_x = 1u16;
        let inner_y = 1u16;
        let inner_width = size.width.saturating_sub(2);
        let inner_height = size.height.saturating_sub(2);

        if inner_width == 0 || inner_height == 0 {
            return None;
        }

        let sidebar_visible = state.sidebar_visible();
        let (main_x, main_width) = if sidebar_visible && inner_width > 80 {
            (inner_x, inner_width.saturating_sub(35))
        } else {
            (inner_x, inner_width)
        };

        let header_height = 2u16;
        let input_height = 5u16;
        let status_message_height = 1u16;
        let status_height = 1u16;

        let header_y = inner_y;
        let messages_y = header_y + header_height;
        let status_y = inner_y + inner_height - status_height;
        let input_y = status_y.saturating_sub(input_height);
        let status_message_y = input_y.saturating_sub(status_message_height);
        let messages_height = status_message_y.saturating_sub(messages_y);

        if messages_height == 0 || main_width == 0 {
            return None;
        }

        Some(Rect {
            x: main_x,
            y: messages_y,
            width: main_width,
            height: messages_height,
        })
    }

    fn message_line_target_at(
        &self,
        col: u16,
        row: u16,
        state: &SharedState,
    ) -> Option<super::widgets::MessageLineTarget> {
        let messages_rect = self.messages_area_rect(state)?;

        if col < messages_rect.x
            || col >= messages_rect.x + messages_rect.width
            || row < messages_rect.y
            || row >= messages_rect.y + messages_rect.height
        {
            return None;
        }

        if messages_rect.width < 3 || messages_rect.height < 3 {
            return None;
        }

        if col == messages_rect.x + messages_rect.width.saturating_sub(1) {
            return None;
        }

        let inner_y = messages_rect.y + 1;
        let inner_height = messages_rect.height.saturating_sub(2);
        if row < inner_y || row >= inner_y + inner_height {
            return None;
        }

        let relative_line = row.saturating_sub(inner_y);
        let messages = state.messages();
        let message_widgets = Self::build_message_widgets(&messages);
        let collapsed = state.collapsed_tool_groups();
        let message_area = MessageArea::new(&message_widgets, &self.theme)
            .focused(matches!(
                state.focused_component(),
                FocusedComponent::Messages
            ))
            .focused_index(state.focused_message())
            .focused_sub_index(state.focused_sub_index())
            .message_cursor(state.message_cursor())
            .message_selection(state.message_selection())
            .vim_mode(state.vim_mode())
            .diff_view_mode(state.diff_view_mode())
            .collapsed_tool_groups(&collapsed)
            .streaming_content(state.streaming_content())
            .streaming_thinking(state.streaming_thinking());
        let targets = message_area.line_targets(messages_rect);

        if targets.is_empty() {
            return None;
        }

        let total_lines = targets.len();
        let viewport_height = if state.messages_viewport_height() == 0 {
            inner_height as usize
        } else {
            state.messages_viewport_height()
        };
        let max_offset = total_lines.saturating_sub(viewport_height);
        let scroll_offset = state.messages_scroll_offset();
        let line_offset = if scroll_offset == usize::MAX {
            max_offset
        } else {
            scroll_offset.min(max_offset)
        };
        let line_index = line_offset.saturating_add(relative_line as usize);

        targets.get(line_index).copied()
    }

    fn handle_messages_click(&self, mouse: MouseEvent, state: &SharedState) -> Option<Command> {
        let target = self.message_line_target_at(mouse.column, mouse.row, state)?;
        state.set_focused_component(FocusedComponent::Messages);
        state.set_vim_mode(crate::ui_backend::VimMode::Normal);

        match target {
            super::widgets::MessageLineTarget::ToolGroupHeader { start_index } => {
                state.set_focused_message(start_index);
                state.exit_group();
                Some(Command::ToggleMessageCollapse)
            }
            super::widgets::MessageLineTarget::ToolHeader(message_index) => {
                let messages = state.messages();
                if let Some((group_start, _)) = tool_group_info(&messages, message_index) {
                    let offset = message_index.saturating_sub(group_start);
                    state.set_focused_message(group_start);
                    state.set_focused_sub_index(Some(offset));
                    if state.is_tool_group_collapsed(group_start) {
                        state.expand_tool_group(group_start);
                    }
                } else {
                    state.set_focused_message(message_index);
                    state.exit_group();
                }
                Some(Command::ToggleMessageCollapse)
            }
            super::widgets::MessageLineTarget::Message(message_index) => {
                let messages = state.messages();
                if let Some((group_start, _)) = tool_group_info(&messages, message_index) {
                    let offset = message_index.saturating_sub(group_start);
                    state.set_focused_message(group_start);
                    state.set_focused_sub_index(Some(offset));
                    if state.is_tool_group_collapsed(group_start) {
                        state.expand_tool_group(group_start);
                    }
                } else {
                    state.set_focused_message(message_index);
                    state.exit_group();
                }
                Some(Command::FocusMessages)
            }
            super::widgets::MessageLineTarget::None => None,
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
                if state.active_modal().is_some() {
                    Some(Command::ModalDown)
                } else {
                    match self.hit_test(mouse.column, mouse.row, state) {
                        ClickTarget::Sidebar => {
                            if let Some(idx) = self.get_clicked_sidebar_panel(mouse.row, state) {
                                state.set_sidebar_selected_panel(idx);
                            }
                            Some(Command::SidebarDown)
                        }
                        _ => Some(Command::ScrollDown),
                    }
                }
            }
            MouseEventKind::ScrollUp => {
                tracing::debug!("Scroll up event detected");
                if state.active_modal().is_some() {
                    Some(Command::ModalUp)
                } else {
                    match self.hit_test(mouse.column, mouse.row, state) {
                        ClickTarget::Sidebar => {
                            if let Some(idx) = self.get_clicked_sidebar_panel(mouse.row, state) {
                                state.set_sidebar_selected_panel(idx);
                            }
                            Some(Command::SidebarUp)
                        }
                        _ => Some(Command::ScrollUp),
                    }
                }
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                tracing::debug!("Left click event detected");
                // Check if clicking a queued task in the Tasks panel (for drag-to-reorder)
                let target = self.hit_test(mouse.column, mouse.row, state);
                if target == ClickTarget::Sidebar {
                    if let Some((task_idx, is_queued)) =
                        self.get_clicked_task_index(mouse.row, state)
                    {
                        if is_queued {
                            // Start drag operation for queued tasks
                            let tasks = state.tasks();
                            let active_count = tasks
                                .iter()
                                .filter(|t| t.status == StateTaskStatus::Active)
                                .count();
                            let completed_count = tasks
                                .iter()
                                .filter(|t| t.status == StateTaskStatus::Completed)
                                .count();
                            let queue_idx = task_idx - active_count - completed_count;
                            state.start_task_drag(queue_idx);
                            tracing::debug!("Started task drag for queue index {}", queue_idx);
                            // Also select the item in the sidebar
                            state.set_sidebar_selected_panel(2); // Tasks panel
                            state.set_sidebar_selected_item(Some(task_idx));
                            return Some(Command::FocusPanel);
                        }
                    }
                }
                if target == ClickTarget::Messages {
                    if let Some(cmd) = self.handle_messages_click(mouse, state) {
                        return Some(cmd);
                    }
                }
                // Handle clicks - delegate to hit testing
                self.handle_mouse_click(mouse.column, mouse.row, state)
            }
            MouseEventKind::Drag(crossterm::event::MouseButton::Left) => {
                tracing::debug!("Mouse drag event detected at row={}", mouse.row);
                // Handle scrollbar dragging for messages area or task drag-to-reorder
                self.handle_scrollbar_drag(mouse.column, mouse.row, state)
            }
            MouseEventKind::Up(crossterm::event::MouseButton::Left) => {
                tracing::debug!("Mouse up event detected");
                // Complete task drag operation if active
                if state.is_dragging_task() {
                    let moved = state.complete_task_drag();
                    if moved {
                        tracing::debug!("Task drag completed, task moved");
                        return Some(Command::RefreshSidebar);
                    } else {
                        tracing::debug!("Task drag cancelled (no movement)");
                    }
                }
                None
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
                    // Select the clicked panel
                    state.set_sidebar_selected_panel(idx);
                    state.set_sidebar_selected_item(None);

                    if idx == 5 {
                        // Theme panel clicked - open theme picker
                        Some(Command::ToggleThemePicker)
                    } else {
                        // Regular panel - toggle expansion
                        Some(Command::ToggleSidebarPanel(idx))
                    }
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
    fn get_clicked_sidebar_panel(&self, row: u16, state: &SharedState) -> Option<usize> {
        use ratatui::layout::{Constraint, Direction, Layout};

        let area = self.terminal.size().unwrap_or_default();
        let area = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: area.width,
            height: area.height,
        };
        let inner = ratatui::layout::Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        let sidebar_visible = state.sidebar_visible();
        let sidebar_rect = if sidebar_visible && inner.width > 80 {
            let horizontal_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Min(60),    // Main terminal area
                    Constraint::Length(35), // Sidebar panel
                ])
                .split(inner);
            Some(horizontal_chunks[1])
        } else {
            None
        }?;

        let _sidebar_height = sidebar_rect.height.saturating_sub(2);

        // Footer is the last 1-2 lines of the sidebar
        // Check if click is in footer area (theme icon)
        let footer_row = sidebar_rect.y + sidebar_rect.height.saturating_sub(2);
        if row >= footer_row {
            return Some(6); // Theme panel
        }

        let inner_y = sidebar_rect.y + 1;

        // Panel heights - use dynamic approach based on expansion state
        let header_h = 2u16; // VIM mode indicator
        let panels = state.sidebar_expanded_panels();

        // Calculate approximate panel positions
        // When collapsed: 1 line for header only
        // When expanded: varies by content, but we use reasonable estimates
        let session_h = if panels[0] { 5u16 } else { 1u16 };
        let context_h = if panels[1] { 6u16 } else { 1u16 };
        let tasks_h = if panels[2] { 4u16 } else { 1u16 };
        let todo_h = if panels[3] { 4u16 } else { 1u16 };
        let plugins_h = if panels[5] { 4u16 } else { 1u16 };

        if row < inner_y + header_h {
            return None; // Header area (VIM mode)
        }

        let mut cursor = inner_y + header_h;

        // Session panel (index 0)
        if row >= cursor && row < cursor + session_h {
            return Some(0);
        }
        cursor += session_h;

        // Context panel (index 1)
        if row >= cursor && row < cursor + context_h {
            return Some(1);
        }
        cursor += context_h;

        // Tasks panel (index 2)
        if row >= cursor && row < cursor + tasks_h {
            return Some(2);
        }
        cursor += tasks_h;

        // Todo panel (index 3)
        if row >= cursor && row < cursor + todo_h {
            return Some(3);
        }
        cursor += todo_h;

        // Plugin panel sits just above footer
        let plugin_start = footer_row.saturating_sub(plugins_h);
        if row >= plugin_start && row < footer_row {
            return Some(5);
        }

        // Git panel (index 4) - takes remaining space between panels and plugins
        if row >= cursor && row < plugin_start {
            return Some(4);
        }

        None
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

    /// Handle scrollbar dragging or task drag-to-reorder
    fn handle_scrollbar_drag(&self, col: u16, row: u16, state: &SharedState) -> Option<Command> {
        let size = self.terminal.size().unwrap_or_default();

        // First, check if we're dragging a task for reorder
        if state.is_dragging_task() {
            // Update drag target based on mouse position in sidebar
            if let Some((task_idx, _is_queued)) = self.get_clicked_task_index(row, state) {
                // Only allow dropping onto queued tasks (excluding the first/active one)
                let tasks = state.tasks();
                let active_count = tasks
                    .iter()
                    .filter(|t| t.status == StateTaskStatus::Active)
                    .count();
                let completed_count = tasks
                    .iter()
                    .filter(|t| t.status == StateTaskStatus::Completed)
                    .count();

                if task_idx >= active_count + completed_count {
                    let queue_idx = task_idx - active_count - completed_count;
                    state.update_drag_target(queue_idx);
                }
            }
            return None; // State updated, no command needed
        }

        // Check if dragging in the messages area scrollbar region
        // The scrollbar is at the right edge of the messages panel
        let header_height = 2u16;
        let input_height = 5u16;
        let status_message_height = 1u16;
        let status_height = 1u16;

        let messages_start_y = 1 + header_height;
        let messages_end_y = size.height - status_height - input_height - status_message_height - 1;
        let messages_height = messages_end_y.saturating_sub(messages_start_y);

        // Check if the drag is in the messages scrollbar area (right edge)
        let sidebar_visible = state.sidebar_visible();
        let main_width = if sidebar_visible && size.width > 80 {
            size.width.saturating_sub(37) // 35 for sidebar + 2 for borders
        } else {
            size.width.saturating_sub(2)
        };

        // Scrollbar is at the right edge of the main area
        if col >= main_width
            && col <= main_width + 1
            && row >= messages_start_y
            && row < messages_end_y
        {
            // Calculate scroll position based on mouse Y position
            let relative_y = row.saturating_sub(messages_start_y);
            let total_lines = state.messages_total_lines();
            let viewport_height = state.messages_viewport_height();
            let max_offset = total_lines.saturating_sub(viewport_height);

            if messages_height > 0 && total_lines > 0 {
                // Map Y position to scroll offset
                let scroll_ratio = relative_y as f32 / messages_height as f32;
                let new_offset = (scroll_ratio * max_offset as f32) as usize;
                state.set_messages_scroll_offset(new_offset.min(max_offset));
            }

            return None; // State is already updated, no command needed
        }

        None
    }

    /// Get the task index clicked within the Tasks panel
    /// Returns (task_list_index, is_queued_task)
    fn get_clicked_task_index(&self, row: u16, state: &SharedState) -> Option<(usize, bool)> {
        let size = self.terminal.size().unwrap_or_default();
        let _sidebar_height = size.height.saturating_sub(2);

        // Calculate Tasks panel start position (same logic as get_clicked_sidebar_panel)
        let inner_y = 1u16;
        let header_h = 2u16;
        let panels = state.sidebar_expanded_panels();

        let session_h = if panels[0] { 5u16 } else { 1u16 };
        let context_h = if panels[1] { 6u16 } else { 1u16 };

        // Tasks panel header starts at this position
        let tasks_start = inner_y + header_h + session_h + context_h;
        let tasks_header_h = 1u16; // "Tasks" header line

        // If click is on or before the header, not a task item
        if row <= tasks_start || !panels[2] {
            return None;
        }

        // Calculate which task item was clicked
        // Each task takes 2 lines: name line + status/label line
        let item_row = row.saturating_sub(tasks_start + tasks_header_h);
        let task_idx = (item_row / 2) as usize; // 2 lines per task

        let tasks = state.tasks();
        if task_idx < tasks.len() {
            // Determine if this is a queued task
            let active_count = tasks
                .iter()
                .filter(|t| t.status == StateTaskStatus::Active)
                .count();
            let completed_count = tasks
                .iter()
                .filter(|t| t.status == StateTaskStatus::Completed)
                .count();

            let is_queued = task_idx >= active_count + completed_count;
            return Some((task_idx, is_queued));
        }

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

        // Main area vertical layout: Header(2) | Messages(Min 5) | Status Strip(1) | Input(5) | Status(1)
        let header_height = 2u16;
        let input_height = 5u16;
        let status_message_height = 1u16;
        let status_height = 1u16;

        let header_y = inner_y;
        let messages_y = header_y + header_height;
        let status_y = inner_y + inner_height - status_height;
        let input_y = status_y.saturating_sub(input_height);
        let status_message_y = input_y.saturating_sub(status_message_height);

        // Determine which vertical section was clicked
        if col >= main_x && col < main_x + main_width {
            if row >= header_y && row < messages_y {
                return ClickTarget::Header;
            } else if row >= messages_y && row < status_message_y {
                return ClickTarget::Messages;
            } else if row >= status_message_y && row < status_y {
                return ClickTarget::Input;
            } else if row >= status_y && row < status_y + status_height {
                return ClickTarget::StatusBar;
            }
        }

        ClickTarget::Outside
    }
}

fn tool_group_info(messages: &[Message], idx: usize) -> Option<(usize, usize)> {
    let msg = messages.get(idx)?;
    if msg.role != UiMessageRole::Tool {
        return None;
    }
    let risk_group = super::widgets::parse_tool_risk_group(&msg.content);
    let mut start = idx;
    while start > 0 {
        let prev = &messages[start - 1];
        if prev.role == UiMessageRole::Tool
            && super::widgets::parse_tool_risk_group(&prev.content) == risk_group
        {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = start;
    while end < messages.len() {
        let current = &messages[end];
        if current.role == UiMessageRole::Tool
            && super::widgets::parse_tool_risk_group(&current.content) == risk_group
        {
            end += 1;
        } else {
            break;
        }
    }
    let size = end.saturating_sub(start);
    if size >= 2 {
        Some((start, size))
    } else {
        None
    }
}

impl<B: Backend> UiRenderer for TuiRenderer<B> {
    fn render(&mut self, state: &SharedState) -> Result<()> {
        use ratatui::layout::{Constraint, Direction, Layout};

        let render_start = std::time::Instant::now();

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

            // Vertical layout: Header | Messages | Status Strip | Input | Status
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(2), // Header
                    Constraint::Min(5),    // Message area
                    Constraint::Length(1), // Status message strip
                    Constraint::Length(5), // Input area
                    Constraint::Length(1), // Status bar
                ])
                .split(main_area);

            // Render header
            let config = super::config::AppConfig::default();
            let remote_indicator = if std::env::var("TARK_REMOTE_ENABLED")
                .map(|v| v == "1")
                .unwrap_or(false)
            {
                let plugin_id = std::env::var("TARK_REMOTE_PLUGIN").ok();
                let widgets = state.plugin_widgets();
                let status = plugin_id
                    .as_ref()
                    .and_then(|id| {
                        widgets
                            .iter()
                            .find(|w| &w.plugin_id == id)
                            .and_then(|w| w.status.clone())
                    })
                    .unwrap_or_else(|| "disconnected".to_string());
                let label = if let Some(id) = plugin_id {
                    format!("Remote: {} ({})", status, id)
                } else {
                    format!("Remote: {}", status)
                };
                let status_color = if status.eq_ignore_ascii_case("connected") {
                    theme.green
                } else if status.eq_ignore_ascii_case("disconnected") {
                    theme.red
                } else {
                    theme.text_muted
                };
                Some(crate::tui_new::widgets::header::RemoteIndicator {
                    label,
                    status_color,
                })
            } else {
                None
            };
            let header = Header::new(&config, theme, remote_indicator);
            frame.render_widget(header, chunks[0]);

            // Render message area
            let message_widgets = Self::build_message_widgets(&messages);

            let streaming_content = state.streaming_content();
            // Always show streaming_thinking if present - it contains both:
            // 1. Native thinking from LLMs (when thinking_enabled)
            // 2. Promoted intermediate turn content (always)
            let streaming_thinking = state.streaming_thinking();

            // Calculate bubble width for markdown rendering
            let bubble_content_width = chunks[1].width.saturating_sub(8) as usize;

            // Use incremental markdown rendering for streaming content
            // This avoids O(n) re-parsing of the entire content on every frame
            let streaming_lines = if let Some(ref content) = streaming_content {
                if !content.is_empty() {
                    let lines = self.streaming_markdown_cache.render_incremental(
                        content,
                        theme,
                        bubble_content_width.saturating_sub(2),
                    );
                    Some(lines)
                } else {
                    None
                }
            } else {
                // No streaming content - clear cache
                self.streaming_markdown_cache.clear();
                None
            };

            // Similarly for thinking content
            let thinking_lines = if let Some(ref thinking) = streaming_thinking {
                if !thinking.is_empty() {
                    let lines = self.streaming_thinking_cache.render_incremental(
                        thinking,
                        theme,
                        bubble_content_width.saturating_sub(2),
                    );
                    Some(lines)
                } else {
                    None
                }
            } else {
                self.streaming_thinking_cache.clear();
                None
            };

            let collapsed_groups = state.collapsed_tool_groups();
            let message_area = MessageArea::new(&message_widgets, theme)
                .agent_name(&config.agent_name_short)
                .focused(matches!(focused_component, FocusedComponent::Messages))
                .focused_index(state.focused_message())
                .focused_sub_index(state.focused_sub_index())
                .message_cursor(state.message_cursor())
                .message_selection(state.message_selection())
                .scroll(state.messages_scroll_offset())
                .vim_mode(state.vim_mode())
                .diff_view_mode(state.diff_view_mode())
                .streaming_content(streaming_content)
                .streaming_thinking(streaming_thinking)
                .streaming_lines(streaming_lines)
                .thinking_lines(thinking_lines)
                .thinking_max_lines(config.thinking_max_lines)
                .processing(state.llm_processing())
                .collapsed_tool_groups(&collapsed_groups);
            let (total_lines, viewport_height) = message_area.metrics(chunks[1]);
            state.set_messages_metrics(total_lines, viewport_height);
            frame.render_widget(message_area, chunks[1]);

            // Render status message strip
            let (flash_state, message) = build_status_message(state);
            let mut status_strip = FlashBar::new(theme)
                .kind(flash_state)
                .animation_frame(state.flash_bar_animation_frame());
            if let Some(message) = message.as_deref() {
                status_strip = status_strip.message(message);
            }
            frame.render_widget(status_strip, chunks[2]);

            // Render input area
            let context_file_paths: Vec<String> =
                context_files.iter().map(|f| f.path.clone()).collect();
            let input = InputWidget::new(&input_text, input_cursor, theme)
                .focused(matches!(focused_component, FocusedComponent::Input))
                .context_files(context_file_paths)
                .selection(state.input_selection())
                .paste_placeholders(state.paste_placeholders());
            frame.render_widget(input, chunks[3]);

            // Render command autocomplete dropdown if active
            if state.autocomplete_active() {
                let mut ac_state = super::widgets::AutocompleteState::new();
                let filter = state.autocomplete_filter();
                ac_state.activate(&filter);
                ac_state.selected = state.autocomplete_selected();
                ac_state.scroll_offset = state.autocomplete_scroll_offset();
                ac_state.matches = super::widgets::SlashCommand::find_matches(&filter);

                let autocomplete = super::widgets::CommandAutocomplete::new(theme, &ac_state);
                frame.render_widget(autocomplete, chunks[3]);
            }

            // Render status bar
            let queued_count = state.queued_message_count();
            // LLM is considered connected if we have a provider configured
            let llm_connected = current_provider.is_some() && state.llm_connected();
            let thinking_tool_enabled = state.thinking_tool_enabled();

            let mut status = StatusBar::new(theme)
                .agent_mode(agent_mode)
                .build_mode(build_mode)
                .thinking(thinking_enabled)
                .thinking_tool(thinking_tool_enabled)
                .queue(queued_count) // Show actual queue count
                .processing(llm_processing)
                .connected(llm_connected);

            // Set provider and model if available
            if let Some(ref provider) = current_provider {
                status = status.provider(provider);
            }
            let model_name_override = resolve_model_display_name(state, &current_model);
            if let Some(name) = model_name_override.as_deref() {
                status = status.model(name);
            }

            frame.render_widget(status, chunks[4]);

            // Render sidebar if visible
            if let Some(sidebar_rect) = sidebar_area {
                let is_sidebar_focused = focused_component == FocusedComponent::Panel;
                let current_theme_name = theme_preset.display_name().to_string();

                let session_costs = state.session_cost_by_model();
                let session_total_cost = state.session_cost_total();
                let session_tokens = state.session_tokens_by_model();
                let session_total_tokens = state.session_tokens_total();

                // Get real session info from state
                let session_info = state
                    .session()
                    .map(|s| SessionInfo {
                        name: s.session_name.clone(),
                        is_remote: s.session_id.starts_with("channel_"),
                        total_cost: session_total_cost.max(s.total_cost),
                        model_count: session_costs.len().max(s.model_count),
                        model_costs: session_costs.clone(),
                        total_tokens: session_total_tokens,
                        model_tokens: session_tokens.clone(),
                    })
                    .unwrap_or_else(|| {
                        // Derive session name from first user message as fallback
                        let session_name = messages
                            .iter()
                            .find(|m| matches!(m.role, UiMessageRole::User))
                            .map(|m| {
                                m.content
                                    .split_whitespace()
                                    .take(4)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            })
                            .unwrap_or_else(|| "New Session".to_string());

                        SessionInfo {
                            name: session_name,
                            is_remote: messages.iter().any(|m| m.remote),
                            total_cost: session_total_cost,
                            model_count: session_costs.len(),
                            model_costs: session_costs.clone(),
                            total_tokens: session_total_tokens,
                            model_tokens: session_tokens.clone(),
                        }
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

                // Get todos from shared tracker (non-blocking)
                let todo_items = {
                    let tracker = state.todo_tracker();
                    // Use try_lock to avoid blocking the render loop
                    let items = match tracker.try_lock() {
                        Ok(todos) => todos.items().to_vec(),
                        Err(_) => Vec::new(), // Skip todos this frame if lock is held
                    };
                    items
                };

                let mut sidebar = Sidebar::new(theme)
                    .visible(true)
                    .theme_name(current_theme_name)
                    .theme_preset(state.theme())
                    .focused(is_sidebar_focused)
                    .selected_panel(state.sidebar_selected_panel())
                    .vim_mode(state.vim_mode())
                    .session_info(session_info)
                    .scroll_offset(state.sidebar_scroll_offset())
                    .panel_scrolls(state.sidebar_panel_scrolls())
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
                    .context_breakdown(state.context_breakdown())
                    .tasks(tasks_widget)
                    .todos(todo_items)
                    .git_changes(git_changes_widget)
                    .plugin_widgets(state.plugin_widgets())
                    .git_branch(crate::tui_new::git_info::get_current_branch(
                        &self.working_dir,
                    ));

                sidebar.expanded_panels = state.sidebar_expanded_panels();
                sidebar.selected_item = state.sidebar_selected_item();

                // Set drag state for visual feedback
                sidebar =
                    sidebar.drag_state(state.dragging_task_index(), state.drag_target_index());

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
                                    p.source == crate::ui_backend::ProviderSource::Plugin,
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
                    ModalType::SessionPicker => {
                        let sessions = state.available_sessions();
                        let sessions_data: Vec<_> = sessions
                            .iter()
                            .map(|s| {
                                let name = if s.name.is_empty() {
                                    format!("Session {}", s.created_at.format("%Y-%m-%d %H:%M"))
                                } else {
                                    s.name.clone()
                                };
                                let meta = format!(
                                    "{} msgs · {}",
                                    s.message_count,
                                    s.updated_at.format("%Y-%m-%d %H:%M")
                                );
                                (name, s.id.clone(), meta, s.is_current)
                            })
                            .collect();
                        let picker = SessionPickerModal::new(theme)
                            .sessions(sessions_data)
                            .selected(state.session_picker_selected())
                            .filter(state.session_picker_filter());
                        frame.render_widget(picker, area);
                    }
                    ModalType::FilePicker => {
                        let files = state.file_picker_files();
                        let filter = state.file_picker_filter();
                        let selected = state.file_picker_selected();
                        let selected_paths: Vec<String> = state
                            .attachment_tokens()
                            .into_iter()
                            .map(|entry| entry.token.trim_start_matches('@').to_string())
                            .collect();
                        let picker = FilePickerModal::new(theme)
                            .files(&files)
                            .filter(&filter)
                            .selected(selected)
                            .selected_paths(&selected_paths)
                            .current_dir("./");
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
                        use crate::tools::ToolCategory;
                        let tools = state.tools_for_modal();
                        let is_external = tools
                            .first()
                            .map(|t| t.category == ToolCategory::External)
                            .unwrap_or(false);
                        let modal = ToolsModal::new(theme, agent_mode)
                            .tools(tools)
                            .selected(state.tools_selected())
                            .scroll_offset(state.tools_scroll_offset())
                            .external(is_external);
                        frame.render_widget(modal, area);
                    }
                    ModalType::Plugin => {
                        let modal = PluginModal::new(theme);
                        frame.render_widget(modal, area);
                    }
                    ModalType::Policy => {
                        if let Some(ref modal) = state.policy_modal() {
                            use crate::tui_new::modals::policy_modal::PolicyModalWidget;
                            let widget = PolicyModalWidget::new(modal, theme);
                            frame.render_widget(widget, area);
                        }
                    }
                    ModalType::DeviceFlow => {
                        if let Some(session) = state.device_flow_session() {
                            let modal = DeviceFlowModal::new(theme, &session);
                            frame.render_widget(modal, area);
                        }
                    }
                    ModalType::SessionSwitchConfirm => {
                        let selected = state.session_switch_confirm_selected();
                        let modal = SessionSwitchConfirmModal::new(theme).selected(selected);
                        frame.render_widget(modal, area);
                    }
                    ModalType::TaskEdit => {
                        let content = state.editing_task_content();
                        let cursor_pos = content.len(); // Cursor at end
                        let modal = TaskEditModal::new(theme)
                            .content(&content)
                            .cursor_position(cursor_pos);
                        frame.render_widget(modal, area);
                    }
                    ModalType::TaskDeleteConfirm => {
                        // Get preview of task to delete
                        let preview = if let Some(idx) = state.pending_delete_task_index() {
                            let messages = state.queued_messages();
                            messages.get(idx).cloned().unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let modal = TaskDeleteConfirmModal::new(theme)
                            .task_preview(&preview)
                            .selected(0); // Default to Cancel
                        frame.render_widget(modal, area);
                    }
                }
            }

            // Render questionnaire if active (on top of modals)
            if let Some(q) = state.active_questionnaire() {
                use super::widgets::question::ThemedQuestion;
                use super::widgets::QuestionType as WidgetQuestionType;
                use crate::ui_backend::questionnaire::QuestionType as StateQuestionType;

                // Convert QuestionnaireState to QuestionWidget
                let widget_question_type = match q.question_type {
                    StateQuestionType::SingleChoice => WidgetQuestionType::SingleChoice,
                    StateQuestionType::MultipleChoice => WidgetQuestionType::MultipleChoice,
                    StateQuestionType::FreeText => WidgetQuestionType::FreeText,
                };

                // Filter out any "Other" options from LLM - we provide our own with text input
                let options: Vec<QuestionOption> = q
                    .options
                    .iter()
                    .filter(|opt| {
                        let text_lower = opt.text.to_lowercase();
                        !text_lower.starts_with("other")
                            && !text_lower.contains("other:")
                            && !text_lower.contains("other...")
                    })
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
                    focused_index: q.focused_index,
                    free_text_answer: q.free_text_answer.clone(),
                    answered: q.answered,
                    allow_other: q.allow_other,
                    other_text: q.other_text.clone(),
                    other_selected: q.other_selected,
                    current_index: q.current_question_index,
                    total_questions: q.total_questions,
                    title: q.title.clone(),
                    is_editing_free_text: q.is_editing_free_text,
                    is_editing_other_text: q.is_editing_other_text,
                };

                // ThemedQuestion handles its own centering, pass full area
                let themed_question = ThemedQuestion::new(&question_widget, theme);
                frame.render_widget(themed_question, area);
            }
        })?;

        // Log render time with correlation_id if available
        let render_time = render_start.elapsed();
        if let Some(correlation_id) = state.current_correlation_id() {
            if let Some(logger) = crate::debug_logger() {
                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                    correlation_id,
                    crate::LogCategory::Tui,
                    "render_frame",
                )
                .with_data(serde_json::json!({
                    "frame_time_ms": render_time.as_millis()
                }));
                logger.log(entry);
            }
        }

        Ok(())
    }

    fn poll_input(&mut self, state: &SharedState) -> Result<Option<Command>> {
        // Use shorter poll timeout during streaming for faster UI updates (100+ fps)
        // Use longer timeout when idle to reduce CPU usage
        let poll_timeout = if state.llm_processing() {
            Duration::from_millis(8) // ~120fps during streaming
        } else {
            Duration::from_millis(50) // Normal idle polling
        };

        if event::poll(poll_timeout)? {
            match event::read()? {
                Event::Key(key) => {
                    // Special handling for ESC key
                    if key.code == KeyCode::Esc {
                        let now = Instant::now();

                        // Check for double-ESC (within 500ms)
                        if let Some(last_esc) = self.last_esc_time {
                            if now.duration_since(last_esc) < Duration::from_millis(500) {
                                // Double-ESC detected - cancel agent if working
                                self.last_esc_time = None;
                                if state.llm_processing() {
                                    return Ok(Some(Command::CancelAgent));
                                }
                            }
                        }

                        self.last_esc_time = Some(now);

                        // First ESC: handle questionnaire first, then modal, then normal ESC
                        if state.active_questionnaire().is_some() {
                            return Ok(Some(Command::QuestionCancel));
                        }
                    }

                    return Ok(Self::key_to_command(key, state));
                }
                Event::Mouse(mouse) => {
                    // Check for click outside questionnaire to dismiss it
                    if let MouseEventKind::Down(_) = mouse.kind {
                        if state.active_questionnaire().is_some() {
                            // Check if click is outside the question modal
                            // The modal is centered, so approximate the bounds
                            let (width, height) = self.get_size();
                            let modal_width = width.min(65);
                            let modal_height = height.min(15); // Approximate
                            let modal_x = (width.saturating_sub(modal_width)) / 2;
                            let modal_y = (height.saturating_sub(modal_height)) / 2;

                            let click_x = mouse.column;
                            let click_y = mouse.row;

                            // If click is outside modal bounds, cancel the questionnaire
                            if click_x < modal_x
                                || click_x >= modal_x + modal_width
                                || click_y < modal_y
                                || click_y >= modal_y + modal_height
                            {
                                return Ok(Some(Command::QuestionCancel));
                            }
                        }
                    }
                    return Ok(self.mouse_to_command(mouse, state));
                }
                Event::Paste(text) => {
                    return Ok(Self::handle_paste(state, text));
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
            AppEvent::LlmTextChunk(_) | AppEvent::LlmThinkingChunk(_) => {
                // Just trigger a render cycle - state is already updated by BFF
                // Cache is updated incrementally during render()
            }
            AppEvent::LlmCompleted { .. } | AppEvent::LlmError(_) => {
                // Streaming completed or errored - clear the incremental markdown caches
                self.streaming_markdown_cache.clear();
                self.streaming_thinking_cache.clear();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_backend::questionnaire::QuestionnaireState;
    use crate::ui_backend::ModelInfo;
    use crossterm::event::KeyEvent;

    #[test]
    fn test_model_display_name_ignores_picker_selection() {
        let state = SharedState::new();
        state.set_available_models(vec![
            ModelInfo {
                id: "model-a".to_string(),
                name: "Model A".to_string(),
                description: String::new(),
                provider: "test".to_string(),
                context_window: 0,
                max_tokens: 0,
            },
            ModelInfo {
                id: "model-b".to_string(),
                name: "Model B".to_string(),
                description: String::new(),
                provider: "test".to_string(),
                context_window: 0,
                max_tokens: 0,
            },
        ]);

        state.set_model(Some("model-a".to_string()));
        state.set_model_picker_selected(1);

        let name = resolve_model_display_name(&state, &state.current_model());
        assert_eq!(name.as_deref(), Some("Model A"));
    }

    #[test]
    fn test_handle_paste_inserts_text_when_input_focused() {
        let state = SharedState::new();
        state.set_focused_component(FocusedComponent::Input);

        let cmd =
            TuiRenderer::<ratatui::backend::TestBackend>::handle_paste(&state, "hello".to_string());

        assert_eq!(cmd, Some(Command::InsertText("hello".to_string())));
    }

    #[test]
    fn test_handle_paste_inserts_placeholder_for_multiline() {
        let state = SharedState::new();
        state.set_focused_component(FocusedComponent::Input);

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::handle_paste(
            &state,
            "line1\nline2".to_string(),
        );

        assert_eq!(
            cmd,
            Some(Command::InsertText("[pasted 2 lines]".to_string()))
        );
        let blocks = state.paste_blocks();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].content, "line1\nline2");
    }

    #[test]
    fn test_handle_paste_routes_to_questionnaire() {
        let state = SharedState::new();
        let mut questionnaire = QuestionnaireState::new(
            "q1".to_string(),
            "Question".to_string(),
            crate::ui_backend::questionnaire::QuestionType::FreeText,
            Vec::new(),
        );
        questionnaire.is_editing_free_text = true;
        state.set_active_questionnaire(Some(questionnaire));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::handle_paste(
            &state,
            "first\nsecond".to_string(),
        );

        assert!(cmd.is_none());
        let updated = state.active_questionnaire().expect("questionnaire");
        assert_eq!(updated.free_text_answer, "first\nsecond");
    }

    // ========== Vim Key Handling Tests ==========

    #[test]
    fn test_i_focuses_input_from_messages() {
        let state = SharedState::new();
        state.set_focused_component(FocusedComponent::Messages);

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('i'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::FocusInput));
    }

    #[test]
    fn test_jk_moves_between_messages_in_messages_panel() {
        let state = SharedState::new();
        state.set_focused_component(FocusedComponent::Messages);
        state.set_vim_mode(crate::ui_backend::VimMode::Normal);

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::NextMessage));
        assert_eq!(cmd_k, Some(Command::PrevMessage));
    }

    /// Helper to create a key event
    fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    /// Test: Vim keys (j, k, h, l) should be typed as characters in picker modals with filter
    #[test]
    fn test_vim_keys_go_to_filter_in_model_picker() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::ModelPicker));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );
        let cmd_h = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            &state,
        );
        let cmd_l = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('l'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalFilter("j".to_string())));
        assert_eq!(cmd_k, Some(Command::ModalFilter("k".to_string())));
        assert_eq!(cmd_h, Some(Command::ModalFilter("h".to_string())));
        assert_eq!(cmd_l, Some(Command::ModalFilter("l".to_string())));
    }

    #[test]
    fn test_vim_keys_go_to_filter_in_theme_picker() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::ThemePicker));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalFilter("j".to_string())));
        assert_eq!(cmd_k, Some(Command::ModalFilter("k".to_string())));
    }

    #[test]
    fn test_vim_keys_go_to_filter_in_provider_picker() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::ProviderPicker));

        let cmd_y = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('y'), KeyModifiers::NONE),
            &state,
        );
        let cmd_v = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('v'), KeyModifiers::NONE),
            &state,
        );
        let cmd_g = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('g'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_y, Some(Command::ModalFilter("y".to_string())));
        assert_eq!(cmd_v, Some(Command::ModalFilter("v".to_string())));
        assert_eq!(cmd_g, Some(Command::ModalFilter("g".to_string())));
    }

    #[test]
    fn test_vim_keys_go_to_filter_in_session_picker() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::SessionPicker));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_l = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('l'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalFilter("j".to_string())));
        assert_eq!(cmd_l, Some(Command::ModalFilter("l".to_string())));
    }

    /// Test: Vim keys (j, k) should navigate in selection-only modals (no text input)
    #[test]
    fn test_vim_keys_navigate_in_trust_modal() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::TrustLevel));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalDown));
        assert_eq!(cmd_k, Some(Command::ModalUp));
    }

    #[test]
    fn test_vim_keys_navigate_in_tools_modal() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::Tools));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalDown));
        assert_eq!(cmd_k, Some(Command::ModalUp));
    }

    #[test]
    fn test_vim_keys_navigate_in_plugin_modal() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::Plugin));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, Some(Command::ModalDown));
        assert_eq!(cmd_k, Some(Command::ModalUp));
    }

    /// Test: Arrow keys work for navigation in filter pickers (using ModalUp/ModalDown)
    #[test]
    fn test_arrow_keys_navigate_in_model_picker() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::ModelPicker));

        let cmd_up = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Up, KeyModifiers::NONE),
            &state,
        );
        let cmd_down = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Down, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_up, Some(Command::ModalUp));
        assert_eq!(cmd_down, Some(Command::ModalDown));
    }

    /// Test: FilePicker has its own filter mechanism (updates state directly)
    #[test]
    fn test_vim_keys_update_file_picker_filter_directly() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::FilePicker));
        state.set_file_picker_filter("test".to_string());

        let _cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );

        // FilePicker updates the filter directly in the key handler
        assert_eq!(state.file_picker_filter(), "testj");
    }

    #[test]
    fn test_vim_keys_disabled_for_single_choice_with_other() {
        let state = SharedState::new();
        let q = QuestionnaireState::new(
            "test-id".to_string(),
            "Pick one".to_string(),
            crate::ui_backend::questionnaire::QuestionType::SingleChoice,
            vec![crate::ui_backend::questionnaire::QuestionOption {
                text: "Option A".to_string(),
                value: "a".to_string(),
            }],
        );
        state.set_active_questionnaire(Some(q));

        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, None);
        let updated = state.active_questionnaire().unwrap();
        assert_eq!(updated.focused_index, 0);
    }

    #[test]
    fn test_vim_keys_disabled_in_task_edit_modal() {
        let state = SharedState::new();
        state.set_active_modal(Some(ModalType::TaskEdit));
        state.set_editing_task_content(String::new());

        let _cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(state.editing_task_content(), "j");
    }

    // ========== FreeText Edit Mode Tests ==========

    /// Helper to create a FreeText questionnaire for testing
    fn create_free_text_questionnaire(is_editing: bool) -> QuestionnaireState {
        let mut q = QuestionnaireState::new(
            "test-id".to_string(),
            "What is your name?".to_string(),
            crate::ui_backend::questionnaire::QuestionType::FreeText,
            vec![],
        );
        if is_editing {
            q.start_editing_free_text();
        }
        q
    }

    /// Test: FreeText questionnaire - Enter starts edit mode when not editing
    #[test]
    fn test_freetext_enter_starts_edit_mode() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionStartEdit));
    }

    /// Test: FreeText questionnaire - Enter submits when in edit mode
    #[test]
    fn test_freetext_enter_submits_when_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionSubmit));
    }

    /// Test: FreeText questionnaire - Escape stops editing when in edit mode
    #[test]
    fn test_freetext_escape_stops_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Esc, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionStopEdit));
    }

    /// Test: FreeText questionnaire - Escape cancels when not editing
    #[test]
    fn test_freetext_escape_cancels_when_not_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Esc, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionCancel));
    }

    /// Test: FreeText questionnaire - j/k are blocked when not editing
    #[test]
    fn test_freetext_jk_blocked_when_not_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        // j should return None (no navigation for FreeText)
        let cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        // k should return None (no navigation for FreeText)
        let cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd_j, None);
        assert_eq!(cmd_k, None);

        // Verify no characters were inserted
        let q = state.active_questionnaire().unwrap();
        assert_eq!(q.free_text_answer, "");
    }

    /// Test: FreeText questionnaire - j/k insert characters when editing
    #[test]
    fn test_freetext_jk_insert_when_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        // j should insert 'j' via state.questionnaire_insert_char()
        let _cmd_j = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            &state,
        );
        // k should insert 'k' via state.questionnaire_insert_char()
        let _cmd_k = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('k'), KeyModifiers::NONE),
            &state,
        );

        // Verify characters were inserted
        let q = state.active_questionnaire().unwrap();
        assert_eq!(q.free_text_answer, "jk");
    }

    /// Test: FreeText questionnaire - other vim keys (l, h) blocked when not editing
    #[test]
    fn test_freetext_other_keys_blocked_when_not_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        // These should return None (blocked)
        let _cmd_l = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('l'), KeyModifiers::NONE),
            &state,
        );
        let _cmd_h = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            &state,
        );

        // Verify no characters were inserted
        let q = state.active_questionnaire().unwrap();
        assert_eq!(q.free_text_answer, "");
    }

    /// Test: FreeText questionnaire - other vim keys (l, h) insert when editing
    #[test]
    fn test_freetext_other_keys_insert_when_editing() {
        let state = SharedState::new();
        let q = create_free_text_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        // These should insert characters
        let _cmd_l = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('l'), KeyModifiers::NONE),
            &state,
        );
        let _cmd_h = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Char('h'), KeyModifiers::NONE),
            &state,
        );

        // Verify characters were inserted
        let q = state.active_questionnaire().unwrap();
        assert_eq!(q.free_text_answer, "lh");
    }

    // ========== "Other" Option Edit Mode Tests ==========

    /// Helper to create a MultipleChoice questionnaire with "Other" selected
    fn create_other_questionnaire(is_editing: bool) -> QuestionnaireState {
        let mut q = QuestionnaireState::new(
            "test-id".to_string(),
            "Select options".to_string(),
            crate::ui_backend::questionnaire::QuestionType::MultipleChoice,
            vec![crate::ui_backend::questionnaire::QuestionOption {
                text: "Option A".to_string(),
                value: "a".to_string(),
            }],
        );
        // Focus on "Other" (index 1 for 1 option)
        q.focused_index = 1;
        q.other_selected = true;
        if is_editing {
            q.start_editing_other_text();
        }
        q
    }

    /// Test: "Other" option - Enter starts edit mode when not editing
    #[test]
    fn test_other_enter_starts_edit_mode() {
        let state = SharedState::new();
        let q = create_other_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionStartEdit));
    }

    /// Test: "Other" option - Enter submits when in edit mode
    #[test]
    fn test_other_enter_submits_when_editing() {
        let state = SharedState::new();
        let q = create_other_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Enter, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionSubmit));
    }

    /// Test: "Other" option - Escape stops editing when in edit mode
    #[test]
    fn test_other_escape_stops_editing() {
        let state = SharedState::new();
        let q = create_other_questionnaire(true);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Esc, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionStopEdit));
    }

    /// Test: "Other" option - Escape cancels questionnaire when not editing
    #[test]
    fn test_other_escape_cancels_when_not_editing() {
        let state = SharedState::new();
        let q = create_other_questionnaire(false);
        state.set_active_questionnaire(Some(q));

        let cmd = TuiRenderer::<ratatui::backend::TestBackend>::key_to_command(
            key_event(KeyCode::Esc, KeyModifiers::NONE),
            &state,
        );

        assert_eq!(cmd, Some(Command::QuestionCancel));
    }
}
