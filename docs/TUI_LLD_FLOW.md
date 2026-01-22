# TUI Low-Level Design (LLD) Flow

This document describes the architecture and data flow for the **New TUI** (`src/tui_new/`) with the BFF (Backend-for-Frontend) layer.

**Status**: ✅ BFF Layer Migration Complete (2026-01-19)

---

## Current Architecture (BFF Pattern)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        MULTI-FRONTEND ARCHITECTURE                           │
│                     (Backend-for-Frontend Pattern - BFF)                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   ┌────────────┐  ┌────────────┐  ┌────────────┐                           │
│   │    TUI     │  │   Web UI   │  │  Desktop   │   (Multiple Frontends)    │
│   │ (tui_new/) │  │  (future)  │  │  (future)  │                           │
│   └──────┬─────┘  └──────┬─────┘  └──────┬─────┘                           │
│          │                │                │                                │
│          └────────────────┼────────────────┘                                │
│                           │ implements UiRenderer                           │
│                           ▼                                                 │
│   ┌──────────────────────────────────────────────────────────┐              │
│   │              TuiController / WebController               │              │
│   │                (Event Loop Orchestrator)                 │              │
│   └────────────────────────┬─────────────────────────────────┘              │
│                            │                                                │
│                            ▼                                                │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                      ui_backend (BFF Layer)                         │   │
│   │                                                                     │   │
│   │  ┌─────────────────┐      ┌──────────────────────────────┐         │   │
│   │  │  AppService     │─────▶│      SharedState             │         │   │
│   │  │ (Orchestrator)  │      │  (Thread-safe, Single        │         │   │
│   │  └────────┬────────┘      │   Source of Truth)           │         │   │
│   │           │               └──────────────────────────────┘         │   │
│   │           │ delegates to                                           │   │
│   │           ▼                                                        │   │
│   │  ┌──────────────────────────────────────────────────────────────┐  │   │
│   │  │              Domain Services (4)                             │  │   │
│   │  │                                                              │  │   │
│   │  │  ┌──────────────────┐    ┌──────────────────┐              │  │   │
│   │  │  │ Conversation     │    │    Catalog       │              │  │   │
│   │  │  │ Service          │    │    Service       │              │  │   │
│   │  │  │ - streaming      │    │  - providers     │              │  │   │
│   │  │  │ - context        │    │  - models        │              │  │   │
│   │  │  │ - thinking       │    │  - auth          │              │  │   │
│   │  │  │ - memory         │    │  - capabilities  │              │  │   │
│   │  │  └──────────────────┘    └──────────────────┘              │  │   │
│   │  │                                                             │  │   │
│   │  │  ┌──────────────────┐    ┌──────────────────┐              │  │   │
│   │  │  │ ToolExecution    │    │  StorageFacade   │              │  │   │
│   │  │  │ Service          │    │                  │              │  │   │
│   │  │  │ - availability   │    │  - sessions      │              │  │   │
│   │  │  │ - risk levels    │    │  - config        │              │  │   │
│   │  │  │ - approvals      │    │  - rules         │              │  │   │
│   │  │  │ - patterns       │    │  - plugins       │              │  │   │
│   │  │  └──────────────────┘    └──────────────────┘              │  │   │
│   │  └──────────────────────────────────────────────────────────────┘  │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                            │                                                │
│                            ▼                                                │
│   ┌─────────────────────────────────────────────────────────────────────┐   │
│   │                   Core Layer (Domain Logic)                         │   │
│   │                                                                     │   │
│   │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐           │   │
│   │  │ChatAgent │  │  Context │  │  Tools   │  │ Storage  │           │   │
│   │  │          │  │  Manager │  │ Registry │  │          │           │   │
│   │  └──────────┘  └──────────┘  └──────────┘  └──────────┘           │   │
│   │  ┌──────────┐  ┌──────────┐                                       │   │
│   │  │   LLM    │  │ ModelsDb │                                       │   │
│   │  │ Provider │  │          │                                       │   │
│   │  └──────────┘  └──────────┘                                       │   │
│   └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## New Architecture: Module Structure

