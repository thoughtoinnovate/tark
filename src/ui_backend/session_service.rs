//! Session Service - Orchestrates session lifecycle with conversation restoration
//!
//! Coordinates SessionManager and ConversationService to handle:
//! - Session creation and switching
//! - Conversation restoration when switching sessions
//! - Session export/import
//! - Session metadata management

use std::path::Path;
use std::sync::Arc;

use crate::core::session_manager::{SessionManager, SessionStats};
use crate::core::types::AgentMode;
use crate::storage::SessionMeta;

use super::conversation::ConversationService;
use super::errors::StorageError;
use super::types::{ArchiveChunkInfo, Message, MessageRole, SessionInfo};

/// Session Service
///
/// Orchestrates session operations and ensures conversation state
/// is properly synchronized when switching sessions.
pub struct SessionService {
    /// Session manager for persistence
    session_mgr: Arc<tokio::sync::RwLock<SessionManager>>,
    /// Conversation service for restoring chat state
    conversation_svc: Arc<ConversationService>,
}

fn session_model_count(session: &crate::storage::ChatSession) -> usize {
    let count = session
        .cost_by_model
        .len()
        .max(session.tokens_by_model.len());
    if count == 0 && (session.total_cost > 0.0 || session.input_tokens + session.output_tokens > 0)
    {
        1
    } else {
        count
    }
}

impl SessionService {
    /// Create a new session service
    pub fn new(session_mgr: SessionManager, conversation_svc: Arc<ConversationService>) -> Self {
        Self {
            session_mgr: Arc::new(tokio::sync::RwLock::new(session_mgr)),
            conversation_svc,
        }
    }

