//! Messaging channel integration (WASM plugins)
//!
//! Channel plugins translate external messages (Slack, Discord, Signal)
//! into tark chat requests and send responses back to those channels.

pub mod remote;

use crate::agent::{ChatAgent, ToolCallLog};
use crate::channels::remote::{normalize_message_preview, RemoteEvent, RemoteRuntime};
use crate::config::Config;
use crate::core::types::AgentMode;
use crate::llm;
use crate::plugins::{
    ChannelInboundMessage, ChannelInfo, ChannelSendRequest, ChannelSendResult,
    ChannelWebhookRequest, ChannelWebhookResponse, InstalledPlugin, PluginHost, PluginRegistry,
    PluginType,
};
use crate::secure_store;
use crate::storage::usage::{UsageLog, UsageTracker};
use crate::storage::{ChatSession, TarkStorage};
use crate::tools::{ToolRegistry, TrustLevel};
use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

const STREAM_DEBOUNCE: Duration = Duration::from_millis(250);
const STREAM_MIN_CHARS: usize = 200;
const METADATA_RAW_LIMIT: usize = 2048;

#[derive(Debug, Serialize)]
struct ToolSummary {
    tool_calls: usize,
    tools_used: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ChannelToolMetadata<'a> {
    tool_calls_made: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_summary: Option<ToolSummary>,
    #[serde(skip_serializing_if = "tool_log_is_empty")]
    tool_log: &'a [ToolCallLog],
    #[serde(skip_serializing_if = "Option::is_none")]
    inbound_raw: Option<String>,
}

#[derive(Clone)]
pub struct ChannelManager {
    config: Config,
    working_dir: PathBuf,
    storage: Option<Arc<TarkStorage>>,
    usage_tracker: Option<Arc<UsageTracker>>,
    remote: Option<Arc<RemoteRuntime>>,
}

impl ChannelManager {
    pub fn new(
        config: Config,
        working_dir: PathBuf,
        storage: Option<Arc<TarkStorage>>,
        usage_tracker: Option<Arc<UsageTracker>>,
        remote: Option<Arc<RemoteRuntime>>,
    ) -> Self {
        Self {
            config,
            working_dir,
            storage,
            usage_tracker,
            remote,
        }
    }

    pub async fn start_all(&self) -> Result<()> {
        let registry = PluginRegistry::new()?;
        for plugin in registry.by_type(PluginType::Channel) {
            if !plugin.enabled {
                continue;
            }
            if let Some(remote) = &self.remote {
                if !remote.allows_plugin(plugin.id()) {
                    continue;
                }
            }
            let plugin = plugin.clone();
            let manager = self.clone();
            let plugin_for_start = plugin.clone();
            tokio::task::spawn_blocking(move || manager.start_plugin(&plugin_for_start)).await??;

            if self.remote.is_some() {
                let manager = self.clone();
                let plugin = plugin.clone();
                tokio::spawn(async move {
                    manager.spawn_poll_loop(plugin).await;
                });
            }
        }
        Ok(())
    }

    async fn spawn_poll_loop(&self, plugin: InstalledPlugin) {
        if !self.plugin_supports_poll(&plugin) {
            return;
        }
        let interval_ms = self.config.remote.channel_poll_ms;
        let mut ticker = tokio::time::interval(Duration::from_millis(interval_ms));
        loop {
            ticker.tick().await;
            let manager = self.clone();
            let plugin_for_poll = plugin.clone();
            let result =
                tokio::task::spawn_blocking(move || manager.poll_plugin(&plugin_for_poll)).await;
            if let Ok(Ok(Some(messages))) = result {
                if !messages.is_empty() {
                    let manager = self.clone();
                    let plugin_id = plugin.id().to_string();
                    tokio::spawn(async move {
                        if let Err(err) =
                            manager.process_inbound_messages(&plugin_id, messages).await
                        {
                            tracing::error!("Channel poll message processing failed: {}", err);
                        }
                    });
                }
            }
        }
    }

    pub async fn handle_webhook(
        &self,
        plugin_id: &str,
        request: ChannelWebhookRequest,
    ) -> Result<ChannelWebhookResponse> {
        let plugin = self.load_channel_plugin(plugin_id)?;
        let response = tokio::task::spawn_blocking({
            let manager = self.clone();
            move || manager.invoke_webhook(&plugin, &request)
        })
        .await??;

        if !response.messages.is_empty() {
            let messages = response.messages.clone();
            let manager = self.clone();
            let plugin_id = plugin_id.to_string();
            tokio::spawn(async move {
                if let Err(err) = manager.process_inbound_messages(&plugin_id, messages).await {
                    tracing::error!("Channel message processing failed: {}", err);
                }
            });
        }

        Ok(response)
    }