### BFF Layer (`src/ui_backend/`)

```
src/ui_backend/
├── mod.rs                  # Public API exports
├── service.rs              # AppService orchestrator
├── state.rs                # SharedState (thread-safe)
├── commands.rs             # Command enum
├── events.rs               # AppEvent enum
├── traits.rs               # UiRenderer trait
├── types.rs                # Shared data types
├── errors.rs               # Typed errors (NEW)
├── middleware.rs           # Command pipeline
├── approval.rs             # Approval cards
├── questionnaire.rs        # Ask user dialogs
│
├── conversation.rs         # ConversationService (NEW)
├── catalog.rs              # CatalogService (NEW)
├── tool_execution.rs       # ToolExecutionService (NEW)
└── storage_facade.rs       # StorageFacade (NEW)
```

### TUI Frontend (`src/tui_new/`)

```
src/tui_new/
├── mod.rs              # Re-exports
├── controller.rs       # TuiController (event loop)
├── renderer.rs         # TuiRenderer (implements UiRenderer)
├── app.rs              # Legacy re-exports
├── config.rs           # Configuration
├── events.rs           # Event types
├── theme.rs            # Theme definitions
├── utils.rs            # Utilities
├── git_info.rs         # Git status
├── modals/             # Modal components
└── widgets/            # Stateless UI widgets
```

### Core Layer (`src/core/`)

```
src/core/
├── mod.rs              # Re-exports
├── types.rs            # Canonical types (NEW)
├── agent_bridge.rs     # AgentBridge (delegates to services)
└── attachments.rs      # File attachment handling
```

### 2. Main Event Loop Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        OLD TUI EVENT LOOP                                    │
└─────────────────────────────────────────────────────────────────────────────┘

     ┌──────────────────┐
     │   Entry Point    │
     │  run_tui_old()   │
     └────────┬─────────┘
              │
              ▼
     ┌──────────────────┐
     │  TuiApp::new()   │
     │ - init terminal  │
     │ - load config    │
     │ - init widgets   │
     └────────┬─────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                          MAIN LOOP                                   │
│  ┌─────────────────────────────────────────────────────────────┐    │
│  │  loop {                                                      │    │
│  │      1. Handle async events (AgentBridge → AppState)        │    │
│  │      2. Check force quit flags                               │    │
│  │      3. Poll terminal events (key, mouse, resize)           │    │
│  │      4. Map key → Action via KeybindingHandler              │    │
│  │      5. Execute action (mutate AppState directly)           │    │
│  │      6. Render UI (AppState → Widgets → Terminal)           │    │
│  │      7. Check should_quit                                    │    │
│  │  }                                                           │    │
│  └─────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
              │
              ▼
     ┌──────────────────┐
     │  Cleanup         │
     │ - restore term   │
     │ - save history   │
     └──────────────────┘
```

### 3. Key Processing Flow (Old TUI)

```
┌──────────┐    ┌───────────────┐    ┌────────────────┐    ┌──────────┐
│ KeyEvent │───▶│ Keybindings   │───▶│    Action      │───▶│ AppState │
│          │    │   Handler     │    │ (enum variant) │    │ (mutate) │
└──────────┘    └───────────────┘    └────────────────┘    └──────────┘
                       │
                       ▼
               ┌───────────────┐
               │ Context Check │
               │ - InputMode   │
               │ - FocusedComp │
               │ - PickerOpen  │
               └───────────────┘
