use crate::agent::ChatAgent;
use crate::completion::{CompletionEngine, CompletionRequest};
use crate::llm;
use crate::storage::TarkStorage;
use crate::tools::questionnaire::{interaction_channel, InteractionRequest};
use crate::tools::{AgentMode, ToolRegistry};
use crate::transport::acp::errors::*;
use crate::transport::acp::framing;
use crate::transport::acp::protocol::*;
use crate::transport::acp::session::{AcpSession, ActiveRequestGuard, SessionContext};
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::io::BufReader;
use tokio::sync::{oneshot, Mutex};

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_TEXT_BYTES: usize = 128 * 1024;
const MAX_BUFFERS: usize = 64;
const MAX_SESSIONS: usize = 8;
const MAX_REQUESTS_PER_MINUTE: usize = 30;
const APPROVAL_TIMEOUT_SECS: u64 = 120;

struct SessionBootstrap {
    mode: AgentMode,
    provider: String,
    model: Option<String>,
}

pub struct AcpServer {
    working_dir: PathBuf,
    shell_enabled: bool,
    tool_timeout_secs: u64,
    max_iterations: usize,
    thinking_config: crate::config::ThinkingConfig,
    default_think_level: String,
    next_session: AtomicU64,
    next_outbound_id: AtomicU64,
    sessions: Mutex<HashMap<String, Arc<AcpSession>>>,
    outbound_pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, JsonRpcError>>>>,
    writer: Arc<Mutex<tokio::io::Stdout>>,
}

impl AcpServer {
    pub fn new(working_dir: PathBuf, config: &crate::config::Config) -> Self {
        Self {
            working_dir,
            shell_enabled: config.tools.shell_enabled,
            tool_timeout_secs: config.tools.tool_timeout_secs,
            max_iterations: config.agent.max_iterations,
            thinking_config: config.thinking.clone(),
            default_think_level: config.thinking.effective_default_level_name(),
            next_session: AtomicU64::new(1),
            next_outbound_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
            outbound_pending: Mutex::new(HashMap::new()),
            writer: Arc::new(Mutex::new(tokio::io::stdout())),
        }
    }

