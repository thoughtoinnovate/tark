//! HTTP server for completions and chat API

use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::config::Config;
use crate::llm::{self, LlmProvider};
use crate::storage::usage::UsageTracker;
use crate::storage::TarkStorage;
use crate::tools::{CodeAnalyzer, ToolRegistry};
use anyhow::Result;
use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

/// Get the global port file path (~/.local/share/tark/server.port)
fn get_port_file_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("tark")
        .join("server.port")
}

/// Write the server port to the global port file
fn write_port_file(port: u16) -> Result<()> {
    let port_file = get_port_file_path();
    if let Some(parent) = port_file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&port_file, port.to_string())?;
    tracing::info!("Wrote port {} to {:?}", port, port_file);
    Ok(())
}

/// Remove the port file on shutdown
fn remove_port_file() {
    let port_file = get_port_file_path();
    if port_file.exists() {
        if let Err(e) = std::fs::remove_file(&port_file) {
            tracing::warn!("Failed to remove port file: {}", e);
        } else {
            tracing::info!("Removed port file {:?}", port_file);
        }
    }
}

/// Find an available port starting from the preferred port
async fn find_available_port(host: &str, preferred: u16) -> Result<(tokio::net::TcpListener, u16)> {
    // Try the preferred port first
    let addr: SocketAddr = format!("{}:{}", host, preferred).parse()?;
    if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
        return Ok((listener, preferred));
    }

    // Try up to 100 subsequent ports
    for offset in 1..100 {
        let port = preferred + offset;
        let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
        if let Ok(listener) = tokio::net::TcpListener::bind(addr).await {
            tracing::warn!("Port {} was in use, using port {} instead", preferred, port);
            return Ok((listener, port));
        }
    }

    anyhow::bail!(
        "Could not find an available port in range {}-{}",
        preferred,
        preferred + 99
    )
}

/// Shared application state
struct AppState {
    current_provider: RwLock<String>,
    current_model: RwLock<String>,
    current_mode: RwLock<String>,
    window_style: RwLock<String>,
    window_position: RwLock<String>,
    current_cwd: RwLock<PathBuf>,
    chat_agent: RwLock<ChatAgent>,
    working_dir: PathBuf,
    config: Config,
    storage: Option<TarkStorage>,
    usage_tracker: Option<Arc<UsageTracker>>,
    session_id: String,
}

