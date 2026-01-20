# Feature: Agent Mode Switching
# Tests cycling between Build, Plan, and Ask modes

@agent-mode @keyboard
Feature: Agent Mode Switching
  As a user of the TUI application
  I want to switch between agent modes
  So that I can use different agent behaviors

  Background:
    Given the TUI application is running
    And I am in the main terminal view

  @agent-mode-cycle
  Scenario: Cycle agent modes with SHIFT+TAB
    Given the current agent mode is "Build"
    When I press "SHIFT+TAB"
    Then the agent mode should change to "Plan"
    And the status bar should show "Plan"
    
    When I press "SHIFT+TAB"
    Then the agent mode should change to "Ask"
    And the status bar should show "Ask"
    
    When I press "SHIFT+TAB"
    Then the agent mode should change to "Build"
    And the status bar should show "Build"

  @agent-mode-direct
  Scenario Outline: Switch to specific agent mode
    Given the current agent mode is "<current_mode>"
    When I switch to "<target_mode>" mode
    Then the agent mode should be "<target_mode>"
    And the status bar should display the correct mode icon
    
    Examples:
      | current_mode | target_mode |
      | Build        | Plan        |
      | Build        | Ask         |
      | Plan         | Build       |
      | Plan         | Ask         |
      | Ask          | Build       |
      | Ask          | Plan        |

  @agent-mode-backend-wiring
  Scenario: Agent mode changes propagate to AgentBridge
    Given the AgentBridge is initialized
    And the current agent mode is "Build"
    When I press "SHIFT+TAB"
    Then the AgentBridge tool registry should update to "Plan" mode
    And read-only tools should be available
    And write tools should be disabled in Plan mode

  @agent-mode-visual
  Scenario: Agent mode indicator updates correctly
    Given the TUI is displaying the status bar
    When the agent mode is "Build"
    Then the status bar should show "🔨 Build"
    
    When the agent mode is "Plan"
    Then the status bar should show "📋 Plan"
    
    When the agent mode is "Ask"
    Then the status bar should show "💬 Ask"
