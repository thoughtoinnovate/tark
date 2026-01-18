//! TUI Controller - Orchestrates AppService and TuiRenderer
//!
//! The controller owns both the business logic (AppService) and the UI (TuiRenderer),
//! coordinating between them via Commands and AppEvents.

use anyhow::Result;
use ratatui::backend::Backend;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::ui_backend::UiRenderer;
use crate::ui_backend::{AppEvent, AppService, Command, SharedState};

use super::modals::{ModalManager, ModalResult};
use super::renderer::TuiRenderer;

/// TUI Controller
///
/// Main event loop coordinator that:
/// 1. Polls input from renderer
/// 2. Processes commands through AppService
/// 3. Handles async events from AppService
/// 4. Renders UI with current state
pub struct TuiController<B: Backend> {
    /// Application service (business logic)
    service: AppService,
    /// UI renderer
    renderer: TuiRenderer<B>,
    /// Event receiver for async AppEvents
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    /// Modal manager for delegating modal-specific commands
    modal_manager: ModalManager,
}

impl<B: Backend> TuiController<B> {
    /// Create a new TUI controller
    pub fn new(
        service: AppService,
        renderer: TuiRenderer<B>,
        event_rx: mpsc::UnboundedReceiver<AppEvent>,
    ) -> Self {
        Self {
            service,
            renderer,
            event_rx,
            modal_manager: ModalManager::new(),
        }
    }

