# UI Backend BDD Feature Tests

This directory contains Behavior-Driven Development (BDD) tests for the UI Backend (BFF) layer.

## Features

### 01_app_service_commands.feature
Tests the `AppService` command handling:
- Agent mode cycling (Build/Plan/Ask)
- Build mode cycling (Manual/Balanced/Careful)
- UI toggles (sidebar, thinking display)
- Input handling (typing, cursor movement)
- Message sending
- Provider/Model selection
- Context file management
- Quit command

### 02_shared_state.feature
Tests the `SharedState` thread-safe state management:
- Thread safety (concurrent reads/writes)
- State getters
- State setters
- Collection management (messages, context files)
- Default values

### 03_event_publishing.feature
Tests async event publishing from backend to frontend:
- LLM events (started, chunks, completed, error)
- Tool events (started, completed, failed)
- UI state events (message added, provider/model changed)
- Session events
- Event ordering guarantees

### 04_provider_model_management.feature
Tests LLM provider and model management:
- Provider listing
- Model listing by provider
- Provider selection
- Model selection
- Configuration status checking

### 05_keyboard_command_mapping.feature
Tests keyboard event to command translation:
- Application control (Ctrl+C, Ctrl+Q)
- Focus management (Tab, Shift+Tab)
- Mode cycling (Ctrl+M)
- UI toggles (Ctrl+B, Ctrl+T)
- Text editing (characters, backspace, delete)
- Cursor movement (arrows, Home/End, word navigation)
- History navigation (Up/Down)
- Modal interaction (Escape)

### 06_ui_renderer_contract.feature
Tests the `UiRenderer` trait implementation contract:
- Renderer initialization
- Render method behavior
- Modal display
- Status bar updates
- Size reporting
- Quit status checking
- Integration with SharedState
- Error handling

## Running the Tests

```bash
# Run all UI Backend BDD tests
cargo test --test ui_backend_bdd

# Run specific feature
cargo test --test ui_backend_bdd -- 01_app_service

# Run with specific tag
cargo test --test ui_backend_bdd -- @thread_safety
```

## Test Tags

- `@ui_backend` - All UI Backend tests
- `@app_service` - AppService tests
- `@shared_state` - SharedState tests
- `@events` - Event publishing tests
- `@providers` - Provider management tests
- `@models` - Model management tests
- `@keybindings` - Keyboard mapping tests
- `@renderer` - UiRenderer trait tests

## Implementation Status

- [ ] 01_app_service_commands - Step definitions needed
- [ ] 02_shared_state - Step definitions needed
- [ ] 03_event_publishing - Step definitions needed
- [ ] 04_provider_model_management - Step definitions needed
- [ ] 05_keyboard_command_mapping - Step definitions needed
- [ ] 06_ui_renderer_contract - Step definitions needed

## Next Steps

1. Create `tests/ui_backend_bdd.rs` - Main test harness
2. Create step definitions in `tests/bdd/ui_backend/step_definitions/`
3. Implement mock UiRenderer for testing
4. Run tests and iterate
