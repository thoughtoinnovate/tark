//! Remote channel mirroring for local TUI sessions.
//!
//! Mirrors local user/assistant messages to a remote channel session when one exists,
//! and streams assistant output so remote listeners see live progress.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use crate::channels::remote::{RemoteRuntime, RemoteSessionEntry};
use crate::channels::ChannelManager;
use crate::config::Config;
use crate::plugins::ChannelSendRequest;
use crate::storage::usage::UsageTracker;
use crate::storage::TarkStorage;

const STREAM_DEBOUNCE: Duration = Duration::from_millis(250);
const STREAM_MIN_CHARS: usize = 200;

#[derive(Debug)]
enum MirrorEvent {
    UserMessage {
        session_id: String,
        content: String,
    },
    LlmStart {
        session_id: String,
    },
    LlmChunk {
        session_id: String,
        chunk: String,
    },
    LlmDone {
        session_id: String,
        text: String,
    },
    LlmError {
        session_id: String,
        error: String,
    },
    ToolStarted {
        session_id: String,
        name: String,
        args: serde_json::Value,
    },
    ToolCompleted {
        session_id: String,
        name: String,
        result: String,
        success: bool,
    },
    AskUser {
        session_id: String,
        prompt: String,
    },
    ApprovalRequest {
        session_id: String,
        prompt: String,
    },
}

#[derive(Debug)]
struct StreamState {
    plugin_id: String,
    conversation_id: String,
    message_id: Option<String>,
    supports_edits: bool,
    supports_streaming: bool,
    accumulated: String,
    last_sent: Instant,
}

fn mirror_streaming_enabled(supports_streaming: bool, message_id: Option<&str>) -> bool {
    supports_streaming && message_id.is_some()
}

#[derive(Clone)]
pub struct RemoteMirror {
    tx: mpsc::UnboundedSender<MirrorEvent>,
}

