//! Integration tests for TarkSimProvider

#[cfg(feature = "test-sim")]
#[tokio::test]
async fn tark_sim_provider_registration() {
    use tark_cli::llm;

    // Test that tark_sim provider can be created
    let result = llm::create_provider_with_options("tark_sim", true, None);
    assert!(result.is_ok(), "Should create tark_sim provider");

    let provider = result.unwrap();
    assert_eq!(provider.name(), "tark_sim");
}

#[cfg(feature = "test-sim")]
#[tokio::test]
async fn tark_sim_echo_response() {
    use tark_cli::llm::{LlmProvider, Message, TarkSimProvider};

    let provider = TarkSimProvider::new();

    let messages = vec![Message::user("Hello, world!")];

    let response = provider.chat(&messages, None).await;
    assert!(response.is_ok(), "Should get successful response");

    let response = response.unwrap();
    if let Some(text) = response.text() {
        assert!(text.contains("Echo"), "Should echo the message");
        assert!(
            text.contains("Hello, world!"),
            "Should contain original message"
        );
    } else {
        panic!("Expected text response, got: {:?}", response);
    }
}

#[cfg(feature = "test-sim")]
#[tokio::test]
async fn tark_sim_context_snapshot() {
    use tark_cli::llm::{LlmProvider, Message, TarkSimProvider, ToolDefinition};

    let provider = TarkSimProvider::new();

    let messages = vec![
        Message::system("You are a helpful assistant"),
        Message::user("What is 2+2?"),
    ];

    let tools = vec![ToolDefinition {
        name: "grep".to_string(),
        description: "Search files".to_string(),
        parameters: serde_json::json!({}),
    }];

    let _response = provider.chat(&messages, Some(&tools)).await;

    // Verify context snapshot was captured
    let snapshot = provider.get_last_context();
    assert!(snapshot.is_some(), "Should capture context snapshot");

    let snapshot = snapshot.unwrap();
    assert_eq!(snapshot.message_count, 2);
    assert_eq!(snapshot.user_message_count, 1);
    assert!(snapshot.system_prompt.is_some());
    assert_eq!(snapshot.tools_in_context.len(), 1);
    assert_eq!(snapshot.tools_in_context[0], "grep");
}

#[cfg(not(feature = "test-sim"))]
#[test]
fn tark_sim_requires_test_sim_feature() {
    // These tests require the test-sim feature
    // Run with: cargo test --features test-sim
}
