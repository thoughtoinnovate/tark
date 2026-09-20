# Tark lightweight subagents — executable build plan v1

**Artifact:** Implementation plan for session-tied lightweight subagents + TUI monitor
**Status:** Approved (auto 1..5 default, manual override)
**Scope:** Rust backend (`src/`) + TUI (`src/tui_new/`, `src/ui_backend/`) + config + docs. No editor plugin changes.
**Non-goals v1:** nested spawn (depth 0), child Write/Shell auto-run (proxied prompt only), git-worktree isolation, CSV batch jobs, remote mirroring of child logs.

## 0. Locked decisions (do not re-debate)

1. `auto` default ON bounds `1..5`; `manual` = fixed `max`. Same dual-mode for `parallel_tools 1..5` per agent (parent + each child). `ollama` auto-caps to 2.
2. Light = `tokio::spawn`, isolated `ConversationContext`, `Ask` read-only, `max_iterations 5`, `timeout 120s`, `WorkspaceCap(subroot ⊆ parent root)`, shared `Arc<dyn LlmProvider>`. No nesting (`spawn_task` absent in child registry). Summaries 200–500 tok only, never raw transcripts.
3. Models: inherit parent snapshot at spawn → per-task override `provider/model/effort`; settings `pinned` wins unless overridden. Invalid override falls back to parent + log line. Effort via `ThinkSettings::resolve` / `resolve_auto` (`src/llm/types.rs:45,64`).
4. Session affinity: `child_id = S:sub:uuid`, `session_tag = S`. Switch session = detach + hide (`+m other` hint), restore tail on return. Close/archive/delete/quit session = kill pool (`interrupt flag + handle.abort() + killpg`). Transcripts to `sessions/<S>/conversations/sub_<id>.json` with `context_transient:true` (excluded from restore/compact); parent `session.json` gets summary-only `Tool` message. Cost: `usage.db` row `S:sub:id`. Approvals: `policy.db` patterns `S:sub:id` and `S:sub:*`, never leak across sessions.
5. Spawn only if independent + non-conflicting + substantive; else return `⊘ inline:<reason>` (subroot overlap / similarity >0.9 dup / trivial <50 chars / write-conflict). Read-heavy first; write-heavy stays inline in parent.
6. Permissions = `deriveSubagentPermission()` intersection (opencode pattern): child gets `Ask` + `readonly + safe_shell + propose_change`, `trust=Careful`, no `spawn_task`/`ask_user`, `WorkspaceCap(subroot)`. Session-start grant modal once per `S` (scope: this-agent | all-in-S; checkboxes: write-proxy, shell-proxy; ro+safe-shell always auto). Runtime Write/Shell proxy to parent queue, stacked by agent: `[R once | S session-this-agent | A all-agents+S | D deny | ■ deny+never-ask]` mapping to existing `ApprovalChoice` (`src/tools/mod.rs:549`). Storm guards: max 3 pending/agent, 5/session, 60s expiry auto-deny, overlapping-write second auto-denied. Ro fast-path stays silent. ApprovalModal never pops for a child directly.
7. Chat with child only via its modal: `i` → input box → `f` followup (new turn) | `n` nudge (no turn) → private `mpsc` inbox → injected next iteration (current tool finishes, not preempted). Parent sees only final summary. Unread `●N` badge if replied while closed.
8. TUI: sidebar `⑂ n/5 AUTO•/▲/▼|FIXED [S]`, per-row `provider·effort`, modal logs/input/kill, keys `Tab/j/k/l/h/Enter/Esc/s/i/x/f/n`
9. Parallel tools: `JoinSet` bounded by `effective_tools`; reads parallel, `Write|Risky|Dangerous` sequential-after-reads; re-sort results by `tool_call_id`; same per-tool `timeout + interrupt-poll 50ms + catch_unwind` (`src/tools/mod.rs:617`).

### Mandatory fixes (apply before/during Phase B)

