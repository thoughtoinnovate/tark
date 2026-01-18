//! End-to-end tests for TarkSimProvider without TUI
//! These tests verify the provider works in actual chat scenarios

#[cfg(feature = "test-sim")]
mod e2e_tests {
    use std::sync::Arc;
    use tark_cli::llm::{LlmProvider, Message, TarkSimProvider};

    #[tokio::test]
    async fn test_tark_sim_full_conversation() {
        // Simulate a full conversation flow
        let provider = Arc::new(TarkSimProvider::new());

        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("Hello, can you help me?"),
        ];

        let response = provider.chat(&messages, None).await;
        assert!(response.is_ok(), "Should get response");

        let response = response.unwrap();
        if let Some(text) = response.text() {
            assert!(
                text.contains("Hello") || text.contains("Echo"),
                "Response should contain echo or original message. Got: {}",
                text
            );
            println!("✓ Full conversation works E2E");
            println!("Response: {}", text);
        } else {
            panic!("Expected text response");
        }
    }

    #[tokio::test]
    async fn test_tark_sim_context_capture_e2e() {
        let provider = Arc::new(TarkSimProvider::new());

        // Simulate multiple messages
        let messages = vec![
            Message::system("You are a helpful assistant"),
            Message::user("First message"),
            Message::assistant("First response"),
            Message::user("Second message"),
        ];

        let _response = provider.chat(&messages, None).await;

        // Verify context was captured
        let snapshot = provider.get_last_context();
        assert!(snapshot.is_some(), "Should capture context");

        let snapshot = snapshot.unwrap();
        assert_eq!(snapshot.message_count, 4, "Should have 4 messages");
        assert_eq!(
            snapshot.user_message_count, 2,
            "Should have 2 user messages"
        );
        assert_eq!(
            snapshot.assistant_message_count, 1,
            "Should have 1 assistant message"
        );
        assert!(snapshot.total_tokens_in_context > 0, "Should count tokens");

        println!("✓ Context capture works E2E");
        println!("Snapshot: {:?}", snapshot);
    }

    #[tokio::test]
    async fn test_tark_sim_streaming_e2e() {
        use std::sync::Arc as StdArc;
        use std::sync::Mutex;
        use tark_cli::llm::{tark_sim::SimScenario, StreamEvent};

        // Use Streaming scenario to actually test streaming
        let provider = TarkSimProvider::new().with_scenario(SimScenario::Streaming {
            text: "This is a streamed response.".to_string(),
            chunk_size: 5,
            delay_ms: 10,
        });

        let messages = vec![Message::user("Test streaming")];

        // Capture streaming events
        let events = StdArc::new(Mutex::new(Vec::new()));
        let events_clone = events.clone();

        let callback = Box::new(move |event: StreamEvent| {
            events_clone.lock().unwrap().push(format!("{:?}", event));
        });

        let response = provider
            .chat_streaming(&messages, None, callback, None)
            .await;

        assert!(response.is_ok(), "Streaming should succeed");

        let captured_events = events.lock().unwrap();
        assert!(!captured_events.is_empty(), "Should have streaming events");

        // Should have multiple TextDelta events + Done
        let text_deltas = captured_events
            .iter()
            .filter(|e| e.contains("TextDelta"))
            .count();
        assert!(text_deltas > 1, "Should have multiple text chunks");

        println!("✓ Streaming works E2E");
        println!(
            "Events captured: {} (text chunks: {})",
            captured_events.len(),
            text_deltas
        );
    }

    #[tokio::test]
    async fn test_tark_sim_event_logging() {
        let provider = Arc::new(TarkSimProvider::new());

        let messages = vec![Message::user("Test logging")];
        let _response = provider.chat(&messages, None).await;

        let events = provider.get_events();
        assert!(!events.is_empty(), "Should log events");

        // Should have at least ContextCaptured event
        let has_context_event = events.iter().any(|e| {
            matches!(
                e.event_type,
                tark_cli::llm::tark_sim::SimEventType::ContextCaptured
            )
        });
        assert!(has_context_event, "Should have ContextCaptured event");

        println!("✓ Event logging works E2E");
        println!("Events logged: {}", events.len());
    }

    #[tokio::test]
    async fn test_tark_sim_token_usage() {
        let provider = TarkSimProvider::new();
        let messages = vec![Message::user("Test token counting")];

        let response = provider.chat(&messages, None).await;
        assert!(response.is_ok(), "Should get response");

        let response = response.unwrap();
        let usage = response.usage();

        assert!(usage.is_some(), "Should include token usage");
        let usage = usage.unwrap();

        assert!(usage.input_tokens > 0, "Should count input tokens");
        assert!(usage.output_tokens > 0, "Should count output tokens");
        assert_eq!(
            usage.total_tokens,
            usage.input_tokens + usage.output_tokens,
            "Total should equal sum"
        );

        println!("✓ Token usage tracking works E2E");
        println!(
            "Usage: in={}, out={}, total={}",
            usage.input_tokens, usage.output_tokens, usage.total_tokens
        );
    }
}

#[cfg(not(feature = "test-sim"))]
#[test]
fn tark_sim_e2e_requires_test_sim_feature() {
    // These tests require the test-sim feature
    // Run with: cargo test --features test-sim
}
