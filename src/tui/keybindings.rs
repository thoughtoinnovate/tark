//! Keybinding handler for vim-style navigation
//!
//! Provides a centralized keybinding system with support for vim-style
//! navigation commands (j/k, gg/G, Ctrl-d/Ctrl-u) and focus management.

#![allow(dead_code)]

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::HashMap;

/// Actions that can be triggered by keybindings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    // Navigation
    /// Move down one line (j)
    LineDown,
    /// Move up one line (k)
    LineUp,
    /// Go to top (gg)
    GoToTop,
    /// Go to bottom (G)
    GoToBottom,
    /// Page down (Ctrl-d)
    HalfPageDown,
    /// Page up (Ctrl-u)
    HalfPageUp,

    // Focus management
    /// Cycle focus to next component (Tab)
    FocusNext,
    /// Cycle focus to previous component (Shift-Tab)
    FocusPrevious,
    /// Focus input area
    FocusInput,
    /// Focus message list
    FocusMessages,
    /// Focus panel
    FocusPanel,

    // Panel-specific
    /// Expand section (zo)
    ExpandSection,
    /// Collapse section (zc)
    CollapseSection,
    /// Toggle section (za)
    ToggleSection,

    // Input mode
    /// Enter insert mode (i)
    EnterInsertMode,
    /// Exit insert mode (Escape)
    ExitInsertMode,

    // General
    /// Quit application (q, Ctrl-c, Ctrl-q)
    Quit,
    /// Submit/Enter
    Submit,
    /// Cancel/Escape
    Cancel,
}

/// Input mode for the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InputMode {
    /// Normal mode - vim-style navigation
    #[default]
    Normal,
    /// Insert mode - typing in input
    Insert,
    /// Command mode - entering slash commands
    Command,
}

/// Focused component in the UI
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedComponent {
    /// Input area at the bottom
    #[default]
    Input,
    /// Message list/history
    Messages,
    /// Side panel (tasks, notifications, files)
    Panel,
}

impl FocusedComponent {
    /// Get the next component in the focus cycle
    pub fn next(self) -> Self {
        match self {
            FocusedComponent::Input => FocusedComponent::Messages,
            FocusedComponent::Messages => FocusedComponent::Panel,
            FocusedComponent::Panel => FocusedComponent::Input,
        }
    }

    /// Get the previous component in the focus cycle
    pub fn previous(self) -> Self {
        match self {
            FocusedComponent::Input => FocusedComponent::Panel,
            FocusedComponent::Messages => FocusedComponent::Input,
            FocusedComponent::Panel => FocusedComponent::Messages,
        }
    }
}

/// State for tracking multi-key sequences (like gg, zo, zc)
#[derive(Debug, Clone, Default)]
pub struct KeySequenceState {
    /// Pending key for multi-key sequences
    pending_key: Option<char>,
}

impl KeySequenceState {
    /// Create a new key sequence state
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear any pending key
    pub fn clear(&mut self) {
        self.pending_key = None;
    }

    /// Check if there's a pending key
    pub fn has_pending(&self) -> bool {
        self.pending_key.is_some()
    }

    /// Get the pending key
    pub fn pending(&self) -> Option<char> {
        self.pending_key
    }

    /// Set a pending key
    pub fn set_pending(&mut self, key: char) {
        self.pending_key = Some(key);
    }
}

/// Keybinding handler
#[derive(Debug)]
pub struct KeybindingHandler {
    /// Key sequence state for multi-key bindings
    sequence_state: KeySequenceState,
    /// Custom keybindings (for future configuration support)
    custom_bindings: HashMap<(KeyCode, KeyModifiers), Action>,
}

impl Default for KeybindingHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl KeybindingHandler {
    /// Create a new keybinding handler with default bindings
    pub fn new() -> Self {
        Self {
            sequence_state: KeySequenceState::new(),
            custom_bindings: HashMap::new(),
        }
    }

    /// Add a custom keybinding
    pub fn add_binding(&mut self, key: KeyCode, modifiers: KeyModifiers, action: Action) {
        self.custom_bindings.insert((key, modifiers), action);
    }

    /// Clear the key sequence state
    pub fn clear_sequence(&mut self) {
        self.sequence_state.clear();
    }

