# Feature: Keyboard Shortcuts
# Tests all keyboard shortcuts and navigation

@keyboard @navigation
Feature: Keyboard Shortcuts
  As a user of the TUI application
  I want to use keyboard shortcuts
  So that I can navigate and control the application efficiently

  Background:
    Given the TUI application is running

  # =============================================================================
  # GLOBAL SHORTCUTS
  # =============================================================================

  @global
  Scenario Outline: Global keyboard shortcuts
    When I press "<shortcut>"
    Then "<action>" should occur

    Examples:
      | shortcut | action                      |
      | ?        | Open help modal             |
      | Ctrl+C   | Cancel current operation    |
      | Ctrl+L   | Clear terminal              |
      | Ctrl+Q   | Quit application            |
      | Ctrl+B   | Toggle sidebar              |
      | Ctrl+T   | Toggle thinking mode        |

  # =============================================================================
  # MODE SHORTCUTS
  # =============================================================================

  @mode-shortcuts
  Scenario: Switch to Build mode
    Given the current mode is "Plan"
    When I press "Ctrl+1"
    Then the agent mode should change to "Build"

  Scenario: Switch to Plan mode
    Given the current mode is "Build"
    When I press "Ctrl+2"
    Then the agent mode should change to "Plan"

  Scenario: Switch to Ask mode
    Given the current mode is "Build"
    When I press "Ctrl+3"
    Then the agent mode should change to "Ask"

  # =============================================================================
  # INPUT SHORTCUTS
  # =============================================================================

  @input-shortcuts
  Scenario: Submit message
    Given I have typed a message
    When I press "Enter"
    Then the message should be submitted

  Scenario: Clear input
    Given I have typed some text
    When I press "Escape"
    Then the input should be cleared

  Scenario: Input history navigation
    Given I have submitted messages before
    When I press "Up Arrow" in empty input
    Then the previous message should appear
    When I press "Down Arrow"
    Then the next message (or empty) should appear

  # =============================================================================
  # NAVIGATION SHORTCUTS
  # =============================================================================

  @navigation
  Scenario: Vim-style navigation in message area
    Given the message area has focus
    When I press "j"
    Then the view should scroll down
    When I press "k"
    Then the view should scroll up

  Scenario: Go to top/bottom
    Given there are many messages
    When I press "g" then "g"
    Then the view should scroll to the top
    When I press "G"
    Then the view should scroll to the bottom

  Scenario: Page navigation
    Given there are many messages
    When I press "Ctrl+D"
    Then the view should scroll down half page
    When I press "Ctrl+U"
    Then the view should scroll up half page

  # =============================================================================
  # MODAL SHORTCUTS
  # =============================================================================

  @modal-shortcuts
  Scenario: Close modal with Escape
    Given any modal is open
    When I press "Escape"
    Then the modal should close

  Scenario: Navigate modal options
    Given a modal with a list is open
    When I press "Down Arrow"
    Then the next option should be highlighted
    When I press "Up Arrow"
    Then the previous option should be highlighted

  Scenario: Select modal option
    Given a modal option is highlighted
    When I press "Enter"
    Then the option should be selected
    And the modal should close

  # =============================================================================
  # COMMAND SHORTCUTS
  # =============================================================================

  @command-shortcuts
  Scenario: Quick command access
    When I press "/"
    Then focus should be in input area
    And "/" should be typed
    And the input should be ready for command

  Scenario: File picker shortcut
    When I press "@"
    Then the file picker should open

  # =============================================================================
  # COPY/PASTE SHORTCUTS
  # =============================================================================

  @clipboard
  Scenario: Copy selected text
    Given some text is selected
    When I press "Ctrl+C" or "y"
    Then the text should be copied to clipboard

  Scenario: Paste from clipboard
    Given text is in the clipboard
    And the input area has focus
    When I press "Ctrl+V"
    Then the clipboard content should be pasted

  Scenario: Copy message content
    Given focus is on a message
    When I press "y" (yank)
    Then the message content should be copied

  # =============================================================================
  # QUESTION SHORTCUTS
  # =============================================================================

  @question-shortcuts
  Scenario: Toggle checkbox option
    Given a multiple choice question is displayed
    And an option is focused
    When I press "Space"
    Then the option should toggle checked/unchecked

  Scenario: Select radio option
    Given a single choice question is displayed
    And an option is focused
    When I press "Space" or "Enter"
    Then the option should be selected

  Scenario: Number key selection
    Given a question with numbered options is displayed
    When I press "1"
    Then the first option should be selected
    When I press "2"
    Then the second option should be selected

  # =============================================================================
  # FOCUS MANAGEMENT
  # =============================================================================

  @focus
  Scenario: Tab cycles focus
    When I press "Tab" repeatedly
    Then focus should cycle through interactive elements
    And the focus order should be logical

  Scenario: Shift+Tab reverse focus
    When I press "Shift+Tab"
    Then focus should move to the previous element

  Scenario: Return focus to input
    Given focus is elsewhere
    When I press "i" (insert mode)
    Then focus should return to the input area

  # =============================================================================
  # ACCESSIBILITY
  # =============================================================================

  @accessibility
  Scenario: All actions accessible via keyboard
    Then every action in the application should be accessible via keyboard
    And mouse should not be required for any essential function

  Scenario: Shortcuts are discoverable
    When I press "?"
    Then all available shortcuts should be listed in the help modal
