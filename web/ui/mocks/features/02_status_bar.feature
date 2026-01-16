# Feature: Status Bar
# Tests the status bar components and interactions
# Reference: screenshots/compact-status-bar.png, screenshots/agent-working-indicator.png

@status-bar @core
Feature: Status Bar
  As a user of the TUI application
  I want to see and interact with the status bar
  So that I can control agent modes, view status, and access settings

  Background:
    Given the TUI application is running
    And the status bar is visible at the bottom of the terminal

  # =============================================================================
  # AGENT MODE SELECTOR
  # =============================================================================

  @agent-mode
  Scenario: Display current agent mode
    Then the status bar should show the current agent mode
    And the agent mode should be one of "Build", "Plan", or "Ask"
    And the mode should have an associated icon

  Scenario Outline: Switch agent mode via dropdown
    Given the current agent mode is "<current_mode>"
    When I click on the agent mode selector
    Then a dropdown should appear with options "Build", "Plan", "Ask"
    When I select "<new_mode>" from the dropdown
    Then the agent mode should change to "<new_mode>"
    And the dropdown should close

    Examples:
      | current_mode | new_mode |
      | Build        | Plan     |
      | Plan         | Ask      |
      | Ask          | Build    |

  Scenario: Agent mode dropdown keyboard navigation
    When I click on the agent mode selector
    Then the dropdown should be visible
    When I press "Down Arrow"
    Then the next option should be highlighted
    When I press "Up Arrow"
    Then the previous option should be highlighted
    When I press "Enter"
    Then the highlighted option should be selected
    And the dropdown should close

  Scenario: Close agent mode dropdown with Escape
    Given the agent mode dropdown is open
    When I press "Escape"
    Then the dropdown should close
    And the mode should remain unchanged

  # =============================================================================
  # BUILD MODE SELECTOR
  # =============================================================================

  @build-mode
  Scenario: Display current build mode
    Given the agent mode is "Build"
    Then the status bar should show the build mode
    And the build mode should be one of "Careful", "Balanced", or "Manual"

  Scenario: Build mode only visible in Build agent mode
    Given the agent mode is "Plan"
    Then the build mode selector should not be visible
    When I switch agent mode to "Build"
    Then the build mode selector should become visible

  Scenario Outline: Switch build mode
    Given the agent mode is "Build"
    And the current build mode is "<current_mode>"
    When I click on the build mode selector
    And I select "<new_mode>" from the dropdown
    Then the build mode should change to "<new_mode>"

    Examples:
      | current_mode | new_mode |
      | Careful      | Balanced |
      | Balanced     | Manual   |
      | Manual       | Careful  |

  # =============================================================================
  # MODEL/PROVIDER SELECTOR
  # =============================================================================

  @model-selector
  Scenario: Display current model and provider
    Then the status bar should show the current model name
    And the status bar should show the current provider name
    And a chevron icon should indicate it's clickable

  Scenario: Click model selector opens provider picker
    When I click on the model selector in the status bar
    Then the provider picker modal should open
    And the modal should list available providers

  Scenario: Model selector shows connection status
    Given the LLM is connected
    Then the model selector should show a connected indicator
    When the LLM connection is lost
    Then the model selector should show a disconnected indicator

  # =============================================================================
  # THINKING MODE TOGGLE
  # =============================================================================

  @thinking-toggle
  Scenario: Display thinking mode state
    Then the status bar should show the thinking toggle icon "🧠"
    And the icon should indicate whether thinking mode is enabled

  Scenario: Toggle thinking mode on
    Given thinking mode is disabled
    When I click on the thinking toggle
    Then thinking mode should be enabled
    And the brain icon should have amber/mustard color

  Scenario: Toggle thinking mode off
    Given thinking mode is enabled
    When I click on the thinking toggle
    Then thinking mode should be disabled
    And the brain icon should be dimmed/inactive color

  Scenario: Thinking toggle keyboard shortcut
    Given thinking mode is disabled
    When I press "Ctrl+T"
    Then thinking mode should be enabled
    When I press "Ctrl+T" again
    Then thinking mode should be disabled

  # =============================================================================
  # TASK QUEUE INDICATOR
  # =============================================================================

  @queue-indicator
  Scenario: Display task queue count
    Given there are 5 tasks in the queue
    Then the status bar should show a queue icon "📋"
    And the queue count should display "5"

  Scenario: Queue indicator updates dynamically
    Given there are 3 tasks in the queue
    When a new task is added to the queue
    Then the queue count should update to "4"
    When a task completes
    Then the queue count should update to "3"

  Scenario: Queue indicator hidden when empty
    Given there are 0 tasks in the queue
    Then the queue indicator should be hidden or dimmed

  # =============================================================================
  # AGENT WORKING INDICATOR
  # =============================================================================

  @working-indicator
  Scenario: Show working indicator when agent is processing
    Given the agent is processing a request
    Then a green blinking dot should be visible in the status bar
    And "Working..." text should be displayed

  Scenario: Hide working indicator when agent is idle
    Given the agent is idle
    Then the working indicator should not be visible

  Scenario: Working indicator animation
    Given the agent is processing a request
    Then the green dot should have a pulsing/ping animation
    And the animation should be smooth and not jarring

  # =============================================================================
  # HELP BUTTON
  # =============================================================================

  @help-button
  Scenario: Display help button on far right
    Then a help button "?" should be visible on the far right of the status bar
    And the button should be monochrome and follow the theme

  Scenario: Click help button opens help modal
    When I click on the help button
    Then the help modal should open
    And the modal should display keyboard shortcuts

  Scenario: Help button keyboard shortcut
    When I press "?"
    Then the help modal should open

  # =============================================================================
  # RESPONSIVE LAYOUT
  # =============================================================================

  @responsive
  Scenario: Status bar adapts to narrow terminals
    Given the terminal width is 80 columns
    Then all status bar elements should be visible
    And elements should be appropriately sized

  Scenario: Status bar elements maintain alignment
    Then the agent mode should be aligned to the left
    And the model selector should be in the center-left area
    And the working indicator should be in the center
    And the help button should be aligned to the far right
