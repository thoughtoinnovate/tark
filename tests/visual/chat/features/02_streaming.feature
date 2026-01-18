# Feature: Streaming Response Display
# Priority: P1 (Core)
# Tests streaming response behavior with tark_sim streaming scenario

@p1 @core @streaming
Feature: Streaming Response Display
  As a user of tark chat
  I want to see responses appear progressively
  So that I know the AI is working and can read as it types

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows
    And TARK_SIM_SCENARIO is "streaming"

  # =============================================================================
  # STREAMING BEHAVIOR
  # =============================================================================

  @visual
  Scenario: Display streaming response progressively
    When I type "Explain Rust closures in detail"
    And I press Enter
    Then I should see the processing indicator
    And text should appear incrementally
    And the final response should be complete
    And a recording is saved as "streaming_response.gif"
    And a snapshot is saved as "streaming_final.png"

  @visual
  Scenario: Spinner shows during streaming
    When I type "Stream a long response"
    And I press Enter
    Then I should see a spinner or progress indicator
    And the spinner should disappear when streaming completes

  @visual
  Scenario: Streaming updates token count progressively
    When I type "Generate some text"
    And I press Enter
    Then the token count in status bar should increase
    And the final token count should reflect total tokens used

  # =============================================================================
  # INTERRUPTION
  # =============================================================================

  @interruption
  Scenario: Interrupt streaming with Escape
    When I type "Stream a very long response"
    And I press Enter
    And I wait 1 second
    And I press Escape
    Then the streaming should stop
    And partial response should be visible
    And the input area should regain focus

  @interruption
  Scenario: Interrupt streaming with Ctrl+C
    When I type "Stream continuously"
    And I press Enter
    And I wait 1 second
    And I press Ctrl+C
    Then the streaming should stop
    And an interruption message should appear
