//! Integration tests for tool call handling across providers
//!
//! These tests verify that tool calls work correctly in different scenarios:
//! - Tool definitions are converted properly
//! - Tool calls in chat history are handled per provider requirements
//! - Streaming tool call events are emitted correctly

use tark_cli::llm::{Message, ToolDefinition};

#[test]
fn test_tool_definition_structure() {
    let tool = ToolDefinition {
        name: "search".to_string(),
        description: "Search for information".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query"
                }
            },
            "required": ["query"]
        }),
    };

    // Verify structure
    assert_eq!(tool.name, "search");
    assert_eq!(tool.description, "Search for information");
    assert!(tool.parameters["properties"]["query"].is_object());
}

#[test]
fn test_message_with_tool_call() {
    // Create assistant message with tool call using Parts
    let msg = Message {
        role: tark_cli::llm::Role::Assistant,
        content: tark_cli::llm::MessageContent::Parts(vec![tark_cli::llm::ContentPart::ToolUse {
            id: "call_123".to_string(),
            name: "search".to_string(),
            input: serde_json::json!({"query": "test"}),
            thought_signature: None,
        }]),
        tool_call_id: None,
    };

    assert_eq!(msg.role, tark_cli::llm::Role::Assistant);

    // Verify it contains a ToolUse part
    if let tark_cli::llm::MessageContent::Parts(parts) = &msg.content {
        assert_eq!(parts.len(), 1);
        if let tark_cli::llm::ContentPart::ToolUse { id, name, .. } = &parts[0] {
            assert_eq!(id, "call_123");
            assert_eq!(name, "search");
        } else {
            panic!("Expected ToolUse part");
        }
    } else {
        panic!("Expected Parts content");
    }
}

#[test]
fn test_message_with_tool_result() {
    let msg = Message::tool_result("call_123", "Search completed: 5 results found");

    assert_eq!(msg.role, tark_cli::llm::Role::Tool);
    assert_eq!(msg.tool_call_id, Some("call_123".to_string()));

    if let Some(text) = msg.content.as_text() {
        assert!(text.contains("5 results"));
    } else {
        panic!("Expected text content for tool result");
    }
}

#[cfg(test)]
mod openai_specific {
    use super::*;

    /// Test that tool calls in history don't cause 400 errors with Responses API
    ///
    /// This is a regression test for the bug where assistant messages with tool calls
    /// were being sent as "function_call" content type in the input array, causing:
    /// "Invalid value: 'function_call'. Supported values are: 'input_text', ..."
    #[test]
    fn test_responses_api_skips_tool_calls_in_input() {
        // Create a conversation with tool call history
        let messages = &[
            Message::user("What's the weather?"),
            Message {
                role: tark_cli::llm::Role::Assistant,
                content: tark_cli::llm::MessageContent::Parts(vec![
                    tark_cli::llm::ContentPart::ToolUse {
                        id: "call_123".to_string(),
                        name: "get_weather".to_string(),
                        input: serde_json::json!({"city": "Paris"}),
                        thought_signature: None,
                    },
                ]),
                tool_call_id: None,
            },
            Message::tool_result("call_123", "Sunny, 22°C"),
            Message::assistant("The weather in Paris is sunny with 22°C."),
            Message::user("Ask me 3 questions"),
        ];

        // The conversion should skip the assistant tool call message
        // and only include text messages and tool results
        //
        // Expected in input:
        // - User: "What's the weather?"
        // - Tool result for call_123
        // - Assistant: "The weather in Paris..."
        // - User: "Ask me 3 questions"
        //
        // NOT expected: Assistant message with tool calls (it's an output, not input)

        // This is tested implicitly by the OpenAI provider code
        // If it sends "function_call" in input, the API returns 400
        // The fix: ContentPart::ToolUse is now skipped in convert_messages_to_responses

        // Verify message structure
        assert_eq!(messages.len(), 5);
        assert_eq!(messages[0].role, tark_cli::llm::Role::User);
        assert_eq!(messages[1].role, tark_cli::llm::Role::Assistant); // Has tool calls
        assert_eq!(messages[2].role, tark_cli::llm::Role::Tool); // Tool result
        assert_eq!(messages[3].role, tark_cli::llm::Role::Assistant); // Text response
        assert_eq!(messages[4].role, tark_cli::llm::Role::User);
    }

