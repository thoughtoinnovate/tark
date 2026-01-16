# Feature: Terminal Layout
# Tests the core terminal layout structure and rendering
# Reference: screenshots/01-main-layout.png, screenshots/02-full-page.png

@layout @core
Feature: Terminal Layout
  As a user of the TUI application
  I want to see a properly structured terminal interface
  So that I can interact with the AI agent effectively

  Background:
    Given the TUI application is running
    And the terminal has at least 80 columns and 24 rows

  # =============================================================================
  # MAIN LAYOUT STRUCTURE
  # =============================================================================

  @smoke
  Scenario: Main layout renders with all sections
    Then I should see the terminal header at the top
    And I should see the message area in the center
    And I should see the input area at the bottom
    And I should see the status bar below the input area

  Scenario: Terminal header displays correct information
    Then the header should display the agent name "{config.agent_name}"
    And the header should display the header icon "{config.header_icon}"
    And the header should display the default path "{config.default_path}"
    And the header should have a border at the bottom

  Scenario: Terminal adapts to different terminal sizes
    Given the terminal is resized to 120 columns and 40 rows
    Then the layout should adapt to the new size
    And the message area should expand to fill available space
    And no content should be clipped or hidden

  Scenario: Minimum terminal size handling
    Given the terminal is resized to 60 columns and 15 rows
    Then the application should display a "terminal too small" warning
    Or the layout should gracefully degrade

  # =============================================================================
  # SIDEBAR INTEGRATION
  # =============================================================================

  @sidebar
  Scenario: Layout with sidebar expanded
    Given the sidebar is expanded
    Then the terminal should occupy the left portion of the screen
    And the sidebar should occupy the right portion
    And there should be a visible border between them

  Scenario: Layout with sidebar collapsed
    Given the sidebar is collapsed
    Then the terminal should occupy the full width
    And a collapse toggle button should be visible

  Scenario: Toggle sidebar visibility
    Given the sidebar is expanded
    When I press the sidebar toggle key
    Then the sidebar should collapse
    And the terminal should expand to full width

  # =============================================================================
  # SCROLL BEHAVIOR
  # =============================================================================

  @scroll
  Scenario: Message area scrolls when content exceeds viewport
    Given there are more messages than can fit in the viewport
    Then a scrollbar should be visible
    And the most recent message should be visible at the bottom

  Scenario: Scroll to top of message history
    Given there are 50 messages in the history
    And I am viewing the bottom of the message area
    When I press "g g" (vim-style go to top)
    Then I should see the first message in the history

  Scenario: Scroll to bottom of message history
    Given there are 50 messages in the history
    And I am viewing the middle of the message area
    When I press "G" (vim-style go to bottom)
    Then I should see the most recent message

  Scenario: Page up and page down navigation
    Given there are 100 messages in the history
    When I press "Page Up"
    Then the view should scroll up by one page
    When I press "Page Down"
    Then the view should scroll down by one page

  # =============================================================================
  # FOCUS MANAGEMENT
  # =============================================================================

  @focus
  Scenario: Default focus is on input area
    When the application starts
    Then the input area should have focus
    And the cursor should be visible in the input area

  Scenario: Focus returns to input after modal closes
    Given a modal is open
    When I close the modal
    Then the input area should have focus

  Scenario: Visual indication of focused element
    When the input area has focus
    Then the input area border should be highlighted
    When a modal has focus
    Then the modal should have a highlighted border

  # =============================================================================
  # BORDERS AND STYLING
  # =============================================================================

  @styling
  Scenario: Borders render correctly with Unicode box drawing
    Then the terminal should use Unicode box drawing characters
    And corners should use "╭", "╮", "╰", "╯" characters
    And horizontal lines should use "─" character
    And vertical lines should use "│" character

  Scenario: Borders adapt to theme colors
    Given the theme is set to "catppuccin-mocha"
    Then borders should use the theme's border color
    When I switch to theme "nord"
    Then borders should update to the new theme's border color
