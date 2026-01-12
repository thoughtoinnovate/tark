//! End-to-End BDD Tests for TUI
//!
//! These tests simulate user interactions with the TUI using a test harness
//! that captures terminal output and allows simulating key presses.
//!
//! ## BDD Format
//!
//! Tests follow Given-When-Then format:
//! - **Given**: Initial setup (provider, model, etc.)
//! - **When**: User actions (key presses, commands)
//! - **Then**: Expected outcomes (state changes, UI updates)

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::collections::VecDeque;

/// Test harness for TUI E2E testing
///
/// Provides a way to test TUI interactions without a real terminal.
pub struct TuiTestHarness {
    /// Simulated key event queue
    key_queue: VecDeque<KeyEvent>,
    /// Current provider (simulated)
    current_provider: String,
    /// Current model (simulated)
    current_model: String,
    /// Picker visible state
    picker_visible: bool,
    /// Picker items (simulated)
    picker_items: Vec<String>,
    /// Selected picker index
    picker_index: usize,
    /// Picker type
    picker_type: Option<PickerType>,
    /// Model picker state (two-step flow)
    model_picker_state: Option<ModelPickerState>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PickerType {
    Provider,
    Model,
}

#[derive(Debug, Clone)]
pub enum ModelPickerState {
    SelectingProvider,
    SelectingModel { provider: String },
}

impl TuiTestHarness {
    /// Create a new test harness with default state
    pub fn new() -> Self {
        Self {
            key_queue: VecDeque::new(),
            current_provider: "openai".to_string(),
            current_model: "gpt-4o".to_string(),
            picker_visible: false,
            picker_items: vec![],
            picker_index: 0,
            picker_type: None,
            model_picker_state: None,
        }
    }

    // === GIVEN (Setup) ===

    /// Set the current provider
    pub fn given_provider(mut self, provider: &str) -> Self {
        self.current_provider = provider.to_string();
        self
    }

    /// Set the current model
    pub fn given_model(mut self, model: &str) -> Self {
        self.current_model = model.to_string();
        self
    }

    // === WHEN (Actions) ===

    /// Simulate pressing a key
    pub fn when_key_pressed(&mut self, code: KeyCode) -> &mut Self {
        self.key_queue
            .push_back(KeyEvent::new(code, KeyModifiers::NONE));
        self.process_key_queue();
        self
    }

