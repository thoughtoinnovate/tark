# Release Evidence (R13)

How each canonical scenario `S1–S33` (`.plans/requirement.md`) is validated.
A release may not claim a scenario without the listed evidence.

## Security (R1–R3)

| Scenario | Validation |
|----------|-----------|
| S1 allowed workspace op | `cargo test --all-features --lib tools::workspace::tests::s1_allowed_workspace_file_succeeds` |
| S2 traversal/absolute denied | `tools::workspace::tests::s2_traversal_and_absolute_escape_denied` + `ReadFileTool`/`WriteFileTool` denial paths return `outside the granted workspace` with no fs content |
| S3 symlink/TOCTOU | `tools::workspace::tests::s3_symlink_escape_denied` (`symlink-escape` reason); creation-path ancestor canonicalization |
| S4 safe mode, no shell | `tools::readonly::safe_shell::tests::test_safe_commands` (metacharacter, `find -exec`, `env`, `git config`, path-escape cases) + no `sh -c` in Ask/Plan code paths |
| S5 bounded cancellable process | `tools::tests::tool_registry_honors_interrupt_mid_execution`, timeout-clamp test, `MAX_CHILD_OUTPUT_*` truncation; process-group kill guard in `src/tools/shell.rs` |
| S6 secret redaction | `debug_logger` redactor tests + audit/UI/mirror redaction sites (`policy::engine::log_decision`, `tool_orchestrator` previews, `RedactedStderr` tracing layer) |
| S7 MCP trust | `mcp::trust::tests::{trust_round_trip, changed_command_reprompts, revoke_clears_trust}` + `tark mcp trust/approve/connect` flow refusing untrusted launches |

## TUI (R4)

| Scenario | Validation |
|----------|-----------|
| S8 hit testing | `tui_new::renderer` cached-layout hit-test unit tests |
| S9 long-history scroll | follow-tail stickiness tests in `ui_backend::state` + bounded caps (`MAX_INPUT_HISTORY`, `MAX_MESSAGE_QUEUE`, `MAX_STREAMING_CONTENT_LEN`) |
| S10 event storm fairness | chunk coalescing tests (`coalesce_chunks`) in `tui_new::controller` |
| S11 terminal restore | `tui_new::terminal_guard` tests + `TerminalGuard` RAII + panic hook |

## MCP (R5–R7)

| Scenario | Validation |
|----------|-----------|
| S12 stdio interop | `tark mcp conformance` report (`ConformanceReport::all_passed`) against the official Everything server |
| S13 Filesystem confinement | conformance restricted to disposable dir + approval/policy enforcement |
| S14 Streamable HTTP security | `transport` loopback/insecure/legacy-SSE unit tests |
| S15 version rejection | `protocol::assert_protocol_version` tests naming required vs offered |
| S16 failure isolation | per-server `Failed` state + `reconnect` creating a clean session |
| S17 auth extensions | bearer-from-env + secure-store wiring, redacted logs |
| S18 Apps consent/isolation | `TARK_MCP_APPS=1` gate + consent flow |
| S19 Tasks gated | `TARK_MCP_TASKS_EXPERIMENTAL=1` gate, never advertised as stable |
| S31 config lifecycle | `mcp_cli` TOML ordering/import/overwrite-refusal tests + `.toml.bak` + atomic write |

## Editor backend (R8–R9)

| Scenario | Validation |
|----------|-----------|
| S20 NDJSON chat session | `transport::acp::framing` round-trip/oversize tests + real-subprocess smoke |
| S21 permission/cancel round-trip | `map_permission_response_*` + `map_elicitation_response_*` tests; session-bound outbound map; `session/close` drains |
| S22 completion extension | `completion_extension_method_is_underscore_prefixed`, opt-in parsing, epoch echo |
| S23 LSP UTF-16/incremental | `lsp::document` multibyte + non-BMP + multi-edit tests |
| S24 LSP cancel/ownership | per-doc diagnostics tracker tests (supersede, close-forget, stale-drop); no `agent`/`mcp`/policy refs in `src/lsp/` |

## Editors + release (R10–R13)

| Scenario | Validation |
|----------|-----------|
| S25–S27 Neovim/VS Code workflows | Real headless-editor E2E (outside this repo) against `docs/EDITOR_COMPATIBILITY.md` |
| S28 concurrent isolation | `fail_session_outbound` + stateless LSP + per-doc state |
| S29 migration/rollback | TOML backup/atomic tests; legacy-method migration errors; `session/load` removal error |
| S30 release evidence | This file + CI (`cargo test --all-features`, clippy `-D warnings`, fmt `--check`) + MCP conformance run |
| S32 platform artifacts | Release-matrix smoke per OS/arch/editor combo; unsupported combos unadvertised |
| S33 executable work packages | Each package: owner files, prerequisites, forbidden changes, failing tests first, validation commands, rollback, stop condition |

## Commands

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
tark mcp conformance --help
```
