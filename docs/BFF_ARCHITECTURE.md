# BFF (Backend-for-Frontend) Architecture

**Last Updated**: 2026-01-19  
**Status**: ✅ Production Ready

## Overview

Tark uses a Backend-for-Frontend (BFF) pattern to separate business logic from UI rendering, enabling support for multiple frontends (TUI, Web, Desktop) with a single codebase.

## Layer Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    PRESENTATION LAYER                            │
│                                                                 │
│  ┌──────────┐    ┌──────────┐    ┌──────────┐                  │
│  │   TUI    │    │  Web UI  │    │ Desktop  │  (Frontends)     │
│  │(tui_new) │    │ (future) │    │ (future) │                  │
│  └─────┬────┘    └─────┬────┘    └─────┬────┘                  │
│        │                │                │                      │
│        └────────────────┼────────────────┘                      │
│                         │                                       │
│                         ▼                                       │
│                  implements UiRenderer trait                    │
│                         │                                       │
│            ┌────────────┴────────────┐                          │
│            │  • render(state)        │                          │
│            │  • poll_input(state)    │                          │
│            │  • handle_event(event)  │                          │
│            └─────────────────────────┘                          │
└─────────────────────────────────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────────┐
│                      BFF LAYER                                   │
│                  (ui_backend module)                             │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              AppService (Orchestrator)                   │   │
│  │  • Routes commands to appropriate service                │   │
│  │  • Coordinates cross-service operations                  │   │
│  │  • Emits AppEvents for UI updates                        │   │
│  └────────────────────┬─────────────────────────────────────┘   │
│                       │                                         │
│                       │ delegates to                            │
│                       ▼                                         │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              Domain Services (4)                         │   │
│  │                                                          │   │
│  │  ┌──────────────────┐    ┌──────────────────┐          │   │
│  │  │ Conversation     │    │  Catalog         │          │   │
│  │  │ Service          │    │  Service         │          │   │
│  │  ├──────────────────┤    ├──────────────────┤          │   │
│  │  │ • Streaming      │    │ • Providers      │          │   │
│  │  │ • Context mgmt   │    │ • Models         │          │   │
│  │  │ • Thinking       │    │ • Capabilities   │          │   │
│  │  │ • Memory         │    │ • Authentication │          │   │
│  │  └──────────────────┘    └──────────────────┘          │   │
│  │                                                         │   │
│  │  ┌──────────────────┐    ┌──────────────────┐          │   │
│  │  │ ToolExecution    │    │  Storage         │          │   │
│  │  │ Service          │    │  Facade          │          │   │
│  │  ├──────────────────┤    ├──────────────────┤          │   │
│  │  │ • Tool listing   │    │ • Sessions       │          │   │
│  │  │ • Availability   │    │ • Config         │          │   │
│  │  │ • Risk levels    │    │ • Rules          │          │   │
│  │  │ • Approvals      │    │ • Plugins        │          │   │
│  │  └──────────────────┘    └──────────────────┘          │   │
│  └──────────────────────────────────────────────────────────┘   │
│                       │                                         │
│                       │ updates                                 │
│                       ▼                                         │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │              SharedState                                 │   │
│  │         (Thread-safe, Arc<RwLock<>>)                     │   │
│  │                                                          │   │
│  │  • messages: Vec<Message>                                │   │
│  │  • streaming_content: Option<String>  ✨NEW              │   │
│  │  • streaming_thinking: Option<String> ✨NEW              │   │
│  │  • current_provider: Option<String>                      │   │
│  │  • current_model: Option<String>                         │   │
│  │  • agent_mode: AgentMode                                 │   │
│  │  • build_mode: BuildMode                                 │   │
│  │  • ... (50+ fields)                                      │   │
│  └──────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────────────────┐
│                    DOMAIN LAYER                                  │
│                                                                 │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ChatAgent │  │  Tools   │  │ Storage  │  │   LLM    │       │
│  │          │  │ Registry │  │          │  │ Provider │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
│  ┌──────────┐  ┌──────────┐                                    │
│  │ Context  │  │ ModelsDb │                                    │
│  │ Manager  │  │          │                                    │
│  └──────────┘  └──────────┘                                    │
└─────────────────────────────────────────────────────────────────┘
```

## Data Flow Examples

### Streaming Flow

```
User types "Hello" → Enter
    │
    ▼
TuiRenderer.poll_input() → Command::SendMessage("Hello")
    │
    ▼
TuiController.handle_command()
    │
    ▼
AppService.handle_command(SendMessage)
    │
    ├─ state.add_message(user_msg)
    ├─ state.clear_streaming()  ✨
    └─ agent_bridge.send_message_streaming()
            │
            ▼
    Spawned Task receives BridgeEvents:
        │
        ├─ TextChunk("Hi") 
        │   → state.append_streaming_content("Hi") ✨
        │   → emit AppEvent::LlmTextChunk (trigger refresh)
        │
        ├─ TextChunk(" there")
        │   → state.append_streaming_content(" there") ✨
        │   → emit AppEvent::LlmTextChunk
        │
        └─ Completed
            → finalize from state.streaming_content() ✨
            → state.add_message(assistant_msg)
            → state.clear_streaming()
            → emit AppEvent::LlmCompleted

