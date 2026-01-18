@ui_backend @app_service
Feature: AppService Command Handling
  As a frontend developer
  I want the AppService to handle all user commands
  So that business logic is separated from UI rendering

  Background:
    Given the AppService is initialized
    And the event channel is listening

  # ========================================================================
  # AGENT MODE MANAGEMENT
  # ========================================================================

  Scenario: Cycle through agent modes
    Given the current agent mode is "Build"
    When I send the "CycleAgentMode" command
    Then the agent mode should be "Plan"
    And an "AgentModeChanged" event should be published
    When I send the "CycleAgentMode" command
    Then the agent mode should be "Ask"
    When I send the "CycleAgentMode" command
    Then the agent mode should be "Build"

  Scenario: Set agent mode explicitly
    Given the current agent mode is "Build"
    When I send the "SetAgentMode(Plan)" command
    Then the agent mode should be "Plan"
    And an "AgentModeChanged" event should be published
    And the status message should contain "Agent mode: Plan"

  # ========================================================================
  # BUILD MODE MANAGEMENT
  # ========================================================================

  Scenario: Cycle through build modes
    Given the current build mode is "Balanced"
    When I send the "CycleBuildMode" command
    Then the build mode should be "Careful"
    When I send the "CycleBuildMode" command
    Then the build mode should be "Manual"
    When I send the "CycleBuildMode" command
    Then the build mode should be "Balanced"

  Scenario: Set build mode explicitly
    Given the current build mode is "Balanced"
    When I send the "SetBuildMode(Manual)" command
    Then the build mode should be "Manual"
    And a "BuildModeChanged" event should be published
    And the status message should contain "Build mode: Manual"

  # ========================================================================
  # UI TOGGLES
  # ========================================================================

  Scenario: Toggle sidebar visibility
    Given the sidebar is visible
    When I send the "ToggleSidebar" command
    Then the sidebar should not be visible
    When I send the "ToggleSidebar" command
    Then the sidebar should be visible

  Scenario: Toggle thinking display
    Given thinking display is disabled
    When I send the "ToggleThinking" command
    Then thinking display should be enabled
    And a "ThinkingToggled" event should be published
    And the status message should contain "enabled"

  # ========================================================================
  # INPUT HANDLING
  # ========================================================================

  Scenario: Insert characters into input
    Given the input is empty
    When I send the "InsertChar('H')" command
    And I send the "InsertChar('i')" command
    Then the input text should be "Hi"
    And the cursor position should be 2

  Scenario: Delete character before cursor
    Given the input text is "Hello"
    And the cursor is at position 5
    When I send the "DeleteCharBefore" command
    Then the input text should be "Hell"
    And the cursor position should be 4

  Scenario: Delete character after cursor
    Given the input text is "Hello"
    And the cursor is at position 2
    When I send the "DeleteCharAfter" command
    Then the input text should be "Helo"
    And the cursor position should be 2

  Scenario: Move cursor left
    Given the input text is "Hello"
    And the cursor is at position 5
    When I send the "CursorLeft" command
    Then the cursor position should be 4

  Scenario: Move cursor right
    Given the input text is "Hello"
    And the cursor is at position 0
    When I send the "CursorRight" command
    Then the cursor position should be 1

  Scenario: Move cursor to line start
    Given the input text is "Hello World"
    And the cursor is at position 11
    When I send the "CursorToLineStart" command
    Then the cursor position should be 0

  Scenario: Move cursor to line end
    Given the input text is "Hello World"
    And the cursor is at position 0
    When I send the "CursorToLineEnd" command
    Then the cursor position should be 11

  Scenario: Insert newline in input
    Given the input text is "Hello"
    And the cursor is at position 5
    When I send the "InsertNewline" command
    Then the input text should contain a newline
    And the cursor position should be 6

  Scenario: Clear input
    Given the input text is "Hello World"
    And the cursor is at position 5
    When I send the "ClearInput" command
    Then the input text should be ""
    And the cursor position should be 0

  # ========================================================================
  # MESSAGE SENDING
  # ========================================================================

  Scenario: Send a message to LLM
    Given the LLM is connected
    And the input text is "Hello, AI!"
    When I send the "SendMessage" command
    Then a user message should be added with content "Hello, AI!"
    And a "MessageAdded" event should be published
    And the input should be cleared
    And the message should be added to history
    And an "LlmStarted" event should be published
    And the LLM processing flag should be set

  Scenario: Send empty message is ignored
    Given the input text is ""
    When I send the "SendMessage" command
    Then no message should be added
    And no events should be published

  Scenario: Send message when LLM not connected
    Given the LLM is not connected
    And the input text is "Hello"
    When I send the "SendMessage" command
    Then a user message should be added
    And an "LlmError" event should be published
    And the error should mention "not connected"

  # ========================================================================
  # PROVIDER/MODEL MANAGEMENT
  # ========================================================================

  Scenario: Select a provider
    Given no provider is selected
    When I send the "SelectProvider(openai)" command
    Then the current provider should be "openai"
    And a "ProviderChanged" event should be published with "openai"

  Scenario: Select a model
    Given no model is selected
    When I send the "SelectModel(gpt-4)" command
    Then the current model should be "gpt-4"
    And a "ModelChanged" event should be published with "gpt-4"

  # ========================================================================
  # CONTEXT FILE MANAGEMENT
  # ========================================================================

  Scenario: Add a context file
    Given no context files are loaded
    When I send the "AddContextFile(src/main.rs)" command
    Then the context should contain "src/main.rs"
    And a "ContextFileAdded" event should be published

  Scenario: Remove a context file
    Given the context contains "src/main.rs"
    When I send the "RemoveContextFile(src/main.rs)" command
    Then the context should not contain "src/main.rs"
    And a "ContextFileRemoved" event should be published

  # ========================================================================
  # QUIT COMMAND
  # ========================================================================

  Scenario: Quit the application
    Given the application is running
    When I send the "Quit" command
    Then the should_quit flag should be set
