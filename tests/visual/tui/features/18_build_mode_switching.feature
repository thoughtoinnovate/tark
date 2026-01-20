# Feature: Build Mode Switching
# Tests cycling between Manual, Balanced, and Careful build modes

@build-mode @keyboard
Feature: Build Mode Switching
  As a user in Build agent mode
  I want to switch between build modes
  So that I can control tool approval levels

  Background:
    Given the TUI application is running
    And the agent mode is "Build"

  @build-mode-cycle
  Scenario: Cycle build modes with Ctrl+M
    Given the current build mode is "Balanced"
    When I press "Ctrl+M"
    Then the build mode should change to "Careful"
    And the status bar should show "Careful"
    
    When I press "Ctrl+M"
    Then the build mode should change to "Manual"
    And the status bar should show "Manual"
    
    When I press "Ctrl+M"
    Then the build mode should change to "Balanced"
    And the status bar should show "Balanced"

  @build-mode-visibility
  Scenario: Build mode only visible in Build agent mode
    Given the agent mode is "Build"
    Then the build mode indicator should be visible in status bar
    
    When I switch to "Plan" mode
    Then the build mode indicator should not be visible
    
    When I switch to "Ask" mode
    Then the build mode indicator should not be visible
    
    When I switch back to "Build" mode
    Then the build mode indicator should be visible again

  @build-mode-backend-wiring
  Scenario: Build mode changes update AgentBridge trust level
    Given the AgentBridge is initialized
    And the build mode is "Balanced"
    When I press "Ctrl+M" to switch to "Careful"
    Then the AgentBridge trust level should be "Careful"
    
    When I press "Ctrl+M" to switch to "Manual"
    Then the AgentBridge trust level should be "Manual"
    And tool approvals should require manual confirmation

  @build-mode-visual
  Scenario: Build mode indicator styling
    Given the agent mode is "Build"
    When the build mode is "Manual"
    Then the status bar should show "🟢 Manual"
    
    When the build mode is "Balanced"
    Then the status bar should show "🟢 Balanced"
    
    When the build mode is "Careful"
    Then the status bar should show "🟢 Careful"