| # | Fix | Where |
|---|-----|-------|
| 1 | `ToolRegistry::for_mode_with_services` accepts `interrupt: Option<Arc<AtomicBool>>` — child gets fresh `Arc::new(AtomicBool::new(false))`. Additionally pass `tokio_util::sync::CancellationToken` (child-linked to global) into registry; replace 50ms `INTERRUPT_POLL_INTERVAL` spin with `select! { tool_fut, _ = token.cancelled() }` | `tools/mod.rs:181,203,264,297,617` |
| 2 | ALL SQLite (`PolicyEngine::check_approval`, `TarkMemory`, `UsageTracker::log_usage`) via `tokio::task::spawn_blocking`. NEW single `UsageWriter` actor: `mpsc::channel(256)` + batch `INSERT` coalesce; children send `UsageEvent`, never touch `Connection` directly | `tools/policy/engine.rs`, `tools/mod.rs:489`, `tools/builtin/memory.rs`, `storage/usage.rs`, new `agent/usage_writer.rs` or `storage/usage_actor.rs` |
| 3 | `ResourceMonitor`: `OnceLock<usize>` cached `available_parallelism()`; `cfg(target_os="linux")` `/proc/meminfo|loadavg` reads via `spawn_blocking` every 2s + 200ms jitter; non-Linux fallback = `cpu/2`, no mem/load. Output via `watch::Sender<usize>` (latest-value, no queue). `manual` = monitor off, `effective=max`. `ollama → min(max,2)` short-circuit before sampling | `agent/resources.rs` new |
| 4 | `SubagentManager` actor owns `pools: HashMap<String, PerSessionPool>` (no `DashMap`, no `RwLock` — single owner task). `PerSessionPool.children: HashMap<Id, Arc<ChildView>>` replace-on-update (copy-on-write). `JoinHandle`s live ONLY in actor, never in shared maps | `agent/subagent.rs` new |
| 5 | Child log ring: `struct RingBuf { buf: VecDeque<Arc<str>>, bytes: usize, cap: 65536 }`, `with_capacity(100)`, push drops oldest. UI receives tail slice only, never full-log clone | `agent/subagent.rs` Child struct |
| 6 | `spawn_task` tool schema includes `timeout_secs: 180` (separate from default 60s tool timeout) | `tools/builtin/subagent.rs` new |
| 7 | Parallel tool executor: collect `(orig_index: usize, tool_call_id, result)` tuples, sort by `orig_index` (NOT lexicographic id) before `add_tool_result`. Reads-barrier: all `ReadOnly` join first; then `Write\|Risky\|Dangerous` one-at-a-time in index order | `agent/tool_orchestrator.rs` + `chat.rs:1577,2060,2534` |
| 8 | `SubagentManager` actor pattern: single owner task + `mpsc::channel(64)` command channel (`Spawn, Kill, SendInput, CloseSession, GetSnapshot`) — eliminates shared state | `agent/subagent.rs` new |
| 9 | `deriveSubagentPermission` = pure fn returning `ChildRegistrySpec { mode, tools, trust, interrupt, cancel_token, session_id, workspace_cap }` + cached `Arc<[ToolDefinition]>` per mode (no rebuild per child) | `agent/subagent.rs` new |
| 10 | Approval proxy: `service.rs` holds `DashMap<String, oneshot::Sender<ApprovalResponse>>` keyed by `S:sub:id`; `tokio::time::timeout(60s)` on every wait + `remove` in ALL arms (`Ok/Err/timeout/kill/close`) + 30s sweeper task. Glob `S:sub:*` matched at lookup | `ui_backend/service.rs` |
| 11 | Hot-path strings `session_tag/title/preview/chunk: Arc<str>`; snapshots `Arc<[SubagentInfo]>` + `version: u64`; renderer uses `try_read` + last-version fallback, skips `terminal.draw` if version unchanged | `ui_backend/events.rs`, `types.rs`, `state.rs`, `tui_new/renderer.rs` |
| 12 | `tiktoken-rs` feature-gated for child token counts (optional, accept approximation v1 if unavailable). Child: `auto_compact` OFF (fail-fast `context_overflow → summarize-and-exit`), per-child token cap 8k enforced at spawn + per iteration | `agent/context.rs` + `Cargo.toml` features + `agent/subagent.rs` |
| 13 | Bounded `event_tx(256)` (replace unbounded) + `try_send` with `Full → coalesce/drop-oldest-chunk`, count dropped. `LogChunk` flush at 10Hz or 4KB, counters only between flushes | `ui_backend/conversation.rs:51`, `service.rs:14`, `controller.rs`, `agent/subagent.rs` |
| 14 | Session files: atomic `write(tmp)+rename`, per-`S` serialization inside actor (no concurrent writers); `restore_from_session` filters `context_transient:true` (test required) | `storage/mod.rs`, `core/session_manager.rs` |
| 15 | Cheap `worthiness`: `xxhash64(prompt)` + `subroot` prefix overlap + `len<50` trivial — O(1), no embedding, no alloc | `agent/subagent.rs` |

