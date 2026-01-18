@ui_backend @shared_state
Feature: SharedState Thread-Safe State Management
  As a developer
  I want thread-safe state management
  So that frontend and backend can safely share state

  # ========================================================================
  # THREAD SAFETY
  # ========================================================================

  Scenario: Concurrent state reads
    Given the SharedState is initialized
    When 10 threads read the agent mode concurrently
    Then all reads should succeed without deadlock
    And all threads should receive the same value

  Scenario: Concurrent state writes
    Given the SharedState is initialized
    When 10 threads toggle the sidebar concurrently
    Then all writes should succeed without deadlock
    And the final state should be consistent

  Scenario: Mixed concurrent reads and writes
    Given the SharedState is initialized
    When 5 threads read state concurrently
    And 5 threads write state concurrently
    Then all operations should succeed without deadlock
    And no data races should occur

  # ========================================================================
  # STATE GETTERS
  # ========================================================================

  Scenario: Read agent mode
    Given the agent mode is "Build"
    When I read the agent mode
    Then I should get "Build"

  Scenario: Read build mode
    Given the build mode is "Balanced"
    When I read the build mode
    Then I should get "Balanced"

  Scenario: Read LLM connection status
    Given the LLM is connected
    When I read the llm_connected flag
    Then I should get true

  Scenario: Read input text
    Given the input text is "Hello"
    When I read the input text
    Then I should get "Hello"

  Scenario: Read messages
    Given there are 3 messages in the conversation
    When I read the messages
    Then I should get a list of 3 messages

  # ========================================================================
  # STATE SETTERS
  # ========================================================================

  Scenario: Set agent mode
    Given the agent mode is "Build"
    When I set the agent mode to "Plan"
    Then reading the agent mode should return "Plan"

  Scenario: Set build mode
    Given the build mode is "Balanced"
    When I set the build mode to "Manual"
    Then reading the build mode should return "Manual"

  Scenario: Set sidebar visibility
    Given the sidebar is visible
    When I set sidebar visibility to false
    Then reading sidebar visibility should return false

  Scenario: Set theme
    Given the theme is "CatppuccinMocha"
    When I set the theme to "Dracula"
    Then reading the theme should return "Dracula"

  # ========================================================================
  # COLLECTION MANAGEMENT
  # ========================================================================

  Scenario: Add message to conversation
    Given the conversation has 0 messages
    When I add a user message "Hello"
    Then the conversation should have 1 message
    And the last message should be from the user
    And the last message content should be "Hello"

  Scenario: Add multiple messages
    Given the conversation has 0 messages
    When I add a user message "Hello"
    And I add an assistant message "Hi there"
    And I add a user message "How are you?"
    Then the conversation should have 3 messages
    And messages should alternate between user and assistant

  Scenario: Add context file
    Given there are 0 context files
    When I add a context file "src/main.rs"
    Then there should be 1 context file
    And the context files should contain "src/main.rs"

  Scenario: Remove context file
    Given there are context files ["src/main.rs", "src/lib.rs"]
    When I remove the context file "src/main.rs"
    Then there should be 1 context file
    And the context files should only contain "src/lib.rs"

  # ========================================================================
  # STATE DEFAULTS
  # ========================================================================

  Scenario: Default state values
    Given a new SharedState is created
    Then should_quit should be false
    And agent_mode should be "Build"
    And build_mode should be "Balanced"
    And thinking_enabled should be false
    And sidebar_visible should be true
    And theme should be "CatppuccinMocha"
    And llm_connected should be false
    And messages should be empty
    And input_text should be empty
    And context_files should be empty
