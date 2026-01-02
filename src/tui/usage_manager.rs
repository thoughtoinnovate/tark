//! Usage tracking manager for the TUI
//!
//! Manages token usage, cost tracking, and persistence to the SQLite database.
//! Integrates with the storage layer for usage data and models.dev for pricing.

#![allow(dead_code)]

use crate::llm::models_db;
use crate::storage::usage::{
    ModeUsage, ModelUsage, SessionWithStats, UsageLog, UsageSummary, UsageTracker,
};
use anyhow::Result;
use std::path::Path;

/// Usage manager for the TUI
pub struct UsageManager {
    /// Usage tracker (SQLite backend)
    tracker: Option<UsageTracker>,
    /// Current session ID
    session_id: Option<String>,
    /// Current session token counts
    current_input_tokens: u32,
    /// Current session output tokens
    current_output_tokens: u32,
    /// Current session cost
    current_cost: f64,
    /// Current provider
    provider: String,
    /// Current model
    model: String,
    /// Cached model capabilities (from models.dev)
    cached_capabilities: Option<crate::llm::ModelCapabilities>,
}

impl Default for UsageManager {
    fn default() -> Self {
        Self::new()
    }
}

impl UsageManager {
    /// Create a new usage manager
    pub fn new() -> Self {
        Self {
            tracker: None,
            session_id: None,
            current_input_tokens: 0,
            current_output_tokens: 0,
            current_cost: 0.0,
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            cached_capabilities: None,
        }
    }

    /// Initialize with storage backend
    pub fn with_storage(mut self, workspace_dir: impl AsRef<Path>) -> Result<Self> {
        let tark_dir = workspace_dir.as_ref().join(".tark");
        self.tracker = Some(UsageTracker::new(&tark_dir)?);
        Ok(self)
    }

    /// Start a new usage session
    pub fn start_session(
        &mut self,
        host: &str,
        username: &str,
        project_name: Option<&str>,
    ) -> Result<String> {
        if let Some(ref tracker) = self.tracker {
            let session = tracker.create_session(host, username, project_name)?;
            self.session_id = Some(session.id.clone());
            self.current_input_tokens = 0;
            self.current_output_tokens = 0;
            self.current_cost = 0.0;
            Ok(session.id)
        } else {
            // Generate a local session ID if no tracker
            let id = format!("local_{}", chrono::Utc::now().timestamp());
            self.session_id = Some(id.clone());
            Ok(id)
        }
    }

    /// Set the current session ID
    pub fn set_session_id(&mut self, session_id: &str) {
        self.session_id = Some(session_id.to_string());
    }

    /// Get the current session ID
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Set the current provider
    pub fn set_provider(&mut self, provider: &str) {
        self.provider = provider.to_string();
        self.cached_capabilities = None; // Clear cache when provider changes
    }

    /// Set the current model
    pub fn set_model(&mut self, model: &str) {
        self.model = model.to_string();
        self.cached_capabilities = None; // Clear cache when model changes
    }

    /// Get current provider
    pub fn provider(&self) -> &str {
        &self.provider
    }

    /// Get current model
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Log usage for a request
    pub fn log_usage(
        &mut self,
        input_tokens: u32,
        output_tokens: u32,
        mode: &str,
        request_type: &str,
    ) -> Result<()> {
        // Update current session totals
        self.current_input_tokens += input_tokens;
        self.current_output_tokens += output_tokens;

        // Calculate cost
        let cost = self.calculate_cost(input_tokens, output_tokens);
        self.current_cost += cost;

        // Persist to database if tracker is available
        if let (Some(ref tracker), Some(ref session_id)) = (&self.tracker, &self.session_id) {
            tracker.log_usage(UsageLog {
                session_id: session_id.clone(),
                provider: self.provider.clone(),
                model: self.model.clone(),
                mode: mode.to_string(),
                input_tokens,
                output_tokens,
                cost_usd: cost,
                request_type: request_type.to_string(),
                estimated: false,
            })?;
        }

        Ok(())
    }

