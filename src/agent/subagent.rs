//! Lightweight session-tied subagents (plan §B2).
//!
//! Design notes for future readers:
//! - [`SubagentManager`] is a thin handle over a single actor task. All
//!   mutable state lives in the actor; admission control is correct by
//!   construction (no semaphore needed — the actor serializes `Spawn`).
//! - Children are `tokio` tasks running an isolated [`ChatAgent`] in
//!   `Ask` mode with a fresh [`ToolRegistry`](crate::tools::ToolRegistry),
//!   fresh interrupt flag, and `WorkspaceCap` confined to their subroot.
//!   They share only `Arc<dyn LlmProvider>` with the parent.
//! - The parent NEVER sees child transcripts — only a truncated summary
//!   delivered through [`SubagentManager::await_outcome`].
//! - Follow-up/resume across completions is intentionally unsupported in
//!   v1 (fresh context per spawn); [`ChildMsg::Followup`] only works while
//!   the child is still running (5s grace window between turns).

#![allow(dead_code)] // Phase C wires SessionGrant modal, mark_seen, event fan-out

use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::JoinHandle;

use crate::agent::ChatAgent;
use crate::config::SubagentConfig;
use crate::core::types::AgentMode;
use crate::llm::LlmProvider;
use crate::storage::usage::{SubagentUsage, UsageWriter};
use crate::storage::{SubTranscript, TarkStorage};
use crate::tools::{ToolRegistry, TrustLevel};

/// Max retained log bytes per child (oldest lines dropped first).
pub const LOG_CAP_BYTES: usize = 64 * 1024;
/// Max retained log lines per child.
pub const LOG_CAP_LINES: usize = 100;
/// Summary truncation budget in chars (~500 tokens).
pub const SUMMARY_CHAR_BUDGET: usize = 2000;
/// Idle window after a turn during which a follow-up still reaches the child.
const FOLLOWUP_GRACE: Duration = Duration::from_secs(5);
/// Bounded inbox per child; `Full` surfaces as an error, never blocks actor.
const INBOX_CAP: usize = 16;

fn truncate_chars(s: &str, budget: usize) -> String {
    if s.chars().count() <= budget {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .nth(budget)
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}…[truncated]", &s[..end])
}

// ========== Status & views ==========

/// Lifecycle status of a child subagent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubStatus {
    Queued,
    Running,
    WaitingInput,
    Completed,
    Failed,
    Killed,
}

/// Immutable snapshot of one child for UI / tool layers.
#[derive(Debug, Clone)]
pub struct ChildView {
    pub id: Arc<str>,
    pub parent_session: Arc<str>,
    pub title: Arc<str>,
    pub status: SubStatus,
    pub provider: Arc<str>,
    pub model: Arc<str>,
    pub effort: Arc<str>,
    pub preview: Arc<str>,
    /// Last ≤20 log lines for the detail modal (bounded).
    pub log_tail: Vec<String>,
    pub unread: u32,
    pub elapsed_s: u64,
}

/// A queued (not yet started) spawn request, for UI display.
#[derive(Debug, Clone)]
pub struct QueuedView {
    pub id: Arc<str>,
    pub title: Arc<str>,
    pub position: usize,
}

/// Full pool snapshot for one parent session.
#[derive(Debug, Clone, Default)]
pub struct PoolSnapshot {
    pub active: Vec<ChildView>,
    pub queued: Vec<QueuedView>,
}

/// Byte-bounded ring of log lines (oldest dropped first).
#[derive(Debug, Default)]
pub struct RingBuf {
    buf: VecDeque<Arc<str>>,
    bytes: usize,
}

impl RingBuf {
    pub fn push(&mut self, line: &str) {
        let line: Arc<str> = Arc::from(line);
        self.bytes += line.len();
        self.buf.push_back(line);
        while self.buf.len() > LOG_CAP_LINES || self.bytes > LOG_CAP_BYTES {
            if let Some(old) = self.buf.pop_front() {
                self.bytes = self.bytes.saturating_sub(old.len());
            } else {
                break;
            }
        }
    }

    /// Last `n` lines joined, for previews and tails.
    pub fn tail(&self, n: usize) -> String {
        self.tail_lines(n).join("\n")
    }