    async fn send_response(&self, id: Value, result: Value) -> Result<()> {
        self.send_json(&JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        })
        .await
    }

    async fn send_error(&self, id: Value, code: i32, message: impl Into<String>) -> Result<()> {
        self.send_json(&JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        })
        .await
    }

    async fn send_error_with_data(
        &self,
        id: Value,
        code: i32,
        message: impl Into<String>,
        data: Value,
    ) -> Result<()> {
        self.send_json(&JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: Some(data),
            }),
        })
        .await
    }

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        self.send_value(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn send_request(&self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        let id = self.next_outbound_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        self.outbound_pending.lock().await.insert(id, tx);

        self.send_value(json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;

        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(err))) => anyhow::bail!("request failed: {} ({})", err.message, err.code),
            Ok(Err(_)) => anyhow::bail!("request cancelled"),
            Err(_) => {
                self.outbound_pending.lock().await.remove(&id);
                anyhow::bail!("request timed out")
            }
        }
    }

    async fn handle_response(&self, resp: JsonRpcResponseEnvelope) {
        let Some(id_u64) = resp.id.as_u64() else {
            return;
        };
        if let Some(tx) = self.outbound_pending.lock().await.remove(&id_u64) {
            let payload = if let Some(err) = resp.error {
                Err(err)
            } else {
                Ok(resp.result.unwrap_or(Value::Null))
            };
            let _ = tx.send(payload);
        }
    }

    async fn send_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let payload = serde_json::to_vec(value)?;
        let mut writer = self.writer.lock().await;
        framing::write_frame(&mut writer, &payload).await
    }

    async fn send_value(&self, value: Value) -> Result<()> {
        let payload = serde_json::to_vec(&value)?;
        let mut writer = self.writer.lock().await;
        framing::write_frame(&mut writer, &payload).await
    }

    async fn get_session(&self, session_id: &str) -> Option<Arc<AcpSession>> {
        self.sessions.lock().await.get(session_id).cloned()
    }

    fn validate_payload(method: &str, params: &Value) -> Result<()> {
        match method {
            "session/prompt" => {
                if let Some(prompt) = params.get("prompt").and_then(|v| v.as_array()) {
                    let prompt_text = prompt_to_text(
                        &prompt
                            .iter()
                            .filter_map(|v| serde_json::from_value::<ContentBlock>(v.clone()).ok())
                            .collect::<Vec<_>>(),
                    );
                    if prompt_text.len() > MAX_MESSAGE_BYTES {
                        anyhow::bail!("prompt exceeds max size");
                    }
                }
            }
            "context/update" => {
                if let Some(text) = params
                    .get("selection")
                    .and_then(|s| s.get("text"))
                    .and_then(|v| v.as_str())
                {
                    if text.len() > MAX_CONTEXT_TEXT_BYTES {
                        anyhow::bail!("selection text exceeds max size");
                    }
                }
                if let Some(text) = params.get("active_excerpt").and_then(|v| v.as_str()) {
                    if text.len() > MAX_CONTEXT_TEXT_BYTES {
                        anyhow::bail!("active_excerpt exceeds max size");
                    }
                }
                if params
                    .get("buffers")
                    .and_then(|b| b.as_array())
                    .map(|v| v.len() > MAX_BUFFERS)
                    .unwrap_or(false)
                {
                    anyhow::bail!("buffers exceeds max size");
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn handle_initialize(&self, req: JsonRpcRequest) -> Result<()> {
        let Some(req_id) = req.id else {
            anyhow::bail!("initialize requires request id");
        };
        let params: InitializeParams = match serde_json::from_value(req.params) {
            Ok(v) => v,
            Err(err) => {
                return self
                    .send_error_with_data(
                        req_id,
                        JSONRPC_INVALID_PARAMS,
                        "Invalid params for initialize",
                        error_data(
                            "invalid_params",
                            format!(
                                "required fields: protocolVersion, clientCapabilities, clientInfo ({})",
                                err
                            ),
                        ),
                    )
                    .await;
            }
        };

        if params.protocol_version != ACP_PROTOCOL_VERSION {
            return self
                .send_error_with_data(
                    req_id,
                    ACP_UNSUPPORTED_VERSION,
                    format!(
                        "Unsupported ACP protocolVersion. Server supports '{}'",
                        ACP_PROTOCOL_VERSION
                    ),
                    error_data(
                        "unsupported_version",
                        format!("supported={}", ACP_PROTOCOL_VERSION),
                    ),
                )
                .await;
        }

        tracing::info!(
            "ACP initialize from client '{}' version '{}'",
            params.client_info.name,
            params.client_info.version
        );

        self.send_response(
            req_id,
            json!({
                "protocolVersion": ACP_PROTOCOL_VERSION,
                "agentInfo": {
                    "name": "tark",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "agentCapabilities": {
                    "loadSession": true,
                    "promptCapabilities": {
                        "image": false,
                        "audio": false,
                        "embeddedContext": true
                    },
                    "mcpCapabilities": {
                        "http": false,
                        "sse": false
                    },
                    "sessionCapabilities": {}
                }
                ,
                "authMethods": []
            }),
        )
        .await
    }

    async fn spawn_interaction_worker(
        self: Arc<Self>,
        session: Arc<AcpSession>,
        mut rx: crate::tools::questionnaire::InteractionReceiver,
    ) {
        while let Some(interaction) = rx.recv().await {
            let request_id = session
                .current_request
                .lock()
                .await
                .clone()
                .unwrap_or_else(|| "req-unknown".to_string());

            match interaction {
                InteractionRequest::Approval { request, responder } => {
                    let options = permission_options_for_request(&request);
                    let tool_call_id = format!(
                        "toolcall-{}",
                        self.next_outbound_id.fetch_add(1, Ordering::SeqCst)
                    );
                    let req = json!({
                        "sessionId": session.id,
                        "toolCall": {
                            "toolCallId": tool_call_id,
                            "title": format!("{} {}", request.tool, request.command),
                            "status": "pending",
                            "kind": "execute",
                            "rawInput": request.command,
                        },
                        "options": options,
                        "_meta": {
                            "tark": {
                                "requestId": request_id,
                                "tool": request.tool,
                            }
                        }
                    });
                    let result = self
                        .send_request(
                            "session/request_permission",
                            req,
                            Duration::from_secs(APPROVAL_TIMEOUT_SECS),
                        )
                        .await;
                    let response = map_permission_response(result, &request);
                    let _ = responder.send(response);
                }
                InteractionRequest::Questionnaire { data, responder } => {
                    tracing::info!(
                        "Questionnaire interaction requested for request {} ('{}') but ACP strict cutover currently supports permission requests only",
                        request_id,
                        data.title
                    );
                    let _ = responder.send(crate::tools::questionnaire::UserResponse::cancelled());
                }
            }
        }
    }

    async fn handle_session_new(self: Arc<Self>, req: JsonRpcRequest) -> Result<()> {
        let Some(req_id) = req.id else {
            anyhow::bail!("session/new requires request id");
        };
        let params: SessionNewParams =
            serde_json::from_value(req.params).context("Invalid params for session/new")?;

        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= MAX_SESSIONS {
            drop(sessions);
            return self
                .send_error_with_data(
                    req_id,
                    ACP_RATE_LIMITED,
                    "Too many ACP sessions",
                    error_data("rate_limited", "max sessions reached"),
                )
                .await;
        }

        let cwd = resolve_cwd(Some(params.cwd.clone()), &self.working_dir)?;
        let bootstrap = session_bootstrap(&cwd, None);
        let mode = bootstrap.mode;
        let provider = bootstrap.provider.clone();
        let model = bootstrap.model.clone();

        let provider_impl = llm::create_provider_with_options(&provider, true, model.as_deref())
            .with_context(|| format!("Failed to create provider '{}'", provider))?;

        let (interaction_tx, interaction_rx) = interaction_channel();
        let mut tools = ToolRegistry::for_mode_with_interaction(
            cwd.clone(),
            mode,
            self.shell_enabled,
            Some(interaction_tx.clone()),
        );
        tools.set_tool_timeout_secs(self.tool_timeout_secs);

        let mut agent = ChatAgent::with_mode(Arc::from(provider_impl), tools, mode)
            .with_max_iterations(self.max_iterations);
        agent.set_thinking_config(self.thinking_config.clone());
        agent.set_think_level_sync(self.default_think_level.clone());

        let id_num = self.next_session.fetch_add(1, Ordering::SeqCst);
        let session_id = format!("acp-{}", id_num);
        let session = Arc::new(AcpSession {
            id: session_id.clone(),
            cwd,
            provider: provider.clone(),
            model: model.clone(),
            agent: Arc::new(Mutex::new(agent)),
            context: Arc::new(Mutex::new(SessionContext::default())),
            current_request: Arc::new(Mutex::new(None)),
            interrupt: Arc::new(AtomicBool::new(false)),
            interaction_tx,
            request_times: Arc::new(Mutex::new(std::collections::VecDeque::new())),
        });

        sessions.insert(session_id.clone(), Arc::clone(&session));
        drop(sessions);

        tokio::spawn(
            Arc::clone(&self).spawn_interaction_worker(Arc::clone(&session), interaction_rx),
        );

        self.send_response(
            req_id,
            json!({
                "sessionId": session_id,
                "modes": {
                    "currentModeId": mode_to_str(mode),
                    "availableModes": [
                        { "id": "ask", "name": "Ask", "description": "Q&A mode" },
                        { "id": "plan", "name": "Plan", "description": "Planning mode" },
                        { "id": "build", "name": "Build", "description": "Implementation mode" }
                    ]
                },
                "configOptions": [
                    {
                        "id": "provider",
                        "name": "Provider",
                        "type": "select",
                        "currentValue": provider,
                        "options": [
                            { "value": "openai", "name": "OpenAI" },
                            { "value": "claude", "name": "Claude" },
                            { "value": "gemini", "name": "Gemini" },
                            { "value": "copilot", "name": "Copilot" },
                            { "value": "openrouter", "name": "OpenRouter" },
                            { "value": "ollama", "name": "Ollama" },
                            { "value": "tark_sim", "name": "Tark Sim" }
                        ]
                    },
                    {
                        "id": "model",
                        "name": "Model",
                        "type": "select",
                        "currentValue": model.unwrap_or_default(),
                        "options": []
                    }
                ]
            }),
        )
        .await
    }

    async fn handle_request(self: Arc<Self>, req: JsonRpcRequest) -> Result<()> {
        if let Err(err) = Self::validate_payload(&req.method, &req.params) {
            let Some(req_id) = req.id else {
                return Ok(());
            };
            return self
                .send_error_with_data(
                    req_id,
                    ACP_PAYLOAD_TOO_LARGE,
                    err.to_string(),
                    error_data("payload_too_large", err.to_string()),
                )
                .await;
        }

        match req.method.as_str() {
            "initialize" => self.handle_initialize(req).await,
            "session/new" => Arc::clone(&self).handle_session_new(req).await,
            "session/load" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: SessionLoadParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/load")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };
                let mode = {
                    let agent = session.agent.lock().await;
                    mode_to_str(agent.mode())
                };
                self.send_response(
                    req_id,
                    json!({
                        "modes": {
                            "currentModeId": mode,
                            "availableModes": [
                                { "id": "ask", "name": "Ask", "description": "Q&A mode" },
                                { "id": "plan", "name": "Plan", "description": "Planning mode" },
                                { "id": "build", "name": "Build", "description": "Implementation mode" }
                            ]
                        },
                        "configOptions": []
                    }),
                )
                .await
            }
            "session/set_mode" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: SessionSetModeParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/set_mode")?;
                let mode = parse_mode(Some(&params.mode_id))?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let mut tools = ToolRegistry::for_mode_with_interaction(
                    session.cwd.clone(),
                    mode,
                    self.shell_enabled,
                    Some(session.interaction_tx.clone()),
                );
                tools.set_tool_timeout_secs(self.tool_timeout_secs);

                let mut agent = session.agent.lock().await;
                tools.set_trust_level(agent.trust_level());
                agent.update_mode(tools, mode);

                self.send_response(
                    req_id,
                    json!({
                        "_meta": {
                            "tark": {
                                "sessionId": params.session_id,
                                "mode": mode_to_str(mode),
                            }
                        }
                    }),
                )
                .await
            }
            "session/set_config_option" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: SessionSetConfigOptionParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/set_config_option")?;
                let Some(_session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                self.send_response(
                    req_id,
                    json!({
                        "configOptions": [
                            {
                                "id": params.config_id,
                                "name": params.config_id,
                                "type": "select",
                                "currentValue": params.value,
                                "options": []
                            }
                        ]
                    }),
                )
                .await
            }
            "context/update" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: ContextUpdateParams = serde_json::from_value(req.params)
                    .context("Invalid params for context/update")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let mut ctx = session.context.lock().await;
                ctx.active_file = params.active_file;
                ctx.cursor = params.cursor;
                ctx.selection = params.selection;
                ctx.active_excerpt = params.active_excerpt;
                ctx.buffers = params.buffers;

                self.send_response(req_id, json!({ "ok": true })).await
            }
            "session/prompt" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: PromptRequestParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/prompt")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                {
                    let now = Instant::now();
                    let mut win = session.request_times.lock().await;
                    while let Some(ts) = win.front() {
                        if now.duration_since(*ts) > Duration::from_secs(60) {
                            win.pop_front();
                        } else {
                            break;
                        }
                    }
                    if win.len() >= MAX_REQUESTS_PER_MINUTE {
                        return self
                            .send_error_with_data(
                                req_id,
                                ACP_RATE_LIMITED,
                                "Too many session requests",
                                error_data("rate_limited", "max requests per minute exceeded"),
                            )
                            .await;
                    }
                    win.push_back(now);
                }

                let request_id =
                    format!("req-{}", self.next_session.fetch_add(1, Ordering::SeqCst));

                {
                    let mut current = session.current_request.lock().await;
                    if current.is_some() {
                        return self
                            .send_error_with_data(
                                req_id,
                                ACP_SESSION_BUSY,
                                "Session is busy",
                                error_data("session_busy", "one request at a time per session"),
                            )
                            .await;
                    }
                    *current = Some(request_id.clone());
                }

                let server = Arc::clone(&self);
                let message = prompt_to_text(&params.prompt);
                let spawn_request_id = request_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = Arc::clone(&server)
                        .run_session_prompt(session, spawn_request_id.clone(), message, req_id)
                        .await
                    {
                        let _ = server
                            .send_notification(
                                "session/update",
                                json!({
                                    "sessionId": params.session_id,
                                    "update": {
                                        "sessionUpdate": "agent_message_end",
                                        "responseId": spawn_request_id,
                                        "stopReason": "refusal",
                                    },
                                    "_meta": {
                                        "tark": {
                                            "requestId": spawn_request_id,
                                            "errorCode": "internal_error",
                                            "errorMessage": err.to_string(),
                                        }
                                    }
                                }),
                            )
                            .await;
                    }
                });

                Ok(())
            }
            "tark/inline_completion" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: InlineCompletionParams = serde_json::from_value(req.params)
                    .context("Invalid params for tark/inline_completion")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req_id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let provider_impl = llm::create_provider_with_options(
                    &session.provider,
                    true,
                    session.model.as_deref(),
                )
                .with_context(|| format!("Failed to create provider '{}'", session.provider))?;
                let config = crate::config::Config::load().unwrap_or_default();
                let engine = CompletionEngine::new(Arc::from(provider_impl))
                    .with_cache_size(config.completion.cache_size)
                    .with_context_lines(
                        config.completion.context_lines_before,
                        config.completion.context_lines_after,
                    );

                let file_content = format!("{}{}", params.prefix, params.suffix);
                let req = CompletionRequest {
                    file_path: std::path::PathBuf::from(params.path),
                    file_content,
                    cursor_line: params.cursor.line,
                    cursor_col: params.cursor.col,
                    related_files: vec![],
                    lsp_context: None,
                };

                let completion = engine
                    .complete(&req)
                    .await
                    .map(|r| r.completion)
                    .unwrap_or_default();
                let stop_reason = if completion.is_empty() {
                    "empty"
                } else {
                    "completed"
                };

                self.send_response(
                    req_id,
                    json!({
                        "completion": completion,
                        "stopReason": stop_reason,
                        "_meta": {
                            "tark": {
                                "provider": session.provider,
                                "model": session.model,
                            }
                        }
                    }),
                )
                .await
            }
            "session/cancel" => {
                let req_id = req.id;
                let params: CancelParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/cancel")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    if let Some(id) = req_id {
                        return self
                            .send_error_with_data(
                                id,
                                ACP_SESSION_NOT_FOUND,
                                "Session not found",
                                error_data("session_not_found", params.session_id),
                            )
                            .await;
                    }
                    return Ok(());
                };

                let mut cancelled = false;
                let current = session.current_request.lock().await;
                if let Some(active) = current.as_ref() {
                    let _ = active;
                    session.interrupt.store(true, Ordering::SeqCst);
                    cancelled = true;
                }

                if let Some(id) = req_id {
                    return self
                        .send_response(id, json!({ "cancelled": cancelled }))
                        .await;
                }
                Ok(())
            }
            "session/close" => {
                let Some(req_id) = req.id else {
                    return Ok(());
                };
                let params: CloseParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/close")?;
                self.sessions.lock().await.remove(&params.session_id);
                self.send_response(req_id, json!({ "closed": true })).await
            }
            _ => {
                if let Some(id) = req.id {
                    let method = req.method;
                    self.send_error_with_data(
                        id,
                        JSONRPC_METHOD_NOT_FOUND,
                        format!("Method '{}' not found", method),
                        error_data("unsupported_method", method),
                    )
                    .await
                } else {
                    Ok(())
                }
            }
        }
    }

    async fn run_session_prompt(
        self: Arc<Self>,
        session: Arc<AcpSession>,
        request_id: String,
        message: String,
        response_id: Value,
    ) -> Result<()> {
        let session_id = session.id.clone();
        let response_stream_id = request_id.clone();
        let mut guard = ActiveRequestGuard::new(Arc::clone(&session.current_request));

        let context_snapshot = session.context.lock().await.clone();
        let merged_message = merge_message_with_context(&message, &context_snapshot);

        session.interrupt.store(false, Ordering::SeqCst);

        let _ = self
            .send_notification(
                "session/update",
                json!({
                    "sessionId": session_id,
                    "update": {
                        "sessionUpdate": "agent_message_start",
                        "responseId": response_stream_id,
                    },
                    "_meta": {
                        "tark": {
                            "requestId": request_id,
                        }
                    }
                }),
            )
            .await;

        let server_for_text = Arc::clone(&self);
        let sid_for_text = session_id.clone();
        let rid_for_text = request_id.clone();
        let response_for_text = response_stream_id.clone();

        let server_for_tool_start = Arc::clone(&self);
        let sid_for_tool_start = session_id.clone();
        let rid_for_tool_start = request_id.clone();
        let response_for_tool_start = response_stream_id.clone();

        let server_for_tool_end = Arc::clone(&self);
        let sid_for_tool_end = session_id.clone();
        let rid_for_tool_end = request_id.clone();
        let response_for_tool_end = response_stream_id.clone();

        let interrupt = Arc::clone(&session.interrupt);
        let check_interrupt = move || interrupt.load(Ordering::SeqCst);

        let response = {
            let mut agent = session.agent.lock().await;
            agent
                .chat_streaming(
                    &merged_message,
                    check_interrupt,
                    move |chunk| {
                        let server = Arc::clone(&server_for_text);
                        let sid = sid_for_text.clone();
                        let rid = rid_for_text.clone();
                        let response_id = response_for_text.clone();
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "session/update",
                                    json!({
                                        "sessionId": sid,
                                        "update": {
                                            "sessionUpdate": "agent_message_chunk",
                                            "responseId": response_id,
                                            "content": {
                                                "type": "text",
                                                "text": chunk,
                                            }
                                        },
                                        "_meta": {
                                            "tark": {
                                                "requestId": rid,
                                            }
                                        }
                                    }),
                                )
                                .await;
                        });
                    },
                    |_thinking| {},
                    move |name, args| {
                        let server = Arc::clone(&server_for_tool_start);
                        let sid = sid_for_tool_start.clone();
                        let rid = rid_for_tool_start.clone();
                        let response_id = response_for_tool_start.clone();
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "session/update",
                                    json!({
                                        "sessionId": sid,
                                        "update": {
                                            "sessionUpdate": "tool_call",
                                            "responseId": response_id,
                                            "toolCallId": format!("tool-{}", rid),
                                            "title": name,
                                            "status": "pending",
                                            "kind": "execute",
                                            "rawInput": args,
                                        },
                                        "_meta": { "tark": { "requestId": rid } }
                                    }),
                                )
                                .await;
                        });
                    },
                    move |name, output, success| {
                        let server = Arc::clone(&server_for_tool_end);
                        let sid = sid_for_tool_end.clone();
                        let rid = rid_for_tool_end.clone();
                        let response_id = response_for_tool_end.clone();
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "session/update",
                                    json!({
                                        "sessionId": sid,
                                        "update": {
                                            "sessionUpdate": "tool_call_update",
                                            "responseId": response_id,
                                            "toolCallId": format!("tool-{}", rid),
                                            "title": name,
                                            "status": if success { "completed" } else { "failed" },
                                            "rawOutput": output,
                                        },
                                        "_meta": { "tark": { "requestId": rid } }
                                    }),
                                )
                                .await;
                        });
                    },
                    |_committed| {},
                )
                .await
        };

        match response {
            Ok(response) => {
                let stop_reason = if session.interrupt.load(Ordering::SeqCst) {
                    "cancelled"
                } else {
                    "end_turn"
                };
                let _ = self
                    .send_response(response_id, json!({ "stopReason": stop_reason }))
                    .await;
                if !response.text.is_empty() {
                    let _ = self
                        .send_notification(
                            "session/update",
                            json!({
                                "sessionId": session_id,
                                "update": {
                                    "sessionUpdate": "agent_message_chunk",
                                    "responseId": response_stream_id,
                                    "content": {
                                        "type": "text",
                                        "text": response.text,
                                    }
                                },
                                "_meta": {
                                    "tark": {
                                        "requestId": request_id,
                                        "usage": response.usage,
                                        "toolCallsMade": response.tool_calls_made,
                                        "contextUsagePercent": response.context_usage_percent,
                                    }
                                }
                            }),
                        )
                        .await;
                }
                let _ = self
                    .send_notification(
                        "session/update",
                        json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_message_end",
                                "responseId": response_stream_id,
                                "stopReason": stop_reason,
                            },
                            "_meta": {
                                "tark": {
                                    "requestId": request_id,
                                    "usage": response.usage,
                                    "toolCallsMade": response.tool_calls_made,
                                    "contextUsagePercent": response.context_usage_percent,
                                }
                            }
                        }),
                    )
                    .await;
            }
            Err(err) => {
                let stop_reason = if session.interrupt.load(Ordering::SeqCst) {
                    "cancelled"
                } else {
                    "refusal"
                };
                let _ = self
                    .send_response(response_id, json!({ "stopReason": stop_reason }))
                    .await;
                let _ = self
                    .send_notification(
                        "session/update",
                        json!({
                            "sessionId": session_id,
                            "update": {
                                "sessionUpdate": "agent_message_end",
                                "responseId": response_stream_id,
                                "stopReason": stop_reason,
                            },
                            "_meta": {
                                "tark": {
                                    "requestId": request_id,
                                    "errorCode": "internal_error",
                                    "errorMessage": err.to_string(),
                                }
                            }
                        }),
                    )
                    .await;
            }
        }

        session.interrupt.store(false, Ordering::SeqCst);
        guard.clear_now().await;
        Ok(())
    }
}