/// LSP context from Neovim plugin
#[derive(Debug, Clone, Deserialize, Default)]
struct LspContext {
    /// Language/filetype
    #[serde(default)]
    language: Option<String>,
    /// Diagnostics near cursor
    #[serde(default)]
    diagnostics: Vec<DiagnosticInfo>,
    /// Type info at cursor (from hover)
    #[serde(default)]
    cursor_type: Option<String>,
    /// Nearby symbols
    #[serde(default)]
    symbols: Vec<SymbolInfo>,
    /// Whether LSP is available
    #[serde(default)]
    has_lsp: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct DiagnosticInfo {
    line: usize,
    #[serde(default)]
    col: Option<usize>,
    message: String,
    #[serde(default)]
    severity: Option<i32>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SymbolInfo {
    name: String,
    kind: String,
    line: usize,
    #[serde(default)]
    detail: Option<String>,
}

/// Request for inline completion
#[derive(Debug, Deserialize)]
struct InlineCompleteRequest {
    file_path: String,
    file_content: String,
    cursor_line: usize,
    cursor_col: usize,
    #[serde(default)]
    provider: Option<String>,
    /// LSP context from Neovim plugin
    #[serde(default)]
    context: Option<LspContext>,
}

/// Response for inline completion
#[derive(Debug, Serialize)]
struct InlineCompleteResponse {
    completion: String,
    line_count: usize,
    /// Token usage statistics (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<crate::llm::TokenUsage>,
}

/// Request for chat
#[derive(Debug, Deserialize)]
struct ChatRequest {
    message: String,
    #[serde(default)]
    clear_history: bool,
    #[serde(default)]
    provider: Option<String>,
    /// Working directory for the chat (editor's cwd)
    #[serde(default)]
    cwd: Option<String>,
    /// Agent mode: plan (read-only), build (all tools), review (approval required)
    #[serde(default)]
    mode: Option<String>,
    /// LSP proxy port from Neovim plugin (dynamic port)
    #[serde(default)]
    lsp_proxy_port: Option<u16>,
}

/// Response for chat
/// Tool call log entry for verbose mode
#[derive(Debug, Serialize)]
struct ToolCallLogEntry {
    tool: String,
    args: serde_json::Value,
    result_preview: String,
}

#[derive(Debug, Serialize)]
struct ChatResponse {
    response: String,
    tool_calls_made: usize,
    tool_call_log: Vec<ToolCallLogEntry>,
    provider: String,
    mode: String,
    auto_compacted: bool,
    context_usage_percent: usize,
}

/// Health check response
#[derive(Debug, Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    current_provider: String,
}

/// Provider list response
#[derive(Debug, Serialize)]
struct ProvidersResponse {
    current: String,
    available: Vec<ProviderInfo>,
}

#[derive(Debug, Serialize)]
struct ProviderInfo {
    name: String,
    display_name: String,
    available: bool,
}

/// Set provider request
#[derive(Debug, Deserialize)]
struct SetProviderRequest {
    provider: String,
}

/// Agent status for real-time progress
#[derive(Debug, Clone, Serialize, Default)]
struct AgentStatus {
    active: bool,
    current_action: String,
    tool_name: Option<String>,
    tool_arg: Option<String>,
    step: usize,
    total_steps: usize,
}

/// Global agent status (shared across requests)
static AGENT_STATUS: std::sync::OnceLock<tokio::sync::RwLock<AgentStatus>> =
    std::sync::OnceLock::new();

fn get_agent_status() -> &'static tokio::sync::RwLock<AgentStatus> {
    AGENT_STATUS.get_or_init(|| tokio::sync::RwLock::new(AgentStatus::default()))
}

/// Update the global agent status
pub async fn update_status(action: &str, tool: Option<&str>, arg: Option<&str>, step: usize) {
    let mut status = get_agent_status().write().await;
    status.active = true;
    status.current_action = action.to_string();
    status.tool_name = tool.map(|s| s.to_string());
    status.tool_arg = arg.map(|s| {
        if s.len() > 50 {
            format!("{}...", &s[..47])
        } else {
            s.to_string()
        }
    });
    status.step = step;
}

async fn clear_status() {
    let mut status = get_agent_status().write().await;
    status.active = false;
    status.current_action = String::new();
    status.tool_name = None;
    status.tool_arg = None;
    status.step = 0;
}

/// Endpoint to get current agent status (for polling)
async fn get_status() -> impl IntoResponse {
    let status = get_agent_status().read().await;
    Json(status.clone())
}