    /// Calculate cost for tokens using models.dev pricing
    ///
    /// Falls back to hardcoded defaults if the model isn't found in the database.
    fn calculate_cost(&self, input_tokens: u32, output_tokens: u32) -> f64 {
        // Try to use cached capabilities first
        if let Some(ref caps) = self.cached_capabilities {
            if caps.input_cost > 0.0 || caps.output_cost > 0.0 {
                let input_cost = (input_tokens as f64) * caps.input_cost / 1_000_000.0;
                let output_cost = (output_tokens as f64) * caps.output_cost / 1_000_000.0;
                return input_cost + output_cost;
            }
        }

        // Fallback to hardcoded pricing
        Self::fallback_cost(&self.provider, &self.model, input_tokens, output_tokens)
    }

    /// Fallback cost calculation when models.dev data isn't available
    fn fallback_cost(provider: &str, model: &str, input_tokens: u32, output_tokens: u32) -> f64 {
        let (input_price, output_price) = match (provider.to_lowercase().as_str(), model) {
            ("openai" | "gpt", m) if m.contains("gpt-4o-mini") => (0.15, 0.60),
            ("openai" | "gpt", m) if m.contains("gpt-4o") => (2.5, 10.0),
            ("openai" | "gpt", m) if m.contains("o1") => (15.0, 60.0),
            ("openai" | "gpt", m) if m.contains("o3") => (10.0, 40.0),
            ("openai" | "gpt", _) => (5.0, 15.0),
            ("anthropic" | "claude", m) if m.contains("haiku") => (0.25, 1.25),
            ("anthropic" | "claude", m) if m.contains("sonnet") => (3.0, 15.0),
            ("anthropic" | "claude", m) if m.contains("opus") => (15.0, 75.0),
            ("anthropic" | "claude", _) => (3.0, 15.0),
            ("ollama" | "local", _) => (0.0, 0.0),
            _ => (0.0, 0.0),
        };

        let input_cost = (input_tokens as f64) * input_price / 1_000_000.0;
        let output_cost = (output_tokens as f64) * output_price / 1_000_000.0;
        input_cost + output_cost
    }

    /// Async method to refresh capabilities from models.dev
    pub async fn refresh_capabilities(&mut self) {
        let caps = models_db()
            .get_capabilities(&self.provider, &self.model)
            .await;
        self.cached_capabilities = Some(caps);
    }

    /// Get cached capabilities (if available)
    pub fn capabilities(&self) -> Option<&crate::llm::ModelCapabilities> {
        self.cached_capabilities.as_ref()
    }

    /// Get current session input tokens
    pub fn input_tokens(&self) -> u32 {
        self.current_input_tokens
    }

    /// Get current session output tokens
    pub fn output_tokens(&self) -> u32 {
        self.current_output_tokens
    }

    /// Get total tokens for current session
    pub fn total_tokens(&self) -> u32 {
        self.current_input_tokens + self.current_output_tokens
    }

    /// Get current session cost
    pub fn cost(&self) -> f64 {
        self.current_cost
    }

    /// Get usage summary
    pub fn get_summary(&self) -> Result<Option<UsageSummary>> {
        if let Some(ref tracker) = self.tracker {
            Ok(Some(tracker.get_summary()?))
        } else {
            Ok(None)
        }
    }

    /// Get usage by model
    pub fn get_usage_by_model(&self) -> Result<Vec<ModelUsage>> {
        if let Some(ref tracker) = self.tracker {
            tracker.get_usage_by_model()
        } else {
            Ok(Vec::new())
        }
    }

    /// Get usage by mode
    pub fn get_usage_by_mode(&self) -> Result<Vec<ModeUsage>> {
        if let Some(ref tracker) = self.tracker {
            tracker.get_usage_by_mode()
        } else {
            Ok(Vec::new())
        }
    }

    /// Get all sessions with stats
    pub fn get_sessions(&self) -> Result<Vec<SessionWithStats>> {
        if let Some(ref tracker) = self.tracker {
            tracker.get_sessions()
        } else {
            Ok(Vec::new())
        }
    }