impl RemoteMirror {
    pub fn new(
        config: Config,
        working_dir: PathBuf,
        storage: Option<TarkStorage>,
        usage_tracker: Option<UsageTracker>,
        remote: Arc<RemoteRuntime>,
    ) -> Arc<Self> {
        let (tx, mut rx) = mpsc::unbounded_channel::<MirrorEvent>();
        let mirror = Arc::new(Self { tx });

        let channel_manager = ChannelManager::new(
            config,
            working_dir,
            storage.map(Arc::new),
            usage_tracker.map(Arc::new),
            Some(remote.clone()),
            None,
            None,
        );

        tokio::spawn(async move {
            let mut streams: HashMap<String, StreamState> = HashMap::new();
            while let Some(event) = rx.recv().await {
                match event {
                    MirrorEvent::UserMessage {
                        session_id,
                        content,
                    } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let text = format!("🖥 {}", content);
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "user" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text,
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                        }
                    }
                    MirrorEvent::LlmStart { session_id } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let (supports_edits, supports_streaming) = channel_manager
                                .channel_info(&entry.plugin_id)
                                .await
                                .map(|info| {
                                    (
                                        info.supports_edits,
                                        info.supports_streaming && info.supports_edits,
                                    )
                                })
                                .unwrap_or((false, false));
                            streams.insert(
                                session_id.clone(),
                                StreamState {
                                    plugin_id: entry.plugin_id.clone(),
                                    conversation_id: entry.conversation_id.clone(),
                                    message_id: None,
                                    supports_edits,
                                    supports_streaming,
                                    accumulated: String::new(),
                                    last_sent: Instant::now(),
                                },
                            );

                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "working" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text: "⏳ Working...".to_string(),
                                message_id: None,
                                is_final: false,
                                metadata_json,
                            };
                            if supports_edits {
                                if let Ok(result) = channel_manager
                                    .send_channel_message(&entry.plugin_id, &request)
                                    .await
                                {
                                    if let Some(state) = streams.get_mut(&session_id) {
                                        if state.message_id.is_none() {
                                            state.message_id = result.message_id;
                                        }
                                        if state.message_id.is_none() {
                                            state.supports_streaming = false;
                                        }
                                    }
                                } else if let Some(state) = streams.get_mut(&session_id) {
                                    state.supports_streaming = false;
                                }
                            } else if let Some(state) = streams.get_mut(&session_id) {
                                state.supports_streaming = false;
                            }

                            let _ = remote.registry().mark_status(
                                &session_id,
                                remote.runtime_id(),
                                &entry.plugin_id,
                                &entry.conversation_id,
                                "working",
                            );
                            remote.emit(crate::channels::remote::RemoteEvent::new(
                                "agent_start",
                                &entry.plugin_id,
                                &session_id,
                                &entry.conversation_id,
                                remote.runtime_id(),
                            ));
                        }
                    }
                    MirrorEvent::LlmChunk { session_id, chunk } => {
                        let Some(state) = streams.get_mut(&session_id) else {
                            continue;
                        };
                        if !mirror_streaming_enabled(
                            state.supports_streaming,
                            state.message_id.as_deref(),
                        ) {
                            continue;
                        }
                        state.accumulated.push_str(&chunk);
                        let should_send = state.accumulated.len() >= STREAM_MIN_CHARS
                            || state.last_sent.elapsed() >= STREAM_DEBOUNCE;
                        if !should_send {
                            continue;
                        }
                        let metadata_json = serde_json::json!({
                            "tark": { "mirror": true, "source": "local", "kind": "stream" }
                        })
                        .to_string();
                        let request = ChannelSendRequest {
                            conversation_id: state.conversation_id.clone(),
                            text: state.accumulated.clone(),
                            message_id: if state.supports_edits {
                                state.message_id.clone()
                            } else {
                                None
                            },
                            is_final: false,
                            metadata_json,
                        };
                        if let Ok(result) = channel_manager
                            .send_channel_message(&state.plugin_id, &request)
                            .await
                        {
                            if state.message_id.is_none() {
                                state.message_id = result.message_id;
                            }
                        }
                        state.last_sent = Instant::now();
                    }
                    MirrorEvent::LlmDone { session_id, text } => {
                        let entry = lookup_entry(&remote, &session_id);
                        let state = streams.remove(&session_id);

                        if let Some(entry) = entry {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let message_id = state.as_ref().and_then(|s| s.message_id.clone());
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "final" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text,
                                message_id,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;

                            let _ = remote.registry().mark_status(
                                &session_id,
                                remote.runtime_id(),
                                &entry.plugin_id,
                                &entry.conversation_id,
                                "idle",
                            );
                            remote.emit(crate::channels::remote::RemoteEvent::new(
                                "agent_done",
                                &entry.plugin_id,
                                &session_id,
                                &entry.conversation_id,
                                remote.runtime_id(),
                            ));
                        }
                    }
                    MirrorEvent::LlmError { session_id, error } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "error" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text: format!("❌ Error: {}", error),
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                            let _ = remote.registry().mark_status(
                                &session_id,
                                remote.runtime_id(),
                                &entry.plugin_id,
                                &entry.conversation_id,
                                "idle",
                            );
                            remote.emit(crate::channels::remote::RemoteEvent::new(
                                "agent_done",
                                &entry.plugin_id,
                                &session_id,
                                &entry.conversation_id,
                                remote.runtime_id(),
                            ));
                        }
                    }
                    MirrorEvent::ToolStarted {
                        session_id,
                        name,
                        args,
                    } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let args_preview = if args.to_string().len() > 300 {
                                format!(
                                    "{}…",
                                    crate::core::truncate_at_char_boundary(&args.to_string(), 300)
                                )
                            } else {
                                args.to_string()
                            };
                            let text = if args_preview.trim().is_empty() {
                                format!("🔧 Running `{}`", name)
                            } else {
                                format!("🔧 Running `{}`\nArgs: {}", name, args_preview)
                            };
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "tool_started" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text,
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                            remote.emit(
                                crate::channels::remote::RemoteEvent::new(
                                    "tool_started",
                                    &entry.plugin_id,
                                    &session_id,
                                    &entry.conversation_id,
                                    remote.runtime_id(),
                                )
                                .with_metadata(serde_json::json!({
                                    "name": name,
                                    "args": args,
                                })),
                            );
                        }
                    }
                    MirrorEvent::ToolCompleted {
                        session_id,
                        name,
                        result,
                        success,
                    } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let result_preview = if result.len() > 400 {
                                format!("{}…", crate::core::truncate_at_char_boundary(&result, 400))
                            } else {
                                result.clone()
                            };
                            let text = if success {
                                format!("✅ Completed `{}`\n{}", name, result_preview)
                            } else {
                                format!("❌ Failed `{}`\n{}", name, result_preview)
                            };
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "tool_completed" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text,
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                            remote.emit(
                                crate::channels::remote::RemoteEvent::new(
                                    if success {
                                        "tool_completed"
                                    } else {
                                        "tool_failed"
                                    },
                                    &entry.plugin_id,
                                    &session_id,
                                    &entry.conversation_id,
                                    remote.runtime_id(),
                                )
                                .with_metadata(serde_json::json!({
                                    "name": name,
                                    "result": result,
                                    "success": success,
                                })),
                            );
                        }
                    }
                    MirrorEvent::AskUser { session_id, prompt } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "ask_user" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text: prompt.clone(),
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                            remote.emit(
                                crate::channels::remote::RemoteEvent::new(
                                    "ask_user",
                                    &entry.plugin_id,
                                    &session_id,
                                    &entry.conversation_id,
                                    remote.runtime_id(),
                                )
                                .with_message(prompt),
                            );
                        }
                    }
                    MirrorEvent::ApprovalRequest { session_id, prompt } => {
                        if let Some(entry) = lookup_entry(&remote, &session_id) {
                            if remote.registry().is_stopped(&session_id) {
                                continue;
                            }
                            let metadata_json = serde_json::json!({
                                "tark": { "mirror": true, "source": "local", "kind": "approval" }
                            })
                            .to_string();
                            let request = ChannelSendRequest {
                                conversation_id: entry.conversation_id.clone(),
                                text: prompt.clone(),
                                message_id: None,
                                is_final: true,
                                metadata_json,
                            };
                            let _ = channel_manager
                                .send_channel_message(&entry.plugin_id, &request)
                                .await;
                            remote.emit(
                                crate::channels::remote::RemoteEvent::new(
                                    "approval_request",
                                    &entry.plugin_id,
                                    &session_id,
                                    &entry.conversation_id,
                                    remote.runtime_id(),
                                )
                                .with_message(prompt),
                            );
                        }
                    }
                }
            }
        });

        mirror
    }

    pub fn user_message(&self, session_id: String, content: String) {
        let _ = self.tx.send(MirrorEvent::UserMessage {
            session_id,
            content,
        });
    }

    pub fn llm_start(&self, session_id: String) {
        let _ = self.tx.send(MirrorEvent::LlmStart { session_id });
    }

    pub fn llm_chunk(&self, session_id: String, chunk: String) {
        let _ = self.tx.send(MirrorEvent::LlmChunk { session_id, chunk });
    }

    pub fn llm_done(&self, session_id: String, text: String) {
        let _ = self.tx.send(MirrorEvent::LlmDone { session_id, text });
    }

    pub fn llm_error(&self, session_id: String, error: String) {
        let _ = self.tx.send(MirrorEvent::LlmError { session_id, error });
    }

    pub fn tool_started(&self, session_id: String, name: String, args: serde_json::Value) {
        let _ = self.tx.send(MirrorEvent::ToolStarted {
            session_id,
            name,
            args,
        });
    }

    pub fn tool_completed(&self, session_id: String, name: String, result: String, success: bool) {
        let _ = self.tx.send(MirrorEvent::ToolCompleted {
            session_id,
            name,
            result,
            success,
        });
    }

    pub fn ask_user(&self, session_id: String, prompt: String) {
        let _ = self.tx.send(MirrorEvent::AskUser { session_id, prompt });
    }

    pub fn approval_request(&self, session_id: String, prompt: String) {
        let _ = self
            .tx
            .send(MirrorEvent::ApprovalRequest { session_id, prompt });
    }
}

fn lookup_entry(remote: &RemoteRuntime, session_id: &str) -> Option<RemoteSessionEntry> {
    remote.registry().get(session_id)
}

#[cfg(test)]
mod tests {
    use super::mirror_streaming_enabled;

    #[test]
    fn test_mirror_streaming_requires_message_id() {
        assert!(!mirror_streaming_enabled(true, None));
        assert!(!mirror_streaming_enabled(false, Some("id")));
        assert!(mirror_streaming_enabled(true, Some("id")));
    }
}
