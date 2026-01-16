# Streaming Tool Call Abstraction Plan

## Problem Statement

Each LLM provider has a different SSE format for streaming tool/function calls:

| Provider | Start Event | Delta Event | Complete Event |
|----------|-------------|-------------|----------------|
| OpenAI | `response.output_item.added` | `response.function_call_arguments.delta` | `response.function_call_arguments.done` |
| Claude | `ContentBlockStart(ToolUse)` | `ContentBlockDelta(InputJsonDelta)` | `ContentBlockStop` |
| Gemini | `FunctionCall` in candidates | N/A (non-streaming) | N/A |
| Copilot | OpenAI-compatible | OpenAI-compatible | OpenAI-compatible |
| OpenRouter | OpenAI-compatible | OpenAI-compatible | OpenAI-compatible |

This leads to:
1. **Duplicated logic** - Each provider manually maps to `StreamEvent::ToolCall*`
2. **Subtle bugs** - Field name mismatches (e.g., `item_id` vs `call_id`)
3. **Inconsistent behavior** - Some providers emit events, others don't
4. **Hard to test** - Tool call parsing is embedded in provider code

## Proposed Architecture

### 1. Common `StreamEvent` Types (Already Exists)

```rust
// src/llm/types.rs - already defined
pub enum StreamEvent {
    TextDelta(String),
    ThinkingDelta(String),
    ToolCallStart { id: String, name: String },
    ToolCallDelta { id: String, arguments_delta: String },
    ToolCallComplete { id: String },
    Done,
    Error(String),
}
```

### 2. New `ToolCallTracker` (Shared State Management)

```rust
// src/llm/streaming/tool_tracker.rs (new)

/// Tracks in-progress tool calls across streaming events
/// Handles the complexity of mapping provider-specific IDs to canonical call IDs
pub struct ToolCallTracker {
    /// call_id -> (name, accumulated_arguments)
    calls: HashMap<String, (String, String)>,
    /// Provider-specific ID -> canonical call_id mapping
    /// (e.g., OpenAI item_id -> call_id, Claude index -> call_id)
    id_mapping: HashMap<String, String>,
}

impl ToolCallTracker {
    pub fn new() -> Self;
    
    /// Register a new tool call, returns ToolCallStart event
    pub fn start_call(&mut self, call_id: &str, name: &str, provider_id: Option<&str>) 
        -> StreamEvent;
    
    /// Append arguments delta, returns ToolCallDelta event
    /// Accepts either call_id or provider_id
    pub fn append_args(&mut self, id: &str, delta: &str) -> Option<StreamEvent>;
    
    /// Mark call complete, returns ToolCallComplete event
    pub fn complete_call(&mut self, id: &str) -> Option<StreamEvent>;
    
    /// Get all tracked calls (for building final response)
    pub fn into_calls(self) -> HashMap<String, (String, String)>;
}
```

### 3. Provider-Specific Parsers Use Shared Tracker

Each provider creates a `ToolCallTracker` and uses it to emit events:

```rust
// In OpenAI streaming:
let mut tracker = ToolCallTracker::new();

// On output_item.added
if item.item_type == "function_call" {
    let event = tracker.start_call(&call_id, &name, Some(&item_id));
    callback(event);
}

// On function_call_arguments.delta  
if let Some(event) = tracker.append_args(&item_id, &delta) {
    callback(event);
}

// On function_call_arguments.done
if let Some(event) = tracker.complete_call(&item_id) {
    callback(event);
}
```

```rust
// In Claude streaming:
let mut tracker = ToolCallTracker::new();

// On ContentBlockStart(ToolUse)
let event = tracker.start_call(&id, &name, Some(&index.to_string()));
callback(event);

// On ContentBlockDelta(InputJsonDelta)
if let Some(event) = tracker.append_args(&index.to_string(), &partial_json) {
    callback(event);
}

// On ContentBlockStop
if let Some(event) = tracker.complete_call(&index.to_string()) {
    callback(event);
}
```

## Benefits

1. **Single source of truth** - Tool call state managed in one place
2. **Consistent ID handling** - Tracker handles provider ID → call ID mapping
3. **Easier testing** - Can unit test `ToolCallTracker` independently
4. **Less duplication** - Providers just parse their format, delegate state to tracker
5. **Cleaner provider code** - No need for manual HashMap management

## Implementation Steps

### Phase 1: Create ToolCallTracker ✅ COMPLETE
- [x] Create `src/llm/streaming/tool_tracker.rs`
- [x] Implement `ToolCallTracker` with start/append/complete methods
- [x] Add comprehensive unit tests (17 tests, all passing)

### Phase 2: Migrate OpenAI ✅ COMPLETE
- [x] Replace manual `tool_call_map` and `item_to_call_id` with `ToolCallTracker`
- [x] Verify streaming tool calls work correctly (7 tests, all passing)
- [x] Reduced code complexity from 2 HashMaps + manual ID mapping to single tracker

### Phase 3: Migrate Claude ✅ COMPLETE
- [x] Replace manual `tool_call_map` with `ToolCallTracker`
- [x] Handle index-based ID mapping (0, 1, 2... → toolu_xxx)
- [x] Verify streaming tool calls work correctly (4 tests passing)

### Phase 4: Migrate Other Providers (Deferred)
- [ ] Copilot - Uses manual SSE parsing, needs `SseDecoder` + `ToolCallTracker`
- [ ] OpenRouter - Uses manual SSE parsing, needs `SseDecoder` + `ToolCallTracker`
- [ ] Gemini - Already working, no delta tool calls (emits complete calls)

**Note**: Copilot/OpenRouter work correctly but use pre-abstraction code. Can be migrated for consistency but not critical.

### Phase 5: Documentation & Tests ✅ COMPLETE
- [x] Created `docs/TOOL_CALL_ARCHITECTURE.md` - comprehensive guide
- [x] Created `tests/tool_call_integration.rs` - 7 integration tests
- [x] Updated `docs/plans/comprehensive_tool_abstraction.md`
- [x] Documented provider-specific quirks and API requirements

## Alternative Considered: Full Trait Abstraction

We considered a `ToolCallParser` trait per provider, but decided against it because:
1. Parsing is tightly coupled with the SSE event format (can't easily abstract)
2. The actual logic is mostly ID tracking, which `ToolCallTracker` handles
3. Adding a trait would require significant refactoring for minimal benefit

The `ToolCallTracker` approach gives us 80% of the benefit with 20% of the effort.

## Success Criteria

1. All providers emit consistent `ToolCallStart`/`Delta`/`Complete` events
2. No manual `HashMap<String, (String, String)>` in provider code
3. Tool call streaming works correctly for all providers
4. Unit tests cover edge cases (out-of-order events, missing IDs, etc.)
