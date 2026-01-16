# Feature: Provider Picker Modal
# Tests the provider selection modal functionality
# Reference: screenshots/06-provider-picker-modal.png

@modal @provider-picker
Feature: Provider Picker Modal
  As a user of the TUI application
  I want to select an LLM provider
  So that I can choose which AI service to use

  Background:
    Given the TUI application is running
    And the provider picker modal is not open

  # =============================================================================
  # MODAL OPENING
  # =============================================================================

  @modal-open
  Scenario: Open provider picker via status bar click
    When I click on the model selector in the status bar
    Then the provider picker modal should open
    And the modal should be centered on screen
    And the background should be dimmed

  Scenario: Open provider picker via /model command
    When I type "/model" in the input
    Then the provider picker modal should open

  Scenario: Modal has proper title
    Given the provider picker modal is open
    Then the modal title should be "Select Provider"
    And a close button "×" should be visible

  # =============================================================================
  # PROVIDER LIST
  # =============================================================================

  @provider-list
  Scenario: Display available providers
    Given the provider picker modal is open
    Then the following providers should be listed:
      | provider   | icon |
      | Anthropic  | 🔷   |
      | OpenAI     | 🟢   |
      | Google     | 🔴   |
      | Local      | 💻   |

  Scenario: Provider list is scrollable
    Given there are more providers than fit in the modal
    Then the provider list should be scrollable
    And a scrollbar should be visible

  Scenario: Highlight current provider
    Given the current provider is "Anthropic"
    And the provider picker modal is open
    Then "Anthropic" should be highlighted or marked as selected

  # =============================================================================
  # SEARCH/FILTER
  # =============================================================================

  @provider-search
  Scenario: Filter providers by typing
    Given the provider picker modal is open
    When I type "open"
    Then only "OpenAI" should be visible in the list
    And non-matching providers should be hidden

  Scenario: Clear search filter
    Given I have filtered to show only "OpenAI"
    When I clear the search input
    Then all providers should be visible again

  Scenario: No results message
    Given the provider picker modal is open
    When I type "xyz123"
    Then a "No providers found" message should be displayed

  # =============================================================================
  # SELECTION
  # =============================================================================

  @provider-selection
  Scenario: Select provider with Enter
    Given the provider picker modal is open
    And "OpenAI" is highlighted
    When I press "Enter"
    Then the model picker modal should open
    And the model picker should show models for "OpenAI"

  Scenario: Select provider with click
    Given the provider picker modal is open
    When I click on "Google"
    Then the model picker modal should open
    And the model picker should show models for "Google"

  Scenario: Provider selection chains to model picker
    Given the provider picker modal is open
    When I select provider "Anthropic"
    Then the provider picker should close
    And the model picker should open automatically
    And the model picker should list Anthropic models

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate providers with arrow keys
    Given the provider picker modal is open
    And "Anthropic" is highlighted
    When I press "Down Arrow"
    Then "OpenAI" should be highlighted
    When I press "Down Arrow"
    Then "Google" should be highlighted
    When I press "Up Arrow"
    Then "OpenAI" should be highlighted

  Scenario: Wrap around navigation
    Given the provider picker modal is open
    And the last provider is highlighted
    When I press "Down Arrow"
    Then the first provider should be highlighted

  Scenario: Close modal with Escape
    Given the provider picker modal is open
    When I press "Escape"
    Then the modal should close
    And no provider should be selected
    And the input area should have focus

  # =============================================================================
  # STYLING
  # =============================================================================

  @modal-styling
  Scenario: Modal follows current theme
    Given the theme is "catppuccin-mocha"
    When the provider picker modal is open
    Then the modal background should use theme colors
    And the text should use theme colors
    And the border should use theme colors

  Scenario: Hover state on provider items
    Given the provider picker modal is open
    When I hover over "Google"
    Then "Google" should have a hover highlight
    And the highlight should use theme's hover color
