# Feature: Extended Thinking Display
# Priority: P2 (Extended)
# Tests thinking/reasoning display with tark_sim thinking scenario

@p2 @extended @thinking
Feature: Extended Thinking Display
  As a user of tark chat
  I want to see the AI's thinking process
  So that I can understand its reasoning

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows
    And TARK_SIM_SCENARIO is "thinking"

  # =============================================================================
  # THINKING DISPLAY
  # =============================================================================

  @display
  Scenario: Display thinking block before response
    When I type "Think carefully about this complex problem"
    And I press Enter
    Then I should see a thinking indicator
    And thinking content should be displayed
    And thinking should be visually distinct from response
    And final response should follow thinking
    And a recording is saved as "thinking_display.gif"
    And a snapshot is saved as "thinking_display.png"

  @display
  Scenario: Thinking block is collapsible
    When I type "Reason through this step by step"
    And I press Enter
    And I wait for response
    Then thinking block should have expand/collapse control
    When I collapse the thinking block
    Then only the response should be visible
    When I expand the thinking block
    Then thinking content should be visible again

  # =============================================================================
  # THINKING INDICATORS
  # =============================================================================

  @indicators
  Scenario: Show thinking progress
    When I type "Deep reasoning required"
    And I press Enter
    Then I should see "Thinking..." indicator
    And the indicator should animate or update
    And the indicator should disappear when thinking completes

  @indicators
  Scenario: Status bar shows thinking state
    When I type "Complex analysis"
    And I press Enter
    Then the status bar should indicate thinking mode
    And token count should track thinking tokens

  # =============================================================================
  # THINKING CONTENT
  # =============================================================================

  @content
  Scenario: Thinking content is readable
    When I type "Explain your reasoning"
    And I press Enter
    Then thinking content should be formatted clearly
    And thinking should use appropriate styling
    And thinking should be distinguishable from final answer

  @content
  Scenario: Long thinking is scrollable
    When I type "Very complex problem requiring extensive reasoning"
    And I press Enter
    And thinking content is longer than viewport
    Then thinking area should be scrollable
    And I should be able to read all thinking content
