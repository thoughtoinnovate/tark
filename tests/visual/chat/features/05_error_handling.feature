# Feature: Error Handling
# Priority: P2 (Extended)
# Tests error scenarios and recovery with tark_sim error scenarios

@p2 @extended @errors
Feature: Error Handling
  As a user of tark chat
  I want errors to be handled gracefully
  So that I can understand what went wrong and recover

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows

  # =============================================================================
  # TIMEOUT ERRORS
  # =============================================================================

  @timeout
  Scenario: Handle timeout error gracefully
    Given TARK_SIM_SCENARIO is "error_timeout"
    When I type "Send a message"
    And I press Enter
    Then I should see a timeout error message
    And the UI should remain responsive
    And I should be able to retry
    And a recording is saved as "error_timeout.gif"
    And a snapshot is saved as "error_timeout.png"

  # =============================================================================
  # RATE LIMIT ERRORS
  # =============================================================================

  @ratelimit
  Scenario: Handle rate limit error
    Given TARK_SIM_SCENARIO is "error_rate_limit"
    When I type "Send a message"
    And I press Enter
    Then I should see a rate limit error message
    And the error should indicate when to retry
    And I should be able to continue after waiting
    And a snapshot is saved as "error_ratelimit.png"

  # =============================================================================
  # CONTEXT LENGTH ERRORS
  # =============================================================================

  @context
  Scenario: Handle context length exceeded error
    Given TARK_SIM_SCENARIO is "error_context_exceeded"
    When I type "Send a very long message"
    And I press Enter
    Then I should see a context length error
    And the error should suggest truncating or clearing history
    And a snapshot is saved as "error_context.png"

  # =============================================================================
  # MALFORMED RESPONSE ERRORS
  # =============================================================================

  @malformed
  Scenario: Handle malformed tool call gracefully
    Given TARK_SIM_SCENARIO is "error_malformed"
    When I type "Execute a tool"
    And I press Enter
    Then I should see an error about malformed response
    And the conversation should continue
    And I should be able to send another message

  # =============================================================================
  # PARTIAL RESPONSE ERRORS
  # =============================================================================

  @partial
  Scenario: Handle partial response
    Given TARK_SIM_SCENARIO is "error_partial"
    When I type "Send a message"
    And I press Enter
    Then I should see a partial response
    And an error indicator should show
    And I should be able to regenerate

  # =============================================================================
  # CONTENT FILTERED ERRORS
  # =============================================================================

  @filtered
  Scenario: Handle content filtered response
    Given TARK_SIM_SCENARIO is "error_filtered"
    When I type "Send filtered content"
    And I press Enter
    Then I should see a content filtered message
    And the message should explain the filtering

  # =============================================================================
  # RECOVERY
  # =============================================================================

  @recovery
  Scenario: Recover from error and continue
    Given TARK_SIM_SCENARIO is "error_timeout"
    When I type "Trigger error"
    And I press Enter
    And I see the error message
    When TARK_SIM_SCENARIO changes to "echo"
    And I type "Normal message"
    And I press Enter
    Then I should receive a normal response
