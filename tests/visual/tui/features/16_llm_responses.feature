# Feature: LLM Response Display
# Priority: P1 (Core)
# Tests LLM response rendering in the new TUI with tark_sim provider

@p1 @core @llm
Feature: LLM Response Display
  As a user of the TUI application
  I want to see LLM responses displayed correctly
  So that I can interact with the AI assistant effectively

  Background:
    Given the TUI application is running
    And the provider is "tark_sim"
    And the input area has focus

  # =============================================================================
  # BASIC RESPONSE DISPLAY
  # =============================================================================

  @echo
  Scenario: Display echo response from tark_sim
    Given TARK_SIM_SCENARIO is "echo"
    When I send message "Hello, can you help me?"
    Then I should see a user message "Hello, can you help me?"
    And I should see an agent response containing "Echo: Hello, can you help me?"
    And the response should be marked as complete

  @echo
  Scenario: Display multiple messages in conversation
    Given TARK_SIM_SCENARIO is "echo"
    When I send message "First question"
    And I wait for response
    And I send message "Second question"
    And I wait for response
    Then I should see 2 user messages
    And I should see 2 agent responses
    And messages should be in chronological order

  @echo
  Scenario: Empty message is not sent
    When I press Enter without typing
    Then no new message should appear
    And the input area should remain focused

  # =============================================================================
  # STREAMING RESPONSES
  # =============================================================================

  @streaming
  Scenario: Display streaming response progressively
    Given TARK_SIM_SCENARIO is "streaming"
    When I send message "Explain Rust closures"
    Then I should see a processing indicator
    And text should appear incrementally in the message area
    And the final response should be complete

  @streaming
  Scenario: Show spinner during streaming
    Given TARK_SIM_SCENARIO is "streaming"
    When I send message "Generate some text"
    Then I should see a spinner or "..." indicator
    And the spinner should disappear when streaming completes

  @streaming
  Scenario: Interrupt streaming with Escape
    Given TARK_SIM_SCENARIO is "streaming"
    When I send message "Stream a very long response"
    And I wait 500ms
    And I press "Escape"
    Then the streaming should stop
    And partial response should be visible
    And the input area should regain focus

  # =============================================================================
  # TOOL INVOCATION DISPLAY
  # =============================================================================

  @tools
  Scenario: Display tool call in message area
    Given TARK_SIM_SCENARIO is "tool"
    When I send message "Search for TODO comments"
    Then I should see a tool execution indicator
    And I should see the tool name in the message
    And the tool result should be displayed

  @tools
  Scenario: Display tool result formatting
    Given TARK_SIM_SCENARIO is "tool"
    When I send message "Read the README file"
    Then tool results should be visually distinct
    And tool output should be in a code block style

  @tools
  Scenario: Display multiple tool calls
    Given TARK_SIM_SCENARIO is "multi_tool"
    When I send message "Search and then read a file"
    Then I should see multiple tool executions
    And each tool result should be displayed in order

  # =============================================================================
  # THINKING MODE
  # =============================================================================

  @thinking
  Scenario: Display thinking content when enabled
    Given TARK_SIM_SCENARIO is "thinking"
    And thinking mode is enabled
    When I send message "Solve this complex problem"
    Then I should see a thinking block
    And the thinking content should be collapsible
    And the final response should follow the thinking

  @thinking
  Scenario: Hide thinking content when disabled
    Given TARK_SIM_SCENARIO is "thinking"
    And thinking mode is disabled
    When I send message "Solve this problem"
    Then I should not see a thinking block
    And I should only see the final response

  @thinking
  Scenario: Toggle thinking visibility
    Given TARK_SIM_SCENARIO is "thinking"
    And thinking mode is enabled
    When I send message "Think about this"
    And I wait for response
    And I press "Ctrl+T"
    Then thinking blocks should be hidden
    When I press "Ctrl+T" again
    Then thinking blocks should be visible

  # =============================================================================
  # ERROR HANDLING
  # =============================================================================

  @errors
  Scenario: Display timeout error
    Given TARK_SIM_SCENARIO is "error_timeout"
    When I send message "This will timeout"
    Then I should see an error message
    And the error should mention "timeout"
    And the input area should be re-enabled

  @errors
  Scenario: Display rate limit error
    Given TARK_SIM_SCENARIO is "error_rate_limit"
    When I send message "This will hit rate limit"
    Then I should see an error message
    And the error should mention "rate limit"

  @errors
  Scenario: Display context length exceeded error
    Given TARK_SIM_SCENARIO is "error_context_exceeded"
    When I send message "This exceeds context"
    Then I should see an error message
    And the error should suggest "/clear" or "new conversation"

  @errors
  Scenario: Display partial response on connection loss
    Given TARK_SIM_SCENARIO is "error_partial"
    When I send message "This will fail mid-response"
    Then I should see partial response if any
    And I should see a connection error indicator

  # =============================================================================
  # STATUS BAR UPDATES
  # =============================================================================

  @status
  Scenario: Status bar shows processing state
    Given TARK_SIM_SCENARIO is "streaming"
    When I send message "Generate response"
    Then the status bar should show processing indicator
    And when response completes the indicator should clear

  @status
  Scenario: Status bar shows provider name
    Given the provider is "tark_sim"
    Then the status bar should display "tark_sim"

  # =============================================================================
  # MESSAGE FORMATTING
  # =============================================================================

  @formatting
  Scenario: User messages have distinct styling
    When I send message "Test message"
    Then user messages should have user icon or indicator
    And user messages should be visually distinct from agent messages

  @formatting
  Scenario: Agent messages have distinct styling
    Given TARK_SIM_SCENARIO is "echo"
    When I send message "Test"
    And I wait for response
    Then agent messages should have agent icon or indicator
    And agent messages should be visually distinct from user messages

  @formatting
  Scenario: Code blocks in responses are formatted
    Given TARK_SIM_SCENARIO is "echo"
    When I send message "Show me code: ```rust\nfn main() {}\n```"
    And I wait for response
    Then code blocks should be rendered with syntax highlighting style
