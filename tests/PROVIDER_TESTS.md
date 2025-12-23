# Provider Display Tests

## Overview

Tests for the provider state tracking fix that ensures the UI displays the correct provider name (e.g., "Google" for Gemini) while using the correct backend protocol (e.g., OpenAI-compatible API).

## What Was Fixed

### Problem
When selecting Google/Gemini models, the UI showed "Openai" as the provider label because the code used `current_provider` (backend protocol) for display instead of tracking the actual provider identity.

### Solution
Added `current_provider_id` to track the actual provider separately from the backend protocol:
- `current_provider`: Backend protocol (`'openai'`, `'claude'`, `'ollama'`)
- `current_provider_id`: Actual provider identity (`'google'`, `'openai'`, `'anthropic'`, `'ollama'`)

## Running the Tests

### Run all chat tests (including new provider tests)
```bash
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/chat_spec.lua"
```

### Run all tests
```bash
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedDirectory tests/specs/ {minimal_init = 'tests/minimal_init.lua'}"
```

### Interactive test run (see output in real-time)
```bash
nvim -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/chat_spec.lua"
```

## Test Coverage

The new tests verify:

1. **Test helpers exist** - Ensures test helper functions are available in test mode
2. **Gemini/Google provider** - Backend is `'openai'`, display is `'google'`
3. **OpenAI provider** - Backend and display both `'openai'`
4. **Claude provider** - Backend is `'claude'`, display is `'anthropic'`
5. **Ollama provider** - Backend and display both `'ollama'`
6. **Nil handling** - `provider_id` can be nil (falls back to `provider`)

## Test Output Example

```
✓ chat - agent mode > provider state tracking > has test helper functions
✓ chat - agent mode > provider state tracking > tracks backend provider separately from display provider
✓ chat - agent mode > provider state tracking > tracks OpenAI provider correctly
✓ chat - agent mode > provider state tracking > tracks Claude provider correctly
✓ chat - agent mode > provider state tracking > tracks Ollama provider correctly
✓ chat - agent mode > provider state tracking > provider_id defaults to provider when not set
```

## Manual Testing

After running automated tests, verify the UI displays correctly:

```vim
:TarkChat
/model
```

Then select each provider and verify the title bar:
- **Google** → Select gemini-1.5-flash → Should show "**Google**" not "Openai"
- **OpenAI** → Select gpt-4o → Should show "**Openai**"
- **Anthropic** → Select claude-sonnet-4 → Should show "**Anthropic**"
- **Ollama** → Select codellama → Should show "**Ollama**"

## Test Helper Functions

The following test helpers are available when `vim.g.tark_test_mode = true`:

- `chat._test_get_current_provider()` - Returns backend provider
- `chat._test_get_current_provider_id()` - Returns display provider ID
- `chat._test_get_current_model()` - Returns current model
- `chat._test_set_provider_state(provider, provider_id, model)` - Sets state for testing

These are **only** available in test mode and won't pollute the production API.

