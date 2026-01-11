# Tark Development Roadmap

This directory contains implementation plans that can be followed step-by-step.

## Execution Order

```
┌─────────────────────────────────────────────────────────────────┐
│  SEQUENCE 1: Foundation                                         │
│  └── plugins/001-gemini-oauth.md                                │
│      • Establishes auth patterns (DeviceFlowAuth trait)         │
│      • Creates TokenStore for secure credentials                │
│      • Immediate user value (Gemini OAuth)                      │
│      • Effort: ~2-3 days                                        │
├─────────────────────────────────────────────────────────────────┤
│  SEQUENCE 2: Plugin Infrastructure                              │
│  └── plugins/002-plugin-runtime.md                              │
│      • WASM plugin host (wasmtime)                              │
│      • Plugin manifest and registry                             │
│      • WIT interfaces for plugins                               │
│      • CLI commands (tark plugin add/remove/list)               │
│      • Effort: ~5-7 days                                        │
├─────────────────────────────────────────────────────────────────┤
│  SEQUENCE 3: Example Plugins (Future)                           │
│  └── plugins/003-*.md                                           │
│      • Reference auth plugin implementations                    │
│      • Reference tool plugins                                   │
│      • Effort: ~2-3 days each                                   │
└─────────────────────────────────────────────────────────────────┘
```

## Plugin Plans

| Seq | Plan | Status | Dependencies | Description |
|-----|------|--------|--------------|-------------|
| 1 | [Gemini OAuth](plans/plugins/001-gemini-oauth.md) | Ready | None | Google OAuth device flow (foundation) |
| 2 | [Plugin Runtime](plans/plugins/002-plugin-runtime.md) | Ready | Seq 1 | WASM-based plug-and-play plugin system |
| 3 | (Future) | Planned | Seq 2 | Example plugin implementations |

## Other Plans

| ID | Name | Status | Description |
|----|------|--------|-------------|
| - | [Steering Files](plans/001-steering-files-feature.md) | Ready | Steering files feature |

## Plan Structure

Each plan follows this format:

1. **Prerequisites** - What you need before starting
2. **Phases** - Sequential implementation phases
3. **Tasks** - Specific actions within each phase
4. **Commits** - When and what to commit
5. **Validation** - How to verify the implementation

## How to Use

1. Read the plan completely before starting
2. Complete Phase 0 (Discovery) first if present
3. Follow tasks in order within each phase
4. Run validation commands after each task
5. Commit at the end of each phase
6. Push after final validation

## Plan Status Legend

- **Pending** - Not yet started
- **Ready** - Ready for implementation
- **In Progress** - Currently being implemented
- **Blocked** - Waiting on external dependency
- **Complete** - Implementation finished
