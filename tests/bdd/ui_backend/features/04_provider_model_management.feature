@ui_backend @providers @models
Feature: Provider and Model Management
  As a user
  I want to select LLM providers and models
  So that I can use different AI backends

  Background:
    Given the AppService is initialized

  # ========================================================================
  # PROVIDER LISTING
  # ========================================================================

  Scenario: Get available providers
    When I request the list of available providers
    Then I should receive at least 3 providers
    And the list should include "openai"
    And the list should include "anthropic"
    And the list should include "ollama"

  Scenario: Provider info includes configuration status
    When I request the list of available providers
    Then each provider should have an "id"
    And each provider should have a "name"
    And each provider should have a "description"
    And each provider should have a "configured" flag
    And each provider should have an "icon"

  Scenario: Configured providers are marked
    Given the OPENAI_API_KEY environment variable is set
    When I request the list of available providers
    Then the "openai" provider should be marked as configured
    And unconfigured providers should be marked as not configured

  # ========================================================================
  # MODEL LISTING
  # ========================================================================

  Scenario: Get models for OpenAI
    When I request models for provider "openai"
    Then I should receive at least 2 models
    And the list should include "gpt-4"
    And the list should include "gpt-3.5-turbo"

  Scenario: Get models for Anthropic
    When I request models for provider "anthropic"
    Then I should receive at least 2 models
    And the list should include "claude-3-opus"
    And the list should include "claude-3-sonnet"

  Scenario: Model info includes metadata
    When I request models for provider "openai"
    Then each model should have an "id"
    And each model should have a "name"
    And each model should have a "description"
    And each model should have a "provider"
    And each model should have a "context_window"
    And each model should have "max_tokens"

  Scenario: Get models for unknown provider
    When I request models for provider "unknown"
    Then I should receive an empty list

  # ========================================================================
  # PROVIDER SELECTION
  # ========================================================================

  Scenario: Select a provider
    Given no provider is selected
    When I set the provider to "openai"
    Then the current provider should be "openai"
    And a "ProviderChanged" event should be published

  Scenario: Change provider
    Given the current provider is "openai"
    When I set the provider to "anthropic"
    Then the current provider should be "anthropic"
    And a "ProviderChanged" event should be published with "anthropic"

  Scenario: Select same provider again
    Given the current provider is "openai"
    When I set the provider to "openai"
    Then the current provider should still be "openai"
    And a "ProviderChanged" event should still be published

  # ========================================================================
  # MODEL SELECTION
  # ========================================================================

  Scenario: Select a model
    Given no model is selected
    When I set the model to "gpt-4"
    Then the current model should be "gpt-4"
    And a "ModelChanged" event should be published

  Scenario: Change model
    Given the current model is "gpt-4"
    When I set the model to "claude-3-opus"
    Then the current model should be "claude-3-opus"
    And a "ModelChanged" event should be published with "claude-3-opus"

  # ========================================================================
  # PROVIDER/MODEL INTEGRATION
  # ========================================================================

  Scenario: Provider change with model selection
    Given the provider is "openai" with model "gpt-4"
    When I change the provider to "anthropic"
    Then the provider should be "anthropic"
    And the model should still be set to "gpt-4"
    # Note: UI should typically clear or suggest models for new provider

  Scenario: Models filtered by provider
    Given the provider is "openai"
    When I request available models
    Then only OpenAI models should be returned
    And Anthropic models should not be included
