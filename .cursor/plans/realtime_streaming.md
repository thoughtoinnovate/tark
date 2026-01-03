# Real-Time Streaming Implementation Plan

## Overview

Implement Server-Sent Events (SSE) streaming for LLM providers to enable real-time text display in the TUI. This allows users to see AI responses as they're generated, token by token.

## Current Architecture

```
User Input --> ChatAgent --> LlmProvider.chat() --> Full Response --> UI
                                   |
                            (BLOCKS until complete)
```

## Target Architecture

```
User Input --> ChatAgent --> LlmProvider.chat_streaming() --> UI
                                   |
                                   +--> TextChunk("Hello") --> UI updates
                                   +--> TextChunk(" world") --> UI updates
                                   +--> ThinkingChunk("...") --> UI updates
                                   +--> Complete --> UI finalizes
```

## Implementation Tasks

### Phase 1: LLM Provider Streaming (Core)

#### 1.1 Add Streaming Callback Types to `src/llm/types.rs`

```rust
/// Callback for streaming text chunks
pub type StreamCallback = Box<dyn Fn(StreamEvent) + Send + Sync>;

/// Events emitted during streaming
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// Regular text chunk
    TextDelta(String),
    /// Thinking/reasoning content (Claude extended thinking, etc.)
    ThinkingDelta(String),
    /// Tool call started
    ToolCallStart { id: String, name: String },
    /// Tool call arguments chunk
    ToolCallDelta { id: String, arguments_delta: String },
    /// Stream complete
    Done,
    /// Error during streaming
    Error(String),
}
```

#### 1.2 Add Streaming Method to `src/llm/mod.rs` LlmProvider Trait

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    // ... existing methods ...
    
    /// Send a streaming chat completion request
    /// The callback is called for each chunk as it arrives
    async fn chat_streaming(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        callback: StreamCallback,
    ) -> Result<LlmResponse>;
}
```

#### 1.3 Implement Streaming in `src/llm/openai.rs`

OpenAI streaming format:
```
data: {"choices":[{"delta":{"content":"Hello"}}]}
data: {"choices":[{"delta":{"content":" world"}}]}
data: [DONE]
```

Key changes:
- Add `stream: true` to request
- Use `reqwest::Response::bytes_stream()` for SSE parsing
- Parse each `data:` line and emit callbacks
- Handle tool_calls streaming (arguments come in chunks)

```rust
async fn chat_streaming(
    &self,
    messages: &[Message],
    tools: Option<&[ToolDefinition]>,
    callback: StreamCallback,
) -> Result<LlmResponse> {
    let mut request = self.build_request(messages, tools);
    request.stream = Some(true);
    
    let response = self.client
        .post(OPENAI_API_URL)
        .header("Authorization", format!("Bearer {}", self.api_key))
        .json(&request)
        .send()
        .await?;
    
    let mut stream = response.bytes_stream();
    let mut full_text = String::new();
    let mut tool_calls = Vec::new();
    
    while let Some(chunk) = stream.next().await {
        let bytes = chunk?;
        let line = String::from_utf8_lossy(&bytes);
        
        for data_line in line.lines() {
            if data_line.starts_with("data: ") {
                let json = &data_line[6..];
                if json == "[DONE]" {
                    callback(StreamEvent::Done);
                    break;
                }
                
                if let Ok(chunk) = serde_json::from_str::<StreamChunk>(json) {
                    if let Some(delta) = chunk.choices.first().and_then(|c| c.delta.content.as_ref()) {
                        full_text.push_str(delta);
                        callback(StreamEvent::TextDelta(delta.clone()));
                    }
                    // Handle tool_calls delta...
                }
            }
        }
    }
    
    // Return final response with accumulated content
    Ok(LlmResponse::Text { text: full_text, usage: None })
}
```

#### 1.4 Implement Streaming in `src/llm/claude.rs`

Claude streaming format (SSE):
```
event: content_block_delta
data: {"type":"content_block_delta","delta":{"type":"text_delta","text":"Hello"}}

