use crate::agent::ChatAgent;
use crate::llm;
use crate::tools::questionnaire::{interaction_channel, InteractionRequest};
use crate::tools::{AgentMode, ToolRegistry};
use crate::transport::acp::errors::*;
use crate::transport::acp::framing;
use crate::transport::acp::interaction_bridge::{
    map_approval_decision, map_questionnaire_response,
};
use crate::transport::acp::protocol::*;
use crate::transport::acp::session::{
    AcpSession, ActiveRequestGuard, PendingInteraction, SessionContext,
};
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
use tokio::sync::Mutex;

const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_MESSAGE_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_TEXT_BYTES: usize = 128 * 1024;
const MAX_BUFFERS: usize = 64;
const MAX_SESSIONS: usize = 8;
const MAX_REQUESTS_PER_MINUTE: usize = 30;
const APPROVAL_TIMEOUT_SECS: u64 = 120;
const QUESTIONNAIRE_TIMEOUT_SECS: u64 = 180;

pub struct AcpServer {
    working_dir: PathBuf,
    shell_enabled: bool,
    tool_timeout_secs: u64,
    max_iterations: usize,
    thinking_config: crate::config::ThinkingConfig,
    default_think_level: String,
    next_session: AtomicU64,
    sessions: Mutex<HashMap<String, Arc<AcpSession>>>,
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
            sessions: Mutex::new(HashMap::new()),
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

    async fn status_value(&self, session: &AcpSession, busy: bool) -> Value {
        let mode = {
            let agent = session.agent.lock().await;
            mode_to_str(agent.mode())
        };
        json!({
            "session_id": session.id,
            "busy": busy,
            "mode": mode,
            "provider": session.provider,
            "model": session.model,
        })
    }

    fn validate_payload(method: &str, params: &Value) -> Result<()> {
        match method {
            "session/send_message" => {
                if let Some(msg) = params.get("message").and_then(|v| v.as_str()) {
                    if msg.len() > MAX_MESSAGE_BYTES {
                        anyhow::bail!("message exceeds max size");
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
        let params: InitializeParams =
            serde_json::from_value(req.params).unwrap_or(InitializeParams {
                client: None,
                versions: vec![],
            });

        if !params.versions.is_empty() && !params.versions.iter().any(|v| v == ACP_VERSION) {
            return self
                .send_error_with_data(
                    req.id,
                    ACP_UNSUPPORTED_VERSION,
                    format!("Unsupported ACP version. Server supports '{}'", ACP_VERSION),
                    error_data("unsupported_version", format!("supported={}", ACP_VERSION)),
                )
                .await;
        }

        if let Some(client) = params.client {
            tracing::info!(
                "ACP initialize from client '{}' version '{}'",
                client.name,
                client.version
            );
        }

        self.send_response(
            req.id,
            json!({
                "acp_version": ACP_VERSION,
                "server": {
                    "name": "tark",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "supports_modes": true,
                    "supports_approvals": true,
                    "supports_questionnaires": true,
                    "supports_editor_open_file": false,
                    "streaming": true,
                    "framing": "content-length"
                }
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

            let interaction_id = format!(
                "itx-{}",
                session.next_interaction_id.fetch_add(1, Ordering::SeqCst)
            );

            match interaction {
                InteractionRequest::Approval { request, responder } => {
                    session.pending_interactions.lock().await.insert(
                        interaction_id.clone(),
                        PendingInteraction::Approval {
                            request_id: request_id.clone(),
                            responder,
                            request: request.clone(),
                        },
                    );

                    let _ = self
                        .send_notification(
                            "approval/request",
                            json!({
                                "session_id": session.id,
                                "request_id": request_id,
                                "interaction_id": interaction_id,
                                "tool": request.tool,
                                "command": request.command,
                                "risk": format!("{:?}", request.risk_level).to_lowercase(),
                                "pattern_options": request.suggested_patterns,
                                "timeout_seconds": APPROVAL_TIMEOUT_SECS,
                            }),
                        )
                        .await;

                    let pending = Arc::clone(&session.pending_interactions);
                    let sid = session.id.clone();
                    let iid = interaction_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(APPROVAL_TIMEOUT_SECS)).await;
                        if let Some(PendingInteraction::Approval { responder, .. }) =
                            pending.lock().await.remove(&iid)
                        {
                            let _ = responder
                                .send(crate::tools::questionnaire::ApprovalResponse::deny());
                            tracing::warn!("ACP approval interaction timed out: {} {}", sid, iid);
                        }
                    });
                }
                InteractionRequest::Questionnaire { data, responder } => {
                    session.pending_interactions.lock().await.insert(
                        interaction_id.clone(),
                        PendingInteraction::Questionnaire {
                            request_id: request_id.clone(),
                            responder,
                        },
                    );

                    let _ = self
                        .send_notification(
                            "questionnaire/request",
                            json!({
                                "session_id": session.id,
                                "request_id": request_id,
                                "interaction_id": interaction_id,
                                "questionnaire": data,
                                "timeout_seconds": QUESTIONNAIRE_TIMEOUT_SECS,
                            }),
                        )
                        .await;

                    let pending = Arc::clone(&session.pending_interactions);
                    let sid = session.id.clone();
                    let iid = interaction_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_secs(QUESTIONNAIRE_TIMEOUT_SECS)).await;
                        if let Some(PendingInteraction::Questionnaire { responder, .. }) =
                            pending.lock().await.remove(&iid)
                        {
                            let _ = responder
                                .send(crate::tools::questionnaire::UserResponse::cancelled());
                            tracing::warn!(
                                "ACP questionnaire interaction timed out: {} {}",
                                sid,
                                iid
                            );
                        }
                    });
                }
            }
        }
    }

