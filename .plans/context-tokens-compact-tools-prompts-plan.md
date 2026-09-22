# Context, Tokens, Compact, Tools & Prompts — Execution Plan

Status: approved for implementation, phases A→E in order.
Locked decisions (user, via questions tool):
- Token display: full last-turn + session in/out in sidebar Context panel AND compact `in/out` pair in status bar.
- Tool renames: clean renames, no aliases (fresh-installs-only policy).
- Scope: all five phases.

## Background findings (read-only research)

### 1. The 5.2k context figure
- Flow: `ChatAgent` (`src/agent/chat.rs:1166-1224`) → `ConversationService::context_breakdown`
  (`src/ui_backend/conversation.rs:631-656`) → `refresh_sidebar_data` adds attachments
  (`src/ui_backend/service.rs:3161-3183`) → `SharedState::set_context_breakdown`
  (`src/ui_backend/state.rs:2294-2299`) → sidebar `Context {used:.1}k`
  (`src/tui_new/widgets/sidebar.rs:775`).
- Startup (Build, empty history): ~5.2k ≈ system prompt (~3.0–3.8k) + tool schemas (~2.1–2.7k).
- Estimators are fiction: history/system use `len/4` with no per-message overhead
  (`src/agent/context.rs:122-144`; `+20/msg` in `src/core/tokenizer.rs:52-60` unused on this path);
  tool schemas are `n_tools × 100` (`chat.rs:1191-1194`) vs 200–550 real → true wire cost ~8–10k.
- Denominator inconsistent: 100k default (`context.rs:8`), 128k catalog fallback
  (`ui_backend/catalog.rs:176`), 1M sidebar default (`sidebar.rs:194`), 100k streaming fallback
  (`conversation.rs:615`).
- Loaded files (`LOADED CONTEXT`) displayed but uncounted; only pending attachments counted
  (`service.rs:3172-3177`).

### 2. `/compact` jitter — root causes
- UI-task await: `handle_slash_command("/compact")` awaits inline
  (`src/tui_new/controller.rs:4020-4025`); render/input/events freeze for the LLM round-trip.
- Write-lock across network: `conversation.rs:662-665` holds `chat_agent.write()` across `llm.chat()`.
- Non-streaming + zero progress: `ChatAgent::compact` (`chat.rs:1452-1516`) uses non-streaming
  `llm.chat`, emits no `LlmStarted`/`Working`/chunk events; completion lands as one giant sync
  relayout (`controller.rs:4026-4053`).
- `AppEvent::ContextCompacted` handler (`controller.rs:3413-3463`) is dead code for manual
  compact (only auto-path sends it, `conversation.rs:225-241`).

### 3. In/out tokens — infra exists, not surfaced
- `TokenUsage{input,output,total}` (`src/llm/types.rs:400-404`) reported by OpenAI/Claude/Gemini/
  Copilot, accumulated per turn (`chat.rs:1581,1608-1612`), forwarded as
  `AppEvent::LlmCompleted{input,output}` (`conversation.rs:459-469`), persisted
  (`storage/usage.rs:146-167`).
- Gap: Ollama chat returns `None` (`ollama.rs:717-947`); needs `len/4` estimate fallback.

### 4. Tools audit
- ~23–28 schemas/request, ~3–5k real tokens every turn. No truncation/caching anywhere.
- Live bug: `SafeShellTool` named `"shell"` collides with `ShellTool`; policy filter drops it from
  Ask/Plan (`tools.toml` id `safe_shell` never matches `t.name()`). Same class: dead `GrepTool`
  vs live `RipgrepTool`; ghost `ripgrep` id in TOML.
- Risk drift: `save_plan`/`update_plan`/`mark_task_done` are `ReadOnly` in code, `write` in TOML.
- Merge clusters: 7-way reference overlap; read trio (`read_file`/`read_files`/`file_preview`);
  `memory_list`⊂`memory_query`; `preview_plan`≈`save_plan`; `switch_mode`⊂`ask_user`.
- Always-on tax: `think`+`todo`+`ask_user` (~350–550 tok each), 4 memory tools in every mode,
  `timeout_secs` augmented onto every schema incl. read-only.