TuiRenderer.render() reads state.streaming_content ✅
```

**Key**: Single accumulation point in SharedState, renderer just displays it.

### Provider Selection Flow

```
User: /provider
    │
    ▼
TuiController.handle_slash_command("/provider") [async]
    │
    ▼
service.get_providers().await ✨ (no blocking!)
    │
    ▼
CatalogService.list_providers()
    │
    ├─ models_db.list_providers().await
    ├─ Filter by config.enabled_providers
    ├─ Check configuration (API keys)
    └─ Return Vec<ProviderInfo>
        │
        ▼
state.set_available_providers(providers)
state.set_active_modal(ProviderPicker)
    │
    ▼
TuiRenderer.render() shows modal with providers
```

### Tool Execution with Approval

```
Agent wants to run: write_file("main.rs", "content")
    │
    ▼
ToolRegistry.execute("write_file", params)
    │
    ▼
ToolExecutionService.check_approval("write_file", "main.rs", RiskLevel::Write)
    │
    ├─ Check trust level (Manual/Balanced/Careful)
    ├─ Check denial patterns
    ├─ Check session approvals
    ├─ Check persistent approvals
    └─ If no match → Ask user via InteractionRequest
        │
        ▼
    ApprovalCard shown in TUI
        │
        ├─ User: Approve Once → Approved
        ├─ User: Approve Session → Save pattern, Approved
        ├─ User: Always → Persist pattern, Approved
        └─ User: Deny → Denied
```

## Service APIs

### ConversationService

```rust
// Messaging
service.send_message("Hello", &state).await?;
service.interrupt();

// Streaming (BFF owns this)
service.start_streaming(&state);
service.append_chunk("text", &state);
service.append_thinking("reasoning", &state);
let msg = service.finalize_message(&state);

// Context
let usage = service.context_usage();
let percent = service.context_percent();
service.compact_context().await?;

// Memory
service.add_memory(MemoryEntry {
    content: "User prefers Rust",
    importance: MemoryImportance::High,
    ...
}).await;
```

### CatalogService

```rust
// Providers
let providers = service.list_providers().await;
let configured = service.is_provider_configured("openai");

// Models
let models = service.list_models("anthropic").await;
let caps = service.model_capabilities("anthropic", "claude-sonnet-4").await;

// Capabilities
let supports_vision = service.supports_vision("openai", "gpt-4o");
let context_limit = service.context_limit("google", "gemini-2.0");

// Auth
let status = service.auth_status("copilot");
```

### ToolExecutionService

```rust
// Introspection
let tools = service.list_tools(AgentMode::Build);
let risk = service.tool_risk_level("shell");
let available = service.is_available("write_file", AgentMode::Plan);

// Approval
service.set_trust_level(TrustLevel::Balanced).await;
let status = service.check_approval("shell", "rm file.txt", RiskLevel::Risky).await?;

// Patterns
service.remove_persistent_approval(index).await?;
service.clear_session().await;
```

### StorageFacade

```rust
// Sessions
let session = facade.create_session()?;
let all = facade.list_sessions()?;
facade.export_session(&id, &path)?;

// Config
let config = facade.get_config();
facade.save_project_config(&config)?;

// Rules
facade.save_rule("style", "# Rust style", ConfigScope::Project)?;
let rules = facade.get_rules();

// Usage
let tracker = facade.get_usage_tracker()?;
```

## Benefits

| Benefit | Explanation |
|---------|-------------|
| **Multi-Frontend** | Same BFF works with TUI, Web, Desktop via `UiRenderer` trait |
| **Testability** | Each service independently testable with mocked dependencies |
| **Type Safety** | Canonical types prevent drift; typed errors improve UX |
| **Async Correct** | No blocking calls; proper async/await throughout |
| **Single Source** | Streaming accumulates once (in SharedState), impossible to desync |
| **Clear Boundaries** | 4 cohesive services with focused responsibilities |
| **Maintainability** | Small, focused modules instead of god objects |
| **Backward Compatible** | Existing code unchanged; incremental migration |

## Trade-offs

| Trade-off | Decision | Rationale |
|-----------|----------|-----------|
| Service count | 4 (not 9-10) | Avoid over-abstraction; cohesive domains |
| State grouping | Deferred | Keep backward compat; migrate incrementally |
| AgentBridge | Keep for now | Validate services in production first |
| Full integration | Deferred | Add as services are adopted |

## Migration Status

- ✅ Phase 0: Critical fixes (streaming, types, async, errors)
- ✅ Phase 1-4: Core services created and tested
- 🔄 Phase 5: AgentBridge decomposition (deferred)
- 🔄 Phase 6: Full AppService integration (incremental)

## Usage

### Creating a New Frontend

```rust
use tark_cli::ui_backend::{UiRenderer, SharedState, Command, AppEvent};

struct MyFrontend;

impl UiRenderer for MyFrontend {
    fn render(&mut self, state: &SharedState) -> Result<()> {
        // Read from state and display
        let messages = state.messages();
        let streaming = state.streaming_content();
        // ... render to your UI
    }
    
