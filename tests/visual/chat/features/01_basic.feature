# Feature: Basic Chat Interaction
# Priority: P0 (Smoke)
# Tests core chat functionality with tark_sim echo scenario

@p0 @smoke @basic
Feature: Basic Chat Interaction
  As a user of tark chat
  I want to send messages and receive responses
  So that I can interact with the AI assistant

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows
    And TARK_SIM_SCENARIO is "echo"

  # =============================================================================
  # STARTUP AND INITIALIZATION
  # =============================================================================

  @startup
  Scenario: Application starts successfully
    Then I should see the welcome message "Welcome to tark chat!"
    And I should see the status bar at the bottom
    And the input area should have focus
    And a recording is saved as "basic_startup.gif"
    And a snapshot is saved as "basic_startup.png"

  @startup
  Scenario: Status bar displays correct initial state
    Then the status bar should show provider "tark_sim"
    And the status bar should show mode indicator
    And the status bar should show token count "0"

  # =============================================================================
  # MESSAGE SENDING
  # =============================================================================

  @messaging
  Scenario: Send simple message and receive echo response
    When I type "Hello, can you help me?"
    And I press Enter
    Then I should see my message in the chat
    And I should see a response containing "Echo: Hello, can you help me?"
    And the response should complete within 5 seconds
    And a recording is saved as "basic_echo.gif"
    And a snapshot is saved as "basic_echo.png"

  @messaging
  Scenario: Send multiple messages in sequence
    When I type "First message"
    And I press Enter
    And I wait for response
    When I type "Second message"
    And I press Enter
    Then I should see both messages in the chat history
    And I should see responses for both messages

  @messaging
  Scenario: Empty message is not sent
    When I press Enter without typing
    Then no message should be sent
    And the input area should remain focused

  # =============================================================================
  # INPUT HANDLING
  # =============================================================================

  @input
  Scenario: Type and edit message before sending
    When I type "Hello wrold"
    And I press Backspace 4 times
    And I type "orld!"
    Then the input should show "Hello world!"
    When I press Enter
    Then the message "Hello world!" should be sent

  @input
  Scenario: Clear input with Escape
    When I type "Some text"
    And I press Escape
    Then the input should be empty

  # =============================================================================
  # NAVIGATION AND EXIT
  # =============================================================================

  @navigation
  Scenario: Exit chat cleanly with /exit command
    When I type "/exit"
    And I press Enter
    Then the application should exit cleanly
    And a recording is saved as "basic_exit.gif"

  @navigation
  Scenario: Exit chat with q in normal mode
    When I press Escape to enter normal mode
    And I press "q"
    Then the application should exit cleanly

  # =============================================================================
  # HELP AND COMMANDS
  # =============================================================================

  @commands
  Scenario: Display help with /help command
    When I type "/help"
    And I press Enter
    Then I should see available commands list
    And the command list should include "/exit"
    And the command list should include "/clear"
    And a snapshot is saved as "basic_help.png"

  @commands
  Scenario: Clear chat history with /clear
    Given I have sent a message "Test message"
    When I type "/clear"
    And I press Enter
    Then the chat history should be empty
    And the welcome message should be visible again