    pub async fn handle_gateway_event(&self, plugin_id: &str, payload_json: &str) -> Result<()> {
        let plugin = self.load_channel_plugin(plugin_id)?;
        let payload = payload_json.to_string();
        let messages = tokio::task::spawn_blocking({
            let manager = self.clone();
            move || manager.invoke_gateway_event(&plugin, &payload)
        })
        .await??;

        if !messages.is_empty() {
            let manager = self.clone();
            let plugin_id = plugin_id.to_string();
            tokio::spawn(async move {
                if let Err(err) = manager.process_inbound_messages(&plugin_id, messages).await {
                    tracing::error!("Channel gateway message processing failed: {}", err);
                }
            });
        }

        Ok(())
    }

    fn start_plugin(&self, plugin: &InstalledPlugin) -> Result<()> {
        self.with_channel_instance(plugin, |instance| instance.channel_start())
    }

    fn invoke_webhook(
        &self,
        plugin: &InstalledPlugin,
        request: &ChannelWebhookRequest,
    ) -> Result<ChannelWebhookResponse> {
        self.with_channel_instance(plugin, |instance| instance.channel_handle_webhook(request))
    }

    fn poll_plugin(&self, plugin: &InstalledPlugin) -> Result<Option<Vec<ChannelInboundMessage>>> {
        self.with_channel_instance(plugin, |instance| {
            if !instance.has_channel_poll() {
                return Ok(None);
            }
            let messages = instance.channel_poll()?;
            Ok(Some(messages))
        })
    }

    fn plugin_supports_poll(&self, plugin: &InstalledPlugin) -> bool {
        self.with_channel_instance(plugin, |instance| Ok(instance.has_channel_poll()))
            .unwrap_or(false)
    }

    fn invoke_gateway_event(
        &self,
        plugin: &InstalledPlugin,
        payload_json: &str,
    ) -> Result<Vec<ChannelInboundMessage>> {
        self.with_channel_instance(plugin, |instance| {
            if !instance.has_channel_gateway_handler() {
                anyhow::bail!("Plugin does not support gateway events");
            }
            instance.channel_handle_gateway_event(payload_json)
        })
    }