    /// Get current session information
    pub async fn get_current(&self) -> SessionInfo {
        let mgr = self.session_mgr.read().await;
        let stats = mgr.stats();
        let session = mgr.current();

        SessionInfo {
            session_id: stats.session_id,
            session_name: if session.name.is_empty() {
                format!("Session {}", session.created_at.format("%Y-%m-%d %H:%M"))
            } else {
                session.name.clone()
            },
            total_cost: session.total_cost,
            model_count: session_model_count(session),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Create a new session
    ///
    /// This saves the current session, creates a new one, and clears the conversation.
    pub async fn create_new(&self) -> Result<SessionInfo, StorageError> {
        // Clear conversation first
        self.conversation_svc.clear_history().await;

        // Create new session
        let session_id = {
            let mut mgr = self.session_mgr.write().await;
            let session = mgr
                .create_new()
                .map_err(|e| StorageError::Other(e.into()))?;
            session.id.clone()
        };

        Ok(SessionInfo {
            session_id,
            session_name: "New Session".to_string(),
            total_cost: 0.0,
            model_count: 0,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    /// Switch to a different session
    ///
    /// This saves the current session, loads the target session,
    /// and restores the conversation history including tool calls and thinking.
    pub async fn switch_to(&self, session_id: &str) -> Result<SessionInfo, StorageError> {
        // Load the session
        let session = {
            let mut mgr = self.session_mgr.write().await;
            let loaded_session = mgr
                .switch_to(session_id)
                .map_err(|e| StorageError::Other(e.into()))?;
            loaded_session.clone()
        };

        // Restore conversation from session
        self.conversation_svc.restore_from_session(&session).await;

        let session_name = if session.name.is_empty() {
            format!("Session {}", session.created_at.format("%Y-%m-%d %H:%M"))
        } else {
            session.name.clone()
        };

        Ok(SessionInfo {
            session_id: session.id.clone(),
            session_name,
            total_cost: session.total_cost,
            model_count: session_model_count(&session),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    /// List all sessions
    pub async fn list_all(&self) -> Result<Vec<SessionMeta>, StorageError> {
        let mgr = self.session_mgr.read().await;
        mgr.list_all().map_err(|e| StorageError::Other(e.into()))
    }

    /// Archive older messages for the current session.
    pub async fn archive_old_messages(
        &self,
        keep_recent: usize,
    ) -> Result<Option<ArchiveChunkInfo>, StorageError> {
        let mut mgr = self.session_mgr.write().await;
        let archived = mgr
            .archive_old_messages(keep_recent)
            .map_err(|e| StorageError::Other(e.into()))?;
        mgr.auto_save_if_needed()
            .map_err(|e| StorageError::Other(e.into()))?;
        Ok(archived.map(ArchiveChunkInfo::from))
    }

    /// List archived conversation chunks for the current session.
    pub async fn list_archive_chunks(&self) -> Result<Vec<ArchiveChunkInfo>, StorageError> {
        let mgr = self.session_mgr.read().await;
        let chunks = mgr
            .list_archive_chunks()
            .map_err(|e| StorageError::Other(e.into()))?;
        Ok(chunks.into_iter().map(ArchiveChunkInfo::from).collect())
    }

    /// Load a specific archive chunk by filename.
    pub async fn load_archive_chunk(&self, filename: &str) -> Result<Vec<Message>, StorageError> {
        let mgr = self.session_mgr.read().await;
        let messages = mgr
            .load_archive_chunk(filename)
            .map_err(|e| StorageError::Other(e.into()))?;
        Ok(Self::messages_to_ui(&messages, false))
    }

    /// Delete a session
    ///
    /// Cannot delete the current session.
    pub async fn delete(&self, session_id: &str) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        mgr.delete(session_id)
            .map_err(|e| StorageError::Other(e.into()))
    }

    /// Save the current session
    pub async fn save_current(&self) -> Result<(), StorageError> {
        let mgr = self.session_mgr.read().await;
        mgr.save().map_err(|e| StorageError::Other(e.into()))
    }

    /// Clear current session messages and conversation history
    pub async fn clear_current_messages(&self) -> Result<(), StorageError> {
        // Clear conversation first
        self.conversation_svc.clear_history().await;

        let mut mgr = self.session_mgr.write().await;
        mgr.current_mut().clear_messages();
        mgr.save().map_err(|e| StorageError::Other(e.into()))
    }

    /// Auto-save if needed (has unsaved changes)
    pub async fn auto_save_if_needed(&self) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        mgr.auto_save_if_needed()
            .map_err(|e| StorageError::Other(e.into()))
    }

    /// Append a UI message to the current session and auto-save
    pub async fn append_message(&self, message: &Message) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        let role = match message.role {
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
            MessageRole::Tool => "tool",
            MessageRole::Thinking => "thinking",
            MessageRole::System => "system",
        };

        // Set session name from first user message if not set
        if role == "user" && mgr.current().name.is_empty() {
            mgr.current_mut().set_name_from_prompt(&message.content);
        }

        let session_message = crate::storage::SessionMessage {
            role: role.to_string(),
            content: message.content.clone(),
            timestamp: chrono::Utc::now(),
            provider: message.provider.clone(),
            model: message.model.clone(),
            context_transient: message.context_transient,
            tool_call_id: None,
            tool_calls: Vec::new(),
            thinking_content: message.thinking.clone(),
            segments: Vec::new(),
        };
        mgr.append_message(session_message);
        mgr.auto_save_if_needed()
            .map_err(|e| StorageError::Other(e.into()))
    }

    /// Update the last tool message for a given tool name with new content
    ///
    /// This is used to persist the final tool status (✓ or ✗) when a tool completes,
    /// replacing the initial "running" (⋯) status that was saved when the tool started.
    pub async fn update_last_tool_message(
        &self,
        tool_name: &str,
        new_content: String,
    ) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        let session = mgr.current_mut();

        // Find the last tool message that matches this tool name
        // Tool message format: "status|name|content"
        for msg in session.messages.iter_mut().rev() {
            if msg.role == "tool" {
                let parts: Vec<&str> = msg.content.splitn(3, '|').collect();
                if parts.len() >= 2 && parts[1] == tool_name {
                    // Update this message in place
                    msg.content = new_content;
                    break;
                }
            }
        }

        mgr.auto_save_if_needed()
            .map_err(|e| StorageError::Other(e.into()))
    }

    /// Export current session to a file
    pub async fn export(&self, path: &Path) -> Result<(), StorageError> {
        let mgr = self.session_mgr.read().await;
        let json = mgr
            .export_json()
            .map_err(|e| StorageError::Other(e.into()))?;

        std::fs::write(path, json).map_err(|e| StorageError::Other(e.into()))?;
        Ok(())
    }

    /// Import a session from a file
    pub async fn import(&self, path: &Path) -> Result<SessionInfo, StorageError> {
        let json = std::fs::read_to_string(path).map_err(|e| StorageError::Other(e.into()))?;

        let session = {
            let mut mgr = self.session_mgr.write().await;
            mgr.import_from_json(&json)
                .map_err(|e| StorageError::Other(e.into()))?
                .clone()
        };

        // Restore conversation from imported session
        self.conversation_svc.restore_from_session(&session).await;

        let session_name = if session.name.is_empty() {
            format!("Session {}", session.created_at.format("%Y-%m-%d %H:%M"))
        } else {
            session.name.clone()
        };

        Ok(SessionInfo {
            session_id: session.id.clone(),
            session_name,
            total_cost: session.total_cost,
            model_count: session_model_count(&session),
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        })
    }

    /// Update session metadata (provider, model, mode)
    pub async fn update_metadata(&self, provider: String, model: String, mode: AgentMode) {
        let mut mgr = self.session_mgr.write().await;
        mgr.update_metadata(provider, model, mode);
    }

    /// Update session usage stats (cost and tokens) and save
    pub async fn update_usage(
        &self,
        model: &str,
        cost: f64,
        input_tokens: usize,
        output_tokens: usize,
    ) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        let session = mgr.current_mut();
        session.total_cost += cost;
        session.input_tokens += input_tokens;
        session.output_tokens += output_tokens;
        let total_tokens = input_tokens + output_tokens;
        if let Some(entry) = session
            .cost_by_model
            .iter_mut()
            .find(|(name, _)| name == model)
        {
            entry.1 += cost;
        } else {
            session.cost_by_model.push((model.to_string(), cost));
        }
        if let Some(entry) = session
            .tokens_by_model
            .iter_mut()
            .find(|(name, _)| name == model)
        {
            entry.1 += total_tokens;
        } else {
            session
                .tokens_by_model
                .push((model.to_string(), total_tokens));
        }
        drop(mgr); // Release lock before saving

        // Save to persist the changes
        let mgr = self.session_mgr.read().await;
        mgr.save().map_err(|e| StorageError::Other(e.into()))
    }

    /// Clear accumulated cost and token usage for the current session
    pub async fn clear_usage(&self) -> Result<(), StorageError> {
        let mut mgr = self.session_mgr.write().await;
        let session = mgr.current_mut();
        session.total_cost = 0.0;
        session.input_tokens = 0;
        session.output_tokens = 0;
        session.cost_by_model.clear();
        session.tokens_by_model.clear();
        drop(mgr); // Release lock before saving

        let mgr = self.session_mgr.read().await;
        mgr.save().map_err(|e| StorageError::Other(e.into()))
    }

    /// Get session statistics
    pub async fn get_stats(&self) -> SessionStats {
        let mgr = self.session_mgr.read().await;
        mgr.stats()
    }

    /// Check if a session ID is the current session
    pub async fn is_current(&self, session_id: &str) -> bool {
        let mgr = self.session_mgr.read().await;
        mgr.is_current(session_id)
    }

    /// Convert session messages to UI messages including tool calls and thinking
    ///
    /// This also sanitizes tool messages: any "⋯" (running) status is converted to "?" (interrupted)
    /// since tools cannot actually be running after a session restart.
    pub fn session_messages_to_ui(session: &crate::storage::ChatSession) -> Vec<Message> {
        let is_remote_session = session.id.starts_with("channel_");
        Self::messages_to_ui(&session.messages, is_remote_session)
    }

    fn messages_to_ui(
        messages: &[crate::storage::SessionMessage],
        is_remote_session: bool,
    ) -> Vec<Message> {
        messages
            .iter()
            .flat_map(|msg| {
                let mut messages = Vec::new();
                let timestamp = msg.timestamp.format("%H:%M:%S").to_string();

                // Convert role
                let role = match msg.role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "tool" => MessageRole::Tool,
                    "thinking" => MessageRole::Thinking,
                    _ => MessageRole::System,
                };

                // Sanitize tool message content: convert "running" status to "interrupted"
                // since tools can't actually be running after restart
                let content = if role == MessageRole::Tool && msg.content.starts_with("⋯|") {
                    // Replace "⋯|" with "?|" to indicate interrupted/unknown status
                    format!("?{}", &msg.content[3..]) // "⋯" is 3 bytes in UTF-8
                } else {
                    msg.content.clone()
                };
                let is_remote_message =
                    is_remote_session && matches!(role, MessageRole::User | MessageRole::Assistant);

                if msg.segments.is_empty() {
                    messages.push(Message {
                        role,
                        content,
                        thinking: msg.thinking_content.clone(),
                        collapsed: false,
                        timestamp: timestamp.clone(),
                        remote: is_remote_message,
                        provider: msg.provider.clone(),
                        model: msg.model.clone(),
                        context_transient: msg.context_transient,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                    });
                } else {
                    for seg in &msg.segments {
                        match seg {
                            crate::storage::SegmentRecord::Text(text) => messages.push(Message {
                                role,
                                content: text.clone(),
                                thinking: None,
                                collapsed: false,
                                timestamp: timestamp.clone(),
                                remote: is_remote_message,
                                provider: msg.provider.clone(),
                                model: msg.model.clone(),
                                context_transient: msg.context_transient,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                            }),
                            crate::storage::SegmentRecord::Tool(idx) => {
                                if let Some(tool_call) = msg.tool_calls.get(*idx) {
                                    let tool_content = if let Some(error) = &tool_call.error {
                                        format!(
                                            "Tool: {}\n\nArgs:\n{}\n\nError:\n{}",
                                            tool_call.tool,
                                            serde_json::to_string_pretty(&tool_call.args)
                                                .unwrap_or_default(),
                                            error
                                        )
                                    } else {
                                        format!(
                                            "Tool: {}\n\nArgs:\n{}\n\nResult:\n{}",
                                            tool_call.tool,
                                            serde_json::to_string_pretty(&tool_call.args)
                                                .unwrap_or_default(),
                                            tool_call.result_preview
                                        )
                                    };
                                    messages.push(Message {
                                        role: MessageRole::Tool,
                                        content: tool_content,
                                        thinking: None,
                                        collapsed: true,
                                        timestamp: timestamp.clone(),
                                        remote: false,
                                        provider: msg.provider.clone(),
                                        model: msg.model.clone(),
                                        context_transient: msg.context_transient,
                                        tool_calls: Vec::new(),
                                        segments: Vec::new(),
                                        tool_args: None,
                                    });
                                }
                            }
                        }
                    }
                }

                if let Some(ref thinking) = msg.thinking_content {
                    if !thinking.is_empty() && msg.segments.is_empty() {
                        messages.push(Message {
                            role: MessageRole::Thinking,
                            content: thinking.clone(),
                            thinking: None,
                            collapsed: true,
                            timestamp: timestamp.clone(),
                            remote: false,
                            provider: msg.provider.clone(),
                            model: msg.model.clone(),
                            context_transient: msg.context_transient,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                        });
                    }
                }

                messages
            })
            .collect()
    }
}

impl From<crate::storage::ArchiveChunkMeta> for ArchiveChunkInfo {
    fn from(meta: crate::storage::ArchiveChunkMeta) -> Self {
        Self {
            filename: meta.filename,
            created_at: meta.created_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            sequence: meta.sequence,
            message_count: meta.message_count,
        }
    }
}