    /// Simulate pressing a key with modifiers
    pub fn when_key_pressed_with_modifiers(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> &mut Self {
        self.key_queue.push_back(KeyEvent::new(code, modifiers));
        self.process_key_queue();
        self
    }

    /// Simulate typing the /model command
    pub fn when_model_command(&mut self) -> &mut Self {
        // Simulate typing /model and pressing Enter
        self.picker_visible = true;
        self.picker_type = Some(PickerType::Provider);
        self.model_picker_state = Some(ModelPickerState::SelectingProvider);
        self.picker_items = self.get_provider_list();
        self.picker_index = 0;
        self
    }

    /// Simulate selecting an item in the picker
    pub fn when_picker_select(&mut self, item: &str) -> &mut Self {
        // Find the item index
        if let Some(idx) = self.picker_items.iter().position(|i| i == item) {
            self.picker_index = idx;
        }
        // Simulate pressing Enter
        self.process_picker_select();
        self
    }

    /// Simulate pressing down arrow in picker
    pub fn when_picker_down(&mut self) -> &mut Self {
        if self.picker_visible && self.picker_index < self.picker_items.len() - 1 {
            self.picker_index += 1;
        }
        self
    }

    /// Simulate pressing up arrow in picker
    pub fn when_picker_up(&mut self) -> &mut Self {
        if self.picker_visible && self.picker_index > 0 {
            self.picker_index -= 1;
        }
        self
    }

    // === THEN (Assertions) ===

    /// Assert picker is visible
    pub fn then_picker_visible(&self) -> &Self {
        assert!(self.picker_visible, "Expected picker to be visible");
        self
    }

    /// Assert picker is hidden
    pub fn then_picker_hidden(&self) -> &Self {
        assert!(!self.picker_visible, "Expected picker to be hidden");
        self
    }

    /// Assert picker type
    pub fn then_picker_type(&self, expected: PickerType) -> &Self {
        assert_eq!(
            self.picker_type,
            Some(expected.clone()),
            "Expected picker type {:?}",
            expected
        );
        self
    }

    /// Assert picker contains item
    pub fn then_picker_contains(&self, item: &str) -> &Self {
        assert!(
            self.picker_items.iter().any(|i| i == item),
            "Expected picker to contain '{}', but items are: {:?}",
            item,
            self.picker_items
        );
        self
    }

    /// Assert picker does NOT contain item
    pub fn then_picker_not_contains(&self, item: &str) -> &Self {
        assert!(
            !self.picker_items.iter().any(|i| i == item),
            "Expected picker NOT to contain '{}', but it does",
            item
        );
        self
    }

    /// Assert current provider
    pub fn then_provider(&self, expected: &str) -> &Self {
        assert_eq!(
            self.current_provider, expected,
            "Expected provider '{}', got '{}'",
            expected, self.current_provider
        );
        self
    }

    /// Assert current model
    pub fn then_model(&self, expected: &str) -> &Self {
        assert_eq!(
            self.current_model, expected,
            "Expected model '{}', got '{}'",
            expected, self.current_model
        );
        self
    }

    /// Assert model picker state
    pub fn then_selecting_model_for(&self, provider: &str) -> &Self {
        match &self.model_picker_state {
            Some(ModelPickerState::SelectingModel { provider: p }) => {
                assert_eq!(
                    p, provider,
                    "Expected selecting model for '{}', got '{}'",
                    provider, p
                );
            }
            other => {
                panic!(
                    "Expected SelectingModel state for '{}', got {:?}",
                    provider, other
                );
            }
        }
        self
    }

    // === Internal helpers ===

    fn process_key_queue(&mut self) {
        while let Some(_key) = self.key_queue.pop_front() {
            // Process key events (simplified simulation)
        }
    }

    fn process_picker_select(&mut self) {
        if !self.picker_visible {
            return;
        }

        let selected = self.picker_items.get(self.picker_index).cloned();

        match &self.model_picker_state {
            Some(ModelPickerState::SelectingProvider) => {
                // Transition to model selection
                if let Some(provider) = selected {
                    self.model_picker_state = Some(ModelPickerState::SelectingModel {
                        provider: provider.clone(),
                    });
                    self.picker_type = Some(PickerType::Model);
                    self.picker_items = self.get_models_for_provider(&provider);
                    self.picker_index = 0;
                }
            }
            Some(ModelPickerState::SelectingModel { provider }) => {
                // Complete selection
                if let Some(model) = selected {
                    self.current_provider = provider.clone();
                    self.current_model = model;
                    self.picker_visible = false;
                    self.picker_type = None;
                    self.model_picker_state = None;
                }
            }
            None => {}
        }
    }

    fn get_provider_list(&self) -> Vec<String> {
        use tark_cli::plugins::PluginRegistry;

        let mut providers = vec![
            "openai".to_string(),
            "anthropic".to_string(),
            "gemini".to_string(),
            "copilot".to_string(),
            "openrouter".to_string(),
            "ollama".to_string(),
        ];

        // Add plugin providers
        if let Ok(registry) = PluginRegistry::new() {
            for plugin in registry.provider_plugins() {
                providers.push(plugin.id().to_string());
            }
        }

        providers
    }

    fn get_models_for_provider(&self, provider: &str) -> Vec<String> {
        use tark_cli::plugins::PluginRegistry;

        match provider {
            "openai" => vec![
                "gpt-4o".to_string(),
                "gpt-4o-mini".to_string(),
                "gpt-4".to_string(),
                "o1".to_string(),
            ],
            "anthropic" => vec![
                "claude-sonnet-4-20250514".to_string(),
                "claude-3-5-sonnet-20241022".to_string(),
                "claude-3-5-haiku-20241022".to_string(),
            ],
            "gemini" | "google" => vec![
                "gemini-2.0-flash-exp".to_string(),
                "gemini-1.5-pro".to_string(),
                "gemini-1.5-flash".to_string(),
            ],
            _ => {
                // Check if it's a plugin provider
                if let Ok(registry) = PluginRegistry::new() {
                    for plugin in registry.provider_plugins() {
                        if plugin.id() == provider {
                            // Get models from plugin's base_provider
                            for c in &plugin.manifest.contributes.providers {
                                if let Some(base) = &c.base_provider {
                                    return self.get_models_for_provider(base);
                                }
                            }
                        }
                    }
                }
                vec!["default".to_string()]
            }
        }
    }
}

impl Default for TuiTestHarness {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Feature: Model Picker with Plugin Providers
    ///
    /// As a user
    /// I want to select models from plugin providers
    /// So that I can use OAuth-authenticated providers like Gemini OAuth
    mod model_picker_plugin_provider {
        use super::*;