event: message_stop
data: {"type":"message_stop"}
```

Claude also supports extended thinking with separate events.

#### 1.5 Implement Streaming in `src/llm/ollama.rs`

Ollama streaming is simpler - set `stream: true` and read newline-delimited JSON:
```json
{"response":"Hello"}
{"response":" world"}
{"done":true}
```

### Phase 2: Chat Agent Streaming

#### 2.1 Add Streaming Method to `src/agent/chat.rs`

```rust
impl ChatAgent {
    /// Chat with streaming callbacks for real-time UI updates
    pub async fn chat_streaming<F, G>(
        &mut self,
        user_message: &str,
        on_text: F,
        on_thinking: G,
    ) -> Result<AgentResponse>
    where
        F: Fn(String) + Send + Sync + 'static,
        G: Fn(String) + Send + Sync + 'static,
    {
        self.context.add_user(user_message);
        
        let callback = Box::new(move |event: StreamEvent| {
            match event {
                StreamEvent::TextDelta(text) => on_text(text),
                StreamEvent::ThinkingDelta(text) => on_thinking(text),
                _ => {}
            }
        });
        
        loop {
            let response = self.llm
                .chat_streaming(self.context.messages(), Some(&tools), callback.clone())
                .await?;
            
            match response {
                LlmResponse::Text { text, .. } => {
                    self.context.add_assistant(&text);
                    return Ok(AgentResponse { text, ... });
                }
                LlmResponse::ToolCalls { calls, .. } => {
                    // Execute tools and continue loop
                    for call in calls {
                        // ... tool execution ...
                    }
                }
                // ...
            }
        }
    }
}
```

### Phase 3: Agent Bridge Integration

#### 3.1 Update `src/tui/agent_bridge.rs`

```rust
pub async fn send_message_streaming(
    &mut self,
    message: &str,
    event_tx: mpsc::Sender<AgentEvent>,
) -> Result<()> {
    let _ = event_tx.send(AgentEvent::Started).await;
    
    let tx_clone = event_tx.clone();
    let on_text = move |chunk: String| {
        let _ = tx_clone.blocking_send(AgentEvent::TextChunk(chunk));
    };
    
    let tx_clone2 = event_tx.clone();
    let on_thinking = move |chunk: String| {
        let _ = tx_clone2.blocking_send(AgentEvent::ThinkingChunk(chunk));
    };
    
    let response = self.agent
        .chat_streaming(message, on_text, on_thinking)
        .await?;
    
    let _ = event_tx.send(AgentEvent::Completed(response.into())).await;
    Ok(())
}
```

### Phase 4: TUI Event Loop (Already Done!)

The TUI already handles these events properly:
- `AgentEvent::TextChunk` - appends to streaming message
- `AgentEvent::ThinkingChunk` - appends to thinking_content
- Non-blocking poll with timeout
- Auto-scroll on new content

## Files to Modify

| File | Changes | Lines (est.) |
|------|---------|--------------|
| `src/llm/types.rs` | Add StreamEvent, StreamCallback | ~30 |
| `src/llm/mod.rs` | Add chat_streaming to trait | ~10 |
| `src/llm/openai.rs` | Implement SSE streaming | ~80 |
| `src/llm/claude.rs` | Implement SSE streaming | ~80 |
| `src/llm/ollama.rs` | Implement JSON streaming | ~50 |
| `src/agent/chat.rs` | Add chat_streaming method | ~60 |
| `src/tui/agent_bridge.rs` | Use chat_streaming | ~20 |

**Total: ~330 lines of new code**

## Dependencies

Need to add to `Cargo.toml`:
```toml
futures = "0.3"  # For StreamExt
```

## Testing Plan

1. Unit tests for SSE parsing in each provider
2. Integration test with mock streaming server
3. Manual testing with each provider:
   - OpenAI GPT-4
   - Claude 3.5 Sonnet
   - Ollama (llama3.2)

## Rollout

1. Implement OpenAI streaming first (most common)
2. Test thoroughly
3. Add Claude streaming
4. Add Ollama streaming
5. Default to streaming when available, fallback to non-streaming

## Risk Mitigation

- Keep existing `chat()` method for fallback
- Add timeout handling for stuck streams
- Handle partial responses gracefully
- Test with slow/unstable connections