    /// Run the main event loop
    pub async fn run(&mut self) -> Result<()> {
        // Initialize sidebar data on startup
        self.service.refresh_sidebar_data();

        let state = self.service.state().clone();

        loop {
            // 1. Render current state
            self.renderer.render(&state)?;

            // 2. Poll for user input (non-blocking)
            if let Some(command) = self.renderer.poll_input(&state)? {
                self.handle_command(command).await?;
            }

            // 3. Process async events (non-blocking)
            self.poll_events(&state)?;

            // 4. Check quit condition
            if self.renderer.should_quit(&state) {
                break;
            }

            // Small sleep to avoid busy-waiting
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        Ok(())
    }

    /// Handle a user command
    async fn handle_command(&mut self, command: Command) -> Result<()> {
        // Handle Quit with interruption if processing
        if let Command::Quit = command {
            // If LLM is processing, interrupt first
            let is_processing = self.service.state().llm_processing();
            if is_processing {
                self.service.handle_command(Command::Interrupt).await?;
            }
        }

        // Intercept SendMessage to check for slash commands
        if let Command::SendMessage(ref text) = command {
            if text.starts_with('/') {
                return self.handle_slash_command(text);
            }
        }

        // Delegate to modal manager if a modal is active
        if self.service.state().active_modal().is_some() {
            // Clone state to avoid borrow issues
            let state = self.service.state().clone();
            match self.modal_manager.handle_command(&command, &state)? {
                ModalResult::Handled => return Ok(()),
                ModalResult::Close => {
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                    return Ok(());
                }
                ModalResult::Transition(next_modal) => {
                    // Handle modal transitions (e.g., ProviderPicker -> ModelPicker)
                    if let crate::ui_backend::ModalType::ModelPicker = next_modal {
                        // Select the current provider before transitioning
                        let all_providers = self.service.get_providers();
                        let filter = state.provider_picker_filter();
                        let filtered_providers: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if let Some(provider_info) = filtered_providers.get(selected) {
                            let provider_id = provider_info.id.clone();
                            self.service
                                .handle_command(Command::SelectProvider(provider_id.clone()))
                                .await?;

                            // Load models for this provider and open model picker
                            let models = self.service.get_models(&provider_id).await;
                            state.set_available_models(models);
                            state.set_model_picker_selected(0);
                            state.set_model_picker_filter(String::new());
                        }
                    }
                    state.set_active_modal(Some(next_modal));
                    return Ok(());
                }
                ModalResult::NotHandled => {
                    // Fall through to general command handling
                }
            }
        }

        let state = self.service.state();

        // Special handling for modal toggle commands
        match &command {
            Command::ToggleHelp => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::Help) {
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                } else {
                    state.set_active_modal(Some(crate::ui_backend::ModalType::Help));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
                return Ok(());
            }
            Command::CloseModal => {
                // Restore original theme if canceling theme picker
                if state.active_modal() == Some(crate::ui_backend::ModalType::ThemePicker) {
                    if let Some(original_theme) = state.theme_before_preview() {
                        state.set_theme(original_theme);
                    }
                    state.set_theme_before_preview(None);
                }
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                return Ok(());
            }
            Command::ConfirmModal => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        // Theme already applied during preview, just clear preview state
                        state.set_theme_before_preview(None);
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Select provider and open model picker
                        // Apply filter to get the actual displayed list
                        let all_providers = self.service.get_providers();
                        let filter = state.provider_picker_filter();
                        let filtered_providers: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if let Some(provider_info) = filtered_providers.get(selected) {
                            let provider_id = provider_info.id.clone();

                            // Set the provider
                            self.service
                                .handle_command(Command::SelectProvider(provider_id.clone()))
                                .await?;

                            // Load models for this provider and open model picker
                            let models = self.service.get_models(&provider_id).await;
                            let state = self.service.state();
                            state.set_available_models(models);
                            state.set_model_picker_selected(0);
                            state.set_model_picker_filter(String::new());
                            state.set_active_modal(Some(crate::ui_backend::ModalType::ModelPicker));
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Select model and close
                        // Apply filter to get the actual displayed list
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered_models: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if let Some(model_info) = filtered_models.get(selected) {
                            let model_id = model_info.id.clone();
                            self.service
                                .handle_command(Command::SelectModel(model_id))
                                .await?;
                        }
                        let state = self.service.state();
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    _ => {
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                }
            }
            Command::ModalUp => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        let selected = state.theme_picker_selected();
                        if selected > 0 {
                            state.set_theme_picker_selected(selected - 1);
                            // Live preview the theme
                            let all_themes = crate::ui_backend::ThemePreset::all();
                            let filter = state.theme_picker_filter();
                            let filtered: Vec<_> = if filter.is_empty() {
                                all_themes
                            } else {
                                let filter_lower = filter.to_lowercase();
                                all_themes
                                    .into_iter()
                                    .filter(|t| {
                                        t.display_name().to_lowercase().contains(&filter_lower)
                                    })
                                    .collect()
                            };
                            if let Some(&theme) = filtered.get(selected - 1) {
                                state.set_theme(theme);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_providers = state.available_providers();
                        let filter = state.provider_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if selected > 0 && selected <= filtered.len() {
                            state.set_provider_picker_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if selected > 0 && selected <= filtered.len() {
                            state.set_model_picker_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::ModalDown => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        let all_themes = crate::ui_backend::ThemePreset::all();
                        let filter = state.theme_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_themes
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_themes
                                .into_iter()
                                .filter(|t| t.display_name().to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.theme_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_theme_picker_selected(selected + 1);
                            // Live preview the theme
                            if let Some(&theme) = filtered.get(selected + 1) {
                                state.set_theme(theme);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_providers = state.available_providers();
                        let filter = state.provider_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_provider_picker_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_model_picker_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::ModalFilter(filter) => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        if filter.is_empty() {
                            // Backspace - pop last character
                            let mut current = state.theme_picker_filter();
                            current.pop();
                            state.set_theme_picker_filter(current);
                        } else {
                            // Add character
                            let mut current = state.theme_picker_filter();
                            current.push_str(filter);
                            state.set_theme_picker_filter(current);
                        }
                        state.set_theme_picker_selected(0);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        if filter.is_empty() {
                            let mut current = state.provider_picker_filter();
                            current.pop();
                            state.set_provider_picker_filter(current);
                        } else {
                            let mut current = state.provider_picker_filter();
                            current.push_str(filter);
                            state.set_provider_picker_filter(current);
                        }
                        state.set_provider_picker_selected(0);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        if filter.is_empty() {
                            let mut current = state.model_picker_filter();
                            current.pop();
                            state.set_model_picker_filter(current);
                        } else {
                            let mut current = state.model_picker_filter();
                            current.push_str(filter);
                            state.set_model_picker_filter(current);
                        }
                        state.set_model_picker_selected(0);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::FocusNext => {
                let current = state.focused_component();
                let next = match current {
                    crate::ui_backend::FocusedComponent::Input => {
                        crate::ui_backend::FocusedComponent::Messages
                    }
                    crate::ui_backend::FocusedComponent::Messages => {
                        crate::ui_backend::FocusedComponent::Panel
                    }
                    crate::ui_backend::FocusedComponent::Panel => {
                        crate::ui_backend::FocusedComponent::Input
                    }
                    crate::ui_backend::FocusedComponent::Modal => current,
                };
                state.set_focused_component(next);
                return Ok(());
            }
            Command::OpenProviderPicker => {
                // Load providers and initialize picker state
                let providers = self.service.get_providers();
                state.set_available_providers(providers);
                state.set_provider_picker_selected(0);
                state.set_provider_picker_filter(String::new());
                state.set_active_modal(Some(crate::ui_backend::ModalType::ProviderPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                return Ok(());
            }
            Command::OpenModelPicker => {
                state.set_active_modal(Some(crate::ui_backend::ModalType::ModelPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                return Ok(());
            }
            Command::ToggleThemePicker => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::ThemePicker) {
                    // Close and restore original theme
                    if let Some(original_theme) = state.theme_before_preview() {
                        state.set_theme(original_theme);
                    }
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                } else {
                    // Open theme picker
                    state.set_theme_before_preview(Some(state.theme()));
                    state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                    // Initialize selection to current theme
                    let all_themes = crate::ui_backend::ThemePreset::all();
                    let current_theme = state.theme();
                    if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                        state.set_theme_picker_selected(idx);
                    }
                    state.set_theme_picker_filter(String::new());
                }
                return Ok(());
            }
            Command::ScrollUp => {
                // Scroll up in messages area
                let current_offset = state.messages_scroll_offset();
                if current_offset > 0 {
                    state.set_messages_scroll_offset(current_offset - 1);
                }
                return Ok(());
            }
            Command::ScrollDown => {
                // Scroll down in messages area
                let current_offset = state.messages_scroll_offset();
                let message_count = state.messages().len();
                // Allow scrolling but limit to reasonable bounds
                if current_offset < message_count.saturating_sub(1) {
                    state.set_messages_scroll_offset(current_offset + 1);
                }
                return Ok(());
            }
            Command::ToggleSidebarPanel(panel_idx) => {
                // Toggle the expansion state of a sidebar panel
                if *panel_idx < 4 {
                    let mut panels = state.sidebar_expanded_panels();
                    panels[*panel_idx] = !panels[*panel_idx];
                    state.set_sidebar_expanded_panels(panels);
                }
                return Ok(());
            }
            _ => {}
        }

        // Pass command to AppService for processing
        self.service.handle_command(command).await?;

        Ok(())
    }

    /// Poll for async events (non-blocking)
    fn poll_events(&mut self, state: &SharedState) -> Result<()> {
        while let Ok(event) = self.event_rx.try_recv() {
            // Let renderer handle the event
            self.renderer.handle_event(&event, state)?;

            // Refresh sidebar data on LLM completion (updates costs and tokens)
            if let AppEvent::LlmCompleted { .. } = event {
                self.service.refresh_sidebar_data();
            }
        }
        Ok(())
    }

    /// Get a reference to the renderer (for testing)
    #[allow(dead_code)]
    pub fn renderer(&self) -> &TuiRenderer<B> {
        &self.renderer
    }

    /// Get a mutable reference to the renderer (for testing)
    #[allow(dead_code)]
    pub fn renderer_mut(&mut self) -> &mut TuiRenderer<B> {
        &mut self.renderer
    }

    /// Get a reference to the service (for testing)
    #[allow(dead_code)]
    pub fn service(&self) -> &AppService {
        &self.service
    }

    /// Handle slash commands (e.g., /theme, /help, /model)
    fn handle_slash_command(&mut self, text: &str) -> Result<()> {
        let state = self.service.state();

        match text.trim() {
            "/help" | "/?" => {
                state.set_active_modal(Some(crate::ui_backend::ModalType::Help));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            "/model" | "/provider" => {
                // Load providers and initialize picker state
                let providers = self.service.get_providers();
                state.set_available_providers(providers);
                state.set_provider_picker_selected(0);
                state.set_provider_picker_filter(String::new());
                state.set_active_modal(Some(crate::ui_backend::ModalType::ProviderPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            "/theme" => {
                // Open theme picker with preview
                state.set_theme_before_preview(Some(state.theme()));
                state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                // Initialize selection to current theme
                let all_themes = crate::ui_backend::ThemePreset::all();
                let current_theme = state.theme();
                if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                    state.set_theme_picker_selected(idx);
                }
                state.set_theme_picker_filter(String::new());
            }
            "/file" | "/files" => {
                state.set_active_modal(Some(crate::ui_backend::ModalType::FilePicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            cmd if cmd.starts_with("/attach ") => {
                // Attach file directly by path: /attach path/to/file
                let path = cmd.trim_start_matches("/attach ").trim().to_string();
                if !path.is_empty() {
                    // Use BFF API to add attachment - drop state borrow first
                    let result = self.service.add_attachment(&path);
                    let state = self.service.state();
                    match result {
                        Ok(info) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let success_msg = Message {
                                role: MessageRole::System,
                                content: format!(
                                    "Attached: {} {} ({})",
                                    info.type_icon, info.filename, info.size_display
                                ),
                                thinking: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(success_msg);
                        }
                        Err(e) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let error_msg = Message {
                                role: MessageRole::System,
                                content: format!("Failed to attach file: {}", e),
                                thinking: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(error_msg);
                        }
                    }
                    // Re-clear input after adding state message
                    state.clear_input();
                    return Ok(());
                } else {
                    // No path provided, open file picker
                    state.set_active_modal(Some(crate::ui_backend::ModalType::FilePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
            }
            "/attachments" | "/files-clear" => {
                // Clear all attachments - drop state borrow first
                self.service.clear_attachments();
                let state = self.service.state();
                use crate::ui_backend::{Message, MessageRole};
                let clear_msg = Message {
                    role: MessageRole::System,
                    content: "Cleared all attachments.".to_string(),
                    thinking: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(clear_msg);
                state.clear_input();
                return Ok(());
            }
            "/clear" => {
                // Clear messages (keep only first 2 system messages)
                // This would need to be implemented in SharedState
                // For now, just clear input
                state.clear_input();
            }
            "/quit" | "/exit" | "/q" => {
                state.set_should_quit(true);
            }
            _ => {
                // Unknown command - add system message
                use crate::ui_backend::{Message, MessageRole};
                let system_msg = Message {
                    role: MessageRole::System,
                    content: format!(
                        "Unknown command: {}. Type /help for available commands.",
                        text
                    ),
                    thinking: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(system_msg);
            }
        }

        // Clear input after processing slash command
        state.clear_input();

        Ok(())
    }
}
