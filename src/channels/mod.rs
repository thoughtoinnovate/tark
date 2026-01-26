//! Messaging channel integration (WASM plugins)
//!
//! Channel plugins translate external messages (Slack, Discord, Signal)
//! into tark chat requests and send responses back to those channels.

use crate::agent::ChatAgent;
use crate::config::Config;
use crate::llm;
use crate::plugins::{
    ChannelInboundMessage, ChannelInfo, ChannelSendRequest, ChannelSendResult,
    ChannelWebhookRequest, ChannelWebhookResponse, InstalledPlugin, PluginHost, PluginRegistry,
    PluginType,
};
use crate::storage::{ChatSession, TarkStorage};
use crate::tools::ToolRegistry;
use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::time::{Duration, Instant};

const STREAM_DEBOUNCE: Duration = Duration::from_millis(250);
const STREAM_MIN_CHARS: usize = 200;

#[derive(Clone)]
pub struct ChannelManager {
    config: Config,
    working_dir: PathBuf,
    storage: Option<Arc<TarkStorage>>,
}

impl ChannelManager {
    pub fn new(config: Config, working_dir: PathBuf, storage: Option<Arc<TarkStorage>>) -> Self {
        Self {
            config,
            working_dir,
            storage,
        }
    }

    pub async fn start_all(&self) -> Result<()> {
        let registry = PluginRegistry::new()?;
        for plugin in registry.by_type(PluginType::Channel) {
            if !plugin.enabled {
                continue;
            }
            let plugin = plugin.clone();
            let manager = self.clone();
            tokio::task::spawn_blocking(move || manager.start_plugin(&plugin)).await??;
        }
        Ok(())
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

    fn load_channel_plugin(&self, plugin_id: &str) -> Result<InstalledPlugin> {
        let registry = PluginRegistry::new()?;
        let plugin = registry
            .get(plugin_id)
            .ok_or_else(|| anyhow::anyhow!("Plugin '{}' not found", plugin_id))?;
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
        host.load(plugin)?;
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

        let creds_path = if let Some(path) = &oauth.credentials_path {
            expand_path(path)
        } else {
            default_oauth_credentials_path(plugin.id())?
        };

        if !creds_path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&creds_path)
            .with_context(|| format!("Failed to read {}", creds_path.display()))?;
        Ok(Some(content))
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
        let mut session = self.load_session(&session_id)?;

        if session.name.is_empty() {
            session.set_name_from_prompt(&message.text);
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

        let mut agent =
            ChatAgent::new(provider, tools).with_max_iterations(self.config.agent.max_iterations);
        agent.set_thinking_config(self.config.thinking.clone());
        agent.set_think_level_sync(self.config.thinking.effective_default_level_name());
        agent.restore_from_session(&session);

        if channel_info.supports_streaming {
            self.respond_streaming(plugin_id, message, &mut agent)
                .await?;
        } else {
            let response = agent.chat(&message.text).await?;
            let send_request = ChannelSendRequest {
                conversation_id: message.conversation_id.clone(),
                text: response.text,
                message_id: None,
                is_final: true,
                metadata_json: message.metadata_json.clone(),
            };
            let _ = self.send_channel_message(plugin_id, &send_request).await?;
        }

        session.messages = agent.get_messages_for_session();
        session.updated_at = Utc::now();
        session.provider = provider_name;
        self.save_session(&session)?;

        Ok(())
    }

    async fn respond_streaming(
        &self,
        plugin_id: &str,
        message: &ChannelInboundMessage,
        agent: &mut ChatAgent,
    ) -> Result<()> {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let tx_stream = tx.clone();
        let manager = self.clone();
        let plugin_id = plugin_id.to_string();
        let conversation_id = message.conversation_id.clone();
        let metadata_json = message.metadata_json.clone();

        let sender = tokio::spawn(async move {
            let mut accumulated = String::new();
            let mut last_sent = Instant::now();
            let mut message_id: Option<String> = None;

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
                    message_id: message_id.clone(),
                    is_final: false,
                    metadata_json: metadata_json.clone(),
                };
                if let Ok(result) = manager
                    .send_channel_message(&plugin_id, &send_request)
                    .await
                {
                    if message_id.is_none() {
                        message_id = result.message_id;
                    }
                }

                last_sent = Instant::now();
            }

            let send_request = ChannelSendRequest {
                conversation_id,
                text: accumulated,
                message_id,
                is_final: true,
                metadata_json,
            };
            let _ = manager
                .send_channel_message(&plugin_id, &send_request)
                .await;
        });

        let response = agent
            .chat_streaming(
                &message.text,
                || false,
                move |chunk| {
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

        if response.text.is_empty() {
            tracing::warn!("Streaming response was empty");
        }

        Ok(())
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
        tokio::task::spawn_blocking({
            let manager = self.clone();
            move || {
                manager.with_channel_instance(&plugin, |instance| instance.channel_send(&request))
            }
        })
        .await?
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
}
