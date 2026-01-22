# E2E Test - Basic TUI Interaction
# Tests the real binary with PTY
# Run: cargo test --test cucumber_e2e --release

@e2e @smoke
Feature: Basic TUI E2E
  As a user
  I want to verify the TUI works end-to-end
  So that I can trust the real binary

  Scenario: Application starts and shows welcome
    Given the TUI application is running
    Then I should see "Welcome"
    And I should see the header at the top

  Scenario: User can toggle sidebar
    Given the TUI application is running
    And the terminal has at least 120 columns and 40 rows
    When I press "Ctrl+B"
    Then the sidebar should be hidden
    When I press "Ctrl+B"
    Then the sidebar should be visible