/// Run the HTTP server
pub async fn run_http_server(host: &str, port: u16, working_dir: PathBuf) -> Result<()> {
    let config = Config::load().unwrap_or_default();
    let working_dir = working_dir.canonicalize().unwrap_or(working_dir);
    tracing::info!("Server working directory: {:?}", working_dir);

    // Initialize storage
    let storage = TarkStorage::new(working_dir.clone()).ok();

    // Initialize usage tracker and create session
    let (usage_tracker, session_id) = if let Some(ref storage) = storage {
        match UsageTracker::new(storage.project_root()) {
            Ok(tracker) => {
                let host = whoami::fallible::hostname().unwrap_or_else(|_| "unknown".to_string());
                let username = whoami::username();

                // Create session
                let session = tracker.create_session(&host, &username);
                let session_id = session
                    .map(|s| s.id)
                    .unwrap_or_else(|_| uuid::Uuid::new_v4().to_string());

                // Fetch pricing in background
                let tracker_clone = Arc::new(tracker);
                let tracker_for_fetch = Arc::clone(&tracker_clone);
                tokio::spawn(async move {
                    if let Err(e) = tracker_for_fetch.fetch_pricing().await {
                        tracing::warn!("Failed to fetch pricing from models.dev: {}", e);
                    }
                });

                (Some(tracker_clone), session_id)
            }
            Err(e) => {
                tracing::warn!("Failed to initialize usage tracker: {}", e);
                (None, uuid::Uuid::new_v4().to_string())
            }
        }
    } else {
        (None, uuid::Uuid::new_v4().to_string())
    };

    // Load saved session or use defaults
    let (default_provider, default_model) = if let Some(ref s) = storage {
        let saved_config = s.load_config().unwrap_or_default();
        let provider = if saved_config.provider != "openai" {
            saved_config.provider
        } else {
            config.llm.default_provider.clone()
        };
        let model = saved_config.model.unwrap_or_else(|| "gpt-4o".to_string());
        (provider, model)
    } else {
        (config.llm.default_provider.clone(), "gpt-4o".to_string())
    };

    // Create initial chat agent
    let provider: Arc<dyn LlmProvider> = Arc::from(llm::create_provider(&default_provider)?);
    let tools = ToolRegistry::with_defaults(working_dir.clone(), config.tools.shell_enabled);
    let chat_agent =
        ChatAgent::new(provider, tools).with_max_iterations(config.agent.max_iterations);

    let state = Arc::new(AppState {
        current_provider: RwLock::new(default_provider),
        current_model: RwLock::new(default_model),
        current_mode: RwLock::new("build".to_string()),
        window_style: RwLock::new("split".to_string()),
        window_position: RwLock::new("right".to_string()),
        current_cwd: RwLock::new(working_dir.clone()),
        chat_agent: RwLock::new(chat_agent),
        working_dir,
        config,
        storage,
        usage_tracker,
        session_id,
    });

    // Build router
    let app = Router::new()
        .route("/health", get(health_check))
        .route("/providers", get(list_providers))
        .route("/provider", post(set_provider))
        .route("/complete", post(handle_completion))
        .route("/inline-complete", post(handle_inline_completion))
        .route("/chat", post(handle_chat))
        .route("/chat/status", get(get_status))
        .route("/session", get(get_session))
        .route("/session/save", post(save_session))
        // Usage dashboard and API
        .route("/usage", get(usage_dashboard))
        .route("/api/usage/summary", get(usage_summary))
        .route("/api/usage/models", get(usage_by_model))
        .route("/api/usage/modes", get(usage_by_mode))
        .route("/api/usage/sessions", get(usage_sessions))
        .route("/api/usage/storage", get(usage_storage))
        .route("/api/usage/cleanup", post(usage_cleanup))
        .route("/api/usage/export", get(usage_export))
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    // Find an available port (auto-selects if preferred port is in use)
    let (listener, actual_port) = find_available_port(host, port).await?;

    // Write the port to the global port file for clients to discover
    if let Err(e) = write_port_file(actual_port) {
        tracing::warn!("Failed to write port file: {}", e);
    }

    let addr = listener.local_addr()?;
    tracing::info!("HTTP server listening on {}", addr);

    // Set up graceful shutdown to clean up port file
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        tracing::info!("Shutdown signal received");
        remove_port_file();
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await?;

    Ok(())
}

async fn health_check(State(state): State<Arc<AppState>>) -> Json<HealthResponse> {
    let current = state.current_provider.read().await.clone();
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        current_provider: current,
    })
}

async fn list_providers(State(state): State<Arc<AppState>>) -> Json<ProvidersResponse> {
    let current = state.current_provider.read().await.clone();

    // Check which providers are available (have API keys set)
    let ollama_available = std::env::var("OLLAMA_MODEL").is_ok()
        || reqwest::get("http://localhost:11434/api/tags")
            .await
            .is_ok();
    let claude_available = std::env::var("ANTHROPIC_API_KEY").is_ok();
    let openai_available = std::env::var("OPENAI_API_KEY").is_ok();

    Json(ProvidersResponse {
        current,
        available: vec![
            ProviderInfo {
                name: "ollama".to_string(),
                display_name: "Ollama (Local)".to_string(),
                available: ollama_available,
            },
            ProviderInfo {
                name: "claude".to_string(),
                display_name: "Claude (Anthropic)".to_string(),
                available: claude_available,
            },
            ProviderInfo {
                name: "openai".to_string(),
                display_name: "OpenAI (GPT-4)".to_string(),
                available: openai_available,
            },
        ],
    })
}

