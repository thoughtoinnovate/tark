# E2E Smoke Test
# Tests the real binary in a real terminal environment
# Run: cargo test --test cucumber_e2e --release

@e2e @smoke
Feature: E2E Smoke Test
  As a user
  I want to verify the TUI starts and responds to basic inputs
  So that I know the binary works in a real terminal

  Background:
    Given the TUI application is running

  Scenario: Application starts successfully
    Then I should see "Welcome"
    And the header should be visible

  Scenario: User can type in input area
    When I type "Hello world"
    Then I should see "Hello world"

  Scenario: User can toggle sidebar with Ctrl+B
    Given the terminal has at least 120 columns and 40 rows
    When I press "Ctrl+B"
    Then the sidebar should be hidden
    When I press "Ctrl+B"
    Then the sidebar should be visible

  Scenario: User can open help with ?
    When I press "?"
    Then I should see "Help"
    When I press "Escape"
    Then the help modal should be hidden
