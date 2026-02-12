use crate::agent::ChatAgent;
use crate::llm;
use crate::tools::{AgentMode, ToolRegistry};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

const ACP_VERSION: &str = "1";

#[derive(Debug, Clone, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: Option<String>,
    id: Value,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
struct InitializeParams {
    #[serde(default)]
    client: Option<AcpClientInfo>,
    #[serde(default)]
    versions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct AcpClientInfo {
    #[serde(default)]
    name: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionCreateParams {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    provider: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionSetModeParams {
    session_id: String,
    mode: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ContextUpdateParams {
    session_id: String,
    #[serde(default)]
    active_file: Option<String>,
    #[serde(default)]
    cursor: Option<CursorPos>,
    #[serde(default)]
    selection: Option<SelectionContext>,
    #[serde(default)]
    buffers: Vec<BufferSummary>,
}

#[derive(Debug, Clone, Deserialize)]
struct CursorPos {
    line: usize,
    col: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct SelectionContext {
    #[serde(default)]
    start_line: usize,
    #[serde(default)]
    start_col: usize,
    #[serde(default)]
    end_line: usize,
    #[serde(default)]
    end_col: usize,
    #[serde(default)]
    text: String,
}

#[derive(Debug, Clone, Deserialize)]
struct BufferSummary {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    modified: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct SendMessageParams {
    session_id: String,
    message: String,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CancelParams {
    session_id: String,
    #[serde(default)]
    request_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct CloseParams {
    session_id: String,
}

#[derive(Debug, Clone, Default)]
struct SessionContext {
    active_file: Option<String>,
    cursor: Option<CursorPos>,
    selection: Option<SelectionContext>,
    buffers: Vec<BufferSummary>,
}

struct AcpSession {
    id: String,
    cwd: PathBuf,
    provider: String,
    model: Option<String>,
    agent: Arc<Mutex<ChatAgent>>,
    context: Arc<Mutex<SessionContext>>,
    current_request: Arc<Mutex<Option<String>>>,
    interrupt: Arc<AtomicBool>,
}

struct AcpServer {
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
    fn new(working_dir: PathBuf, config: &crate::config::Config) -> Self {
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

    async fn send_notification(&self, method: &str, params: Value) -> Result<()> {
        self.send_value(json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    async fn send_json<T: Serialize>(&self, value: &T) -> Result<()> {
        let line = serde_json::to_string(value)?;
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    async fn send_value(&self, value: Value) -> Result<()> {
        let line = serde_json::to_string(&value)?;
        let mut writer = self.writer.lock().await;
        writer.write_all(line.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    async fn get_session(&self, session_id: &str) -> Result<Arc<AcpSession>> {
        let sessions = self.sessions.lock().await;
        sessions
            .get(session_id)
            .cloned()
            .with_context(|| format!("Session '{}' not found", session_id))
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

    async fn handle_request(self: Arc<Self>, req: JsonRpcRequest) -> Result<()> {
        let method = req.method.clone();
        let id = req.id.clone();

        let response = match method.as_str() {
            "initialize" => {
                let params: InitializeParams =
                    serde_json::from_value(req.params).unwrap_or(InitializeParams {
                        client: None,
                        versions: vec![],
                    });
                if !params.versions.is_empty() && !params.versions.iter().any(|v| v == ACP_VERSION)
                {
                    self.send_error(
                        id,
                        -32010,
                        format!("Unsupported ACP version. Server supports '{}'", ACP_VERSION),
                    )
                    .await?;
                    return Ok(());
                }

                if let Some(client) = params.client {
                    tracing::info!(
                        "ACP initialize from client '{}' version '{}'",
                        client.name,
                        client.version
                    );
                }

                json!({
                    "acp_version": ACP_VERSION,
                    "server": {
                        "name": "tark",
                        "version": env!("CARGO_PKG_VERSION")
                    },
                    "capabilities": {
                        "supports_modes": true,
                        "supports_approvals": false,
                        "supports_questionnaires": false,
                        "supports_editor_open_file": false,
                        "streaming": true
                    }
                })
            }
            "session/create" => {
                let params: SessionCreateParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/create")?;
                let mode = parse_mode(params.mode.as_deref())?;
                let provider = params.provider.unwrap_or_else(default_provider);
                let cwd = resolve_cwd(params.cwd, &self.working_dir)?;

                let provider_impl =
                    llm::create_provider_with_options(&provider, true, params.model.as_deref())
                        .with_context(|| format!("Failed to create provider '{}'", provider))?;
                let mut tools = ToolRegistry::for_mode(cwd.clone(), mode, self.shell_enabled);
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
                    model: params.model,
                    agent: Arc::new(Mutex::new(agent)),
                    context: Arc::new(Mutex::new(SessionContext::default())),
                    current_request: Arc::new(Mutex::new(None)),
                    interrupt: Arc::new(AtomicBool::new(false)),
                });

                self.sessions
                    .lock()
                    .await
                    .insert(session_id.clone(), Arc::clone(&session));

                json!({
                    "session_id": session_id,
                    "mode": mode_to_str(mode),
                    "provider": provider,
                    "model": session.model,
                })
            }
            "session/set_mode" => {
                let params: SessionSetModeParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/set_mode")?;
                let mode = parse_mode(Some(&params.mode))?;
                let session = self.get_session(&params.session_id).await?;

                let mut tools =
                    ToolRegistry::for_mode(session.cwd.clone(), mode, self.shell_enabled);
                tools.set_tool_timeout_secs(self.tool_timeout_secs);

                let mut agent = session.agent.lock().await;
                tools.set_trust_level(agent.trust_level());
                agent.update_mode(tools, mode);

                json!({
                    "session_id": params.session_id,
                    "mode": mode_to_str(mode),
                })
            }
            "context/update" => {
                let params: ContextUpdateParams = serde_json::from_value(req.params)
                    .context("Invalid params for context/update")?;
                let session = self.get_session(&params.session_id).await?;
                let mut ctx = session.context.lock().await;
                ctx.active_file = params.active_file;
                ctx.cursor = params.cursor;
                ctx.selection = params.selection;
                ctx.buffers = params.buffers;

                json!({ "ok": true })
            }
            "session/send_message" => {
                let params: SendMessageParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/send_message")?;
                let session = self.get_session(&params.session_id).await?;

                let request_id = params.request_id.unwrap_or_else(|| {
                    format!("req-{}", self.next_session.fetch_add(1, Ordering::SeqCst))
                });

                {
                    let mut current = session.current_request.lock().await;
                    if current.is_some() {
                        self.send_error(id, -32020, "Session is busy").await?;
                        return Ok(());
                    }
                    *current = Some(request_id.clone());
                }

                let server = Arc::clone(&self);
                let request_id_spawn = request_id.clone();
                tokio::spawn(async move {
                    let server_for_run = Arc::clone(&server);
                    let request_id_for_run = request_id_spawn.clone();
                    if let Err(err) = server_for_run
                        .run_session_message(session, request_id_spawn, params.message)
                        .await
                    {
                        let _ = server
                            .send_notification(
                                "error/event",
                                json!({
                                    "session_id": params.session_id,
                                    "request_id": request_id_for_run,
                                    "code": "internal_error",
                                    "message": err.to_string(),
                                }),
                            )
                            .await;
                    }
                });

                json!({
                    "accepted": true,
                    "request_id": request_id,
                })
            }
            "session/cancel" => {
                let params: CancelParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/cancel")?;
                let session = self.get_session(&params.session_id).await?;

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

                json!({ "cancelled": cancelled })
            }
            "session/close" => {
                let params: CloseParams = serde_json::from_value(req.params)
                    .context("Invalid params for session/close")?;
                self.sessions.lock().await.remove(&params.session_id);
                json!({ "closed": true })
            }
            _ => {
                self.send_error(id, -32601, format!("Method '{}' not found", method))
                    .await?;
                return Ok(());
            }
        };

        self.send_response(id, response).await
    }

    async fn run_session_message(
        self: Arc<Self>,
        session: Arc<AcpSession>,
        request_id: String,
        message: String,
    ) -> Result<()> {
        let session_id = session.id.clone();

        self.send_notification("session/status", self.status_value(&session, true).await)
            .await?;

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
                .await?
        };

        self.send_notification(
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
        .await?;

        self.send_notification("session/status", self.status_value(&session, false).await)
            .await?;

        session.interrupt.store(false, Ordering::SeqCst);
        *session.current_request.lock().await = None;
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
    if context.active_file.is_some() || context.cursor.is_some() || context.selection.is_some() {
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
    let mut reader = BufReader::new(stdin).lines();

    while let Some(line) = reader.next_line().await? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = serde_json::from_str::<JsonRpcRequest>(trimmed);
        let req = match parsed {
            Ok(r) => r,
            Err(err) => {
                tracing::warn!("ACP parse error: {}", err);
                continue;
            }
        };

        if req.jsonrpc.as_deref().is_some_and(|v| v != "2.0") {
            server
                .send_error(req.id, -32600, "Invalid JSON-RPC version")
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
    }
}
