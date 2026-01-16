# Feature: Tool Invocation
# Priority: P1 (Core)
# Tests tool calling behavior with tark_sim tool scenario

@p1 @core @tools
Feature: Tool Invocation
  As a user of tark chat
  I want the AI to use tools to help me
  So that it can search files, run commands, and perform actions

  Background:
    Given tark chat is running with provider "tark_sim"
    And the terminal is 120 columns by 40 rows

  # =============================================================================
  # BASIC TOOL EXECUTION
  # =============================================================================

  @grep
  Scenario: Execute grep tool to search files
    Given TARK_SIM_SCENARIO is "tool"
    When I type "Search for TODO comments in this project"
    And I press Enter
    Then I should see tool execution indicator
    And I should see "grep" or "search" in the tool name
    And I should see tool result in the response
    And a recording is saved as "tool_grep.gif"
    And a snapshot is saved as "tool_grep.png"

  @file
  Scenario: Read file contents
    Given TARK_SIM_SCENARIO is "tool"
    When I type "Show me the contents of README.md"
    And I press Enter
    Then I should see file read tool execution
    And the file contents should be displayed

  # =============================================================================
  # TOOL RESULT DISPLAY
  # =============================================================================

  @display
  Scenario: Tool results are formatted correctly
    Given TARK_SIM_SCENARIO is "tool"
    When I type "Run a simple tool"
    And I press Enter
    Then tool results should be visually distinct
    And tool name should be visible
    And tool output should be in a code block or distinct area

  @display
  Scenario: Multiple tool calls in sequence
    Given TARK_SIM_SCENARIO is "multi_tool"
    When I type "Search and then read a file"
    And I press Enter
    Then I should see multiple tool executions
    And each tool result should be displayed in order
    And the final summary should reference all tools

  # =============================================================================
  # APPROVAL WORKFLOW
  # =============================================================================

  @approval
  Scenario: Tool requires approval in paranoid mode
    Given approval mode is "paranoid"
    And TARK_SIM_SCENARIO is "tool"
    When I type "Run a shell command"
    And I press Enter
    Then I should see the approval dialog
    And the dialog should show the command to be executed
    And I should see approve/deny options
    And a snapshot is saved as "tool_approval.png"

  @approval
  Scenario: Approve tool execution
    Given approval mode is "paranoid"
    And TARK_SIM_SCENARIO is "tool"
    When I type "Execute a command"
    And I press Enter
    And I see the approval dialog
    And I press "y" to approve
    Then the tool should execute
    And the result should be displayed

  @approval
  Scenario: Deny tool execution
    Given approval mode is "paranoid"
    And TARK_SIM_SCENARIO is "tool"
    When I type "Execute a dangerous command"
    And I press Enter
    And I see the approval dialog
    And I press "n" to deny
    Then the tool should not execute
    And a denial message should appear
