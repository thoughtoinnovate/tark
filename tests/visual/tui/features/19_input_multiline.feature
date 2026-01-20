# Feature: Multi-line Input Support
# Tests text wrapping, scrolling, and multi-line editing in input area

@input @multiline
Feature: Multi-line Input Support
  As a user of the TUI application
  I want to write multi-line messages
  So that I can compose complex prompts

  Background:
    Given the TUI application is running
    And the input area has focus

  @multiline-entry
  Scenario: Insert newline with SHIFT+Enter
    Given the input is empty
    When I type "First line"
    And I press "SHIFT+Enter"
    Then a newline should be inserted
    And the cursor should be on line 2
    
    When I type "Second line"
    And I press "SHIFT+Enter"
    And I type "Third line"
    Then the input should contain 3 lines

  @multiline-navigation
  Scenario: Navigate multi-line input with cursor keys
    Given I have multi-line input:
      """
      Line one
      Line two
      Line three
      """
    And the cursor is at the end
    When I press "Up Arrow"
    Then the cursor should move to line 2
    
    When I press "Up Arrow"
    Then the cursor should move to line 1
    
    When I press "Down Arrow"
    Then the cursor should move to line 2

  @multiline-scrolling
  Scenario: Scroll long input to keep cursor visible
    Given the input area height is 5 lines
    And I have input with 10 lines
    When the cursor is on line 1
    Then lines 1-5 should be visible
    
    When I move the cursor to line 7
    Then lines 3-7 should be visible
    And the view should have scrolled down
    
    When I move the cursor to line 10
    Then lines 6-10 should be visible

  @text-wrapping
  Scenario: Long lines wrap correctly
    Given the input area width is 40 characters
    When I type a line longer than 40 characters
    Then the line should wrap to the next visual line
    And the cursor should remain visible
    And the line count should still be 1 (logical line)

  @cursor-movement-words
  Scenario: Navigate by words with Ctrl+Arrow
    Given I have typed "The quick brown fox jumps"
    And the cursor is at the end
    When I press "Ctrl+Left"
    Then the cursor should be at the start of "jumps"
    
    When I press "Ctrl+Left"
    Then the cursor should be at the start of "fox"
    
    When I press "Ctrl+Right"
    Then the cursor should be at the start of "jumps"

  @home-end-keys
  Scenario: Home and End keys in multi-line input
    Given I have multi-line input with cursor on line 2, column 5
    When I press "Home"
    Then the cursor should move to column 0 of line 2
    
    When I press "End"
    Then the cursor should move to end of line 2
    
    When I press "Ctrl+Home"
    Then the cursor should move to start of entire input
    
    When I press "Ctrl+End"
    Then the cursor should move to end of entire input

  @multiline-submit
  Scenario: Submit multi-line message with Enter
    Given I have multi-line input:
      """
      This is a complex
      multi-line prompt
      for the LLM
      """
    When I press "Enter" (without SHIFT)
    Then the message should be submitted
    And all lines should be included in the message
    And the input should be cleared