    fn load_channel_plugin(&self, plugin_id: &str) -> Result<InstalledPlugin> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;
        if let Some(remote) = &self.remote {
            if !remote.allows_plugin(plugin_id) {
                anyhow::bail!(
                    "Plugin '{}' is not allowed for this remote runtime",
                    plugin_id
                );
            }
        }
        if plugin.plugin_type() != PluginType::Channel {
            anyhow::bail!("Plugin '{}' is not a channel plugin", plugin_id);
        }
        if !plugin.enabled {
            anyhow::bail!("Plugin '{}' is disabled", plugin_id);
        }
        Ok(plugin.clone())
    }

    fn with_channel_instance<R>(
        &self,
        plugin: &InstalledPlugin,
        f: impl FnOnce(&mut crate::plugins::PluginInstance) -> Result<R>,
    ) -> Result<R> {
        let mut host = PluginHost::new()?;
        if let Some(local_dir) = self.local_plugin_data_dir(plugin.id()) {
            if local_dir.exists()
                || self
                    .local_oauth_credentials_path(plugin.id())
                    .as_ref()
                    .map(|p| p.exists())
                    .unwrap_or(false)
            {
                host.load_with_data_dir(plugin, local_dir)?;
            } else {
                host.load(plugin)?;
            }
        } else {
            host.load(plugin)?;
        }
        let instance = host
            .get_mut(plugin.id())
            .ok_or_else(|| anyhow::anyhow!("Failed to load plugin '{}'", plugin.id()))?;

        if let Some(creds) = self.load_oauth_credentials(plugin)? {
            if instance.has_channel_auth_init() {
                instance.channel_auth_init(&creds)?;
            }
        }

        f(instance)
    }

    fn load_oauth_credentials(&self, plugin: &InstalledPlugin) -> Result<Option<String>> {
        let oauth = match &plugin.manifest.oauth {
            Some(config) => config,
            None => return Ok(None),
        };

        if let Some(local_path) = self.local_oauth_credentials_path(plugin.id()) {
            if local_path.exists() {
                let content = std::fs::read_to_string(&local_path)
                    .with_context(|| format!("Failed to read {}", local_path.display()))?;
                return Ok(Some(content));
            }
        }

        let creds_path = if let Some(path) = &oauth.credentials_path {
            expand_path(path)
        } else {
            default_oauth_credentials_path(plugin.id())?
        };

        if !creds_path.exists() {
            return Ok(None);
        }

        let content = secure_store::read_maybe_encrypted(&creds_path)
            .with_context(|| format!("Failed to read {}", creds_path.display()))?;
        Ok(Some(content))
    }

    fn local_plugin_data_dir(&self, plugin_id: &str) -> Option<PathBuf> {
        let storage = self.storage.as_ref()?;
        Some(
            storage
                .project_root()
                .join("plugins")
                .join(plugin_id)
                .join("data"),
        )
    }

    fn local_oauth_credentials_path(&self, plugin_id: &str) -> Option<PathBuf> {
        let storage = self.storage.as_ref()?;
        Some(
            storage
                .project_root()
                .join("plugins")
                .join(plugin_id)
                .join("oauth.json"),
        )
    }

    async fn process_inbound_messages(
        &self,
        plugin_id: &str,
        messages: Vec<ChannelInboundMessage>,
    ) -> Result<()> {
        for message in messages {
            if let Err(err) = self.process_inbound_message(plugin_id, &message).await {
                tracing::error!(
                    "Failed to handle channel message {}: {}",
                    message.conversation_id,
                    err
                );
            }
        }
        Ok(())
    }

    async fn process_inbound_message(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
    ) -> Result<()> {
        let channel_info = self.channel_info(plugin_id).await?;
        let session_id = channel_session_id(plugin_id, &message.conversation_id);

        let remote_ctx = parse_remote_context(&message.metadata_json);
        if let Some(remote) = &self.remote {
            if remote.registry().is_stopped(&session_id) {
                let send_request = ChannelSendRequest {
                    conversation_id: message.conversation_id.clone(),
                    text: "This session is stopped. Use /tark resume to continue.".to_string(),
                    message_id: None,
                    is_final: true,
                    metadata_json: message.metadata_json.clone(),
                };
                let _ = self.send_channel_message(plugin_id, &send_request).await?;
                return Ok(());
            }

            if !self.remote_allowed(plugin_id, &remote_ctx) {
                let send_request = ChannelSendRequest {
                    conversation_id: message.conversation_id.clone(),
                    text: "Unauthorized remote request.".to_string(),
                    message_id: None,
                    is_final: true,
                    metadata_json: message.metadata_json.clone(),
                };
                let _ = self.send_channel_message(plugin_id, &send_request).await?;
                remote.emit(
                    RemoteEvent::new(
                        "error:unauthorized",
                        plugin_id,
                        &session_id,
                        &message.conversation_id,
                        remote.runtime_id(),
                    )
                    .with_user(remote_ctx.user_id.clone())
                    .with_message(normalize_message_preview(&message.text)),
                );
                return Ok(());
            }
        }

        let mut session = self.load_session(&session_id)?;
        if session.name.is_empty() {
            session.set_name_from_prompt(&message.text);
        }

        let default_mode = self.default_remote_mode();
        if session.mode.is_empty()
            || (self.remote.is_some() && session.messages.is_empty() && session.mode == "build")
        {
            session.mode = default_mode.to_string();
        }
        let trust_level =
            trust_from_approval_mode(&session.approval_mode).unwrap_or(self.default_remote_trust());
        if self.remote.is_some()
            && session.messages.is_empty()
            && (session.approval_mode.is_empty() || session.approval_mode == "ask_risky")
        {
            session.approval_mode = approval_mode_from_trust(trust_level).to_string();
        }

        if let Some(remote) = &self.remote {
            let _ = remote.registry().update_context(
                &session_id,
                remote.runtime_id(),
                plugin_id,
                &message.conversation_id,
                Some(session.provider.clone()),
                Some(session.model.clone()),
                Some(session.mode.clone()),
                Some(trust_level.label().to_lowercase()),
                remote_ctx.user_id.clone(),
                remote_ctx.channel_id.clone(),
                remote_ctx.guild_id.clone(),
            );
            remote.emit(
                RemoteEvent::new(
                    "inbound",
                    plugin_id,
                    &session_id,
                    &message.conversation_id,
                    remote.runtime_id(),
                )
                .with_user(remote_ctx.user_id.clone())
                .with_message(normalize_message_preview(&message.text)),
            );
            let _ = remote.registry().set_last_message(
                &session_id,
                remote.runtime_id(),
                plugin_id,
                &message.conversation_id,
                Some(normalize_message_preview(&message.text)),
            );
        }

        if let Some(cmd) = parse_remote_command(message, &remote_ctx) {
            let handled = self
                .handle_remote_command(plugin_id, message, &mut session, cmd, &remote_ctx)
                .await?;
            if handled {
                self.save_session(&session)?;
                return Ok(());
            }
        }

        let provider_name = if session.provider.is_empty() {
            self.config.llm.default_provider.clone()
        } else {
            session.provider.clone()
        };

        let model_override = if session.model.is_empty() {
            None
        } else {
            Some(session.model.as_str())
        };

        let provider = llm::create_provider_with_options(&provider_name, true, model_override)?;
        let provider: Arc<dyn llm::LlmProvider> = Arc::from(provider);

        let mut tools =
            ToolRegistry::with_defaults(self.working_dir.clone(), self.config.tools.shell_enabled);
        tools.set_tool_timeout_secs(self.config.tools.tool_timeout_secs);

        let agent_mode: AgentMode = session.mode.parse().unwrap_or(AgentMode::Ask);
        let mut agent = ChatAgent::with_mode(provider, tools, agent_mode)
            .with_max_iterations(self.config.agent.max_iterations);
        agent.set_thinking_config(self.config.thinking.clone());
        agent.set_think_level_sync(self.config.thinking.effective_default_level_name());
        agent.set_trust_level(trust_level).await;
        agent.restore_from_session(&session);

        if let Some(remote) = &self.remote {
            let _ = remote.registry().mark_status(
                &session_id,
                remote.runtime_id(),
                plugin_id,
                &message.conversation_id,
                "running",
            );
            remote.emit(RemoteEvent::new(
                "agent_start",
                plugin_id,
                &session_id,
                &message.conversation_id,
                remote.runtime_id(),
            ));
        }

        let response = if channel_info.supports_streaming {
            Some(
                self.respond_streaming(plugin_id, message, &mut agent)
                    .await?,
            )
        } else {
            Some(agent.chat(&message.text).await?)
        };

        if let Some(response) = response.as_ref() {
            let summary = build_tool_summary(response.tool_calls_made, &response.tool_call_log);
            let send_request = ChannelSendRequest {
                conversation_id: message.conversation_id.clone(),
                text: append_tool_summary(response.text.clone(), summary.as_ref()),
                message_id: None,
                is_final: true,
                metadata_json: build_metadata_json(
                    &message.metadata_json,
                    response.tool_calls_made,
                    &response.tool_call_log,
                ),
            };
            let _ = self.send_channel_message(plugin_id, &send_request).await?;
        }

        session.messages = agent.get_messages_for_session();
        session.updated_at = Utc::now();
        session.provider = provider_name.clone();
        session.mode = agent_mode.to_string();
        session.approval_mode = approval_mode_from_trust(trust_level).to_string();

        if let Some(response) = response {
            self.apply_usage(
                &mut session,
                plugin_id,
                &provider_name,
                response.usage.as_ref(),
                &message.conversation_id,
            )
            .await?;
        }

        if let Some(remote) = &self.remote {
            let _ = remote.registry().mark_status(
                &session_id,
                remote.runtime_id(),
                plugin_id,
                &message.conversation_id,
                "idle",
            );
            remote.emit(RemoteEvent::new(
                "agent_done",
                plugin_id,
                &session_id,
                &message.conversation_id,
                remote.runtime_id(),
            ));
        }

        self.save_session(&session)?;

        Ok(())
    }

    async fn respond_streaming(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
        agent: &mut ChatAgent,
    ) -> Result<crate::agent::AgentResponse> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let tx_stream = tx.clone();
        let final_text = Arc::new(std::sync::Mutex::new(String::new()));
        let final_text_clone = Arc::clone(&final_text);
        let message_id = Arc::new(std::sync::Mutex::new(None::<String>));
        let message_id_clone = Arc::clone(&message_id);
        let manager = self.clone();
        let plugin_id = plugin_id.to_string();
        let plugin_id_clone = plugin_id.clone();
        let conversation_id = message.conversation_id.clone();
        let metadata_json = message.metadata_json.clone();

        let sender = tokio::spawn(async move {
            let mut accumulated = String::new();
            let mut last_sent = Instant::now();
            let mut message_id_local: Option<String> = None;

            while let Some(chunk) = rx.recv().await {
                accumulated.push_str(&chunk);
                let should_send =
                    accumulated.len() >= STREAM_MIN_CHARS || last_sent.elapsed() >= STREAM_DEBOUNCE;
                if !should_send {
                    continue;
                }

                let send_request = ChannelSendRequest {
                    conversation_id: conversation_id.clone(),
                    text: accumulated.clone(),
                    message_id: message_id_local.clone(),
                    is_final: false,
                    metadata_json: metadata_json.clone(),
                };
                if let Ok(result) = manager
                    .send_channel_message(&plugin_id_clone, &send_request)
                    .await
                {
                    if message_id_local.is_none() {
                        message_id_local = result.message_id;
                    }
                }

                last_sent = Instant::now();
            }
            if let Ok(mut guard) = message_id_clone.lock() {
                if guard.is_none() {
                    *guard = message_id_local;
                }
            }
        });

        let response = agent
            .chat_streaming(
                &message.text,
                || false,
                move |chunk| {
                    if let Ok(mut guard) = final_text_clone.lock() {
                        guard.push_str(&chunk);
                    }
                    let _ = tx_stream.send(chunk);
                },
                |_| {},
                |_, _| {},
                |_, _, _| {},
                |_| {},
            )
            .await?;

        drop(tx);
        let _ = sender.await;

        let summary = build_tool_summary(response.tool_calls_made, &response.tool_call_log);
        let fallback_text = final_text
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let (response_text, response_was_empty) =
            finalize_response_text(&response.text, &fallback_text, summary.as_ref());
        let response_message_id = message_id.lock().ok().and_then(|guard| guard.clone());
        let final_request = ChannelSendRequest {
            conversation_id: message.conversation_id.clone(),
            text: response_text,
            message_id: response_message_id,
            is_final: true,
            metadata_json: build_metadata_json(
                &message.metadata_json,
                response.tool_calls_made,
                &response.tool_call_log,
            ),
        };
        let _ = self
            .send_channel_message(&plugin_id, &final_request)
            .await?;

        if response_was_empty {
            tracing::warn!("Streaming response was empty");
        }

        Ok(response)
    }

    async fn channel_info(&self, plugin_id: &str) -> Result<ChannelInfo> {
        let plugin = self.load_channel_plugin(plugin_id)?;
        tokio::task::spawn_blocking({
            let manager = self.clone();
            move || manager.with_channel_instance(&plugin, |instance| instance.channel_info())
        })
        .await?
    }

    async fn send_channel_message(
        &self,
        plugin_id: &str,
        request: &ChannelSendRequest,
    ) -> Result<ChannelSendResult> {
        let plugin = self.load_channel_plugin(plugin_id)?;
        let request = request.clone();
        let request_for_send = request.clone();
        let result = tokio::task::spawn_blocking({
            let manager = self.clone();
            move || {
                manager.with_channel_instance(&plugin, |instance| {
                    instance.channel_send(&request_for_send)
                })
            }
        })
        .await?;

        if let Some(remote) = &self.remote {
            remote.emit(
                RemoteEvent::new(
                    "outbound",
                    plugin_id,
                    channel_session_id(plugin_id, &request.conversation_id),
                    &request.conversation_id,
                    remote.runtime_id(),
                )
                .with_message(normalize_message_preview(&request.text)),
            );
        }

        result
    }

    fn load_session(&self, session_id: &str) -> Result<ChatSession> {
        if let Some(storage) = &self.storage {
            if let Ok(session) = storage.load_session(session_id) {
                return Ok(session);
            }
        }

        let mut session = ChatSession::new();
        session.id = session_id.to_string();
        Ok(session)
    }

    fn save_session(&self, session: &ChatSession) -> Result<()> {
        if let Some(storage) = &self.storage {
            storage.save_session(session)?;
        }
        Ok(())
    }

    fn default_remote_mode(&self) -> AgentMode {
        if self.remote.is_none() {
            return AgentMode::Build;
        }
        self.config
            .remote
            .default_mode
            .parse()
            .unwrap_or(AgentMode::Ask)
    }

    fn default_remote_trust(&self) -> TrustLevel {
        if self.remote.is_none() {
            return TrustLevel::default();
        }
        self.config
            .remote
            .default_trust_level
            .parse()
            .unwrap_or(TrustLevel::Manual)
    }

    fn remote_allowed(&self, plugin_id: &str, ctx: &RemoteContext) -> bool {
        if self.remote.is_none() {
            return true;
        }
        let cfg = &self.config.remote;
        if !cfg.allowed_plugins.is_empty() && !cfg.allowed_plugins.contains(&plugin_id.to_string())
        {
            return false;
        }
        if cfg.require_allowlist
            && cfg.allowed_users.is_empty()
            && cfg.allowed_channels.is_empty()
            && cfg.allowed_guilds.is_empty()
            && cfg.allowed_roles.is_empty()
        {
            return false;
        }
        if !cfg.allowed_users.is_empty() {
            if let Some(user) = &ctx.user_id {
                if !cfg.allowed_users.contains(user) {
                    return false;
                }
            } else {
                return false;
            }
        }
        if !cfg.allowed_channels.is_empty() {
            if let Some(channel) = &ctx.channel_id {
                if !cfg.allowed_channels.contains(channel) {
                    return false;
                }
            } else {
                return false;
            }
        }
        if !cfg.allowed_guilds.is_empty() {
            if let Some(guild) = &ctx.guild_id {
                if !cfg.allowed_guilds.contains(guild) {
                    return false;
                }
            } else {
                return false;
            }
        }
        if !cfg.allowed_roles.is_empty()
            && ctx
                .roles
                .iter()
                .all(|role| !cfg.allowed_roles.contains(role))
        {
            return false;
        }
        true
    }

    async fn handle_remote_command(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
        session: &mut ChatSession,
        cmd: RemoteCommand,
        ctx: &RemoteContext,
    ) -> Result<bool> {
        let Some(remote) = &self.remote else {
            return Ok(false);
        };

        let mut response = match cmd {
            RemoteCommand::Help => Some(
                "Commands: /tark status | /tark mode <ask|plan|build> | /tark model <id> | /tark provider <id> | /tark trust <manual|careful|balanced> | /tark usage | /tark stop | /tark resume".to_string(),
            ),
            RemoteCommand::Status => Some(format!(
                "session={} mode={} trust={} provider={} model={}",
                session.id,
                session.mode,
                trust_from_approval_mode(&session.approval_mode)
                    .unwrap_or(self.default_remote_trust())
                    .label(),
                if session.provider.is_empty() {
                    self.config.llm.default_provider.clone()
                } else {
                    session.provider.clone()
                },
                if session.model.is_empty() {
                    "(default)".to_string()
                } else {
                    session.model.clone()
                }
            )),
            RemoteCommand::Stop => {
                remote.registry().stop_session(&session.id)?;
                Some("Session stopped.".to_string())
            }
            RemoteCommand::Resume => {
                remote.registry().resume_session(&session.id)?;
                Some("Session resumed.".to_string())
            }
            RemoteCommand::Mode(mode) => {
                if !self.config.remote.allow_mode_change {
                    Some("Mode changes are disabled.".to_string())
                } else {
                    session.mode = mode.to_string();
                    Some(format!("Mode set to {}.", mode))
                }
            }
            RemoteCommand::Model(model) => {
                if !self.config.remote.allow_model_change {
                    Some("Model changes are disabled.".to_string())
                } else if !self.model_allowed(&model) {
                    Some("Model not allowed.".to_string())
                } else {
                    session.model = model.clone();
                    Some(format!("Model set to {}.", model))
                }
            }
            RemoteCommand::Provider(provider) => {
                if !self.config.remote.allow_provider_change {
                    Some("Provider changes are disabled.".to_string())
                } else if !self.provider_allowed(&provider) {
                    Some("Provider not allowed.".to_string())
                } else {
                    session.provider = provider.clone();
                    Some(format!("Provider set to {}.", provider))
                }
            }
            RemoteCommand::Trust(level) => {
                if !self.config.remote.allow_trust_change {
                    Some("Trust level changes are disabled.".to_string())
                } else {
                    session.approval_mode = approval_mode_from_trust(level).to_string();
                    Some(format!("Trust level set to {}.", level.label()))
                }
            }
            RemoteCommand::Usage => Some(format!(
                "Session usage: input={} output={} total_cost=${:.4}",
                session.input_tokens,
                session.output_tokens,
                session.total_cost
            )),
        };

        if let Some(remote) = &self.remote {
            let _ = remote.registry().update_context(
                &session.id,
                remote.runtime_id(),
                plugin_id,
                &message.conversation_id,
                Some(session.provider.clone()),
                Some(session.model.clone()),
                Some(session.mode.clone()),
                trust_from_approval_mode(&session.approval_mode).map(|t| t.label().to_lowercase()),
                ctx.user_id.clone(),
                ctx.channel_id.clone(),
                ctx.guild_id.clone(),
            );
        }

        if let Some(text) = response.take() {
            let send_request = ChannelSendRequest {
                conversation_id: message.conversation_id.clone(),
                text,
                message_id: None,
                is_final: true,
                metadata_json: message.metadata_json.clone(),
            };
            let _ = self.send_channel_message(plugin_id, &send_request).await?;
        }

        Ok(true)
    }

    async fn apply_usage(
        &self,
        session: &mut ChatSession,
        plugin_id: &str,
        provider_name: &str,
        usage: Option<&crate::llm::TokenUsage>,
        conversation_id: &str,
    ) -> Result<()> {
        let usage = match usage {
            Some(usage) => usage,
            None => return Ok(()),
        };

        let input_tokens = usage.input_tokens as usize;
        let output_tokens = usage.output_tokens as usize;
        let total_tokens = input_tokens + output_tokens;

        let model_name = if session.model.is_empty() {
            self.default_model_for_provider(provider_name)
        } else {
            session.model.clone()
        };

        let calculated_cost = if let Some(tracker) = &self.usage_tracker {
            tracker
                .calculate_cost(
                    provider_name,
                    &model_name,
                    usage.input_tokens,
                    usage.output_tokens,
                )
                .await
        } else {
            0.0
        };

        session.input_tokens += input_tokens;
        session.output_tokens += output_tokens;
        session.total_cost += calculated_cost;

        if let Some(entry) = session
            .tokens_by_model
            .iter_mut()
            .find(|(name, _)| name == &model_name)
        {
            entry.1 += total_tokens;
        } else {
            session
                .tokens_by_model
                .push((model_name.clone(), total_tokens));
        }

        if let Some(entry) = session
            .cost_by_model
            .iter_mut()
            .find(|(name, _)| name == &model_name)
        {
            entry.1 += calculated_cost;
        } else {
            session
                .cost_by_model
                .push((model_name.clone(), calculated_cost));
        }

        if let Some(tracker) = &self.usage_tracker {
            let _ = tracker.log_usage(UsageLog {
                session_id: session.id.clone(),
                provider: provider_name.to_string(),
                model: model_name.clone(),
                mode: session.mode.clone(),
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cost_usd: calculated_cost,
                request_type: "channel".to_string(),
                estimated: false,
            });
        }

        if let Some(remote) = &self.remote {
            remote.emit(
                RemoteEvent::new(
                    "usage",
                    plugin_id,
                    &session.id,
                    conversation_id,
                    remote.runtime_id(),
                )
                .with_metadata(json!({
                    "provider": provider_name,
                    "model": model_name,
                    "input_tokens": usage.input_tokens,
                    "output_tokens": usage.output_tokens,
                    "cost_usd": calculated_cost,
                })),
            );
        }

        Ok(())
    }

    fn model_allowed(&self, model: &str) -> bool {
        let allowed = &self.config.remote.allowed_models;
        allowed.is_empty() || allowed.iter().any(|m| m == model)
    }

    fn provider_allowed(&self, provider: &str) -> bool {
        let allowed = &self.config.remote.allowed_providers;
        allowed.is_empty() || allowed.iter().any(|p| p == provider)
    }

    fn default_model_for_provider(&self, provider: &str) -> String {
        match provider {
            "openai" => self.config.llm.openai.model.clone(),
            "claude" | "anthropic" => self.config.llm.claude.model.clone(),
            "gemini" | "google" => self.config.llm.gemini.model.clone(),
            "ollama" => self.config.llm.ollama.model.clone(),
            "copilot" => self.config.llm.copilot.model.clone(),
            "openrouter" => self.config.llm.openrouter.model.clone(),
            _ => self.config.llm.tark_sim.model.clone(),
        }
    }
}