async fn set_provider(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SetProviderRequest>,
) -> impl IntoResponse {
    let provider_name = req.provider.to_lowercase();

    // Validate provider name
    if !["ollama", "claude", "openai", "local", "anthropic", "gpt"]
        .contains(&provider_name.as_str())
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "Invalid provider. Use: ollama, claude, or openai"
            })),
        )
            .into_response();
    }

    // Create new provider
    let provider = match llm::create_provider(&provider_name) {
        Ok(p) => Arc::from(p),
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    // Create new chat agent with new provider
    let tools =
        ToolRegistry::with_defaults(state.working_dir.clone(), state.config.tools.shell_enabled);
    let new_agent =
        ChatAgent::new(provider, tools).with_max_iterations(state.config.agent.max_iterations);

    // Update state
    {
        let mut current = state.current_provider.write().await;
        *current = provider_name.clone();
    }
    {
        let mut agent = state.chat_agent.write().await;
        *agent = new_agent;
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "success": true,
            "provider": provider_name
        })),
    )
        .into_response()
}

async fn handle_completion(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CompletionRequest>,
) -> impl IntoResponse {
    // Get current provider for completion
    let provider_name = state.current_provider.read().await.clone();
    let provider: Arc<dyn LlmProvider> = match llm::create_provider(&provider_name) {
        Ok(p) => Arc::from(p),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let engine = CompletionEngine::new(provider)
        .with_cache_size(state.config.completion.cache_size)
        .with_context_lines(
            state.config.completion.context_lines_before,
            state.config.completion.context_lines_after,
        );

    match engine.complete(&req).await {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(e) => {
            tracing::error!("Completion error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

async fn handle_inline_completion(
    State(state): State<Arc<AppState>>,
    Json(req): Json<InlineCompleteRequest>,
) -> impl IntoResponse {
    // Use request provider or current provider
    let current = state.current_provider.read().await.clone();
    let provider_name = req.provider.unwrap_or(current);

    let provider: Arc<dyn LlmProvider> = match llm::create_provider(&provider_name) {
        Ok(p) => Arc::from(p),
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response();
        }
    };

    let engine = CompletionEngine::new(provider)
        .with_cache_size(state.config.completion.cache_size)
        .with_context_lines(
            state.config.completion.context_lines_before,
            state.config.completion.context_lines_after,
        );

    // Convert HTTP LSP context to completion engine LSP context
    let mut lsp_context = req.context.map(|ctx| crate::completion::LspContext {
        language: ctx.language,
        diagnostics: ctx
            .diagnostics
            .into_iter()
            .map(|d| crate::completion::DiagnosticInfo {
                line: d.line,
                col: d.col,
                message: d.message,
                severity: d.severity,
                source: d.source,
            })
            .collect(),
        cursor_type: ctx.cursor_type,
        symbols: ctx
            .symbols
            .into_iter()
            .map(|s| crate::completion::SymbolInfo {
                name: s.name,
                kind: s.kind,
                line: s.line,
                detail: s.detail,
            })
            .collect(),
        has_lsp: ctx.has_lsp,
    });

    // Tree-sitter fallback: if no LSP and no symbols, try to extract symbols locally
    if lsp_context
        .as_ref()
        .is_none_or(|ctx| !ctx.has_lsp && ctx.symbols.is_empty())
    {
        let analyzer = CodeAnalyzer::new(state.working_dir.clone());
        let file_path = PathBuf::from(&req.file_path);
        if let Ok(symbols) = analyzer.extract_symbols_from_str(&file_path, &req.file_content) {
            if !symbols.is_empty() {
                let mapped: Vec<_> = symbols
                    .into_iter()
                    .map(|s| crate::completion::SymbolInfo {
                        name: s.name,
                        kind: s.kind.to_string(),
                        line: s.line,
                        detail: s.signature.or(s.doc_comment).filter(|d| !d.is_empty()),
                    })
                    .collect();

                lsp_context = Some(crate::completion::LspContext {
                    language: None,
                    diagnostics: vec![],
                    cursor_type: None,
                    symbols: mapped,
                    has_lsp: false,
                });
            }
        }
    }

    let completion_req = CompletionRequest {
        file_path: PathBuf::from(&req.file_path),
        file_content: req.file_content,
        cursor_line: req.cursor_line,
        cursor_col: req.cursor_col,
        related_files: vec![],
        lsp_context,
    };

    match engine.complete(&completion_req).await {
        Ok(response) => {
            // Log usage if tracker is available
            if let (Some(ref tracker), Some(ref usage)) = (&state.usage_tracker, &response.usage) {
                let provider = state.current_provider.read().await.clone();
                let model = state.current_model.read().await.clone();

                // Calculate cost
                let cost = tracker
                    .calculate_cost(&provider, &model, usage.input_tokens, usage.output_tokens)
                    .await;

                // Log usage in background
                let tracker_clone = Arc::clone(tracker);
                let session_id = state.session_id.clone();
                let usage_clone = usage.clone();
                tokio::spawn(async move {
                    if let Err(e) = tracker_clone.log_usage(crate::storage::usage::UsageLog {
                        session_id,
                        provider,
                        model,
                        mode: "completion".to_string(),
                        input_tokens: usage_clone.input_tokens,
                        output_tokens: usage_clone.output_tokens,
                        cost_usd: cost,
                        request_type: "fim".to_string(),
                        estimated: false,
                    }) {
                        tracing::error!("Failed to log completion usage: {}", e);
                    }
                });
            }

            (
                StatusCode::OK,
                Json(InlineCompleteResponse {
                    completion: response.completion,
                    line_count: response.line_count,
                    usage: response.usage,
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Inline completion error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

async fn handle_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatRequest>,
) -> impl IntoResponse {
    use crate::tools::AgentMode;

    // Set LSP proxy port for tools to use (if provided by Neovim plugin)
    crate::tools::set_lsp_proxy_port(req.lsp_proxy_port);

    // Determine working directory: use request cwd if provided, otherwise server's working dir
    let working_dir = req
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| state.working_dir.clone());

    // Parse agent mode
    let mode_str = req
        .mode
        .as_ref()
        .map(|m| m.to_lowercase())
        .unwrap_or_else(|| "build".to_string());

    let mode = match mode_str.as_str() {
        "plan" => AgentMode::Plan,
        "review" => AgentMode::Review,
        _ => AgentMode::Build,
    };

    // Check what has actually changed
    let current_mode = state.current_mode.read().await.clone();
    let current_cwd = state.current_cwd.read().await.clone();
    let current_provider = state.current_provider.read().await.clone();

    let mode_changed = mode_str != current_mode;
    let cwd_changed = working_dir != current_cwd;
    let provider_changed = req
        .provider
        .as_ref()
        .map(|p| p != &current_provider)
        .unwrap_or(false);

    tracing::debug!(
        "Chat request - mode: {}, cwd_changed: {}, provider_changed: {}, mode_changed: {}",
        mode_str,
        cwd_changed,
        provider_changed,
        mode_changed
    );

    // Only create a completely NEW agent if working directory changes (need fresh file context)
    // For mode/provider changes, update the existing agent to PRESERVE conversation history
    if cwd_changed {
        // CWD changed - need a fresh agent with new file context
        let provider_name = req
            .provider
            .clone()
            .unwrap_or_else(|| current_provider.clone());

        let provider = match llm::create_provider(&provider_name) {
            Ok(p) => Arc::from(p),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({ "error": e.to_string() })),
                )
                    .into_response();
            }
        };

        let tools =
            ToolRegistry::for_mode(working_dir.clone(), mode, state.config.tools.shell_enabled);
        tracing::info!("Creating new agent for new cwd: {:?}", working_dir);

        let new_agent = ChatAgent::with_mode(provider, tools, mode)
            .with_max_iterations(state.config.agent.max_iterations);

        // Update all stored state
        {
            let mut current = state.current_cwd.write().await;
            *current = working_dir.clone();
        }
        if provider_changed {
            let mut current = state.current_provider.write().await;
            *current = provider_name;
        }
        {
            let mut current = state.current_mode.write().await;
            *current = mode_str.clone();
        }
        {
            let mut agent = state.chat_agent.write().await;
            *agent = new_agent;
        }
    } else {
        // CWD same - update existing agent to preserve conversation history
        let mut agent = state.chat_agent.write().await;

        // Update mode if changed (preserves conversation)
        if mode_changed {
            let tools =
                ToolRegistry::for_mode(working_dir.clone(), mode, state.config.tools.shell_enabled);
            agent.update_mode(tools, mode);
            tracing::info!(
                "Updated agent mode to: {} (conversation preserved)",
                mode_str
            );

            let mut current = state.current_mode.write().await;
            *current = mode_str.clone();
        }

        // Update provider if changed (preserves conversation)
        // Safety: provider_changed is only true when req.provider.is_some()
        if let Some(provider_name) = req.provider.clone().filter(|_| provider_changed) {
            let provider = match llm::create_provider(&provider_name) {
                Ok(p) => Arc::from(p),
                Err(e) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(serde_json::json!({ "error": e.to_string() })),
                    )
                        .into_response();
                }
            };
            agent.update_provider(provider);
            tracing::info!(
                "Updated agent provider to: {} (conversation preserved)",
                provider_name
            );

            let mut current = state.current_provider.write().await;
            *current = provider_name;
        }

        // Clear history if requested
        if req.clear_history {
            agent.reset();
        }
    }

    // Get final state and process the chat
    let final_provider = state.current_provider.read().await.clone();
    let mut agent = state.chat_agent.write().await;

    // Clear history if requested (for cwd_changed case where agent was replaced)
    if cwd_changed && req.clear_history {
        agent.reset();
    }

    // Set status to thinking
    update_status("Thinking...", None, None, 0).await;

    let result = agent.chat(&req.message).await;

    // Clear status when done
    clear_status().await;

    match result {
        Ok(response) => {
            // Log usage if tracker is available
            if let (Some(ref tracker), Some(ref usage)) = (&state.usage_tracker, &response.usage) {
                let provider = state.current_provider.read().await.clone();
                let model = state.current_model.read().await.clone();
                let mode = state.current_mode.read().await.clone();

                // Calculate cost
                let cost = tracker
                    .calculate_cost(&provider, &model, usage.input_tokens, usage.output_tokens)
                    .await;

                // Log usage in background
                let tracker_clone = Arc::clone(tracker);
                let session_id = state.session_id.clone();
                let usage_clone = usage.clone();
                tokio::spawn(async move {
                    if let Err(e) = tracker_clone.log_usage(crate::storage::usage::UsageLog {
                        session_id,
                        provider,
                        model,
                        mode,
                        input_tokens: usage_clone.input_tokens,
                        output_tokens: usage_clone.output_tokens,
                        cost_usd: cost,
                        request_type: "chat".to_string(),
                        estimated: false,
                    }) {
                        tracing::error!("Failed to log usage: {}", e);
                    }
                });
            }

            (
                StatusCode::OK,
                Json(ChatResponse {
                    response: response.text,
                    tool_calls_made: response.tool_calls_made,
                    tool_call_log: response
                        .tool_call_log
                        .into_iter()
                        .map(|l| ToolCallLogEntry {
                            tool: l.tool,
                            args: l.args,
                            result_preview: l.result_preview,
                        })
                        .collect(),
                    provider: final_provider,
                    mode: mode_str,
                    auto_compacted: response.auto_compacted,
                    context_usage_percent: response.context_usage_percent,
                }),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Chat error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// Session response
#[derive(Debug, Serialize)]
struct SessionResponse {
    provider: String,
    model: String,
    mode: String,
    style: String,
    position: String,
}

/// Get current session state
async fn get_session(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let provider = state.current_provider.read().await.clone();
    let model = state.current_model.read().await.clone();
    let mode = state.current_mode.read().await.clone();
    let style = state.window_style.read().await.clone();
    let position = state.window_position.read().await.clone();

    Json(SessionResponse {
        provider,
        model,
        mode,
        style,
        position,
    })
}

/// Save session request
#[derive(Debug, Deserialize)]
struct SaveSessionRequest {
    provider: Option<String>,
    model: Option<String>,
    mode: Option<String>,
    style: Option<String>,
    position: Option<String>,
}

/// Save current session to .tark
async fn save_session(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SaveSessionRequest>,
) -> impl IntoResponse {
    // Update in-memory state
    if let Some(provider) = &req.provider {
        let mut current = state.current_provider.write().await;
        *current = provider.clone();
    }
    if let Some(model) = &req.model {
        let mut current = state.current_model.write().await;
        *current = model.clone();
    }
    if let Some(mode) = &req.mode {
        let mut current = state.current_mode.write().await;
        *current = mode.clone();
    }
    if let Some(style) = &req.style {
        let mut current = state.window_style.write().await;
        *current = style.clone();
    }
    if let Some(position) = &req.position {
        let mut current = state.window_position.write().await;
        *current = position.clone();
    }

    // Save to storage
    if let Some(ref storage) = state.storage {
        let provider = state.current_provider.read().await.clone();
        let model = state.current_model.read().await.clone();

        let config = crate::storage::WorkspaceConfig {
            provider,
            model: Some(model),
            ..Default::default()
        };

        if let Err(e) = storage.save_config(&config) {
            tracing::warn!("Failed to save session: {}", e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to save: {}", e) })),
            )
                .into_response();
        }
    }

    (StatusCode::OK, Json(serde_json::json!({ "success": true }))).into_response()
}

// ========== Usage Dashboard Endpoints ==========

/// Serve the HTML usage dashboard
async fn usage_dashboard() -> impl IntoResponse {
    use axum::response::Html;
    Html(crate::transport::dashboard::DASHBOARD_HTML)
}

/// Get usage summary
async fn usage_summary(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.get_summary() {
            Ok(summary) => (StatusCode::OK, Json(summary)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Get usage by model
async fn usage_by_model(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.get_usage_by_model() {
            Ok(models) => (StatusCode::OK, Json(models)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Get usage by mode
async fn usage_by_mode(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.get_usage_by_mode() {
            Ok(modes) => (StatusCode::OK, Json(modes)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Get sessions with stats
async fn usage_sessions(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.get_sessions() {
            Ok(sessions) => (StatusCode::OK, Json(sessions)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Get storage size
async fn usage_storage(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.get_summary() {
            Ok(summary) => (
                StatusCode::OK,
                Json(serde_json::json!({
                    "size_bytes": summary.db_size_bytes,
                    "size_human": summary.db_size_human,
                })),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Cleanup old usage logs
async fn usage_cleanup(
    State(state): State<Arc<AppState>>,
    Json(req): Json<crate::storage::usage::CleanupRequest>,
) -> impl IntoResponse {
    match &state.usage_tracker {
        Some(tracker) => match tracker.cleanup(req).await {
            Ok(response) => (StatusCode::OK, Json(response)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Usage tracking not initialized" })),
        )
            .into_response(),
    }
}

/// Export usage data as CSV
async fn usage_export(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    use axum::body::Body;
    use axum::http::header;
    use axum::response::Response;

    match &state.usage_tracker {
        Some(tracker) => {
            // Get all data
            let models = tracker.get_usage_by_model().ok().unwrap_or_default();
            let sessions = tracker.get_sessions().ok().unwrap_or_default();

            // Build CSV
            let mut csv = String::from("Type,Provider,Model,Host,Username,Session,Tokens,Cost\n");

            // Add model data
            for m in models {
                csv.push_str(&format!(
                    "model,{},{},,,{},{}\n",
                    m.provider,
                    m.model,
                    m.input_tokens + m.output_tokens,
                    m.cost
                ));
            }

            // Add session data
            for s in sessions {
                csv.push_str(&format!(
                    "session,,,{},{},{},{},{}\n",
                    s.host, s.username, s.id, s.total_tokens, s.total_cost
                ));
            }

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/csv")
                .header(
                    header::CONTENT_DISPOSITION,
                    "attachment; filename=\"tark-usage.csv\"",
                )
                .body(Body::from(csv))
                .unwrap()
        }
        None => Response::builder()
            .status(StatusCode::SERVICE_UNAVAILABLE)
            .body(Body::from("Usage tracking not initialized"))
            .unwrap(),
    }
}
