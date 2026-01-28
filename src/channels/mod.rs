#![allow(dead_code)]
//! Messaging channel integration (WASM plugins)
//!
//! Channel plugins translate external messages (Slack, Discord, Signal)
//! into tark chat requests and send responses back to those channels.

pub mod remote;

use crate::agent::{ChatAgent, ToolCallLog};
use crate::channels::remote::{
    normalize_message_preview, QueuedRemoteMessage, RemoteEvent, RemoteRuntime,
};
use crate::config::Config;
use crate::core::attachments::{base64_encode, format_size, AttachmentContent, MessageAttachment};
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
use crate::tools::questionnaire::{AnswerValue, Questionnaire, UserResponse};
use crate::tools::{
    interaction_channel, ApprovalChoice, ApprovalPattern, ApprovalRequest, ApprovalResponse,
    InteractionRequest, MatchType, ToolRegistry, TrustLevel,
};
use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant as StdInstant;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::{Duration, Instant};

const STREAM_DEBOUNCE: Duration = Duration::from_millis(250);
const STREAM_MIN_CHARS: usize = 200;
const METADATA_RAW_LIMIT: usize = 2048;
static CHANNEL_POLL_SHUTDOWN: AtomicBool = AtomicBool::new(false);
const REMOTE_INTERACTION_TIMEOUT: Duration = Duration::from_secs(300);
const INBOUND_DEDUPE_TTL: Duration = Duration::from_secs(6);
const INBOUND_DEDUPE_MAX: usize = 64;

#[derive(Debug)]
enum PendingInteraction {
    Questionnaire {
        data: Questionnaire,
        responder: oneshot::Sender<UserResponse>,
        created_at: Instant,
    },
    Approval {
        request: ApprovalRequest,
        responder: oneshot::Sender<ApprovalResponse>,
        created_at: Instant,
    },
}

impl PendingInteraction {
    fn is_expired(&self) -> bool {
        let created_at = match self {
            PendingInteraction::Questionnaire { created_at, .. }
            | PendingInteraction::Approval { created_at, .. } => *created_at,
        };
        created_at.elapsed() > REMOTE_INTERACTION_TIMEOUT
    }
}

pub fn request_channel_shutdown() {
    CHANNEL_POLL_SHUTDOWN.store(true, Ordering::SeqCst);
}

pub fn reset_channel_shutdown() {
    CHANNEL_POLL_SHUTDOWN.store(false, Ordering::SeqCst);
}

#[derive(Debug, Serialize)]
struct ToolSummary {
    tool_calls: usize,
    tools_used: Vec<String>,
}

#[derive(Copy, Clone)]
enum RemoteResponseLabel {
    Answer,
    ToolOutput,
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
    remote_provider_override: Option<String>,
    remote_model_override: Option<String>,
    pending_interactions:
        Arc<std::sync::Mutex<std::collections::HashMap<String, PendingInteraction>>>,
    recent_inbound:
        Arc<std::sync::Mutex<std::collections::HashMap<String, VecDeque<InboundMarker>>>>,
}

#[derive(Clone, Debug)]
struct InboundMarker {
    key: String,
    seen_at: StdInstant,
}

struct RemoteRunGuard {
    remote: Option<Arc<RemoteRuntime>>,
    session_id: String,
    plugin_id: String,
    conversation_id: String,
    active: bool,
}

impl RemoteRunGuard {
    fn new(
        remote: Option<Arc<RemoteRuntime>>,
        session_id: String,
        plugin_id: String,
        conversation_id: String,
    ) -> Self {
        Self {
            remote,
            session_id,
            plugin_id,
            conversation_id,
            active: false,
        }
    }

    fn activate(&mut self) {
        self.active = true;
    }

    fn disarm(&mut self) {
        self.active = false;
    }
}

impl Drop for RemoteRunGuard {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(remote) = &self.remote {
            let _ = remote.registry().mark_status(
                &self.session_id,
                remote.runtime_id(),
                &self.plugin_id,
                &self.conversation_id,
                "idle",
            );
            remote.emit(RemoteEvent::new(
                "agent_done",
                &self.plugin_id,
                &self.session_id,
                &self.conversation_id,
                remote.runtime_id(),
            ));
        }
    }
}