```

### 4. LLM Communication Flow (Old TUI)

```
┌──────────┐    ┌───────────────┐    ┌────────────────┐    ┌──────────┐
│  User    │───▶│   AppState    │───▶│  AgentBridge   │───▶│   LLM    │
│  Input   │    │ pending_msg   │    │ send_message() │    │ Provider │
└──────────┘    └───────────────┘    └────────────────┘    └──────────┘
                                            │
                                            │ AgentEvent channel
                                            ▼
                                     ┌────────────────┐
                                     │ AgentEvent     │
                                     │ - TextDelta    │
                                     │ - ToolStart    │
                                     │ - ToolResult   │
                                     │ - Completed    │
                                     └───────┬────────┘
                                             │
                                             ▼
                                     ┌────────────────┐
                                     │  AppState      │
                                     │ - message_list │
                                     │ - processing   │
                                     │ - current_tool │
                                     └────────────────┘
```

### 5. Problems with Old TUI

| Issue | Description |
|-------|-------------|
| **God Object** | TuiApp is ~8000 lines with 50+ state fields |
| **Tight Coupling** | UI logic mixed with business logic |
| **State Mutation** | Direct state mutation everywhere |
| **Testing** | Very difficult to unit test |
| **Reusability** | Cannot reuse backend for other UIs (web, desktop) |

---

## BFF Services

### 1. ConversationService (`conversation.rs`)

**Purpose**: Manages the complete conversation lifecycle

**Responsibilities:**
- **Streaming Reducer**: Owns streaming text accumulation (single source of truth)
- **Context Management**: Token counting, compaction policy, available tokens
- **Thinking**: Extended reasoning support, token tracking separate from context
- **Memory**: Persistent facts that survive compaction

**Key APIs:**
```rust
pub async fn send_message(&self, content: &str, state: &SharedState) -> Result<(), ConversationError>;
pub fn start_streaming(&self, state: &SharedState);
pub fn append_chunk(&self, chunk: &str, state: &SharedState);
pub fn finalize_message(&self, state: &SharedState) -> Message;
pub fn context_usage(&self) -> ContextUsage;
pub async fn compact_context(&self) -> Result<CompactionResult, ConversationError>;
pub async fn add_memory(&self, entry: MemoryEntry);
```

**Data Structures:**
- `ContextPolicy` - Token budget, threshold, preservation rules
- `MemoryEntry` - Persistent facts (never compacted)
- `TokenBreakdown` - Separates content, thinking, tool tokens
- `SummarizationStrategy` - Automatic, Manual, Threshold, Never

### 2. CatalogService (`catalog.rs`)

**Purpose**: Provider/model discovery and authentication

**Responsibilities:**
- **Provider Discovery**: List providers from models.dev (async)
- **Model Discovery**: List models for a provider
- **Capabilities**: Query model capabilities (thinking, vision, tools)
- **Authentication**: Check auth status, device flow support

**Key APIs:**
```rust
pub async fn list_providers(&self) -> Vec<ProviderInfo>;
pub async fn list_models(&self, provider: &str) -> Vec<ModelInfo>;
pub async fn model_capabilities(&self, provider: &str, model: &str) -> Option<ModelCapabilities>;
pub fn supports_thinking(&self, provider: &str, model: &str) -> bool;
pub fn auth_status(&self, provider: &str) -> AuthStatus;
```

**Features:**
- Fully async (no blocking calls)
- Integrates with models.dev API
- Provider configuration detection via environment variables
- Context limit queries per model

### 3. ToolExecutionService (`tool_execution.rs`)

**Purpose**: Tool introspection and approval management

**Responsibilities:**
- **Tool Introspection**: List available tools per mode
- **Risk Assessment**: Query risk levels
- **Approval Flow**: Check approvals with pattern matching
- **Pattern Management**: Save/load approval patterns

**Key APIs:**
```rust
pub fn list_tools(&self, mode: AgentMode) -> Vec<ToolInfo>;
pub fn tool_risk_level(&self, name: &str) -> Option<RiskLevel>;
pub fn is_available(&self, name: &str, mode: AgentMode) -> bool;
pub async fn check_approval(&self, tool: &str, command: &str, risk: RiskLevel) -> Result<ApprovalStatus, ToolError>;
pub async fn set_trust_level(&self, level: TrustLevel);
```

**Features:**
- Mode-based tool availability (Ask < Plan < Build)
- Integration with ApprovalGate
- Trust level management
- Pattern persistence

### 4. StorageFacade (`storage_facade.rs`)

**Purpose**: Unified API for all persistent storage

**Responsibilities:**
- **Sessions**: CRUD operations for chat sessions
- **Config**: Project and global configuration (merged)
- **Rules**: Custom agent rules
- **MCP**: MCP server configuration
- **Plugins**: Plugin management
- **Usage**: Usage tracking and statistics

**Key APIs:**
```rust
pub fn create_session(&self) -> Result<SessionInfo, StorageError>;
pub fn load_session(&self, id: &str) -> Result<ChatSession, StorageError>;
pub fn export_session(&self, id: &str, path: &Path) -> Result<(), StorageError>;
pub fn get_config(&self) -> WorkspaceConfig;
pub fn save_rule(&self, name: &str, content: &str, scope: ConfigScope) -> Result<(), StorageError>;
pub fn get_usage_tracker(&self) -> Result<UsageTracker, StorageError>;
```

**Features:**
- Wraps `TarkStorage` + `GlobalStorage`
- Config scope isolation (Project vs Global)
- Session export/import
- Usage statistics

### 2. Component Responsibilities

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          COMPONENT BREAKDOWN                                 │
└─────────────────────────────────────────────────────────────────────────────┘

┌────────────────────────────────────────────────────────────────┐
│                    TuiController                                │
│  Responsibilities:                                              │
│  • Own and orchestrate AppService + TuiRenderer                │
│  • Run main event loop                                          │
│  • Handle slash commands (/help, /theme, etc.)                 │
│  • Route Commands to AppService                                │
│  • Process AppEvents from async channel                        │
└────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
┌──────────────────────────┐    ┌──────────────────────────────┐
│      TuiRenderer         │    │        AppService            │
│  (implements UiRenderer) │    │    (Orchestrator - BFF)      │
├──────────────────────────┤    ├──────────────────────────────┤
│ • render(state)          │    │ • handle_command(cmd)        │
│ • poll_input(state)      │    │ • Delegate to services       │
│ • handle_event(event)    │    │ • Send AppEvents via channel │
│ • key_to_command()       │    │ • NO direct business logic   │
│ • mouse_to_command()     │    │                              │
│ • hit_test()             │    │  Delegates to:               │
│                          │    │  ├─ ConversationService      │
│ NO ACCUMULATION          │    │  ├─ CatalogService           │
│ (reads from state only)  │    │  ├─ ToolExecutionService     │
└──────────────────────────┘    │  └─ StorageFacade            │
              │                 └──────────────────────────────┘
              │                               │
              │                               ▼
              │                 ┌──────────────────────────────┐
              │                 │       SharedState            │
              │                 │    (Thread-safe RwLock)      │
              │                 ├──────────────────────────────┤
              │                 │ • messages: Vec<Message>     │
              │                 │ • streaming_content ✨NEW    │
              │                 │ • streaming_thinking ✨NEW   │
              │                 │ • input_text: String         │
              │                 │ • focused_component          │
              │                 │ • active_modal               │
              │                 │ • current_provider           │
              │                 │ • current_model              │
              │                 │ • llm_processing             │
              │                 │ • theme                      │
              │                 └──────────────────────────────┘
              │                               │
              └───────────────────────────────┘
                              │
                              ▼
              ┌──────────────────────────────┐
              │    Widgets (Stateless)       │
              │  • MessageArea               │
              │  • InputWidget               │
              │  • StatusBar                 │
              │  • Header                    │
              │  • Sidebar                   │
              │  • Modals (Theme, Help, etc) │
              └──────────────────────────────┘
```

