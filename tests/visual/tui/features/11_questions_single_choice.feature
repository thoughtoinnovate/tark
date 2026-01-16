# Feature: Single Choice Questions
# Tests the radio-button style single-select questions from the agent
# Reference: screenshots/12-question-single-selected.png

@questions @single-choice
Feature: Single Choice Questions (Radio)
  As a user of the TUI application
  I want to answer single choice questions from the agent
  So that I can select exactly one option

  Background:
    Given the TUI application is running
    And the agent asks a single choice question

  # =============================================================================
  # QUESTION DISPLAY
  # =============================================================================

  @question-display
  Scenario: Display single choice question
    When the agent asks "Which framework do you prefer?"
    And provides options:
      | option      |
      | React       |
      | Vue         |
      | Angular     |
      | Svelte      |
    Then the question should be displayed in the message area
    And all options should be visible with radio buttons "○"
    And the question should indicate single selection

  Scenario: Radio buttons indicate single selection
    Given a single choice question is displayed
    Then options should have radio button style "○"
    And text should indicate "Select one"
    Or the visual style should imply single selection

  # =============================================================================
  # OPTION SELECTION
  # =============================================================================

  @option-selection
  Scenario: Select option with Space
    Given a single choice question is displayed
    And "React" option is focused
    When I press "Space"
    Then "React" should be selected "●"
    And other options should remain unselected "○"

  Scenario: Selection replaces previous
    Given "React" is already selected
    When I navigate to "Vue"
    And I press "Space"
    Then "Vue" should be selected "●"
    And "React" should be deselected "○"

  Scenario: Select option with Enter (immediate submit)
    Given a single choice question is displayed
    And "Angular" is focused
    When I press "Enter"
    Then "Angular" should be selected
    And the question should be submitted immediately

  Scenario: Select option with click
    Given a single choice question is displayed
    When I click on "Svelte"
    Then "Svelte" should be selected
    And the question may be submitted automatically

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate options with arrow keys
    Given a single choice question with 4 options is displayed
    When I press "Down Arrow"
    Then focus should move to the next option
    When I press "Up Arrow"
    Then focus should move to the previous option

  Scenario: Number keys for quick selection
    Given a single choice question is displayed
    When I press "1"
    Then the first option should be selected
    When I press "3"
    Then the third option should be selected
    And the first option should be deselected

  # =============================================================================
  # SUBMISSION
  # =============================================================================

  @submission
  Scenario: Submit selection with Enter
    Given I have selected "React"
    When I press "Enter"
    Then the question should be submitted
    And the selection should be sent to the agent

  Scenario: Cannot submit without selection
    Given no option is selected
    When I press "Enter" on the submit action
    Then a warning should appear "Please select an option"
    Or submit should be prevented

  Scenario: Display answered question state
    Given I submitted selection "React"
    Then the question should display "Answered: React"
    And the answer should be shown as a badge
    And the question should be non-interactive

  # =============================================================================
  # ANSWERED STATE DISPLAY
  # =============================================================================

  @answered-state
  Scenario: Answered question shows single selection
    Given a single choice question was answered with "Vue"
    Then the question should show "Vue" as the answer
    And the answer should be styled as a badge/tag
    And other options should not be visible

  Scenario: Answered question is collapsed
    Given a single choice question was answered
    Then the question should be in a compact/collapsed state
    And only the question text and answer should be visible

  # =============================================================================
  # STYLING
  # =============================================================================

  @styling
  Scenario: Question follows theme
    Given the theme is "Catppuccin Mocha"
    When a single choice question is displayed
    Then radio buttons should use theme colors
    And the selected option should use accent color
    And the focused option should have highlight color

  Scenario: Radio button states
    Given a single choice question is displayed
    Then unselected options should show "○" or "( )"
    And selected option should show "●" or "(•)"
    And the focused option should be visually highlighted
