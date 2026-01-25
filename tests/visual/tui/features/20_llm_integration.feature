# Feature: LLM Integration
# Tests actual LLM message sending, streaming, and error handling

@llm @integration
Feature: LLM Integration
  As a user of the TUI application
  I want to send messages to the LLM
  So that I can get AI assistance

  Background:
    Given the TUI application is running
    And the AgentBridge is initialized with a valid API key

  @llm-send-message
  Scenario: Send message to LLM successfully
    Given I have typed "Hello, can you help me?"
    When I press "Enter"
    Then the user message should appear in the message area
    And the LLM should start processing
    And a response should appear after a few seconds
    And the response should be from role "Agent"

  @llm-streaming
  Scenario: LLM response streams in real-time
    Given I have sent a message to the LLM
    When the LLM starts responding
    Then text should appear incrementally
    And each chunk should be rendered as it arrives
    And the message area should auto-scroll to bottom
    And the cursor should remain in the input area

  @llm-thinking-blocks
  Scenario: Display thinking blocks when enabled
    Given thinking mode is enabled (Ctrl+T)
    When I send a message that triggers thinking
    Then a thinking block should appear above the response
    And the thinking block should be collapsible
    And the thinking content should be visually distinct

  @llm-error-handling
  Scenario: Handle LLM errors gracefully
    Given the API key is invalid
    When I send a message
    Then an error message should appear
    And the error should be styled as a system message
    And the error should include troubleshooting hints
    And the input should remain intact

  @llm-no-connection
  Scenario: Handle missing AgentBridge initialization
    Given the AgentBridge failed to initialize
    And no API key is configured
    When I try to send a message
    Then a system message should explain the issue
    And instructions for running 'tark auth' should be shown
    And the application should not crash

  @llm-interrupt
  Scenario: Interrupt LLM processing with Ctrl+C
    Given the LLM is currently generating a response
    When I press "Ctrl+C"
    Then the LLM processing should stop
    And a system message "Interrupted by user" should appear
    And I can send another message immediately

  @llm-provider-selection
  Scenario: LLM uses selected provider and model
    Given I have opened the provider picker with /model
    And I select "Anthropic" as the provider
    And I select "Claude 3.5 Sonnet" as the model
    When I send a message
    Then the request should be sent to Anthropic's API
    And the status bar should show "Claude 3.5 Sonnet ANTHROPIC"

  @llm-attachments
  Scenario: Send message with file attachments
    Given I have attached "README.md" using /attach
    And I have typed "Summarize this file"
    When I press "Enter"
    Then the file content should be included in the request
    And the LLM should have access to the file context
    And the attachment should clear after sending

  @llm-context-preservation
  Scenario: Maintain conversation context
    Given I have sent "What is Rust?"
    And received a response
    When I send "Tell me more about its memory safety"
    Then the LLM should have context of the previous exchange
    And the response should reference our earlier discussion
    And all messages should remain in the message area

  @llm-empty-response
  Scenario: Handle empty LLM response
    Given the LLM returns an empty response
    When the streaming completes
    Then a message should appear indicating no response
    Or the message area should show minimal whitespace
    And the application should remain stable
