//! Messaging channel integration (WASM plugins)
//!
//! Channel plugins translate external messages (Slack, Discord, Signal)
//! into tark chat requests and send responses back to those channels.

use crate::agent::{ChatAgent, ToolCallLog};
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
            let summary = build_tool_summary(response.tool_calls_made, &response.tool_call_log);
            let send_request = ChannelSendRequest {
                conversation_id: message.conversation_id.clone(),
                text: append_tool_summary(response.text, summary.as_ref()),
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
        let response_was_empty = response.text.is_empty();
        let mut response_text = if response_was_empty {
            fallback_text
        } else {
            response.text
        };
        response_text = append_tool_summary(response_text, summary.as_ref());
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
}
