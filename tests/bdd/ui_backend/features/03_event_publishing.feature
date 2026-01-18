@ui_backend @events
Feature: Event Publishing and Async Updates
  As a frontend
  I want to receive async events from the backend
  So that I can update the UI in real-time

  Background:
    Given the AppService is initialized
    And an event subscriber is listening

  # ========================================================================
  # LLM EVENTS
  # ========================================================================

  Scenario: LLM starts processing
    Given the LLM is connected
    When I send a message "Hello"
    Then an "LlmStarted" event should be received

  Scenario: LLM sends text chunks (streaming)
    Given the LLM is streaming a response
    When the LLM sends a chunk "Hello"
    Then an "LlmTextChunk" event should be received with "Hello"
    When the LLM sends a chunk " World"
    Then an "LlmTextChunk" event should be received with " World"

  Scenario: LLM sends thinking chunks
    Given the LLM is processing with thinking enabled
    When the LLM sends a thinking chunk "Analyzing the question..."
    Then an "LlmThinkingChunk" event should be received
    And the thinking content should be "Analyzing the question..."

  Scenario: LLM completes successfully
    Given the LLM is processing a message
    When the LLM completes with response "Done"
    Then an "LlmCompleted" event should be received
    And the event should contain text "Done"
    And the event should contain token counts

  Scenario: LLM encounters an error
    Given the LLM is processing a message
    When the LLM encounters an error "API rate limit exceeded"
    Then an "LlmError" event should be received
    And the error message should be "API rate limit exceeded"

  Scenario: LLM is interrupted by user
    Given the LLM is streaming a response
    When the user sends an "Interrupt" command
    Then an "LlmInterrupted" event should be received

  # ========================================================================
  # TOOL EVENTS
  # ========================================================================

  Scenario: Tool execution starts
    Given the agent is using tools
    When a tool "grep" starts executing with args {"pattern": "main"}
    Then a "ToolStarted" event should be received
    And the tool name should be "grep"
    And the args should contain "pattern": "main"

  Scenario: Tool execution completes
    Given a tool "grep" is executing
    When the tool completes with result "3 matches found"
    Then a "ToolCompleted" event should be received
    And the result should be "3 matches found"

  Scenario: Tool execution fails
    Given a tool "grep" is executing
    When the tool fails with error "File not found"
    Then a "ToolFailed" event should be received
    And the error should be "File not found"

  # ========================================================================
  # UI STATE EVENTS
  # ========================================================================

  Scenario: Message added to conversation
    When I send a message "Hello"
    Then a "MessageAdded" event should be received
    And the event should contain a user message with "Hello"

  Scenario: Provider changed
    When I select provider "openai"
    Then a "ProviderChanged" event should be received with "openai"

  Scenario: Model changed
    When I select model "gpt-4"
    Then a "ModelChanged" event should be received with "gpt-4"

  Scenario: Theme changed
    When I change the theme to "Dracula"
    Then a "ThemeChanged" event should be received with "Dracula"

  Scenario: Context file added
    When I add context file "src/main.rs"
    Then a "ContextFileAdded" event should be received with "src/main.rs"

  Scenario: Context file removed
    Given context contains "src/main.rs"
    When I remove context file "src/main.rs"
    Then a "ContextFileRemoved" event should be received with "src/main.rs"

  # ========================================================================
  # SESSION EVENTS
  # ========================================================================

  Scenario: Session information updated
    When the session is updated with new information
    Then a "SessionUpdated" event should be received
    And the event should contain session_id
    And the event should contain branch name
    And the event should contain total_cost

  Scenario: Task queue updated
    When a task is added to the queue
    Then a "TaskQueueUpdated" event should be received
    And the event should contain the queue count

  Scenario: Status message changed
    When a status message "Ready" is set
    Then a "StatusChanged" event should be received with "Ready"

  # ========================================================================
  # EVENT ORDERING
  # ========================================================================

  Scenario: Events are received in order
    Given the event subscriber is listening
    When I perform 5 actions that generate events
    Then events should be received in the same order
    And no events should be lost
    And no events should be duplicated
