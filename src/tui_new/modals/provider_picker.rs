//! Provider picker modal handler

use anyhow::Result;

use crate::ui_backend::{Command, ModalType, SharedState};

use super::common::{ModalHandler, ModalResult};

/// Provider picker modal handler
pub struct ProviderPickerHandler;

impl ProviderPickerHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ProviderPickerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalHandler for ProviderPickerHandler {
    fn handle_command(&mut self, cmd: &Command, state: &SharedState) -> Result<ModalResult> {
        match cmd {
            Command::ModalUp => {
                let selected = state.provider_picker_selected();
                if selected > 0 {
                    state.set_provider_picker_selected(selected - 1);
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalDown => {
                // Apply filter to get the actual list being displayed
                let all_providers = state.available_providers();
                let filter = state.provider_picker_filter();
                let filtered_count = if filter.is_empty() {
                    all_providers.len()
                } else {
                    let filter_lower = filter.to_lowercase();
                    all_providers
                        .iter()
                        .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                        .count()
                };

                let selected = state.provider_picker_selected();
                if selected + 1 < filtered_count {
                    state.set_provider_picker_selected(selected + 1);
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalFilter(text) => {
                if text.is_empty() {
                    // Backspace - remove last char
                    let current = state.provider_picker_filter();
                    if !current.is_empty() {
                        let new_filter = current[..current.len() - 1].to_string();
                        state.set_provider_picker_filter(new_filter);
                        state.set_provider_picker_selected(0);
                    }
                } else {
                    // Add to filter
                    let mut current = state.provider_picker_filter();
                    current.push_str(text);
                    state.set_provider_picker_filter(current);
                    state.set_provider_picker_selected(0);
                }
                Ok(ModalResult::Handled)
            }
            Command::ConfirmModal => {
                // Signal transition to model picker
                // The controller will handle selecting the provider and loading models
                Ok(ModalResult::Transition(ModalType::ModelPicker))
            }
            Command::CloseModal => Ok(ModalResult::Close),
            _ => Ok(ModalResult::NotHandled),
        }
    }

    fn is_active(&self, state: &SharedState) -> bool {
        state.active_modal() == Some(ModalType::ProviderPicker)
    }

    fn modal_type(&self) -> ModalType {
        ModalType::ProviderPicker
    }
}