    /// Format usage for display
    pub fn format_usage_display(&self) -> String {
        let mut output = String::new();

        output.push_str("=== Current Session ===\n");
        output.push_str(&format!("Provider: {}\n", self.provider));
        output.push_str(&format!("Model: {}\n", self.model));
        output.push_str(&format!("Input tokens: {}\n", self.current_input_tokens));
        output.push_str(&format!("Output tokens: {}\n", self.current_output_tokens));
        output.push_str(&format!("Total tokens: {}\n", self.total_tokens()));
        output.push_str(&format!("Cost: ${:.4}\n", self.current_cost));

        if let Ok(Some(summary)) = self.get_summary() {
            output.push_str("\n=== All-Time Summary ===\n");
            output.push_str(&format!("Total cost: ${:.4}\n", summary.total_cost));
            output.push_str(&format!("Total tokens: {}\n", summary.total_tokens));
            output.push_str(&format!("Sessions: {}\n", summary.session_count));
            output.push_str(&format!("Requests: {}\n", summary.log_count));
            output.push_str(&format!("Database size: {}\n", summary.db_size_human));
        }

        if let Ok(models) = self.get_usage_by_model() {
            if !models.is_empty() {
                output.push_str("\n=== Usage by Model ===\n");
                for model in models.iter().take(5) {
                    output.push_str(&format!(
                        "{}/{}: {} tokens, ${:.4}\n",
                        model.provider,
                        model.model,
                        model.input_tokens + model.output_tokens,
                        model.cost
                    ));
                }
            }
        }

        output
    }

    /// Get context usage as a percentage (0.0 - 1.0)
    pub fn context_usage_percent(&self, max_tokens: u32) -> f32 {
        if max_tokens == 0 {
            0.0
        } else {
            (self.total_tokens() as f32) / (max_tokens as f32)
        }
    }

    /// Get max tokens for current model
    ///
    /// Uses cached capabilities from models.dev if available.
    pub fn max_tokens_for_model(&self) -> u32 {
        // Try cached capabilities first
        if let Some(ref caps) = self.cached_capabilities {
            if caps.context_limit > 0 {
                return caps.context_limit;
            }
        }

        // Fallback to hardcoded values
        Self::fallback_context_limit(&self.provider, &self.model)
    }

    /// Fallback context limit when models.dev data isn't available
    fn fallback_context_limit(provider: &str, model: &str) -> u32 {
        match (provider.to_lowercase().as_str(), model) {
            ("openai" | "gpt", m) if m.contains("gpt-4o") => 128_000,
            ("openai" | "gpt", m) if m.contains("gpt-4-turbo") => 128_000,
            ("openai" | "gpt", m) if m.contains("gpt-4") => 8_192,
            ("openai" | "gpt", m) if m.contains("gpt-3.5") => 16_385,
            ("openai" | "gpt", m) if m.contains("o1") || m.contains("o3") => 200_000,
            ("anthropic" | "claude", _) => 200_000,
            ("ollama" | "local", _) => 32_000,
            _ => 128_000,
        }
    }

    /// Check if current model supports tool calling
    pub fn supports_tools(&self) -> bool {
        if let Some(ref caps) = self.cached_capabilities {
            return caps.tool_call;
        }
        // Assume true for major providers
        !matches!(self.provider.to_lowercase().as_str(), "ollama" | "local")
    }

    /// Check if current model supports reasoning/thinking mode
    pub fn supports_reasoning(&self) -> bool {
        if let Some(ref caps) = self.cached_capabilities {
            return caps.reasoning;
        }
        // Check known reasoning models
        self.model.contains("o1")
            || self.model.contains("o3")
            || self.model.contains("deepseek-r1")
            || self.model.contains("thinking")
    }

    /// Check if current model supports vision
    pub fn supports_vision(&self) -> bool {
        if let Some(ref caps) = self.cached_capabilities {
            return caps.vision;
        }
        // Fallback check
        self.model.contains("vision")
            || self.model.contains("4o")
            || self.model.contains("claude-3")
            || self.model.contains("gemini")
            || self.model.contains("llava")
    }

    /// Check if current model supports prompt caching
    pub fn supports_caching(&self) -> bool {
        if let Some(ref caps) = self.cached_capabilities {
            return caps.supports_caching;
        }
        // Known models with caching support
        self.provider.to_lowercase() == "anthropic" || self.model.contains("gpt-4")
    }

    /// Get a capability summary string for display
    pub fn capability_summary(&self) -> String {
        if let Some(ref caps) = self.cached_capabilities {
            caps.summary()
        } else {
            let mut caps = Vec::new();
            if self.supports_tools() {
                caps.push("tools");
            }
            if self.supports_reasoning() {
                caps.push("reasoning");
            }
            if self.supports_vision() {
                caps.push("vision");
            }
            if caps.is_empty() {
                "text".to_string()
            } else {
                caps.join(", ")
            }
        }
    }

