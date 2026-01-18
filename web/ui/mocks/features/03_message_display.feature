# Feature: Message Display
# Tests rendering of different message types in the conversation
# Reference: screenshots/03-message-types-top.png, screenshots/messages-with-themes.png

@messages @core
Feature: Message Display
  As a user of the TUI application
  I want to see different message types displayed distinctly
  So that I can easily follow the conversation flow

  Background:
    Given the TUI application is running
    And the message area is visible

  # =============================================================================
  # SYSTEM MESSAGES
  # =============================================================================

  @system-message
  Scenario: Display system message
    When a system message is received with content "Agent initialized successfully"
    Then the message should be displayed with system styling
    And the message should use the theme's system color
    And the message should have a system icon "⚡" or "ℹ"

  Scenario: System message formatting
    Given a system message "{agent_name} Core v{version} initialized"
    Then the message should display the configured agent name
    And the message should display the configured version

  # =============================================================================
  # USER INPUT MESSAGES
  # =============================================================================

  @user-message
  Scenario: Display user input message
    When the user submits "Hello, can you help me?"
    Then the message should be displayed with user styling
    And the message should show the user icon "{config.user_icon}"
    And the message should show the user label "{config.user_name}"
    And the message bubble should use theme's user bubble colors

  Scenario: User message bubble styling
    Given a user message is displayed
    Then the bubble should have background color "var(--user-bubble-bg)"
    And the bubble should have border color "var(--user-bubble-border)"
    And the text should use color "var(--user-bubble-text)"

  Scenario: User icon container styling
    Given a user message is displayed
    Then the user icon container should be visible on the left
    And the container should have background "var(--user-icon-bg)"
    And the icon should use color "var(--user-icon-color)"

  Scenario: Long user message wraps correctly
    When the user submits a message longer than the terminal width
    Then the message should wrap to multiple lines
    And the text should not be truncated
    And the bubble should expand to contain all text

  # =============================================================================
  # AGENT OUTPUT MESSAGES
  # =============================================================================

  @agent-message
  Scenario: Display agent output message
    When the agent responds with "I can help you with that!"
    Then the message should be displayed with agent styling
    And the message should show the agent icon "{config.agent_icon}" or "🤖"
    And the message should show the agent label "{config.agent_name_short}"
    And the message bubble should use theme's agent bubble colors

  Scenario: Agent message bubble styling
    Given an agent message is displayed
    Then the bubble should have background color "var(--agent-bubble-bg)"
    And the bubble should have border color "var(--agent-bubble-border)"
    And the text should use color "var(--agent-bubble-text)"

  Scenario: Agent icon container styling
    Given an agent message is displayed
    Then the agent icon container should be visible on the left
    And the container should have background "var(--agent-icon-bg)"
    And the icon should use color "var(--agent-icon-color)"

  Scenario: Multi-line agent response
    When the agent responds with a multi-line message
    Then each line should be displayed
    And code blocks should be formatted with monospace font
    And the message should preserve whitespace formatting

  # =============================================================================
  # TOOL MESSAGES
  # =============================================================================

  @tool-message
  Scenario: Display tool execution message
    When a tool message is displayed for "read_file" operation
    Then the message should show tool styling
    And the message should indicate the tool name
    And the message should use the theme's tool color

  Scenario: Tool message with file path
    Given a tool reads file "/src/main.rs"
    When the tool message is displayed
    Then the file path should be highlighted
    And the path should be styled distinctly from regular text

  Scenario: Tool message with status
    Given a tool execution completes successfully
    Then the message should show a success indicator "✓"
    Given a tool execution fails
    Then the message should show a failure indicator "✗"

  # =============================================================================
  # COMMAND MESSAGES
  # =============================================================================

  @command-message
  Scenario: Display command message
    When a command "/model" is executed
    Then the message should be displayed with command styling
    And the command should be prefixed with appropriate indicator
    And the message should use the theme's command color

  Scenario: Command with arguments
    When a command "/theme catppuccin-mocha" is executed
    Then the command name should be styled differently from arguments
    And both should be visible in the message

  # =============================================================================
  # THINKING MESSAGES
  # =============================================================================

  @thinking-message
  Scenario: Display thinking block
    Given thinking mode is enabled
    When the agent sends a thinking block
    Then the thinking message should be displayed
    And the message should show brain icon "🧠"
    And the content should be in a collapsible section

  Scenario: Thinking message styling
    Given a thinking message is displayed
    Then the text should use "var(--msg-thinking)" color
    And the content should be italicized or styled distinctly
    And the message should be visually de-emphasized compared to output

  Scenario: Collapse/expand thinking block
    Given a thinking message is displayed and expanded
    When I press "Enter" on the thinking message
    Then the thinking content should collapse
    When I press "Enter" again
    Then the thinking content should expand

  Scenario: Thinking disabled hides thinking blocks
    Given thinking mode is disabled
    When the agent sends a thinking block
    Then the thinking message should not be displayed

  # =============================================================================
  # MESSAGE ORDERING AND GROUPING
  # =============================================================================

  @message-ordering
  Scenario: Messages display in chronological order
    Given the following messages occur in order:
      | type   | content                  |
      | system | Initialized              |
      | input  | Hello                    |
      | output | Hi there!                |
      | tool   | Reading config.toml      |
      | output | Found the configuration  |
    Then the messages should be displayed in the same order

  Scenario: Related messages are visually grouped
    Given a user message followed by an agent response
    Then there should be appropriate spacing between message groups
    And consecutive messages from the same source should have tighter spacing

  # =============================================================================
  # MESSAGE INTERACTIONS
  # =============================================================================

  @message-interactions
  Scenario: Copy message content
    Given an agent message is displayed
    When I focus on the message and press "y" (yank)
    Then the message content should be copied to clipboard
    And a "Copied!" indicator should briefly appear

  Scenario: Navigate between messages
    Given there are multiple messages displayed
    When I press "j" (down)
    Then focus should move to the next message
    When I press "k" (up)
    Then focus should move to the previous message