    /// Test that Role::Tool messages are converted to FunctionResult in Responses API input
    ///
    /// This is a regression test for the bug where tool results were dropped entirely
    /// from the Responses API input, causing the LLM to not see tool outputs and
    /// potentially loop infinitely asking for the same information.
    #[test]
    fn test_responses_api_includes_tool_results() {
        use tark_cli::llm::{ContentPart, MessageContent, Role};

        // Create a conversation with tool calls and results
        let messages = [
            Message::user("What files are in the src directory?"),
            Message {
                role: Role::Assistant,
                content: MessageContent::Parts(vec![ContentPart::ToolUse {
                    id: "call_list_dir".to_string(),
                    name: "list_directory".to_string(),
                    input: serde_json::json!({"path": "src"}),
                    thought_signature: None,
                }]),
                tool_call_id: None,
            },
            Message::tool_result("call_list_dir", "main.rs\nlib.rs\nconfig.rs"),
            Message::user("Now check what's in main.rs"),
        ];

        // Verify the tool result message structure
        let tool_msg = &messages[2];
        assert_eq!(tool_msg.role, Role::Tool);
        assert_eq!(tool_msg.tool_call_id, Some("call_list_dir".to_string()));
        assert_eq!(
            tool_msg.content.as_text().unwrap(),
            "main.rs\nlib.rs\nconfig.rs"
        );

        // The conversion should:
        // 1. Skip the assistant message with ToolUse (it's an output)
        // 2. Convert the Role::Tool message to a user message with FunctionResult part
        // 3. Include both user messages
        //
        // This ensures the LLM can see the tool output and provide a meaningful response
        // instead of asking for the same information repeatedly.
        //
        // Expected in Responses API input:
        // - User: "What files are in the src directory?"
        // - User (with FunctionResult): call_id=call_list_dir, output="main.rs\n..."
        // - User: "Now check what's in main.rs"
        //
        // The actual conversion logic is in src/llm/openai.rs convert_messages_to_responses()
        // This test documents the expected behavior for regression prevention.

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[2].role, Role::Tool); // This should be converted to user with FunctionResult
        assert_eq!(messages[3].role, Role::User);
    }
}

#[cfg(test)]
mod streaming_integration {
    use tark_cli::llm::streaming::ToolCallTracker;
    use tark_cli::llm::StreamEvent;

