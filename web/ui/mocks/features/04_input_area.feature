# Feature: Input Area
# Tests the input/prompt area functionality
# Reference: screenshots/01-main-layout.png

@input @core
Feature: Input Area
  As a user of the TUI application
  I want to type commands and messages in the input area
  So that I can interact with the AI agent

  Background:
    Given the TUI application is running
    And the input area has focus

  # =============================================================================
  # BASIC INPUT
  # =============================================================================

  @basic-input
  Scenario: Type text in input area
    When I type "Hello world"
    Then the input area should display "Hello world"
    And the cursor should be at the end of the text

  Scenario: Submit message with Enter
    Given I have typed "Build a TODO app"
    When I press "Enter"
    Then the message should be submitted
    And a new user message should appear in the message area
    And the input area should be cleared

  Scenario: Multi-line input with text wrapping
    Given the input area supports multi-line input
    When I type a message longer than the input width
    Then the text should wrap to the next line
    And the input area should expand vertically if needed

  Scenario: Clear input with Escape
    Given I have typed "Some text"
    When I press "Escape"
    Then the input area should be cleared

  # =============================================================================
  # CURSOR NAVIGATION
  # =============================================================================

  @cursor-navigation
  Scenario: Move cursor with arrow keys
    Given I have typed "Hello World"
    When I press "Left Arrow" 5 times
    Then the cursor should be between "Hello" and "World"
    When I press "Right Arrow" 2 times
    Then the cursor should be after "Wo"

  Scenario: Move cursor to start/end
    Given I have typed "Hello World"
    When I press "Home" or "Ctrl+A"
    Then the cursor should be at the start
    When I press "End" or "Ctrl+E"
    Then the cursor should be at the end

  Scenario: Move cursor by word
    Given I have typed "Hello Beautiful World"
    When I press "Ctrl+Left Arrow"
    Then the cursor should jump to the start of "World"
    When I press "Ctrl+Left Arrow" again
    Then the cursor should jump to the start of "Beautiful"

  # =============================================================================
  # TEXT EDITING
  # =============================================================================

  @text-editing
  Scenario: Delete character with Backspace
    Given I have typed "Hello"
    When I press "Backspace"
    Then the input should show "Hell"

  Scenario: Delete character with Delete
    Given I have typed "Hello" and cursor is after "Hel"
    When I press "Delete"
    Then the input should show "Helo"

  Scenario: Delete word with Ctrl+Backspace
    Given I have typed "Hello World"
    When I press "Ctrl+Backspace"
    Then the input should show "Hello "

  Scenario: Insert text in the middle
    Given I have typed "Hello World"
    And the cursor is after "Hello "
    When I type "Beautiful "
    Then the input should show "Hello Beautiful World"

  # =============================================================================
  # COMMAND DETECTION
  # =============================================================================

  @commands
  Scenario: Detect /model command
    When I type "/model"
    Then the command should be recognized
    And the provider picker modal should open
    And the input should be cleared

  Scenario: Detect /theme command
    When I type "/theme"
    Then the command should be recognized
    And the theme picker modal should open
    And the input should be cleared

  Scenario: Detect /help command
    When I type "/help"
    Then the command should be recognized
    And the help modal should open
    And the input should be cleared

  Scenario: Unknown command shows error
    When I type "/unknown" and press Enter
    Then an error message should be displayed
    And the message should say "Unknown command: /unknown"

  # =============================================================================
  # @ MENTION FILE PICKER
  # =============================================================================

  @file-mention
  Scenario: Typing @ triggers file picker
    When I type "@"
    Then the file picker modal should open immediately
    And the input should show "@"

  Scenario: File selection adds @filename to input
    Given the file picker is open
    When I select file "src/main.rs"
    Then the input should contain "@src/main.rs"
    And the file picker should close
    And the file should be added to context

  Scenario: Multiple file mentions
    Given I have "@config.toml" in the input
    When I type " and also @"
    Then the file picker should open again
    When I select "src/lib.rs"
    Then the input should show "@config.toml and also @src/lib.rs"
    And both files should be in context

  Scenario: Remove file from context by deleting @mention
    Given the input contains "@src/main.rs check this"
    And "src/main.rs" is in context
    When I delete the text "@src/main.rs"
    Then the file should be removed from context

  Scenario: Backspace removes entire @mention
    Given the input is "Look at @src/main.rs please"
    And the cursor is after "@src/main.rs"
    When I press "Backspace"
    Then the entire "@src/main.rs" should be deleted
    And the input should show "Look at  please"
    And the file should be removed from context

  # =============================================================================
  # CONTEXT FILES DISPLAY
  # =============================================================================

  @context-files
  Scenario: Display added context files
    Given files "config.toml" and "main.rs" are in context
    Then context file badges should be displayed above the input
    And each badge should show the filename with an "×" button

  Scenario: Remove context file via badge
    Given file "config.toml" is in context with a badge displayed
    When I click the "×" on the "config.toml" badge
    Then the file should be removed from context
    And the corresponding @mention should be removed from input

  Scenario: Click + button opens file picker
    When I click the "+" button in the input area
    Then the file picker modal should open

  # =============================================================================
  # INPUT HISTORY
  # =============================================================================

  @input-history
  Scenario: Navigate input history with Up Arrow
    Given I have previously submitted "First message"
    And I have previously submitted "Second message"
    When I press "Up Arrow"
    Then the input should show "Second message"
    When I press "Up Arrow" again
    Then the input should show "First message"

  Scenario: Navigate forward in history with Down Arrow
    Given I am viewing "First message" from history
    When I press "Down Arrow"
    Then the input should show "Second message"
    When I press "Down Arrow" again
    Then the input should be empty (current input)

  Scenario: Editing historical input creates new entry
    Given I recalled "Old message" from history
    When I edit it to "Old message modified"
    And I press "Enter"
    Then "Old message modified" should be submitted
    And the original "Old message" should remain in history

  # =============================================================================
  # VISUAL FEEDBACK
  # =============================================================================

  @visual-feedback
  Scenario: Input area shows focus state
    When the input area has focus
    Then the border should be highlighted
    And the cursor should be blinking

  Scenario: Input area shows disabled state
    Given a modal is open
    Then the input area should appear disabled
    And typing should not affect the input

  Scenario: Placeholder text when empty
    Given the input area is empty
    Then placeholder text "Type a message or command..." should be displayed
    When I start typing
    Then the placeholder should disappear
