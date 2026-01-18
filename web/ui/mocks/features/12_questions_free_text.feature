# Feature: Free Text Questions
# Tests text input questions from the agent

@questions @free-text
Feature: Free Text Questions
  As a user of the TUI application
  I want to answer free text questions from the agent
  So that I can provide custom text input when needed

  Background:
    Given the TUI application is running
    And the agent asks a free text question

  # =============================================================================
  # QUESTION DISPLAY
  # =============================================================================

  @question-display
  Scenario: Display free text question
    When the agent asks "What is your project name?"
    Then the question should be displayed in the message area
    And a text input field should be visible
    And the input should have focus

  Scenario: Question with placeholder text
    When the agent asks "Enter your API key:"
    And provides placeholder "sk-..."
    Then the input should show placeholder "sk-..."
    When I start typing
    Then the placeholder should disappear

  Scenario: Question with validation hint
    When the agent asks "Enter a valid email address:"
    Then a hint should be displayed below the input
    And the hint should indicate the expected format

  # =============================================================================
  # TEXT INPUT
  # =============================================================================

  @text-input
  Scenario: Type text in the answer field
    Given a free text question is displayed
    When I type "my-awesome-project"
    Then the input should show "my-awesome-project"
    And the cursor should be visible

  Scenario: Edit typed text
    Given I have typed "my-proect"
    When I navigate to fix the typo
    And I insert "j" to make "my-project"
    Then the input should show "my-project"

  Scenario: Clear input
    Given I have typed "some text"
    When I press "Ctrl+U" or select all and delete
    Then the input should be empty

  # =============================================================================
  # SUBMISSION
  # =============================================================================

  @submission
  Scenario: Submit answer with Enter
    Given I have typed "MyProject"
    When I press "Enter"
    Then the answer should be submitted
    And the answer should be sent to the agent
    And the question should show as answered

  Scenario: Submit empty answer shows warning
    Given the input is empty
    When I press "Enter"
    Then a warning should appear "Please enter a response"
    Or a default value should be used if specified

  Scenario: Display answered question state
    Given I submitted answer "my-api-key-123"
    Then the question should display "Answered: my-api-key-123"
    And the input field should be replaced with the answer text
    And the question should be non-interactive

  # =============================================================================
  # ANSWERED STATE DISPLAY
  # =============================================================================

  @answered-state
  Scenario: Answered free text shows the response
    Given a free text question was answered with "my-project-name"
    Then the question should show the answer text
    And the answer should be styled distinctly
    And the input field should no longer be visible

  Scenario: Long answer is truncated with ellipsis
    Given a free text question was answered with a very long response
    Then the answer should be truncated if too long
    And "..." should indicate truncation
    And hovering or focusing may show full text

  # =============================================================================
  # VALIDATION
  # =============================================================================

  @validation
  Scenario: Required field validation
    Given the question requires an answer
    And the input is empty
    When I try to submit
    Then the input should show an error state
    And an error message should appear

  Scenario: Pattern validation
    Given the question expects an email format
    When I type "not-an-email"
    And I try to submit
    Then a validation error should appear
    And the input should show error styling

  Scenario: Valid input clears error
    Given the input shows a validation error
    When I correct the input to a valid value
    Then the error should disappear
    And the input should return to normal styling

  # =============================================================================
  # KEYBOARD SHORTCUTS
  # =============================================================================

  @keyboard
  Scenario: Cancel input with Escape
    Given I have typed partial text
    When I press "Escape"
    Then the input should be cleared
    Or focus should leave the question

  Scenario: Navigate to previous question with Shift+Tab
    Given there are multiple questions
    And I am answering a free text question
    When I press "Shift+Tab"
    Then focus should move to the previous question

  # =============================================================================
  # STYLING
  # =============================================================================

  @styling
  Scenario: Question follows theme
    Given the theme is "Gruvbox Dark"
    When a free text question is displayed
    Then the input should use Gruvbox colors
    And the border should use theme border color
    And focus should show theme accent color

  Scenario: Input has appropriate styling
    Given a free text question is displayed
    Then the input should have a visible border
    And the input should have appropriate padding
    And the cursor should be clearly visible
