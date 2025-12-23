# Provider Display Fix

**Date**: 2025-12-23  
**Issue**: UI shows "Openai" when Google/Gemini is selected  
**Status**: ✅ Completed with automated tests

## Problem

When selecting Google/Gemini models, the UI incorrectly displayed "Openai" as the provider label because:
- Gemini uses OpenAI-compatible API (backend protocol = `'openai'`)
- The code used `current_provider` for both API routing AND UI display
- Result: UI showed the backend protocol name instead of the actual provider name

## Solution

Added separate tracking for backend protocol vs display identity:
- **`current_provider`**: Backend protocol for API routing (`'openai'`, `'claude'`, `'ollama'`)
- **`current_provider_id`**: Actual provider for UI display (`'google'`, `'openai'`, `'anthropic'`, `'ollama'`)

## Files Changed

### 1. `lua/tark/chat.lua`

#### Added state variable (line 42)
```lua
local current_provider = 'ollama'      -- Backend protocol for API calls
local current_provider_id = 'ollama'   -- Actual provider identity for display
```

#### Updated model selection (line 1744)
```lua
current_provider_id = provider_info.id  -- Track actual provider for display
```

#### Updated 3 display functions
- `update_input_window_title()` - Floating mode title (line 517-525)
- Split mode statusline (line 2415-2426)  
- Initial window creation (line 2523-2531)

All now use `current_provider_id` to look up the proper display name from `providers_info`.

#### Added test helpers (line 2797-2807)
```lua
if vim.g.tark_test_mode then
    M._test_get_current_provider = function() return current_provider end
    M._test_get_current_provider_id = function() return current_provider_id end
    M._test_get_current_model = function() return current_model end
    M._test_set_provider_state = function(provider, provider_id, model)
        current_provider = provider
        current_provider_id = provider_id
        current_model = model
    end
end
```

### 2. `tests/specs/chat_spec.lua`

Added comprehensive provider state tracking tests:
- Test helper function existence
- Gemini/Google provider (backend `'openai'`, display `'google'`)
- OpenAI provider (both `'openai'`)
- Claude provider (backend `'claude'`, display `'anthropic'`)
- Ollama provider (both `'ollama'`)
- Nil handling fallback

### 3. New Documentation

- `tests/PROVIDER_TESTS.md` - Detailed test documentation
- Updated `tests/README.md` - Added provider test coverage

## Results

### Before Fix
```
┌─ Build gemini-1.5-flash Openai ─┐  ❌ Wrong!
│                                  │
└──────────────────────────────────┘
```

### After Fix
```
┌─ Build gemini-1.5-flash Google ─┐  ✅ Correct!
│                                  │
└──────────────────────────────────┘
```

## Provider Mapping

| Provider Selected | Backend Protocol | Display Label |
|------------------|------------------|---------------|
| Google (Gemini)  | `openai`        | `Google`      |
| OpenAI (GPT)     | `openai`        | `Openai`      |
| Anthropic (Claude)| `claude`       | `Anthropic`   |
| Ollama (Local)   | `ollama`        | `Ollama`      |

## Testing

### Run Automated Tests
```bash
nvim --headless -u tests/minimal_init.lua \
  -c "PlenaryBustedFile tests/specs/chat_spec.lua"
```

### Manual Verification
```vim
:TarkChat
/model
" Select Google → Pick gemini-1.5-flash
" Check title shows "Google" not "Openai"
```

## Backward Compatibility

✅ Fully backward compatible:
- Existing sessions continue working
- Falls back to `current_provider` if `current_provider_id` is nil
- No breaking changes to API or config

## Linting

```bash
# No linter errors introduced
✅ lua/tark/chat.lua - Clean
✅ tests/specs/chat_spec.lua - Clean
```

## Related Issues

This also exposed (but did not fix):
1. Duplicate Ollama error messages when Ollama is not running
2. Truncated AI responses in some cases

These can be addressed in separate fixes if needed.

