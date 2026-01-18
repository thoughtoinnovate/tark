# Feature: File Picker Modal
# Tests the file picker/context file selection modal
# Reference: screenshots/09-file-picker-modal.png

@modal @file-picker
Feature: File Picker Modal
  As a user of the TUI application
  I want to select files to add as context
  So that the AI agent can reference them in responses

  Background:
    Given the TUI application is running
    And the current working directory contains source files

  # =============================================================================
  # MODAL OPENING
  # =============================================================================

  @modal-open
  Scenario: Open file picker by typing @
    When I type "@" in the input area
    Then the file picker modal should open immediately
    And the search input should have focus

  Scenario: Open file picker via + button
    When I click the "+" button in the input area
    Then the file picker modal should open
    And the search input should have focus

  Scenario: Modal has proper title and structure
    Given the file picker modal is open
    Then the modal title should be "Add Context File"
    And a search input should be visible at the top
    And a file list should be visible below the search

  # =============================================================================
  # FILE LISTING
  # =============================================================================

  @file-list
  Scenario: Display files in current directory
    Given the file picker modal is open
    Then files from the project should be listed
    And directories should be distinguishable from files
    And file icons should indicate file type

  Scenario: Show file type icons
    Given the file picker modal is open
    Then ".rs" files should show Rust icon "🦀"
    And ".ts" files should show TypeScript icon "📜"
    And ".md" files should show Markdown icon "📄"
    And directories should show folder icon "📁"

  Scenario: Show relative file paths
    Given the file picker modal is open
    Then file paths should be relative to project root
    And paths should be truncated if too long

  # =============================================================================
  # SEARCH/FILTER
  # =============================================================================

  @file-search
  Scenario: Filter files by typing
    Given the file picker modal is open
    When I type "main"
    Then only files containing "main" should be visible
    And "src/main.rs" should be in the list
    And "src/lib.rs" should not be visible

  Scenario: Search matches path components
    Given the file picker modal is open
    When I type "components"
    Then files in "src/components/" directory should be visible

  Scenario: Fuzzy search matching
    Given the file picker modal is open
    When I type "mntx"
    Then "src/main.tsx" should appear (fuzzy match for m-n-tx)

  Scenario: No matching files message
    Given the file picker modal is open
    When I type "nonexistentfile123"
    Then a "No files found" message should be displayed

  # =============================================================================
  # FILE SELECTION
  # =============================================================================

  @file-selection
  Scenario: Select file with Enter adds to input
    Given the file picker modal is open
    And "src/main.rs" is highlighted
    When I press "Enter"
    Then the modal should close
    And the input should contain "@src/main.rs"
    And "src/main.rs" should be added to context files

  Scenario: Select file with click
    Given the file picker modal is open
    When I click on "config.toml"
    Then the modal should close
    And the input should contain "@config.toml"

  Scenario: Select multiple files sequentially
    Given I have selected "file1.rs" via @ mention
    When I type " @" in the input
    Then the file picker should open again
    When I select "file2.rs"
    Then the input should contain "@file1.rs @file2.rs"
    And both files should be in context

  Scenario: Selecting already-added file
    Given "main.rs" is already in context
    And the file picker is open
    Then "main.rs" should show a checkmark or "added" indicator
    When I select "main.rs" again
    Then it should be removed from context (toggle behavior)
    And it should show "already added" message

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate file list with arrow keys
    Given the file picker modal is open
    When I press "Down Arrow"
    Then the next file should be highlighted
    When I press "Up Arrow"
    Then the previous file should be highlighted

  Scenario: Page through long file lists
    Given the file picker shows more than 10 files
    When I press "Page Down"
    Then the list should scroll down by a page
    When I press "Page Up"
    Then the list should scroll up by a page

  Scenario: Close with Escape
    Given the file picker modal is open
    When I press "Escape"
    Then the modal should close
    And no file should be selected
    And the "@" in input should remain

  Scenario: Cancel with Escape removes @ trigger
    Given I typed "@" which opened the file picker
    When I press "Escape"
    Then the modal should close
    And the "@" should be removed from input

  # =============================================================================
  # DIRECTORY NAVIGATION
  # =============================================================================

  @directory-nav
  Scenario: Enter directory to see contents
    Given the file picker modal is open
    And "src/" directory is highlighted
    When I press "Enter" or "Right Arrow"
    Then the file picker should show contents of "src/"
    And a breadcrumb should show current path

  Scenario: Go up to parent directory
    Given the file picker is showing "src/components/"
    When I press "Backspace" or "Left Arrow"
    Then the file picker should show "src/"
    When I press "Backspace" again
    Then the file picker should show the root

  # =============================================================================
  # CONTEXT FILE DISPLAY
  # =============================================================================

  @context-display
  Scenario: Show selected files count
    Given 3 files are in context
    And the file picker is open
    Then a "3 files selected" indicator should be visible

  Scenario: Quick access to recently used files
    Given I previously added "common_file.rs" to context
    When I open the file picker
    Then "common_file.rs" should appear in a "Recent" section

  # =============================================================================
  # STYLING
  # =============================================================================

  @modal-styling
  Scenario: File picker follows theme
    Given the theme is "tokyo-night"
    When the file picker modal is open
    Then colors should match Tokyo Night theme
    And highlighted files should use theme accent color

  Scenario: Visual feedback on hover
    Given the file picker modal is open
    When I hover over a file
    Then the file should have a hover highlight