fn channel_session_id(plugin_id: &str, conversation_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plugin_id.as_bytes());
    hasher.update(b":");
    hasher.update(conversation_id.as_bytes());
    let hash = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("channel_{}_{}", plugin_id, hash)
}

fn default_oauth_credentials_path(plugin_id: &str) -> Result<PathBuf> {
    let config_dir = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config_dir
        .join("tark")
        .join(format!("{}_oauth.json", plugin_id)))
}

fn expand_path(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    } else if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

fn build_tool_summary(tool_calls_made: usize, tool_log: &[ToolCallLog]) -> Option<ToolSummary> {
    if tool_calls_made == 0 && tool_log.is_empty() {
        return None;
    }

    let mut seen = HashSet::new();
    let mut tools_used = Vec::new();
    for entry in tool_log {
        if seen.insert(entry.tool.clone()) {
            tools_used.push(entry.tool.clone());
        }
    }

    Some(ToolSummary {
        tool_calls: tool_calls_made.max(tool_log.len()),
        tools_used,
    })
}

fn append_tool_summary(text: String, summary: Option<&ToolSummary>) -> String {
    let Some(summary) = summary else {
        return text;
    };

    let summary_text = if summary.tools_used.is_empty() {
        format!("Tools used: {} call(s).", summary.tool_calls)
    } else {
        format!(
            "Tools used: {} ({} call(s)).",
            summary.tools_used.join(", "),
            summary.tool_calls
        )
    };

    let trimmed = text.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        summary_text
    } else {
        format!("{}\n\n{}", trimmed, summary_text)
    }
}

