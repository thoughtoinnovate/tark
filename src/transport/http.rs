//! HTTP server for completions and chat API

use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::config::Config;
use crate::llm::{self, LlmProvider};
use crate::storage::TarkStorage;
use crate::tools::ToolRegistry;
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

/// Shared application state
struct AppState {
    current_provider: RwLock<String>,
    current_model: RwLock<String>,
    current_mode: RwLock<String>,
    current_cwd: RwLock<PathBuf>,
    chat_agent: RwLock<ChatAgent>,
    working_dir: PathBuf,
    config: Config,
    storage: Option<TarkStorage>,
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
        current_cwd: RwLock::new(working_dir.clone()),
        chat_agent: RwLock::new(chat_agent),
        working_dir,
        config,
        storage,
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
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", host, port).parse()?;
    tracing::info!("HTTP server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

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
    let lsp_context = req.context.map(|ctx| crate::completion::LspContext {
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

    let completion_req = CompletionRequest {
        file_path: PathBuf::from(&req.file_path),
        file_content: req.file_content,
        cursor_line: req.cursor_line,
        cursor_col: req.cursor_col,
        related_files: vec![],
        lsp_context,
    };

    match engine.complete(&completion_req).await {
        Ok(response) => (
            StatusCode::OK,
            Json(InlineCompleteResponse {
                completion: response.completion,
                line_count: response.line_count,
                usage: response.usage,
            }),
        )
            .into_response(),
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
        Ok(response) => (
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
            .into_response(),
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
}

/// Get current session state
async fn get_session(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let provider = state.current_provider.read().await.clone();
    let model = state.current_model.read().await.clone();
    let mode = state.current_mode.read().await.clone();

    Json(SessionResponse {
        provider,
        model,
        mode,
    })
}

/// Save session request
#[derive(Debug, Deserialize)]
struct SaveSessionRequest {
    provider: Option<String>,
    model: Option<String>,
    mode: Option<String>,
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