## 1. Phase A — config + caps (sequential, 1 agent)

**A1. `src/config/mod.rs`** (ref `AgentConfig:231`, `ToolsConfig:247`):
Add with `#[serde(default)]`, documented:
```rust
pub struct SubagentConfig { pub mode: String, pub max_subagents: usize, pub min_subagents: usize, pub poll_ms: u64, pub cooldown_ms: u64, pub min_free_mem_mb: u64, pub subagent_max_iterations: usize, pub subagent_timeout_secs: u64, pub subagent_mode: String }
pub struct ParallelToolsConfig { pub mode: String, pub max_parallel_tools: usize, pub min_parallel_tools: usize }
pub struct SubagentModelPin { pub mode: String, pub provider: String, pub model: String, pub effort: String }
```
Defaults: `auto,5,1,2000,8000,512,5,120,ask` / `auto,5,1` / `inherit,"","",""`. Wire into `Config`. Validate `1<=min<=max<=16`, warn + clamp on bad TOML. Update `examples/tark-config/` sample.
Tests: `config::tests::{subagent_defaults, subagent_invalid_clamp}`.
Done: `cargo check -p tark` passes.

## 2. Phase B — backend core

**B1. `src/agent/resources.rs` (NEW):**
`ResourceMonitor { tx: watch::Sender<usize>, mode, min, max, poll, cooldown, cpu_cached: OnceLock<usize>, … }`. `sample_blocking()` via cached `available_parallelism()` + Linux-only `/proc/meminfo|loadavg` (2s + 200ms jitter, `spawn_blocking`); non-Linux = cpu/2. `tick()`: down if `free<floor || load/cpu>0.85 || rate-limited(429/529)`; up if `free>floor+512 && load/cpu<0.55 && saturated && clean 2×poll`; cooldown gate; clamp `[min,max]`; send only on change via `watch`. `manual` = task not spawned, `effective=max`. `ollama → min(max,2)` short-circuit.
Tests: down/up/hold/hysteresis/no-flap with fake sampler (no `/proc` in tests).