    /// Get pricing info string for display
    pub fn pricing_info(&self) -> String {
        if let Some(ref caps) = self.cached_capabilities {
            caps.cost_string()
        } else {
            let (input, output) = match self.provider.to_lowercase().as_str() {
                "ollama" | "local" => return "free".to_string(),
                "openai" | "gpt" => (2.5, 10.0),
                "anthropic" | "claude" => (3.0, 15.0),
                _ => return "unknown".to_string(),
            };
            format!("${:.2}/${:.2} per 1M tokens", input, output)
        }
    }

    /// Reset current session counters
    pub fn reset_session_counters(&mut self) {
        self.current_input_tokens = 0;
        self.current_output_tokens = 0;
        self.current_cost = 0.0;
    }
}

/// Usage display info for the status bar
#[derive(Debug, Clone)]
pub struct UsageDisplayInfo {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub total_tokens: u32,
    pub max_tokens: u32,
    pub usage_percent: f32,
    pub cost: f64,
    pub provider: String,
    pub model: String,
}

impl From<&UsageManager> for UsageDisplayInfo {
    fn from(manager: &UsageManager) -> Self {
        let max_tokens = manager.max_tokens_for_model();
        Self {
            input_tokens: manager.input_tokens(),
            output_tokens: manager.output_tokens(),
            total_tokens: manager.total_tokens(),
            max_tokens,
            usage_percent: manager.context_usage_percent(max_tokens),
            cost: manager.cost(),
            provider: manager.provider().to_string(),
            model: manager.model().to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usage_manager_new() {
        let manager = UsageManager::new();
        assert_eq!(manager.input_tokens(), 0);
        assert_eq!(manager.output_tokens(), 0);
        assert_eq!(manager.cost(), 0.0);
        assert_eq!(manager.provider(), "openai");
        assert_eq!(manager.model(), "gpt-4o");
    }

    #[test]
    fn test_usage_manager_set_provider_model() {
        let mut manager = UsageManager::new();
        manager.set_provider("anthropic");
        manager.set_model("claude-3-sonnet");
        assert_eq!(manager.provider(), "anthropic");
        assert_eq!(manager.model(), "claude-3-sonnet");
    }

    #[test]
    fn test_usage_manager_calculate_cost_openai() {
        let _manager = UsageManager::new(); // Default is openai/gpt-4o
        let cost = UsageManager::fallback_cost("openai", "gpt-4o", 1000, 500);
        // gpt-4o: $2.5/M input, $10/M output
        // Expected: (1000 * 2.5 + 500 * 10) / 1_000_000 = 0.0075
        assert!((cost - 0.0075).abs() < 0.0001);
    }

    #[test]
    fn test_usage_manager_calculate_cost_ollama() {
        let cost = UsageManager::fallback_cost("ollama", "llama3", 10000, 5000);
        assert_eq!(cost, 0.0); // Ollama is free
    }

    #[test]
    fn test_usage_manager_log_usage_no_tracker() {
        let mut manager = UsageManager::new();
        manager.session_id = Some("test_session".to_string());

        // Should not fail even without tracker
        manager.log_usage(100, 50, "build", "chat").unwrap();

        assert_eq!(manager.input_tokens(), 100);
        assert_eq!(manager.output_tokens(), 50);
        assert_eq!(manager.total_tokens(), 150);
        assert!(manager.cost() > 0.0);
    }

    #[test]
    fn test_usage_manager_context_usage_percent() {
        let mut manager = UsageManager::new();
        manager.current_input_tokens = 64000;
        manager.current_output_tokens = 0;

        // gpt-4o has 128K context
        let percent = manager.context_usage_percent(128000);
        assert!((percent - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_usage_manager_max_tokens() {
        let mut manager = UsageManager::new();

        // OpenAI gpt-4o
        assert_eq!(manager.max_tokens_for_model(), 128_000);

        // Anthropic Claude
        manager.set_provider("anthropic");
        manager.set_model("claude-3-sonnet");
        assert_eq!(manager.max_tokens_for_model(), 200_000);

        // Ollama
        manager.set_provider("ollama");
        manager.set_model("llama3");
        assert_eq!(manager.max_tokens_for_model(), 32_000);
    }

    #[test]
    fn test_usage_manager_reset_counters() {
        let mut manager = UsageManager::new();
        manager.current_input_tokens = 1000;
        manager.current_output_tokens = 500;
        manager.current_cost = 0.05;

        manager.reset_session_counters();

        assert_eq!(manager.input_tokens(), 0);
        assert_eq!(manager.output_tokens(), 0);
        assert_eq!(manager.cost(), 0.0);
    }

    #[test]
    fn test_usage_display_info_from_manager() {
        let mut manager = UsageManager::new();
        manager.current_input_tokens = 1000;
        manager.current_output_tokens = 500;
        manager.current_cost = 0.0125;

        let info = UsageDisplayInfo::from(&manager);

        assert_eq!(info.input_tokens, 1000);
        assert_eq!(info.output_tokens, 500);
        assert_eq!(info.total_tokens, 1500);
        assert_eq!(info.max_tokens, 128_000);
        assert_eq!(info.cost, 0.0125);
        assert_eq!(info.provider, "openai");
        assert_eq!(info.model, "gpt-4o");
    }
}

/// Property-based tests for usage persistence
///
/// **Property 13: Usage Data Persistence**
/// **Validates: Requirements 18.1, 18.4**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Generate a random provider name
    fn arb_provider() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("openai".to_string()),
            Just("anthropic".to_string()),
            Just("ollama".to_string()),
        ]
    }

    /// Generate a random model name
    fn arb_model() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("gpt-4o".to_string()),
            Just("gpt-4o-mini".to_string()),
            Just("claude-3-sonnet".to_string()),
            Just("llama3".to_string()),
        ]
    }