fn permission_options_for_request(
    request: &crate::tools::questionnaire::ApprovalRequest,
) -> Vec<Value> {
    let mut options = vec![
        json!({ "optionId": "allow_once", "name": "Allow once", "kind": "allow_once" }),
        json!({ "optionId": "reject_once", "name": "Reject once", "kind": "reject_once" }),
    ];

    if let Some(first) = request.suggested_patterns.first() {
        options.push(json!({
            "optionId": format!("allow_always:{}", first.pattern),
            "name": format!("Always allow {}", first.pattern),
            "kind": "allow_always",
            "_meta": { "tark": { "pattern": first.pattern } }
        }));
        options.push(json!({
            "optionId": format!("reject_always:{}", first.pattern),
            "name": format!("Always reject {}", first.pattern),
            "kind": "reject_always",
            "_meta": { "tark": { "pattern": first.pattern } }
        }));
    }

    options
}

fn map_permission_response(
    result: Result<Value>,
    request: &crate::tools::questionnaire::ApprovalRequest,
) -> crate::tools::questionnaire::ApprovalResponse {
    use crate::tools::questionnaire::{ApprovalPattern, ApprovalResponse};
    use crate::tools::risk::MatchType;

    let Ok(value) = result else {
        return ApprovalResponse::deny();
    };
    let Ok(parsed) = serde_json::from_value::<RequestPermissionResponseResult>(value) else {
        return ApprovalResponse::deny();
    };

    match parsed.outcome {
        RequestPermissionOutcome::Cancelled => ApprovalResponse::deny(),
        RequestPermissionOutcome::Selected { option_id } => {
            if option_id == "allow_once" {
                return ApprovalResponse::approve_once();
            }
            if option_id == "reject_once" {
                return ApprovalResponse::deny();
            }

            if let Some(pattern) = option_id.strip_prefix("allow_always:") {
                return ApprovalResponse::approve_always(ApprovalPattern::new(
                    request.tool.clone(),
                    pattern.to_string(),
                    MatchType::Prefix,
                ));
            }
            if let Some(pattern) = option_id.strip_prefix("reject_always:") {
                return ApprovalResponse::deny_always(ApprovalPattern::new(
                    request.tool.clone(),
                    pattern.to_string(),
                    MatchType::Prefix,
                ));
            }

            ApprovalResponse::deny()
        }
    }
}