    /// Last `n` lines as owned strings (detail modal feed).
    pub fn tail_lines(&self, n: usize) -> Vec<String> {
        self.buf
            .iter()
            .rev()
            .take(n)
            .rev()
            .map(|s| s.to_string())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// ========== Messages & requests ==========

/// Parent → running child input.
#[derive(Debug, Clone)]
pub enum ChildMsg {
    /// Starts a new turn (child replies).
    Followup(String),
    /// Context note applied to the next turn (no turn by itself).
    Nudge(String),
}

/// Spawn request (built by the `spawn_task` tool).
///
/// No `Debug`: carries `Arc<dyn LlmProvider>`.
pub struct SpawnReq {
    pub parent_session: Arc<str>,
    pub parent_root: PathBuf,
    pub title: String,
    pub prompt: String,
    pub subroot: PathBuf,
    pub llm: Arc<dyn LlmProvider>,
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub max_iterations: usize,
    pub timeout: Duration,
}

/// Immediate admission outcome.
#[derive(Debug, Clone)]
pub enum SpawnResult {
    Spawned { id: Arc<str> },
    Queued { id: Arc<str>, position: usize },
    Denied { reason: String },
}

/// Terminal outcome of a child run.
#[derive(Debug, Clone)]
pub enum ChildOutcome {
    Completed(ChildSummary),
    Failed(String),
    Killed(String),
}

/// Truncated result handed back to the parent agent.
#[derive(Debug, Clone)]
pub struct ChildSummary {
    pub id: Arc<str>,
    pub text: String,
    pub tool_calls: usize,
    pub elapsed_s: u64,
}

/// Session-scoped permission grant (modal UI lands in Phase C;
/// defaults deny everything beyond read-only + safe shell).
#[derive(Debug, Clone, Default)]
pub struct SessionGrant {
    pub write_proxy: bool,
    pub shell_proxy: bool,
    pub scope_all: bool,
    pub never_ask: bool,
}

/// Parent context snapshot captured by the `spawn_task` tool.
///
/// No `Debug`: carries `Arc<dyn LlmProvider>`.
#[derive(Clone)]
pub struct ParentCtx {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub llm: Arc<dyn LlmProvider>,
    pub provider: String,
    pub model: String,
    pub effort: String,
    pub max_iterations: usize,
    pub pin: crate::config::SubagentModelPin,
}

/// Shareable, updatable parent context (short lock, clone-only, never held
/// across `.await`).
pub type SharedParentCtx = Arc<std::sync::RwLock<ParentCtx>>;

// ========== Pure policy fns (unit-testable) ==========

/// Minimal scope record used by [`worthiness`].
#[derive(Debug, Clone)]
pub struct SiblingScope {
    pub title: Arc<str>,
    pub subroot: PathBuf,
    pub prompt_hash: u64,
}

pub fn prompt_hash(prompt: &str) -> u64 {
    let mut h = DefaultHasher::new();
    prompt.trim().hash(&mut h);
    h.finish()
}

/// Kernel-style scope overlap: either path prefixes the other.
pub fn scopes_overlap(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

/// Spawn-worthiness gate. Denies trivial and duplicate work inline so no
/// tokens are wasted on a child. Scope overlap is allowed in v1 because
/// children are read-only (`may_write` reserved for future write-mode,
/// which must deny overlapping scopes).
pub fn worthiness(
    prompt: &str,
    subroot: &Path,
    siblings: &[SiblingScope],
    may_write: bool,
) -> Result<(), String> {
    if prompt.trim().chars().count() < 50 {
        return Err("trivial task — do it inline instead of spawning".to_string());
    }
    let hash = prompt_hash(prompt);
    for sib in siblings {
        if sib.prompt_hash == hash {
            return Err(format!(
                "duplicate of '{}' — reuse its summary instead of spawning",
                sib.title
            ));
        }
        if may_write && scopes_overlap(&sib.subroot, subroot) {
            return Err(format!(
                "shared scope with '{}' — run inline to avoid conflicts",
                sib.title
            ));
        }
    }
    Ok(())
}

/// Child registry specification (pure data; see [`build_child_registry`]).
#[derive(Debug, Clone)]
pub struct ChildRegistrySpec {
    pub mode: AgentMode,
    pub shell_enabled: bool,
    pub trust: TrustLevel,
    pub session_id: String,
    pub tool_timeout_secs: u64,
    pub max_iterations: usize,
}

/// Derive the locked-down child spec: `Ask`, no shell, careful trust,
/// namespaced session id. Pure — testable without I/O.
pub fn derive_child_registry_spec(parent_session: &str) -> ChildRegistrySpec {
    ChildRegistrySpec {
        mode: AgentMode::Ask,
        shell_enabled: false,
        trust: TrustLevel::Careful,
        session_id: format!("{}:sub:{}", parent_session, uuid::Uuid::new_v4()),
        tool_timeout_secs: 60,
        max_iterations: 5,
    }
}

/// Build the actual child registry: read-only tools only, no interaction
/// channel (deny-by-default until the Phase C approval proxy lands),
/// fresh trackers, fresh interrupt flag owned by the caller via `spec`.
pub fn build_child_registry(spec: &ChildRegistrySpec, subroot: PathBuf) -> ToolRegistry {
    let mut reg = ToolRegistry::for_mode_with_services(
        subroot,
        spec.mode,
        spec.shell_enabled,
        None, // No interaction channel → approval-gated tools fail safe (deny)
        None, // No plan service in children
        None, // Fresh todo tracker (never shared with parent)
        None, // Fresh thinking tracker
    );
    reg.set_session_id(spec.session_id.clone());
    reg.set_trust_level(spec.trust);
    reg.set_tool_timeout_secs(spec.tool_timeout_secs);
    reg
}

// ========== Manager (handle + actor) ==========

enum ManagerCmd {
    SetUsageWriter(UsageWriter),
    Spawn {
        req: SpawnReq,
        respond: oneshot::Sender<SpawnResult>,
    },
    AwaitOutcome {
        id: Arc<str>,
        session: Arc<str>,
        respond: oneshot::Sender<ChildOutcome>,
    },
    SendInput {
        id: Arc<str>,
        session: Arc<str>,
        msg: ChildMsg,
        respond: oneshot::Sender<Result<(), String>>,
    },
    Kill {
        id: Arc<str>,
        session: Arc<str>,
    },
    MarkSeen {
        id: Arc<str>,
        session: Arc<str>,
    },
    SessionCounts {
        respond: oneshot::Sender<HashMap<String, usize>>,
    },
    CloseSession {
        session: Arc<str>,
    },
    /// Drop finished children beyond the `keep` most recent per status
    /// group (implements the sidebar `[c Clear done]` retention rule).
    ClearFinished {
        session: Arc<str>,
        keep: usize,
    },
    Snapshot {
        session: Arc<str>,
        respond: oneshot::Sender<PoolSnapshot>,
    },
}

#[derive(Debug)]
enum ChildEvent {
    Log {
        session: Arc<str>,
        id: Arc<str>,
        chunk: Arc<str>,
    },
    TurnDone {
        session: Arc<str>,
        id: Arc<str>,
        text: String,
        tool_calls: usize,
        input_tokens: u32,
        output_tokens: u32,
    },
    TurnFailed {
        session: Arc<str>,
        id: Arc<str>,
        error: String,
    },
}

struct Child {
    title: Arc<str>,
    provider: Arc<str>,
    model: Arc<str>,
    effort: Arc<str>,
    subroot: PathBuf,
    workspace_root: PathBuf,
    prompt_hash: u64,
    status: SubStatus,
    inbox: mpsc::Sender<ChildMsg>,
    interrupt: Arc<AtomicBool>,
    log: RingBuf,
    unread: u32,
    started: Instant,
    handle: Option<JoinHandle<()>>,
    outcome: Option<ChildOutcome>,
    waiters: Vec<oneshot::Sender<ChildOutcome>>,
}

struct Pool {
    children: HashMap<Arc<str>, Child>,
    /// Queued spawns: admission responded immediately (`Queued{position}`);
    /// completion is delivered through `await_outcome` waiters below.
    queue: VecDeque<(Arc<str>, SpawnReq)>,
    /// Waiters for queued ids (moved into the child on dequeue).
    queued_waiters: HashMap<Arc<str>, Vec<oneshot::Sender<ChildOutcome>>>,
    grant: SessionGrant,
}

struct Inner {
    pools: HashMap<Arc<str>, Pool>,
    max: usize,
    effective: usize,
    resource_rx: watch::Receiver<usize>,
    event_tx: mpsc::Sender<ChildEvent>,
    /// Optional usage attribution (set by the host via `set_usage_writer`).
    usage: Option<UsageWriter>,
    /// `TarkStorage` handles cached per workspace root (created on demand;
    /// transcript persistence only).
    storages: HashMap<PathBuf, Arc<TarkStorage>>,
}

/// Cheap handle to the subagent actor. Clone it freely (`mpsc::Sender`).
#[derive(Debug, Clone)]
pub struct SubagentManager {
    cmd_tx: mpsc::Sender<ManagerCmd>,
}

impl SubagentManager {
    /// Start the manager actor. `resource_rx` feeds auto-tuned caps;
    /// pass a `watch` receiver stuck at `max` for manual mode / tests.
    pub fn new(cfg: &SubagentConfig, resource_rx: watch::Receiver<usize>) -> Self {
        let (min, max) = cfg.validated_bounds();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (event_tx, event_rx) = mpsc::channel(256);
        let initial = (*resource_rx.borrow()).clamp(min, max);
        let inner = Inner {
            pools: HashMap::new(),
            max,
            effective: initial,
            resource_rx,
            event_tx,
            usage: None,
            storages: HashMap::new(),
        };
        tokio::spawn(actor_loop(inner, cmd_rx, event_rx));
        Self { cmd_tx }
    }

    /// Request a spawn. Never blocks the caller beyond a 10s safety bound
    /// (the actor never awaits child work, so this is normally instant).
    pub async fn try_spawn(&self, req: SpawnReq) -> SpawnResult {
        let (tx, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(ManagerCmd::Spawn { req, respond: tx })
            .await
            .is_err()
        {
            return SpawnResult::Denied {
                reason: "subagent manager unavailable".to_string(),
            };
        }
        match tokio::time::timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(r)) => r,
            _ => SpawnResult::Denied {
                reason: "subagent manager unresponsive".to_string(),
            },
        }
    }

    /// Await a child's terminal outcome (works for queued children too —
    /// resolves when their run eventually completes).
    pub async fn await_outcome(&self, id: &str, session: &str, timeout: Duration) -> ChildOutcome {
        let (tx, rx) = oneshot::channel();
        let cmd = ManagerCmd::AwaitOutcome {
            id: Arc::from(id),
            session: Arc::from(session),
            respond: tx,
        };
        if self.cmd_tx.send(cmd).await.is_err() {
            return ChildOutcome::Failed("subagent manager unavailable".to_string());
        }
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(outcome)) => outcome,
            _ => ChildOutcome::Failed(format!(
                "subagent {id} did not finish within {}s",
                timeout.as_secs()
            )),
        }
    }

