# Feature: Multi-turn Conversation
# Priority: P1 (Core)
# Tests conversation memory and context handling

@p1 @core @memory
Feature: Multi-turn Conversation
  As a user of tark chat
  I want the AI to remember our conversation
  So that I can have coherent multi-turn discussions

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows
    And TARK_SIM_SCENARIO is "echo"

  # =============================================================================
  # CONTEXT RETENTION
  # =============================================================================

  @context
  Scenario: Remember user name across turns
    When I type "My name is Alice"
    And I press Enter
    And I wait for response
    When I type "What is my name?"
    And I press Enter
    Then the response should reference "Alice"
    And a recording is saved as "multi_turn_name.gif"

  @context
  Scenario: Remember topic across turns
    When I type "Let's discuss Rust programming"
    And I press Enter
    And I wait for response
    When I type "What are we discussing?"
    And I press Enter
    Then the response should reference "Rust"

  @context
  Scenario: Reference previous tool results
    Given TARK_SIM_SCENARIO is "tool"
    When I type "Search for main function"
    And I press Enter
    And I wait for response
    When I type "Tell me more about what you found"
    And I press Enter
    Then the response should reference the previous search results

  # =============================================================================
  # CONVERSATION HISTORY
  # =============================================================================

  @history
  Scenario: Scroll through conversation history
    Given I have sent 10 messages
    When I scroll up in the message area
    Then I should see earlier messages
    When I scroll down
    Then I should see recent messages

  @history
  Scenario: Long conversation maintains coherence
    Given I have sent 5 messages on a topic
    When I ask a follow-up question
    Then the response should maintain context
    And the response should be coherent with previous turns

  # =============================================================================
  # TOKEN TRACKING
  # =============================================================================

  @tokens
  Scenario: Token count increases with conversation
    When I type "First message"
    And I press Enter
    And I wait for response
    Then the token count should be greater than 0
    When I type "Second message"
    And I press Enter
    Then the token count should have increased

  @tokens
  Scenario: Display context usage in sidebar
    Given the sidebar is visible
    When I have sent several messages
    Then the context usage indicator should show percentage
    And the token count should be displayed