fn default_provider() -> String {
    let config = crate::config::Config::load().unwrap_or_default();
    config.llm.default_provider
}

fn model_preference_for_mode(
    session: &crate::storage::ChatSession,
    mode: AgentMode,
) -> Option<crate::storage::ModelPreference> {
    let pref = match mode {
        AgentMode::Build => &session.mode_preferences.build,
        AgentMode::Plan => &session.mode_preferences.plan,
        AgentMode::Ask => &session.mode_preferences.ask,
    };
    if pref.is_empty() {
        None
    } else {
        Some(pref.clone())
    }
}

fn session_bootstrap(cwd: &std::path::Path, requested_mode: Option<&str>) -> SessionBootstrap {
    let default_mode = parse_mode(requested_mode).unwrap_or(AgentMode::Build);
    let mut bootstrap = SessionBootstrap {
        mode: default_mode,
        provider: default_provider(),
        model: None,
    };

    let Ok(storage) = TarkStorage::new(cwd) else {
        return bootstrap;
    };
    let Ok(session) = storage.load_current_session() else {
        return bootstrap;
    };

    bootstrap.mode = if requested_mode.is_some() {
        default_mode
    } else {
        parse_mode(Some(&session.mode)).unwrap_or(AgentMode::Build)
    };

    if let Some(pref) = model_preference_for_mode(&session, bootstrap.mode) {
        if !pref.provider.trim().is_empty() {
            bootstrap.provider = pref.provider.trim().to_string();
        }
        if !pref.model.trim().is_empty() {
            bootstrap.model = Some(pref.model.trim().to_string());
        }
        return bootstrap;
    }

    if !session.provider.trim().is_empty() {
        bootstrap.provider = session.provider.trim().to_string();
    }
    if !session.model.trim().is_empty() {
        bootstrap.model = Some(session.model.trim().to_string());
    }

    bootstrap
}