    /// Send follow-up / nudge input to a running child.
    pub async fn send_input(&self, id: &str, session: &str, msg: ChildMsg) -> Result<(), String> {
        let (tx, rx) = oneshot::channel();
        let cmd = ManagerCmd::SendInput {
            id: Arc::from(id),
            session: Arc::from(session),
            msg,
            respond: tx,
        };
        self.cmd_tx
            .send(cmd)
            .await
            .map_err(|_| "manager gone".to_string())?;
        tokio::time::timeout(Duration::from_secs(10), rx)
            .await
            .map_err(|_| "manager unresponsive".to_string())?
            .map_err(|_| "manager gone".to_string())?
    }

    /// Attach a usage writer for token attribution (`S:sub:uuid` sessions).
    /// Fire-and-forget; safe to call any time before completions land.
    pub async fn set_usage_writer(&self, writer: UsageWriter) {
        let _ = self.cmd_tx.send(ManagerCmd::SetUsageWriter(writer)).await;
    }

    /// Kill one child (running → interrupt + abort; queued → dequeue).
    pub async fn kill(&self, id: &str, session: &str) {
        let _ = self
            .cmd_tx
            .send(ManagerCmd::Kill {
                id: Arc::from(id),
                session: Arc::from(session),
            })
            .await;
    }

    /// Reset the unread counter (called when the detail modal opens).
    pub async fn mark_seen(&self, id: &str, session: &str) {
        let _ = self
            .cmd_tx
            .send(ManagerCmd::MarkSeen {
                id: Arc::from(id),
                session: Arc::from(session),
            })
            .await;
    }

    /// Kill a whole session pool (running + queued). Close/quit path.
    pub async fn close_session(&self, session: &str) {
        let _ = self
            .cmd_tx
            .send(ManagerCmd::CloseSession {
                session: Arc::from(session),
            })
            .await;
    }

    /// Live (running/waiting/queued) counts per parent session, for the
    /// `(+N other)` sidebar hint and saturation signals.
    pub async fn running_counts(&self) -> HashMap<String, usize> {
        let (tx, rx) = oneshot::channel();
        if self
            .cmd_tx
            .send(ManagerCmd::SessionCounts { respond: tx })
            .await
            .is_err()
        {
            return HashMap::new();
        }
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .ok()
            .and_then(|r| r.ok())
            .unwrap_or_default()
    }

    /// Drop finished children beyond the `keep` most recent (retention).
    pub async fn clear_finished(&self, session: &str, keep: usize) {
        let _ = self
            .cmd_tx
            .send(ManagerCmd::ClearFinished {
                session: Arc::from(session),
                keep,
            })
            .await;
    }

    /// Immutable snapshot for UI layers.
    pub async fn snapshot(&self, session: &str) -> PoolSnapshot {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .cmd_tx
            .send(ManagerCmd::Snapshot {
                session: Arc::from(session),
                respond: tx,
            })
            .await;
        rx.await.unwrap_or_default()
    }
}

// ========== Actor ==========

async fn actor_loop(
    mut inner: Inner,
    mut cmd_rx: mpsc::Receiver<ManagerCmd>,
    mut event_rx: mpsc::Receiver<ChildEvent>,
) {
    loop {
        tokio::select! {
            biased;
            Some(cmd) = cmd_rx.recv() => {
                // All commands are idempotent; the actor lives as long as
                // any manager handle exists (channel open).
                handle_cmd(cmd, &mut inner).await;
            }
            Some(ev) = event_rx.recv() => {
                handle_event(ev, &mut inner).await;
            }
            _ = inner.resource_rx.changed() => {
                let (min, max) = (1usize, inner.max);
                inner.effective = (*inner.resource_rx.borrow()).clamp(min, max.max(1));
                drain_queue(&mut inner).await;
            }
            else => break,
        }
    }
}

fn running_count(pool: &Pool) -> usize {
    pool.children
        .values()
        .filter(|c| matches!(c.status, SubStatus::Running | SubStatus::WaitingInput))
        .count()
}

fn sibling_scopes(pool: &Pool) -> Vec<SiblingScope> {
    pool.children
        .values()
        .filter(|c| {
            matches!(
                c.status,
                SubStatus::Running | SubStatus::WaitingInput | SubStatus::Queued
            )
        })
        .map(|c| SiblingScope {
            title: c.title.clone(),
            subroot: c.subroot.clone(),
            prompt_hash: c.prompt_hash,
        })
        .collect()
}

fn child_view(id: &Arc<str>, session: &Arc<str>, c: &Child) -> ChildView {
    let preview = c.log.tail(1);
    let preview: Arc<str> = Arc::from(preview.chars().take(80).collect::<String>());
    ChildView {
        id: id.clone(),
        parent_session: session.clone(),
        title: c.title.clone(),
        status: c.status,
        provider: c.provider.clone(),
        model: c.model.clone(),
        effort: c.effort.clone(),
        preview,
        log_tail: c.log.tail_lines(20),
        unread: c.unread,
        elapsed_s: c.started.elapsed().as_secs(),
    }
}