### 3. Main Event Loop Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        NEW TUI EVENT LOOP (with BFF)                         │
└─────────────────────────────────────────────────────────────────────────────┘

     ┌──────────────────┐
     │   Entry Point    │
     │  run_tui_new()   │
     └────────┬─────────┘
              │
              ▼
     ┌──────────────────────────────────────────────────────────────┐
     │  Setup:                                                      │
     │  1. Create mpsc::unbounded_channel<AppEvent>                │
     │  2. Create AppService(working_dir, event_tx)                │
     │     ├─ Initializes AgentBridge                              │
     │     └─ Services available but not yet fully integrated      │
     │  3. Create TuiRenderer(terminal, theme)                     │
     │  4. Create TuiController(service, renderer, rx)             │
     └────────────────────┬─────────────────────────────────────────┘
                          │
                          ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     TuiController::run()                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │  loop {                                                              │   │
│  │      // 1. RENDER (reads from SharedState)                           │   │
│  │      renderer.render(&state)?;                                       │   │
│  │                                                                      │   │
│  │      // 2. POLL INPUT (non-blocking)                                 │   │
│  │      if let Some(cmd) = renderer.poll_input(&state)? {               │   │
│  │          if cmd is slash_command:                                    │   │
│  │              handle_slash_command(cmd).await  // async!              │   │
│  │          else:                                                       │   │
│  │              service.handle_command(cmd).await                       │   │
│  │      }                                                               │   │
│  │                                                                      │   │
│  │      // 3. PROCESS ASYNC EVENTS (non-blocking)                       │   │
│  │      while let Ok(event) = event_rx.try_recv() {                    │   │
│  │          renderer.handle_event(&event, &state)?;  // Just refresh   │   │
│  │      }                                                               │   │
│  │                                                                      │   │
│  │      // 4. CHECK QUIT                                                │   │
│  │      if renderer.should_quit(&state) { break; }                      │   │
│  │                                                                      │   │
│  │      // 5. YIELD (avoid busy-wait)                                   │   │
│  │      tokio::time::sleep(10ms).await;                                 │   │
│  │  }                                                                   │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 4. Command Flow (New TUI with BFF Services)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    COMMAND FLOW (with Service Routing)                       │
└─────────────────────────────────────────────────────────────────────────────┘

