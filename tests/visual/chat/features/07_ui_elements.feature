# Feature: UI Elements and Navigation
# Priority: P2 (Extended)
# Tests UI components and navigation

@p2 @extended @ui
Feature: UI Elements and Navigation
  As a user of tark chat
  I want the UI to be well-organized and navigable
  So that I can efficiently use the application

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows

  # =============================================================================
  # STATUS BAR
  # =============================================================================

  @statusbar
  Scenario: Status bar displays all information
    Then the status bar should show:
      | Element | Value |
      | provider | tark_sim |
      | mode | Build or Ask |
      | tokens | numeric count |
    And a snapshot is saved as "ui_statusbar.png"

  @statusbar
  Scenario: Status bar updates on mode change
    When I type "/ask"
    And I press Enter
    Then the status bar should show mode "Ask"
    When I type "/build"
    And I press Enter
    Then the status bar should show mode "Build"

  # =============================================================================
  # SIDEBAR
  # =============================================================================

  @sidebar
  Scenario: Toggle sidebar visibility
    When I press Tab to focus sidebar
    Then the sidebar should be highlighted
    When I press Tab again
    Then focus should return to input

  @sidebar
  Scenario: Sidebar shows context information
    Given the sidebar is visible
    Then I should see session info section
    And I should see context usage section
    And I should see modified files section

  # =============================================================================
  # MODALS
  # =============================================================================

  @modal
  Scenario: Provider picker modal
    When I type "/provider"
    And I press Enter
    Then the provider picker modal should appear
    And available providers should be listed
    And I should be able to select a provider
    And a snapshot is saved as "ui_provider_modal.png"

  @modal
  Scenario: Model picker modal
    When I type "/model"
    And I press Enter
    Then the model picker modal should appear
    And available models should be listed
    And I should be able to select a model

  @modal
  Scenario: Close modal with Escape
    Given a modal is open
    When I press Escape
    Then the modal should close
    And focus should return to input

  # =============================================================================
  # INPUT AREA
  # =============================================================================

  @input
  Scenario: Input area shows mode indicator
    Then the input area should show current mode
    And mode should change between INSERT and NORMAL

  @input
  Scenario: Multi-line input support
    When I type "Line 1"
    And I press Shift+Enter
    And I type "Line 2"
    Then the input should show both lines
    When I press Enter
    Then both lines should be sent as one message

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard
  Scenario: Vim-style navigation in normal mode
    When I press Escape to enter normal mode
    Then I should be in normal mode
    When I press "i"
    Then I should be in insert mode
    When I press Escape
    Then I should be in normal mode again

  @keyboard
  Scenario: Command completion
    When I type "/cl"
    Then I should see command suggestions
    And suggestions should include "/clear"
    When I press Tab
    Then "/clear" should be completed

  # =============================================================================
  # THEMING
  # =============================================================================

  @theme
  Scenario: Theme is applied correctly
    Then the UI should use consistent colors
    And borders should be visible
    And text should be readable
