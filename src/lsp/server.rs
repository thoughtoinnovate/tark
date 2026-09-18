//! Main LSP server implementation
//!
//! Deliberate capability subset (requirement R9; this server does **not** claim
//! complete LSP 3.18 compliance):
//!
//! * `initialize` / `shutdown` / `exit`
//! * document synchronization: `INCREMENTAL` advertised, `FULL` still accepted
//! * UTF-16 position conversion (see [`document`] helpers)
//! * `completion`, `hover`, `code_action` (only `refactor` kinds; the returned
//!   `WorkspaceEdit`s are suggestions the client applies — the server itself
//!   never writes files, spawns processes, or mutates editor state)
//! * `diagnostics` (per-document, version-guarded, cancellable)
//! * cancellation: in-flight requests are aborted by tower-lsp 0.20 itself on
//!   `$/cancelRequest` (see `DiagnosticsTracker` docs); background diagnostics
//!   are aborted via [`DiagnosticsTracker`]
//! * workspace folders (`rootUri` + `workspaceFolders`, fail-closed scoping)
//!
//! Read-only / ownership boundary: LSP code paths create no agent sessions, no
//! MCP connections, no permission state, and no persistent policy state. The
//! backend only holds a [`DocumentStore`], workspace roots, diagnostics
//! bookkeeping, an LLM provider handle shared with the completion/diagnostics
//! engines, and configuration. Verified by inspection: nothing under `src/lsp/`
//! references `crate::agent`, `crate::mcp`, tools, or policy state.