fn finalize_response_text(
    response_text: &str,
    fallback_text: &str,
    summary: Option<&ToolSummary>,
) -> (String, bool) {
    let response_was_empty = response_text.is_empty();
    let mut final_text = if response_was_empty {
        fallback_text.to_string()
    } else {
        response_text.to_string()
    };
    final_text = append_tool_summary(final_text, summary);
    (final_text, response_was_empty)
}

fn build_metadata_json(
    inbound_metadata: &str,
    tool_calls_made: usize,
    tool_log: &[ToolCallLog],
) -> String {
    let mut parse_failed = false;
    let mut root = match serde_json::from_str::<Value>(inbound_metadata) {
        Ok(Value::Object(map)) => Value::Object(map),
        Ok(other) => {
            let mut map = Map::new();
            map.insert("inbound".to_string(), other);
            Value::Object(map)
        }
        Err(_) => {
            parse_failed = true;
            Value::Object(Map::new())
        }
    };

    let summary = build_tool_summary(tool_calls_made, tool_log);
    let inbound_raw = if parse_failed && !inbound_metadata.is_empty() {
        let trimmed = inbound_metadata.chars().take(METADATA_RAW_LIMIT).collect();
        Some(trimmed)
    } else {
        None
    };

    let metadata = ChannelToolMetadata {
        tool_calls_made,
        tool_summary: summary,
        tool_log,
        inbound_raw,
    };

    let mut tark_value = serde_json::to_value(metadata).unwrap_or_else(|_| json!({}));
    if !tool_log.is_empty() || tool_calls_made > 0 {
        if let Value::Object(map) = &mut tark_value {
            map.insert("has_tools".to_string(), Value::Bool(true));
        }
    }

    if let Value::Object(map) = &mut root {
        map.insert("tark".to_string(), tark_value);
    }

    serde_json::to_string(&root).unwrap_or_else(|_| "{}".to_string())
}