- Gaps: no `fetch_url`; `patch_file` single-occurrence-only; no background shell; no `undo_edit`.
- `write/`/`risky/`/`dangerous/` are doc-only stubs; real tools live in `file_ops.rs`/`shell.rs`.
- MCP zero-bloat only because unwired (`wrap_server_tools*` have no callers) — namespace + cap
  when wiring.

### 5. System prompts
- Build ≈ 2.8–3.2k tokens before tools/history (`chat.rs:183-740`). Bloat ranking:
  60-line `todo` tutorial; 75-line Plan tech-stack + `save_plan` example; `ask_user` nag ×3 modes
  + 4× in schema; per-mode pasted boilerplate; hard-coded tool lists; in-prompt shell blocklist.
- Contradictions: Ask-mode modify-file confirmation (`chat.rs:442-445`) vs Ask read-only;
  questionnaire no-TUI fallback (`questionnaire.rs:527-530`) vs "NEVER ASK IN CHAT".
- Dead `custom_instructions` (stored, never read). Provider `explain/refactor/review` + FIM
  prompts copy-pasted across 6 providers.

## Phase A — Honest context accounting
- Measure real schema JSON tokens instead of `n×100` (`chat.rs:1191-1194`).
- Unify `max_tokens` denominator (single source; fix 100k/128k/1M drift).
- Count loaded context files, not just pending attachments.
- Unit tests for the estimator (incl. tool-result truncation boundary at `context.rs:11,93-99`).

## Phase B — In/out token UI
- Thread last-turn + session `TokenUsage` into `SharedState`.
- Sidebar Context panel: full breakdown; status bar: compact `in/out` pair.
- Ollama chat: `len/4` estimate fallback (mirror FIM path `ollama.rs:992-1002`).
- Widget + snapshot tests for sidebar/status-bar changes.

## Phase C — Fluid `/compact`
- `spawn` compaction off the UI task (mirror `SendMessage` path `service.rs:1618`).
- `CompactionStarted/Progress` events reusing streaming spinner/flash-bar path
  (`LlmStarted` → `FlashBar::Working`, fast pacing).
- Streaming summary if provider supports it; release `RwLock` before network await.
- Send `ContextCompacted` on manual path; incremental UI update instead of giant relayout.

## Phase D — Tools overhaul
1. `SafeShellTool.name()` → `"safe_shell"` (correctness fix, first).
2. Remove dead `GrepTool`; remove/merge text-heuristic `find_references`.
3. Merge: `read_file` ← `read_files` + `file_preview`; `lookup_symbol` ← goto+signature+refs;
   `memory_query` ← `memory_list`; `save_plan` ← `preview_plan` (dry_run flag);
   `ask_user` ← `switch_mode`.
4. Gate `think`/memory/`todo` behind capability flags or relevance injection.
5. Fix plan-tool risks to `Write` in code (match TOML).
6. Skip `timeout_secs` augmentation for `ReadOnly` tools; slim XL schemas.
7. Add: bounded `fetch_url`, multi-hunk edit (replace single-`replacen` `patch_file`),
   `shell_bg` + poll/kill, `undo_edit`.
8. Finish or delete `write/`/`risky/`/`dangerous/` stub-module fiction.
- Regression tests per merge/rename. Clean renames, no aliases.

## Phase E — Prompt cleanup
- Shared mode-suffix constant (search/loop/never-give-up boilerplate once).
- Trim `todo` tutorial, tech-stack matrix, `save_plan` example to pointers (rely on schemas).
- Collapse `ask_user` nag to one shared constant.
- Fix Ask contradiction + questionnaire contradiction; delete dead `custom_instructions`
  or wire it; drop in-prompt shell lists to one line; plain status header.
- Centralize provider `explain/refactor/review` + FIM prompts into `llm/mod.rs` helpers.

## Validation (per AGENTS.md, after each phase)
`cargo fmt --all` → `cargo clippy --all-targets --all-features -- -D warnings` →
`cargo test --all-features` → snapshot review (`cargo insta review`) where visual →
manual TUI smoke (`/help`, `/model`, `/theme`, `/compact`, Ctrl+?, Escape, Enter) →
docs update (README/AGENTS/doc comments) where user-visible.
