//! Live Ollama end-to-end (opt-in, heavy).
//!
//! Runs ONLY when `OLLAMA_TEST_URL` is set (e.g. the `ollama-live`
//! workflow, or a dev machine with `ollama serve` running); otherwise each
//! test passes immediately without doing anything. This keeps per-push CI
//! free of model downloads while still giving real-model coverage on
//! demand (manual dispatch, schedule, or Ollama-code changes).
//!
//! Assertions check shapes (text vs tool calls, tool names), never exact
//! model strings: real-model output is nondeterministic.

use std::sync::{Arc, Mutex};
use tark_cli::llm::{LlmProvider, Message, MessageContent, OllamaProvider, Role, ToolDefinition};

/// (base_url, model) when live testing is requested, else None.
fn live_target() -> Option<(String, String)> {
    let url = std::env::var("OLLAMA_TEST_URL").ok()?;
    if url.trim().is_empty() {
        return None;
    }
    let model = std::env::var("OLLAMA_TEST_MODEL")
        .ok()
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| "qwen2.5:1.5b".to_string());
    Some((url, model))
}

fn user_message(text: &str) -> Message {
    Message {
        role: Role::User,
        content: MessageContent::Text(text.to_string()),
        tool_call_id: None,
    }
}

fn weather_tool() -> ToolDefinition {
    ToolDefinition {
        name: "get_weather".to_string(),
        description: "Get weather for a city".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {"city": {"type": "string"}},
            "required": ["city"],
        }),
    }
}

/// Warmup + streaming chat with tools against a real model.
/// Accepts either plain text or exactly one `get_weather` tool call.
#[tokio::test]
async fn ollama_live_streaming_tools_shape() {
    let Some((base_url, model)) = live_target() else {
        eprintln!("SKIP: set OLLAMA_TEST_URL (and optionally OLLAMA_TEST_MODEL) for live tests");
        return;
    };
    std::env::set_var("OLLAMA_INITIAL_TIMEOUT", "600");
    std::env::set_var("OLLAMA_CHUNK_TIMEOUT", "180");
    let provider = OllamaProvider::new()
        .expect("provider")
        .with_base_url(&base_url)
        .with_model(&model);
    assert!(provider.is_available().await, "ollama not reachable");

    let collected = Arc::new(Mutex::new(String::new()));
    let sink = collected.clone();
    let callback: Box<dyn Fn(tark_cli::llm::StreamEvent) + Send + Sync> = Box::new(move |event| {
        if let tark_cli::llm::StreamEvent::TextDelta(delta) = event {
            sink.lock().unwrap().push_str(&delta);
        }
    });
    let response = provider
        .chat_streaming(
            &[user_message("What is the weather in Paris? Use the tool.")],
            Some(&[weather_tool()]),
            callback,
            None,
        )
        .await
        .expect("live streaming chat");

    let text = collected.lock().unwrap().clone();
    let calls = response.tool_calls();
    assert!(
        !text.is_empty() || calls.len() == 1,
        "expected streamed text or exactly one tool call"
    );
    if calls.len() == 1 {
        assert_eq!(calls[0].name, "get_weather");
    }
    std::env::remove_var("OLLAMA_INITIAL_TIMEOUT");
    std::env::remove_var("OLLAMA_CHUNK_TIMEOUT");
}
