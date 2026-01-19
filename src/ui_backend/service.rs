//! Application Service - Business Logic
//!
//! This service handles all business logic operations using the service architecture:
//! - ConversationService for chat
//! - SessionService for session lifecycle
//! - CatalogService for provider/model discovery

use anyhow::Result;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::core::attachments::{resolve_file_path, AttachmentConfig, AttachmentManager};
use crate::core::session_manager::SessionManager;
use crate::storage::TarkStorage;

use super::commands::{BuildMode, Command};
use super::conversation::ConversationService;
use super::events::AppEvent;
use super::git_service::GitService;
use super::session_service::SessionService;
use super::state::{ModalType, SharedState};
use super::types::{AttachmentInfo, GitChangeInfo, Message, MessageRole, ModelInfo, ProviderInfo};

/// Application Service - Business Logic Layer
///
/// Coordinates multiple services to provide the full application functionality.
pub struct AppService {
    /// Conversation service for chat operations
    conversation_svc: Option<Arc<ConversationService>>,

    /// Session service for session lifecycle
    session_svc: Option<Arc<SessionService>>,

    /// Shared application state
    state: SharedState,

    /// Event channel for async updates to UI
    event_tx: mpsc::UnboundedSender<AppEvent>,

    /// Working directory
    working_dir: PathBuf,

    /// Attachment manager for handling file attachments
    attachment_manager: AttachmentManager,

    /// BFF Services
    catalog: super::CatalogService,
    storage: super::StorageFacade,
    tools: super::ToolExecutionService,
    git: GitService,
}

impl std::fmt::Debug for AppService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppService")
            .field("conversation_svc", &self.conversation_svc.is_some())
            .field("session_svc", &self.session_svc.is_some())
            .field("state", &self.state)
            .field("working_dir", &self.working_dir)
            .finish()
    }
}

impl AppService {
    /// Create a new application service
    pub fn new(working_dir: PathBuf, event_tx: mpsc::UnboundedSender<AppEvent>) -> Result<Self> {
        Self::new_with_debug(working_dir, event_tx, false)
    }

    /// Create a new application service with debug mode
    pub fn new_with_debug(
        working_dir: PathBuf,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        debug: bool,
    ) -> Result<Self> {
        Self::new_with_options(working_dir, event_tx, None, None, debug)
    }