fn parse_mode(mode: Option<&str>) -> Result<AgentMode> {
    Ok(match mode.unwrap_or("build").to_lowercase().as_str() {
        "ask" => AgentMode::Ask,
        "plan" => AgentMode::Plan,
        "build" => AgentMode::Build,
        other => anyhow::bail!("Unsupported mode '{}'. Use ask|plan|build", other),
    })
}

fn mode_to_str(mode: AgentMode) -> &'static str {
    match mode {
        AgentMode::Ask => "ask",
        AgentMode::Plan => "plan",
        AgentMode::Build => "build",
    }
}

fn resolve_cwd(raw: Option<String>, fallback: &std::path::Path) -> Result<PathBuf> {
    let candidate = raw
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback.to_path_buf());
    if candidate.exists() {
        Ok(candidate.canonicalize().unwrap_or(candidate))
    } else {
        anyhow::bail!("Working directory does not exist: {}", candidate.display())
    }
}

fn merge_message_with_context(message: &str, context: &SessionContext) -> String {
    let mut lines: Vec<String> = vec![];
    if context.active_file.is_some()
        || context.cursor.is_some()
        || context.selection.is_some()
        || context.active_excerpt.is_some()
    {
        lines.push("[Editor Context]".to_string());

        if let Some(path) = &context.active_file {
            lines.push(format!("Active file: {}", path));
        }

        if let Some(cursor) = &context.cursor {
            lines.push(format!("Cursor: line {}, col {}", cursor.line, cursor.col));
        }

        if let Some(sel) = &context.selection {
            lines.push(format!(
                "Selection: {}:{} to {}:{}",
                sel.start_line, sel.start_col, sel.end_line, sel.end_col
            ));
            if !sel.text.is_empty() {
                lines.push("Selected text:".to_string());
                lines.push("```".to_string());
                lines.push(sel.text.clone());
                lines.push("```".to_string());
            }
        }

        if let Some(excerpt) = &context.active_excerpt {
            lines.push("Active excerpt:".to_string());
            lines.push("```".to_string());
            lines.push(excerpt.clone());
            lines.push("```".to_string());
        }

        if !context.buffers.is_empty() {
            lines.push("Open buffers:".to_string());
            for buf in context.buffers.iter().take(10) {
                let name = buf
                    .name
                    .clone()
                    .or_else(|| buf.path.clone())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let marker = if buf.modified { " (modified)" } else { "" };
                lines.push(format!("- {}{}", name, marker));
            }
        }

        lines.push("".to_string());
        lines.push("[User Request]".to_string());
    }

    lines.push(message.to_string());
    lines.join("\n")
}

