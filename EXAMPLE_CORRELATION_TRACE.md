# Example Correlation Trace

This document shows a real example of how correlation IDs work in the debug logs.

## Request: `87177785-4c90-4a7e-88ff-e9dd76610d83`

User asked: **"as a question"**

### Complete Event Timeline

```json
{"timestamp":"2026-01-19T21:15:33.181901505+00:00","correlation_id":"87177785-4c90-4a7e-88ff-e9dd76610d83","category":"service","event":"user_message","content_length":13,"content_preview":"as a question"}

{"timestamp":"2026-01-19T21:15:33.181993755+00:00","correlation_id":"87177785-4c90-4a7e-88ff-e9dd76610d83","category":"service","event":"llm_request_start","content_length":13}

{"timestamp":"2026-01-19T21:15:35.343863339+00:00","correlation_id":"87177785-4c90-4a7e-88ff-e9dd76610d83","category":"service","event":"tool_invocation","args":{"depth":3,"include_file_counts":true},"tool":"codebase_overview"}

{"timestamp":"2026-01-19T21:15:35.350550006+00:00","correlation_id":"87177785-4c90-4a7e-88ff-e9dd76610d83","category":"service","event":"tool_response","result_length":203,"result_preview":"## Directory Structure...","success":true,"tool":"codebase_overview"}

{"timestamp":"2026-01-19T21:15:37.714397966+00:00","correlation_id":"87177785-4c90-4a7e-88ff-e9dd76610d83","category":"service","event":"llm_response","input_tokens":6591,"output_tokens":64,"text_length":193,"tool_calls_made":1}
```

### Visual Flow

```
User Input: "as a question"
    ↓
[service] user_message (t=0.000s)
    ↓
[service] llm_request_start (t=0.000s)
    ↓
    ... LLM processing ...
    ↓
[service] tool_invocation: codebase_overview (t=2.162s)
    ↓
[service] tool_response: success (t=2.169s)
    ↓
    ... LLM continuing ...
    ↓
[service] llm_response: 193 chars, 1 tool call (t=4.532s)
```

### Key Observations

1. **Single Correlation ID**: All events share `87177785-4c90-4a7e-88ff-e9dd76610d83`
2. **Event Ordering**: Chronologically ordered by timestamp
3. **Duration Tracking**: Can calculate durations between events
4. **Tool Usage**: Agent invoked `codebase_overview` tool
5. **Token Usage**: 6,591 input, 64 output tokens logged

### How to Extract This Trace

```bash
# Store correlation ID
CORR_ID="87177785-4c90-4a7e-88ff-e9dd76610d83"

# Get all service events (no render frames)
grep "\"correlation_id\":\"$CORR_ID\"" tark-debug.log | \
  grep '"category":"service"'

# Get all categories
grep "\"correlation_id\":\"$CORR_ID\"" tark-debug.log

# Count events in this trace
grep "\"correlation_id\":\"$CORR_ID\"" tark-debug.log | wc -l
```

## Another Example with LLM Streaming

For requests with streaming, you'll also see `llm_raw` category events with `stream_chunk`:

```json
{"category":"llm_raw","event":"stream_chunk","data":"Hello"}
{"category":"llm_raw","event":"stream_chunk","data":" world"}
{"category":"llm_raw","event":"stream_chunk","data":"!"}
```

All with the same correlation_id, allowing you to see the exact stream chunks received from the LLM.

## Error Trace Example

When an error occurs (Correlation ID: `e1227bb2-5bf7-45f3-ad8d-cd3bf9492d40`):

```json
{"event":"llm_error","error_message":"Service error: {...}","category":"service"}

{"event":"llm_error_detail","category":"service","error_context":{
  "error_type":"...",
  "error_message":"...",
  "backtrace":"...",
  "env":{"OPENAI_API_KEY":"[REDACTED]"},
  "system":{"os":"linux","tark_version":"0.7.0"}
}}
```

This gives support engineers everything needed to diagnose the issue.