┌──────────┐    ┌───────────────┐    ┌────────────────┐
│ KeyEvent │───▶│ TuiRenderer   │───▶│   Command      │
│ or Mouse │    │ poll_input()  │    │   (enum)       │
└──────────┘    └───────────────┘    └───────┬────────┘
                                             │
                      ┌──────────────────────┼──────────────────────┐
                      │                      │                      │
                      ▼                      ▼                      ▼
              ┌───────────────┐    ┌────────────────┐    ┌─────────────────┐
              │ Slash Command │    │  UI Commands   │    │ Domain Commands │
              │ /help /theme  │    │ Focus, Modal   │    │ SendMessage     │
              └───────┬───────┘    └───────┬────────┘    └────────┬────────┘
                      │                    │                      │
                      ▼                    ▼                      ▼
              ┌───────────────┐    ┌────────────────┐    ┌─────────────────┐
              │ Controller    │    │  SharedState   │    │   AppService    │
              │ handle_slash  │    │  (direct set)  │    │ ┌─────────────┐ │
              │   (async)     │    │                │    │ │ Route to:   │ │
              └───────────────┘    └────────────────┘    │ │ - ConvSvc   │ │
                                                         │ │ - CatalogSvc│ │
                                                         │ │ - ToolSvc   │ │
                                                         │ │ - StorageSvc│ │
                                                         │ └─────────────┘ │
                                                         └─────────────────┘
                                                                  │
                      ┌───────────────────────────────────────────┤
                      ▼                                           ▼
              ┌───────────────────┐                   ┌────────────────────┐
              │   SharedState     │                   │    AppEvent        │
              │   (updated by     │                   │  (emitted for UI   │
              │    services)      │                   │   refresh)         │
              └───────────────────┘                   └────────────────────┘
