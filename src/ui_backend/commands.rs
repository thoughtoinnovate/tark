//! User Commands
//!
//! Defines all possible user actions that can be triggered from the UI.

use crossterm::event::KeyEvent;

/// User commands that can be executed
///
/// These represent user actions translated from keybindings, mouse clicks,
/// menu selections, etc. The AppService handles these commands and updates state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    // ========== Application Control ==========
    /// Quit the application
    Quit,

    /// Toggle help modal
    ToggleHelp,

    // ========== Focus Management ==========
    /// Cycle focus to next component
    FocusNext,

    /// Cycle focus to previous component
    FocusPrevious,

    /// Focus the input area
    FocusInput,

    /// Focus the message list
    FocusMessages,

    /// Focus the sidebar panel
    FocusPanel,

    // ========== Agent Mode ==========
    /// Cycle to next agent mode (Build -> Plan -> Ask -> Build)
    CycleAgentMode,

    /// Set agent mode explicitly
    SetAgentMode(AgentMode),

    // ========== Build Mode ==========
    /// Cycle to next build mode (Manual -> Balanced -> Careful -> Manual)
    CycleBuildMode,

    /// Set build mode explicitly
    SetBuildMode(BuildMode),

    // ========== UI Toggles ==========
    /// Toggle sidebar visibility
    ToggleSidebar,

    /// Toggle thinking block display
    ToggleThinking,

    /// Toggle theme picker modal
    ToggleThemePicker,

    // ========== Provider/Model Selection ==========
    /// Open provider picker modal
    OpenProviderPicker,

    /// Open model picker modal
    OpenModelPicker,

    /// Select a provider
    SelectProvider(String),

    /// Select a model
    SelectModel(String),

    // ========== Message Input ==========
    /// Send the current input as a message
    SendMessage(String),

    /// Insert a character at cursor
    InsertChar(char),

    /// Delete character before cursor
    DeleteCharBefore,

    /// Delete character after cursor
    DeleteCharAfter,

    /// Move cursor left
    CursorLeft,

    /// Move cursor right
    CursorRight,

    /// Move cursor to start of line
    CursorToLineStart,

    /// Move cursor to end of line
    CursorToLineEnd,

    /// Move cursor forward one word
    CursorWordForward,

    /// Move cursor backward one word
    CursorWordBackward,

    /// Insert newline
    InsertNewline,

    /// Clear input
    ClearInput,

    // ========== Message Navigation ==========
    /// Navigate to next message
    NextMessage,

    /// Prev message
    PrevMessage,

    /// Scroll messages up
    ScrollUp,

    /// Scroll messages down
    ScrollDown,

    /// Collapse/expand focused message
    ToggleMessageCollapse,

    /// Yank (copy) message content
    YankMessage,

    // ========== Context Files ==========
    /// Open file picker to add context
    OpenFilePicker,

    /// Add a file to context
    AddContextFile(String),

    /// Remove a file from context
    RemoveContextFile(String),

    // ========== Input History ==========
    /// Navigate to previous input in history
    HistoryPrevious,

    /// Navigate to next input in history
    HistoryNext,

    // ========== Interruption ==========
    /// Interrupt current LLM operation
    Interrupt,

    // ========== Modal Interaction ==========
    /// Close current modal
    CloseModal,

    /// Confirm modal action
    ConfirmModal,

    /// Navigate modal selection up
    ModalUp,

    /// Navigate modal selection down
    ModalDown,

    /// Filter modal items (for search)
    ModalFilter(String),
}

/// Agent operation mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentMode {
    Build,
    Plan,
    Ask,
}

/// Build mode (only active in Build agent mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildMode {
    Manual,
    Balanced,
    Careful,
}

impl AgentMode {
    pub fn next(&self) -> Self {
        match self {
            AgentMode::Build => AgentMode::Plan,
            AgentMode::Plan => AgentMode::Ask,
            AgentMode::Ask => AgentMode::Build,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            AgentMode::Build => "Build",
            AgentMode::Plan => "Plan",
            AgentMode::Ask => "Ask",
        }
    }
}

impl BuildMode {
    pub fn next(&self) -> Self {
        match self {
            BuildMode::Manual => BuildMode::Balanced,
            BuildMode::Balanced => BuildMode::Careful,
            BuildMode::Careful => BuildMode::Manual,
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            BuildMode::Manual => "Manual",
            BuildMode::Balanced => "Balanced",
            BuildMode::Careful => "Careful",
        }
    }
}

/// Convert keyboard events to commands
pub fn key_to_command(key: KeyEvent) -> Option<Command> {
    use crossterm::event::{KeyCode, KeyModifiers};

    match (key.code, key.modifiers) {
        // Application control
        (KeyCode::Char('c'), KeyModifiers::CONTROL)
        | (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(Command::Quit),
        (KeyCode::Char('?'), _) => Some(Command::ToggleHelp),

        // Focus management
        (KeyCode::Tab, KeyModifiers::NONE) => Some(Command::FocusNext),
        (KeyCode::BackTab, KeyModifiers::SHIFT) => Some(Command::CycleAgentMode),

        // Mode cycling
        (KeyCode::Char('m'), KeyModifiers::CONTROL) => Some(Command::CycleBuildMode),

        // UI toggles
        (KeyCode::Char('b'), KeyModifiers::CONTROL) => Some(Command::ToggleSidebar),
        (KeyCode::Char('t'), KeyModifiers::CONTROL) => Some(Command::ToggleThinking),

        // Message sending
        (KeyCode::Enter, KeyModifiers::NONE) => Some(Command::SendMessage(String::new())),
        (KeyCode::Enter, KeyModifiers::SHIFT) => Some(Command::InsertNewline),

        // Text editing
        (KeyCode::Char(c), KeyModifiers::NONE | KeyModifiers::SHIFT) => {
            Some(Command::InsertChar(c))
        }
        (KeyCode::Backspace, _) => Some(Command::DeleteCharBefore),
        (KeyCode::Delete, _) => Some(Command::DeleteCharAfter),

        // Cursor movement
        (KeyCode::Left, KeyModifiers::NONE) => Some(Command::CursorLeft),
        (KeyCode::Right, KeyModifiers::NONE) => Some(Command::CursorRight),
        (KeyCode::Home, _) => Some(Command::CursorToLineStart),
        (KeyCode::End, _) => Some(Command::CursorToLineEnd),
        (KeyCode::Left, KeyModifiers::CONTROL) => Some(Command::CursorWordBackward),
        (KeyCode::Right, KeyModifiers::CONTROL) => Some(Command::CursorWordForward),

        // Message navigation
        (KeyCode::Up, _) => Some(Command::HistoryPrevious),
        (KeyCode::Down, _) => Some(Command::HistoryNext),

        // Modals
        (KeyCode::Esc, _) => Some(Command::CloseModal),

        _ => None,
    }
}