    /// Create a new application service with full options
    pub fn new_with_options(
        working_dir: PathBuf,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        provider: Option<String>,
        model: Option<String>,
        _debug: bool,
    ) -> Result<Self> {
        let state = SharedState::new();

        // Initialize storage
        let storage_facade = super::StorageFacade::new(&working_dir)?;
        let tark_storage = TarkStorage::new(&working_dir)?;

        // Initialize ChatAgent
        let chat_agent_result = (|| -> Result<crate::agent::ChatAgent> {
            // Create LLM provider
            let provider_name = provider.clone().unwrap_or_else(|| "openai".to_string());
            let llm_provider = crate::llm::create_provider_with_options(
                &provider_name,
                true, // silent
                model.as_deref(),
            )?;

            // Create tool registry
            let tools = crate::tools::ToolRegistry::for_mode(
                working_dir.clone(),
                crate::core::types::AgentMode::Build,
                true, // shell_enabled
            );

            Ok(crate::agent::ChatAgent::new(Arc::from(llm_provider), tools))
        })();

        let (conversation_svc, session_svc) = match chat_agent_result {
            Ok(chat_agent) => {
                // Initialize ConversationService
                let conv_svc = Arc::new(ConversationService::new(chat_agent, event_tx.clone()));

                // Initialize SessionManager
                let session_mgr = SessionManager::new(tark_storage);

                // Initialize SessionService
                let sess_svc = Arc::new(SessionService::new(session_mgr, conv_svc.clone()));

                state.set_llm_connected(true);

                // Set initial provider and model in state
                if let Some(ref prov) = provider {
                    state.set_provider(Some(prov.clone()));
                } else {
                    state.set_provider(Some("openai".to_string()));
                }
                if let Some(ref mdl) = model {
                    state.set_model(Some(mdl.clone()));
                }

                tracing::info!("Services initialized successfully");

                (Some(conv_svc), Some(sess_svc))
            }
            Err(e) => {
                let error_msg = format!("Failed to initialize LLM: {}", e);
                tracing::error!("{}", error_msg);
                state.set_llm_connected(false);

                let system_msg = Message {
                    role: MessageRole::System,
                    content: format!(
                        "⚠️  {}\n\nPlease configure your API key or check your provider settings.",
                        error_msg
                    ),
                    thinking: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(system_msg);

                (None, None)
            }
        };

        // Initialize attachment manager with default config
        let attachment_manager = AttachmentManager::new(AttachmentConfig::default());

        // Initialize BFF services
        let catalog = super::CatalogService::new();
        let tools = super::ToolExecutionService::new(super::commands::AgentMode::default(), None);
        let git = GitService::new(working_dir.clone());

        Ok(Self {
            conversation_svc,
            session_svc,
            state,
            event_tx,
            working_dir,
            attachment_manager,
            catalog,
            storage: storage_facade,
            tools,
            git,
        })
    }

    /// Get the shared state
    pub fn state(&self) -> &SharedState {
        &self.state
    }

    /// Handle a user command
    pub async fn handle_command(&mut self, command: Command) -> Result<()> {
        match command {
            // Application control
            Command::Quit => {
                self.state.set_should_quit(true);
            }

            // Agent mode
            Command::CycleAgentMode => {
                let current = self.state.agent_mode();
                let next = current.next();
                self.state.set_agent_mode(next);

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Agent mode: {}",
                        next.display_name()
                    )))
                    .ok();
            }
            Command::SetAgentMode(mode) => {
                self.state.set_agent_mode(mode);

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Agent mode: {}",
                        mode.display_name()
                    )))
                    .ok();
            }

            // Build mode
            Command::CycleBuildMode => {
                let current = self.state.build_mode();
                let next = current.next();
                self.state.set_build_mode(next);

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Build mode: {}",
                        next.display_name()
                    )))
                    .ok();
            }
            Command::SetBuildMode(mode) => {
                self.state.set_build_mode(mode);

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Build mode: {}",
                        mode.display_name()
                    )))
                    .ok();
            }
            Command::OpenTrustLevelSelector => {
                self.state.set_active_modal(Some(ModalType::TrustLevel));
                self.state
                    .set_trust_level_selected(self.state.trust_level().index());
            }
            Command::SetTrustLevel(level) => {
                self.state.set_trust_level(level);

                let build_mode = match level {
                    crate::tools::TrustLevel::Manual => BuildMode::Manual,
                    crate::tools::TrustLevel::Balanced => BuildMode::Balanced,
                    crate::tools::TrustLevel::Careful => BuildMode::Careful,
                };
                self.state.set_build_mode(build_mode);

                self.state.set_active_modal(None);
                self.event_tx
                    .send(AppEvent::StatusChanged(format!("Trust level: {:?}", level)))
                    .ok();
            }

            // UI toggles
            Command::ToggleSidebar => {
                let visible = !self.state.sidebar_visible();
                self.state.set_sidebar_visible(visible);
            }
            Command::ToggleThinking => {
                let enabled = !self.state.thinking_enabled();
                self.state.set_thinking_enabled(enabled);

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Thinking blocks: {}",
                        if enabled { "enabled" } else { "disabled" }
                    )))
                    .ok();
            }
            Command::SetVimMode(mode) => {
                self.state.set_vim_mode(mode);
            }

            // Input handling
            Command::InsertChar(c) => {
                let mut text = self.state.input_text();
                let cursor = self.state.input_cursor();
                text.insert(cursor, c);
                self.state.set_input_text(text);
                self.state.set_input_cursor(cursor + c.len_utf8());

                // @ triggers file picker
                if c == '@' {
                    self.refresh_file_picker("");
                    self.state.set_file_picker_filter(String::new());
                    self.state.set_file_picker_selected(0);
                    self.state.set_active_modal(Some(ModalType::FilePicker));
                }
            }
            Command::DeleteCharBefore => {
                let mut text = self.state.input_text();
                let cursor = self.state.input_cursor();
                if cursor > 0 {
                    text.remove(cursor - 1);
                    self.state.set_input_text(text);
                    self.state.set_input_cursor(cursor - 1);
                }
            }
            Command::DeleteCharAfter => {
                let mut text = self.state.input_text();
                let cursor = self.state.input_cursor();
                if cursor < text.len() {
                    text.remove(cursor);
                    self.state.set_input_text(text);
                }
            }
            Command::CursorLeft => {
                let cursor = self.state.input_cursor();
                if cursor > 0 {
                    self.state.set_input_cursor(cursor - 1);
                }
            }
            Command::CursorRight => {
                let cursor = self.state.input_cursor();
                let text = self.state.input_text();
                if cursor < text.len() {
                    self.state.set_input_cursor(cursor + 1);
                }
            }
            Command::CursorToLineStart => {
                self.state.set_input_cursor(0);
            }
            Command::CursorToLineEnd => {
                let text = self.state.input_text();
                self.state.set_input_cursor(text.len());
            }
            Command::CursorWordForward => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();

                let mut new_cursor = cursor;
                let chars: Vec<char> = text.chars().collect();

                while new_cursor < chars.len() && !chars[new_cursor].is_whitespace() {
                    new_cursor += chars[new_cursor].len_utf8();
                }

                while new_cursor < chars.len() && chars[new_cursor].is_whitespace() {
                    new_cursor += chars[new_cursor].len_utf8();
                }

                self.state.set_input_cursor(new_cursor.min(text.len()));
            }
            Command::CursorWordBackward => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();

                if cursor == 0 {
                    return Ok(());
                }

                let chars: Vec<char> = text.chars().collect();
                let mut byte_pos = 0;
                let mut positions: Vec<usize> = vec![0];

                for ch in &chars {
                    byte_pos += ch.len_utf8();
                    positions.push(byte_pos);
                }

                let char_idx = positions
                    .iter()
                    .position(|&p| p >= cursor)
                    .unwrap_or(chars.len());
                let mut new_char_idx = char_idx.saturating_sub(1);

                while new_char_idx > 0 && chars[new_char_idx].is_whitespace() {
                    new_char_idx -= 1;
                }

                while new_char_idx > 0 && !chars[new_char_idx - 1].is_whitespace() {
                    new_char_idx -= 1;
                }

                self.state.set_input_cursor(positions[new_char_idx]);
            }
            Command::InsertNewline => {
                let mut text = self.state.input_text();
                let cursor = self.state.input_cursor();
                text.insert(cursor, '\n');
                self.state.set_input_text(text);
                self.state.set_input_cursor(cursor + 1);
            }
            Command::ClearInput => {
                self.state.clear_input();
            }

            // Message sending
            Command::SendMessage(content) => {
                let text = if content.is_empty() {
                    self.state.input_text()
                } else {
                    content
                };

                if text.trim().is_empty() {
                    return Ok(());
                }

                // If already processing, queue the message
                if self.state.llm_processing() {
                    self.state.queue_message(text.clone());
                    self.state.clear_input();

                    // Add queued indicator message
                    let queue_count = self.state.queued_message_count();
                    let queue_msg = Message {
                        role: MessageRole::System,
                        content: format!("⏳ Message queued ({} in queue)", queue_count),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(queue_msg.clone());
                    self.event_tx.send(AppEvent::MessageAdded(queue_msg)).ok();
                    return Ok(());
                }

                // Add user message
                let user_msg = Message {
                    role: MessageRole::User,
                    content: text.clone(),
                    thinking: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                self.state.add_message(user_msg.clone());
                self.event_tx.send(AppEvent::MessageAdded(user_msg)).ok();

                // Add to history (only non-slash commands)
                if !text.starts_with('/') {
                    self.state.add_to_history(text.clone());
                }

                // Clear input
                self.state.clear_input();

                // Send to LLM via ConversationService
                if let Some(ref conv_svc) = self.conversation_svc {
                    self.state.set_llm_processing(true);

                    let conv_svc = conv_svc.clone();
                    let text = text.clone();
                    let event_tx = self.event_tx.clone();
                    let state = self.state.clone();

                    // Spawn task for async message sending
                    tokio::spawn(async move {
                        if let Err(e) = conv_svc.send_message(&text).await {
                            tracing::error!("Failed to send message: {}", e);
                            let error_msg = Message {
                                role: MessageRole::System,
                                content: format!("❌ Failed to send message: {}", e),
                                thinking: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(error_msg.clone());
                            let _ = event_tx.send(AppEvent::MessageAdded(error_msg));
                            let _ = event_tx.send(AppEvent::LlmError(e.to_string()));
                        }
                        state.set_llm_processing(false);

                        // Process next queued message if any
                        if let Some(_next_msg) = state.pop_queued_message() {
                            let _ = event_tx.send(AppEvent::TaskQueueUpdated {
                                count: state.queued_message_count(),
                            });
                            // Trigger sending the queued message
                            // Note: This will be handled by the controller polling for events
                        }
                    });
                } else {
                    tracing::warn!(
                        "Attempted to send message but ConversationService is not initialized"
                    );
                    let error_msg = Message {
                        role: MessageRole::System,
                        content: "⚠️  LLM not connected. Please configure your API key."
                            .to_string(),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(error_msg.clone());
                    self.event_tx.send(AppEvent::MessageAdded(error_msg)).ok();
                }
            }

            // Provider/Model selection
            Command::SelectProvider(provider) => {
                self.set_provider(&provider).await?;
            }
            Command::SelectModel(model) => {
                self.set_model(&model).await?;
            }

            // Context files / Attachments
            Command::AddContextFile(path) => match self.add_attachment(&path) {
                Ok(info) => {
                    self.event_tx
                        .send(AppEvent::ContextFileAdded(info.path.clone()))
                        .ok();
                    self.event_tx
                        .send(AppEvent::StatusChanged(format!(
                            "Added: {} ({})",
                            info.filename, info.size_display
                        )))
                        .ok();
                }
                Err(e) => {
                    let error_msg = Message {
                        role: MessageRole::System,
                        content: format!("Failed to attach file: {}", e),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(error_msg.clone());
                    self.event_tx.send(AppEvent::MessageAdded(error_msg)).ok();
                }
            },
            Command::RemoveContextFile(path) => {
                self.remove_attachment(&path);
                self.state.remove_context_file(&path);
                self.event_tx.send(AppEvent::ContextFileRemoved(path)).ok();
            }
            Command::RemoveContextByIndex(index) => {
                let context_files = self.state.context_files();
                if let Some(file) = context_files.get(index) {
                    let path = file.path.clone();
                    self.remove_attachment(&path);
                    self.state.remove_context_file(&path);
                    self.event_tx.send(AppEvent::ContextFileRemoved(path)).ok();
                }
            }

            // Git operations
            Command::OpenGitDiff(file_path) => match self.git.get_file_diff(&file_path) {
                Ok(diff) => {
                    let msg = Message {
                        role: MessageRole::System,
                        content: format!("Diff for {}:\n\n{}", file_path, diff),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(msg);
                }
                Err(e) => {
                    tracing::error!("Failed to get diff: {}", e);
                }
            },
            Command::StageGitFile(file_path) => match self.git.stage_file(&file_path) {
                Ok(_) => {
                    self.event_tx
                        .send(AppEvent::StatusChanged(format!("Staged: {}", file_path)))
                        .ok();
                    self.refresh_sidebar_data().await;
                }
                Err(e) => {
                    tracing::error!("Failed to stage file: {}", e);
                    self.event_tx
                        .send(AppEvent::StatusChanged(format!("Failed to stage: {}", e)))
                        .ok();
                }
            },

            // File picker navigation
            Command::FilePickerUp => {
                let selected = self.state.file_picker_selected();
                if selected > 0 {
                    self.state.set_file_picker_selected(selected - 1);
                }
            }
            Command::FilePickerDown => {
                let selected = self.state.file_picker_selected();
                let files = self.state.file_picker_files();
                if selected + 1 < files.len() {
                    self.state.set_file_picker_selected(selected + 1);
                }
            }
            Command::FilePickerSelect => {
                let files = self.state.file_picker_files();
                let selected = self.state.file_picker_selected();
                if let Some(file_path) = files.get(selected) {
                    match self.add_attachment(file_path) {
                        Ok(info) => {
                            self.event_tx
                                .send(AppEvent::ContextFileAdded(info.path.clone()))
                                .ok();
                        }
                        Err(e) => {
                            tracing::error!("Failed to add file to context: {}", e);
                        }
                    }
                    self.state.set_active_modal(None);
                }
            }
            Command::UpdateFilePickerFilter(filter) => {
                self.state.set_file_picker_filter(filter.clone());
                self.state.set_file_picker_selected(0);
                self.refresh_file_picker(&filter);
            }

            // Interruption
            Command::Interrupt => {
                if let Some(ref conv_svc) = self.conversation_svc {
                    conv_svc.interrupt();
                }
                self.state.set_llm_processing(false);
                self.event_tx
                    .send(AppEvent::StatusChanged("Interrupted".to_string()))
                    .ok();
            }

            // Approval actions
            Command::ApproveOperation => {
                if let Some(mut approval) = self.state.pending_approval() {
                    approval.approve();
                    self.state.set_pending_approval(Some(approval));
                    self.state.set_active_modal(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Operation approved".to_string()))
                        .ok();
                }
            }
            Command::DenyOperation => {
                if let Some(mut approval) = self.state.pending_approval() {
                    approval.reject();
                    self.state.set_pending_approval(Some(approval));
                    self.state.set_active_modal(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Operation denied".to_string()))
                        .ok();
                }
            }

            // Questionnaire actions
            Command::QuestionUp => {}
            Command::QuestionDown => {}
            Command::QuestionToggle => {}
            Command::QuestionSubmit => {
                if let Some(q) = self.state.active_questionnaire() {
                    let answer = q.get_answer();
                    self.state.answer_questionnaire(answer);
                    self.state.set_active_questionnaire(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Answer submitted".to_string()))
                        .ok();
                }
            }

            // History navigation
            Command::HistoryPrevious => {
                self.state.navigate_history_prev();
            }
            Command::HistoryNext => {
                self.state.navigate_history_next();
            }

            // Session operations
            Command::NewSession => {
                if let Some(ref session_svc) = self.session_svc {
                    let session_svc = session_svc.clone();
                    let event_tx = self.event_tx.clone();
                    let state = self.state.clone();

                    tokio::spawn(async move {
                        match session_svc.create_new().await {
                            Ok(info) => {
                                state.clear_input();
                                let _ = event_tx.send(AppEvent::StatusChanged(format!(
                                    "New session: {}",
                                    info.session_id
                                )));
                            }
                            Err(e) => {
                                tracing::error!("Failed to create session: {}", e);
                            }
                        }
                    });
                }
            }
            Command::SwitchSession(session_id) => {
                if let Some(ref session_svc) = self.session_svc {
                    let session_svc = session_svc.clone();
                    let event_tx = self.event_tx.clone();
                    let _state = self.state.clone();

                    tokio::spawn(async move {
                        match session_svc.switch_to(&session_id).await {
                            Ok(info) => {
                                let _ = event_tx.send(AppEvent::StatusChanged(format!(
                                    "Switched to: {}",
                                    info.session_id
                                )));
                            }
                            Err(e) => {
                                tracing::error!("Failed to switch session: {}", e);
                            }
                        }
                    });
                }
            }

            // Not yet implemented
            _ => {
                tracing::debug!("Command not yet implemented: {:?}", command);
            }
        }

        Ok(())
    }

    /// Get available LLM providers
    pub async fn get_providers(&self) -> Vec<ProviderInfo> {
        self.catalog.list_providers().await
    }

    /// Get available models for a provider
    pub async fn get_models(&self, provider: &str) -> Vec<ModelInfo> {
        self.catalog.list_models(provider).await
    }

    /// Get available tools for current agent mode
    pub fn get_tools(&self) -> Vec<super::tool_execution::ToolInfo> {
        let mode = self.state.agent_mode();
        self.tools.list_tools(mode)
    }

    /// Refresh file picker with workspace files
    pub fn refresh_file_picker(&self, filter: &str) {
        use crate::core::attachments::search_workspace_files;

        let files = search_workspace_files(&self.working_dir, filter, true);
        let file_paths: Vec<String> = files
            .iter()
            .filter_map(|p| {
                p.strip_prefix(&self.working_dir)
                    .ok()
                    .map(|rel| rel.to_string_lossy().to_string())
            })
            .collect();

        self.state.set_file_picker_files(file_paths);
        tracing::debug!("File picker refreshed with {} files", files.len());
    }

    /// Get current session information
    pub async fn get_session_info(&self) -> Option<super::types::SessionInfo> {
        if let Some(ref session_svc) = self.session_svc {
            session_svc.get_current().await.into()
        } else {
            None
        }
    }

    /// Create a new session
    pub fn new_session(&self) -> Result<()> {
        // TODO: Implement through SessionService
        Ok(())
    }

    /// Switch to a different session
    pub fn switch_session(&self, _session_id: &str) -> Result<()> {
        // TODO: Implement through SessionService
        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<crate::storage::SessionMeta>> {
        self.storage.list_sessions().map_err(|e| e.into())
    }

    /// Export current session to file
    pub fn export_session(&self, _path: &std::path::Path) -> Result<()> {
        // TODO: Implement session export through SessionService
        Ok(())
    }

    /// Import session from file
    pub fn import_session(&self, _path: &std::path::Path) -> Result<super::types::SessionInfo> {
        // TODO: Implement session import through SessionService
        Err(anyhow::anyhow!("Import not yet implemented"))
    }

    /// Set the active provider
    pub async fn set_provider(&mut self, provider: &str) -> Result<()> {
        self.state.set_provider(Some(provider.to_string()));
        self.event_tx
            .send(AppEvent::ProviderChanged(provider.to_string()))
            .ok();
        Ok(())
    }

    /// Set the active model
    pub async fn set_model(&mut self, model: &str) -> Result<()> {
        self.state.set_model(Some(model.to_string()));
        self.event_tx
            .send(AppEvent::ModelChanged(model.to_string()))
            .ok();
        Ok(())
    }

    // ========== Attachment Management APIs ==========

    /// Add a file attachment by path
    pub fn add_attachment(&mut self, path_str: &str) -> Result<AttachmentInfo> {
        use crate::core::attachments::format_size;

        let resolved_path = resolve_file_path(path_str)
            .map_err(|e| anyhow::anyhow!("Failed to resolve path: {}", e))?;

        let attachment = self
            .attachment_manager
            .attach_file(&resolved_path)
            .map_err(|e| anyhow::anyhow!("Failed to attach file: {}", e))?;

        let info = AttachmentInfo {
            filename: attachment.filename.clone(),
            path: resolved_path.display().to_string(),
            size_display: format_size(attachment.size),
            size_bytes: attachment.size,
            type_icon: attachment.file_type.icon().to_string(),
            mime_type: attachment.file_type.mime_type().to_string(),
            is_image: matches!(
                attachment.file_type,
                crate::core::attachments::AttachmentType::Image { .. }
            ),
            added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        };

        use super::types::ContextFile;
        let token_count = info.size_bytes / 4;

        let context_file = ContextFile {
            path: info.path.clone(),
            size: info.size_bytes as usize,
            token_count: token_count as usize,
            added_at: info.added_at.clone(),
        };
        self.state.add_context_file(context_file);

        Ok(info)
    }

    /// Remove a file attachment by path
    pub fn remove_attachment(&mut self, path: &str) {
        let pending = self.attachment_manager.pending();
        if let Some(pos) = pending.iter().position(|a| {
            a.filename == path
                || pending
                    .iter()
                    .any(|a| path.ends_with(&a.filename) || a.filename.ends_with(path))
        }) {
            self.attachment_manager.remove_at(pos);
        }
    }

    /// Get list of pending attachments
    pub fn get_attachments(&self) -> Vec<AttachmentInfo> {
        use crate::core::attachments::format_size;

        self.attachment_manager
            .pending()
            .iter()
            .map(|a| AttachmentInfo {
                filename: a.filename.clone(),
                path: match &a.content {
                    crate::core::attachments::AttachmentContent::Path(p) => p.display().to_string(),
                    _ => a.filename.clone(),
                },
                size_display: format_size(a.size),
                size_bytes: a.size,
                type_icon: a.file_type.icon().to_string(),
                mime_type: a.file_type.mime_type().to_string(),
                is_image: matches!(
                    a.file_type,
                    crate::core::attachments::AttachmentType::Image { .. }
                ),
                added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
            })
            .collect()
    }

    /// Clear all pending attachments
    pub fn clear_attachments(&mut self) {
        self.attachment_manager.clear();
        self.state.clear_context_files();
    }

    /// Check if there are any pending attachments
    pub fn has_attachments(&self) -> bool {
        !self.attachment_manager.is_empty()
    }

    /// Refresh all sidebar data
    pub async fn refresh_sidebar_data(&self) {
        if let Some(info) = self.get_session_info().await {
            self.state.set_session(Some(info));
        }

        let git_changes = crate::tui_new::git_info::get_git_changes(&self.working_dir);
        self.state.set_git_changes(git_changes);
    }

    /// Get git changes for the working directory
    pub fn get_git_changes(&self) -> Vec<GitChangeInfo> {
        crate::tui_new::git_info::get_git_changes(&self.working_dir)
    }

    /// Clear conversation history
    pub async fn clear_conversation(&self) {
        if let Some(ref conv_svc) = self.conversation_svc {
            conv_svc.clear_history().await;
        }
    }

    /// Load and restore the current active session
    pub async fn load_active_session(&self) -> Result<()> {
        if let Some(ref session_svc) = self.session_svc {
            // Try to get current session info
            if let Some(session_info) = session_svc.get_current().await.into() {
                self.state.set_session(Some(session_info));
                tracing::info!("Restored active session");
            }
        }
        Ok(())
    }

    /// Save the current session
    pub async fn save_current_session(&self) -> Result<()> {
        if let Some(ref session_svc) = self.session_svc {
            session_svc.save_current().await?;
            tracing::info!("Saved current session");
        }
        Ok(())
    }
}