impl ChannelManager {
    pub fn new(
        config: Config,
        working_dir: PathBuf,
        storage: Option<Arc<TarkStorage>>,
        usage_tracker: Option<Arc<UsageTracker>>,
        remote: Option<Arc<RemoteRuntime>>,
        remote_provider_override: Option<String>,
        remote_model_override: Option<String>,
    ) -> Self {
        Self {
            config,
            working_dir,
            storage,
            usage_tracker,
            remote,
            remote_provider_override,
            remote_model_override,
            pending_interactions: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
            recent_inbound: Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    pub async fn start_all(&self) -> Result<()> {
        reset_channel_shutdown();
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
            let supports_poll = self.remote.is_some() && self.plugin_supports_poll(&plugin);
            if !supports_poll {
                let manager = self.clone();
                let plugin_for_start = plugin.clone();
                tokio::task::spawn_blocking(move || manager.start_plugin(&plugin_for_start))
                    .await??;
            }

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
        let manager = self.clone();
        let runtime = tokio::runtime::Handle::current();
        tokio::task::spawn_blocking(move || {
            let mut host = match PluginHost::new() {
                Ok(host) => host,
                Err(err) => {
                    tracing::error!("Channel poll host init failed: {}", err);
                    return;
                }
            };

            if let Some(local_dir) = manager.local_plugin_data_dir(plugin.id()) {
                if local_dir.exists()
                    || manager
                        .local_oauth_credentials_path(plugin.id())
                        .as_ref()
                        .map(|p| p.exists())
                        .unwrap_or(false)
                {
                    if let Err(err) = host.load_with_data_dir(&plugin, local_dir) {
                        tracing::error!("Channel poll load failed: {}", err);
                        return;
                    }
                } else if let Err(err) = host.load(&plugin) {
                    tracing::error!("Channel poll load failed: {}", err);
                    return;
                }
            } else if let Err(err) = host.load(&plugin) {
                tracing::error!("Channel poll load failed: {}", err);
                return;
            }

            let instance = match host.get_mut(plugin.id()) {
                Some(instance) => instance,
                None => {
                    tracing::error!("Channel poll failed to load instance for {}", plugin.id());
                    return;
                }
            };

            if let Ok(Some(creds)) = manager.load_oauth_credentials(&plugin) {
                if instance.has_channel_auth_init() {
                    if let Err(err) = instance.channel_auth_init(&creds) {
                        tracing::warn!("Channel poll auth init failed: {}", err);
                    }
                }
            }

            if let Err(err) = instance.channel_start() {
                tracing::warn!("Channel poll start failed: {}", err);
            }

            if !instance.has_channel_poll() {
                return;
            }

            loop {
                if CHANNEL_POLL_SHUTDOWN.load(Ordering::SeqCst) {
                    break;
                }
                let messages = match instance.channel_poll() {
                    Ok(messages) => messages,
                    Err(err) => {
                        tracing::error!("Channel poll failed: {}", err);
                        Vec::new()
                    }
                };

                if !messages.is_empty() {
                    let manager = manager.clone();
                    let plugin_id = plugin.id().to_string();
                    runtime.spawn(async move {
                        if let Err(err) =
                            manager.process_inbound_messages(&plugin_id, messages).await
                        {
                            tracing::error!("Channel poll message processing failed: {}", err);
                        }
                    });
                }

                std::thread::sleep(Duration::from_millis(interval_ms));
            }
        });
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

    fn insert_pending_interaction(&self, conversation_id: String, pending: PendingInteraction) {
        if let Ok(mut guard) = self.pending_interactions.lock() {
            guard.insert(conversation_id, pending);
        }
    }

    fn take_pending_interaction(&self, conversation_id: &str) -> Option<PendingInteraction> {
        if let Ok(mut guard) = self.pending_interactions.lock() {
            if let Some(pending) = guard.remove(conversation_id) {
                if pending.is_expired() {
                    return None;
                }
                return Some(pending);
            }
        }
        None
    }

    fn has_pending_interaction(&self, conversation_id: &str) -> bool {
        if let Ok(mut guard) = self.pending_interactions.lock() {
            if let Some(pending) = guard.get(conversation_id) {
                if pending.is_expired() {
                    guard.remove(conversation_id);
                    return false;
                }
                return true;
            }
        }
        false
    }

    fn is_duplicate_inbound(&self, session_id: &str, message: &ChannelInboundMessage) -> bool {
        let key = inbound_dedupe_key(message);
        let now = StdInstant::now();
        let mut guard = match self.recent_inbound.lock() {
            Ok(guard) => guard,
            Err(err) => err.into_inner(),
        };
        let entries = guard.entry(session_id.to_string()).or_default();
        let ttl = INBOUND_DEDUPE_TTL;
        while let Some(front) = entries.front() {
            if now.duration_since(front.seen_at) > ttl {
                entries.pop_front();
            } else {
                break;
            }
        }
        if entries.iter().any(|entry| entry.key == key) {
            return true;
        }
        entries.push_back(InboundMarker { key, seen_at: now });
        if entries.len() > INBOUND_DEDUPE_MAX {
            entries.pop_front();
        }
        false
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
        if self.is_duplicate_inbound(&session_id, message) {
            tracing::debug!(
                "Skipping duplicate inbound message {}: {}",
                message.conversation_id,
                normalize_message_preview(&message.text)
            );
            return Ok(());
        }

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

        if let Some(pending) = self.take_pending_interaction(&message.conversation_id) {
            self.handle_pending_interaction_response(plugin_id, message, pending, &remote_ctx)
                .await?;
            return Ok(());
        }

        let cmd = parse_remote_command(message, &remote_ctx);
        let mut session = self.load_session(&session_id)?;
        if let Some(storage) = &self.storage {
            if let Ok(current) = storage.load_current_session() {
                if !current.mode.is_empty() {
                    session.mode = current.mode.clone();
                }
            }
        }
        if let Some(cmd) = cmd.clone() {
            let handled = self
                .handle_remote_command(plugin_id, message, &mut session, cmd, &remote_ctx)
                .await?;
            if handled {
                self.save_session(&session)?;
                return Ok(());
            }
        }
        if session.name.is_empty() {
            session.set_name_from_prompt(&message.text);
        }
        let previous_messages = session.messages.clone();

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

        let mut remote_guard = RemoteRunGuard::new(
            self.remote.clone(),
            session_id.clone(),
            plugin_id.to_string(),
            message.conversation_id.clone(),
        );
        if let Some(remote) = &self.remote {
            if cmd.is_none() {
                let started = remote.registry().try_mark_running(
                    &session_id,
                    remote.runtime_id(),
                    plugin_id,
                    &message.conversation_id,
                )?;
                if !started {
                    let preview = normalize_message_preview(&message.text);
                    let queued = remote.registry().enqueue_message(
                        &session_id,
                        QueuedRemoteMessage {
                            plugin_id: plugin_id.to_string(),
                            message: message.clone(),
                            received_at: Utc::now().to_rfc3339(),
                        },
                    )?;
                    remote.emit(
                        RemoteEvent::new(
                            "queued",
                            plugin_id,
                            &session_id,
                            &message.conversation_id,
                            remote.runtime_id(),
                        )
                        .with_user(remote_ctx.user_id.clone())
                        .with_message(preview.clone())
                        .with_metadata(serde_json::json!({
                            "count": queued,
                            "preview": preview,
                        })),
                    );
                    let queue_text = format!(
                        "⏳ Busy... queued at position {}. Use `/tark interrupt` to cancel the current task.",
                        queued
                    );
                    let _ = self
                        .send_channel_message(
                            plugin_id,
                            &ChannelSendRequest {
                                conversation_id: message.conversation_id.clone(),
                                text: queue_text,
                                message_id: None,
                                is_final: true,
                                metadata_json: message.metadata_json.clone(),
                            },
                        )
                        .await?;
                    return Ok(());
                }

                remote_guard.activate();
                let _ = remote.registry().clear_interrupt(&session_id);
                remote.emit(RemoteEvent::new(
                    "agent_start",
                    plugin_id,
                    &session_id,
                    &message.conversation_id,
                    remote.runtime_id(),
                ));
            }
        }

        if previous_messages.is_empty() {
            let header = format!(
                "── Session: {} · {} ──",
                if session.name.is_empty() {
                    "New session"
                } else {
                    session.name.as_str()
                },
                Utc::now().format("%Y-%m-%d %H:%M:%S UTC")
            );
            let _ = self
                .send_channel_message(
                    plugin_id,
                    &ChannelSendRequest {
                        conversation_id: message.conversation_id.clone(),
                        text: header,
                        message_id: None,
                        is_final: true,
                        metadata_json: message.metadata_json.clone(),
                    },
                )
                .await;
        }

        if let Some(provider) = &self.remote_provider_override {
            if self.provider_allowed(provider) {
                session.provider = provider.clone();
            }
        }
        if let Some(model) = &self.remote_model_override {
            if self.model_allowed(model) {
                session.model = model.clone();
            }
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

        let agent_mode: AgentMode = session.mode.parse().unwrap_or(AgentMode::Ask);
        let (interaction_tx, interaction_rx) = interaction_channel();

        let provider = llm::create_provider_with_options(&provider_name, true, model_override)?;
        let provider: Arc<dyn llm::LlmProvider> = Arc::from(provider);

        let mut tools = ToolRegistry::for_mode_with_interaction(
            self.working_dir.clone(),
            agent_mode,
            self.config.tools.shell_enabled,
            Some(interaction_tx),
        );
        tools.set_tool_timeout_secs(self.config.tools.tool_timeout_secs);

        let mut agent = ChatAgent::with_mode(provider, tools, agent_mode)
            .with_max_iterations(self.config.agent.max_iterations);
        agent.set_thinking_config(self.config.thinking.clone());
        agent.set_think_level_sync(self.config.thinking.effective_default_level_name());
        agent.set_trust_level(trust_level).await;
        agent.restore_from_session(&session);

        let attachments = parse_remote_attachments(&message.metadata_json);
        let (attachment_prompt, attachment_display) = build_remote_attachment_context(
            &attachments,
            &self.config.remote.attachments,
            &provider_name,
            model_override,
        )
        .await;
        let user_text = message.text.trim().to_string();
        let base_prompt = if user_text.is_empty() && !attachment_prompt.is_empty() {
            "Please analyze the attachment.".to_string()
        } else {
            message.text.clone()
        };
        let prompt_text = if attachment_prompt.is_empty() {
            base_prompt.clone()
        } else if base_prompt.trim().is_empty() {
            attachment_prompt.clone()
        } else {
            format!("{}\n\n{}", base_prompt, attachment_prompt)
        };

        let manager = self.clone();
        let plugin_id_owned = plugin_id.to_string();
        let conversation_id = message.conversation_id.clone();
        let metadata_json = message.metadata_json.clone();
        let remote_ctx = remote_ctx.clone();
        tokio::spawn(async move {
            manager
                .handle_interactions(
                    plugin_id_owned,
                    conversation_id,
                    interaction_rx,
                    metadata_json,
                    remote_ctx,
                )
                .await;
        });

        let mut initial_message_id = None;
        if channel_info.supports_edits {
            let working_request = ChannelSendRequest {
                conversation_id: message.conversation_id.clone(),
                text: "⏳ Working...".to_string(),
                message_id: None,
                is_final: false,
                metadata_json: message.metadata_json.clone(),
            };
            if let Ok(result) = self.send_channel_message(plugin_id, &working_request).await {
                initial_message_id = result.message_id;
            }
        }
        let can_stream = allow_remote_streaming(
            channel_info.supports_streaming,
            channel_info.supports_edits,
            initial_message_id.as_deref(),
        );

        let response = if can_stream {
            Some(
                self.respond_streaming(
                    plugin_id,
                    message,
                    &mut agent,
                    &prompt_text,
                    initial_message_id,
                    true,
                )
                .await?,
            )
        } else {
            Some(
                self.respond_streaming(
                    plugin_id,
                    message,
                    &mut agent,
                    &prompt_text,
                    initial_message_id,
                    false,
                )
                .await?,
            )
        };

        let mut response_text_override: Option<String> = None;
        if let Some(response) = response.as_ref() {
            let summary = build_tool_summary(response.tool_calls_made, &response.tool_call_log);
            let fallback_text = if !response.tool_call_log.is_empty() {
                "Tool activity sent below.".to_string()
            } else {
                "I’m ready when you are. Please ask a specific question.".to_string()
            };
            let (response_text, _was_empty) =
                finalize_response_text(&response.text, &fallback_text, summary.as_ref());
            if response.text.trim().is_empty() {
                if let Some(tool_text) = format_tool_log_for_remote(&response.tool_call_log) {
                    response_text_override = Some(format!("Tool result:\n{}", tool_text));
                } else {
                    response_text_override = Some(response_text.clone());
                }
            }
        }

        let mut new_messages = agent.get_messages_for_session();
        sanitize_last_user_message(&mut new_messages, &base_prompt, &attachment_display);
        if let Some(override_text) = response_text_override {
            if let Some(last_assistant) = new_messages
                .iter_mut()
                .rev()
                .find(|msg| msg.role == "assistant")
            {
                if last_assistant.content.trim().is_empty() {
                    last_assistant.content = override_text;
                }
            }
        }
        apply_remote_flags(&mut new_messages, &previous_messages, true);
        session.messages = new_messages;
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

        remote_guard.disarm();
        self.save_session(&session)?;

        if let Some(remote) = &self.remote {
            let queued = remote.registry().drain_queue(&session_id);
            if !queued.is_empty() {
                for queued in queued {
                    if let Err(err) =
                        Box::pin(self.process_inbound_message(&queued.plugin_id, &queued.message))
                            .await
                    {
                        tracing::error!(
                            "Failed to process queued remote message {}: {}",
                            queued.message.conversation_id,
                            err
                        );
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_pending_interaction_response(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
        pending: PendingInteraction,
        remote_ctx: &RemoteContext,
    ) -> Result<()> {
        match pending {
            PendingInteraction::Questionnaire {
                data, responder, ..
            } => {
                let response = parse_questionnaire_response(&data, &message.text);
                let _ = responder.send(response);
                let ack = ChannelSendRequest {
                    conversation_id: message.conversation_id.clone(),
                    text: "✅ Response received.".to_string(),
                    message_id: None,
                    is_final: true,
                    metadata_json: message.metadata_json.clone(),
                };
                let _ = self.send_channel_message(plugin_id, &ack).await;
                if let Some(remote) = &self.remote {
                    remote.emit(
                        RemoteEvent::new(
                            "ask_user_answer",
                            plugin_id,
                            channel_session_id(plugin_id, &message.conversation_id),
                            &message.conversation_id,
                            remote.runtime_id(),
                        )
                        .with_user(remote_ctx.user_id.clone())
                        .with_message(normalize_message_preview(&message.text)),
                    );
                }
            }
            PendingInteraction::Approval {
                request, responder, ..
            } => {
                let response = parse_approval_response(&request, &message.text);
                let _ = responder.send(response);
                let ack = ChannelSendRequest {
                    conversation_id: message.conversation_id.clone(),
                    text: "✅ Approval response received.".to_string(),
                    message_id: None,
                    is_final: true,
                    metadata_json: message.metadata_json.clone(),
                };
                let _ = self.send_channel_message(plugin_id, &ack).await;
                if let Some(remote) = &self.remote {
                    remote.emit(
                        RemoteEvent::new(
                            "approval_answer",
                            plugin_id,
                            channel_session_id(plugin_id, &message.conversation_id),
                            &message.conversation_id,
                            remote.runtime_id(),
                        )
                        .with_user(remote_ctx.user_id.clone())
                        .with_message(normalize_message_preview(&message.text)),
                    );
                }
            }
        }
        Ok(())
    }

    async fn handle_interactions(
        &self,
        plugin_id: String,
        conversation_id: String,
        mut rx: crate::tools::InteractionReceiver,
        metadata_json: String,
        remote_ctx: RemoteContext,
    ) {
        while let Some(request) = rx.recv().await {
            match request {
                InteractionRequest::Questionnaire { data, responder } => {
                    if self.has_pending_interaction(&conversation_id) {
                        let _ = responder.send(UserResponse::cancelled());
                        let _ = self
                            .send_channel_message(
                                &plugin_id,
                                &ChannelSendRequest {
                                    conversation_id: conversation_id.clone(),
                                    text: "A question is already pending. Reply to it first."
                                        .to_string(),
                                    message_id: None,
                                    is_final: true,
                                    metadata_json: metadata_json.clone(),
                                },
                            )
                            .await;
                        continue;
                    }

                    let prompt = format_questionnaire_for_remote(&data);
                    let send = self
                        .send_channel_message(
                            &plugin_id,
                            &ChannelSendRequest {
                                conversation_id: conversation_id.clone(),
                                text: prompt.clone(),
                                message_id: None,
                                is_final: true,
                                metadata_json: metadata_json.clone(),
                            },
                        )
                        .await;

                    if send.is_err() {
                        let _ = responder.send(UserResponse::cancelled());
                        continue;
                    }

                    if let Some(remote) = &self.remote {
                        remote.emit(
                            RemoteEvent::new(
                                "ask_user",
                                &plugin_id,
                                channel_session_id(&plugin_id, &conversation_id),
                                &conversation_id,
                                remote.runtime_id(),
                            )
                            .with_user(remote_ctx.user_id.clone())
                            .with_message(normalize_message_preview(&prompt)),
                        );
                    }

                    self.insert_pending_interaction(
                        conversation_id.clone(),
                        PendingInteraction::Questionnaire {
                            data,
                            responder,
                            created_at: Instant::now(),
                        },
                    );
                }
                InteractionRequest::Approval { request, responder } => {
                    if self.has_pending_interaction(&conversation_id) {
                        let _ = responder.send(ApprovalResponse::deny());
                        let _ = self
                            .send_channel_message(
                                &plugin_id,
                                &ChannelSendRequest {
                                    conversation_id: conversation_id.clone(),
                                    text:
                                        "An approval request is already pending. Reply to it first."
                                            .to_string(),
                                    message_id: None,
                                    is_final: true,
                                    metadata_json: metadata_json.clone(),
                                },
                            )
                            .await;
                        continue;
                    }

                    let prompt = format_approval_for_remote(&request);
                    let send = self
                        .send_channel_message(
                            &plugin_id,
                            &ChannelSendRequest {
                                conversation_id: conversation_id.clone(),
                                text: prompt.clone(),
                                message_id: None,
                                is_final: true,
                                metadata_json: metadata_json.clone(),
                            },
                        )
                        .await;

                    if send.is_err() {
                        let _ = responder.send(ApprovalResponse::deny());
                        continue;
                    }

                    if let Some(remote) = &self.remote {
                        remote.emit(
                            RemoteEvent::new(
                                "approval_request",
                                &plugin_id,
                                channel_session_id(&plugin_id, &conversation_id),
                                &conversation_id,
                                remote.runtime_id(),
                            )
                            .with_user(remote_ctx.user_id.clone())
                            .with_message(normalize_message_preview(&prompt)),
                        );
                    }

                    self.insert_pending_interaction(
                        conversation_id.clone(),
                        PendingInteraction::Approval {
                            request,
                            responder,
                            created_at: Instant::now(),
                        },
                    );
                }
            }
        }
    }

    async fn respond_streaming(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
        agent: &mut ChatAgent,
        prompt_text: &str,
        initial_message_id: Option<String>,
        stream_to_channel: bool,
    ) -> Result<crate::agent::AgentResponse> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let tx_stream = tx.clone();
        let final_text = Arc::new(std::sync::Mutex::new(String::new()));
        let final_text_clone = Arc::clone(&final_text);
        let message_id = Arc::new(std::sync::Mutex::new(initial_message_id));
        let message_id_clone = Arc::clone(&message_id);
        let manager = self.clone();
        let plugin_id = plugin_id.to_string();
        let plugin_id_clone = plugin_id.clone();
        let conversation_id = message.conversation_id.clone();
        let sender_conversation_id = conversation_id.clone();
        let stream_conversation_id = conversation_id.clone();
        let stream_plugin_id = plugin_id.clone();
        let metadata_json = message.metadata_json.clone();
        let remote_runtime = self.remote.clone();
        let remote_runtime_text = remote_runtime.clone();
        let remote_runtime_tool = remote_runtime.clone();
        let remote_runtime_tool_complete = remote_runtime.clone();
        let session_id = channel_session_id(&plugin_id, &conversation_id);
        let session_id_text = session_id.clone();
        let session_id_tool = session_id.clone();
        let session_id_tool_complete = session_id.clone();
        let tool_events_sent = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let tool_events_sent_call = Arc::clone(&tool_events_sent);
        let tool_events_sent_complete = Arc::clone(&tool_events_sent);
        let tool_plugin_id = plugin_id.clone();
        let tool_plugin_id_call = tool_plugin_id.clone();
        let tool_plugin_id_complete = tool_plugin_id.clone();
        let tool_conversation_id = conversation_id.clone();
        let tool_conversation_id_call = tool_conversation_id.clone();
        let tool_conversation_id_complete = tool_conversation_id.clone();
        let tool_metadata_json = metadata_json.clone();
        let tool_metadata_json_call = tool_metadata_json.clone();
        let tool_metadata_json_complete = tool_metadata_json.clone();
        let tool_manager = self.clone();
        let tool_manager_call = tool_manager.clone();
        let tool_manager_complete = tool_manager.clone();
        let interrupt_flag = Arc::new(AtomicBool::new(false));
        let interrupt_flag_check = Arc::clone(&interrupt_flag);
        let interrupt_session_id = session_id.clone();
        let interrupt_remote = self.remote.clone();
        let add_prefix = stream_to_channel && remote_runtime.is_some();
        let prefix_text = if add_prefix {
            Some(format!(
                "**{}**\n\n",
                remote_response_label(RemoteResponseLabel::Answer)
            ))
        } else {
            None
        };

        let sender = if stream_to_channel {
            Some(tokio::spawn(async move {
                let mut accumulated = prefix_text.unwrap_or_default();
                let mut last_sent = Instant::now();
                let mut message_id_local =
                    message_id_clone.lock().ok().and_then(|guard| guard.clone());

                while let Some(chunk) = rx.recv().await {
                    accumulated.push_str(&chunk);
                    let should_send = accumulated.len() >= STREAM_MIN_CHARS
                        || last_sent.elapsed() >= STREAM_DEBOUNCE;
                    if !should_send {
                        continue;
                    }

                    let send_request = ChannelSendRequest {
                        conversation_id: sender_conversation_id.clone(),
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
            }))
        } else {
            None
        };

        let response = agent
            .chat_streaming(
                prompt_text,
                move || {
                    if let Some(remote) = &interrupt_remote {
                        if remote.registry().is_interrupted(&interrupt_session_id) {
                            interrupt_flag_check.store(true, Ordering::SeqCst);
                            return true;
                        }
                    }
                    false
                },
                move |chunk| {
                    if let Ok(mut guard) = final_text_clone.lock() {
                        guard.push_str(&chunk);
                    }
                    if let Some(remote) = &remote_runtime_text {
                        remote.emit(
                            RemoteEvent::new(
                                "stream_chunk",
                                &stream_plugin_id,
                                &session_id_text,
                                &stream_conversation_id,
                                remote.runtime_id(),
                            )
                            .with_message(chunk.clone()),
                        );
                    }
                    if stream_to_channel {
                        let _ = tx_stream.send(chunk);
                    }
                },
                |_| {},
                move |tool_name, args| {
                    tool_events_sent_call.store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(remote) = &remote_runtime_tool {
                        remote.emit(
                            RemoteEvent::new(
                                "tool_started",
                                &tool_plugin_id_call,
                                &session_id_tool,
                                &tool_conversation_id_call,
                                remote.runtime_id(),
                            )
                            .with_metadata(serde_json::json!({
                                "name": tool_name,
                                "args": args,
                            })),
                        );
                    }
                    let args_preview = if args.len() > 300 {
                        format!("{}…", crate::core::truncate_at_char_boundary(&args, 300))
                    } else {
                        args.clone()
                    };
                    let tool_text = if args_preview.is_empty() {
                        format!("🔧 Running `{}`", tool_name)
                    } else {
                        format!("🔧 Running `{}`\nArgs: {}", tool_name, args_preview)
                    };
                    let request = ChannelSendRequest {
                        conversation_id: tool_conversation_id_call.clone(),
                        text: tool_text,
                        message_id: None,
                        is_final: true,
                        metadata_json: tool_metadata_json_call.clone(),
                    };
                    let manager = tool_manager_call.clone();
                    let tool_plugin_id = tool_plugin_id_call.clone();
                    tokio::spawn(async move {
                        let _ = manager
                            .send_channel_message(&tool_plugin_id, &request)
                            .await;
                    });
                },
                move |tool_name, result, success| {
                    tool_events_sent_complete.store(true, std::sync::atomic::Ordering::Relaxed);
                    if let Some(remote) = &remote_runtime_tool_complete {
                        remote.emit(
                            RemoteEvent::new(
                                if success {
                                    "tool_completed"
                                } else {
                                    "tool_failed"
                                },
                                &tool_plugin_id_complete,
                                &session_id_tool_complete,
                                &tool_conversation_id_complete,
                                remote.runtime_id(),
                            )
                            .with_metadata(serde_json::json!({
                                "name": tool_name,
                                "result": result,
                                "success": success,
                            })),
                        );
                    }
                    let result_preview = if result.len() > 400 {
                        format!("{}…", crate::core::truncate_at_char_boundary(&result, 400))
                    } else {
                        result.clone()
                    };
                    let tool_text = if success {
                        format!("✅ Completed `{}`\n{}", tool_name, result_preview)
                    } else {
                        format!("❌ Failed `{}`\n{}", tool_name, result_preview)
                    };
                    let request = ChannelSendRequest {
                        conversation_id: tool_conversation_id_complete.clone(),
                        text: tool_text,
                        message_id: None,
                        is_final: true,
                        metadata_json: tool_metadata_json_complete.clone(),
                    };
                    let manager = tool_manager_complete.clone();
                    let tool_plugin_id = tool_plugin_id_complete.clone();
                    tokio::spawn(async move {
                        let _ = manager
                            .send_channel_message(&tool_plugin_id, &request)
                            .await;
                    });
                },
                |_| {},
            )
            .await?;

        drop(tx);
        if let Some(sender) = sender {
            let _ = sender.await;
        }

        let summary = build_tool_summary(response.tool_calls_made, &response.tool_call_log);
        let fallback_text = final_text
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        let (mut response_text, response_was_empty) =
            finalize_response_text(&response.text, &fallback_text, summary.as_ref());
        let mut response_label = RemoteResponseLabel::Answer;
        let mut send_tool_log = !tool_events_sent.load(std::sync::atomic::Ordering::Relaxed);
        if response.text.trim().is_empty() {
            if let Some(tool_text) = format_tool_log_for_remote(&response.tool_call_log) {
                response_text = format!("Tool result:\n{}", tool_text);
                send_tool_log = false;
                response_label = RemoteResponseLabel::ToolOutput;
            }
        }
        if remote_runtime.is_some() {
            response_text = prefix_remote_response(response_text, response_label);
        }
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

        if send_tool_log {
            if let Some(tool_text) = format_tool_log_for_remote(&response.tool_call_log) {
                let tool_request = ChannelSendRequest {
                    conversation_id: message.conversation_id.clone(),
                    text: tool_text,
                    message_id: None,
                    is_final: true,
                    metadata_json: message.metadata_json.clone(),
                };
                let _ = self.send_channel_message(&plugin_id, &tool_request).await?;
            }
        }

        if interrupt_flag.load(Ordering::SeqCst) {
            if let Some(remote) = &self.remote {
                let _ = remote.registry().clear_interrupt(&session_id);
            }
        }

        if response_was_empty {
            tracing::warn!("Streaming response was empty");
        }

        Ok(response)
    }

    pub async fn channel_info(&self, plugin_id: &str) -> Result<ChannelInfo> {
        let plugin = self.load_channel_plugin(plugin_id)?;
        tokio::task::spawn_blocking({
            let manager = self.clone();
            move || manager.with_channel_instance(&plugin, |instance| instance.channel_info())
        })
        .await?
    }

    pub async fn send_channel_message(
        &self,
        plugin_id: &str,
        request: &ChannelSendRequest,
    ) -> Result<ChannelSendResult> {
        let max_len = self.config.remote.max_message_chars;
        if request.is_final && max_len > 0 && request.text.len() > max_len {
            return self
                .send_channel_message_chunked(plugin_id, request, max_len)
                .await;
        }

        self.send_channel_message_inner(plugin_id, request).await
    }

    async fn send_channel_message_chunked(
        &self,
        plugin_id: &str,
        request: &ChannelSendRequest,
        max_len: usize,
    ) -> Result<ChannelSendResult> {
        let chunks = split_message_by_chars(&request.text, max_len);
        let mut message_id = request.message_id.clone();
        let mut first_result: Option<ChannelSendResult> = None;

        for (idx, chunk) in chunks.into_iter().enumerate() {
            let mut chunk_request = request.clone();
            chunk_request.text = chunk;
            chunk_request.message_id = if idx == 0 { message_id.clone() } else { None };
            let result = self
                .send_channel_message_inner(plugin_id, &chunk_request)
                .await?;
            if idx == 0 {
                message_id = result.message_id.clone();
                first_result = Some(result);
            }
        }

        Ok(first_result.unwrap_or(ChannelSendResult {
            success: true,
            message_id,
            error: None,
        }))
    }

    async fn send_channel_message_inner(
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
                "Commands: /tark status | /tark mode <ask|plan|build> | /tark model <id> | /tark provider <id> | /tark trust <manual|careful|balanced> | /tark usage | /tark stop | /tark resume | /tark interrupt".to_string(),
            ),
            RemoteCommand::Status => {
                let queued = remote.registry().queued_count(&session.id);
                let queued_suffix = if queued > 0 {
                    format!(" queued={}", queued)
                } else {
                    String::new()
                };
                Some(format!(
                    "session={} mode={} trust={} provider={} model={}{}",
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
                    },
                    queued_suffix
                ))
            }
            RemoteCommand::Stop => {
                remote.registry().stop_session(&session.id)?;
                Some("Session stopped.".to_string())
            }
            RemoteCommand::Resume => {
                remote.registry().resume_session(&session.id)?;
                Some("Session resumed.".to_string())
            }
            RemoteCommand::Interrupt => {
                remote.registry().interrupt_session(&session.id)?;
                Some("⏹ Interrupt requested. Current task will stop shortly.".to_string())
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

fn format_tool_log_for_remote(tool_log: &[ToolCallLog]) -> Option<String> {
    if tool_log.is_empty() {
        return None;
    }
    let mut lines = Vec::new();
    lines.push(format!("Tool activity ({}):", tool_log.len()));
    for (idx, entry) in tool_log.iter().enumerate() {
        let args = serde_json::to_string_pretty(&entry.args).unwrap_or_default();
        let result = entry.result_preview.trim();
        lines.push(format!("{}. {}", idx + 1, entry.tool));
        lines.push(format!("   Args: {}", normalize_multiline(&args, 2000)));
        if !result.is_empty() {
            lines.push(format!("   Result: {}", normalize_multiline(result, 2000)));
        }
    }
    Some(lines.join("\n"))
}

fn normalize_multiline(value: &str, max_len: usize) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= max_len {
        return trimmed.to_string();
    }
    let mut end = max_len;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &trimmed[..end])
}

fn remote_response_label(label: RemoteResponseLabel) -> &'static str {
    match label {
        RemoteResponseLabel::Answer => "🧠 Answer",
        RemoteResponseLabel::ToolOutput => "🔧 Tool Output",
    }
}

fn prefix_remote_response(text: String, label: RemoteResponseLabel) -> String {
    let header = remote_response_label(label);
    if text.trim().is_empty() {
        format!("**{}**", header)
    } else {
        format!("**{}**\n\n{}", header, text)
    }
}

fn split_message_by_chars(text: &str, max_len: usize) -> Vec<String> {
    if max_len == 0 || text.len() <= max_len {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < text.len() {
        let mut end = std::cmp::min(start + max_len, text.len());
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            break;
        }
        chunks.push(text[start..end].to_string());
        start = end;
    }

    if chunks.is_empty() {
        chunks.push(text.to_string());
    }

    chunks
}

fn allow_remote_streaming(
    supports_streaming: bool,
    supports_edits: bool,
    message_id: Option<&str>,
) -> bool {
    supports_streaming && supports_edits && message_id.is_some()
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

fn inbound_dedupe_key(message: &ChannelInboundMessage) -> String {
    if !message.metadata_json.trim().is_empty() {
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&message.metadata_json) {
            if let Some(id) = extract_message_id(&map) {
                return format!("id:{}", id);
            }
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(message.conversation_id.as_bytes());
    hasher.update(b"|");
    hasher.update(message.user_id.as_bytes());
    hasher.update(b"|");
    hasher.update(message.text.as_bytes());
    let hash = URL_SAFE_NO_PAD.encode(hasher.finalize());
    format!("hash:{}", hash)
}

fn extract_message_id(map: &Map<String, Value>) -> Option<String> {
    if let Some(id) = extract_string(map.get("message_id")) {
        return Some(id);
    }
    if let Some(id) = extract_string(map.get("id")) {
        return Some(id);
    }
    if let Some(Value::Object(discord)) = map.get("discord") {
        if let Some(id) = extract_string(discord.get("message_id")) {
            return Some(id);
        }
        if let Some(id) = extract_string(discord.get("id")) {
            return Some(id);
        }
    }
    None
}

fn extract_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(num)) => Some(num.to_string()),
        _ => None,
    }
}

fn apply_remote_flags(
    new_messages: &mut [crate::storage::SessionMessage],
    previous: &[crate::storage::SessionMessage],
    mark_new_as_remote: bool,
) {
    for (idx, msg) in new_messages.iter_mut().enumerate() {
        if let Some(prev) = previous.get(idx) {
            msg.remote = prev.remote;
            continue;
        }
        if mark_new_as_remote {
            let role = msg.role.as_str();
            if matches!(role, "user" | "assistant") {
                msg.remote = true;
            }
        }
    }
}

pub(crate) fn format_questionnaire_for_remote(questionnaire: &Questionnaire) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Question: {}", questionnaire.title));
    if let Some(desc) = &questionnaire.description {
        if !desc.trim().is_empty() {
            lines.push(desc.trim().to_string());
        }
    }
    for (idx, question) in questionnaire.questions.iter().enumerate() {
        lines.push(format!("{}. {} ({})", idx + 1, question.text, question.id));
        match &question.kind {
            crate::tools::questionnaire::QuestionType::SingleSelect { options, .. } => {
                let opts = options
                    .iter()
                    .map(|o| format!("{} ({})", o.label, o.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("   Options: {}", opts));
            }
            crate::tools::questionnaire::QuestionType::MultiSelect { options, .. } => {
                let opts = options
                    .iter()
                    .map(|o| format!("{} ({})", o.label, o.value))
                    .collect::<Vec<_>>()
                    .join(", ");
                lines.push(format!("   Options (multi): {}", opts));
            }
            crate::tools::questionnaire::QuestionType::FreeText { placeholder, .. } => {
                if let Some(placeholder) = placeholder {
                    if !placeholder.trim().is_empty() {
                        lines.push(format!("   Hint: {}", placeholder.trim()));
                    }
                }
            }
        }
    }

    if questionnaire.questions.len() == 1 {
        lines.push("Reply with your answer.".to_string());
    } else {
        lines.push(
            "Reply with one line per question: <id>: <answer> (or send a JSON object).".to_string(),
        );
    }

    lines.join("\n")
}

fn parse_questionnaire_response(questionnaire: &Questionnaire, text: &str) -> UserResponse {
    let trimmed = text.trim();
    if trimmed.eq_ignore_ascii_case("cancel") {
        return UserResponse::cancelled();
    }

    if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
        if let Some(obj) = json.as_object() {
            let mut answers = std::collections::HashMap::new();
            for question in &questionnaire.questions {
                if let Some(value) = obj.get(&question.id) {
                    let answer = match value {
                        serde_json::Value::Array(values) => {
                            let vals = values.iter().filter_map(json_value_to_string).collect();
                            AnswerValue::Multi(vals)
                        }
                        _ => AnswerValue::Single(json_value_to_string(value).unwrap_or_default()),
                    };
                    answers.insert(question.id.clone(), answer);
                }
            }
            if !answers.is_empty() {
                return UserResponse::with_answers(answers);
            }
        }
    }

    let mut answers = std::collections::HashMap::new();
    let keyed = parse_keyed_answers(trimmed);

    if questionnaire.questions.len() == 1 {
        let question = &questionnaire.questions[0];
        let answer = answer_for_question(question, trimmed);
        answers.insert(question.id.clone(), answer);
        return UserResponse::with_answers(answers);
    }

    if !keyed.is_empty() {
        for question in &questionnaire.questions {
            if let Some(value) = keyed.get(&question.id) {
                answers.insert(question.id.clone(), answer_for_question(question, value));
            }
        }
    }

    if answers.is_empty() {
        if let Some(question) = questionnaire.questions.first() {
            answers.insert(question.id.clone(), answer_for_question(question, trimmed));
        }
    }

    UserResponse::with_answers(answers)
}

fn parse_keyed_answers(text: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();
            if !key.is_empty() && !value.is_empty() {
                map.insert(key.to_string(), value.to_string());
            }
        }
    }
    map
}

fn answer_for_question(
    question: &crate::tools::questionnaire::Question,
    text: &str,
) -> AnswerValue {
    let trimmed = text.trim();
    match &question.kind {
        crate::tools::questionnaire::QuestionType::SingleSelect { options, .. } => {
            AnswerValue::Single(match_option(trimmed, options))
        }
        crate::tools::questionnaire::QuestionType::MultiSelect { options, .. } => {
            let values = split_multi_values(trimmed)
                .into_iter()
                .map(|value| match_option(&value, options))
                .collect();
            AnswerValue::Multi(values)
        }
        crate::tools::questionnaire::QuestionType::FreeText { .. } => {
            AnswerValue::Single(trimmed.to_string())
        }
    }
}

fn split_multi_values(text: &str) -> Vec<String> {
    text.split([',', ';', '|'])
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn match_option(input: &str, options: &[crate::tools::questionnaire::OptionItem]) -> String {
    for opt in options {
        if opt.value.eq_ignore_ascii_case(input) || opt.label.eq_ignore_ascii_case(input) {
            return opt.value.clone();
        }
    }
    input.to_string()
}

fn json_value_to_string(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        serde_json::Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn format_approval_for_remote(request: &ApprovalRequest) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Approval needed: {}", request.tool));
    lines.push(format!("Command: {}", request.command));
    lines.push(format!("Risk: {:?}", request.risk_level));
    lines.push("Reply with: y/1 (approve once), s/2 (approve session), p/3 (approve always), n/4 (deny), N/5 (deny always).".to_string());
    lines.join("\n")
}

fn parse_approval_response(request: &ApprovalRequest, text: &str) -> ApprovalResponse {
    let normalized = text.trim().to_lowercase();
    let choice = match normalized.as_str() {
        "y" | "yes" | "1" | "approve" | "allow" => ApprovalChoice::ApproveOnce,
        "s" | "2" | "session" => ApprovalChoice::ApproveSession,
        "p" | "3" | "always" => ApprovalChoice::ApproveAlways,
        "n" | "4" | "deny" | "no" => ApprovalChoice::Deny,
        "n/5" | "5" | "deny_always" | "never" | "deny always" => ApprovalChoice::DenyAlways,
        _ => ApprovalChoice::Deny,
    };

    match choice {
        ApprovalChoice::ApproveOnce => ApprovalResponse::approve_once(),
        ApprovalChoice::Deny => ApprovalResponse::deny(),
        ApprovalChoice::ApproveSession => {
            ApprovalResponse::approve_session(suggested_pattern(request))
        }
        ApprovalChoice::ApproveAlways => {
            ApprovalResponse::approve_always(suggested_pattern(request))
        }
        ApprovalChoice::DenyAlways => ApprovalResponse::deny_always(suggested_pattern(request)),
    }
}

fn suggested_pattern(request: &ApprovalRequest) -> ApprovalPattern {
    if let Some(first) = request.suggested_patterns.first() {
        ApprovalPattern::new(
            request.tool.clone(),
            first.pattern.clone(),
            first.match_type,
        )
        .with_description(first.description.clone())
    } else {
        ApprovalPattern::new(
            request.tool.clone(),
            request.command.clone(),
            MatchType::Exact,
        )
    }
}

#[derive(Debug, Clone)]
enum RemoteCommand {
    Help,
    Status,
    Stop,
    Resume,
    Interrupt,
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

#[derive(Debug, Clone, serde::Deserialize)]
struct RemoteAttachmentMeta {
    #[serde(default)]
    id: String,
    #[serde(default)]
    filename: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content_type: Option<String>,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    width: Option<u64>,
    #[serde(default)]
    height: Option<u64>,
}

impl RemoteAttachmentMeta {
    fn is_image(&self) -> bool {
        if self
            .content_type
            .as_deref()
            .map(|v| v.starts_with("image/"))
            .unwrap_or(false)
        {
            return true;
        }
        let lower = self.filename.to_lowercase();
        [".png", ".jpg", ".jpeg", ".gif", ".webp"]
            .iter()
            .any(|ext| lower.ends_with(ext))
    }

    fn display_name(&self) -> Cow<'_, str> {
        if !self.filename.is_empty() {
            Cow::Borrowed(self.filename.as_str())
        } else if !self.id.is_empty() {
            Cow::Borrowed(self.id.as_str())
        } else {
            Cow::Borrowed("attachment")
        }
    }

    fn mime_display(&self) -> &str {
        self.content_type
            .as_deref()
            .unwrap_or("application/octet-stream")
    }
}

fn parse_remote_attachments(metadata_json: &str) -> Vec<RemoteAttachmentMeta> {
    let Ok(value) = serde_json::from_str::<Value>(metadata_json) else {
        return Vec::new();
    };
    let mut attachments = Vec::new();
    let sources = [
        value
            .get("discord")
            .and_then(|d| d.get("attachments"))
            .cloned(),
        value.get("attachments").cloned(),
    ];
    for src in sources.into_iter().flatten() {
        match src {
            Value::Array(items) => {
                for item in items {
                    if let Ok(meta) = serde_json::from_value::<RemoteAttachmentMeta>(item) {
                        if !meta.url.is_empty() {
                            attachments.push(meta);
                        }
                    }
                }
            }
            Value::Object(map) => {
                for (_, item) in map {
                    if let Ok(meta) = serde_json::from_value::<RemoteAttachmentMeta>(item) {
                        if !meta.url.is_empty() {
                            attachments.push(meta);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    attachments
}

fn format_remote_attachment_metadata(attachments: &[RemoteAttachmentMeta]) -> String {
    let mut lines = Vec::new();
    lines.push("[Attachments]".to_string());
    for att in attachments {
        let size = att
            .size
            .map(format_size)
            .unwrap_or_else(|| "unknown size".to_string());
        lines.push(format!(
            "- {} ({}, {})",
            att.display_name(),
            att.mime_display(),
            size
        ));
    }
    lines.push("[/Attachments]".to_string());
    lines.join("\n")
}

fn format_attachments_for_prompt(attachments: &[MessageAttachment]) -> String {
    let mut sections = Vec::new();
    sections.push("[Attachments]".to_string());
    for attachment in attachments {
        sections.push(format!(
            "- {} ({})",
            attachment.filename, attachment.mime_type
        ));
        match &attachment.content {
            AttachmentContent::Base64(encoded) => {
                sections.push(encoded.clone());
            }
            AttachmentContent::Text(text) => {
                sections.push(text.clone());
            }
            AttachmentContent::Path(path) => {
                sections.push(format!("(file: {})", path.display()));
            }
        }
    }
    sections.push("[/Attachments]".to_string());
    sections.join("\n")
}

fn is_allowed_attachment_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    let Some(host) = parsed.host_str() else {
        return false;
    };
    host.ends_with("discordapp.com")
        || host.ends_with("discordapp.net")
        || host.ends_with("discord.com")
        || host.ends_with("discord.gg")
}

async fn download_attachment_bytes(url: &str, max_size: u64) -> Result<Vec<u8>> {
    let response = reqwest::get(url).await?;
    if !response.status().is_success() {
        anyhow::bail!("attachment download failed with {}", response.status());
    }
    if let Some(len) = response.content_length() {
        if len > max_size {
            anyhow::bail!("attachment too large ({} bytes)", len);
        }
    }
    let bytes = response.bytes().await?;
    if bytes.len() as u64 > max_size {
        anyhow::bail!("attachment too large ({} bytes)", bytes.len());
    }
    Ok(bytes.to_vec())
}

async fn build_remote_attachment_context(
    attachments: &[RemoteAttachmentMeta],
    config: &crate::config::RemoteAttachmentConfig,
    provider: &str,
    model: Option<&str>,
) -> (String, String) {
    if attachments.is_empty() {
        return (String::new(), String::new());
    }
    let display = format_remote_attachment_metadata(attachments);
    if config.mode == "disabled" {
        return (String::new(), display);
    }

    let mut prompt_attachments = Vec::new();
    if config.analyze_images {
        let max_size = config.max_image_size_mb.saturating_mul(1024 * 1024);
        let model_id = model.unwrap_or_default();
        let supports_vision = if model_id.is_empty() {
            matches!(
                provider.to_lowercase().as_str(),
                "openai" | "claude" | "gemini" | "openrouter"
            )
        } else {
            llm::models_db().supports_vision(provider, model_id).await
        };
        if supports_vision {
            for att in attachments {
                if !att.is_image() || att.url.is_empty() {
                    continue;
                }
                if !is_allowed_attachment_url(&att.url) {
                    continue;
                }
                if let Some(size) = att.size {
                    if size > max_size {
                        continue;
                    }
                }
                if let Ok(bytes) = download_attachment_bytes(&att.url, max_size).await {
                    let encoded = base64_encode(&bytes);
                    let mime = att.mime_display().to_string();
                    prompt_attachments.push(MessageAttachment {
                        filename: att.display_name().to_string(),
                        mime_type: mime,
                        content: AttachmentContent::Base64(encoded),
                    });
                }
            }
        }
    }

    let prompt = if prompt_attachments.is_empty() {
        display.clone()
    } else {
        format_attachments_for_prompt(&prompt_attachments)
    };

    (prompt, display)
}

fn sanitize_last_user_message(
    messages: &mut [crate::storage::SessionMessage],
    user_text: &str,
    attachment_display: &str,
) {
    if attachment_display.trim().is_empty() {
        return;
    }
    if let Some(last_user) = messages.iter_mut().rev().find(|msg| msg.role == "user") {
        let base = if user_text.trim().is_empty() {
            "Attachment received.".to_string()
        } else {
            user_text.to_string()
        };
        last_user.content = format!("{}\n\n{}", base, attachment_display);
    }
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
        "interrupt" | "cancel" | "abort" => Some(RemoteCommand::Interrupt),
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

    #[test]
    fn test_split_message_by_chars_chunks_ascii() {
        let chunks = split_message_by_chars("abcdef", 2);
        assert_eq!(chunks, vec!["ab", "cd", "ef"]);
    }

    #[test]
    fn test_split_message_by_chars_zero_limit() {
        let chunks = split_message_by_chars("abcdef", 0);
        assert_eq!(chunks, vec!["abcdef"]);
    }

    #[test]
    fn test_prefix_remote_response_adds_label() {
        let text = prefix_remote_response("hello".to_string(), RemoteResponseLabel::Answer);
        assert!(text.contains("**🧠 Answer**"));
        assert!(text.contains("hello"));
    }

    #[test]
    fn test_allow_remote_streaming_requires_message_id() {
        assert!(!allow_remote_streaming(true, true, None));
        assert!(!allow_remote_streaming(true, false, Some("id")));
        assert!(!allow_remote_streaming(false, true, Some("id")));
        assert!(allow_remote_streaming(true, true, Some("id")));
    }

    #[test]
    fn test_inbound_dedupe_key_uses_metadata_id() {
        let msg = ChannelInboundMessage {
            conversation_id: "c1".to_string(),
            user_id: "u1".to_string(),
            text: "hello".to_string(),
            metadata_json: r#"{"message_id":"abc123"}"#.to_string(),
        };
        let key = inbound_dedupe_key(&msg);
        assert!(key.starts_with("id:"));
        assert!(key.contains("abc123"));
    }

    #[test]
    fn test_channel_shutdown_reset() {
        request_channel_shutdown();
        assert!(CHANNEL_POLL_SHUTDOWN.load(Ordering::SeqCst));
        reset_channel_shutdown();
        assert!(!CHANNEL_POLL_SHUTDOWN.load(Ordering::SeqCst));
    }
}