**B2. `src/agent/subagent.rs` (NEW, biggest file) — Actor pattern:**
```rust
// ── commands ──────────────────────────────────────────────
pub enum ManagerCmd {
    Spawn { req: SpawnReq, respond: oneshot::Sender<SpawnResult> },
    Kill { id: String, session: String },
    SendInput { id: String, msg: ChildMsg, session: String },
    CloseSession { session: String },
    GetSnapshot { session: String, respond: oneshot::Sender<PoolSnapshot> },
}

// ── state (owned by actor task, NEVER shared) ─────────────
pub enum SubStatus { Running, Queued, WaitingInput, Completed, Failed, Killed }
pub struct RingBuf { pub buf: VecDeque<Arc<str>>, pub bytes: usize, pub cap: usize } // cap 65536, with_capacity(100)
pub struct Child {
    pub id: Arc<str>, pub parent_session: Arc<str>, pub title: Arc<str>, pub status: SubStatus,
    pub provider: Arc<str>, pub model: Arc<str>, pub effort: Arc<str>, pub subroot: PathBuf,
    pub interrupt: Arc<AtomicBool>,
    pub cancel: tokio_util::sync::CancellationToken, // child-linked to global; replaces 50ms spin
    pub inbox: mpsc::Sender<ChildMsg>,      // followup/nudge/kill, bounded 16, try_send only
    pub log: RingBuf,
    pub usage: TokenUsage,
}
pub struct ChildView { pub id: Arc<str>, pub title: Arc<str>, pub status: SubStatus, pub provider: Arc<str>, pub model: Arc<str>, pub effort: Arc<str>, pub preview: Arc<str>, pub unread: u32, pub elapsed_s: u64 } // immutable snapshot
pub struct PerSessionPool {
    pub children: HashMap<Arc<str>, Arc<ChildView>>, // replace-on-update COW
    pub queue: VecDeque<SpawnReq>,
    pub grant: SessionGrant,
}
struct ManagerInner {
    pools: HashMap<Arc<str>, PerSessionPool>,
    handles: HashMap<Arc<str>, JoinHandle<Summary>>, // JoinHandles ONLY here
    global: Arc<Semaphore>,
    cfg: SubagentConfig,
    resource_rx: watch::Receiver<usize>,     // latest-value from ResourceMonitor
    approval_proxy_tx: mpsc::Sender<ApprovalProxyReq>, // bounded 64 to service.rs
    usage_tx: mpsc::Sender<UsageEvent>,      // to UsageWriter actor
}

// ── pure fns (testable, no I/O) ─────────────────────────────
pub fn derive_child_registry_spec(parent: &ParentSpec) -> ChildRegistrySpec {
    // mode=Ask, tools=readonly+safe_shell+propose (cached Arc<[ToolDefinition]> per mode),
    // trust=Careful, no spawn_task/ask_user, fresh interrupt + CancellationToken child,
    // fresh Todo/Thinking trackers, WorkspaceCap(subroot), session_id=S:sub:uuid,
    // token_cap=8k, auto_compact=OFF (fail-fast summarize-and-exit)
}
pub fn worthiness(req: &SpawnReq, siblings: &[ChildView]) -> Result<(), String> {
    // O(1): xxhash64(prompt) equality → deny "dup"
    // subroot prefix-overlap + write intent → deny "shared-write conflict"
    // prompt len <50 → deny "inline: trivial"
}
impl SubagentManager {
    pub fn new(cfg, resource_tx, approval_proxy_tx) -> (Self, JoinHandle<()>) {
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let inner = ManagerInner { … };
        let handle = tokio::spawn(actor_loop(inner, cmd_rx));
        (Self { cmd_tx, cfg }, handle)
    }
    pub async fn try_spawn(&self, req) -> SpawnResult { /* send cmd, await oneshot */ }
    pub async fn kill(&self, id, session) { self.cmd_tx.send(ManagerCmd::Kill{id,session}).await.ok(); }
    // …
}
```
Actor loop: `select! { cmd = cmd_rx.recv() => handle_cmd (try_spawn uses global.try_acquire_owned, NEVER .await inside actor; queued if full), _ = resource_rx.changed() => update_effective(*borrow), … }`. Child spawn: builds fresh `ChatAgent` (independent locks) + `tokio::spawn` child task + `JoinHandle` stored in `handles` map only; inserts `Arc<ChildView>` snapshot; returns `Spawned|Queued|Denied`. Kill/close: `interrupt.store(true) + cancel.cancel() + handle.abort() + approval responder remove + permit drop + status snapshot`. Log flush to TUI at 10Hz/4KB via bounded `event_tx.try_send`, `Full → drop-oldest-chunk + dropped counter`.

**B3. `src/tools/builtin/subagent.rs` (NEW `spawn_task` tool):**
Schema `{description, prompt, subroot?, provider?, model?, effort?, timeout_secs?}` default 180, `risk_level()=ReadOnly`. Calls `Manager::try_spawn` via cmd channel + `oneshot(timeout 5s)`; returns summary or `⊘ inline:<reason>` / `○ QUEUED …`. Register in `tools/mod.rs::for_mode_with_services` for parent modes only (explicit `allow_spawn: bool` flag, NOT `mode != Ask` heuristic); test asserts absent in child registry. `for_mode_with_services` gains `interrupt: Option<Arc<AtomicBool>>` + `cancel: Option<CancellationToken>` params (child = fresh both).

