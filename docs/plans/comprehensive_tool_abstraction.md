# Comprehensive Tool Call Abstraction

## Overview

Currently, tool call handling is scattered across providers with duplicated logic for:
1. **Converting tool definitions** → Provider API format
2. **Converting chat history with tool calls** → Provider message format
3. **Parsing tool calls from responses** → Our internal format
4. **Streaming tool call state** → Already abstracted with `ToolCallTracker` ✅

This plan abstracts #1-3 to reduce duplication and bugs.

---

## Current State Analysis

### Tool Definition Conversion (ToolDefinition → Provider Format)

| Provider | Method | Output Type | Format |
|----------|--------|-------------|--------|
| OpenAI (Chat) | `convert_tools()` | `Vec<OpenAiTool>` | `{type: "function", function: {name, description, parameters}}` |
| OpenAI (Responses) | `convert_tools_to_responses()` | `Vec<ResponsesTool>` | `{type: "function", name, description, parameters}` (flat) |
| Claude | `convert_tools()` | `Vec<ClaudeTool>` | `{name, description, input_schema}` |
| Gemini | `convert_tools()` | `Vec<GeminiFunctionDeclaration>` | `{name, description, parameters}` |
| Copilot | `convert_tools()` | `Vec<OpenAiTool>` | Same as OpenAI |
| OpenRouter | `convert_tools()` | `Vec<OpenAiTool>` | Same as OpenAI |

**Observation**: 
- All use `ToolDefinition` as input
- All output similar structures (name, description, parameters/input_schema)
- Main difference is nesting level and field names

### Chat History Conversion (Messages with Tool Calls)

| Provider | Assistant Tool Calls | Tool Results |
|----------|---------------------|--------------|
| OpenAI (Chat) | `tool_calls: [{ function: {name, arguments} }]` | `role: "tool", tool_call_id, content` |
| OpenAI (Responses) | **SKIP** (outputs, not inputs) | `content: [{type: "function_result", call_id, output}]` |
| Claude | **SKIP** (outputs in assistant, include in history as tool_use blocks) | `content: [{type: "tool_result", tool_use_id, content}]` |
| Gemini | `parts: [{functionCall: {name, args}}]` | `parts: [{functionResponse: {name, response}}]` |

**Observation**:
- OpenAI Responses API **rejects** tool calls in input (lines 6, 10, 14, 22 in debug log)
- Claude includes tool_use blocks but only in streaming context  
- Gemini includes functionCall in history
- Each has different field names (call_id vs tool_use_id vs tool_call_id)

---

## Proposed Abstraction

### Level 1: Tool Definition Converter (Simple)

Since tool definitions are simple and providers just rename fields, we can use a **trait-based adapter pattern**:

```rust
// src/llm/tools/mod.rs (new)

pub trait ToolConverter {
    type ToolType: Serialize;
    
    /// Convert our ToolDefinition to provider-specific format
    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<Self::ToolType>;
}
```

**Benefit**: Minimal - tool conversion is already simple. **Skip this for now.**

### Level 2: Message Conversion with Tool Calls (Critical)

The **real issue** is in chat history conversion. Different providers have incompatible requirements:

**Current Bug**: OpenAI Responses API rejects `ContentPart::ToolUse` in input
**Root Cause**: Providers don't know which parts to skip vs include

**Solution**: Add a `skip_tool_calls_in_history` flag to providers:

```rust
// In convert_messages_to_responses (OpenAI Responses API):
match part {
    ContentPart::ToolUse { .. } => {
        // Skip - Responses API doesn't accept tool calls as input
        continue;
    }
    ContentPart::ToolResult { .. } => {
        // Include - these are valid inputs
        content_parts.push(...)
    }
}
```

**Already fixed** in previous change ✅

### Level 3: Streaming Tool Call Parser (Already Done)

`ToolCallTracker` abstracts the state management ✅

---

## Implementation Plan

### ✅ Already Done
1. Streaming tool call state management → `ToolCallTracker`
2. SSE decoding → `SseDecoder`  
3. Error handling → `LlmError` enum
4. OpenAI Responses API fix → Skip tool calls in input

### 🔧 Remaining Work

#### 1. Fix Tool Call History Handling for All Providers

**Problem**: Different APIs have different rules for what to include in history:
- OpenAI Chat Completions: Include tool calls
- OpenAI Responses: Exclude tool calls
- Claude: Exclude tool calls (they use tool_use blocks differently)
- Gemini: Include tool calls

**Solution**: Document and test each provider's message conversion logic

#### 2. Abstract Tool Result Conversion

**Problem**: Each provider uses different field names for tool results:
- OpenAI: `tool_call_id`
- Claude: `tool_use_id`  
- Gemini: `name` (in functionResponse)
- OpenRouter/Copilot: Same as OpenAI

**Solution**: Create a shared helper:

```rust
// src/llm/tools/result_formatter.rs

pub struct ToolResultFormatter;

impl ToolResultFormatter {
    /// Format tool result for OpenAI-compatible APIs
    pub fn for_openai(tool_call_id: &str, content: &str) -> OpenAiMessage;
    
    /// Format tool result for Claude API
    pub fn for_claude(tool_use_id: &str, content: &str) -> ClaudeMessage;
    
    /// Format tool result for Gemini API
    pub fn for_gemini(name: &str, content: &str) -> GeminiPart;
}
```

**Benefit**: Centralizes field name mapping, reduces errors

#### 3. Integration Tests for Tool Calls

**Problem**: Tool call bugs (like the Responses API error) only surface at runtime

**Solution**: Add end-to-end tests:

```rust
#[tokio::test]
async fn test_openai_responses_with_tool_history() {
    // Verify that tool calls in history don't cause 400 errors
}

#[tokio::test]  
async fn test_streaming_tool_calls_emit_correct_events() {
    // Verify ToolCallStart/Delta/Complete are emitted correctly
}
```

---

## Priority

### Immediate (Fix the OpenAI Responses API Error)
1. ✅ Skip `ContentPart::ToolUse` in `convert_messages_to_responses()` - **DONE**
2. Test with a real conversation that has tool calls
3. Verify no more 400 errors

### Short-term (Polish)
1. Migrate Claude to use `ToolCallTracker` (Phase 3 of streaming abstraction)
2. Migrate Copilot/OpenRouter to use `ToolCallTracker`
3. Add integration tests for tool call streaming

### Long-term (Nice-to-Have)
1. Consider `ToolResultFormatter` helper if field name mismatches cause bugs
2. Document each provider's tool handling quirks

---

## Validation

```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo build --release
```

Then test manually:
```bash
./target/release/tark chat
> (ask a question that triggers a tool call)
> (verify response appears correctly, no 400 errors)
```

---

## Summary

The **critical fix** (skip tool calls in OpenAI Responses input) is already done. The remaining work is:
1. **Phase 3-4**: Migrate other providers to `ToolCallTracker` (cleanup)
2. **Integration tests**: Prevent regression
3. **(Optional)** `ToolResultFormatter` if needed

The major bug (lines 6, 10, 14, 22 in debug log) should now be fixed!
