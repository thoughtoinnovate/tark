@ui_backend @renderer
Feature: UiRenderer Trait Contract
  As a frontend implementation
  I want a clear contract to implement
  So that I can correctly integrate with the BFF layer

  # ========================================================================
  # RENDERER INITIALIZATION
  # ========================================================================

  Scenario: Create a mock renderer
    Given I implement the UiRenderer trait
    When I create a new renderer instance
    Then the renderer should be ready to use
    And the renderer should have a reference to SharedState

  # ========================================================================
  # RENDER METHOD
  # ========================================================================

  Scenario: Render current state
    Given a mock renderer is created
    And the SharedState has some messages
    When I call render() with the state
    Then the render should succeed
    And the renderer should display the messages
    And the renderer should display the input area
    And the renderer should display the status bar

  Scenario: Render with empty state
    Given a mock renderer is created
    And the SharedState is empty
    When I call render() with the state
    Then the render should succeed
    And the renderer should show an empty message list

  Scenario: Render with modal active
    Given a mock renderer is created
    And a help modal is active
    When I call render() with the state
    Then the render should succeed
    And the renderer should display the modal overlay

  # ========================================================================
  # SHOW MODAL METHOD
  # ========================================================================

  Scenario: Show help modal
    Given a mock renderer is created
    When I call show_modal() with HelpModal
    Then the modal should be displayed
    And the main content should be dimmed

  Scenario: Show provider picker modal
    Given a mock renderer is created
    When I call show_modal() with ProviderPickerModal
    Then the provider list should be displayed
    And the modal should allow selection

  Scenario: Show model picker modal
    Given a mock renderer is created
    When I call show_modal() with ModelPickerModal
    Then the model list should be displayed
    And the modal should allow filtering

  Scenario: Show file picker modal
    Given a mock renderer is created
    When I call show_modal() with FilePickerModal
    Then the file browser should be displayed
    And navigation should be enabled

  Scenario: Show theme picker modal
    Given a mock renderer is created
    When I call show_modal() with ThemePickerModal
    Then the theme list should be displayed
    And live preview should be enabled

  # ========================================================================
  # SET STATUS METHOD
  # ========================================================================

  Scenario: Set status message
    Given a mock renderer is created
    When I call set_status() with message "Ready"
    Then the status bar should display "Ready"

  Scenario: Set status with LLM connected
    Given a mock renderer is created
    When I call set_status() with llm_connected=true
    Then the status bar should show connected indicator

  Scenario: Set status with processing active
    Given a mock renderer is created
    When I call set_status() with processing=true
    Then the status bar should show processing spinner

  Scenario: Set status with token usage
    Given a mock renderer is created
    When I call set_status() with tokens_used=1500 and tokens_total=100000
    Then the status bar should display "1.5k / 100.0k tokens"

  # ========================================================================
  # GET SIZE METHOD
  # ========================================================================

  Scenario: Get terminal size
    Given a mock renderer with size 80x24
    When I call get_size()
    Then I should receive (80, 24)

  Scenario: Get window size after resize
    Given a mock renderer with size 80x24
    When the window is resized to 120x40
    And I call get_size()
    Then I should receive (120, 40)

  # ========================================================================
  # SHOULD QUIT METHOD
  # ========================================================================

  Scenario: Check quit status
    Given a mock renderer is created
    And the should_quit flag is false
    When I call should_quit()
    Then I should receive false

  Scenario: Check quit status after quit command
    Given a mock renderer is created
    When the user sends a Quit command
    And I call should_quit()
    Then I should receive true

  # ========================================================================
  # INTEGRATION WITH SHARED STATE
  # ========================================================================

  Scenario: Renderer reads latest state
    Given a mock renderer is created
    And the SharedState is updated with new messages
    When I call render()
    Then the renderer should display the updated messages

  Scenario: Multiple renderers share same state
    Given two renderers are created with the same SharedState
    When one renderer triggers a state change
    And both renderers call render()
    Then both should display the same updated state

  # ========================================================================
  # ERROR HANDLING
  # ========================================================================

  Scenario: Render with corrupted state
    Given a mock renderer is created
    And the SharedState has invalid data
    When I call render()
    Then the render should handle the error gracefully
    And should not crash

  Scenario: Show modal with invalid content
    Given a mock renderer is created
    When I call show_modal() with invalid modal content
    Then an error should be returned
    And the renderer should remain stable
