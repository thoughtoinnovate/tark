//! Application Service - Business Logic
//!
//! This service handles all business logic operations, delegating to the appropriate
//! backend modules (AgentBridge, Storage, ToolRegistry, etc.).

use anyhow::Result;
use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::core::agent_bridge::{AgentBridge, AgentEvent as BridgeEvent};
use crate::core::attachments::{resolve_file_path, AttachmentConfig, AttachmentManager};

use super::commands::{AgentMode, BuildMode, Command};
use super::events::AppEvent;
use super::state::SharedState;
use super::types::{AttachmentInfo, GitChangeInfo, Message, MessageRole, ModelInfo, ProviderInfo};

/// Convert AgentBridge events to AppEvents
fn bridge_event_to_app_event(event: BridgeEvent) -> Option<AppEvent> {
    match event {
        BridgeEvent::Started => Some(AppEvent::LlmStarted),
        BridgeEvent::TextChunk(chunk) => Some(AppEvent::LlmTextChunk(chunk)),
        BridgeEvent::ThinkingChunk(chunk) => Some(AppEvent::LlmThinkingChunk(chunk)),
        BridgeEvent::ToolCallStarted { tool, args } => {
            Some(AppEvent::ToolStarted { name: tool, args })
        }
        BridgeEvent::ToolCallCompleted {
            tool,
            result_preview,
        } => Some(AppEvent::ToolCompleted {
            name: tool,
            result: result_preview,
        }),
        BridgeEvent::ToolCallFailed { tool, error } => {
            Some(AppEvent::ToolFailed { name: tool, error })
        }
        BridgeEvent::Completed(info) => Some(AppEvent::LlmCompleted {
            text: info.text,
            input_tokens: info.input_tokens,
            output_tokens: info.output_tokens,
        }),
        BridgeEvent::Error(err) => Some(AppEvent::LlmError(err)),
        BridgeEvent::Interrupted => Some(AppEvent::LlmInterrupted),
        _ => None, // Other events not mapped yet
    }
}

/// Application Service - Business Logic Layer
///
/// This service sits between the UI and the core backend, providing a clean API
/// for all business operations without exposing UI-specific details.
pub struct AppService {
    /// Agent bridge for LLM communication
    agent_bridge: Option<AgentBridge>,

    /// Shared application state
    state: SharedState,

    /// Event channel for async updates to UI
    event_tx: mpsc::UnboundedSender<AppEvent>,

    /// Receiver for agent bridge events
    agent_event_rx: Option<mpsc::UnboundedReceiver<BridgeEvent>>,

    /// Working directory
    working_dir: PathBuf,

    /// Attachment manager for handling file attachments
    attachment_manager: AttachmentManager,
}

