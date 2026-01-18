# Feature: Theme Picker Modal
# Tests the theme selection modal functionality
# Reference: screenshots/10-theme-picker-modal.png

@modal @theme-picker
Feature: Theme Picker Modal
  As a user of the TUI application
  I want to select and preview different themes
  So that I can customize the appearance of the TUI

  Background:
    Given the TUI application is running
    And the theme picker modal is not open

  # =============================================================================
  # MODAL OPENING
  # =============================================================================

  @modal-open
  Scenario: Open theme picker via /theme command
    When I type "/theme" in the input
    Then the theme picker modal should open
    And the input should be cleared

  Scenario: Modal has proper title
    Given the theme picker modal is open
    Then the modal title should be "Select Theme"
    And a close button should be visible

  # =============================================================================
  # THEME LIST
  # =============================================================================

  @theme-list
  Scenario: Display available themes
    Given the theme picker modal is open
    Then the following themes should be listed:
      | theme              | type  |
      | Catppuccin Mocha   | Dark  |
      | Catppuccin Latte   | Light |
      | Nord               | Dark  |
      | GitHub Dark        | Dark  |
      | One Dark Pro       | Dark  |
      | Tokyo Night        | Dark  |
      | Gruvbox Dark       | Dark  |

  Scenario: Themes grouped by type
    Given the theme picker modal is open
    Then themes should be organized into "Dark" and "Light" sections
    Or themes should show a light/dark indicator

  Scenario: Highlight current theme
    Given the current theme is "Catppuccin Mocha"
    And the theme picker modal is open
    Then "Catppuccin Mocha" should be marked as active

  # =============================================================================
  # SEARCH/FILTER
  # =============================================================================

  @theme-search
  Scenario: Filter themes by typing
    Given the theme picker modal is open
    When I type "cat"
    Then only Catppuccin themes should be visible

  Scenario: Filter by theme type
    Given the theme picker modal is open
    When I type "dark"
    Then only dark themes should be visible

  # =============================================================================
  # THEME SELECTION
  # =============================================================================

  @theme-selection
  Scenario: Select theme with Enter applies immediately
    Given the theme picker modal is open
    And "Nord" is highlighted
    When I press "Enter"
    Then the Nord theme should be applied
    And the modal should close
    And all UI elements should reflect the new theme

  Scenario: Select theme with click
    Given the theme picker modal is open
    When I click on "Tokyo Night"
    Then the Tokyo Night theme should be applied
    And the modal should close

  Scenario: Theme persists after selection
    Given I selected "Gruvbox Dark" theme
    When I restart the application
    Then the theme should still be "Gruvbox Dark"

  # =============================================================================
  # THEME PREVIEW
  # =============================================================================

  @theme-preview
  Scenario: Preview theme on hover
    Given the theme picker modal is open
    And the current theme is "Catppuccin Mocha"
    When I hover over "Nord"
    Then the UI should preview the Nord theme
    When I move away from "Nord"
    Then the UI should revert to "Catppuccin Mocha"

  Scenario: Theme preview shows color samples
    Given the theme picker modal is open
    Then each theme entry should show a color palette preview
    And the preview should include primary, secondary, and accent colors

  # =============================================================================
  # KEYBOARD NAVIGATION
  # =============================================================================

  @keyboard-nav
  Scenario: Navigate themes with arrow keys
    Given the theme picker modal is open
    When I press "Down Arrow"
    Then the next theme should be highlighted
    When I press "Up Arrow"
    Then the previous theme should be highlighted

  Scenario: Close without changing with Escape
    Given the theme picker modal is open
    And the current theme is "Nord"
    When I navigate to "Tokyo Night"
    And I press "Escape"
    Then the modal should close
    And the theme should remain "Nord"

  # =============================================================================
  # THEME APPLICATION
  # =============================================================================

  @theme-application
  Scenario: Theme applies to all components
    Given I apply theme "Catppuccin Mocha"
    Then the terminal header should use Catppuccin colors
    And the message area should use Catppuccin colors
    And the input area should use Catppuccin colors
    And the status bar should use Catppuccin colors
    And the sidebar should use Catppuccin colors

  Scenario: Theme applies to message bubbles
    Given I apply theme "Nord"
    Then user message bubbles should use Nord user colors
    And agent message bubbles should use Nord agent colors
    And system messages should use Nord system color

  Scenario: Theme applies to modals
    Given I apply theme "One Dark Pro"
    When I open any modal
    Then the modal should use One Dark Pro colors

  # =============================================================================
  # STYLING
  # =============================================================================

  @modal-styling
  Scenario: Theme picker modal itself follows current theme
    Given the current theme is "Catppuccin Latte"
    When the theme picker modal is open
    Then the modal should be styled with Catppuccin Latte colors

  Scenario: Visual distinction for theme types
    Given the theme picker modal is open
    Then dark themes should have a moon icon "🌙"
    And light themes should have a sun icon "☀️"