**B4. Parallel tools (`src/agent/tool_orchestrator.rs` + `src/agent/chat.rs:1577,2060,2534`):**
Two-phase barrier. Phase 1 reads-parallel, phase 2 writes-sequential:
```rust
// split preserving original index
let (reads, writes): (Vec<_>, Vec<_>) = limited_calls.into_iter().enumerate()
    .partition(|(_, c)| tools.risk_of(&c.name) == ReadOnly);
// phase 1: JoinSet bounded by effective_tools, acquire with timeout (never hang on 0)
let sem = Arc::new(Semaphore::new(effective_tools.max(1)));
let mut set = JoinSet::new();
for (idx, call) in reads {
    let permit = tokio::time::timeout(Duration::from_secs(5), sem.clone().acquire_owned()).await??;
    let cancel = child_cancel.clone();
    set.spawn(async move {
        let _p = permit;
        let res = tokio::select! {
            r = tools.execute(&call.name, call.args) => r,
            _ = cancel.cancelled() => ToolResult::error("cancelled"),
        };
        (idx, call.id, res) // catch_unwind + timeout kept inside execute()
    });
}
let mut out = vec![];
while let Some(r) = set.join_next().await {
    match r { Ok(t) => out.push(t), Err(e) if e.is_cancelled() => {}, Err(e) => out.push((usize::MAX, String::new(), ToolResult::error(format!("panic: {e}")))) }
}
// phase 2: writes strictly sequential in index order
for (idx, call) in writes.into_iter().sorted_by_key(|(i,_)| *i) {
    let res = tools.execute(&call.name, call.args).await;
    out.push((idx, call.id, res));
}
out.sort_by_key(|(idx, _, _)| *idx); // index order, NOT lexicographic id
for (_, id, res) in out { add_tool_result(id, res); }
```
`JoinSet: abort_on_drop`. Cancel via `CancellationToken`, no 50ms spin.
Tests: index-order-preserved, writes-barriered-alone, cap-respected, cancel/timeout per branch, `effective=0 → clamp 1`.

**B5. `src/storage/mod.rs` + `src/core/session_manager.rs:15` (+ NEW usage actor):**
`save_sub_transcript(S, sub_id, msgs)`: atomic `write(tmp)+rename`, called ONLY from manager actor (per-`S` serialization, no concurrent writers). Header `{parent_session, child_id, provider/model/effort, subroot, status, context_transient:true}`. `close_session(S)` kills pool first, then persists summary-only parent `Tool` messages. `restore_from_session` MUST filter `context_transient:true` (regression test required). NEW `UsageWriter` actor (`mpsc 256`, batch flush 1s/100 rows) — all `log_usage` go through it, never direct `Mutex<Connection>` on async path.
Done Phase B: `cargo check && cargo test --all-features agent::subagent agent::resources tools::builtin::subagent storage`.

## 3. Phase C — BFF + TUI

**C1. `src/ui_backend/{types.rs:302, events.rs:14, commands.rs:15, state.rs:242}`:**
`SubagentInfo{id: Arc<str>, title: Arc<str>, status, provider: Arc<str>, model: Arc<str>, effort: Arc<str>, preview: Arc<str>, unread, elapsed}`, `SubStatus`, `SessionGrant{per_agent, all}`; snapshot `SubagentSnapshot { list: Arc<[SubagentInfo]>, version: u64 }`; events `SubagentsUpdated{session_tag: Arc<str>, snapshot, effective, mode}`, `SubagentLogChunk{id, session_tag: Arc<str>, chunk: Arc<str>, dropped: u32}`, `SubagentStatusChanged{id, session_tag: Arc<str>, status}`, `ApprovalRequested{subagent_id: Arc<str>, session_tag: Arc<str>, …}`; commands `FocusSubagents, SubagentsUp/Down, SubagentSelect, SubagentSendFollowup(id,msg), SubagentSendNudge(id,msg), SubagentKill(id), SubagentsClearDone, SubagentsFilter(All|Active|Done), GrantSubagentScope`; state: single `RwLock<SubagentUiState { snapshot, grant, pending_approvals: VecDeque (cap 5/S), filter, dropped_chunks }>` + `version: AtomicU64` (no per-chunk `write_inner` on hot path; batch 10Hz), `expanded_panels [bool;6]→[bool;7]`, `ModalType::SubagentDetail | SubagentGrant`.

**C2. `src/ui_backend/service.rs:291,519,3161,3376`:**
Replace unbounded `event_tx` with bounded `mpsc::channel(256)` + `try_send` fallback (coalesce/drop-oldest, count). `refresh_subagents()/update_subagents()` publishes `Arc` snapshots (version bump) in `refresh_sidebar_data`; renderer `try_read` + last-version fallback, skips draw if unchanged. `handle_command` arms for nav/kill/followup/nudge/clear/filter/grant + stacked approve `R/S/A/D/■` resolving `approval_responders: DashMap<Arc<str>, oneshot::Sender<ApprovalResponse>>` with `timeout(60s)` + `remove` in all arms + 30s sweeper; `S:sub:*` glob at lookup; storm caps `3/agent,5/S` enforced before modal.

