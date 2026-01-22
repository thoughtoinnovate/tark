# Feature: Theming System
# Tests the theme application and color system

@theming
Feature: Theming System
  As a user of the TUI application
  I want themes to be applied consistently
  So that the application looks cohesive and matches my preferences

  Background:
    Given the TUI application is running

  # =============================================================================
  # THEME APPLICATION
  # =============================================================================

  @theme-application
  Scenario: Default theme on first launch
    Given this is the first launch
    Then a default theme should be applied
    And the theme should be "Catppuccin Mocha" or system default

  Scenario: Theme persists across sessions
    Given I selected theme "Nord"
    And I close the application
    When I restart the application
    Then the theme should still be "Nord"

  Scenario: Theme applies to all components
    Given I apply theme "One Dark Pro"
    Then the following components should use theme colors:
      | component           |
      | Terminal header     |
      | Message area        |
      | Input area          |
      | Status bar          |
      | Sidebar             |
      | All modals          |

  # =============================================================================
  # COLOR MAPPINGS
  # =============================================================================

  @color-mappings
  Scenario Outline: Theme colors map correctly
    Given the theme is "<theme>"
    Then the background should be "<bg_color>"
    And the foreground text should be "<fg_color>"
    And the accent color should be "<accent>"

    Examples:
      | theme            | bg_color | fg_color | accent  |
      | Catppuccin Mocha | #1e1e2e  | #cdd6f4  | #89b4fa |
      | Catppuccin Latte | #eff1f5  | #4c4f69  | #1e66f5 |
      | Nord             | #2e3440  | #eceff4  | #88c0d0 |

  Scenario: Message type colors
    Given the theme is "Catppuccin Mocha"
    Then system messages should use teal color
    And command messages should use green color
    And thinking messages should use muted color
    And question messages should use sky/cyan color

  Scenario: User and agent message bubble colors
    Given the theme is "Catppuccin Mocha"
    Then user message bubbles should use blue tint
    And user icons should be blue
    And agent message bubbles should use green tint
    And agent icons should be green

  # =============================================================================
  # SPECIFIC THEMES
  # =============================================================================

  @catppuccin
  Scenario: Catppuccin Mocha theme specifics
    Given I apply theme "Catppuccin Mocha"
    Then the base background should be "#1e1e2e"
    And the mantle (darker) should be "#181825"
    And the surface should be "#313244"
    And text should be "#cdd6f4"
    And accents should use pastel colors

  Scenario: Catppuccin Latte theme specifics
    Given I apply theme "Catppuccin Latte"
    Then the base background should be "#eff1f5"
    And the text should be dark "#4c4f69"
    And it should be a light theme

  @nord
  Scenario: Nord theme specifics
    Given I apply theme "Nord"
    Then backgrounds should use Nord polar night colors
    And text should use Nord snow storm colors
    And accents should use Nord frost colors

  @gruvbox
  Scenario: Gruvbox Dark theme specifics
    Given I apply theme "Gruvbox Dark"
    Then backgrounds should use warm dark colors
    And text should use cream/light colors
    And accents should use orange and yellow tones

  # =============================================================================
  # ACCESSIBILITY
  # =============================================================================

  @accessibility
  Scenario: Sufficient color contrast
    Given any theme is applied
    Then text should have sufficient contrast against background
    And interactive elements should be distinguishable
    And error states should be clearly visible

  Scenario: Focus indicators visible
    Given any theme is applied
    When an element has focus
    Then the focus indicator should be clearly visible
    And it should use a contrasting color

  # =============================================================================
  # DYNAMIC UPDATES
  # =============================================================================

  @dynamic-updates
  Scenario: Theme change updates immediately
    Given the theme is "Nord"
    When I switch to "Tokyo Night"
    Then all colors should update immediately
    And there should be no visual glitches
    And no restart should be required

  Scenario: New messages follow current theme
    Given the theme is "Catppuccin Mocha"
    When a new message arrives
    Then the message should use Catppuccin colors
    When I switch to "Nord"
    And another message arrives
    Then the new message should use Nord colors

  # =============================================================================
  # SPECIAL STATES
  # =============================================================================

  @special-states
  Scenario: Disabled state theming
    Given the theme is "One Dark Pro"
    When an element is disabled
    Then it should use muted/dimmed colors
    And it should be visually distinct from enabled elements

  Scenario: Selected state theming
    Given the theme is "Tokyo Night"
    When an element is selected
    Then it should use the theme's selection color
    And text should remain readable

  Scenario: Error state theming
    Given the theme is "Gruvbox Dark"
    When an error occurs
    Then error indicators should use red/error color
    And the color should be visible against the background

  # =============================================================================
  # TERMINAL CAPABILITIES
  # =============================================================================

  @terminal-capabilities
  Scenario: Theme adapts to terminal color support
    Given the terminal supports 256 colors
    Then themes should use appropriate color palette

  Scenario: Fallback for limited color terminals
    Given the terminal only supports 16 colors
    Then themes should gracefully degrade
    And the UI should still be usable