    /// Generate a random mode
    fn arb_mode() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("build".to_string()),
            Just("plan".to_string()),
            Just("review".to_string()),
        ]
    }

    /// Generate a random request type
    fn arb_request_type() -> impl Strategy<Value = String> {
        prop_oneof![Just("chat".to_string()), Just("fim".to_string()),]
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any valid token usage, logging it SHALL update the current session
        /// totals correctly.
        #[test]
        fn prop_log_usage_updates_totals(
            input_tokens in 0u32..100_000,
            output_tokens in 0u32..50_000,
            mode in arb_mode(),
            request_type in arb_request_type(),
        ) {
            let mut manager = UsageManager::new();
            manager.session_id = Some("test_session".to_string());

            let initial_input = manager.input_tokens();
            let initial_output = manager.output_tokens();
            let initial_cost = manager.cost();

            // Log usage
            manager.log_usage(input_tokens, output_tokens, &mode, &request_type).unwrap();

            // Verify totals were updated
            prop_assert_eq!(
                manager.input_tokens(),
                initial_input + input_tokens,
                "Input tokens should be accumulated"
            );
            prop_assert_eq!(
                manager.output_tokens(),
                initial_output + output_tokens,
                "Output tokens should be accumulated"
            );
            prop_assert!(
                manager.cost() >= initial_cost,
                "Cost should not decrease"
            );
            prop_assert_eq!(
                manager.total_tokens(),
                manager.input_tokens() + manager.output_tokens(),
                "Total tokens should equal input + output"
            );
        }

        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any sequence of usage logs, the totals SHALL be the sum of all
        /// individual logs.
        #[test]
        fn prop_usage_totals_are_cumulative(
            logs in prop::collection::vec(
                (0u32..10_000, 0u32..5_000),
                1..10
            ),
        ) {
            let mut manager = UsageManager::new();
            manager.session_id = Some("test_session".to_string());

            let expected_input: u32 = logs.iter().map(|(i, _)| i).sum();
            let expected_output: u32 = logs.iter().map(|(_, o)| o).sum();

            // Log all usage
            for (input, output) in logs {
                manager.log_usage(input, output, "build", "chat").unwrap();
            }

            prop_assert_eq!(
                manager.input_tokens(),
                expected_input,
                "Total input tokens should be sum of all logs"
            );
            prop_assert_eq!(
                manager.output_tokens(),
                expected_output,
                "Total output tokens should be sum of all logs"
            );
        }

        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any provider/model combination, the cost calculation SHALL be
        /// non-negative and consistent.
        #[test]
        fn prop_cost_calculation_is_consistent(
            provider in arb_provider(),
            model in arb_model(),
            input_tokens in 0u32..1_000_000,
            output_tokens in 0u32..500_000,
        ) {
            let mut manager = UsageManager::new();
            manager.set_provider(&provider);
            manager.set_model(&model);

            let cost = manager.calculate_cost(input_tokens, output_tokens);

            // Cost should be non-negative
            prop_assert!(cost >= 0.0, "Cost should be non-negative");

            // Cost should be zero for Ollama (free)
            if provider == "ollama" {
                prop_assert_eq!(cost, 0.0, "Ollama should be free");
            }

            // Cost should scale with tokens
            if cost > 0.0 {
                let double_cost = manager.calculate_cost(input_tokens * 2, output_tokens * 2);
                prop_assert!(
                    (double_cost - cost * 2.0).abs() < 0.0001,
                    "Cost should scale linearly with tokens"
                );
            }
        }

        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any model, the max tokens value SHALL be positive and reasonable.
        #[test]
        fn prop_max_tokens_is_valid(
            provider in arb_provider(),
            model in arb_model(),
        ) {
            let mut manager = UsageManager::new();
            manager.set_provider(&provider);
            manager.set_model(&model);

            let max_tokens = manager.max_tokens_for_model();

            // Max tokens should be positive
            prop_assert!(max_tokens > 0, "Max tokens should be positive");

            // Max tokens should be reasonable (between 1K and 1M)
            prop_assert!(
                (1_000..=1_000_000).contains(&max_tokens),
                "Max tokens should be between 1K and 1M, got {}",
                max_tokens
            );
        }

        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any usage, the context usage percentage SHALL be between 0 and 1
        /// (or slightly above 1 if over limit).
        #[test]
        fn prop_context_usage_percent_is_valid(
            input_tokens in 0u32..200_000,
            output_tokens in 0u32..100_000,
            max_tokens in 1u32..500_000,
        ) {
            let mut manager = UsageManager::new();
            manager.current_input_tokens = input_tokens;
            manager.current_output_tokens = output_tokens;

            let percent = manager.context_usage_percent(max_tokens);

            // Percent should be non-negative
            prop_assert!(percent >= 0.0, "Usage percent should be non-negative");

            // Percent should match expected calculation
            let expected = (input_tokens + output_tokens) as f32 / max_tokens as f32;
            prop_assert!(
                (percent - expected).abs() < 0.0001,
                "Usage percent should match expected calculation"
            );
        }

        /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
        /// **Validates: Requirements 18.1, 18.4**
        ///
        /// For any usage manager, resetting counters SHALL set all values to zero.
        #[test]
        fn prop_reset_clears_all_counters(
            input_tokens in 0u32..100_000,
            output_tokens in 0u32..50_000,
            cost in 0.0f64..100.0,
        ) {
            let mut manager = UsageManager::new();
            manager.current_input_tokens = input_tokens;
            manager.current_output_tokens = output_tokens;
            manager.current_cost = cost;

            manager.reset_session_counters();

            prop_assert_eq!(manager.input_tokens(), 0, "Input tokens should be 0 after reset");
            prop_assert_eq!(manager.output_tokens(), 0, "Output tokens should be 0 after reset");
            prop_assert_eq!(manager.cost(), 0.0, "Cost should be 0 after reset");
            prop_assert_eq!(manager.total_tokens(), 0, "Total tokens should be 0 after reset");
        }
    }

    /// **Feature: terminal-tui-chat, Property 13: Usage Data Persistence**
    /// **Validates: Requirements 18.1, 18.4**
    ///
    /// Test that usage is persisted to the database and retrievable.
    #[test]
    fn test_usage_persistence_with_storage() {
        let tmp = TempDir::new().unwrap();
        let tark_dir = tmp.path().join(".tark");
        std::fs::create_dir_all(&tark_dir).unwrap();

        let mut manager = UsageManager::new().with_storage(tmp.path()).unwrap();

        // Start a session
        let session_id = manager
            .start_session("test-host", "test-user", Some("test-project"))
            .unwrap();
        assert!(!session_id.is_empty());

        // Log some usage
        manager.log_usage(1000, 500, "build", "chat").unwrap();
        manager.log_usage(2000, 1000, "plan", "chat").unwrap();

        // Verify totals
        assert_eq!(manager.input_tokens(), 3000);
        assert_eq!(manager.output_tokens(), 1500);
        assert_eq!(manager.total_tokens(), 4500);

        // Verify summary is retrievable
        let summary = manager.get_summary().unwrap();
        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert_eq!(summary.total_tokens, 4500);
        assert_eq!(summary.log_count, 2);
    }
}