**C3. `src/tui_new/widgets/sidebar.rs:20,872` + `src/tui_new/modals/subagent_modal.rs` + `src/tui_new/modals/grant_modal.rs` + `src/tui_new/modals/subagent_settings_modal.rs` + `src/tui_new/widgets/command_autocomplete.rs:19` + `src/tui_new/renderer.rs:2538,2783,2868,345` + `src/tui_new/controller.rs:963,2486,2813,3678`:**
Sidebar panel between Tasks/Todo; 60x60 modals; keymap additions; `poll_events` chunk coalescing (32KB) + `S≠current` discard + queue drain on upshift.
Slash + settings contract (MUST honor existing patterns, no new paradigm):
- `SlashCommand` enum (`command_autocomplete.rs:19`): add `Subagents` (`/subagents` → open settings modal; MUST appear in `all()`, `name()`, `description()` = "Subagent caps, tools, model", `icon()` = `⑂`; add unit tests mirroring `test_policy_command_properties`). Partial-match via existing `find_matches` (typing `/sub` resolves).
- `handle_slash_command` (`controller.rs:3678`): arm `"/subagents"` mirroring `"/theme"` block — set selection from current cfg, `set_active_modal(Some(ModalType::SubagentSettings))`, `set_focused_component(Modal)`. Also support inline forms `/subagents auto|manual`, `/subagents tools 5`, `/subagents pin haiku/low` → apply + transient System msg (mirror `/diff [auto|inline|split]` pattern at `:3748`), `clear_input`, `return Ok`.
- `ModalType` (`ui_backend/state.rs:122`): add `SubagentDetail, SubagentGrant, SubagentSettings` (extend `modals/mod.rs` handler match alongside `TrustLevel/Tools/Policy`). `SubagentSettings` widget copies `trust_modal.rs` structure (centered, `↑↓ Navigate / Enter Select / Esc Cancel` header, `selected` index state).
- Keymap (`renderer.rs:345 key_to_command`): `Ctrl+G` → `Command::OpenSubagentSettings` (next to `Ctrl+Y` trust / `Ctrl+T` thinking / `Ctrl+B` sidebar / `Ctrl+?` help). Sidebar `⑂` header `Enter` → `e` opens settings. Register in `/help` modal table + `README.md` shortcuts.
- Live-apply rule: modal edits apply to current session immediately (effective/mode/pin), `w` persists to `config.toml` (global or `.tark/` overlay), session switch keeps per-`S` grant; global caps stay cross-session (fair FIFO).
TUI test ladder (per AGENTS.md): unit `tests/tui_widget_tests.rs` (rows/filter/badges + `/sub` match + settings rows), snapshot `tests/tui_snapshot_tests.rs` + `cargo insta review` (panel/detail/grant/stacked-approval/settings), cucumber `tests/cucumber_tui_new.rs` + `tests/visual/tui/features/` (spawn/queue/kill/followup/nudge/grant/stack/switch-detach/close-kill/settings-apply-persist), manual smoke `cargo build && ./target/debug/tark tui` (`/help /model /theme /subagents Ctrl+? Esc Enter` + subagent keys) and release `cargo build --release && ./target/release/tark tui`.

## 4. Verify + docs (sequential)

```
cargo build --release && cargo fmt --all -- --check && cargo clippy --all-targets --all-features -- -D warnings && cargo test --all-features
```
Lua tests only if plugin touched (not in v1). Update `README.md` (features/keys/config), `AGENTS.md` arch tables, `///` doc comments. Keep versions in sync if release (`Cargo.toml` + neovim `init.lua`). Commit `feat: lightweight session-tied subagents (auto 1..5)` only when explicitly requested.

## 5. Parallelization plan

Critical path: `A1 → B1+B2+B3 → B4+B5 → C1 → C2 → C3 → §4`. Max safe fan-out **2 workers** (backend + frontend; 3-way risks same-file contention in `state.rs/renderer.rs/service.rs`):
- **Batch 1 (after A1):** W1=`B1 resources + B4 parallel-tools` (both touch agent loop but disjoint files: `resources.rs` vs `tool_orchestrator.rs/chat.rs`), W2=`B2 manager + B3 spawn_task + B5 storage/usage-actor` (single owner — manager header shapes storage + tool). Merge on `ChildView/SpawnReq/ManagerCmd` freeze first.
- **Batch 2 (after B green):** W1=`C1 BFF types/events/commands/state`, W2=`sidebar + modals widgets`, W3=`renderer keymap + controller events` against `C1` draft; merge on `C1` lock, then `C2 service` solo (needs final C1).
- **Never parallelize:** `C2` before `C1` final; `§4` verify before green; two workers in same file (`state.rs`, `renderer.rs`) — split by file, not hunk.
- Solo ≈ 7–9 sequential units; 3 workers ≈ 3 waves. Freeze struct/event names in `C1`/`B2` headers first; others code against them.