async fn handle_cmd(cmd: ManagerCmd, inner: &mut Inner) {
    match cmd {
        ManagerCmd::SetUsageWriter(writer) => {
            inner.usage = Some(writer);
        }
        ManagerCmd::Spawn { req, respond } => {
            let session: Arc<str> = req.parent_session.clone();
            let id: Arc<str> = Arc::from(format!("{}:sub:{}", session, uuid::Uuid::new_v4()));

            // Workspace escape gate (lexical; kernel enforces at open).
            if req.subroot != req.parent_root && !req.subroot.starts_with(&req.parent_root) {
                let _ = respond.send(SpawnResult::Denied {
                    reason: format!(
                        "subroot {} escapes parent workspace {} — spawn denied",
                        req.subroot.display(),
                        req.parent_root.display()
                    ),
                });
                return;
            }

            let pool = inner.pools.entry(session.clone()).or_insert_with(|| Pool {
                children: HashMap::new(),
                queue: VecDeque::new(),
                queued_waiters: HashMap::new(),
                grant: SessionGrant::default(),
            });

            if let Err(reason) = worthiness(&req.prompt, &req.subroot, &sibling_scopes(pool), false)
            {
                let _ = respond.send(SpawnResult::Denied { reason });
                return;
            }

            if running_count(pool) >= inner.effective.min(inner.max).max(1) {
                let position = pool.queue.len() + 1;
                pool.queue.push_back((id.clone(), req));
                tracing::info!("subagent {id} queued at position {position}");
                let _ = respond.send(SpawnResult::Queued { id, position });
                return;
            }

            spawn_child(inner, session, id.clone(), req, Vec::new());
            let _ = respond.send(SpawnResult::Spawned { id });
        }

        ManagerCmd::AwaitOutcome {
            id,
            session,
            respond,
        } => {
            // Unknown id: fail fast so callers never hang.
            match inner.pools.get_mut(&session) {
                None => {
                    let _ = respond.send(ChildOutcome::Failed(format!(
                        "subagent {id} is gone (killed or session closed)"
                    )));
                }
                Some(pool) => match pool.children.get_mut(&id) {
                    Some(child) => match child.outcome.clone() {
                        Some(outcome) => {
                            let _ = respond.send(outcome);
                        }
                        None => child.waiters.push(respond),
                    },
                    None => {
                        // Still queued → park until dequeue/completion.
                        pool.queued_waiters
                            .entry(id.clone())
                            .or_default()
                            .push(respond);
                    }
                },
            }
        }

        ManagerCmd::SendInput {
            id,
            session,
            msg,
            respond,
        } => {
            let mut result = Err(format!("subagent {id} is not running"));
            if let Some(pool) = inner.pools.get_mut(&session) {
                // Queued: fold follow-ups into the prompt; nudges dropped w/ log.
                if let Some(pos) = pool.queue.iter().position(|(qid, _)| *qid == id) {
                    match &msg {
                        ChildMsg::Followup(text) => {
                            pool.queue[pos].1.prompt.push_str("\n\nParent follow-up: ");
                            pool.queue[pos].1.prompt.push_str(text);
                            result = Ok(());
                        }
                        ChildMsg::Nudge(_) => {
                            tracing::debug!("nudge to queued {id} dropped (no turn yet)");
                            result = Ok(());
                        }
                    }
                } else if let Some(child) = pool.children.get_mut(&id) {
                    match child.status {
                        SubStatus::Running | SubStatus::WaitingInput => {
                            match child.inbox.try_send(msg) {
                                Ok(()) => result = Ok(()),
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    result = Err("child inbox full — try again shortly".to_string())
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    result = Err(format!("subagent {id} is not accepting input"))
                                }
                            }
                        }
                        _ => result = Err(format!("subagent {id} already finished")),
                    }
                }
            }
            let _ = respond.send(result);
        }

        ManagerCmd::Kill { id, session } => {
            let mut persisted = false;
            if let Some(pool) = inner.pools.get_mut(&session) {
                if let Some(pos) = pool.queue.iter().position(|(qid, _)| *qid == id) {
                    pool.queue.remove(pos);
                    for waiter in pool.queued_waiters.remove(&id).unwrap_or_default() {
                        let _ =
                            waiter.send(ChildOutcome::Killed("killed while queued".to_string()));
                    }
                    tracing::info!("subagent {id} dequeued by kill");
                    return;
                }
                if let Some(child) = pool.children.get_mut(&id) {
                    if matches!(
                        child.status,
                        SubStatus::Running | SubStatus::WaitingInput | SubStatus::Queued
                    ) {
                        child.interrupt.store(true, Ordering::SeqCst);
                        if let Some(handle) = child.handle.take() {
                            handle.abort();
                        }
                        finish_child(
                            pool,
                            &session,
                            &id,
                            ChildOutcome::Killed("killed by user".to_string()),
                        );
                        persisted = true;
                    }
                }
            }
            if persisted {
                persist_transcript(inner, &session, &id, "killed", "killed by user").await;
            }
        }

        ManagerCmd::MarkSeen { id, session } => {
            if let Some(pool) = inner.pools.get_mut(&session) {
                if let Some(child) = pool.children.get_mut(&id) {
                    child.unread = 0;
                }
            }
        }

        ManagerCmd::CloseSession { session } => {
            // Persist running transcripts BEFORE the pool is removed.
            let running: Vec<Arc<str>> = inner
                .pools
                .get(&session)
                .map(|pool| {
                    pool.children
                        .iter()
                        .filter(|(_, c)| {
                            matches!(
                                c.status,
                                SubStatus::Running | SubStatus::WaitingInput | SubStatus::Queued
                            )
                        })
                        .map(|(id, _)| id.clone())
                        .collect()
                })
                .unwrap_or_default();
            for id in &running {
                persist_transcript(inner, &session, id, "killed", "parent session closed").await;
            }
            if let Some(mut pool) = inner.pools.remove(&session) {
                pool.queue.clear();
                for (_, waiters) in pool.queued_waiters.drain() {
                    for waiter in waiters {
                        let _ =
                            waiter.send(ChildOutcome::Failed("parent session closed".to_string()));
                    }
                }
                let ids: Vec<Arc<str>> = pool.children.keys().cloned().collect();
                for id in ids {
                    if let Some(child) = pool.children.get_mut(&id) {
                        if matches!(
                            child.status,
                            SubStatus::Running | SubStatus::WaitingInput | SubStatus::Queued
                        ) {
                            child.interrupt.store(true, Ordering::SeqCst);
                            if let Some(handle) = child.handle.take() {
                                handle.abort();
                            }
                            finish_child(
                                &mut pool,
                                &session,
                                &id,
                                ChildOutcome::Killed("parent session closed".to_string()),
                            );
                        }
                    }
                }
                tracing::info!("subagent pool for {session} closed");
            }
        }

        ManagerCmd::ClearFinished { session, keep } => {
            if let Some(pool) = inner.pools.get_mut(&session) {
                // Running children always stay; finished are pruned to the
                // `keep` most recently started (recency proxy for completion).
                let mut finished: Vec<(Arc<str>, Instant)> = pool
                    .children
                    .iter()
                    .filter(|(_, c)| {
                        matches!(
                            c.status,
                            SubStatus::Completed | SubStatus::Failed | SubStatus::Killed
                        )
                    })
                    .map(|(id, c)| (id.clone(), c.started))
                    .collect();
                finished.sort_by_key(|a| std::cmp::Reverse(a.1));
                for (id, _) in finished.into_iter().skip(keep) {
                    pool.children.remove(&id);
                }
            }
        }

        ManagerCmd::SessionCounts { respond } => {
            let mut counts: HashMap<String, usize> = HashMap::new();
            for (session, pool) in inner.pools.iter() {
                let n = running_count(pool) + pool.queue.len();
                if n > 0 {
                    counts.insert(session.to_string(), n);
                }
            }
            let _ = respond.send(counts);
        }

        ManagerCmd::Snapshot { session, respond } => {
            let snap = inner
                .pools
                .get(&session)
                .map(|pool| {
                    let mut active: Vec<ChildView> = pool
                        .children
                        .iter()
                        .map(|(id, c)| child_view(id, &session, c))
                        .collect();
                    active.sort_by_key(|v| v.elapsed_s);
                    let queued = pool
                        .queue
                        .iter()
                        .enumerate()
                        .map(|(i, (id, req))| QueuedView {
                            id: id.clone(),
                            title: Arc::from(req.title.as_str()),
                            position: i + 1,
                        })
                        .collect();
                    PoolSnapshot { active, queued }
                })
                .unwrap_or_default();
            let _ = respond.send(snap);
        }
    }
}