    fn poll_input(&mut self, state: &SharedState) -> Result<Option<Command>> {
        // Convert user input to Command
        // Return Some(Command::SendMessage("...")) etc.
    }
    
    fn handle_event(&mut self, event: &AppEvent, state: &SharedState) -> Result<()> {
        // React to events (LlmTextChunk, ProviderChanged, etc.)
        // Trigger UI refresh
    }
    
    fn should_quit(&self, state: &SharedState) -> bool {
        state.should_quit()
    }
    
    fn get_size(&self) -> (u16, u16) {
        // Return UI dimensions
    }
}

// Use with AppService
let (tx, rx) = mpsc::unbounded_channel();
let service = AppService::new(working_dir, tx)?;
let renderer = MyFrontend::new();
let controller = MyController::new(service, renderer, rx);

controller.run().await?;
```

### Using Services Directly

```rust
use tark_cli::ui_backend::{
    ConversationService, CatalogService,
    ToolExecutionService, StorageFacade
};

// Conversation
let conv_svc = ConversationService::new(agent, event_tx);
conv_svc.send_message("Hello", &state).await?;

// Catalog
let cat_svc = CatalogService::new();
let providers = cat_svc.list_providers().await;

// Tools
let tool_svc = ToolExecutionService::new(AgentMode::Build, None);
let tools = tool_svc.list_tools(AgentMode::Build);

// Storage
let storage = StorageFacade::new(project_dir)?;
let sessions = storage.list_sessions()?;
```

## Design Principles

1. **Single Source of Truth**: All state changes flow through SharedState
2. **Event-Driven**: Async updates via AppEvent channel
3. **Command Pattern**: All user actions are Command enum variants
4. **Trait Abstraction**: UiRenderer enables any frontend
5. **Typed Errors**: Service-specific errors for better handling
6. **Canonical Types**: One definition, re-exported everywhere
7. **Async First**: No blocking calls in async context
8. **Thread Safety**: Arc<RwLock<>> for shared state

## Files

### Core BFF Files

- `src/ui_backend/service.rs` - AppService orchestrator
- `src/ui_backend/state.rs` - SharedState (thread-safe)
- `src/ui_backend/commands.rs` - Command enum
- `src/ui_backend/events.rs` - AppEvent enum
- `src/ui_backend/traits.rs` - UiRenderer trait
- `src/ui_backend/errors.rs` - Typed errors
- `src/ui_backend/types.rs` - Shared data types

### Service Files

- `src/ui_backend/conversation.rs` - ConversationService
- `src/ui_backend/catalog.rs` - CatalogService
- `src/ui_backend/tool_execution.rs` - ToolExecutionService
- `src/ui_backend/storage_facade.rs` - StorageFacade

### Canonical Types

- `src/core/types.rs` - AgentMode, BuildMode, ThinkLevel

### Tests

- `tests/ui_backend_conversation_test.rs` - 12 tests
- `tests/ui_backend_catalog_test.rs` - 11 tests
- `tests/ui_backend_tool_test.rs` - 10 tests
- `tests/ui_backend_storage_test.rs` - 14 tests

## Testing

```bash
# Run all BFF service tests
cargo test --test ui_backend_conversation_test
cargo test --test ui_backend_catalog_test
cargo test --test ui_backend_tool_test
cargo test --test ui_backend_storage_test

# Run all library tests
cargo test --lib

# Should see: 793+ tests passing ✅
```

## Comparison

### Before BFF (Old Architecture)

```
TuiApp (8000 lines god object)
    ├─ AppState (mutable, 50+ fields)
    ├─ Event loop
    ├─ Rendering
    ├─ Business logic
    └─ AgentBridge wrapper

Problems:
- Tight coupling (can't swap TUI)
- Dual streaming accumulation (renderer + service)
- Type drift (3 AgentMode definitions)
- Blocking async calls (block_in_place)
- Untyped errors (anyhow::Error)
```

### After BFF (New Architecture)

```
TuiController (686 lines orchestrator)
    ├─ TuiRenderer (implements UiRenderer trait)
    └─ AppService
        ├─ ConversationService
        ├─ CatalogService
        ├─ ToolExecutionService
        └─ StorageFacade

Benefits:
✅ Loose coupling (swap TUI for Web/Desktop)
✅ Single streaming accumulation (SharedState)
✅ Zero type drift (canonical types in core::types)
✅ Proper async (no blocking)
✅ Typed errors (programmatic handling)
```

## Future Work

- Integrate services into AppService (gradual replacement of AgentBridge calls)
- Add state grouping (ConversationState, CatalogState, UiState nested structs)
- Build Web UI using same BFF services
- Build Desktop UI using same BFF services
- Add more integration tests for complex workflows

## References

- [TUI_LLD_FLOW.md](TUI_LLD_FLOW.md) - Detailed data flow diagrams
- [BFF_MIGRATION_COMPLETE.md](BFF_MIGRATION_COMPLETE.md) - Migration completion summary
- Plan: `/root/.cursor/plans/bff_layer_migration_9f8d19ba.plan.md`
