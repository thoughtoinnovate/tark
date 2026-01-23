//! User Commands
//!
//! Defines all possible user actions that can be triggered from the UI.

use crossterm::event::KeyEvent;

// Re-export canonical types from core (for backward compatibility)
pub use crate::core::types::{AgentMode, BuildMode};

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

    /// Open trust level selector modal
    OpenTrustLevelSelector,

    /// Set trust level
    SetTrustLevel(crate::tools::TrustLevel),

    // ========== UI Toggles ==========
    /// Toggle sidebar visibility
    ToggleSidebar,

    /// Toggle thinking block display
    ToggleThinking,

    /// Toggle theme picker modal
    ToggleThemePicker,

    /// Toggle sidebar panel expansion (panel index: 0=Session, 1=Context, 2=Tasks, 3=Git)
    ToggleSidebarPanel(usize),

    /// Navigate up in sidebar
    SidebarUp,

    /// Navigate down in sidebar
    SidebarDown,

    /// Select item in sidebar (toggle expand/collapse or action on item)
    SidebarSelect,

    /// Enter into sidebar panel (select first item if expanded)
    SidebarEnter,

    /// Exit from inside sidebar panel back to panel header
    SidebarExit,

    /// Refresh sidebar data (e.g., after task reorder)
    RefreshSidebar,

    /// Set Vim editing mode
    SetVimMode(super::VimMode),

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

    /// Insert a full string (used for paste)
    InsertText(String),

    /// Clear input
    ClearInput,

    /// Delete current line (vim dd)
    DeleteLine,

    /// Delete word forward (vim dw)
    DeleteWord,

    /// Delete current selection (visual mode)
    DeleteSelection,

    /// Yank current selection (visual mode)
    YankSelection,

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

    /// Exit from tool group navigation (go back to message level)
    ExitGroup,

    /// Yank (copy) message content
    YankMessage,

    // ========== Context Files ==========
    /// Open file picker to add context
    OpenFilePicker,

    /// Add a file to context
    AddContextFile(String),

    /// Remove a file from context
    RemoveContextFile(String),

    /// Remove context file by index (from sidebar)
    RemoveContextByIndex(usize),

    // ========== Git Operations ==========
    /// Open git diff modal for a file
    OpenGitDiff(String),

    /// Stage a file for commit
    StageGitFile(String),

    // ========== File Picker Navigation ==========
    /// Navigate up in file picker
    FilePickerUp,

    /// Navigate down in file picker
    FilePickerDown,

    /// Select current file in picker
    FilePickerSelect,

    /// Update file picker filter
    UpdateFilePickerFilter(String),

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

    /// Delete the selected session from the session picker
    DeleteSessionSelected,

    /// Delete the selected policy pattern
    DeletePolicyPattern,

    /// Navigate modal selection up
    ModalUp,

    /// Navigate modal selection down
    ModalDown,

    /// Filter modal items (for search)
    ModalFilter(String),

    // ========== Approval Actions ==========
    /// Approve pending operation
    ApproveOperation,
    /// Approve with pattern for this session
    ApproveSession,
    /// Approve with pattern persistently
    ApproveAlways,

    /// Deny pending operation
    DenyOperation,
    /// Deny with pattern persistently
    DenyAlways,

    // ========== Questionnaire Actions ==========
    /// Navigate up in question options
    QuestionUp,

    /// Navigate down in question options
    QuestionDown,

    /// Toggle current question option
    QuestionToggle,

    /// Submit questionnaire answer
    QuestionSubmit,

    /// Cancel/skip questionnaire (ESC)
    QuestionCancel,

    /// Start editing free text input (Enter in FreeText question when not editing)
    QuestionStartEdit,

    /// Stop editing free text input (Escape in FreeText question when editing)
    QuestionStopEdit,

    // ========== Agent Control ==========
    /// Cancel ongoing agent operation (double-ESC)
    CancelAgent,

    // ========== Attachments ==========
    /// Toggle attachment dropdown
    ToggleAttachmentDropdown,

    /// Add an attachment
    AddAttachment(String),

    /// Remove an attachment
    RemoveAttachment(String),

    /// Clear all attachments
    ClearAttachments,

    // ========== Session Management ==========
    /// Create a new session
    NewSession,

    /// Switch to a different session
    SwitchSession(String),

    /// List all sessions
    ListSessions,

    /// Export current session to file
    ExportSession(std::path::PathBuf),

    // ========== Autocomplete ==========
    /// Select autocomplete option (TAB)
    AutocompleteSelect,

    /// Confirm autocomplete option (ENTER)
    AutocompleteConfirm,

    /// Navigate autocomplete up
    AutocompleteUp,

    /// Navigate autocomplete down
    AutocompleteDown,

    /// Deactivate autocomplete
    AutocompleteCancel,

    // ========== Task Queue Management ==========
    /// Open edit modal for a queued task at index
    EditQueuedTask(usize),

    /// Request deletion of queued task (shows confirmation)
    DeleteQueuedTask(usize),

    /// Confirm deletion of the pending task
    ConfirmDeleteTask,

    /// Cancel task deletion
    CancelDeleteTask,

    /// Move queued task up in the queue
    MoveTaskUp(usize),

    /// Move queued task down in the queue
    MoveTaskDown(usize),

    /// Update task edit content (while editing)
    UpdateTaskEditContent(String),

    /// Confirm task edit
    ConfirmTaskEdit,

    /// Cancel task edit
    CancelTaskEdit,
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
        (KeyCode::Char('A'), KeyModifiers::SHIFT) => Some(Command::OpenTrustLevelSelector),

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
