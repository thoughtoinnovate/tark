# Debug Logging Feature - Implementation Summary

## Overview

Implemented comprehensive JSON-based debug logging for `tark tui --debug` with correlation IDs to trace requests end-to-end across all system layers.

## What Was Implemented

### Core Components

1. **Debug Logger Module** (`src/debug_logger.rs`)
   - `DebugLogger` with automatic log rotation (10MB max, 3 rotated files)
   - `DebugLogEntry` with structured JSON format
   - `LogCategory` enum (Service, Tui, LlmRaw)
   - `SensitiveDataRedactor` for masking API keys, tokens, credentials
   - `ErrorContext` for capturing stack traces, env vars, system info on errors

2. **Global Logger** (`src/lib.rs` and `src/main.rs`)
   - Global `TARK_DEBUG_LOGGER` instance
   - `init_debug_logger()` function
   - `debug_logger()` accessor
   - `debug_log()` helper function

3. **Correlation ID Tracking** (`src/ui_backend/state.rs`)
   - Added `current_correlation_id` field to `SharedState`
   - `generate_new_correlation_id()` method
   - Generated on each user message send

4. **Service Logging** 
   - `src/ui_backend/service.rs` - User message logging
   - `src/ui_backend/conversation.rs` - LLM request/response/error logging with full error context
   - Tool invocation and response logging

5. **LLM Raw Logging** (`src/llm/raw_log.rs`)
   - Unified with main debug logger
   - Changed `REQUEST_ID` to `CORRELATION_ID`
   - Now logs to single `tark-debug.log` file with `llm_raw` category

6. **TUI Logging**
   - `src/tui_new/controller.rs` - Command processing logs
   - `src/tui_new/renderer.rs` - Frame render timing logs

7. **CLI Integration** (`src/transport/cli.rs`)
   - Initializes debug logger when `--debug` flag is set
   - Logs written to `.tark/debug/tark-debug.log`

## Usage

### Enable Debug Logging

```bash
tark tui --debug
```

### Log Output Location

```
.tark/debug/
├── tark-debug.log       # Current log
├── tark-debug.1.log     # Previous (after rotation)
└── tark-debug.2.log     # Older (after 2nd rotation)
```

### Log Format

Each line is a JSON object with:

```json
{
  "timestamp": "2026-01-19T10:30:00.000Z",
  "correlation_id": "550e8400-e29b-41d4-a716-446655440000",
  "category": "service",
  "event": "llm_request_start",
  "content_length": 42
}
```

### Log Categories

- **`service`** - Backend operations (LLM requests, tool invocations/responses)
- **`tui`** - TUI events (command processing, frame rendering)
- **`llm_raw`** - Raw LLM streaming data

### Example Log Entries

#### User Message
```json
{"timestamp":"2026-01-19T10:30:00.000Z","correlation_id":"abc123","category":"service","event":"user_message","content_preview":"Hello, can you...","content_length":150}
```

#### LLM Request
```json
{"timestamp":"2026-01-19T10:30:00.100Z","correlation_id":"abc123","category":"service","event":"llm_request_start","content_length":150}
```

#### Tool Invocation
```json
{"timestamp":"2026-01-19T10:30:01.000Z","correlation_id":"abc123","category":"service","event":"tool_invocation","tool":"read_file","args":{"path":"src/main.rs"}}
```

#### Tool Response
```json
{"timestamp":"2026-01-19T10:30:01.500Z","correlation_id":"abc123","category":"service","event":"tool_response","tool":"read_file","success":true,"result_preview":"use anyhow...","result_length":5000}
```

#### LLM Response
```json
{"timestamp":"2026-01-19T10:30:02.000Z","correlation_id":"abc123","category":"service","event":"llm_response","text_length":500,"tool_calls_made":1,"input_tokens":1000,"output_tokens":200}
```

#### Raw LLM Stream
```json
{"timestamp":"2026-01-19T10:30:00.200Z","correlation_id":"abc123","category":"llm_raw","event":"stream_chunk","data":"Hello"}
```

#### TUI Events
```json
{"timestamp":"2026-01-19T10:30:02.100Z","correlation_id":"abc123","category":"tui","event":"render_frame","frame_time_ms":16}
```

#### Error with Full Context
```json
{
  "timestamp": "2026-01-19T10:30:05.000Z",
  "correlation_id": "abc123",
  "category": "service",
  "event": "llm_error_detail",
  "error_context": {
    "error_type": "reqwest::Error",
    "error_message": "Connection refused",
    "backtrace": "stack trace here...",
    "env": {
      "OPENAI_API_KEY": "[REDACTED]",
      "RUST_LOG": "debug"
    },
    "system": {
      "os": "linux",
      "tark_version": "0.7.0",
      "working_dir": "/home/user/project"
    }
  }
}
```

## Troubleshooting with Debug Logs

### Filter by Correlation ID

To trace a single user request through the system:

```bash
cat .tark/debug/tark-debug.log | jq 'select(.correlation_id=="abc123")'
```

### View by Category

```bash
# Service logs only
cat .tark/debug/tark-debug.log | jq 'select(.category=="service")'

# TUI logs only
cat .tark/debug/tark-debug.log | jq 'select(.category=="tui")'

# LLM raw logs only
cat .tark/debug/tark-debug.log | jq 'select(.category=="llm_raw")'
```

### Count Events by Type

```bash
cat .tark/debug/tark-debug.log | jq -r '.event' | sort | uniq -c
```

### Find Errors

```bash
cat .tark/debug/tark-debug.log | jq 'select(.event | contains("error"))'
```

### Verify Log Rotation

```bash
ls -lah .tark/debug/
# Should show multiple log files after generating >10MB of logs
```

### Verify Sensitive Data Redacted

```bash
# This should return nothing (or only "[REDACTED]")
grep -E "sk-|ghp_|Bearer [a-zA-Z0-9]+" .tark/debug/tark-debug.log
```

## Sensitive Data Protection

The following patterns are automatically redacted:

- OpenAI API keys: `sk-*` → `sk-[REDACTED]`
- Generic API keys: `key-*` → `key-[REDACTED]`  
- Bearer tokens: `Bearer xyz...` → `Bearer [REDACTED]`
- GitHub tokens: `ghp_*`, `gho_*`, `ghs_*` → `[REDACTED]`
- Environment variables: `API_KEY=value` → `API_KEY=[REDACTED]`
- JSON fields: `{"api_key": "value"}` → `{"api_key": "[REDACTED]"}`

## Files Modified

| File | Changes |
|------|---------|
| `src/debug_logger.rs` | NEW - Core debug logging infrastructure |
| `src/lib.rs` | Added global logger and exports |
| `src/main.rs` | Added debug logger for binary crate |
| `src/ui_backend/state.rs` | Added correlation_id tracking |
| `src/ui_backend/service.rs` | Added user message logging |
| `src/ui_backend/conversation.rs` | Added LLM request/response/error logging |
| `src/llm/raw_log.rs` | Migrated to unified logger, renamed REQUEST_ID to CORRELATION_ID |
| `src/llm/debug_wrapper.rs` | Updated to use CORRELATION_ID |
| `src/tui_new/controller.rs` | Added command processing logging |
| `src/tui_new/renderer.rs` | Added render frame timing logging |
| `src/transport/cli.rs` | Initialize debug logger with --debug flag |

## Testing

All tests pass:
- Library tests: ✓ 310 passed
- Compilation: ✓ Success with --release
- Formatting: ✓ cargo fmt applied
- Linting: ✓ cargo clippy passed (warnings only for dead code)
