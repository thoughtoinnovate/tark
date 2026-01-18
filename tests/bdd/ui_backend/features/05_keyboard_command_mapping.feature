@ui_backend @keybindings
Feature: Keyboard Event to Command Mapping
  As a frontend
  I want keyboard events mapped to commands
  So that user input is properly translated to actions

  # ========================================================================
  # APPLICATION CONTROL
  # ========================================================================

  Scenario: Ctrl+C quits the application
    When the user presses "Ctrl+C"
    Then the key should map to "Quit" command

  Scenario: Ctrl+Q quits the application
    When the user presses "Ctrl+Q"
    Then the key should map to "Quit" command

  Scenario: Question mark toggles help
    When the user presses "?"
    Then the key should map to "ToggleHelp" command

  # ========================================================================
  # FOCUS MANAGEMENT
  # ========================================================================

  Scenario: Tab cycles focus forward
    When the user presses "Tab"
    Then the key should map to "FocusNext" command

  Scenario: Shift+Tab cycles agent mode
    When the user presses "Shift+Tab"
    Then the key should map to "CycleAgentMode" command
    # Note: This is different from BackTab for focus

  # ========================================================================
  # MODE CYCLING
  # ========================================================================

  Scenario: Ctrl+M cycles build mode
    When the user presses "Ctrl+M"
    Then the key should map to "CycleBuildMode" command

  # ========================================================================
  # UI TOGGLES
  # ========================================================================

  Scenario: Ctrl+B toggles sidebar
    When the user presses "Ctrl+B"
    Then the key should map to "ToggleSidebar" command

  Scenario: Ctrl+T toggles thinking display
    When the user presses "Ctrl+T"
    Then the key should map to "ToggleThinking" command

  # ========================================================================
  # MESSAGE SENDING
  # ========================================================================

  Scenario: Enter sends message
    When the user presses "Enter"
    Then the key should map to "SendMessage" command

  Scenario: Shift+Enter inserts newline
    When the user presses "Shift+Enter"
    Then the key should map to "InsertNewline" command

  # ========================================================================
  # TEXT EDITING
  # ========================================================================

  Scenario: Regular character inserts into input
    When the user presses "a"
    Then the key should map to "InsertChar('a')" command

  Scenario: Shift+character inserts uppercase
    When the user presses "Shift+A"
    Then the key should map to "InsertChar('A')" command

  Scenario: Backspace deletes before cursor
    When the user presses "Backspace"
    Then the key should map to "DeleteCharBefore" command

  Scenario: Delete deletes after cursor
    When the user presses "Delete"
    Then the key should map to "DeleteCharAfter" command

  # ========================================================================
  # CURSOR MOVEMENT
  # ========================================================================

  Scenario: Left arrow moves cursor left
    When the user presses "Left"
    Then the key should map to "CursorLeft" command

  Scenario: Right arrow moves cursor right
    When the user presses "Right"
    Then the key should map to "CursorRight" command

  Scenario: Home moves to line start
    When the user presses "Home"
    Then the key should map to "CursorToLineStart" command

  Scenario: End moves to line end
    When the user presses "End"
    Then the key should map to "CursorToLineEnd" command

  Scenario: Ctrl+Left moves cursor backward by word
    When the user presses "Ctrl+Left"
    Then the key should map to "CursorWordBackward" command

  Scenario: Ctrl+Right moves cursor forward by word
    When the user presses "Ctrl+Right"
    Then the key should map to "CursorWordForward" command

  # ========================================================================
  # HISTORY NAVIGATION
  # ========================================================================

  Scenario: Up arrow navigates to previous input
    When the user presses "Up"
    Then the key should map to "HistoryPrevious" command

  Scenario: Down arrow navigates to next input
    When the user presses "Down"
    Then the key should map to "HistoryNext" command

  # ========================================================================
  # MODAL INTERACTION
  # ========================================================================

  Scenario: Escape closes modal
    When the user presses "Escape"
    Then the key should map to "CloseModal" command

  # ========================================================================
  # UNMAPPED KEYS
  # ========================================================================

  Scenario: F1 key has no mapping
    When the user presses "F1"
    Then the key should not map to any command

  Scenario: Alt+A has no mapping
    When the user presses "Alt+A"
    Then the key should not map to any command