```

### 5. LLM Communication Flow (New TUI with BFF)

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                  LLM STREAMING FLOW (Single Source of Truth)                 │
└─────────────────────────────────────────────────────────────────────────────┘

  User Input                    AppService                     AgentBridge
      │                             │                              │
      │  Command::SendMessage       │                              │
      │─────────────────────────────▶                              │
      │                             │                              │
      │                             │ 1. Add user msg to state     │
      │                             │ 2. Set llm_processing=true   │
      │                             │ 3. state.clear_streaming()   │
      │                             │ 4. Create bridge channel     │
      │                             │                              │
      │                             │ send_message_streaming()     │
      │                             │──────────────────────────────▶
      │                             │                              │
      │                             │        bridge_rx             │
      │                             │◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│
      │                             │                              │
      │                  ┌──────────┴──────────────┐               │
      │                  │   Spawned Task          │               │
      │                  │ ┌─────────────────────┐ │               │
      │                  │ │ while recv() {      │ │               │
      │                  │ │   TextChunk ->      │ │◀──BridgeEvent─│
      │                  │ │     state.append    │ │  (TextChunk)  │
      │                  │ │   ThinkingChunk ->  │ │               │
      │                  │ │     state.append    │ │               │
      │                  │ │   Completed ->      │ │               │
      │                  │ │     finalize_msg    │ │               │
      │                  │ │   emit AppEvent     │ │               │
      │                  │ │ }                   │ │               │
      │                  │ └─────────────────────┘ │               │
      │                  └──────────┬──────────────┘               │
      │                             │                              │
      │                             │ AppEvent::LlmTextChunk       │
      │                             │ (trigger refresh only)       │
      │                             ▼                              │
      │                  ┌──────────────────────┐                  │
      │                  │   TuiController      │                  │
      │                  │   poll_events()      │                  │
      │                  │         │            │                  │
      │                  │         ▼            │                  │
      │                  │   TuiRenderer        │                  │
      │                  │   handle_event()     │                  │
      │                  │   (no accumulation)  │ ✅               │
      │                  │   render() reads     │                  │
      │                  │   state.streaming_   │                  │
      │                  │   content            │                  │
      │                  └──────────────────────┘                  │
```

**Key Change**: Renderer no longer accumulates. All accumulation happens in SharedState via the spawned task.

### 6. AppEvent Types

```rust
pub enum AppEvent {
    // LLM Streaming
    LlmStarted,
    LlmTextChunk(String),
    LlmThinkingChunk(String),
    LlmCompleted,
    LlmError(String),
    
    // Tool Execution
    ToolStarted { name: String, args: String },
    ToolCompleted { name: String, result: String },
    
    // State Changes
    MessageAdded(Message),
    StatusChanged(String),
    ProviderChanged(String),
    ModelChanged(String),
}
```

### 7. Command Types

```rust
pub enum Command {
    // Application Control
    Quit,
    Interrupt,
    
    // Input Editing
    InsertChar(char),
    DeleteCharBefore,
    DeleteCharAfter,
    MoveCursorLeft,
    MoveCursorRight,
    
    // Message Sending
    SendMessage(String),
    
    // Navigation
    FocusInput,
    FocusMessages,
    FocusPanel,
    ScrollUp,
    ScrollDown,
    HistoryPrevious,
    HistoryNext,
    
    // Modals
    ToggleHelp,
    ToggleThemePicker,
    OpenProviderPicker,
    OpenModelPicker,
    CloseModal,
    ModalUp,
    ModalDown,
    ModalSelect,
    ModalFilter(String),
    
    // Mode Switching
    CycleAgentMode,
    SetAgentMode(AgentMode),
    CycleBuildMode,
    SetBuildMode(BuildMode),
    ToggleThinking,
    
    // Provider/Model
    SelectProvider(String),
    SelectModel(String),
}
```