impl std::fmt::Debug for AppService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppService")
            .field("agent_bridge", &self.agent_bridge.is_some())
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
        debug: bool,
    ) -> Result<Self> {
        let state = SharedState::new();

        // Initialize AgentBridge with debug support
        let (agent_bridge, agent_event_rx) = match AgentBridge::with_provider_and_interaction(
            working_dir.clone(),
            provider.clone(),
            model.clone(),
            debug,
        ) {
            Ok((bridge, _interaction_rx)) => {
                state.set_llm_connected(true);

                // Set provider/model in state
                if let Some(ref p) = provider {
                    state.set_provider(Some(p.clone()));
                    tracing::info!("LLM provider set to: {}", p);
                }
                if let Some(ref m) = model {
                    state.set_model(Some(m.clone()));
                    tracing::info!("LLM model set to: {}", m);
                }

                tracing::info!("AgentBridge initialized successfully");
                (Some(bridge), None) // TODO: Wire up agent events
            }
            Err(e) => {
                let error_msg = format!("Failed to initialize LLM: {}", e);
                tracing::error!("{}", error_msg);
                state.set_llm_connected(false);

                // Add error message to state so user sees it
                use super::types::{Message, MessageRole};
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

        Ok(Self {
            agent_bridge,
            state,
            event_tx,
            agent_event_rx,
            working_dir,
            attachment_manager,
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

                // Update AgentBridge tool registry for new mode
                if let Some(ref mut bridge) = self.agent_bridge {
                    let mode_enum = match next {
                        AgentMode::Build => crate::tui::agent_bridge::AgentMode::Build,
                        AgentMode::Plan => crate::tui::agent_bridge::AgentMode::Plan,
                        AgentMode::Ask => crate::tui::agent_bridge::AgentMode::Ask,
                    };
                    let _ = bridge.set_mode(mode_enum);
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

                // Update AgentBridge tool registry for new mode
                if let Some(ref mut bridge) = self.agent_bridge {
                    let mode_enum = match mode {
                        AgentMode::Build => crate::tui::agent_bridge::AgentMode::Build,
                        AgentMode::Plan => crate::tui::agent_bridge::AgentMode::Plan,
                        AgentMode::Ask => crate::tui::agent_bridge::AgentMode::Ask,
                    };
                    let _ = bridge.set_mode(mode_enum);
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

                // Map BuildMode to TrustLevel in AgentBridge
                if let Some(ref mut bridge) = self.agent_bridge {
                    let trust = match next {
                        BuildMode::Manual => crate::tools::TrustLevel::Manual,
                        BuildMode::Balanced => crate::tools::TrustLevel::Balanced,
                        BuildMode::Careful => crate::tools::TrustLevel::Careful,
                    };
                    bridge.set_trust_level(trust);
                }

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Build mode: {}",
                        next.display_name()
                    )))
                    .ok();
            }
            Command::SetBuildMode(mode) => {
                self.state.set_build_mode(mode);

                // Map BuildMode to TrustLevel in AgentBridge
                if let Some(ref mut bridge) = self.agent_bridge {
                    let trust = match mode {
                        BuildMode::Manual => crate::tools::TrustLevel::Manual,
                        BuildMode::Balanced => crate::tools::TrustLevel::Balanced,
                        BuildMode::Careful => crate::tools::TrustLevel::Careful,
                    };
                    bridge.set_trust_level(trust);
                }

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Build mode: {}",
                        mode.display_name()
                    )))
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

                // Wire to agent's think level
                if let Some(ref mut bridge) = self.agent_bridge {
                    if enabled {
                        // Enable default thinking level
                        bridge.set_think_level_sync("normal".to_string());
                    } else {
                        // Disable thinking
                        bridge.set_think_level_sync("off".to_string());
                    }
                }

                self.event_tx
                    .send(AppEvent::StatusChanged(format!(
                        "Thinking blocks: {}",
                        if enabled { "enabled" } else { "disabled" }
                    )))
                    .ok();
            }

            // Input handling
            Command::InsertChar(c) => {
                let mut text = self.state.input_text();
                let cursor = self.state.input_cursor();
                text.insert(cursor, c);
                self.state.set_input_text(text);
                self.state.set_input_cursor(cursor + c.len_utf8());
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

                // Find next word boundary (skip current word, find start of next word)
                let mut new_cursor = cursor;
                let chars: Vec<char> = text.chars().collect();

                // Skip current word
                while new_cursor < chars.len() && !chars[new_cursor].is_whitespace() {
                    new_cursor += chars[new_cursor].len_utf8();
                }

                // Skip whitespace
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

                // Find previous word boundary
                let chars: Vec<char> = text.chars().collect();
                let mut byte_pos = 0;
                let mut positions: Vec<usize> = vec![0];

                for ch in &chars {
                    byte_pos += ch.len_utf8();
                    positions.push(byte_pos);
                }

                // Find char index from byte position
                let char_idx = positions
                    .iter()
                    .position(|&p| p >= cursor)
                    .unwrap_or(chars.len());
                let mut new_char_idx = char_idx.saturating_sub(1);

                // Skip whitespace backwards
                while new_char_idx > 0 && chars[new_char_idx].is_whitespace() {
                    new_char_idx -= 1;
                }

                // Skip word backwards
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

                // Send to LLM
                if let Some(ref mut bridge) = self.agent_bridge {
                    self.state.set_llm_processing(true);

                    // Get pending attachments (consumes them from manager)
                    let attachments = self.attachment_manager.prepare_for_send();
                    let has_attachments = !attachments.is_empty();

                    // Clear context files from state since attachments are being sent
                    if has_attachments {
                        self.state.clear_context_files();
                    }

                    // Create channel for AgentBridge events
                    let (bridge_tx, mut bridge_rx) = mpsc::unbounded_channel();
                    let app_event_tx = self.event_tx.clone();
                    let state = self.state.clone();

                    // Spawn task to bridge AgentEvents to AppEvents
                    tokio::spawn(async move {
                        let mut accumulated_text = String::new();
                        let mut accumulated_thinking = String::new();

                        while let Some(event) = bridge_rx.recv().await {
                            // Accumulate text for final message
                            if let BridgeEvent::TextChunk(ref chunk) = event {
                                accumulated_text.push_str(chunk);
                            }
                            if let BridgeEvent::ThinkingChunk(ref chunk) = event {
                                accumulated_thinking.push_str(chunk);
                            }

                            // Handle completion
                            if let BridgeEvent::Completed(_) = event {
                                // Create assistant message
                                let assistant_msg = Message {
                                    role: MessageRole::Assistant,
                                    content: accumulated_text.clone(),
                                    thinking: if accumulated_thinking.is_empty() {
                                        None
                                    } else {
                                        Some(accumulated_thinking.clone())
                                    },
                                    collapsed: false,
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                };
                                state.add_message(assistant_msg.clone());
                                let _ = app_event_tx.send(AppEvent::MessageAdded(assistant_msg));
                                state.set_llm_processing(false);
                            }

                            // Handle error
                            if let BridgeEvent::Error(ref err) = event {
                                // Show error as system message
                                let error_msg = Message {
                                    role: MessageRole::System,
                                    content: format!("Error: {}", err),
                                    thinking: None,
                                    collapsed: false,
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                };
                                state.add_message(error_msg.clone());
                                let _ = app_event_tx.send(AppEvent::MessageAdded(error_msg));
                                state.set_llm_processing(false);
                            }

                            // Handle interruption
                            if let BridgeEvent::Interrupted = event {
                                let interrupted_msg = Message {
                                    role: MessageRole::System,
                                    content: "Interrupted by user".to_string(),
                                    thinking: None,
                                    collapsed: false,
                                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                };
                                state.add_message(interrupted_msg.clone());
                                let _ = app_event_tx.send(AppEvent::MessageAdded(interrupted_msg));
                                state.set_llm_processing(false);
                            }

                            // Forward event to AppEvent
                            if let Some(app_event) = bridge_event_to_app_event(event) {
                                let _ = app_event_tx.send(app_event);
                            }
                        }
                    });

                    // Send message to AgentBridge with or without attachments
                    let result = if has_attachments {
                        let config = self.attachment_manager.config().clone();
                        bridge
                            .send_message_with_attachments(&text, attachments, bridge_tx, &config)
                            .await
                    } else {
                        bridge.send_message_streaming(&text, bridge_tx).await
                    };

                    if let Err(e) = result {
                        tracing::error!("Failed to send message to LLM: {}", e);
                        // Show error as system message
                        let error_msg = Message {
                            role: MessageRole::System,
                            content: format!("❌ Failed to send message: {}\n\nCheck your API key configuration and network connection.", e),
                            thinking: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        };
                        self.state.add_message(error_msg.clone());
                        self.event_tx.send(AppEvent::MessageAdded(error_msg)).ok();
                        self.event_tx.send(AppEvent::LlmError(e.to_string())).ok();
                        self.state.set_llm_processing(false);
                    }
                } else {
                    // No AgentBridge - show error
                    tracing::warn!("Attempted to send message but AgentBridge is not initialized");
                    let error_msg = Message {
                        role: MessageRole::System,
                        content: "⚠️  LLM not connected. Please configure your API key.\n\nRun 'tark auth <provider>' to set up authentication.".to_string(),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    self.state.add_message(error_msg.clone());
                    self.event_tx.send(AppEvent::MessageAdded(error_msg)).ok();
                    self.event_tx
                        .send(AppEvent::LlmError(
                            "LLM not connected. Please configure an API key.".to_string(),
                        ))
                        .ok();
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
            Command::AddContextFile(path) => {
                // Use AttachmentManager to properly load and process the file
                match self.add_attachment(&path) {
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
                }
            }
            Command::RemoveContextFile(path) => {
                self.remove_attachment(&path);
                self.state.remove_context_file(&path);
                self.event_tx.send(AppEvent::ContextFileRemoved(path)).ok();
            }

            // Interruption
            Command::Interrupt => {
                if let Some(ref bridge) = self.agent_bridge {
                    bridge.interrupt();
                }
                self.state.set_llm_processing(false);
                self.event_tx
                    .send(AppEvent::StatusChanged("Interrupted".to_string()))
                    .ok();
            }

            // History navigation
            Command::HistoryPrevious => {
                self.state.navigate_history_prev();
            }
            Command::HistoryNext => {
                self.state.navigate_history_next();
            }

            // Not yet implemented
            _ => {
                tracing::debug!("Command not yet implemented: {:?}", command);
            }
        }

        Ok(())
    }

    /// Get available LLM providers from models.dev
    ///
    /// Returns the list of providers filtered by config.enabled_providers.
    /// Loads provider data exclusively from models.dev.
    pub fn get_providers(&self) -> Vec<ProviderInfo> {
        // Load config to check enabled_providers filter
        let config = crate::config::Config::load().unwrap_or_default();
        let enabled_providers = if config.llm.enabled_providers.is_empty() {
            // If empty, show all providers
            None
        } else {
            Some(config.llm.enabled_providers.clone())
        };

        // Get providers from models.dev
        let models_db = crate::llm::models_db();

        // Use runtime to block on async in sync context (acceptable for UI operations)
        let all_provider_ids = tokio::runtime::Handle::try_current()
            .ok()
            .and_then(|_| {
                tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current()
                        .block_on(async { models_db.list_providers().await.ok() })
                })
            })
            .unwrap_or_else(|| {
                tracing::warn!("Failed to load providers from models.dev, using defaults");
                // Fallback to config-enabled providers only
                enabled_providers
                    .clone()
                    .unwrap_or_else(|| vec!["openai".to_string(), "google".to_string()])
            });

        // Filter by enabled_providers if configured
        let provider_ids: Vec<String> = if let Some(ref enabled) = enabled_providers {
            all_provider_ids
                .into_iter()
                .filter(|id| enabled.contains(id))
                .collect()
        } else {
            all_provider_ids
        };

        tracing::info!(
            "Loading {} providers: {:?}",
            provider_ids.len(),
            provider_ids
        );

        provider_ids
            .iter()
            .filter_map(|id| {
                // Get provider info from models.dev
                let provider_info = tokio::runtime::Handle::try_current().ok().and_then(|_| {
                    tokio::task::block_in_place(|| {
                        tokio::runtime::Handle::current()
                            .block_on(async { models_db.get_provider(id).await.ok().flatten() })
                    })
                });

                // Skip if no provider info from models.dev
                let info = provider_info?;

                // Determine if provider is configured by checking env vars
                let configured = if info.env.is_empty() {
                    // No env vars required (e.g., ollama) - always configured
                    true
                } else {
                    // Check if at least one required env var exists
                    info.env
                        .iter()
                        .any(|env_var| std::env::var(env_var).is_ok())
                };

                // Get icon
                let icon = Self::provider_icon(id);

                // Generate description from model count
                let description = if info.models.is_empty() {
                    "AI models".to_string()
                } else {
                    format!("{} models available", info.models.len())
                };

                Some(ProviderInfo {
                    id: id.clone(),
                    name: info.name.clone(),
                    description,
                    configured,
                    icon,
                })
            })
            .collect()
    }

    /// Check if a provider is configured (fallback method)
    fn is_provider_configured(provider_id: &str) -> bool {
        match provider_id {
            "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
            "anthropic" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
            "google" | "gemini" => {
                std::env::var("GEMINI_API_KEY").is_ok() || std::env::var("GOOGLE_API_KEY").is_ok()
            }
            "openrouter" => std::env::var("OPENROUTER_API_KEY").is_ok(),
            "ollama" => true, // Local, always available
            _ => false,
        }
    }

    /// Get display name for provider (fallback)
    fn provider_display_name(provider_id: &str) -> String {
        match provider_id {
            "openai" => "OpenAI",
            "anthropic" => "Anthropic",
            "google" | "gemini" => "Google Gemini",
            "openrouter" => "OpenRouter",
            "ollama" => "Ollama",
            _ => provider_id,
        }
        .to_string()
    }

    /// Get icon for provider
    fn provider_icon(provider_id: &str) -> String {
        match provider_id {
            "openai" => "🔑",
            "anthropic" => "🤖",
            "google" | "gemini" => "💎",
            "openrouter" => "🔀",
            "ollama" => "🦙",
            _ => "📦",
        }
        .to_string()
    }

    /// Get available models for a provider from models.dev
    pub async fn get_models(&self, provider: &str) -> Vec<ModelInfo> {
        // Try to get models from models.dev directly
        let models_db = crate::llm::models_db();

        match models_db.list_models(provider).await {
            Ok(models) if !models.is_empty() => models
                .into_iter()
                .map(|m| ModelInfo {
                    id: m.id.clone(),
                    name: m.name.clone(),
                    description: m.capability_summary(),
                    provider: provider.to_string(),
                    context_window: m.limit.context as usize,
                    max_tokens: m.limit.output as usize,
                })
                .collect(),
            _ => {
                // Fallback to AgentBridge if available
                if let Some(ref bridge) = self.agent_bridge {
                    let models = bridge.list_available_models().await;
                    models
                        .into_iter()
                        .map(|(id, name, description)| ModelInfo {
                            id: id.clone(),
                            name,
                            description,
                            provider: provider.to_string(),
                            context_window: 0,
                            max_tokens: 0,
                        })
                        .collect()
                } else {
                    vec![]
                }
            }
        }
    }

    /// Get current session information
    pub fn get_session_info(&self) -> Option<super::types::SessionInfo> {
        if let Some(ref bridge) = self.agent_bridge {
            let session = bridge.current_session();
            Some(super::types::SessionInfo {
                session_id: session.id.clone(),
                branch: "main".to_string(), // TODO: Get from git
                total_cost: bridge.total_cost(),
                model_count: 1, // TODO: Get actual model count
                created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            })
        } else {
            None
        }
    }

    /// Create a new session
    pub fn new_session(&mut self) -> Result<()> {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.new_session()?;
        }
        Ok(())
    }

    /// Switch to a different session
    pub fn switch_session(&mut self, session_id: &str) -> Result<()> {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.switch_session(session_id)?;
        }
        Ok(())
    }

    /// List all sessions
    pub fn list_sessions(&self) -> Result<Vec<crate::storage::SessionMeta>> {
        if let Some(ref bridge) = self.agent_bridge {
            bridge.list_sessions()
        } else {
            Ok(vec![])
        }
    }

    /// Save current session state
    pub fn save_session(&self) -> Result<()> {
        if let Some(ref _bridge) = self.agent_bridge {
            // Session is auto-saved by AgentBridge after each message
            tracing::debug!("Session auto-saved by AgentBridge");
        }
        Ok(())
    }

    /// Load a session by ID
    pub fn load_session(&mut self, session_id: &str) -> Result<()> {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.switch_session(session_id)?;

            // Update state with loaded session info
            let session = bridge.current_session();
            self.state.set_provider(Some(session.provider.clone()));
            self.state.set_model(Some(session.model.clone()));

            // Messages will be loaded from storage automatically
            tracing::info!("Switched to session: {}", session_id);
        }
        Ok(())
    }

    /// Export current session to file
    pub fn export_session(&self, path: &std::path::Path) -> Result<()> {
        if let Some(ref bridge) = self.agent_bridge {
            let session = bridge.current_session();
            let messages = self.state.messages();

            let export_data = serde_json::json!({
                "session_id": session.id,
                "provider": session.provider,
                "model": session.model,
                "messages": messages,
                "context_files": self.state.context_files(),
                "exported_at": chrono::Local::now().to_rfc3339(),
            });

            std::fs::write(path, serde_json::to_string_pretty(&export_data)?)?;
            tracing::info!("Session exported to {:?}", path);
        }
        Ok(())
    }

    /// Set the active provider
    pub async fn set_provider(&mut self, provider: &str) -> Result<()> {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.set_provider(provider)?;
        }
        self.state.set_provider(Some(provider.to_string()));
        self.event_tx
            .send(AppEvent::ProviderChanged(provider.to_string()))
            .ok();
        Ok(())
    }

    /// Set the active model
    pub async fn set_model(&mut self, model: &str) -> Result<()> {
        if let Some(ref mut bridge) = self.agent_bridge {
            bridge.set_model(model);
        }
        self.state.set_model(Some(model.to_string()));
        self.event_tx
            .send(AppEvent::ModelChanged(model.to_string()))
            .ok();
        Ok(())
    }

    // ========== Attachment Management APIs ==========

    /// Add a file attachment by path
    ///
    /// Reads the file, processes it appropriately based on type,
    /// and adds it to the pending attachments for the next message.
    pub fn add_attachment(&mut self, path_str: &str) -> Result<AttachmentInfo> {
        use crate::core::attachments::format_size;

        // Resolve the file path (handles relative paths, ~, etc.)
        let resolved_path = resolve_file_path(path_str)
            .map_err(|e| anyhow::anyhow!("Failed to resolve path: {}", e))?;

        // Attach file using the manager
        let attachment = self
            .attachment_manager
            .attach_file(&resolved_path)
            .map_err(|e| anyhow::anyhow!("Failed to attach file: {}", e))?;

        // Create info for UI
        let info = AttachmentInfo {
            filename: attachment.filename.clone(),
            path: resolved_path.display().to_string(),
            size_display: format_size(attachment.size),
            size_bytes: attachment.size,
            type_icon: attachment.file_type.icon().to_string(),
            mime_type: attachment.file_type.mime_type().to_string(),
            is_image: matches!(
                attachment.file_type,
                crate::tui::attachments::AttachmentType::Image { .. }
            ),
            added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
        };

        // Also add to state context files for UI display
        use super::types::ContextFile;
        let context_file = ContextFile {
            path: info.path.clone(),
            size: info.size_bytes as usize,
            added_at: info.added_at.clone(),
        };
        self.state.add_context_file(context_file);

        Ok(info)
    }

    /// Remove a file attachment by path
    pub fn remove_attachment(&mut self, path: &str) {
        // Find and remove from attachment manager by filename
        let pending = self.attachment_manager.pending();
        if let Some(pos) = pending.iter().position(|a| {
            // Match by filename or full path
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
                    crate::tui::attachments::AttachmentContent::Path(p) => p.display().to_string(),
                    _ => a.filename.clone(),
                },
                size_display: format_size(a.size),
                size_bytes: a.size,
                type_icon: a.file_type.icon().to_string(),
                mime_type: a.file_type.mime_type().to_string(),
                is_image: matches!(
                    a.file_type,
                    crate::tui::attachments::AttachmentType::Image { .. }
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

    /// Get attachment count
    pub fn attachment_count(&self) -> usize {
        self.attachment_manager.count()
    }

    /// Check if any pending attachments are images
    pub fn has_image_attachments(&self) -> bool {
        self.attachment_manager.has_images()
    }

    /// Try to add an image from clipboard
    ///
    /// Returns Ok(Some(info)) if an image was found and attached,
    /// Ok(None) if no image was in the clipboard.
    pub fn add_clipboard_image(&mut self) -> Result<Option<AttachmentInfo>> {
        use crate::core::attachments::format_size;

        match self.attachment_manager.attach_clipboard() {
            Ok(Some(attachment)) => {
                let info = AttachmentInfo {
                    filename: attachment.filename.clone(),
                    path: attachment.filename.clone(),
                    size_display: format_size(attachment.size),
                    size_bytes: attachment.size,
                    type_icon: attachment.file_type.icon().to_string(),
                    mime_type: attachment.file_type.mime_type().to_string(),
                    is_image: true,
                    added_at: chrono::Local::now().format("%H:%M:%S").to_string(),
                };

                // Add to state context files
                use super::types::ContextFile;
                let context_file = ContextFile {
                    path: info.filename.clone(),
                    size: info.size_bytes as usize,
                    added_at: info.added_at.clone(),
                };
                self.state.add_context_file(context_file);

                Ok(Some(info))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(anyhow::anyhow!("Clipboard error: {}", e)),
        }
    }

    /// Refresh all sidebar data (called on startup and after events)
    pub fn refresh_sidebar_data(&self) {
        // Update session info from AgentBridge
        if let Some(info) = self.get_session_info() {
            self.state.set_session(Some(info));
        }

        // Update git changes from working directory
        let git_changes = crate::tui_new::git_info::get_git_changes(&self.working_dir);
        self.state.set_git_changes(git_changes);

        // TODO: Update tasks from PlanService once integrated
    }

    /// Get git changes for the working directory
    pub fn get_git_changes(&self) -> Vec<GitChangeInfo> {
        crate::tui_new::git_info::get_git_changes(&self.working_dir)
    }
}