    async fn handle_session_create(self: Arc<Self>, req: JsonRpcRequest) -> Result<()> {
        let params: SessionCreateParams =
            serde_json::from_value(req.params).context("Invalid params for session/create")?;

        if params.provider.is_some() || params.model.is_some() {
            return self
                .send_error_with_data(
                    req.id,
                    ACP_PROVIDER_MODEL_OVERRIDE,
                    "ACP provider/model overrides are not allowed",
                    error_data(
                        "provider_model_override_not_allowed",
                        "configure provider/model locally via tark configuration",
                    ),
                )
                .await;
        }

        let mut sessions = self.sessions.lock().await;
        if sessions.len() >= MAX_SESSIONS {
            drop(sessions);
            return self
                .send_error_with_data(
                    req.id,
                    ACP_RATE_LIMITED,
                    "Too many ACP sessions",
                    error_data("rate_limited", "max sessions reached"),
                )
                .await;
        }

        let mode = parse_mode(params.mode.as_deref())?;
        let provider = default_provider();
        let cwd = resolve_cwd(params.cwd, &self.working_dir)?;

        let provider_impl = llm::create_provider_with_options(&provider, true, None)
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
            model: None,
            agent: Arc::new(Mutex::new(agent)),
            context: Arc::new(Mutex::new(SessionContext::default())),
            current_request: Arc::new(Mutex::new(None)),
            interrupt: Arc::new(AtomicBool::new(false)),
            interaction_tx,
            pending_interactions: Arc::new(Mutex::new(HashMap::new())),
            next_interaction_id: AtomicU64::new(1),
            request_times: Arc::new(Mutex::new(std::collections::VecDeque::new())),
        });

        sessions.insert(session_id.clone(), Arc::clone(&session));
        drop(sessions);

        tokio::spawn(
            Arc::clone(&self).spawn_interaction_worker(Arc::clone(&session), interaction_rx),
        );

        self.send_response(
            req.id,
            json!({
                "session_id": session_id,
                "mode": mode_to_str(mode),
                "provider": provider,
                "model": Value::Null,
            }),
        )
        .await
    }

    async fn handle_request(self: Arc<Self>, req: JsonRpcRequest) -> Result<()> {
        if let Err(err) = Self::validate_payload(&req.method, &req.params) {
            return self
                .send_error_with_data(
                    req.id,
                    ACP_PAYLOAD_TOO_LARGE,
                    err.to_string(),
                    error_data("payload_too_large", err.to_string()),
                )
                .await;
        }

        match req.method.as_str() {
            "initialize" => self.handle_initialize(req).await,
            "session/create" => Arc::clone(&self).handle_session_create(req).await,
            "session/set_mode" => {
                let params: SessionSetModeParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/set_mode")?;
                let mode = parse_mode(Some(&params.mode))?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
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
                    req.id,
                    json!({
                        "session_id": params.session_id,
                        "mode": mode_to_str(mode),
                    }),
                )
                .await
            }
            "context/update" => {
                let params: ContextUpdateParams = serde_json::from_value(req.params)
                    .context("Invalid params for context/update")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
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

                self.send_response(req.id, json!({ "ok": true })).await
            }
            "session/send_message" => {
                let params: SendMessageParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/send_message")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
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
                                req.id,
                                ACP_RATE_LIMITED,
                                "Too many session requests",
                                error_data("rate_limited", "max requests per minute exceeded"),
                            )
                            .await;
                    }
                    win.push_back(now);
                }

                let request_id = params.request_id.unwrap_or_else(|| {
                    format!("req-{}", self.next_session.fetch_add(1, Ordering::SeqCst))
                });

                {
                    let mut current = session.current_request.lock().await;
                    if current.is_some() {
                        return self
                            .send_error_with_data(
                                req.id,
                                ACP_SESSION_BUSY,
                                "Session is busy",
                                error_data("session_busy", "one request at a time per session"),
                            )
                            .await;
                    }
                    *current = Some(request_id.clone());
                }

                let server = Arc::clone(&self);
                let message = params.message;
                let spawn_request_id = request_id.clone();
                tokio::spawn(async move {
                    if let Err(err) = Arc::clone(&server)
                        .run_session_message(session, spawn_request_id.clone(), message)
                        .await
                    {
                        let _ = server
                            .send_notification(
                                "error/event",
                                json!({
                                    "request_id": spawn_request_id,
                                    "code": "internal_error",
                                    "message": err.to_string(),
                                }),
                            )
                            .await;
                    }
                });

                self.send_response(
                    req.id,
                    json!({
                        "accepted": true,
                        "request_id": request_id,
                    }),
                )
                .await
            }
            "session/cancel" => {
                let params: CancelParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/cancel")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let mut cancelled = false;
                let current = session.current_request.lock().await;
                if let Some(active) = current.as_ref() {
                    let matches = params
                        .request_id
                        .as_ref()
                        .map(|rid| rid == active)
                        .unwrap_or(true);
                    if matches {
                        session.interrupt.store(true, Ordering::SeqCst);
                        cancelled = true;
                    }
                }

                self.send_response(req.id, json!({ "cancelled": cancelled }))
                    .await
            }
            "approval/respond" => {
                let params: ApprovalRespondParams = serde_json::from_value(req.params)
                    .context("Invalid params for approval/respond")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let mut pending = session.pending_interactions.lock().await;
                let Some(interaction) = pending.remove(&params.interaction_id) else {
                    return self
                        .send_error(req.id, JSONRPC_INVALID_PARAMS, "Unknown interaction_id")
                        .await;
                };

                let expected_request_id = match &interaction {
                    PendingInteraction::Approval { request_id, .. } => request_id,
                    PendingInteraction::Questionnaire { request_id, .. } => request_id,
                };
                if expected_request_id != &params.request_id {
                    return self
                        .send_error(req.id, JSONRPC_INVALID_PARAMS, "request_id mismatch")
                        .await;
                }
                drop(pending);

                let (decision, _) =
                    map_approval_decision(&params.decision, params.selected_pattern, interaction)?;

                self.send_response(req.id, json!({ "accepted": true, "decision": decision }))
                    .await
            }
            "questionnaire/respond" => {
                let params: QuestionnaireRespondParams = serde_json::from_value(req.params)
                    .context("Invalid params for questionnaire/respond")?;
                let Some(session) = self.get_session(&params.session_id).await else {
                    return self
                        .send_error_with_data(
                            req.id,
                            ACP_SESSION_NOT_FOUND,
                            "Session not found",
                            error_data("session_not_found", params.session_id),
                        )
                        .await;
                };

                let mut pending = session.pending_interactions.lock().await;
                let Some(interaction) = pending.remove(&params.interaction_id) else {
                    return self
                        .send_error(req.id, JSONRPC_INVALID_PARAMS, "Unknown interaction_id")
                        .await;
                };

                let expected_request_id = match &interaction {
                    PendingInteraction::Approval { request_id, .. } => request_id,
                    PendingInteraction::Questionnaire { request_id, .. } => request_id,
                };
                if expected_request_id != &params.request_id {
                    return self
                        .send_error(req.id, JSONRPC_INVALID_PARAMS, "request_id mismatch")
                        .await;
                }
                drop(pending);

                map_questionnaire_response(params.cancelled, params.answers, interaction)?;

                self.send_response(req.id, json!({ "accepted": true }))
                    .await
            }
            "session/close" => {
                let params: CloseParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/close")?;
                self.sessions.lock().await.remove(&params.session_id);
                self.send_response(req.id, json!({ "closed": true })).await
            }
            _ => {
                self.send_error(
                    req.id,
                    JSONRPC_METHOD_NOT_FOUND,
                    format!("Method '{}' not found", req.method),
                )
                .await
            }
        }
    }

    async fn run_session_message(
        self: Arc<Self>,
        session: Arc<AcpSession>,
        request_id: String,
        message: String,
    ) -> Result<()> {
        let session_id = session.id.clone();
        let mut guard = ActiveRequestGuard::new(Arc::clone(&session.current_request));

        let _ = self
            .send_notification("session/status", self.status_value(&session, true).await)
            .await;

        let context_snapshot = session.context.lock().await.clone();
        let merged_message = merge_message_with_context(&message, &context_snapshot);

        session.interrupt.store(false, Ordering::SeqCst);

        let server_for_text = Arc::clone(&self);
        let sid_for_text = session_id.clone();
        let rid_for_text = request_id.clone();

        let server_for_tool_start = Arc::clone(&self);
        let sid_for_tool_start = session_id.clone();
        let rid_for_tool_start = request_id.clone();

        let server_for_tool_end = Arc::clone(&self);
        let sid_for_tool_end = session_id.clone();
        let rid_for_tool_end = request_id.clone();

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
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "response/delta",
                                    json!({
                                        "session_id": sid,
                                        "request_id": rid,
                                        "delta": chunk,
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
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "tool/event",
                                    json!({
                                        "session_id": sid,
                                        "request_id": rid,
                                        "event": "start",
                                        "tool": name,
                                        "args": args,
                                    }),
                                )
                                .await;
                        });
                    },
                    move |name, output, success| {
                        let server = Arc::clone(&server_for_tool_end);
                        let sid = sid_for_tool_end.clone();
                        let rid = rid_for_tool_end.clone();
                        tokio::spawn(async move {
                            let _ = server
                                .send_notification(
                                    "tool/event",
                                    json!({
                                        "session_id": sid,
                                        "request_id": rid,
                                        "event": "end",
                                        "tool": name,
                                        "success": success,
                                        "output": output,
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
                let _ = self
                    .send_notification(
                        "response/final",
                        json!({
                            "session_id": session_id,
                            "request_id": request_id,
                            "text": response.text,
                            "tool_calls_made": response.tool_calls_made,
                            "usage": response.usage,
                            "context_usage_percent": response.context_usage_percent,
                        }),
                    )
                    .await;
            }
            Err(err) => {
                let _ = self
                    .send_notification(
                        "error/event",
                        json!({
                            "session_id": session_id,
                            "request_id": request_id,
                            "code": "internal_error",
                            "message": err.to_string(),
                        }),
                    )
                    .await;
            }
        }

        let _ = self
            .send_notification("session/status", self.status_value(&session, false).await)
            .await;

        session.interrupt.store(false, Ordering::SeqCst);
        guard.clear_now().await;
        Ok(())
    }
}

fn default_provider() -> String {
    let config = crate::config::Config::load().unwrap_or_default();
    config.llm.default_provider
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

        let req: JsonRpcRequest = match serde_json::from_slice(&payload) {
            Ok(v) => v,
            Err(err) => {
                tracing::warn!("ACP parse error: {}", err);
                continue;
            }
        };

        if req.jsonrpc.as_deref().is_some_and(|v| v != "2.0") {
            server
                .send_error(req.id, JSONRPC_INVALID_REQUEST, "Invalid JSON-RPC version")
                .await?;
            continue;
        }

        let server_clone = Arc::clone(&server);
        if let Err(err) = server_clone.handle_request(req).await {
            tracing::error!("ACP request error: {}", err);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn validate_payload_limits_message_size() {
        let huge = "x".repeat(MAX_MESSAGE_BYTES + 1);
        let params = json!({ "message": huge });
        assert!(AcpServer::validate_payload("session/send_message", &params).is_err());
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
}