/// Owned snapshot of everything a transcript needs (gathered under a
/// short read so persistence never holds pool borrows).
struct TranscriptData {
    workspace_root: PathBuf,
    title: String,
    provider: String,
    model: String,
    effort: String,
    subroot: PathBuf,
    log_tail: Vec<String>,
    elapsed_s: u64,
}

/// Persist one child transcript (best-effort, never fails the outcome).
///
/// Storage handles are cached per workspace root; the write itself goes
/// through `spawn_blocking` so the actor loop never blocks on the disk.
async fn persist_transcript(
    inner: &mut Inner,
    session: &Arc<str>,
    id: &Arc<str>,
    status: &str,
    summary: &str,
) {
    let data: Option<TranscriptData> = inner
        .pools
        .get(session)
        .and_then(|p| p.children.get(id))
        .map(|c| TranscriptData {
            workspace_root: c.workspace_root.clone(),
            title: c.title.to_string(),
            provider: c.provider.to_string(),
            model: c.model.to_string(),
            effort: c.effort.to_string(),
            subroot: c.subroot.clone(),
            log_tail: c
                .log
                .tail(LOG_CAP_LINES)
                .lines()
                .map(str::to_string)
                .collect(),
            elapsed_s: c.started.elapsed().as_secs(),
        });
    let Some(data) = data else { return };
    let storage = match inner.storages.get(&data.workspace_root) {
        Some(s) => s.clone(),
        None => match TarkStorage::new(&data.workspace_root) {
            Ok(s) => {
                let s = Arc::new(s);
                inner
                    .storages
                    .insert(data.workspace_root.clone(), s.clone());
                s
            }
            Err(e) => {
                tracing::warn!("subagent transcript skipped (storage): {e:#}");
                return;
            }
        },
    };
    let doc = SubTranscript {
        parent_session: session.to_string(),
        child_id: id.to_string(),
        title: data.title,
        provider: data.provider,
        model: data.model,
        effort: data.effort,
        subroot: data.subroot,
        status: status.to_string(),
        summary: summary.to_string(),
        log_tail: data.log_tail,
        elapsed_s: data.elapsed_s,
        input_tokens: 0,
        output_tokens: 0,
        context_transient: true,
        created_at: chrono::Utc::now(),
    };
    match tokio::task::spawn_blocking(move || storage.save_sub_transcript(&doc)).await {
        Ok(Ok(path)) => tracing::debug!("subagent transcript saved: {}", path.display()),
        Ok(Err(e)) => tracing::warn!("subagent transcript write failed: {e:#}"),
        Err(e) => tracing::warn!("subagent transcript task failed: {e}"),
    }
}

/// Move a terminal outcome into the child: store, resolve waiters, status.
fn finish_child(pool: &mut Pool, _session: &Arc<str>, id: &Arc<str>, outcome: ChildOutcome) {
    if let Some(child) = pool.children.get_mut(id) {
        child.status = match &outcome {
            ChildOutcome::Completed(_) => SubStatus::Completed,
            ChildOutcome::Failed(_) => SubStatus::Failed,
            ChildOutcome::Killed(_) => SubStatus::Killed,
        };
        child.outcome = Some(outcome.clone());
        for waiter in child.waiters.drain(..) {
            let _ = waiter.send(outcome.clone());
        }
        child.handle.take();
    }
}

async fn handle_event(ev: ChildEvent, inner: &mut Inner) {
    match ev {
        ChildEvent::Log { session, id, chunk } => {
            if let Some(pool) = inner.pools.get_mut(&session) {
                if let Some(child) = pool.children.get_mut(&id) {
                    for line in chunk.split('\n') {
                        if !line.trim().is_empty() {
                            child.log.push(line);
                        }
                    }
                    child.unread = child.unread.saturating_add(1);
                }
            }
        }
        ChildEvent::TurnDone {
            session,
            id,
            text,
            tool_calls,
            input_tokens,
            output_tokens,
        } => {
            // run_child sends exactly one terminal event per run.
            if let Some(writer) = inner.usage.clone() {
                let (provider, model) = inner
                    .pools
                    .get(&session)
                    .and_then(|p| p.children.get(&id))
                    .map(|c| (c.provider.to_string(), c.model.to_string()))
                    .unwrap_or_default();
                writer.log_subagent(SubagentUsage {
                    session_id: id.to_string(),
                    provider,
                    model,
                    input_tokens,
                    output_tokens,
                });
            }
            persist_transcript(inner, &session, &id, "completed", &text).await;
            if let Some(pool) = inner.pools.get_mut(&session) {
                let elapsed = pool
                    .children
                    .get(&id)
                    .map(|c| c.started.elapsed().as_secs())
                    .unwrap_or(0);
                finish_child(
                    pool,
                    &session,
                    &id,
                    ChildOutcome::Completed(ChildSummary {
                        id: id.clone(),
                        text,
                        tool_calls,
                        elapsed_s: elapsed,
                    }),
                );
                drain_queue_for(inner, &session).await;
            }
        }
        ChildEvent::TurnFailed { session, id, error } => {
            persist_transcript(inner, &session, &id, "failed", &error).await;
            if let Some(pool) = inner.pools.get_mut(&session) {
                finish_child(pool, &session, &id, ChildOutcome::Failed(error));
                drain_queue_for(inner, &session).await;
            }
        }
    }
}