    /// Handle a key event in normal mode
    ///
    /// Returns the action to perform, if any.
    pub fn handle_normal_mode(&mut self, key: KeyEvent) -> Option<Action> {
        // Check for custom bindings first
        if let Some(action) = self.custom_bindings.get(&(key.code, key.modifiers)) {
            self.sequence_state.clear();
            return Some(*action);
        }

        // Handle multi-key sequences
        if let Some(pending) = self.sequence_state.pending() {
            self.sequence_state.clear();
            return self.handle_sequence(pending, key);
        }

        // Handle single key bindings
        match (key.code, key.modifiers) {
            // Quit
            (KeyCode::Char('c'), KeyModifiers::CONTROL)
            | (KeyCode::Char('q'), KeyModifiers::CONTROL) => Some(Action::Quit),
            (KeyCode::Char('q'), KeyModifiers::NONE) => Some(Action::Quit),

            // Navigation
            (KeyCode::Char('j'), KeyModifiers::NONE) | (KeyCode::Down, KeyModifiers::NONE) => {
                Some(Action::LineDown)
            }
            (KeyCode::Char('k'), KeyModifiers::NONE) | (KeyCode::Up, KeyModifiers::NONE) => {
                Some(Action::LineUp)
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => Some(Action::HalfPageDown),
            (KeyCode::Char('u'), KeyModifiers::CONTROL) => Some(Action::HalfPageUp),
            (KeyCode::Char('G'), KeyModifiers::SHIFT) => Some(Action::GoToBottom),

            // Multi-key sequences
            (KeyCode::Char('g'), KeyModifiers::NONE) => {
                self.sequence_state.set_pending('g');
                None
            }
            (KeyCode::Char('z'), KeyModifiers::NONE) => {
                self.sequence_state.set_pending('z');
                None
            }

            // Focus management
            (KeyCode::Tab, KeyModifiers::NONE) => Some(Action::FocusNext),
            (KeyCode::BackTab, KeyModifiers::SHIFT) | (KeyCode::Tab, KeyModifiers::SHIFT) => {
                Some(Action::FocusPrevious)
            }

            // Enter insert mode
            (KeyCode::Char('i'), KeyModifiers::NONE) | (KeyCode::Enter, KeyModifiers::NONE) => {
                Some(Action::EnterInsertMode)
            }

            // Escape
            (KeyCode::Esc, KeyModifiers::NONE) => Some(Action::Cancel),

            _ => None,
        }
    }

    /// Handle a key event in insert mode
    ///
    /// Returns the action to perform, if any.
    /// Most keys in insert mode are passed through to the input widget.
    pub fn handle_insert_mode(&mut self, key: KeyEvent) -> Option<Action> {
        match (key.code, key.modifiers) {
            // Exit insert mode
            (KeyCode::Esc, KeyModifiers::NONE) => Some(Action::ExitInsertMode),

            // Quit (always available)
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Quit),

            // Submit
            (KeyCode::Enter, KeyModifiers::NONE) => Some(Action::Submit),

            // Everything else is handled by the input widget
            _ => None,
        }
    }

    /// Handle multi-key sequences
    fn handle_sequence(&self, pending: char, key: KeyEvent) -> Option<Action> {
        match pending {
            'g' => match key.code {
                KeyCode::Char('g') => Some(Action::GoToTop),
                _ => None,
            },
            'z' => match key.code {
                KeyCode::Char('o') => Some(Action::ExpandSection),
                KeyCode::Char('c') => Some(Action::CollapseSection),
                KeyCode::Char('a') => Some(Action::ToggleSection),
                _ => None,
            },
            _ => None,
        }
    }

    /// Handle a key event based on the current mode
    pub fn handle_key(&mut self, key: KeyEvent, mode: InputMode) -> Option<Action> {
        match mode {
            InputMode::Normal => self.handle_normal_mode(key),
            InputMode::Insert | InputMode::Command => self.handle_insert_mode(key),
        }
    }

    /// Check if there's a pending key sequence
    pub fn has_pending_sequence(&self) -> bool {
        self.sequence_state.has_pending()
    }

    /// Get the pending key for display
    pub fn pending_key(&self) -> Option<char> {
        self.sequence_state.pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn test_focused_component_cycle() {
        let input = FocusedComponent::Input;
        assert_eq!(input.next(), FocusedComponent::Messages);
        assert_eq!(input.previous(), FocusedComponent::Panel);

        let messages = FocusedComponent::Messages;
        assert_eq!(messages.next(), FocusedComponent::Panel);
        assert_eq!(messages.previous(), FocusedComponent::Input);

        let panel = FocusedComponent::Panel;
        assert_eq!(panel.next(), FocusedComponent::Input);
        assert_eq!(panel.previous(), FocusedComponent::Messages);
    }

    #[test]
    fn test_normal_mode_navigation() {
        let mut handler = KeybindingHandler::new();

        // j for line down
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('j'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::LineDown));

        // k for line up
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('k'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::LineUp));