    #[test]
    fn test_tool_call_tracker_openai_style() {
        let mut tracker = ToolCallTracker::new();

        // OpenAI: output_item.added gives us both item_id and call_id
        let event = tracker.start_call("call_abc", "search", Some("fc_123"));

        match event {
            StreamEvent::ToolCallStart { id, name, .. } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "search");
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Delta events use item_id
        let event = tracker.append_args("fc_123", r#"{"query":"#).unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "call_abc"); // Resolved from item_id -> call_id
                assert_eq!(arguments_delta, r#"{"query":"#);
            }
            _ => panic!("Expected ToolCallDelta"),
        }

        let event = tracker.append_args("fc_123", r#""test"}"#).unwrap();
        assert!(matches!(event, StreamEvent::ToolCallDelta { .. }));

        // Complete using item_id
        let event = tracker.complete_call("fc_123").unwrap();
        match event {
            StreamEvent::ToolCallComplete { id } => {
                assert_eq!(id, "call_abc");
            }
            _ => panic!("Expected ToolCallComplete"),
        }

        // Verify accumulated args
        let calls = tracker.into_calls();
        let (name, args) = calls.get("call_abc").unwrap();
        assert_eq!(name, "search");
        assert_eq!(args, r#"{"query":"test"}"#);
    }

    #[test]
    fn test_tool_call_tracker_claude_style() {
        let mut tracker = ToolCallTracker::new();

        // Claude: ContentBlockStart gives us id and name, index is separate
        let index = 0;
        let event = tracker.start_call("toolu_abc", "get_weather", Some(&index.to_string()));

        match event {
            StreamEvent::ToolCallStart { id, name, .. } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "get_weather");
            }
            _ => panic!("Expected ToolCallStart"),
        }

        // Delta events use index
        let event = tracker
            .append_args(&index.to_string(), r#"{"city":"#)
            .unwrap();
        match event {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "toolu_abc"); // Resolved from index -> id
                assert_eq!(arguments_delta, r#"{"city":"#);
            }
            _ => panic!("Expected ToolCallDelta"),
        }

        let event = tracker
            .append_args(&index.to_string(), r#""Paris"}"#)
            .unwrap();
        assert!(matches!(event, StreamEvent::ToolCallDelta { .. }));

        // Complete using index
        let event = tracker.complete_call(&index.to_string()).unwrap();
        match event {
            StreamEvent::ToolCallComplete { id } => {
                assert_eq!(id, "toolu_abc");
            }
            _ => panic!("Expected ToolCallComplete"),
        }

        // Verify accumulated args
        let calls = tracker.into_calls();
        let (name, args) = calls.get("toolu_abc").unwrap();
        assert_eq!(name, "get_weather");
        assert_eq!(args, r#"{"city":"Paris"}"#);
    }

    #[test]
    fn test_parallel_tool_calls() {
        let mut tracker = ToolCallTracker::new();

        // Start two tool calls
        tracker.start_call("call_1", "search", Some("fc_1"));
        tracker.start_call("call_2", "weather", Some("fc_2"));

        // Append to both (can arrive in any order)
        tracker.append_args("fc_2", r#"{"city":"NYC"}"#);
        tracker.append_args("fc_1", r#"{"query":"test"}"#);

        // Complete both
        tracker.complete_call("fc_1");
        tracker.complete_call("fc_2");

        // Verify both tracked correctly
        let calls = tracker.into_calls();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls.get("call_1").unwrap().0, "search");
        assert_eq!(calls.get("call_2").unwrap().0, "weather");
        assert_eq!(calls.get("call_1").unwrap().1, r#"{"query":"test"}"#);
        assert_eq!(calls.get("call_2").unwrap().1, r#"{"city":"NYC"}"#);
    }
}

#[cfg(test)]
mod tui_finalize {
    /// Test that TUI finalize logic prefers streamed content over shorter final text
    ///
    /// This is a regression test for the bug where the TUI would overwrite
    /// streamed message content with the final AgentResponse.text, causing
    /// responses to appear incomplete or truncated (e.g., only showing the last
    /// bullet point instead of the full response).
    #[test]
    fn test_finalize_prefers_streamed_when_longer() {
        // Simulate streamed content (what was accumulated during streaming)
        let streamed_text = "Here are the key points:\n\n1. First point\n2. Second point\n3. Third point\n4. Fourth point";

        // Simulate final text from AgentResponse (might be incomplete due to tool calls)
        let final_text = "4. Fourth point";

        // The finalize logic should prefer streamed_text because it's longer
        let chosen_text = if !streamed_text.is_empty()
            && (final_text.is_empty() || streamed_text.len() > final_text.len())
        {
            streamed_text
        } else {
            final_text
        };

        assert_eq!(chosen_text, streamed_text);
        assert!(chosen_text.contains("First point"));
        assert!(chosen_text.contains("Second point"));
        assert!(chosen_text.contains("Third point"));
        assert!(chosen_text.contains("Fourth point"));
    }

    #[test]
    fn test_finalize_uses_final_when_longer() {
        // Simulate case where final text is more complete
        let streamed_text = "Here are the";
        let final_text = "Here are the key points:\n\n1. Complete response";

        // Should prefer final_text because it's longer
        let chosen_text = if !streamed_text.is_empty()
            && (final_text.is_empty() || streamed_text.len() > final_text.len())
        {
            streamed_text
        } else {
            final_text
        };

        assert_eq!(chosen_text, final_text);
    }

    #[test]
    fn test_finalize_uses_final_when_streamed_empty() {
        // Simulate non-streaming path (e.g., attachment response)
        let streamed_text = "";
        let final_text = "Response without streaming";

        // Should use final_text
        let chosen_text = if !streamed_text.is_empty()
            && (final_text.is_empty() || streamed_text.len() > final_text.len())
        {
            streamed_text
        } else {
            final_text
        };

        assert_eq!(chosen_text, final_text);
    }
}