/// Start queued spawns while capacity allows (single session).
async fn drain_queue_for(inner: &mut Inner, session: &Arc<str>) {
    loop {
        let next = {
            let pool = match inner.pools.get_mut(session) {
                Some(p) => p,
                None => return,
            };
            if running_count(pool) >= inner.effective.min(inner.max).max(1) {
                return;
            }
            pool.queue.pop_front()
        };
        match next {
            Some((id, req)) => {
                // Move any queued waiters into the child on start.
                let waiters = inner
                    .pools
                    .get_mut(session)
                    .and_then(|p| p.queued_waiters.remove(&id))
                    .unwrap_or_default();
                let started = spawn_child(inner, session.clone(), id.clone(), req, waiters);
                if !started {
                    // Pool vanished mid-drain (should not happen) — keep draining.
                    continue;
                }
            }
            None => return,
        }
    }
}

/// Drain all pools (after cap upshift).
async fn drain_queue(inner: &mut Inner) {
    let sessions: Vec<Arc<str>> = inner.pools.keys().cloned().collect();
    for s in sessions {
        drain_queue_for(inner, &s).await;
    }
}

/// Build the child agent and launch its task. Returns false only if the
/// session pool vanished mid-call (caller keeps draining).
fn spawn_child(
    inner: &mut Inner,
    session: Arc<str>,
    id: Arc<str>,
    req: SpawnReq,
    waiters: Vec<oneshot::Sender<ChildOutcome>>,
) -> bool {
    let pool = match inner.pools.get_mut(&session) {
        Some(p) => p,
        None => return false,
    };

    let spec = ChildRegistrySpec {
        mode: AgentMode::Ask,
        shell_enabled: false,
        trust: TrustLevel::Careful,
        session_id: id.to_string(),
        tool_timeout_secs: 60,
        max_iterations: req.max_iterations.clamp(1, 10),
    };
    let reg = build_child_registry(&spec, req.subroot.clone());
    let interrupt = Arc::new(AtomicBool::new(false));
    // NOTE: ToolRegistry owns its own flag; mirror the child flag into it so
    // a Kill is visible to a running tool within INTERRUPT_POLL_INTERVAL.
    let mut reg = reg;
    reg.set_interrupt_flag(interrupt.clone());

    let mut agent = ChatAgent::with_mode(req.llm.clone(), reg, AgentMode::Ask)
        .with_max_iterations(spec.max_iterations);
    if !req.effort.is_empty() && req.effort != "off" {
        agent.set_think_level_sync(req.effort.clone());
    }

    let (inbox_tx, inbox_rx) = mpsc::channel(INBOX_CAP);
    let event_tx = inner.event_tx.clone();
    let title: Arc<str> = Arc::from(req.title.as_str());
    let provider: Arc<str> = Arc::from(req.provider.as_str());
    let model: Arc<str> = Arc::from(req.model.as_str());
    let effort: Arc<str> = Arc::from(req.effort.as_str());
    let prompt_hash = prompt_hash(&req.prompt);

    let handle = tokio::spawn(run_child(ChildRun {
        id: id.clone(),
        session: session.clone(),
        agent,
        first_prompt: req.prompt,
        timeout: req.timeout,
        inbox: inbox_rx,
        interrupt: interrupt.clone(),
        event_tx: event_tx.clone(),
    }));
    let _ = event_tx.try_send(ChildEvent::Log {
        session: session.clone(),
        id: id.clone(),
        chunk: Arc::from(format!("spawned: {}", req.title).as_str()),
    });

    pool.children.insert(
        id.clone(),
        Child {
            title,
            provider,
            model,
            effort,
            subroot: req.subroot.clone(),
            workspace_root: req.parent_root.clone(),
            prompt_hash,
            status: SubStatus::Running,
            inbox: inbox_tx,
            interrupt,
            log: RingBuf::default(),
            unread: 0,
            started: Instant::now(),
            handle: Some(handle),
            outcome: None,
            waiters,
        },
    );
    tracing::info!("subagent {id} spawned in session {session}");
    true
}

// ========== Child task ==========

/// Inputs for one child run (bundled for clippy's arg limit).
struct ChildRun {
    id: Arc<str>,
    session: Arc<str>,
    agent: ChatAgent,
    first_prompt: String,
    timeout: Duration,
    inbox: mpsc::Receiver<ChildMsg>,
    interrupt: Arc<AtomicBool>,
    event_tx: mpsc::Sender<ChildEvent>,
}