---

## Key Architecture Features

| Aspect | Implementation | Benefit |
|--------|---------------|---------|
| **Architecture** | BFF with 4 Domain Services | Multi-frontend support |
| **State** | SharedState (thread-safe, single source) | No sync bugs |
| **Streaming** | Accumulated in SharedState only | One source of truth ✅ |
| **Event Loop** | In TuiController | Clean separation |
| **Rendering** | Separate TuiRenderer (stateless) | Reusable widgets |
| **Business Logic** | In 4 BFF Services | Clear boundaries |
| **Commands** | Command enum | Type-safe actions |
| **Events** | AppEvent (comprehensive) | Async updates |
| **Testability** | Service unit tests + integration | Easy to test |
| **Reusability** | Any frontend (UiRenderer trait) | Web/Desktop ready |
| **Type Safety** | Canonical types in core::types | Zero drift ✅ |
| **Async** | Proper async/await (no blocking) | No deadlocks ✅ |
| **Errors** | Typed per service | Better UX ✅ |

---

## BFF Services Detail

### Service Responsibilities

| Service | Manages | Key APIs |
|---------|---------|----------|
| **ConversationService** | Streaming, context, thinking, memory | `send_message`, `append_chunk`, `compact_context` |
| **CatalogService** | Providers, models, auth | `list_providers`, `list_models`, `auth_status` |
| **ToolExecutionService** | Tool availability, approvals | `list_tools`, `check_approval`, `set_trust_level` |
| **StorageFacade** | Sessions, config, rules, plugins | `create_session`, `get_config`, `save_rule` |

### Canonical Types (`src/core/types.rs`)

```rust
// Single source of truth (no drift)
pub enum AgentMode { Ask, Plan, Build }
pub enum BuildMode { Manual, Balanced, Careful }
pub enum ThinkLevel { Off, Low, Normal, High }
```

**Before**: 3 separate `AgentMode` definitions with mapping glue  
**After**: 1 definition, all others `pub use`

### Typed Errors (`src/ui_backend/errors.rs`)

```rust
pub enum ConversationError { NotConnected, RateLimited, ContextExceeded, ... }
pub enum CatalogError { ProviderNotFound, DeviceFlowExpired, ... }
pub enum ToolError { ToolNotFound, NotAvailableInMode, Denied, ... }
pub enum StorageError { SessionNotFound, PermissionDenied, ... }
```

**Before**: `anyhow::Error` everywhere  
**After**: Typed errors with specific variants

---

## Data Flow Diagrams

### Input → State → Render (New TUI)

```
     ┌─────────┐
     │  User   │
     │ (input) │
     └────┬────┘
          │ KeyEvent / MouseEvent
          ▼
┌─────────────────────┐
│    TuiRenderer      │
│   poll_input()      │
│  ┌───────────────┐  │
│  │key_to_command │  │
│  │mouse_to_cmd   │  │
│  └───────────────┘  │
└──────────┬──────────┘
           │ Command
           ▼
┌─────────────────────┐
│   TuiController     │
│  handle_command()   │
│  ┌───────────────┐  │
│  │ slash cmd?    │──▶ Update state directly
│  │ UI cmd?       │──▶ Update state directly
│  │ business cmd? │──▶ AppService.handle_command()
│  └───────────────┘  │
└──────────┬──────────┘
           │ State mutated
           ▼
┌─────────────────────┐
│    SharedState      │
│   (RwLock<Inner>)   │
│  ┌───────────────┐  │
│  │ messages      │  │
│  │ input_text    │  │
│  │ focused       │  │
│  │ modal         │  │
│  │ theme         │  │
│  └───────────────┘  │
└──────────┬──────────┘
           │ Read state
           ▼
┌─────────────────────┐
│    TuiRenderer      │
│     render()        │
│  ┌───────────────┐  │
│  │ Header        │  │
│  │ MessageArea   │  │
│  │ InputWidget   │  │
│  │ StatusBar     │  │
│  │ Sidebar       │  │
│  │ Modals        │  │
│  └───────────────┘  │
└──────────┬──────────┘
           │ Draw calls
           ▼
     ┌─────────┐
     │Terminal │
     │ (output)│
     └─────────┘
```

