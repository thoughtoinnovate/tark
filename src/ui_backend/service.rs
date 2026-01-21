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
use super::state::{FocusedComponent, ModalType, SharedState, VimMode};
use super::types::{AttachmentInfo, GitChangeInfo, Message, MessageRole, ModelInfo, ProviderInfo};
use crate::tui_new::widgets::command_autocomplete::SlashCommand;

fn char_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or_else(|| s.len())
}

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

    /// Interaction channel receiver (ask_user / approval)
    interaction_rx: Option<crate::tools::InteractionReceiver>,

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
    pub fn working_dir(&self) -> &PathBuf {
        &self.working_dir
    }

    fn default_model_for_provider(
        config: &crate::config::Config,
        provider: &str,
    ) -> Option<String> {
        match provider {
            "tark_sim" => Some(config.llm.tark_sim.model.clone()),
            "openai" => Some(config.llm.openai.model.clone()),
            "claude" => Some(config.llm.claude.model.clone()),
            "ollama" => Some(config.llm.ollama.model.clone()),
            "copilot" => Some(config.llm.copilot.model.clone()),
            "openrouter" => Some(config.llm.openrouter.model.clone()),
            "google" | "gemini" => Some(config.llm.gemini.model.clone()),
            _ => None,
        }
    }
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

        let (interaction_tx, interaction_rx) = crate::tools::interaction_channel();
        let approvals_path = tark_storage
            .load_current_session()
            .ok()
            .map(|session| tark_storage.session_dir(&session.id).join("approvals.json"));

        // Initialize ChatAgent
        let chat_agent_result = (|| -> Result<crate::agent::ChatAgent> {
            // Create LLM provider
            let provider_name = provider.clone().unwrap_or_else(|| "tark_sim".to_string());
            let llm_provider = crate::llm::create_provider_with_options(
                &provider_name,
                true, // silent
                model.as_deref(),
            )?;

            // Create tool registry
            let tools = crate::tools::ToolRegistry::for_mode_with_services(
                working_dir.clone(),
                crate::core::types::AgentMode::Build,
                true, // shell_enabled
                Some(interaction_tx.clone()),
                None,
                approvals_path.clone(),
            );

            Ok(crate::agent::ChatAgent::new(Arc::from(llm_provider), tools))
        })();

        let (conversation_svc, session_svc) = match chat_agent_result {
            Ok(chat_agent) => {
                // Initialize ConversationService
                let conv_svc = Arc::new(ConversationService::new_with_interaction(
                    chat_agent,
                    event_tx.clone(),
                    Some(interaction_tx.clone()),
                    approvals_path.clone(),
                ));

                // Initialize SessionManager
                let session_mgr = SessionManager::new(tark_storage);

                // Initialize SessionService
                let sess_svc = Arc::new(SessionService::new(session_mgr, conv_svc.clone()));

                state.set_llm_connected(true);

                // Set initial provider and model in state
                if let Some(ref prov) = provider {
                    state.set_provider(Some(prov.clone()));
                } else {
                    state.set_provider(Some("tark_sim".to_string()));
                }

                // Set initial model in state
                if let Some(ref mdl) = model {
                    state.set_model(Some(mdl.clone()));
                } else {
                    // Default to tark_llm for tark_sim provider
                    let default_model =
                        if provider.as_deref() == Some("tark_sim") || provider.is_none() {
                            "tark_llm"
                        } else {
                            // For other providers, don't set a model yet - wait for user selection
                            ""
                        };
                    if !default_model.is_empty() {
                        state.set_model(Some(default_model.to_string()));
                    }
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
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
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
            interaction_rx: Some(interaction_rx),
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

    /// Take the interaction receiver (ask_user / approval)
    pub fn take_interaction_receiver(&mut self) -> Option<crate::tools::InteractionReceiver> {
        self.interaction_rx.take()
    }

    /// Get storage facade
    pub fn storage(&self) -> &super::StorageFacade {
        &self.storage
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
                self.tools.set_mode(next);
                if let Some(ref conv_svc) = self.conversation_svc {
                    conv_svc.update_mode(self.working_dir.clone(), next).await;
                }
                if let Some(ref session_svc) = self.session_svc {
                    if let (Some(provider), Some(model)) =
                        (self.state.current_provider(), self.state.current_model())
                    {
                        session_svc.update_metadata(provider, model, next).await;
                    }
                }

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Agent mode: {}",
                        next.display_name()
                    )))
                    .ok();
            }
            Command::SetAgentMode(mode) => {
                self.state.set_agent_mode(mode);
                self.tools.set_mode(mode);
                if let Some(ref conv_svc) = self.conversation_svc {
                    conv_svc.update_mode(self.working_dir.clone(), mode).await;
                }
                if let Some(ref session_svc) = self.session_svc {
                    if let (Some(provider), Some(model)) =
                        (self.state.current_provider(), self.state.current_model())
                    {
                        session_svc.update_metadata(provider, model, mode).await;
                    }
                }

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

                self.tools.set_trust_level(level).await;
                if let Some(ref conv_svc) = self.conversation_svc {
                    let _ = conv_svc.set_trust_level(level).await;
                }

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
                self.state.set_pending_operator(None);
                if mode == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let cursor = self.state.input_cursor();
                    self.state.set_input_selection(cursor, cursor);
                } else if mode != VimMode::Visual {
                    self.state.clear_input_selection();
                }
            }

            // Message scrolling
            Command::ScrollDown => {
                let current = self.state.messages_scroll_offset();
                let total_lines = self.state.messages_total_lines();
                let viewport_height = self.state.messages_viewport_height();
                let max_offset = total_lines.saturating_sub(viewport_height);
                let next = current.saturating_add(1).min(max_offset);
                self.state.set_messages_scroll_offset(next);
            }
            Command::ScrollUp => {
                let total_lines = self.state.messages_total_lines();
                let viewport_height = self.state.messages_viewport_height();
                let max_offset = total_lines.saturating_sub(viewport_height);
                let current = self.state.messages_scroll_offset();
                let normalized = if current == usize::MAX {
                    max_offset
                } else {
                    current
                };
                if normalized > 0 {
                    self.state
                        .set_messages_scroll_offset(normalized.saturating_sub(1));
                }
            }
            Command::YankMessage => {
                let messages = self.state.messages();
                let idx = self.state.focused_message();
                if let Some(msg) = messages.get(idx) {
                    if let Ok(mut clipboard) = arboard::Clipboard::new() {
                        let _ = clipboard.set_text(msg.content.clone());
                        self.state
                            .set_status_message(Some("Yanked message".to_string()));
                    }
                }
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Messages
                {
                    self.state.set_vim_mode(VimMode::Normal);
                }
            }
            Command::NextMessage => {
                let msg_count = self.state.message_count();
                if msg_count == 0 {
                    return Ok(());
                }
                let current = self.state.focused_message();
                let next = (current + 1).min(msg_count - 1);
                self.state.set_focused_message(next);
                if next == msg_count - 1 {
                    self.state.scroll_to_bottom();
                }
            }
            Command::PrevMessage => {
                let msg_count = self.state.message_count();
                if msg_count == 0 {
                    return Ok(());
                }
                let current = self.state.focused_message();
                let prev = current.saturating_sub(1);
                self.state.set_focused_message(prev);
                if prev == 0 {
                    self.state.set_messages_scroll_offset(0);
                }
            }
            Command::ToggleMessageCollapse => {
                let idx = self.state.focused_message();
                self.state.toggle_message_collapse(idx);
            }

            // Input handling
            Command::InsertChar(c) => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let mut chars: Vec<char> = text.chars().collect();
                chars.insert(cursor, c);
                let new_text: String = chars.into_iter().collect();
                self.state.set_input_text(new_text);
                self.state.set_input_cursor(cursor + 1);

                // Update autocomplete filter if input starts with '/'
                let input_text = self.state.input_text();
                if input_text.starts_with('/') {
                    let filter = input_text.trim_start_matches('/').to_string();
                    if !self.state.autocomplete_active() {
                        self.state.activate_autocomplete(&filter);
                    } else {
                        self.state.update_autocomplete_filter(&filter);
                    }
                } else {
                    self.state.deactivate_autocomplete();
                }

                // @ triggers file picker
                if c == '@' {
                    self.refresh_file_picker("");
                    self.state.set_file_picker_filter(String::new());
                    self.state.set_file_picker_selected(0);
                    self.state.set_active_modal(Some(ModalType::FilePicker));
                }

                if self.state.active_modal() == Some(ModalType::FilePicker) {
                    let input_text = self.state.input_text();
                    if let Some(at_pos) = input_text.rfind('@') {
                        let filter = input_text[at_pos + 1..].to_string();
                        self.state.set_file_picker_filter(filter.clone());
                        self.state.set_file_picker_selected(0);
                        self.refresh_file_picker(&filter);
                    }
                }
            }
            Command::InsertText(text) => {
                let mut input = self.state.input_text();
                let cursor = self.state.input_cursor();
                let byte_cursor = char_to_byte(&input, cursor);
                input.insert_str(byte_cursor, &text);
                self.state.set_input_text(input);
                self.state.set_input_cursor(cursor + text.chars().count());
            }
            Command::DeleteCharBefore => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                if self.state.active_modal() == Some(ModalType::FilePicker) {
                    let byte_cursor = char_to_byte(&text, cursor);
                    if let Some(at_pos) = text[..byte_cursor].rfind('@') {
                        let mut new_text = text.clone();
                        new_text.drain(at_pos + 1..byte_cursor);
                        self.state.set_input_text(new_text);
                        let at_char = text[..=at_pos].chars().count();
                        self.state.set_input_cursor(at_char);
                        let filter = text[at_pos + 1..byte_cursor].to_string();
                        self.state.set_file_picker_filter(filter);
                        return Ok(());
                    }
                }

                if cursor > 0 {
                    let mut chars: Vec<char> = text.chars().collect();
                    chars.remove(cursor - 1);
                    let new_text: String = chars.into_iter().collect();
                    self.state.set_input_text(new_text);
                    self.state.set_input_cursor(cursor - 1);
                }
                let input_text = self.state.input_text();
                if input_text.starts_with('/') {
                    let filter = input_text.trim_start_matches('/').to_string();
                    if !self.state.autocomplete_active() {
                        self.state.activate_autocomplete(&filter);
                    } else {
                        self.state.update_autocomplete_filter(&filter);
                    }
                } else {
                    self.state.deactivate_autocomplete();
                }
            }
            Command::DeleteCharAfter => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let chars: Vec<char> = text.chars().collect();
                if cursor < chars.len() {
                    let mut chars = chars;
                    chars.remove(cursor);
                    let new_text: String = chars.into_iter().collect();
                    self.state.set_input_text(new_text);
                }
            }
            Command::CursorLeft => {
                let cursor = self.state.input_cursor();
                if cursor > 0 {
                    self.state.set_input_cursor(cursor - 1);
                }
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::CursorRight => {
                let cursor = self.state.input_cursor();
                let text = self.state.input_text();
                let chars: Vec<char> = text.chars().collect();
                if cursor < chars.len() {
                    self.state.set_input_cursor(cursor + 1);
                }
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::CursorToLineStart => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let chars: Vec<char> = text.chars().collect();
                let mut idx = cursor;
                while idx > 0 && chars[idx - 1] != '\n' {
                    idx -= 1;
                }
                self.state.set_input_cursor(idx);
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::CursorToLineEnd => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let chars: Vec<char> = text.chars().collect();
                let mut idx = cursor;
                while idx < chars.len() && chars[idx] != '\n' {
                    idx += 1;
                }
                self.state.set_input_cursor(idx);
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::CursorWordForward => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();

                let mut new_cursor = cursor;
                let chars: Vec<char> = text.chars().collect();

                while new_cursor < chars.len() && !chars[new_cursor].is_whitespace() {
                    new_cursor += 1;
                }

                while new_cursor < chars.len() && chars[new_cursor].is_whitespace() {
                    new_cursor += 1;
                }

                self.state.set_input_cursor(new_cursor.min(chars.len()));
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::CursorWordBackward => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();

                if cursor == 0 {
                    return Ok(());
                }

                let chars: Vec<char> = text.chars().collect();
                let mut new_char_idx = cursor.saturating_sub(1);

                while new_char_idx > 0 && chars[new_char_idx].is_whitespace() {
                    new_char_idx -= 1;
                }

                while new_char_idx > 0 && !chars[new_char_idx - 1].is_whitespace() {
                    new_char_idx -= 1;
                }

                self.state.set_input_cursor(new_char_idx);
                if self.state.vim_mode() == VimMode::Visual
                    && self.state.focused_component() == FocusedComponent::Input
                {
                    let end = self.state.input_cursor();
                    if let Some((start, _)) = self.state.input_selection() {
                        self.state.set_input_selection(start, end);
                    }
                }
            }
            Command::InsertNewline => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let mut chars: Vec<char> = text.chars().collect();
                chars.insert(cursor, '\n');
                let new_text: String = chars.into_iter().collect();
                self.state.set_input_text(new_text);
                self.state.set_input_cursor(cursor + 1);
            }
            Command::DeleteSelection => {
                if let Some((start, end)) = self.state.input_selection() {
                    let mut text = self.state.input_text();
                    let start_byte = char_to_byte(&text, start);
                    let end_byte = char_to_byte(&text, end);
                    if start < end && end_byte <= text.len() {
                        text.drain(start_byte..end_byte);
                        self.state.set_input_text(text);
                        self.state.set_input_cursor(start);
                    }
                    self.state.clear_input_selection();
                    self.state.set_vim_mode(VimMode::Normal);
                }
            }
            Command::YankSelection => {
                if let Some((start, end)) = self.state.input_selection() {
                    let text = self.state.input_text();
                    if start < end && end <= text.len() {
                        let selection = text[start..end].to_string();
                        if let Ok(mut clipboard) = arboard::Clipboard::new() {
                            let _ = clipboard.set_text(selection);
                        }
                    }
                    self.state
                        .set_status_message(Some("Yanked selection".to_string()));
                    self.state.set_vim_mode(VimMode::Normal);
                }
            }
            Command::DeleteLine => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let chars: Vec<char> = text.chars().collect();
                let mut line_start = cursor;
                while line_start > 0 && chars[line_start - 1] != '\n' {
                    line_start -= 1;
                }
                let mut line_end = cursor;
                while line_end < chars.len() && chars[line_end] != '\n' {
                    line_end += 1;
                }
                let mut chars = chars;
                chars.drain(line_start..line_end);
                let new_text: String = chars.into_iter().collect();
                self.state.set_input_text(new_text);
                self.state.set_input_cursor(line_start);
            }
            Command::DeleteWord => {
                let text = self.state.input_text();
                let cursor = self.state.input_cursor();
                let mut chars: Vec<char> = text.chars().collect();
                if cursor >= chars.len() {
                    return Ok(());
                }
                let mut end = cursor;
                while end < chars.len() && chars[end].is_whitespace() {
                    end += 1;
                }
                while end < chars.len() && !chars[end].is_whitespace() {
                    end += 1;
                }
                if end > cursor {
                    chars.drain(cursor..end);
                }
                let new_text: String = chars.into_iter().collect();
                self.state.set_input_text(new_text);
                self.state.set_input_cursor(cursor);
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
                let is_processing = self.state.llm_processing();

                // Log the processing state check
                if let Some(logger) = crate::debug_logger() {
                    let correlation_id = self
                        .state
                        .current_correlation_id()
                        .unwrap_or_else(|| "none".to_string());
                    let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                        correlation_id,
                        crate::LogCategory::Service,
                        "send_message_check",
                    )
                    .with_data(serde_json::json!({
                        "llm_processing": is_processing,
                        "text": text,
                        "will_queue": is_processing
                    }));
                    logger.log(entry);
                }

                if is_processing {
                    self.state.queue_message(text.clone());
                    self.state.clear_input();

                    // Add queued indicator message
                    let queue_count = self.state.queued_message_count();
                    tracing::info!("Message queued: '{}', queue_count={}", text, queue_count);

                    // Log to debug logger as well
                    if let Some(logger) = crate::debug_logger() {
                        let correlation_id = self
                            .state
                            .current_correlation_id()
                            .unwrap_or_else(|| "none".to_string());
                        let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                            correlation_id,
                            crate::LogCategory::Service,
                            "message_queued",
                        )
                        .with_data(serde_json::json!({
                            "text": text,
                            "queue_count": queue_count
                        }));
                        logger.log(entry);
                    }

                    let queue_msg = Message {
                        role: MessageRole::System,
                        content: format!("⏳ Message queued ({} in queue)", queue_count),
                        thinking: None,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(queue_msg.clone());
                    self.state.scroll_to_bottom();
                    self.event_tx.send(AppEvent::MessageAdded(queue_msg)).ok();
                    self.update_tasks(); // Update sidebar tasks with new queued item
                    return Ok(());
                }

                // Add user message
                let user_msg = Message {
                    role: MessageRole::User,
                    content: text.clone(),
                    thinking: None,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                self.state.add_message(user_msg.clone());
                self.record_session_message(&user_msg).await;
                self.state.scroll_to_bottom();
                self.event_tx.send(AppEvent::MessageAdded(user_msg)).ok();

                // Add to history (only non-slash commands)
                if !text.starts_with('/') {
                    self.state.add_to_history(text.clone());
                }

                // Clear input
                self.state.clear_input();

                // Update context usage after adding user message
                self.refresh_sidebar_data().await;

                // Generate new correlation ID for this request
                let correlation_id = self.state.generate_new_correlation_id();

                if let Some(logger) = crate::debug_logger() {
                    let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                        correlation_id.clone(),
                        crate::LogCategory::Service,
                        "llm_send_start",
                    )
                    .with_data(serde_json::json!({
                        "provider": self.state.current_provider(),
                        "model": self.state.current_model(),
                        "queued_messages": self.state.queued_message_count(),
                        "content_length": text.len()
                    }));
                    logger.log(entry);
                }

                // Send to LLM via ConversationService
                if let Some(ref conv_svc) = self.conversation_svc {
                    self.state.set_llm_processing(true);
                    self.update_tasks(); // Update sidebar tasks

                    let conv_svc = conv_svc.clone();
                    let text = text.clone();
                    let event_tx = self.event_tx.clone();
                    let state = self.state.clone();

                    // Log the user message with correlation_id
                    if let Some(logger) = crate::debug_logger() {
                        let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                            correlation_id.clone(),
                            crate::LogCategory::Service,
                            "user_message",
                        )
                        .with_data(serde_json::json!({
                            "content_preview": if text.len() > 100 {
                                format!("{}...", &text[..100])
                            } else {
                                text.clone()
                            },
                            "content_length": text.len()
                        }));
                        logger.log(entry);
                    }

                    // Spawn task for async message sending
                    // Store correlation_id and session_id for this request to prevent race conditions
                    let this_correlation_id = correlation_id.clone();
                    state.set_processing_correlation_id(Some(this_correlation_id.clone()));
                    // Track which session this request belongs to
                    let this_session_id = state.session().map(|s| s.session_id.clone());
                    state.set_processing_session_id(this_session_id.clone());

                    if let Some(logger) = crate::debug_logger() {
                        let entry = crate::DebugLogEntry::new(
                            correlation_id.clone(),
                            crate::LogCategory::Service,
                            "processing_session_set",
                        )
                        .with_data(serde_json::json!({
                            "processing_session_id": this_session_id,
                            "correlation_id": this_correlation_id
                        }));
                        logger.log(entry);
                    }

                    tokio::spawn(async move {
                        if let Err(e) = conv_svc
                            .send_message(&text, Some(correlation_id.clone()))
                            .await
                        {
                            tracing::error!("Failed to send message: {}", e);
                            let error_msg = Message {
                                role: MessageRole::System,
                                content: format!("❌ Failed to send message: {}", e),
                                thinking: None,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(error_msg.clone());
                            state.scroll_to_bottom();
                            let _ = event_tx.send(AppEvent::MessageAdded(error_msg));
                            let _ = event_tx.send(AppEvent::LlmError(e.to_string()));
                            if let Some(logger) = crate::debug_logger() {
                                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                                    correlation_id.clone(),
                                    crate::LogCategory::Service,
                                    "llm_send_error",
                                )
                                .with_data(serde_json::json!({
                                    "error": e.to_string()
                                }));
                                logger.log(entry);
                            }
                        } else if let Some(logger) = crate::debug_logger() {
                            let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                                correlation_id.clone(),
                                crate::LogCategory::Service,
                                "llm_send_done",
                            )
                            .with_data(serde_json::json!({
                                "provider": state.current_provider(),
                                "model": state.current_model()
                            }));
                            logger.log(entry);
                        }

                        // Only reset processing if we're still the active request
                        // This prevents race conditions when a new message is sent while
                        // the old one is being cancelled
                        if state.processing_correlation_id().as_ref() == Some(&this_correlation_id)
                        {
                            state.set_llm_processing(false);
                            state.set_processing_correlation_id(None);
                            state.set_processing_session_id(None);
                        }

                        // Notify controller if there are queued messages to process
                        // Note: The controller handles actually popping and sending queued messages
                        // via the LlmComplete event handler - we just notify here
                        if state.queued_message_count() > 0 {
                            let _ = event_tx.send(AppEvent::TaskQueueUpdated {
                                count: state.queued_message_count(),
                            });
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
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(error_msg.clone());
                    self.record_session_message(&error_msg).await;
                    self.state.scroll_to_bottom();
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
                    self.refresh_sidebar_data().await;
                }
                Err(e) => {
                    let error_msg = Message {
                        role: MessageRole::System,
                        content: format!("Failed to attach file: {}", e),
                        thinking: None,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(error_msg.clone());
                    self.state.scroll_to_bottom();
                    self.event_tx.send(AppEvent::MessageAdded(error_msg)).ok();
                }
            },
            Command::RemoveContextFile(path) => {
                self.remove_attachment(&path);
                self.state.remove_context_file(&path);
                self.event_tx.send(AppEvent::ContextFileRemoved(path)).ok();
                self.refresh_sidebar_data().await;
            }
            Command::RemoveContextByIndex(index) => {
                let context_files = self.state.context_files();
                if let Some(file) = context_files.get(index) {
                    let path = file.path.clone();
                    self.remove_attachment(&path);
                    self.state.remove_context_file(&path);
                    self.event_tx.send(AppEvent::ContextFileRemoved(path)).ok();
                    self.refresh_sidebar_data().await;
                }
            }

            // Git operations
            Command::OpenGitDiff(file_path) => match self.git.get_file_diff(&file_path) {
                Ok(diff) => {
                    let msg = Message {
                        role: MessageRole::System,
                        content: format!("Diff for {}:\n\n{}", file_path, diff),
                        thinking: None,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
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

            // Autocomplete
            Command::AutocompleteSelect | Command::AutocompleteConfirm => {
                let filter = self.state.autocomplete_filter();
                let matches = SlashCommand::find_matches(&filter);
                if let Some(selected) = matches.get(self.state.autocomplete_selected()) {
                    let completion = format!("/{}", selected.name());
                    self.state.set_input_text(completion.clone());
                    self.state.set_input_cursor(completion.len());
                    self.state.deactivate_autocomplete();
                }
            }
            Command::AutocompleteUp => {
                let filter = self.state.autocomplete_filter();
                let matches = SlashCommand::find_matches(&filter);
                if !matches.is_empty() {
                    self.state.autocomplete_move_up();
                }
            }
            Command::AutocompleteDown => {
                let filter = self.state.autocomplete_filter();
                let matches = SlashCommand::find_matches(&filter);
                self.state.autocomplete_move_down(matches.len());
            }
            Command::AutocompleteCancel => {
                self.state.deactivate_autocomplete();
            }

            // Interruption
            Command::Interrupt => {
                if self.state.llm_processing() {
                    if let Some(ref conv_svc) = self.conversation_svc {
                        // Use async version to properly reset streaming state
                        conv_svc.interrupt_and_reset().await;
                    }
                    self.state.set_llm_processing(false);
                    self.state.set_processing_correlation_id(None);
                    self.state.set_processing_session_id(None);

                    // Add a visible message in the chat
                    self.state.add_message(crate::ui_backend::Message {
                        role: crate::ui_backend::MessageRole::System,
                        content: "⚠️ Operation interrupted (Ctrl+C)".to_string(),
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        collapsed: false,
                        thinking: None,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                    });

                    self.event_tx
                        .send(AppEvent::StatusChanged("Ready".to_string()))
                        .ok();
                }
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
            Command::ApproveSession => {
                if let Some(mut approval) = self.state.pending_approval() {
                    approval.approve();
                    self.state.set_pending_approval(Some(approval));
                    self.state.set_active_modal(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Approved for session".to_string()))
                        .ok();
                }
            }
            Command::ApproveAlways => {
                if let Some(mut approval) = self.state.pending_approval() {
                    approval.approve();
                    self.state.set_pending_approval(Some(approval));
                    self.state.set_active_modal(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Approved always".to_string()))
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
            Command::DenyAlways => {
                if let Some(mut approval) = self.state.pending_approval() {
                    approval.reject();
                    self.state.set_pending_approval(Some(approval));
                    self.state.set_active_modal(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged("Denied always".to_string()))
                        .ok();
                }
            }

            // Questionnaire actions
            Command::QuestionUp => {
                self.state.questionnaire_focus_prev();
            }
            Command::QuestionDown => {
                self.state.questionnaire_focus_next();
            }
            Command::QuestionToggle => {
                self.state.questionnaire_toggle_focused();
            }
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
            Command::QuestionCancel => {
                // Cancel/skip the questionnaire - user can answer via chat instead
                if self.state.active_questionnaire().is_some() {
                    self.state.cancel_questionnaire();
                    self.state.set_active_questionnaire(None);
                    self.event_tx
                        .send(AppEvent::StatusChanged(
                            "Question skipped - answer via chat".to_string(),
                        ))
                        .ok();
                }
            }
            Command::CancelAgent => {
                // Emergency stop for ongoing agent work (double-ESC)
                // Uses the async interrupt to properly reset streaming state
                if self.state.llm_processing() {
                    if let Some(ref conv_svc) = self.conversation_svc {
                        conv_svc.interrupt_and_reset().await;
                    }
                    self.state.set_llm_processing(false);
                    self.state.set_processing_correlation_id(None);
                    self.state.set_processing_session_id(None);

                    // Clear queued messages - user explicitly cancelled
                    let discarded_count = self.state.clear_message_queue();

                    // Add a visible message in the chat
                    let cancel_msg = if discarded_count > 0 {
                        format!(
                            "⚠️ Agent operation cancelled (double-ESC) — {} queued message{} discarded",
                            discarded_count,
                            if discarded_count == 1 { "" } else { "s" }
                        )
                    } else {
                        "⚠️ Agent operation cancelled (double-ESC)".to_string()
                    };

                    self.state.add_message(crate::ui_backend::Message {
                        role: crate::ui_backend::MessageRole::System,
                        content: cancel_msg,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        collapsed: false,
                        thinking: None,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                    });

                    // Also update status bar
                    self.event_tx
                        .send(AppEvent::StatusChanged("Ready".to_string()))
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
                    let conv_svc = self.conversation_svc.clone();
                    let approvals_base = self.working_dir.clone();
                    let catalog = self.catalog;

                    tokio::spawn(async move {
                        match session_svc.create_new().await {
                            Ok(info) => {
                                // Clear UI state for new session
                                state.clear_messages();
                                state.clear_input();
                                state.clear_context_files(); // Context files are session-specific
                                                             // Reset cost/token tracking for new session
                                state.set_session_cost_total(0.0);
                                state.set_session_tokens_total(0);
                                state.set_session_cost_by_model(Vec::new());
                                state.set_session_tokens_by_model(Vec::new());
                                let _ = state.clear_message_queue();

                                // Set session info in state (critical for session tracking during mid-request switches)
                                state.set_session(Some(info.clone()));

                                let _ = event_tx.send(AppEvent::StatusChanged(format!(
                                    "New session: {}",
                                    info.session_id
                                )));

                                let config = crate::config::Config::load().unwrap_or_default();
                                let default_provider = config.llm.default_provider.clone();
                                let default_model = config.llm.tark_sim.model.clone();

                                if !default_provider.is_empty() {
                                    state.set_provider(Some(default_provider.clone()));
                                }
                                if !default_model.is_empty() {
                                    state.set_model(Some(default_model.clone()));
                                }

                                let models = catalog.list_models(&default_provider).await;
                                state.set_available_models(models);

                                let _ = session_svc
                                    .update_metadata(
                                        default_provider.clone(),
                                        default_model.clone(),
                                        crate::core::types::AgentMode::Build,
                                    )
                                    .await;

                                // Save session to persist provider/model metadata
                                if let Err(e) = session_svc.save_current().await {
                                    tracing::warn!("Failed to save new session: {}", e);
                                }

                                if let Some(ref conv_svc) = conv_svc {
                                    if !default_provider.is_empty() {
                                        match crate::llm::create_provider_with_options(
                                            &default_provider,
                                            true,
                                            state.current_model().as_deref(),
                                        ) {
                                            Ok(llm_provider) => {
                                                let _ = conv_svc
                                                    .update_llm_provider(std::sync::Arc::from(
                                                        llm_provider,
                                                    ))
                                                    .await;
                                                tracing::info!(
                                                    "Switched to provider: {} model: {:?}",
                                                    default_provider,
                                                    state.current_model()
                                                );
                                            }
                                            Err(e) => {
                                                tracing::error!(
                                                    "Failed to create provider '{}': {}",
                                                    default_provider,
                                                    e
                                                );
                                            }
                                        }
                                    }
                                }

                                if let Some(conv_svc) = conv_svc {
                                    let approvals_path = approvals_base
                                        .join(".tark")
                                        .join("sessions")
                                        .join(&info.session_id)
                                        .join("approvals.json");
                                    let _ =
                                        conv_svc.update_approval_storage_path(approvals_path).await;
                                }
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
                    let conv_svc = self.conversation_svc.clone();
                    let approvals_base = self.working_dir.clone();

                    tokio::spawn(async move {
                        match session_svc.switch_to(&session_id).await {
                            Ok(_info) => {
                                // Messages are restored via restore_from_session in session_svc
                                // UI will need to be synced separately via event
                                let _ = event_tx.send(AppEvent::SessionSwitched {
                                    session_id: session_id.clone(),
                                });
                                if let Some(conv_svc) = conv_svc {
                                    let approvals_path = approvals_base
                                        .join(".tark")
                                        .join("sessions")
                                        .join(&session_id)
                                        .join("approvals.json");
                                    let _ =
                                        conv_svc.update_approval_storage_path(approvals_path).await;
                                }
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
                p.strip_prefix(&self.working_dir).ok().map(|rel| {
                    let mut path = rel.to_string_lossy().to_string();
                    if p.is_dir() && !path.ends_with('/') {
                        path.push('/');
                    }
                    path
                })
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
    /// Note: For full functionality, use `handle_command(Command::SwitchSession(id))` instead.
    /// This method is kept for sync API compatibility.
    pub fn switch_session(&self, session_id: &str) -> Result<()> {
        // Send the command to the async event loop
        let session_id = session_id.to_string();
        let _ = self.event_tx.send(AppEvent::StatusChanged(format!(
            "Switching to session {}",
            session_id
        )));
        // The actual switch happens when Command::SwitchSession is processed
        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<crate::storage::SessionMeta>> {
        self.storage.list_sessions().map_err(|e| e.into())
    }

    /// Delete a session by ID
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        if let Some(ref session_svc) = self.session_svc {
            session_svc.delete(session_id).await?;
            Ok(())
        } else {
            anyhow::bail!("Session service unavailable")
        }
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

    /// Set the trust level for tool execution
    pub async fn set_trust_level(&self, level: crate::tools::TrustLevel) {
        self.state.set_trust_level(level);

        let build_mode = match level {
            crate::tools::TrustLevel::Manual => BuildMode::Manual,
            crate::tools::TrustLevel::Balanced => BuildMode::Balanced,
            crate::tools::TrustLevel::Careful => BuildMode::Careful,
        };
        self.state.set_build_mode(build_mode);

        self.tools.set_trust_level(level).await;
        if let Some(ref conv_svc) = self.conversation_svc {
            let _ = conv_svc.set_trust_level(level).await;
        }
    }

    /// Set the active provider
    pub async fn set_provider(&mut self, provider: &str) -> Result<()> {
        let previous_provider = self.state.current_provider();
        let current_model = self.state.current_model();
        self.state.set_provider(Some(provider.to_string()));

        // Persist to session metadata
        if let Some(ref session_svc) = self.session_svc {
            if let Some(model) = self.state.current_model() {
                let mode = self.state.agent_mode();
                session_svc
                    .update_metadata(provider.to_string(), model.clone(), mode)
                    .await;
                if let Some(logger) = crate::debug_logger() {
                    let correlation_id = self
                        .state
                        .current_correlation_id()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                        correlation_id,
                        crate::LogCategory::Service,
                        "provider_model_metadata_saved",
                    )
                    .with_data(serde_json::json!({
                        "provider": provider,
                        "model": model,
                        "agent_mode": format!("{:?}", mode)
                    }));
                    logger.log(entry);
                }
            }
        }

        if let Some(ref conv_svc) = self.conversation_svc {
            match crate::llm::create_provider_with_options(
                provider,
                true,
                self.state.current_model().as_deref(),
            ) {
                Ok(llm_provider) => {
                    if let Err(e) = conv_svc.update_llm_provider(Arc::from(llm_provider)).await {
                        self.state.set_llm_connected(false);
                        let _ = self.event_tx.send(AppEvent::LlmError(e.to_string()));
                    } else {
                        self.state.set_llm_connected(true);
                    }
                }
                Err(e) => {
                    self.state.set_llm_connected(false);
                    let _ = self.event_tx.send(AppEvent::LlmError(e.to_string()));
                }
            }
        }

        if let Some(logger) = crate::debug_logger() {
            let correlation_id = self
                .state
                .current_correlation_id()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let session_id = self.state.session().map(|s| s.session_id);
            let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                correlation_id,
                crate::LogCategory::Service,
                "provider_set",
            )
            .with_data(serde_json::json!({
                "previous_provider": previous_provider,
                "new_provider": provider,
                "current_model": current_model,
                "session_id": session_id,
                "agent_mode": format!("{:?}", self.state.agent_mode())
            }));
            logger.log(entry);
        }

        self.event_tx
            .send(AppEvent::ProviderChanged(provider.to_string()))
            .ok();
        Ok(())
    }

    /// Set the active model
    pub async fn set_model(&mut self, model: &str) -> Result<()> {
        let previous_model = self.state.current_model();
        let current_provider = self.state.current_provider();
        self.state.set_model(Some(model.to_string()));

        // Persist to session metadata
        if let Some(ref session_svc) = self.session_svc {
            if let Some(provider) = self.state.current_provider() {
                let mode = self.state.agent_mode();
                session_svc
                    .update_metadata(provider, model.to_string(), mode)
                    .await;
                if let Some(logger) = crate::debug_logger() {
                    let correlation_id = self
                        .state
                        .current_correlation_id()
                        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                    let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                        correlation_id,
                        crate::LogCategory::Service,
                        "provider_model_metadata_saved",
                    )
                    .with_data(serde_json::json!({
                        "provider": self.state.current_provider(),
                        "model": model,
                        "agent_mode": format!("{:?}", mode)
                    }));
                    logger.log(entry);
                }
            }
        }

        if let (Some(conv_svc), Some(provider)) = (
            self.conversation_svc.as_ref(),
            self.state.current_provider(),
        ) {
            match crate::llm::create_provider_with_options(provider.as_str(), true, Some(model)) {
                Ok(llm_provider) => {
                    if let Err(e) = conv_svc.update_llm_provider(Arc::from(llm_provider)).await {
                        self.state.set_llm_connected(false);
                        let _ = self.event_tx.send(AppEvent::LlmError(e.to_string()));
                    } else {
                        self.state.set_llm_connected(true);
                    }
                }
                Err(e) => {
                    self.state.set_llm_connected(false);
                    let _ = self.event_tx.send(AppEvent::LlmError(e.to_string()));
                }
            }
        }

        if let Some(logger) = crate::debug_logger() {
            let correlation_id = self
                .state
                .current_correlation_id()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let session_id = self.state.session().map(|s| s.session_id);
            let entry: crate::DebugLogEntry =
                crate::DebugLogEntry::new(correlation_id, crate::LogCategory::Service, "model_set")
                    .with_data(serde_json::json!({
                        "previous_model": previous_model,
                        "new_model": model,
                        "current_provider": current_provider,
                        "session_id": session_id,
                        "agent_mode": format!("{:?}", self.state.agent_mode())
                    }));
            logger.log(entry);
        }

        self.event_tx
            .send(AppEvent::ModelChanged(model.to_string()))
            .ok();
        Ok(())
    }

    pub async fn record_session_message(&self, message: &Message) {
        if let Some(ref session_svc) = self.session_svc {
            if let Err(err) = session_svc.append_message(message).await {
                tracing::warn!("Failed to persist session message: {}", err);
            }
        }
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

        if let Some(ref conv_svc) = self.conversation_svc {
            let usage = conv_svc.context_usage().await;
            self.state.set_tokens(usage.used_tokens, usage.max_tokens);
        }

        let git_changes = crate::tui_new::git_info::get_git_changes(&self.working_dir);
        self.state.set_git_changes(git_changes);

        // Update tasks from current state
        self.update_tasks();
    }

    /// Silently interrupt agent processing without adding a visible message
    /// Used when switching sessions to prevent responses from bleeding into new session
    pub async fn silent_interrupt(&self) {
        if self.state.llm_processing() {
            if let Some(ref conv_svc) = self.conversation_svc {
                conv_svc.interrupt_and_reset().await;
            }
            self.state.set_llm_processing(false);
            self.state.set_processing_correlation_id(None);
            self.state.set_processing_session_id(None);
            // No visible message added - this is intentional for session switching
        }
    }

    /// Update session usage stats (cost and tokens) and persist to disk
    pub async fn update_session_usage(&self, cost: f64, input_tokens: usize, output_tokens: usize) {
        if let Some(ref session_svc) = self.session_svc {
            if let Err(e) = session_svc
                .update_usage(cost, input_tokens, output_tokens)
                .await
            {
                tracing::warn!("Failed to update session usage: {}", e);
            }
        }
    }

    /// Update tasks in sidebar from current processing state and message queue
    fn update_tasks(&self) {
        use crate::ui_backend::types::{TaskInfo, TaskStatus};

        let mut tasks = Vec::new();

        // If LLM is processing, find the last user message as the active task
        if self.state.llm_processing() {
            let messages = self.state.messages();
            // Find the last user message (that's what's being processed)
            if let Some(last_user_msg) = messages.iter().rev().find(|m| m.role == MessageRole::User)
            {
                // Truncate long messages for display
                let task_name = if last_user_msg.content.len() > 50 {
                    format!("{}...", &last_user_msg.content[..47])
                } else {
                    last_user_msg.content.clone()
                };

                tasks.push(TaskInfo {
                    id: format!("active_{}", tasks.len()),
                    name: task_name,
                    status: TaskStatus::Active,
                    created_at: last_user_msg.timestamp.clone(),
                });
            }
        }

        // Add queued messages as queued tasks
        let queued_messages = self.state.queued_messages();
        for (idx, msg) in queued_messages.iter().enumerate() {
            let task_name = if msg.len() > 50 {
                format!("{}...", &msg[..47])
            } else {
                msg.clone()
            };

            tasks.push(TaskInfo {
                id: format!("queued_{}", idx),
                name: task_name,
                status: TaskStatus::Queued,
                created_at: chrono::Local::now().format("%H:%M:%S").to_string(),
            });
        }

        self.state.set_tasks(tasks);
    }

    /// Get git changes for the working directory
    pub fn get_git_changes(&self) -> Vec<GitChangeInfo> {
        crate::tui_new::git_info::get_git_changes(&self.working_dir)
    }

    /// Clear conversation history
    pub async fn clear_conversation(&self) {
        if let Some(ref session_svc) = self.session_svc {
            if let Err(err) = session_svc.clear_current_messages().await {
                tracing::warn!("Failed to clear current session messages: {}", err);
            }
            return;
        }
        if let Some(ref conv_svc) = self.conversation_svc {
            conv_svc.clear_history().await;
        }
    }

    /// Load and restore the current active session
    pub async fn load_active_session(&self) -> Result<()> {
        if let Some(ref session_svc) = self.session_svc {
            // Try to get current session info
            if let Some(session_info) = session_svc.get_current().await.into() {
                self.state.set_session(Some(session_info.clone()));
                if let Ok(session) = self.storage.load_session(&session_info.session_id) {
                    let messages = SessionService::session_messages_to_ui(&session);
                    self.state.set_messages(messages);

                    // Restore provider and model from session if set
                    if !session.provider.is_empty() {
                        self.state.set_provider(Some(session.provider.clone()));
                    }
                    if !session.model.is_empty() {
                        self.state.set_model(Some(session.model.clone()));
                    }
                    if !session.provider.is_empty() {
                        let models = self.get_models(&session.provider).await;
                        self.state.set_available_models(models);
                    }
                    if let Some(ref conv_svc) = self.conversation_svc {
                        if !session.provider.is_empty() {
                            if let Ok(llm_provider) = crate::llm::create_provider_with_options(
                                &session.provider,
                                true,
                                if session.model.is_empty() {
                                    None
                                } else {
                                    Some(session.model.as_str())
                                },
                            ) {
                                let _ = conv_svc
                                    .update_llm_provider(std::sync::Arc::from(llm_provider))
                                    .await;
                            }
                        }
                    }
                }
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

    /// Internal method to update LLM provider without state updates
    /// Used when restoring session state (state is updated separately)
    pub async fn set_provider_internal(&self, provider: &str, model: Option<&str>) -> Result<()> {
        if let Some(ref conv_svc) = self.conversation_svc {
            match crate::llm::create_provider_with_options(provider, true, model) {
                Ok(llm_provider) => {
                    conv_svc
                        .update_llm_provider(std::sync::Arc::from(llm_provider))
                        .await?;
                    self.state.set_llm_connected(true);
                    tracing::info!("Updated LLM provider: {} model: {:?}", provider, model);
                }
                Err(e) => {
                    self.state.set_llm_connected(false);
                    tracing::error!("Failed to create provider '{}': {}", provider, e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }
}