/// Run one child to a terminal event. Single prompt plus a short grace
/// window for racing follow-ups; nudges become context notes.
async fn run_child(run: ChildRun) {
    let ChildRun {
        id,
        session,
        mut agent,
        first_prompt,
        timeout,
        mut inbox,
        interrupt,
        event_tx,
    } = run;
    let deadline = Instant::now() + timeout;
    let mut input_tokens: u32 = 0;
    let mut output_tokens: u32 = 0;
    let mut pending: Option<String> = Some(first_prompt);
    let mut notes: Vec<String> = Vec::new();
    let mut tool_calls = 0usize;

    let log = |text: String| {
        let _ = event_tx.try_send(ChildEvent::Log {
            session: session.clone(),
            id: id.clone(),
            chunk: Arc::from(text.as_str()),
        });
    };

    loop {
        if interrupt.load(Ordering::SeqCst) {
            break;
        }
        let prompt = match pending.take() {
            Some(p) => {
                if notes.is_empty() {
                    p
                } else {
                    format!(
                        "Context notes from parent (background only):\n{}\n\nTask:\n{}",
                        notes.join("\n"),
                        p
                    )
                }
            }
            None => {
                // Grace window: follow-ups arriving just after a turn count.
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    break;
                }
                let grace = FOLLOWUP_GRACE.min(remaining);
                match tokio::time::timeout(grace, inbox.recv()).await {
                    Ok(Some(ChildMsg::Followup(text))) => text,
                    Ok(Some(ChildMsg::Nudge(note))) => {
                        notes.push(note);
                        continue;
                    }
                    _ => break,
                }
            }
        };
        notes.clear();

        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            let _ = event_tx.try_send(ChildEvent::TurnFailed {
                session: session.clone(),
                id: id.clone(),
                error: format!("timed out after {}s", timeout.as_secs()),
            });
            return;
        }

        // Drain racing inputs that arrived while idle: latest follow-up wins
        // as the turn prompt, nudges become notes.
        let mut prompt = prompt;
        while let Ok(msg) = inbox.try_recv() {
            match msg {
                ChildMsg::Followup(text) => {
                    prompt.push_str("\n\nParent follow-up: ");
                    prompt.push_str(&text);
                }
                ChildMsg::Nudge(note) => notes.push(note),
            }
        }
        if !notes.is_empty() {
            prompt = format!(
                "Context notes from parent (background only):\n{}\n\nTask:\n{}",
                notes.join("\n"),
                prompt
            );
            notes.clear();
        }

        match tokio::time::timeout(remaining, agent.chat(&prompt)).await {
            Ok(Ok(resp)) => {
                tool_calls += resp.tool_calls_made;
                if let Some(u) = resp.usage.as_ref() {
                    input_tokens = input_tokens.saturating_add(u.input_tokens);
                    output_tokens = output_tokens.saturating_add(u.output_tokens);
                }
                log(format!(
                    "turn complete ({} tool calls this turn)",
                    resp.tool_calls_made
                ));
                // Immediate terminal event per turn would end follow-ups;
                // instead finish ONLY via grace expiry — but the tool layer
                // needs a result promptly. Compromise (v1): emit the summary
                // as the terminal event on the FIRST completed turn, then
                // keep serving follow-ups only if they already arrived.
                // Late follow-ups after Done are rejected ("already finished").
                let text = truncate_chars(&resp.text, SUMMARY_CHAR_BUDGET);
                let _ = event_tx.try_send(ChildEvent::TurnDone {
                    session: session.clone(),
                    id: id.clone(),
                    text,
                    tool_calls,
                    input_tokens,
                    output_tokens,
                });
                return;
            }
            Ok(Err(e)) => {
                let _ = event_tx.try_send(ChildEvent::TurnFailed {
                    session: session.clone(),
                    id: id.clone(),
                    error: format!("agent error: {e:#}"),
                });
                return;
            }
            Err(_) => {
                let _ = event_tx.try_send(ChildEvent::TurnFailed {
                    session: session.clone(),
                    id: id.clone(),
                    error: format!("timed out after {}s", timeout.as_secs()),
                });
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::{LlmProvider, LlmResponse, Message, ToolDefinition};
    use anyhow::Result;

    // --- pure policy tests ---

    #[test]
    fn worthiness_rejects_trivial() {
        let err = worthiness("read x", Path::new("/w"), &[], false).unwrap_err();
        assert!(err.contains("trivial"), "{err}");
    }

    #[test]
    fn worthiness_rejects_duplicate() {
        let sibs = vec![SiblingScope {
            title: Arc::from("auth scan"),
            subroot: PathBuf::from("/w/src/auth"),
            prompt_hash: prompt_hash(
                "find every caller of authenticated() across the whole auth module in detail",
            ),
        }];
        let err = worthiness(
            "find every caller of authenticated() across the whole auth module in detail",
            Path::new("/w/other"),
            &sibs,
            false,
        )
        .unwrap_err();
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn worthiness_allows_readonly_overlap() {
        let sibs = vec![SiblingScope {
            title: Arc::from("other"),
            subroot: PathBuf::from("/w/src"),
            prompt_hash: 123,
        }];
        assert!(worthiness(
            "a sufficiently long independent exploration prompt describing the auth call graph",
            Path::new("/w/src/auth"),
            &sibs,
            false
        )
        .is_ok());
    }

    #[test]
    fn worthiness_denies_overlap_when_writes_possible() {
        let sibs = vec![SiblingScope {
            title: Arc::from("other"),
            subroot: PathBuf::from("/w/src"),
            prompt_hash: 123,
        }];
        let err = worthiness(
            "a sufficiently long independent exploration prompt describing the auth call graph",
            Path::new("/w/src/auth"),
            &sibs,
            true,
        )
        .unwrap_err();
        assert!(err.contains("shared scope"), "{err}");
    }

    #[test]
    fn scopes_overlap_prefix_both_ways() {
        assert!(scopes_overlap(Path::new("/w/a"), Path::new("/w/a/b")));
        assert!(scopes_overlap(Path::new("/w/a/b"), Path::new("/w/a")));
        assert!(!scopes_overlap(Path::new("/w/a"), Path::new("/w/b")));
    }

    #[test]
    fn derive_spec_is_locked_down() {
        let spec = derive_child_registry_spec("sess1");
        assert_eq!(spec.mode, AgentMode::Ask);
        assert!(!spec.shell_enabled);
        assert!(spec.session_id.starts_with("sess1:sub:"));
    }

    #[test]
    fn ringbuf_caps_bytes_and_lines() {
        let mut ring = RingBuf::default();
        for i in 0..200 {
            ring.push(&"x".repeat(1000));
            let _ = i;
        }
        assert!(ring.len() <= LOG_CAP_LINES);
        assert!(ring.bytes <= LOG_CAP_BYTES);
        assert!(!ring.tail(1).is_empty());
    }

    // --- actor tests with a controllable provider ---

    /// Test provider whose `chat` blocks on a gate until released, then
    /// returns canned text. Gives deterministic queue/kill/close tests.
    struct GateProvider {
        gate: Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,
        release: Arc<tokio::sync::Mutex<Option<oneshot::Receiver<()>>>>,
    }

    impl GateProvider {
        fn new() -> (Self, oneshot::Sender<()>) {
            // Single gate: first chat waits, release opens it once.
            let (tx, rx) = oneshot::channel();
            (
                Self {
                    gate: Arc::new(tokio::sync::Mutex::new(None)),
                    release: Arc::new(tokio::sync::Mutex::new(Some(rx))),
                },
                tx,
            )
        }
    }

    #[async_trait::async_trait]
    impl LlmProvider for GateProvider {
        fn name(&self) -> &str {
            "gate"
        }

        async fn chat(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> Result<LlmResponse> {
            if let Some(rx) = self.release.lock().await.take() {
                let _ = rx.await;
            }
            Ok(LlmResponse::Text {
                text: "gated summary".to_string(),
                usage: Some(crate::llm::TokenUsage {
                    input_tokens: 10,
                    output_tokens: 5,
                    total_tokens: 15,
                }),
            })
        }

        async fn complete_fim(
            &self,
            _prefix: &str,
            _suffix: &str,
            _language: &str,
        ) -> Result<crate::llm::CompletionResult> {
            anyhow::bail!("gate: no FIM support")
        }

        async fn explain_code(&self, _code: &str, _context: &str) -> Result<String> {
            Ok("gated explanation".to_string())
        }

        async fn suggest_refactorings(
            &self,
            _code: &str,
            _context: &str,
        ) -> Result<Vec<crate::llm::RefactoringSuggestion>> {
            Ok(Vec::new())
        }

        async fn review_code(
            &self,
            _code: &str,
            _language: &str,
        ) -> Result<Vec<crate::llm::CodeIssue>> {
            Ok(Vec::new())
        }
    }

    fn test_cfg(max: usize) -> SubagentConfig {
        SubagentConfig {
            max_subagents: max,
            min_subagents: 1,
            ..SubagentConfig::default()
        }
    }

    fn spawn_req(
        llm: Arc<dyn LlmProvider>,
        session: &str,
        title: &str,
        prompt: &str,
        dir: &Path,
    ) -> SpawnReq {
        SpawnReq {
            parent_session: Arc::from(session),
            parent_root: dir.to_path_buf(),
            title: title.to_string(),
            prompt: prompt.to_string(),
            subroot: dir.to_path_buf(),
            llm,
            provider: "gate".to_string(),
            model: "m".to_string(),
            effort: "off".to_string(),
            max_iterations: 2,
            timeout: Duration::from_secs(30),
        }
    }

    const LONG_PROMPT: &str =
        "explore the authentication module call graph in detail and list every caller";

    fn manager_with_effective(max: usize, effective: usize) -> SubagentManager {
        let (_tx, rx) = watch::channel(effective);
        SubagentManager::new(&test_cfg(max), rx)
    }

    #[tokio::test]
    async fn spawn_runs_to_summary() {
        let temp = tempfile::TempDir::new().unwrap();
        let (gate, release) = GateProvider::new();
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(5, 5);

        let id = match mgr
            .try_spawn(spawn_req(llm, "s1", "t1", LONG_PROMPT, temp.path()))
            .await
        {
            SpawnResult::Spawned { id } => id,
            other => panic!("expected spawn, got {other:?}"),
        };
        let _ = release.send(());
        match mgr.await_outcome(&id, "s1", Duration::from_secs(30)).await {
            ChildOutcome::Completed(s) => assert!(s.text.contains("gated summary")),
            other => panic!("expected completed, got {other:?}"),
        }
        let snap = mgr.snapshot("s1").await;
        assert_eq!(snap.active.len(), 1);
        assert_eq!(snap.active[0].status, SubStatus::Completed);
    }

    #[tokio::test]
    async fn second_spawn_queues_when_full() {
        let temp = tempfile::TempDir::new().unwrap();
        let (gate, release) = GateProvider::new();
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(1, 1);

        let first = match mgr
            .try_spawn(spawn_req(llm.clone(), "s1", "t1", LONG_PROMPT, temp.path()))
            .await
        {
            SpawnResult::Spawned { id } => id,
            other => panic!("expected spawn, got {other:?}"),
        };
        // First child blocks in chat; second must queue deterministically.
        match mgr
            .try_spawn(spawn_req(
                llm,
                "s1",
                "t2",
                "a different long exploration prompt about session handling flows",
                temp.path(),
            ))
            .await
        {
            SpawnResult::Queued { position, .. } => assert_eq!(position, 1),
            other => panic!("expected queued, got {other:?}"),
        }
        let snap = mgr.snapshot("s1").await;
        assert_eq!(snap.queued.len(), 1);

        // Release the gate: first completes, queued child starts and blocks
        // on the (consumed) gate → completes immediately with canned text.
        let _ = release.send(());
        match mgr
            .await_outcome(&first, "s1", Duration::from_secs(30))
            .await
        {
            ChildOutcome::Completed(_) => {}
            other => panic!("expected completed, got {other:?}"),
        }
        // Queued child drained and finished.
        let snap = mgr.snapshot("s1").await;
        assert!(snap.queued.is_empty());
        assert_eq!(snap.active.len(), 2);
    }

    #[tokio::test]
    async fn kill_running_child() {
        let temp = tempfile::TempDir::new().unwrap();
        let (gate, _release) = GateProvider::new(); // never released
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(5, 5);

        let id = match mgr
            .try_spawn(spawn_req(llm, "s1", "t1", LONG_PROMPT, temp.path()))
            .await
        {
            SpawnResult::Spawned { id } => id,
            other => panic!("expected spawn, got {other:?}"),
        };
        // Give the actor a chance to start the child task.
        tokio::time::sleep(Duration::from_millis(100)).await;
        mgr.kill(&id, "s1").await;
        match mgr.await_outcome(&id, "s1", Duration::from_secs(10)).await {
            ChildOutcome::Killed(_) => {}
            other => panic!("expected killed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn close_session_kills_pool() {
        let temp = tempfile::TempDir::new().unwrap();
        let (gate, _release) = GateProvider::new();
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(5, 5);

        let id = match mgr
            .try_spawn(spawn_req(llm, "s1", "t1", LONG_PROMPT, temp.path()))
            .await
        {
            SpawnResult::Spawned { id } => id,
            other => panic!("expected spawn, got {other:?}"),
        };
        tokio::time::sleep(Duration::from_millis(100)).await;
        mgr.close_session("s1").await;
        match mgr.await_outcome(&id, "s1", Duration::from_secs(10)).await {
            ChildOutcome::Failed(msg) | ChildOutcome::Killed(msg) => {
                assert!(!msg.is_empty())
            }
            other => panic!("expected terminal, got {other:?}"),
        }
        let snap = mgr.snapshot("s1").await;
        assert!(snap.active.is_empty());
    }

    #[tokio::test]
    async fn completion_persists_transcript_and_usage() {
        use crate::storage::usage::{UsageTracker, UsageWriter};

        let tmp = tempfile::TempDir::new().unwrap();
        let tracker = Arc::new(UsageTracker::new(tmp.path()).unwrap());
        let writer = UsageWriter::spawn(tracker.clone());

        let (gate, release) = GateProvider::new();
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(5, 5);
        // Same channel = FIFO: writer is set before the spawn below runs.
        mgr.set_usage_writer(writer).await;

        let id = match mgr
            .try_spawn(spawn_req(llm, "s1", "t1", LONG_PROMPT, tmp.path()))
            .await
        {
            SpawnResult::Spawned { id } => id,
            other => panic!("expected spawn, got {other:?}"),
        };
        let _ = release.send(());
        match mgr.await_outcome(&id, "s1", Duration::from_secs(30)).await {
            ChildOutcome::Completed(_) => {}
            other => panic!("expected completed, got {other:?}"),
        }

        // Transcript is written before waiters resolve — deterministic.
        let uuid = id.rsplit(':').next().unwrap();
        let transcript = tmp
            .path()
            .join(".tark")
            .join("sessions")
            .join("s1")
            .join("conversations")
            .join(format!("sub_{uuid}.json"));
        assert!(transcript.exists(), "missing {transcript:?}");
        let raw = std::fs::read_to_string(&transcript).unwrap();
        let doc: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(doc["context_transient"], true);
        assert_eq!(doc["parent_session"], "s1");

        // Usage attribution is async — poll with deadline.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let totals = loop {
            let t = tracker.get_session_totals(&id).unwrap();
            if t.0 == 15 || std::time::Instant::now() > deadline {
                break t;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        assert_eq!(totals.0, 15, "child tokens attributed to S:sub:uuid");
    }

    #[tokio::test]
    async fn spawn_denies_workspace_escape() {
        let temp = tempfile::TempDir::new().unwrap();
        let (gate, _release) = GateProvider::new();
        let llm: Arc<dyn LlmProvider> = Arc::new(gate);
        let mgr = manager_with_effective(5, 5);

        let mut req = spawn_req(llm, "s1", "t1", LONG_PROMPT, temp.path());
        req.subroot = PathBuf::from("/definitely/outside/the/workspace");
        match mgr.try_spawn(req).await {
            SpawnResult::Denied { reason } => assert!(reason.contains("escapes")),
            other => panic!("expected denied, got {other:?}"),
        }
    }
}