pub async fn run_acp_stdio(cwd: Option<String>) -> Result<()> {
    let working_dir = resolve_cwd(cwd, &std::env::current_dir()?)?;
    let config = crate::config::Config::load().unwrap_or_default();
    let server = Arc::new(AcpServer::new(working_dir, &config));

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);

    loop {
        let payload = match framing::read_frame(&mut reader, MAX_FRAME_BYTES).await {
            Ok(Some(payload)) => payload,
            Ok(None) => break,
            Err(err) => {
                tracing::warn!("ACP frame parse error: {}", err);
                continue;
            }
        };

        let value: Value = match serde_json::from_slice(&payload) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!("ACP parse error: {}", err);
                continue;
            }
        };

        if value.get("method").is_some() {
            let req: JsonRpcRequest = match serde_json::from_value(value) {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("ACP request parse error: {}", err);
                    continue;
                }
            };

            if req.jsonrpc.as_deref().is_some_and(|v| v != "2.0") {
                if let Some(id) = req.id {
                    server
                        .send_error(id, JSONRPC_INVALID_REQUEST, "Invalid JSON-RPC version")
                        .await?;
                }
                continue;
            }

            let server_clone = Arc::clone(&server);
            if let Err(err) = server_clone.handle_request(req).await {
                tracing::error!("ACP request error: {}", err);
            }
        } else if value.get("id").is_some()
            && (value.get("result").is_some() || value.get("error").is_some())
        {
            let resp: JsonRpcResponseEnvelope = match serde_json::from_value(value) {
                Ok(v) => v,
                Err(err) => {
                    tracing::warn!("ACP response parse error: {}", err);
                    continue;
                }
            };
            server.handle_response(resp).await;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{ChatSession, ModelPreference, TarkStorage};
    use crate::tools::questionnaire::{ApprovalChoice, ApprovalRequest, SuggestedPattern};
    use crate::tools::risk::{MatchType, RiskLevel};
    use tempfile::tempdir;

    #[test]
    fn parse_mode_values() {
        assert!(matches!(parse_mode(Some("ask")).unwrap(), AgentMode::Ask));
        assert!(matches!(parse_mode(Some("plan")).unwrap(), AgentMode::Plan));
        assert!(matches!(
            parse_mode(Some("build")).unwrap(),
            AgentMode::Build
        ));
        assert!(parse_mode(Some("invalid")).is_err());
    }

    #[test]
    fn merge_message_includes_context() {
        let msg = merge_message_with_context(
            "Explain this",
            &SessionContext {
                active_file: Some("src/main.rs".to_string()),
                cursor: Some(CursorPos { line: 10, col: 4 }),
                selection: Some(SelectionContext {
                    start_line: 8,
                    start_col: 1,
                    end_line: 10,
                    end_col: 4,
                    text: "fn main() {}".to_string(),
                }),
                active_excerpt: Some("let x = 1;".to_string()),
                buffers: vec![BufferSummary {
                    path: Some("src/main.rs".to_string()),
                    name: Some("main.rs".to_string()),
                    modified: false,
                }],
            },
        );

        assert!(msg.contains("[Editor Context]"));
        assert!(msg.contains("Active file: src/main.rs"));
        assert!(msg.contains("Explain this"));
        assert!(msg.contains("Active excerpt:"));
    }

    #[test]
    fn validate_payload_limits_prompt_size() {
        let huge = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let params = json!({
            "prompt": [
                { "type": "text", "text": huge }
            ]
        });
        assert!(AcpServer::validate_payload("session/prompt", &params).is_err());
    }

    #[test]
    fn validate_payload_limits_buffer_count() {
        let mut buffers = vec![];
        for i in 0..(MAX_BUFFERS + 1) {
            buffers.push(json!({ "name": format!("b{}", i) }));
        }
        let params = json!({ "buffers": buffers });
        assert!(AcpServer::validate_payload("context/update", &params).is_err());
    }

    #[test]
    fn session_bootstrap_uses_current_workspace_session_defaults() {
        let tmp = tempdir().unwrap();
        let storage = TarkStorage::new(tmp.path()).unwrap();

        let mut session = ChatSession::new();
        session.mode = "plan".to_string();
        session.provider = "ollama".to_string();
        session.model = "qwen2.5-coder:3b".to_string();
        session.mode_preferences.plan = ModelPreference::new("openai", "gpt-4o-mini");
        storage.save_session(&session).unwrap();

        let bootstrap = session_bootstrap(tmp.path(), None);
        assert!(matches!(bootstrap.mode, AgentMode::Plan));
        assert_eq!(bootstrap.provider, "openai");
        assert_eq!(bootstrap.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn session_bootstrap_respects_requested_mode_and_mode_preferences() {
        let tmp = tempdir().unwrap();
        let storage = TarkStorage::new(tmp.path()).unwrap();

        let mut session = ChatSession::new();
        session.mode = "build".to_string();
        session.provider = "ollama".to_string();
        session.model = "qwen2.5-coder:7b".to_string();
        session.mode_preferences.ask = ModelPreference::new("claude", "claude-sonnet-4");
        storage.save_session(&session).unwrap();

        let bootstrap = session_bootstrap(tmp.path(), Some("ask"));
        assert!(matches!(bootstrap.mode, AgentMode::Ask));
        assert_eq!(bootstrap.provider, "claude");
        assert_eq!(bootstrap.model.as_deref(), Some("claude-sonnet-4"));
    }

    #[test]
    fn map_permission_response_maps_allow_once() {
        let req = ApprovalRequest {
            tool: "shell".to_string(),
            command: "ls".to_string(),
            risk_level: RiskLevel::Risky,
            suggested_patterns: vec![SuggestedPattern {
                pattern: "ls".to_string(),
                match_type: MatchType::Prefix,
                description: "ls".to_string(),
            }],
        };

        let response = map_permission_response(
            Ok(json!({
                "outcome": {
                    "outcome": "selected",
                    "optionId": "allow_once"
                }
            })),
            &req,
        );

        assert_eq!(response.choice, ApprovalChoice::ApproveOnce);
        assert!(response.selected_pattern.is_none());
    }

    #[test]
    fn map_permission_response_maps_reject_once() {
        let req = ApprovalRequest {
            tool: "shell".to_string(),
            command: "rm -rf /tmp/foo".to_string(),
            risk_level: RiskLevel::Dangerous,
            suggested_patterns: vec![],
        };

        let response = map_permission_response(
            Ok(json!({
                "outcome": {
                    "outcome": "selected",
                    "optionId": "reject_once"
                }
            })),
            &req,
        );

        assert_eq!(response.choice, ApprovalChoice::Deny);
        assert!(response.selected_pattern.is_none());
    }

    #[test]
    fn map_permission_response_invalid_option_defaults_to_deny_once() {
        let req = ApprovalRequest {
            tool: "shell".to_string(),
            command: "echo hi".to_string(),
            risk_level: RiskLevel::Risky,
            suggested_patterns: vec![SuggestedPattern {
                pattern: "echo".to_string(),
                match_type: MatchType::Prefix,
                description: "echo".to_string(),
            }],
        };

        let response = map_permission_response(
            Ok(json!({
                "outcome": {
                    "outcome": "selected",
                    "optionId": "unknown-option"
                }
            })),
            &req,
        );

        assert_eq!(response.choice, ApprovalChoice::Deny);
    }
}