## 6. Acceptance checklist

- [ ] `auto` starts `min(max,cpu/2)` (OnceLock-cached), shifts ±1 with cooldown+jitter, clamps `[min,max]`, `ollama→≤2`; `manual` fixed, no monitor task.
- [ ] 6th spawn queues with visible `QUEUED` (`try_acquire`, no actor stall); upshift drains FIFO; downshift never preempts running.
- [ ] Child inherits `provider/model/effort` snapshot; override/pin shown with `*`; invalid override falls back + logs; cached `Arc<LlmProvider>` + `Arc<[ToolDefinition]>` reused (no re-auth/rebuild per spawn).
- [ ] Overlap/dup(trivial hash)/trivial-len spawn denied with `⊘ inline:<reason>` O(1); no nesting possible from child (`allow_spawn:false`).
- [ ] Ro+SafeShell silent; Write/Shell proxied stacked by agent with `R/S/A/D/■`, caps `3/agent,5/S,60s expiry` + overlap auto-deny + sweeper (no responder leak); session-start grant scopes correctly; no cross-session pattern leak.
- [ ] Modal `f`(turn)/`n`(no-turn) distinction works via bounded inbox `try_send`; parent sees only final summary; unread badges; `Enter` on done pastes summary; `Full → WaitingInput`, never blocks parent.
- [ ] Switch hides/restores tail; close/quit kills pool (`interrupt+cancel+abort+killpg+responder-remove+permit-drop`) + atomic `write+rename` summaries; `restore` filters `context_transient`; parallel tools index-ordered + reads-barrier + writes-sequential; TUI filter + clear-done retain 20.
- [ ] Hot path: `Arc<str>` payloads, `Arc<[SubagentInfo]>` snapshots + versioned skip-draw, bounded `event_tx(256)` + 10Hz/4KB flush, `spawn_blocking` all SQLite, `UsageWriter` batching, no per-chunk `write_inner`, no 50ms spin (CancellationToken), child token cap 8k + no-compact.
- [ ] Settings honored: `/subagents` in autocomplete + `handle_slash_command` (incl. inline `/subagents auto|manual|tools N|pin …`); `Ctrl+G` + sidebar-`e` open `SubagentSettings` (TrustModal pattern); live-apply session + `w` persists to `config.toml`; `/help` + README document all keys; `config.toml` sample in `examples/tark-config/` updated.
- [ ] Full `AGENTS.md` pre-commit suite green (`build --release + fmt --check + clippy -D warnings + test --all-features`); docs (`README/AGENTS.md` + `///`) updated.

## 7. TUI mocks + interaction contract (agents MUST match these)

Layout ground truth (`renderer.rs:2508-2532`): `Header(2) | Messages | StatusStrip(1) | Input(5) | StatusBar(1)` + `Sidebar(35)` when visible. New `SidebarPanel::Subagents` between Tasks and Todo. StatusBar gains `⑂ n/5` + `⇉ n/5`.

```
Idle:            │ ▼ ⑂ Subagents 0/5 AUTO• (dim, single line)        │ StatusBar: ⑂ 0/5 AUTO•
Working:         │ ▼ ⑂ Subagents 3/5 AUTO• (amber+badge)             │
                 │   ● explore-auth 12s sonnet·med ⇉ 4/5             │
                 │     ↳ rg + Read… • 8 calls (muted preview)        │
                 │   ● quick-grep 4s haiku·low * ⇉ 5/5 (*=override)  │
Backpressure:    │   ○ write-migr QUEUED 5/5 busy [x cancel]         │
                 │   (+1 other ⏳ — switch to view)                   │
Done row:        │   ✓ check-policy Done 21s [Enter=paste summary]   │
Filter tabs:     │   [All|Active|Done]  c=clear (retain 20/S)        │
```