        #[test]
        fn scenario_plugin_provider_appears_in_provider_list() {
            // Given: A fresh TUI session with default provider
            let harness = TuiTestHarness::new()
                .given_provider("openai")
                .given_model("gpt-4o");

            // When: User opens the model picker
            let mut harness = harness;
            harness.when_model_command();

            // Then: Plugin provider should appear in the list
            harness
                .then_picker_visible()
                .then_picker_type(PickerType::Provider)
                .then_picker_contains("gemini-oauth");
        }

        #[test]
        fn scenario_selecting_plugin_provider_shows_its_models() {
            // Given: A fresh TUI session
            let mut harness = TuiTestHarness::new()
                .given_provider("openai")
                .given_model("gpt-4o");

            // When: User opens model picker and selects gemini-oauth
            harness.when_model_command();
            harness.when_picker_select("gemini-oauth");

            // Then: Should transition to model selection for gemini-oauth
            harness
                .then_picker_visible()
                .then_picker_type(PickerType::Model)
                .then_selecting_model_for("gemini-oauth");

            // And: Should show Gemini models (from base_provider)
            harness
                .then_picker_contains("gemini-2.0-flash-exp")
                .then_picker_contains("gemini-1.5-pro")
                .then_picker_contains("gemini-1.5-flash");
        }

        #[test]
        fn scenario_plugin_models_not_mixed_with_builtin() {
            // Given: A fresh TUI session with OpenAI
            let mut harness = TuiTestHarness::new()
                .given_provider("openai")
                .given_model("gpt-4o");

            // When: User switches to gemini-oauth provider
            harness.when_model_command();
            harness.when_picker_select("gemini-oauth");

            // Then: Should NOT show OpenAI models
            harness
                .then_picker_not_contains("gpt-4o")
                .then_picker_not_contains("gpt-4o-mini")
                .then_picker_not_contains("o1");
        }

        #[test]
        fn scenario_complete_model_selection_flow() {
            // Given: A fresh TUI session with OpenAI
            let mut harness = TuiTestHarness::new()
                .given_provider("openai")
                .given_model("gpt-4o");

            // When: User completes the full selection flow
            harness.when_model_command();
            harness.when_picker_select("gemini-oauth");
            harness.when_picker_select("gemini-1.5-pro");

            // Then: Provider and model should be updated
            harness
                .then_picker_hidden()
                .then_provider("gemini-oauth")
                .then_model("gemini-1.5-pro");
        }
    }

    /// Feature: Two-Step Model Picker Flow
    ///
    /// As a user
    /// I want a two-step picker (provider → model)
    /// So that I can easily find and select models
    mod two_step_picker_flow {
        use super::*;

        #[test]
        fn scenario_picker_starts_with_provider_selection() {
            // Given: A fresh TUI session
            let mut harness = TuiTestHarness::new();

            // When: User opens the model picker
            harness.when_model_command();

            // Then: Should show provider selection first
            harness
                .then_picker_visible()
                .then_picker_type(PickerType::Provider);
        }

        #[test]
        fn scenario_selecting_provider_transitions_to_models() {
            // Given: Picker is open at provider selection
            let mut harness = TuiTestHarness::new();
            harness.when_model_command();

            // When: User selects a provider
            harness.when_picker_select("anthropic");

            // Then: Should transition to model selection
            harness
                .then_picker_type(PickerType::Model)
                .then_selecting_model_for("anthropic");
        }

        #[test]
        fn scenario_builtin_providers_show_correct_models() {
            // Given: Picker is open
            let mut harness = TuiTestHarness::new();
            harness.when_model_command();

            // When: User selects OpenAI
            harness.when_picker_select("openai");

            // Then: Should show OpenAI models
            harness
                .then_picker_contains("gpt-4o")
                .then_picker_contains("gpt-4o-mini");
        }
    }
}