fn tool_log_is_empty(log: &[ToolCallLog]) -> bool {
    log.is_empty()
}

#[derive(Debug, Clone)]
enum RemoteCommand {
    Help,
    Status,
    Stop,
    Resume,
    Mode(AgentMode),
    Model(String),
    Provider(String),
    Trust(TrustLevel),
    Usage,
}

#[derive(Debug, Default, Clone)]
struct RemoteContext {
    user_id: Option<String>,
    channel_id: Option<String>,
    guild_id: Option<String>,
    roles: Vec<String>,
    command: Option<RemoteCommand>,
    metadata: Option<Value>,
}

fn parse_remote_context(metadata_json: &str) -> RemoteContext {
    if metadata_json.trim().is_empty() {
        return RemoteContext::default();
    }

    let value = serde_json::from_str::<Value>(metadata_json).ok();
    let mut ctx = RemoteContext {
        metadata: value.clone(),
        ..RemoteContext::default()
    };

    let root = match value {
        Some(Value::Object(map)) => Value::Object(map),
        Some(other) => {
            ctx.metadata = Some(other);
            return ctx;
        }
        None => return ctx,
    };

    let mut user_id = root
        .get("user_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut channel_id = root
        .get("channel_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut guild_id = root
        .get("guild_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut roles = parse_roles(root.get("roles"));
    let mut command = parse_command_from_value(root.get("tark_command"));

    if let Some(Value::Object(discord)) = root.get("discord") {
        if user_id.is_none() {
            user_id = discord
                .get("user_id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if channel_id.is_none() {
            channel_id = discord
                .get("channel_id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if guild_id.is_none() {
            guild_id = discord
                .get("guild_id")
                .and_then(Value::as_str)
                .map(str::to_string);
        }
        if roles.is_empty() {
            roles = parse_roles(discord.get("roles"));
        }
        if command.is_none() {
            command = parse_command_from_value(discord.get("tark_command"));
        }
    }

    ctx.user_id = user_id;
    ctx.channel_id = channel_id;
    ctx.guild_id = guild_id;
    ctx.roles = roles;
    ctx.command = command;
    ctx
}

fn parse_roles(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_remote_command(
    message: &ChannelInboundMessage,
    ctx: &RemoteContext,
) -> Option<RemoteCommand> {
    if let Some(cmd) = ctx.command.clone() {
        return Some(cmd);
    }
    parse_command_from_text(&message.text)
}

fn parse_command_from_value(value: Option<&Value>) -> Option<RemoteCommand> {
    let value = value?;
    let obj = value.as_object()?;
    let name = obj.get("name").and_then(Value::as_str)?;
    let arg_value = obj
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| obj.get("arg").and_then(Value::as_str).map(str::to_string));
    parse_command_tokens(name, arg_value.as_deref())
}

fn parse_command_from_text(text: &str) -> Option<RemoteCommand> {
    let trimmed = text.trim();
    let trimmed = trimmed.strip_prefix('/').unwrap_or(trimmed);
    let mut parts = trimmed.split_whitespace();
    let head = parts.next()?;
    if head.to_lowercase() != "tark" {
        return None;
    }
    let cmd = parts.next().unwrap_or("help");
    let arg = parts.next();
    parse_command_tokens(cmd, arg)
}

fn parse_command_tokens(cmd: &str, arg: Option<&str>) -> Option<RemoteCommand> {
    match cmd.to_lowercase().as_str() {
        "help" | "h" => Some(RemoteCommand::Help),
        "status" | "info" => Some(RemoteCommand::Status),
        "stop" => Some(RemoteCommand::Stop),
        "resume" => Some(RemoteCommand::Resume),
        "mode" => arg
            .and_then(|v| v.parse::<AgentMode>().ok())
            .map(RemoteCommand::Mode),
        "model" => arg.map(|v| RemoteCommand::Model(v.to_string())),
        "provider" => arg.map(|v| RemoteCommand::Provider(v.to_string())),
        "trust" => arg.and_then(parse_trust_level).map(RemoteCommand::Trust),
        "usage" => Some(RemoteCommand::Usage),
        _ => None,
    }
}

fn parse_trust_level(value: &str) -> Option<TrustLevel> {
    match value.to_lowercase().as_str() {
        "manual" | "zero_trust" => Some(TrustLevel::Manual),
        "careful" | "only_reads" => Some(TrustLevel::Careful),
        "balanced" | "ask_risky" => Some(TrustLevel::Balanced),
        _ => None,
    }
}

fn trust_from_approval_mode(value: &str) -> Option<TrustLevel> {
    if value.is_empty() {
        return None;
    }
    parse_trust_level(value)
}

fn approval_mode_from_trust(level: TrustLevel) -> &'static str {
    match level {
        TrustLevel::Manual => "zero_trust",
        TrustLevel::Careful => "only_reads",
        TrustLevel::Balanced => "ask_risky",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_session_id_is_stable() {
        let first = channel_session_id("slack", "thread-123");
        let second = channel_session_id("slack", "thread-123");
        assert_eq!(first, second);
        assert!(first.starts_with("channel_slack_"));
    }

    #[test]
    fn test_append_tool_summary() {
        let summary = ToolSummary {
            tool_calls: 2,
            tools_used: vec!["read_file".to_string(), "grep".to_string()],
        };
        let text = "Hello world".to_string();
        let result = append_tool_summary(text, Some(&summary));
        assert!(result.contains("Tools used: read_file, grep (2 call(s))."));
    }

    #[test]
    fn test_build_metadata_json_includes_tool_log() {
        let log = vec![ToolCallLog {
            tool: "read_file".to_string(),
            args: json!({"path": "README.md"}),
            result_preview: "ok".to_string(),
        }];
        let json_str = build_metadata_json("", 1, &log);
        let value: Value = serde_json::from_str(&json_str).unwrap();
        let tark = value.get("tark").expect("tark metadata missing");
        assert_eq!(tark.get("tool_calls_made").unwrap(), 1);
        assert!(tark.get("tool_log").is_some());
    }

    #[test]
    fn test_finalize_response_text_prefers_fallback_when_empty() {
        let summary = ToolSummary {
            tool_calls: 0,
            tools_used: Vec::new(),
        };
        let (text, was_empty) = finalize_response_text("", "fallback", Some(&summary));
        assert!(was_empty);
        assert!(text.contains("fallback"));
        assert!(text.contains("Tools used: 0 call(s)."));
    }
}