Detail modal `SubagentDetail` (centered 60x60, copies `approval_modal.rs` chrome):
```
┌─ ⑂ explore-auth [● 12s | sess_abc:sub:7f | openrouter/sonnet | med | Ask ro] ─┐
│ Perms: intersect(parent∩Ask) · subroot src/auth ✓ · iter 3/5 · 4.1k/8k       │
│ Task: find callers of authenticated() (subroot src/auth)                     │
│ ── Logs (follow-tail ON · j/k pause · s resume) ──────────────────────────── │
│ ✓ ReadFile middleware.rs / ✓ rg → 14 hits / ⋯ session.rs (400ms blink)       │
│ You→child(followup): also check crates/api/auth_*                            │
│ ── Input ─────────────────────────────────────────────────────────────────── │
│ > what about legacy token path?_ █                                           │
│ [f Followup=turn] [n Nudge=no turn] [x Kill] [s Tail] · done:[Enter]=paste   │
└── Esc=list ──────────────────────────────────────────────────────────────────┘
```

Grant modal `SubagentGrant` (first spawn per S, `Esc`=deny all):
```
┌─ ⑂ Permissions for sess_abc ─────────── Esc=deny all ─┐
│ Scope: [• this agent] [ all subagents in S]            │
│ ☑ Read + SafeShell — auto, always on                   │
│ ☐ Write (subroot only) — prompt per use                │
│ ☐ Shell (subroot only) — prompt per use                │
│ ☐ Never ask again this session (auto-deny extras)      │
│ [Allow session] [Allow once] [Deny]                    │
└────────────────────────────────────────────────────────┘
```

Stacked approval (proxy; child never pops its own modal):
```
┌─ ⚠ ⑂ explore-auth wants Write (1/2) ── sess_abc ──┐
│ ✎ write_file src/auth/session.rs (in subroot ✓)     │
│ @@ preview 3 lines…                                  │
│ [R Once] [S Session·agent] [A All+S] [D Deny] [■+never] │
│ queued: check-policy ×1 ( ] peek) · 60s expiry      │
└─────────────────────────────────────────────────────┘
```

Settings modal `SubagentSettings` (copies `trust_modal.rs`: `↑↓ Navigate / Enter Select / Esc Cancel`, `w` write-to-file):
```
┌─ ⑂ Subagent settings ── w=write to config ─┐
│ Mode:        [• auto 1..5] [ manual=3]      │
│ Tools/agent: [• auto 1..5] [ manual=5]      │
│ Model:       [• inherit (sonnet·med)] [ pinned…] │
│   provider [openrouter____] model [haiku] effort [low] │
│ [Apply session] [Write file] [Reset defaults]│
└─────────────────────────────────────────────┘
```
Open via `/subagents` (autocomplete `/sub` resolves), `Ctrl+G`, or sidebar `⑂` header `Enter`→`e`.

Keybindings (extend `/help` table + README shortcuts; `Esc` pops exactly one level, never quits):
`Tab` focus cycle · `Ctrl+B` sidebar · `Ctrl+G` subagent settings · `Ctrl+Y` trust · `Ctrl+T` thinking · `Ctrl+?` help · `j/k/l/h` nav · `Enter/s` open · `i` input · `f` followup · `n` nudge · `x` kill · `c` clear-done · `[`/`]` filter/peek · `Ctrl+C` parent-only interrupt.

Parent-chat echo lines (exact strings agents must emit):
`│ spawn_task explore-auth (sonnet·med, inherit) ✓ spawned │`
`│ spawn_task quick-grep (haiku·low, override) ✓ spawned │`
`│ ⊘ spawn fix-same-file denied → inline: shared write conflict │`
`│ ✓ ⑂ explore-auth done 18s — 14 callers (320 tok) │`

Config truth (`config.toml`, global `~/.config/tark/` + `.tark/` overlay):
```toml
[agent.subagents]
mode = "auto" # auto | manual
max_subagents = 5
min_subagents = 1
poll_ms = 2000
cooldown_ms = 8000
min_free_mem_mb = 512
subagent_max_iterations = 5
subagent_timeout_secs = 120
subagent_mode = "ask"
[agent.parallel_tools]
mode = "auto"
max_parallel_tools = 5
min_parallel_tools = 1
[agent.subagents.models]
mode = "inherit" # inherit | pinned
provider = ""
model = ""
effort = ""
```
