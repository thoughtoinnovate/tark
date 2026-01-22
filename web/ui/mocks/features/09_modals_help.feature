# Feature: Help Modal
# Tests the help and keyboard shortcuts modal
# Reference: screenshots/08-help-modal.png

@modal @help
Feature: Help Modal
  As a user of the TUI application
  I want to view help and keyboard shortcuts
  So that I can learn how to use the application efficiently

  Background:
    Given the TUI application is running
    And the help modal is not open

  # =============================================================================
  # MODAL OPENING
  # =============================================================================

  @modal-open
  Scenario: Open help modal via status bar button
    When I click on the "?" button in the status bar
    Then the help modal should open
    And the modal should be centered on screen

  Scenario: Open help modal via /help command
    When I type "/help" in the input
    Then the help modal should open

  Scenario: Open help modal via ? key
    When I press "?" key
    Then the help modal should open

  Scenario: Modal has proper title
    Given the help modal is open
    Then the modal title should be "Help & Shortcuts"
    And a close button "×" should be visible

  # =============================================================================
  # CONTENT SECTIONS
  # =============================================================================

  @help-content
  Scenario: Display general shortcuts section
    Given the help modal is open
    Then a "General" section should be visible
    And it should contain the following shortcuts:
      | shortcut      | description              |
      | Enter         | Submit message           |
      | Escape        | Clear input / Close modal|
      | Ctrl+C        | Cancel current operation |
      | Ctrl+L        | Clear terminal           |

  Scenario: Display navigation shortcuts section
    Given the help modal is open
    Then a "Navigation" section should be visible
    And it should contain the following shortcuts:
      | shortcut      | description              |
      | ↑ / k         | Previous message/item    |
      | ↓ / j         | Next message/item        |
      | Page Up       | Scroll up one page       |
      | Page Down     | Scroll down one page     |
      | g g           | Go to top                |
      | G             | Go to bottom             |

  Scenario: Display commands section
    Given the help modal is open
    Then a "Commands" section should be visible
    And it should contain the following commands:
      | command       | description              |
      | /model        | Change AI model          |
      | /theme        | Change color theme       |
      | /help         | Show this help           |
      | /clear        | Clear conversation       |

  Scenario: Display context/file shortcuts section
    Given the help modal is open
    Then a "Context Files" section should be visible
    And it should contain the following:
      | shortcut      | description              |
      | @             | Open file picker         |
      | + button      | Add context file         |
      | Backspace on @| Remove @mention          |

  Scenario: Display mode shortcuts section
    Given the help modal is open
    Then a "Modes" section should be visible
    And it should contain the following:
      | shortcut      | description              |
      | Ctrl+1        | Switch to Build mode     |
      | Ctrl+2        | Switch to Plan mode      |
      | Ctrl+3        | Switch to Ask mode       |
      | Ctrl+T        | Toggle thinking mode     |

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Scroll through help content
    Given the help modal is open
    And the content is longer than the modal
    When I press "Down Arrow" or "j"
    Then the content should scroll down
    When I press "Up Arrow" or "k"
    Then the content should scroll up

  Scenario: Close with Escape
    Given the help modal is open
    When I press "Escape"
    Then the modal should close
    And focus should return to the input area

  Scenario: Close with ? again (toggle)
    Given the help modal is open
    When I press "?" again
    Then the modal should close

  # =============================================================================
  # SEARCH WITHIN HELP
  # =============================================================================

  @help-search
  Scenario: Search for specific shortcut
    Given the help modal is open
    When I type "model"
    Then the "/model" command should be highlighted
    And non-matching entries should be dimmed

  Scenario: Search highlights matches
    Given the help modal is open
    When I type "ctrl"
    Then all shortcuts containing "Ctrl" should be highlighted

  # =============================================================================
  # STYLING
  # =============================================================================

  @modal-styling
  Scenario: Help modal follows current theme
    Given the theme is "Catppuccin Mocha"
    When the help modal is open
    Then the modal should use Catppuccin Mocha colors
    And section headers should use accent color
    And shortcut keys should be styled as code

  Scenario: Shortcut keys have distinct styling
    Given the help modal is open
    Then keyboard shortcuts should appear in a distinct style
    And they should look like keyboard keys (kbd style)
    And they should use a monospace font

  Scenario: Command names have distinct styling
    Given the help modal is open
    Then commands like "/model" should be styled as code
    And they should use the theme's command color

  # =============================================================================
  # ACCESSIBILITY
  # =============================================================================

  @accessibility
  Scenario: Help is accessible without mouse
    Given the application just started
    When I press "?"
    Then the help modal should open
    And I should be able to navigate with keyboard only
    And I should be able to close with Escape

  Scenario: All sections are reachable
    Given the help modal is open
    When I navigate through all sections
    Then I should be able to reach every section
    And every shortcut should be visible
