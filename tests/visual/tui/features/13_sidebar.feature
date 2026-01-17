# Feature: Sidebar Panel
# Tests the collapsible sidebar functionality
# Reference: screenshots/11-sidebar-panel.png

@sidebar
Feature: Sidebar Panel
  As a user of the TUI application
  I want to use the sidebar panel
  So that I can view session info, context files, tasks, and git changes

  Background:
    Given the TUI application is running
    And the sidebar is visible

  # =============================================================================
  # SIDEBAR TOGGLE
  # =============================================================================

  @sidebar-toggle
  Scenario: Toggle sidebar visibility
    Given the sidebar is expanded
    When I press the sidebar toggle key "Ctrl+B"
    Then the sidebar should collapse
    And the terminal should expand to full width

  Scenario: Expand collapsed sidebar
    Given the sidebar is collapsed
    When I press the sidebar toggle key "Ctrl+B"
    Then the sidebar should expand
    And the terminal should resize accordingly

  Scenario: Sidebar toggle button visible
    Given the sidebar is expanded
    Then a collapse button "«" should be visible
    Given the sidebar is collapsed
    Then an expand button "»" should be visible

  # =============================================================================
  # SESSION PANEL
  # =============================================================================

  @session-panel
  Scenario: Display session information
    Given the sidebar is expanded
    Then the "Session" panel should be visible
    And it should show session ID or name
    And it should show session duration or start time

  Scenario: Session panel is collapsible
    Given the "Session" panel is expanded
    When I click on the "Session" panel header
    Then the panel should collapse
    When I click on the header again
    Then the panel should expand

  # =============================================================================
  # CONTEXT FILES PANEL
  # =============================================================================

  @context-panel
  Scenario: Display context files
    Given files "main.rs" and "config.toml" are in context
    Then the "Context" panel should list both files
    And each file should show an icon based on type
    And each file should have a remove button "×"

  Scenario: Remove file from context via sidebar
    Given "main.rs" is in context
    When I click the "×" button next to "main.rs"
    Then "main.rs" should be removed from context
    And the @mention should be removed from input

  Scenario: Click file to preview
    Given "config.toml" is in context
    When I click on "config.toml" in the sidebar
    Then a preview of the file should be shown
    And the file path should be copied to clipboard

  Scenario: Empty context state
    Given no files are in context
    Then the "Context" panel should show "No files added"
    And it may show hint text "Type @ to add files"

  # =============================================================================
  # TASKS PANEL
  # =============================================================================

  @tasks-panel
  Scenario: Display active tasks
    Given there are tasks in the queue
    Then the "Tasks" panel should show the task list
    And active task should be highlighted
    And task status should be visible (pending, running, completed)

  Scenario: Task status indicators
    Given the following tasks exist:
      | task              | status    |
      | Reading files     | completed |
      | Analyzing code    | running   |
      | Generating output | pending   |
    Then each task should show appropriate status icon
    And "completed" should show "✓"
    And "running" should show spinner or "⟳"
    And "pending" should show "○"

  Scenario: Empty tasks state
    Given there are no tasks
    Then the "Tasks" panel should show "No active tasks"

  # =============================================================================
  # GIT CHANGES PANEL
  # =============================================================================

  @git-panel
  Scenario: Display git changes
    Given there are uncommitted git changes
    Then the "Git Changes" panel should show modified files
    And files should be categorized by status

  Scenario: Git file status indicators
    Given the following git changes:
      | file        | status   |
      | main.rs     | modified |
      | new_file.rs | added    |
      | old_file.rs | deleted  |
    Then modified files should show "M" indicator in yellow
    And added files should show "A" indicator in green
    And deleted files should show "D" indicator in red

  Scenario: Git branch display
    Given the current branch is "feature/new-ui"
    Then the "Git" panel header should show the branch name
    And a branch icon "⎇" should be visible

  Scenario: No git changes state
    Given there are no uncommitted changes
    Then the "Git Changes" panel should show "No changes"
    And show "Working tree clean"

  # =============================================================================
  # PANEL NAVIGATION
  # =============================================================================

  @panel-navigation
  Scenario: Navigate between panels
    Given the sidebar has focus
    When I press "Tab"
    Then focus should move to the next panel
    When I press "Shift+Tab"
    Then focus should move to the previous panel

  Scenario: Collapse/expand with Enter
    Given focus is on a panel header
    When I press "Enter"
    Then the panel should toggle collapsed/expanded state

  Scenario: Navigate items within panel
    Given focus is on the "Context" panel
    And there are multiple files listed
    When I press "Down Arrow"
    Then focus should move to the next file
    When I press "Up Arrow"
    Then focus should move to the previous file

  # =============================================================================
  # STYLING
  # =============================================================================

  @styling
  Scenario: Sidebar follows theme
    Given the theme is "Tokyo Night"
    Then the sidebar should use Tokyo Night colors
    And panel headers should use theme accent color
    And borders should use theme border color

  Scenario: Panel headers are visually distinct
    Given the sidebar is visible
    Then panel headers should be clearly distinguishable
    And headers should show expand/collapse indicator "▼" or "▶"
    And headers should have icons for each panel type

  Scenario: Sidebar has proper borders
    Given the sidebar is visible
    Then the sidebar should have a left border separating it from terminal
    And panels should have borders between them

  # =============================================================================
  # RESPONSIVE BEHAVIOR
  # =============================================================================

  @responsive
  Scenario: Sidebar adapts to terminal height
    Given the terminal is resized to a smaller height
    Then panels should still be accessible
    And panels may auto-collapse to fit
    And scrolling should be available if needed

  Scenario: Sidebar width is appropriate
    Given the sidebar is expanded
    Then the sidebar should not take more than 30% of terminal width
    And content should not be clipped
