//! Verifies system prompt hasn't changed unexpectedly.
//! Update EXPECTED_HASH when intentionally changing prompts.
//!
//! Note: This is a placeholder test. To make it functional, you would need to:
//! 1. Export get_system_prompt() from agent::chat module, or
//! 2. Add a public context() method to ChatAgent, or
//! 3. Refactor to test at a different level

#[cfg(feature = "test-sim")]
#[test]
fn tark_sim_provider_compiles() {
    use tark_cli::llm::{LlmProvider, TarkSimProvider};

    // Test that TarkSimProvider can be instantiated with test-sim feature
    let provider = TarkSimProvider::new();
    assert_eq!(provider.name(), "tark_sim");
    assert!(provider.supports_streaming());

    println!("✓ TarkSimProvider compiles and instantiates successfully");
}

#[cfg(not(feature = "test-sim"))]
#[test]
fn system_prompt_hash_test_requires_test_sim_feature() {
    // This test requires the test-sim feature to be enabled
    // Run with: cargo test --features test-sim prompt_hash
}