        // Ctrl-d for half page down
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('d'), KeyModifiers::CONTROL));
        assert_eq!(action, Some(Action::HalfPageDown));

        // Ctrl-u for half page up
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('u'), KeyModifiers::CONTROL));
        assert_eq!(action, Some(Action::HalfPageUp));

        // G for go to bottom
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('G'), KeyModifiers::SHIFT));
        assert_eq!(action, Some(Action::GoToBottom));
    }

    #[test]
    fn test_gg_sequence() {
        let mut handler = KeybindingHandler::new();

        // First g - should set pending
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(action, None);
        assert!(handler.has_pending_sequence());
        assert_eq!(handler.pending_key(), Some('g'));

        // Second g - should trigger GoToTop
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::GoToTop));
        assert!(!handler.has_pending_sequence());
    }

    #[test]
    fn test_z_sequences() {
        let mut handler = KeybindingHandler::new();

        // zo - expand section
        handler.handle_normal_mode(make_key_event(KeyCode::Char('z'), KeyModifiers::NONE));
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('o'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::ExpandSection));

        // zc - collapse section
        handler.handle_normal_mode(make_key_event(KeyCode::Char('z'), KeyModifiers::NONE));
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('c'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::CollapseSection));

        // za - toggle section
        handler.handle_normal_mode(make_key_event(KeyCode::Char('z'), KeyModifiers::NONE));
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::ToggleSection));
    }

    #[test]
    fn test_focus_management() {
        let mut handler = KeybindingHandler::new();

        // Tab for focus next
        let action = handler.handle_normal_mode(make_key_event(KeyCode::Tab, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::FocusNext));

        // Shift-Tab for focus previous
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(action, Some(Action::FocusPrevious));
    }

    #[test]
    fn test_insert_mode() {
        let mut handler = KeybindingHandler::new();

        // Escape exits insert mode
        let action = handler.handle_insert_mode(make_key_event(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::ExitInsertMode));

        // Enter submits
        let action = handler.handle_insert_mode(make_key_event(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::Submit));

        // Regular keys return None (handled by input widget)
        let action =
            handler.handle_insert_mode(make_key_event(KeyCode::Char('a'), KeyModifiers::NONE));
        assert_eq!(action, None);
    }

    #[test]
    fn test_quit_keys() {
        let mut handler = KeybindingHandler::new();

        // q quits in normal mode
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('q'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::Quit));

        // Ctrl-c quits in any mode
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(action, Some(Action::Quit));

        let action =
            handler.handle_insert_mode(make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert_eq!(action, Some(Action::Quit));
    }

    #[test]
    fn test_enter_insert_mode() {
        let mut handler = KeybindingHandler::new();

        // i enters insert mode
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('i'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::EnterInsertMode));

        // Enter also enters insert mode (for quick typing)
        let action = handler.handle_normal_mode(make_key_event(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::EnterInsertMode));
    }

    #[test]
    fn test_arrow_keys() {
        let mut handler = KeybindingHandler::new();

        // Down arrow for line down
        let action = handler.handle_normal_mode(make_key_event(KeyCode::Down, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::LineDown));

        // Up arrow for line up
        let action = handler.handle_normal_mode(make_key_event(KeyCode::Up, KeyModifiers::NONE));
        assert_eq!(action, Some(Action::LineUp));
    }

    #[test]
    fn test_sequence_cleared_on_invalid() {
        let mut handler = KeybindingHandler::new();

        // Start g sequence
        handler.handle_normal_mode(make_key_event(KeyCode::Char('g'), KeyModifiers::NONE));
        assert!(handler.has_pending_sequence());

        // Invalid second key - sequence should be cleared
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(action, None);
        assert!(!handler.has_pending_sequence());
    }

    #[test]
    fn test_custom_binding() {
        let mut handler = KeybindingHandler::new();

        // Add custom binding
        handler.add_binding(KeyCode::Char('x'), KeyModifiers::NONE, Action::Quit);

        // Custom binding should work
        let action =
            handler.handle_normal_mode(make_key_event(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(action, Some(Action::Quit));
    }

    #[test]
    fn test_handle_key_with_mode() {
        let mut handler = KeybindingHandler::new();

        // Normal mode
        let action = handler.handle_key(
            make_key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            InputMode::Normal,
        );
        assert_eq!(action, Some(Action::LineDown));

        // Insert mode - j is not a navigation key
        let action = handler.handle_key(
            make_key_event(KeyCode::Char('j'), KeyModifiers::NONE),
            InputMode::Insert,
        );
        assert_eq!(action, None);
    }
}
