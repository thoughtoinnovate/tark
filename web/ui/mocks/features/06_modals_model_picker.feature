# Feature: Model Picker Modal
# Tests the model selection modal functionality
# Reference: screenshots/07-model-picker-modal.png

@modal @model-picker
Feature: Model Picker Modal
  As a user of the TUI application
  I want to select an AI model
  So that I can use my preferred model for the conversation

  Background:
    Given the TUI application is running
    And a provider "Anthropic" has been selected

  # =============================================================================
  # MODAL OPENING
  # =============================================================================

  @modal-open
  Scenario: Model picker opens after provider selection
    Given I selected "Anthropic" in the provider picker
    Then the model picker modal should open automatically
    And the modal title should be "Select Model"
    And the subtitle should show "Anthropic"

  Scenario: Model picker has back button
    Given the model picker modal is open
    Then a back button "←" should be visible
    When I click the back button
    Then the provider picker should open
    And the model picker should close

  # =============================================================================
  # MODEL LIST
  # =============================================================================

  @model-list
  Scenario: Display models for selected provider
    Given the model picker is open for provider "Anthropic"
    Then the following models should be listed:
      | model                | description        |
      | Claude 3.5 Sonnet    | Latest, balanced   |
      | Claude 3 Opus        | Most capable       |
      | Claude 3 Haiku       | Fast, efficient    |

  Scenario: Display models for OpenAI
    Given the model picker is open for provider "OpenAI"
    Then the following models should be listed:
      | model          | description       |
      | GPT-4 Turbo    | Most capable      |
      | GPT-4          | High quality      |
      | GPT-3.5 Turbo  | Fast, economical  |

  Scenario: Highlight current model
    Given the current model is "Claude 3.5 Sonnet"
    And the model picker is open for "Anthropic"
    Then "Claude 3.5 Sonnet" should be highlighted as current

  # =============================================================================
  # SEARCH/FILTER
  # =============================================================================

  @model-search
  Scenario: Filter models by typing
    Given the model picker is open for "Anthropic"
    When I type "opus"
    Then only "Claude 3 Opus" should be visible
    And other models should be hidden

  Scenario: Search is case-insensitive
    Given the model picker is open for "OpenAI"
    When I type "GPT"
    Then all GPT models should be visible
    When I clear and type "gpt"
    Then the same GPT models should still be visible

  Scenario: No matching models message
    Given the model picker is open
    When I type "nonexistent"
    Then a "No models found" message should be displayed

  # =============================================================================
  # SELECTION
  # =============================================================================

  @model-selection
  Scenario: Select model with Enter
    Given the model picker is open
    And "Claude 3 Opus" is highlighted
    When I press "Enter"
    Then "Claude 3 Opus" should be selected as the current model
    And the modal should close
    And the status bar should show "Claude 3 Opus"

  Scenario: Select model with click
    Given the model picker is open
    When I click on "GPT-4 Turbo"
    Then "GPT-4 Turbo" should be selected
    And the modal should close
    And the status bar should update

  Scenario: Selection updates both model and provider
    Given the current model is "Claude 3.5 Sonnet" from "Anthropic"
    When I select provider "OpenAI" then model "GPT-4"
    Then the status bar should show "GPT-4"
    And the status bar should show provider "OpenAI"

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate models with arrow keys
    Given the model picker is open
    When I press "Down Arrow"
    Then the next model should be highlighted
    When I press "Up Arrow"
    Then the previous model should be highlighted

  Scenario: Go back to provider picker with Backspace
    Given the model picker is open
    When I press "Backspace" with empty search
    Then the provider picker should open
    And the model picker should close

  Scenario: Close entire flow with Escape
    Given the model picker is open
    When I press "Escape"
    Then the model picker should close
    And no selection change should occur
    And the input area should have focus

  # =============================================================================
  # MODEL DETAILS
  # =============================================================================

  @model-details
  Scenario: Display model description
    Given the model picker is open
    Then each model should show a brief description
    And the description should help users understand the model

  Scenario: Display model capabilities badges
    Given the model picker is open for "Anthropic"
    Then "Claude 3 Opus" should show "Most capable" badge
    And "Claude 3 Haiku" should show "Fast" badge

  # =============================================================================
  # STYLING
  # =============================================================================

  @modal-styling
  Scenario: Model picker follows theme
    Given the theme is "nord"
    When the model picker is open
    Then all colors should match the Nord theme
    And selected/highlighted items should use theme accent color

  Scenario: Visual distinction between available and unavailable models
    Given the model picker is open
    And some models are unavailable
    Then unavailable models should be visually dimmed
    And unavailable models should not be selectable