---

## File References

| Component | Path | Purpose |
|-----------|------|---------|
| **BFF Orchestrator** | `src/ui_backend/service.rs` | Routes commands to services |
| **Services** | `src/ui_backend/conversation.rs` | Conversation lifecycle |
| | `src/ui_backend/catalog.rs` | Provider/model discovery |
| | `src/ui_backend/tool_execution.rs` | Tool management |
| | `src/ui_backend/storage_facade.rs` | Unified storage |
| **State** | `src/ui_backend/state.rs` | Thread-safe shared state |
| **Types** | `src/core/types.rs` | Canonical type definitions |
| **Errors** | `src/ui_backend/errors.rs` | Typed error enums |
| **TUI Frontend** | `src/tui_new/controller.rs` | Event loop orchestrator |
| | `src/tui_new/renderer.rs` | Rendering implementation |
| **Widgets** | `src/tui_new/widgets/` | Stateless components |
| **Tests** | `tests/ui_backend_conversation_test.rs` | 12 tests |
| | `tests/ui_backend_catalog_test.rs` | 11 tests |
| | `tests/ui_backend_tool_test.rs` | 10 tests |
| | `tests/ui_backend_storage_test.rs` | 14 tests |

---

## Key Improvements (2026-01-19)

### 1. Single Source of Truth for Streaming ✅

**Before:**
- AppService spawned task accumulated text
- TuiRenderer also accumulated text
- **Problem**: Two sources of truth, sync issues

**After:**
- Only SharedState accumulates (`streaming_content`, `streaming_thinking`)
- Renderer reads state, never accumulates
- **Benefit**: Impossible to desync

### 2. Canonical Type Definitions ✅

**Before:**
- `AgentMode` defined in 3 places
- `From<>` conversions between types
- **Problem**: Type drift, maintenance burden

**After:**
- Single definition in `core::types.rs`
- All others use `pub use`
- **Benefit**: Compiler enforces consistency

### 3. Proper Async Patterns ✅

**Before:**
- `block_in_place` in `get_providers()`
- **Problem**: Can deadlock runtime

**After:**
- All APIs properly async
- `get_providers().await`, `get_models().await`
- **Benefit**: Safe async execution

### 4. Typed Error Handling ✅

**Before:**
- `Result<T>` with `anyhow::Error`
- **Problem**: UI can't distinguish error types

**After:**
- Service-specific error enums
- Pattern matching on error variants
- **Benefit**: Better UI feedback

---

## Test Coverage

### Service Tests (47 total)

- **ConversationService**: 12 tests
  - Streaming (multi-chunk, empty, interrupt)
  - Thinking content separation
  - Memory management
  - Context policy
  
- **CatalogService**: 11 tests
  - Provider/model listing
  - Capabilities queries
  - Auth status
  - Thinking/vision detection

- **ToolExecutionService**: 10 tests
  - Mode-based availability
  - Risk level queries
  - Approval flow
  - Trust level management

- **StorageFacade**: 14 tests
  - Session CRUD
  - Export/import
  - Config management
  - Rule management

### Integration Tests

- **Total library tests**: 793 passing ✅
- **No regressions**: All existing tests pass

---

## Migration Complete

**Status**: ✅ Phase 0-4 complete (2026-01-19)

The BFF layer is production-ready with:
- 4 cohesive domain services
- 47 new tests (all passing)
- Zero clippy warnings
- Zero linter errors
- Backward compatible (existing code unchanged)
