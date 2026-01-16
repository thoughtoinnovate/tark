# Tool Call Architecture

## Overview

Tool calls in `tark` flow through three layers:
1. **Core Layer** - Tool registry, execution, and orchestration
2. **LLM Layer** - Provider-specific serialization and parsing
3. **Streaming Layer** - Shared abstractions for real-time events

---

## Architecture

```
┌──────────────────────────────────────────────────────┐
│  Core Layer (src/agent/, src/tools/)                 │
│  • Tool registry with risk-based categorization      │
│  • Tool execution with approval gates                │
│  • Context management (adds tool results to history) │
│  • Provides ToolDefinition (name, desc, params)      │
└────────────────────┬─────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────┐
│  LLM Provider Layer (src/llm/*.rs)                   │
│  • Converts ToolDefinition → API format              │
│  • Converts chat history with tools → API format     │
│  • Parses tool calls from response → our format      │
│  • Uses shared abstractions for streaming            │
└────────────────────┬─────────────────────────────────┘
                     │
                     ▼
┌──────────────────────────────────────────────────────┐
│  Shared Streaming Abstractions (src/llm/streaming/)  │
│  • SseDecoder - parses SSE streams                   │
│  • ToolCallTracker - tracks tool call state          │
│  • Emits StreamEvent::ToolCall* events               │
└──────────────────────────────────────────────────────┘
```

---

## Provider-Specific Tool Handling

### OpenAI

**Two APIs with different requirements:**

#### Chat Completions API (`/v1/chat/completions`)
- **Tool Definitions**: `{type: "function", function: {name, description, parameters}}`
- **Tool Calls in History**: ✅ Include as `tool_calls: [{id, type, function: {name, arguments}}]`
- **Tool Results**: `role: "tool", tool_call_id, content`
- **Streaming**: Standard OpenAI SSE format with `tool_calls` deltas

#### Responses API (`/v1/responses`)
- **Tool Definitions**: `{type: "function", name, description, parameters}` (flat structure)
- **Tool Calls in History**: ❌ **MUST SKIP** - they're outputs, not inputs!
- **Tool Results**: `content: [{type: "function_result", call_id, output}]`
- **Streaming**: Event-based SSE:
  - `response.output_item.added` - announces function call with `item.id`, `item.call_id`, `item.name`
  - `response.function_call_arguments.delta` - sends args with `item_id`, `delta`
  - `response.function_call_arguments.done` - completes with `item_id`
  - Uses `ToolCallTracker` to map `item_id` → `call_id`

**Critical Bug Fixed**: Lines 6, 10, 14, 22 in debug logs showed 400 errors because `ContentPart::ToolUse` was being sent as `"function_call"` in input, which Responses API rejects. Now **skipped** in `convert_messages_to_responses()`.

### Claude

- **Tool Definitions**: `{name, description, input_schema}`
- **Tool Calls in History**: ✅ Include as `content: [{type: "tool_use", id, name, input}]`
- **Tool Results**: `content: [{type: "tool_result", tool_use_id, content}]`
- **Streaming**: Content-block-based SSE:
  - `ContentBlockStart(ToolUse)` - announces with `index`, `id`, `name`
  - `ContentBlockDelta(InputJsonDelta)` - sends `index`, `partial_json`
  - `ContentBlockStop` - completes with `index`
  - Uses `ToolCallTracker` to map `index` (0, 1, 2...) → `id` (toolu_xxx)

### Gemini

- **Tool Definitions**: `{functionDeclarations: [{name, description, parameters}]}`
- **Tool Calls in History**: ✅ Include as `parts: [{functionCall: {name, args}}]`
- **Tool Results**: `parts: [{functionResponse: {name, response}}]`
- **Streaming**: Currently emits complete function calls, not deltas (no ToolCallTracker needed yet)

### Copilot / OpenRouter

- **Tool Definitions**: Same as OpenAI Chat Completions
- **Tool Calls**: Same as OpenAI Chat Completions
- **Streaming**: Uses manual parsing (pre-dates SseDecoder migration)
- **Migration**: Pending (Phases 4-5)

---

## Shared Abstractions

### SseDecoder (`src/llm/streaming/mod.rs`)

Handles SSE parsing for all providers:
- Buffers incomplete events
- Extracts `data:` payloads
- Handles final events without trailing newline

**Used by**: OpenAI (both APIs), Claude, Gemini

### ToolCallTracker (`src/llm/streaming/tool_tracker.rs`)

Tracks tool call state during streaming:
- Maps provider-specific IDs to canonical call IDs
- Accumulates arguments across delta events
- Emits standard `StreamEvent::ToolCall*` events

**Used by**: OpenAI Responses API, Claude  
**Pending**: Copilot, OpenRouter

---

## StreamEvent Types

All providers emit these standard events:

```rust
pub enum StreamEvent {
    TextDelta(String),              // Incremental text
    ThinkingDelta(String),          // Extended thinking content
    ToolCallStart { id, name },     // Function call announced
    ToolCallDelta { id, args_delta },  // Arguments streaming
    ToolCallComplete { id },        // Function call complete
    Done,                           // Stream finished
    Error(String),                  // Error occurred
}
```

