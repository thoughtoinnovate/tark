# Feature: Multiple Choice Questions
# Tests the checkbox-style multi-select questions from the agent
# Reference: screenshots/12-question-single-selected.png

@questions @multiple-choice
Feature: Multiple Choice Questions (Checkbox)
  As a user of the TUI application
  I want to answer multiple choice questions from the agent
  So that I can provide multiple selections when needed

  Background:
    Given the TUI application is running
    And the agent asks a multiple choice question

  # =============================================================================
  # QUESTION DISPLAY
  # =============================================================================

  @question-display
  Scenario: Display multiple choice question
    When the agent asks "Which features do you want to include?"
    And provides options:
      | option              |
      | Authentication      |
      | Database            |
      | API endpoints       |
      | Unit tests          |
    Then the question should be displayed in the message area
    And all options should be visible with checkboxes "☐"
    And the question should use the theme's question color

  Scenario: Question has visual distinction
    Given a multiple choice question is displayed
    Then it should have a question icon "❓" or "?"
    And the question text should be prominent
    And options should be indented below the question

  Scenario: Question indicates multi-select capability
    Given a multiple choice question is displayed
    Then text should indicate "Select all that apply"
    Or checkboxes should indicate multiple selection is allowed

  # =============================================================================
  # OPTION SELECTION
  # =============================================================================

  @option-selection
  Scenario: Select option with Space
    Given a multiple choice question is displayed
    And "Authentication" option is focused
    When I press "Space"
    Then "Authentication" should be checked "☑"
    And other options should remain unchecked

  Scenario: Select multiple options
    Given a multiple choice question is displayed
    When I select "Authentication"
    And I navigate to "Database"
    And I select "Database"
    Then both "Authentication" and "Database" should be checked

  Scenario: Deselect option with Space
    Given "Authentication" is already selected
    When I focus on "Authentication"
    And I press "Space"
    Then "Authentication" should be unchecked "☐"

  Scenario: Select option with click
    Given a multiple choice question is displayed
    When I click on "Unit tests"
    Then "Unit tests" should be checked

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate options with arrow keys
    Given a multiple choice question with 4 options is displayed
    When I press "Down Arrow"
    Then focus should move to the next option
    When I press "Up Arrow"
    Then focus should move to the previous option

  Scenario: Navigate with vim keys
    Given a multiple choice question is displayed
    When I press "j"
    Then focus should move to the next option
    When I press "k"
    Then focus should move to the previous option

  Scenario: Wrap around navigation
    Given focus is on the last option
    When I press "Down Arrow"
    Then focus should wrap to the first option

  # =============================================================================
  # SUBMISSION
  # =============================================================================

  @submission
  Scenario: Submit selections with Enter
    Given I have selected "Authentication" and "Database"
    When I press "Enter"
    Then the question should be submitted
    And the selections should be sent to the agent
    And the question should show as answered

  Scenario: Submit with no selection shows warning
    Given no options are selected
    When I press "Enter"
    Then a warning should appear "Please select at least one option"
    Or the submit should be prevented

  Scenario: Display answered question state
    Given I submitted selections "Authentication, Database"
    Then the question should display "Answered: Authentication, Database"
    And the answer should be shown as badges or tags
    And the question should be non-interactive

  # =============================================================================
  # ANSWERED STATE DISPLAY
  # =============================================================================

  @answered-state
  Scenario: Answered question shows selections as badges
    Given a multiple choice question was answered with "Auth, DB"
    Then the question should show:
      | badge text     |
      | Auth           |
      | DB             |
    And badges should use the theme's context/tag colors

  Scenario: Answered question is visually distinct
    Given a multiple choice question was answered
    Then the question should have a different visual style
    And checkboxes should be replaced with the selection summary
    And the question should be dimmed or marked as complete

  Scenario: Cannot modify answered question
    Given a multiple choice question was answered
    When I try to focus on the question
    Then it should not be interactive
    And navigation should skip to the next unanswered element

  # =============================================================================
  # STYLING
  # =============================================================================

  @styling
  Scenario: Question follows theme
    Given the theme is "Nord"
    When a multiple choice question is displayed
    Then the question should use Nord's question color
    And checkboxes should use Nord's accent color when checked
    And the focused option should have Nord's highlight color

  Scenario: Checkbox states have visual distinction
    Given a multiple choice question is displayed
    Then unchecked boxes should show "☐" or "[ ]"
    And checked boxes should show "☑" or "[x]"
    And the focused option should be highlighted
