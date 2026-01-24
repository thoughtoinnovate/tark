# Provider Switch and Max Iterations Fixes

## Summary

Fixed two bugs in the TUI:

1. **Max iterations not applied** - TUI was using hardcoded default of 10 steps instead of config value
2. **Provider switch state mismatch** - When switching to a failed provider, UI showed new provider/model but backend still used old ones

## Changes Made

### 1. Apply max_iterations Config to TUI

**File**: `src/ui_backend/service.rs` (line ~151)

**Change**: Added `.with_max_iterations(global_config.agent.max_iterations)` when creating ChatAgent.

```rust
// Before:
Ok(crate::agent::ChatAgent::new(Arc::from(llm_provider), tools))

// After:
Ok(crate::agent::ChatAgent::new(Arc::from(llm_provider), tools)
    .with_max_iterations(global_config.agent.max_iterations))
```

**Impact**: TUI now respects the `max_iterations` setting from config (default 50, configurable via `~/.config/tark/config.toml`).

### 2. Fix Provider Switch Fallback

**Files**: `src/ui_backend/service.rs`
- Modified `set_provider()` (line ~1775)
- Modified `set_model()` (line ~1856)

**Changes**:
1. Moved session metadata persistence AFTER successful provider switch (not before)
2. Added state revert logic on provider creation/update failure - **reverts BOTH provider AND model**
3. Added informative error messages showing fallback provider/model

**Before**: 
- UI state updated immediately
- Provider creation fails → error shown but UI state not reverted
- Backend still uses old provider while UI shows new one
- Model state remained pointing to new provider's model (causing confusion)

**After**:
- UI state updated tentatively
- Provider creation fails → **both provider AND model** reverted to previous values
- Error message shows: `"<error>\n\nFalling back to: <previous_provider>"`
- Backend and UI state remain synchronized
- Status bar shows correct provider/model pair

**Key Fix**: When provider switch fails, we now revert BOTH:
- `self.state.set_provider(Some(prev.clone()))` ✅
- `self.state.set_model(Some(prev_model))` ✅ (NEW)

This prevents the confusing state where status bar would show "Gemini 2.0 Flash" model with "TARK_SIM" provider.

## Testing

### Build & Quality Checks

```bash
✅ cargo build --release - SUCCESS (2m 29s)
✅ cargo fmt --all - Applied
✅ cargo fmt --all -- --check - PASS
✅ cargo clippy --all-targets --all-features - PASS (0 warnings)
✅ cargo test --all-features - 416 passed (4 pre-existing failures unrelated to our changes)
```

### Pre-existing Test Failures (Unrelated)

The following tests were already failing before our changes:
- `policy::integrity::tests::test_clear_builtin_preserves_user_data`
- `tools::builtin::thinking::tests::test_think_tool_basic`
- `tui_new::widgets::command_autocomplete::tests::test_autocomplete_move_down_scrolls_viewport`
- `tui_new::widgets::command_autocomplete::tests::test_slash_command_find_matches`

These failures are in different modules (policy, thinking tool, command autocomplete) and not related to provider switching or max_iterations.

### Manual Testing Recommended

1. **Max iterations**:
   - Set `max_iterations = 100` in `~/.config/tark/config.toml`
   - Run a complex task requiring >10 steps
   - Verify it continues past 10 iterations

2. **Provider fallback**:
   - Switch to Gemini without auth configured
   - Verify error message shows: "Gemini authentication required\n\nFalling back to: <previous_provider>"
   - Verify status bar shows previous provider
   - Verify sending messages uses previous provider

## Configuration

Users can now configure max iterations in `~/.config/tark/config.toml`:

```toml
[agent]
# Maximum number of agent steps (tool calls) per message
# Increase for complex multi-step tasks
max_iterations = 50  # Or 100, 200, etc.
```

## Files Modified

- `src/ui_backend/service.rs` - Main changes for both fixes

## Commit Message Suggestion

```
fix(tui): apply max_iterations config and fix provider switch fallback

- TUI now uses max_iterations from config instead of hardcoded 10
- Provider/model switches now revert UI state on failure
- Session metadata only persisted after successful provider switch
- Error messages now show fallback provider/model

Fixes issues where:
1. Agent would stop at 10 iterations even with higher config
2. UI showed new provider while backend used old one after auth failure
```