These events are consumed by:
- `StreamingResponseBuilder` - accumulates into final `LlmResponse`
- Agent callbacks - drive TUI real-time updates

---

## Testing Strategy

### Unit Tests
- Provider-specific parsing: `test_parse_*`
- Message conversion: `test_convert_messages_*`
- Streaming events: `test_parse_*_stream_*`

### Integration Tests (`tests/tool_call_integration.rs`)
- Tool definition structure validation
- Tool call history handling (regression test for OpenAI 400 error)
- ToolCallTracker behavior (OpenAI-style and Claude-style)
- Parallel tool call handling

### Manual Testing
```bash
# Start TUI
./target/release/tark chat

# Trigger tool call
> understand the code

# Verify:
# 1. Tool is called (should see codebase_overview)
# 2. Result appears in chat
# 3. No 400 errors in debug log
# 4. Follow-up questions work (tool results in history)
```

---

## Migration Status

| Provider | SseDecoder | ToolCallTracker | Integration Tests |
|----------|-----------|-----------------|-------------------|
| OpenAI (Chat) | ✅ | N/A (simple format) | ✅ |
| OpenAI (Responses) | ✅ | ✅ | ✅ |
| Claude | ✅ | ✅ | ✅ |
| Gemini | ✅ | N/A (no delta tool calls) | ✅ |
| Copilot | ❌ Manual parsing | ⏳ Pending | ✅ |
| OpenRouter | ❌ Manual parsing | ⏳ Pending | ✅ |

---

## Known Issues & Solutions

### Issue 1: OpenAI Responses API 400 Error (FIXED ✅)
**Symptom**: "Invalid value: 'function_call'. Supported values are: 'input_text'..."  
**Root Cause**: Sending `ContentPart::ToolUse` in input array  
**Fix**: Skip tool calls in `convert_messages_to_responses()` - they're outputs, not inputs  
**Test**: `tests/tool_call_integration.rs::test_responses_api_skips_tool_calls_in_input`

### Issue 2: OpenAI Streaming Tool Calls Not Appearing (FIXED ✅)
**Symptom**: Tool call responses showed 215 output tokens but `text_len: 0`  
**Root Cause**: Struct mismatch - looking for `call_id` but API sends `item_id`  
**Fix**: Use `ToolCallTracker` with proper ID mapping  
**Test**: `tests/tool_call_integration.rs::test_tool_call_tracker_openai_style`

### Issue 3: Duplicate Tool Call Tracking Code
**Symptom**: Each provider had its own HashMap management  
**Root Cause**: No shared abstraction  
**Fix**: `ToolCallTracker` used by OpenAI Responses and Claude  
**Benefit**: 50+ lines of boilerplate removed per provider

---

## Future Improvements

### Short-term
1. Migrate Copilot to use `SseDecoder` and `ToolCallTracker`
2. Migrate OpenRouter to use `SseDecoder` and `ToolCallTracker`
3. Add end-to-end test that runs full agent loop with tool calls

### Long-term
1. Consider `ToolResultFormatter` helper if more field name bugs surface
2. Abstract reasoning/thinking handling (similar to tool calls)
3. Add performance benchmarks for streaming throughput

---

## Developer Guidelines

### When Adding a New Provider

1. **Tool Definitions**: Implement `convert_tools()` - just map field names
2. **Message Conversion**: Check API docs for tool call history requirements:
   - Do they want tool calls in input? (Gemini, OpenAI Chat: yes; OpenAI Responses, Claude: no)
   - What field names? (`tool_call_id` vs `call_id` vs `tool_use_id`)
3. **Streaming**: Use `SseDecoder` + `ToolCallTracker`:
   ```rust
   let mut decoder = SseDecoder::new();
   let mut tool_tracker = ToolCallTracker::new();
   
   // On function call start
   let event = tool_tracker.start_call(call_id, name, provider_id);
   
   // On arguments delta
   if let Some(event) = tool_tracker.append_args(provider_id, delta) {
       callback(event);
   }
   
   // On complete
   if let Some(event) = tool_tracker.complete_call(provider_id) {
       callback(event);
   }
   ```
4. **Tests**: Add to `tests/tool_call_integration.rs`

### Common Pitfalls

❌ **Don't** assume all APIs accept tool calls in history  
❌ **Don't** manually manage tool call HashMaps  
❌ **Don't** parse SSE manually (use `SseDecoder`)  
✅ **Do** check API documentation for input format requirements  
✅ **Do** use `ToolCallTracker` for streaming tool calls  
✅ **Do** add tests for your provider's specific quirks  

---

## References

- OpenAI Responses API: https://platform.openai.com/docs/api-reference/responses
- Claude Messages API: https://docs.anthropic.com/claude/reference/messages_post
- Gemini API: https://ai.google.dev/gemini-api/docs/function-calling
- Implementation: `src/llm/streaming/tool_tracker.rs`
- Tests: `tests/tool_call_integration.rs`