use super::document::DocumentStore;
use super::{code_action, completion, diagnostics, hover};
use crate::completion::CompletionEngine;
use crate::config::Config;
use crate::diagnostics::DiagnosticsEngine;
use crate::llm::{self, LlmProvider};
use anyhow::Result;
use dashmap::{DashMap, DashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tower_lsp::jsonrpc::{Error, Result as JsonRpcResult};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// Bound for every LLM-backed LSP computation (completion, hover, code action,
/// diagnostics). On expiry the handler logs a warning and falls back to
/// `Ok(None)` (no result) or skips the diagnostics publish.
const LLM_TIMEOUT: Duration = Duration::from_secs(30);

/// Returns true when `uri` is contained in `roots`.
///
/// Containment requires the same scheme and host plus a whole-path-segment
/// prefix match, so `file:///a/bc` is not inside `file:///a/b`. An empty root
/// list means the client advertised no workspace scope, in which case there is
/// nothing to enforce against and every URI is accepted.
fn uri_within_any(roots: &[Url], uri: &Url) -> bool {
    if roots.is_empty() {
        return true;
    }
    roots.iter().any(|root| {
        if root.scheme() != uri.scheme() || root.host_str() != uri.host_str() {
            return false;
        }
        match (root.path_segments(), uri.path_segments()) {
            (Some(root_segs), Some(uri_segs)) => {
                let root_segs: Vec<&str> = root_segs.collect();
                let uri_segs: Vec<&str> = uri_segs.collect();
                uri_segs.len() >= root_segs.len() && uri_segs[..root_segs.len()] == root_segs[..]
            }
            // Non-hierarchical URIs (e.g. `untitled:`) cannot be proven inside
            // a folder: fail closed.
            _ => false,
        }
    })
}

/// Known workspace roots from `initialize` (`rootUri` + `workspaceFolders`)
/// and `workspace/didChangeWorkspaceFolders`.
///
/// A stored `Vec<Url>` plus a containment check is sufficient; no per-folder
/// document partitioning is needed.
#[derive(Debug, Default)]
pub struct WorkspaceState {
    roots: Vec<Url>,
}

impl WorkspaceState {
    /// Build the initial scope. `rootUri` is honored when the client sends no
    /// `workspaceFolders` (it remains common in practice despite being
    /// deprecated in favor of workspace folders).
    pub fn new(root_uri: Option<Url>, folders: Option<Vec<WorkspaceFolder>>) -> Self {
        let mut roots = Vec::new();
        if let Some(folders) = folders {
            roots.extend(folders.into_iter().map(|f| f.uri));
        }
        if roots.is_empty() {
            roots.extend(root_uri);
        }
        Self { roots }
    }

    /// Apply a `workspace/didChangeWorkspaceFolders` event.
    pub fn apply_change(&mut self, added: Vec<WorkspaceFolder>, removed: Vec<WorkspaceFolder>) {
        self.roots
            .retain(|root| !removed.iter().any(|r| r.uri == *root));
        for folder in added {
            if !self.roots.contains(&folder.uri) {
                self.roots.push(folder.uri);
            }
        }
    }

    pub fn roots(&self) -> Vec<Url> {
        self.roots.clone()
    }

    /// Fail-closed containment check: with a known scope, URIs outside every
    /// root are rejected; with no known scope everything is accepted.
    pub fn contains(&self, uri: &Url) -> bool {
        uri_within_any(&self.roots, uri)
    }
}

/// Tracks background diagnostics work per document plus publish staleness.
///
/// Cancellation model (requirement R9, scenario S24):
///
/// * tower-lsp 0.20 aborts **in-flight request futures** itself when the client
///   sends `$/cancelRequest`: its generated router registers that method
///   against an internal pending-request map and every request handler runs
///   inside a cancellable middleware future. That map (`Pending`) is
///   `pub(crate)`, so there is no public cancellation-token API for the
///   backend to wire into — and none is needed for requests.
/// * Diagnostics, however, run as **background tasks spawned from
///   notifications** (`didOpen`/`didChange` carry no request id, so
///   `$/cancelRequest` can never name them). Those tasks are tracked here keyed
///   by document URI: a newer version aborts the previous task, `didClose`
///   aborts and forgets the document, and a publish is skipped whenever its
///   version is older than the latest seen for that document, so stale
///   diagnostics are never published.
#[derive(Debug, Default)]
pub struct DiagnosticsTracker {
    /// Currently running diagnostics task per document: (version, handle).
    pending: DashMap<Url, (i32, JoinHandle<()>)>,
    /// Latest document version seen via open/change per document.
    latest: DashMap<Url, i32>,
    /// Last version actually published per document.
    last_published: DashMap<Url, i32>,
    /// Documents closed since their last open; guards against a task that was
    /// spawned but not yet tracked racing a `didClose` clear-publish.
    closed: DashSet<Url>,
}

impl DiagnosticsTracker {
    /// Record the latest seen version for an open document (monotonic).
    pub fn note_version(&self, uri: &Url, version: i32) {
        self.latest
            .entry(uri.clone())
            .and_modify(|v| *v = (*v).max(version))
            .or_insert(version);
    }

    /// Record an open: resets the latest version (re-opens may restart
    /// versioning) and clears any closed tombstone.
    pub fn note_open(&self, uri: &Url, version: i32) {
        self.latest.insert(uri.clone(), version);
        self.closed.remove(uri);
    }

    /// Store a newly spawned task, aborting the previous one for the document.
    pub fn track_pending(&self, uri: Url, version: i32, handle: JoinHandle<()>) {
        if let Some((_, (_, old))) = self.pending.remove(&uri) {
            old.abort();
        }
        self.pending.insert(uri, (version, handle));
    }

    /// Abort and drop the pending task for a document, if any.
    pub fn cancel_pending(&self, uri: &Url) -> bool {
        if let Some((_, (_, handle))) = self.pending.remove(uri) {
            handle.abort();
            true
        } else {
            false
        }
    }

    /// True when `version` is still the tracked pending task for `uri`.
    pub fn is_tracked(&self, uri: &Url, version: i32) -> bool {
        self.pending
            .get(uri)
            .map(|entry| entry.value().0 == version)
            .unwrap_or(false)
    }

    /// True when diagnostics computed for `version` may still be published:
    /// the document is open and `version` is not older than the latest seen.
    pub fn should_publish(&self, uri: &Url, version: i32) -> bool {
        if self.closed.contains(uri) {
            return false;
        }
        self.latest.get(uri).map(|v| version >= *v).unwrap_or(true)
    }

    /// Record a completed publish (monotonic per document).
    pub fn record_published(&self, uri: &Url, version: i32) {
        self.last_published
            .entry(uri.clone())
            .and_modify(|v| *v = (*v).max(version))
            .or_insert(version);
    }

    /// Last version actually published per document (test observability).
    #[allow(dead_code)]
    pub fn last_published_version(&self, uri: &Url) -> Option<i32> {
        self.last_published.get(uri).map(|v| *v)
    }

    /// Abort every pending task (used by the explicit `exit` handler).
    pub fn abort_all(&self) {
        for entry in self.pending.iter() {
            entry.value().1.abort();
        }
        self.pending.clear();
    }

    /// Abort pending work for documents outside `roots` (workspace shrank).
    /// Returns the number of aborted tasks.
    pub fn abort_out_of_scope(&self, roots: &[Url]) -> usize {
        let outside: Vec<Url> = self
            .pending
            .iter()
            .map(|entry| entry.key().clone())
            .filter(|uri| !uri_within_any(roots, uri))
            .collect();
        let count = outside.len();
        for uri in &outside {
            self.cancel_pending(uri);
            tracing::warn!(uri = %uri, "aborted pending diagnostics outside workspace scope");
        }
        count
    }

    /// Forget a closed document: aborts pending work, drops publish state,
    /// and installs a closed tombstone so late tasks cannot publish stale
    /// diagnostics over the `didClose` clear.
    pub fn forget(&self, uri: &Url) {
        self.cancel_pending(uri);
        self.last_published.remove(uri);
        self.closed.insert(uri.clone());
    }
}

/// Backend exit/shutdown flag.
///
/// tower-lsp 0.20 intercepts the `exit` notification in its `Exit` middleware
/// (cancelling in-flight requests and closing the transport) and never
/// dispatches it to the backend, so the flag is set through the explicit
/// [`EngLspBackend::handle_exit`] handler, which [`run_lsp_server`] invokes
/// once the transport shuts down.
#[derive(Debug, Clone, Default)]
pub struct ShutdownState {
    exited: Arc<AtomicBool>,
}

impl ShutdownState {
    pub fn set_exited(&self) {
        self.exited.store(true, Ordering::SeqCst);
    }

    /// Test/embedder observability for the exit flag.
    #[allow(dead_code)]
    pub fn is_exited(&self) -> bool {
        self.exited.load(Ordering::SeqCst)
    }
}

/// The LSP backend.
///
/// Supported subset (deliberate; not complete LSP 3.18): initialize,
/// shutdown, exit, incremental (+full) text sync with UTF-16 positions,
/// completion, hover, `refactor` code actions, version-guarded diagnostics,
/// cancellation, and workspace folders. Stateless and read-only: no agent
/// sessions, MCP connections, permission state, or persistent policy state are
/// created from any LSP code path.
pub struct EngLspBackend {
    client: Client,
    documents: Arc<DocumentStore>,
    workspace: Arc<RwLock<WorkspaceState>>,
    diagnostics_state: Arc<DiagnosticsTracker>,
    exit_state: ShutdownState,
    completion_engine: Arc<CompletionEngine>,
    diagnostics_engine: Arc<DiagnosticsEngine>,
    llm: Arc<dyn LlmProvider>,
    config: Arc<Config>,
}

impl EngLspBackend {
    pub fn new(client: Client, llm: Arc<dyn LlmProvider>, config: Config) -> Self {
        let completion_engine = CompletionEngine::new(llm.clone())
            .with_cache_size(config.completion.cache_size)
            .with_context_lines(
                config.completion.context_lines_before,
                config.completion.context_lines_after,
            );

        let diagnostics_engine =
            DiagnosticsEngine::new(llm.clone()).with_debounce(config.completion.debounce_ms);

        Self {
            client,
            documents: Arc::new(DocumentStore::new()),
            workspace: Arc::new(RwLock::new(WorkspaceState::default())),
            diagnostics_state: Arc::new(DiagnosticsTracker::default()),
            exit_state: ShutdownState::default(),
            completion_engine: Arc::new(completion_engine),
            diagnostics_engine: Arc::new(diagnostics_engine),
            llm,
            config: Arc::new(config),
        }
    }

    /// Explicit backend-level `exit` handling: marks the backend exited and
    /// aborts all pending diagnostics work.
    ///
    /// tower-lsp 0.20 performs the protocol-level `exit` teardown itself (its
    /// `Exit` middleware cancels in-flight requests and closes the transport
    /// without dispatching to the backend); [`run_lsp_server`] retains a
    /// backend handle and invokes this method once `serve()` returns.
    pub fn handle_exit(&self) {
        self.exit_state.set_exited();
        self.diagnostics_state.abort_all();
        tracing::info!("LSP exit: backend marked exited, pending diagnostics aborted");
    }

    /// Fail-closed workspace check used by every document-accepting method.
    async fn workspace_allows(&self, uri: &Url) -> bool {
        self.workspace.read().await.contains(uri)
    }

    /// Reject an out-of-scope document from a notification handler: there is
    /// no JSON-RPC response channel for notifications, so the rejection is a
    /// warning trace plus a client-facing log message, and the document is
    /// neither stored nor diagnosed (fail closed).
    async fn reject_out_of_scope(&self, method: &str, uri: &Url) {
        tracing::warn!(method, uri = %uri, "rejecting out-of-workspace document (fail-closed)");
        self.client
            .log_message(
                MessageType::WARNING,
                format!("tark: ignoring {uri} outside the known workspace folders"),
            )
            .await;
    }

    /// Spawn version-tracked background diagnostics for an open document.
    ///
    /// Any previous pending task for the document is aborted first, so work is
    /// scoped per document and a newer version always wins (scenario S24).
    fn spawn_diagnostics(&self, uri: Url, version: i32) {
        let debounce_ms = self.config.completion.debounce_ms;
        let client = self.client.clone();
        let documents = self.documents.clone();
        let diagnostics_engine = self.diagnostics_engine.clone();
        let tracker = self.diagnostics_state.clone();

        tracker.note_version(&uri, version);
        tracker.cancel_pending(&uri);
        let task_uri = uri.clone();
        let task_tracker = tracker.clone();
        let handle = tokio::spawn(async move {
            // Debounce: rapid successive edits collapse onto the latest task,
            // because each new version aborts the previous sleeper.
            tokio::time::sleep(Duration::from_millis(debounce_ms)).await;

            // Belt and braces: the task may have been superseded between spawn
            // and wake-up without its handle being aborted in time.
            if !task_tracker.is_tracked(&task_uri, version) {
                return;
            }

            let doc = match documents.get(&task_uri) {
                Some(d) => d,
                None => return,
            };

            match tokio::time::timeout(
                LLM_TIMEOUT,
                diagnostics::run_diagnostics(&diagnostics_engine, &doc),
            )
            .await
            {
                Ok(Ok(diags)) => {
                    // Never publish diagnostics for a version older than the
                    // latest seen, and never for a closed document.
                    if !task_tracker.should_publish(&task_uri, version) {
                        tracing::warn!(
                            uri = %task_uri,
                            version,
                            "dropping stale diagnostics publish"
                        );
                        return;
                    }
                    client
                        .publish_diagnostics(task_uri.clone(), diags, None)
                        .await;
                    task_tracker.record_published(&task_uri, version);
                }
                Ok(Err(e)) => {
                    tracing::error!(uri = %task_uri, version, "diagnostics failed: {e:#}");
                }
                Err(_) => {
                    tracing::warn!(
                        method = "textDocument/publishDiagnostics",
                        elapsed_secs = LLM_TIMEOUT.as_secs(),
                        uri = %task_uri,
                        version,
                        "LLM-backed diagnostics timed out; skipping publish"
                    );
                }
            }
        });
        tracker.track_pending(uri, version, handle);
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for EngLspBackend {
    #[allow(deprecated)]
    async fn initialize(&self, params: InitializeParams) -> JsonRpcResult<InitializeResult> {
        // `rootUri` is deprecated in favor of `workspaceFolders`, but clients
        // still send it when no folders are configured, so it remains the
        // fallback scope root.
        {
            let mut workspace = self.workspace.write().await;
            *workspace =
                WorkspaceState::new(params.root_uri.clone(), params.workspace_folders.clone());
            tracing::info!(roots = ?workspace.roots(), "LSP workspace scope initialized");
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Incremental sync advertised; full-document changes are still
                // accepted as input (see `Document::apply_content_changes`).
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![
                        ".".to_string(),
                        ":".to_string(),
                        "(".to_string(),
                        " ".to_string(),
                    ]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Only `refactor` actions are ever produced; each carries a
                // `WorkspaceEdit` suggestion that the client — not the server —
                // applies. The server performs no workspace mutation itself.
                code_action_provider: Some(CodeActionProviderCapability::Options(
                    CodeActionOptions {
                        code_action_kinds: Some(vec![CodeActionKind::REFACTOR]),
                        ..Default::default()
                    },
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "tark".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        tracing::info!("LSP server initialized");
        self.client
            .log_message(MessageType::INFO, "tark LSP server ready")
            .await;
    }

    async fn shutdown(&self) -> JsonRpcResult<()> {
        tracing::info!("LSP server shutting down");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        if !self.workspace_allows(&uri).await {
            self.reject_out_of_scope("textDocument/didOpen", &uri).await;
            return;
        }
        self.documents.open(params);
        self.diagnostics_state.note_open(&uri, version);
        self.spawn_diagnostics(uri, version);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let version = params.text_document.version;
        if !self.workspace_allows(&uri).await {
            self.reject_out_of_scope("textDocument/didChange", &uri)
                .await;
            return;
        }
        self.documents.change(params);
        // Only diagnose documents the client actually opened with us.
        if self.documents.get(&uri).is_none() {
            return;
        }
        self.spawn_diagnostics(uri, version);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        // Always clean up local state, regardless of workspace scope: the
        // client no longer manages this document. Aborting first guarantees no
        // stale publish can land after the clear below.
        self.diagnostics_state.forget(&uri);
        self.documents.close(params);
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let roots = {
            let mut workspace = self.workspace.write().await;
            workspace.apply_change(params.event.added, params.event.removed);
            let roots = workspace.roots();
            tracing::info!(roots = ?roots, "LSP workspace folders changed");
            roots
        };
        // Documents that fell out of scope stop getting diagnostics; their
        // stored text is left alone until `didClose`.
        self.diagnostics_state.abort_out_of_scope(&roots);
    }

    async fn completion(
        &self,
        params: CompletionParams,
    ) -> JsonRpcResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.clone();
        if !self.workspace_allows(&uri).await {
            tracing::warn!(
                method = "textDocument/completion",
                uri = %uri,
                "rejecting out-of-workspace request (fail-closed)"
            );
            return Err(Error::invalid_params(format!(
                "document {uri} is outside the known workspace folders"
            )));
        }
        let started = std::time::Instant::now();
        match tokio::time::timeout(
            LLM_TIMEOUT,
            completion::handle_completion(&self.completion_engine, &self.documents, params),
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => {
                tracing::error!("Completion error: {}", e);
                Ok(None)
            }
            Err(_) => {
                tracing::warn!(
                    method = "textDocument/completion",
                    elapsed_secs = started.elapsed().as_secs(),
                    "LLM-backed operation timed out; returning no result"
                );
                Ok(None)
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> JsonRpcResult<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .clone();
        if !self.workspace_allows(&uri).await {
            tracing::warn!(
                method = "textDocument/hover",
                uri = %uri,
                "rejecting out-of-workspace request (fail-closed)"
            );
            return Err(Error::invalid_params(format!(
                "document {uri} is outside the known workspace folders"
            )));
        }
        let started = std::time::Instant::now();
        match tokio::time::timeout(
            LLM_TIMEOUT,
            hover::handle_hover(self.llm.clone(), &self.documents, params),
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => {
                tracing::error!("Hover error: {}", e);
                Ok(None)
            }
            Err(_) => {
                tracing::warn!(
                    method = "textDocument/hover",
                    elapsed_secs = started.elapsed().as_secs(),
                    "LLM-backed operation timed out; returning no result"
                );
                Ok(None)
            }
        }
    }

    async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> JsonRpcResult<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.clone();
        if !self.workspace_allows(&uri).await {
            tracing::warn!(
                method = "textDocument/codeAction",
                uri = %uri,
                "rejecting out-of-workspace request (fail-closed)"
            );
            return Err(Error::invalid_params(format!(
                "document {uri} is outside the known workspace folders"
            )));
        }
        let started = std::time::Instant::now();
        match tokio::time::timeout(
            LLM_TIMEOUT,
            code_action::handle_code_action(self.llm.clone(), &self.documents, params),
        )
        .await
        {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(e)) => {
                tracing::error!("Code action error: {}", e);
                Ok(None)
            }
            Err(_) => {
                tracing::warn!(
                    method = "textDocument/codeAction",
                    elapsed_secs = started.elapsed().as_secs(),
                    "LLM-backed operation timed out; returning no result"
                );
                Ok(None)
            }
        }
    }
}

impl Clone for EngLspBackend {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            documents: self.documents.clone(),
            workspace: self.workspace.clone(),
            diagnostics_state: self.diagnostics_state.clone(),
            exit_state: self.exit_state.clone(),
            completion_engine: self.completion_engine.clone(),
            diagnostics_engine: self.diagnostics_engine.clone(),
            llm: self.llm.clone(),
            config: self.config.clone(),
        }
    }
}

/// Run the LSP server on stdio
pub async fn run_lsp_server() -> Result<()> {
    let config = Config::load().unwrap_or_default();

    // Create LLM provider
    let provider: Arc<dyn LlmProvider> =
        Arc::from(llm::create_provider(&config.llm.default_provider)?);

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    // tower-lsp 0.20 intercepts `exit` in its `Exit` middleware (cancelling
    // in-flight requests and closing the transport) without dispatching to the
    // backend, so retain a handle to invoke the explicit `handle_exit`
    // teardown below once the transport shuts down.
    let retained: Arc<std::sync::Mutex<Option<EngLspBackend>>> =
        Arc::new(std::sync::Mutex::new(None));
    let retain = retained.clone();
    let (service, socket) = LspService::new(|client| {
        let backend = EngLspBackend::new(client, provider, config);
        *retain.lock().unwrap_or_else(|e| e.into_inner()) = Some(backend.clone());
        backend
    });

    Server::new(stdin, stdout, socket).serve(service).await;

    // `serve` returns after the `exit` notification or transport EOF.
    match retained.lock().unwrap_or_else(|e| e.into_inner()).take() {
        Some(backend) => backend.handle_exit(),
        None => tracing::warn!("LSP backend handle unavailable; skipping explicit exit handling"),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workspace_folder(uri: &str) -> WorkspaceFolder {
        WorkspaceFolder {
            uri: Url::parse(uri).unwrap(),
            name: "root".to_string(),
        }
    }

    #[test]
    fn workspace_state_prefers_folders_over_root_uri() {
        let state = WorkspaceState::new(
            Some(Url::parse("file:///root").unwrap()),
            Some(vec![workspace_folder("file:///ws")]),
        );
        assert_eq!(state.roots(), vec![Url::parse("file:///ws").unwrap()]);
    }

    #[test]
    fn workspace_state_falls_back_to_root_uri() {
        let state = WorkspaceState::new(Some(Url::parse("file:///root").unwrap()), None);
        assert_eq!(state.roots(), vec![Url::parse("file:///root").unwrap()]);
    }

    #[test]
    fn workspace_containment_accepts_inside_rejects_outside() {
        let state = WorkspaceState::new(None, Some(vec![workspace_folder("file:///ws")]));
        assert!(state.contains(&Url::parse("file:///ws/src/main.rs").unwrap()));
        assert!(state.contains(&Url::parse("file:///ws").unwrap()));
        // Sibling-prefix paths share a string prefix but are not contained.
        assert!(!state.contains(&Url::parse("file:///ws-evil/main.rs").unwrap()));
        assert!(!state.contains(&Url::parse("file:///other/main.rs").unwrap()));
        // Different scheme/host fail closed.
        assert!(!state.contains(&Url::parse("https://ws/src/main.rs").unwrap()));
        // Non-hierarchical URIs cannot be proven inside a folder.
        assert!(!state.contains(&Url::parse("untitled:Untitled-1").unwrap()));
    }

    #[test]
    fn workspace_empty_scope_accepts_everything() {
        // With no advertised scope there is nothing to enforce against.
        let state = WorkspaceState::default();
        assert!(state.contains(&Url::parse("file:///anywhere/main.rs").unwrap()));
    }

    #[test]
    fn workspace_apply_change_adds_and_removes() {
        let mut state = WorkspaceState::new(None, Some(vec![workspace_folder("file:///a")]));
        state.apply_change(
            vec![workspace_folder("file:///b")],
            vec![workspace_folder("file:///a")],
        );
        assert_eq!(state.roots(), vec![Url::parse("file:///b").unwrap()]);
        assert!(!state.contains(&Url::parse("file:///a/x.rs").unwrap()));
        assert!(state.contains(&Url::parse("file:///b/x.rs").unwrap()));
    }

    #[test]
    fn shutdown_state_flag_defaults_false_and_sets() {
        let state = ShutdownState::default();
        assert!(!state.is_exited());
        state.set_exited();
        assert!(state.is_exited());
        // Clone shares the flag.
        assert!(state.clone().is_exited());
    }

    #[tokio::test]
    async fn diagnostics_newer_version_wins_and_older_publish_dropped() {
        let tracker = DiagnosticsTracker::default();
        let uri = Url::parse("file:///a.rs").unwrap();

        tracker.note_open(&uri, 1);
        assert!(tracker.should_publish(&uri, 1));

        tracker.note_version(&uri, 2);
        // Older version must never publish once a newer one was seen.
        assert!(!tracker.should_publish(&uri, 1));
        assert!(tracker.should_publish(&uri, 2));

        tracker.record_published(&uri, 2);
        assert_eq!(tracker.last_published_version(&uri), Some(2));
    }

    #[tokio::test]
    async fn diagnostics_tracking_supersedes_previous_task() {
        let tracker = DiagnosticsTracker::default();
        let uri = Url::parse("file:///a.rs").unwrap();

        let first = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        tracker.track_pending(uri.clone(), 1, first);
        assert!(tracker.is_tracked(&uri, 1));

        let second = tokio::spawn(async {});
        tracker.track_pending(uri.clone(), 2, second);
        // The first task was aborted by supersession.
        assert!(!tracker.is_tracked(&uri, 1));
        assert!(tracker.is_tracked(&uri, 2));

        // Per-document isolation: another document is unaffected.
        let other = Url::parse("file:///b.rs").unwrap();
        let other_handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        tracker.track_pending(other.clone(), 1, other_handle);
        assert!(tracker.is_tracked(&other, 1));
        tracker.cancel_pending(&uri);
        assert!(!tracker.is_tracked(&uri, 2));
        assert!(tracker.is_tracked(&other, 1));
        tracker.abort_all();
        assert!(!tracker.is_tracked(&other, 1));
    }

    #[tokio::test]
    async fn diagnostics_forget_cancels_and_blocks_stale_publish() {
        let tracker = DiagnosticsTracker::default();
        let uri = Url::parse("file:///a.rs").unwrap();

        tracker.note_open(&uri, 1);
        let handle = tokio::spawn(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
        });
        tracker.track_pending(uri.clone(), 1, handle);

        tracker.forget(&uri);
        // Pending work is gone and even the tracked version must not publish:
        // `didClose` already cleared the client's diagnostics.
        assert!(!tracker.is_tracked(&uri, 1));
        assert!(!tracker.should_publish(&uri, 1));

        // Re-opening resets the tombstone so fresh versions publish again.
        tracker.note_open(&uri, 1);
        assert!(tracker.should_publish(&uri, 1));
    }

    #[tokio::test]
    async fn diagnostics_abort_all_clears_every_document() {
        let tracker = DiagnosticsTracker::default();
        for name in ["a.rs", "b.rs"] {
            let uri = Url::parse(&format!("file:///{name}")).unwrap();
            tracker.track_pending(
                uri,
                1,
                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }),
            );
        }
        tracker.abort_all();
        assert!(!tracker.is_tracked(&Url::parse("file:///a.rs").unwrap(), 1));
        assert!(!tracker.is_tracked(&Url::parse("file:///b.rs").unwrap(), 1));
    }

    #[tokio::test]
    async fn diagnostics_abort_out_of_scope_only_aborts_outside() {
        let tracker = DiagnosticsTracker::default();
        let inside = Url::parse("file:///ws/in.rs").unwrap();
        let outside = Url::parse("file:///other/out.rs").unwrap();
        for uri in [&inside, &outside] {
            tracker.track_pending(
                uri.clone(),
                1,
                tokio::spawn(async {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }),
            );
        }
        let roots = vec![Url::parse("file:///ws").unwrap()];
        assert_eq!(tracker.abort_out_of_scope(&roots), 1);
        assert!(tracker.is_tracked(&inside, 1));
        assert!(!tracker.is_tracked(&outside, 1));
        tracker.abort_all();
    }
}
