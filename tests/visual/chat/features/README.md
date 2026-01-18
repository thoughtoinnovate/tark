# BDD Feature Files for Chat E2E Testing

This directory contains Gherkin feature files for Behavior-Driven Development (BDD) testing of `tark chat`.

## Overview

These feature files define end-to-end test scenarios that:
1. Drive the TUI via `expect` scripts
2. Record sessions with `asciinema`
3. Generate animated GIFs with `agg`
4. Capture PNG snapshots for visual regression

## Feature Files

| File | Feature | Priority | Scenarios |
|------|---------|----------|-----------|
| `01_basic.feature` | Basic chat interaction | P0 (Smoke) | 12 |
| `02_streaming.feature` | Streaming responses | P1 (Core) | 5 |
| `03_tool_invocation.feature` | Tool calling | P1 (Core) | 8 |
| `04_multi_turn.feature` | Conversation memory | P1 (Core) | 7 |
| `05_error_handling.feature` | Error scenarios | P2 (Extended) | 8 |
| `06_thinking.feature` | Extended thinking | P2 (Extended) | 6 |
| `07_ui_elements.feature` | UI components | P2 (Extended) | 12 |

**Total: ~58 scenarios**

## Tags

### Priority Tags
- `@p0` / `@smoke` - Critical smoke tests (run on every commit)
- `@p1` / `@core` - Core functionality (run on PRs)
- `@p2` / `@extended` - Extended coverage (run on release)

### Feature Tags
- `@basic` - Basic chat functionality
- `@streaming` - Streaming response tests
- `@tools` - Tool invocation tests
- `@memory` - Conversation context tests
- `@errors` - Error handling tests
- `@thinking` - Extended thinking tests
- `@ui` - UI component tests

## tark_sim Scenarios

The `tark_sim` provider supports these simulation scenarios via `TARK_SIM_SCENARIO` env var:

| Scenario | Behavior |
|----------|----------|
| `echo` (default) | Echoes user input |
| `streaming` | Chunked response with delays |
| `tool` | Returns tool call, then response |
| `multi_tool` | Chained tool calls |
| `thinking` | Shows thinking before response |
| `error_timeout` | Simulates timeout error |
| `error_rate_limit` | Simulates rate limit error |
| `error_context_exceeded` | Simulates context length error |
| `error_malformed` | Simulates malformed response |
| `error_partial` | Simulates partial response |
| `error_filtered` | Simulates content filtering |

## Running Tests

```bash
# Run all chat E2E tests
make e2e-chat

# Run smoke tests only (P0)
make e2e-smoke

# Run core tests (P0 + P1)
make e2e-core

# Run specific feature
./tests/visual/e2e_runner.sh --feature basic

# Verify snapshots against baseline
make e2e-verify

# Update baseline snapshots
make e2e-update
```

## Outputs

- **Recordings**: `tests/visual/chat/recordings/*.gif` - Animated recordings
- **Current Snapshots**: `tests/visual/chat/current/*.png` - Latest run
- **Baseline Snapshots**: `tests/visual/chat/snapshots/*.png` - Expected results
- **Diffs**: `tests/visual/chat/diffs/` - Visual differences

## Writing New Scenarios

1. Choose the appropriate feature file or create a new one
2. Use proper tags (`@p0`/`@p1`/`@p2`, feature tags)
3. Follow Given-When-Then format
4. Include `And a recording is saved as "name.gif"` for visual tests
5. Include `And a snapshot is saved as "name.png"` for regression tests
