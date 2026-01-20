//! Theme picker modal handler

use anyhow::Result;

use crate::ui_backend::{Command, ModalType, SharedState, ThemePreset};

use super::common::{ModalHandler, ModalResult};

/// Theme picker modal handler
pub struct ThemePickerHandler;

impl ThemePickerHandler {
    pub fn new() -> Self {
        Self
    }

    /// Apply filter and get filtered themes
    fn get_filtered_themes(&self, state: &SharedState) -> Vec<ThemePreset> {
        let all_themes = ThemePreset::all();
        let filter = state.theme_picker_filter();

        if filter.is_empty() {
            all_themes
        } else {
            let filter_lower = filter.to_lowercase();
            all_themes
                .into_iter()
                .filter(|t| t.display_name().to_lowercase().contains(&filter_lower))
                .collect()
        }
    }
}

impl Default for ThemePickerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalHandler for ThemePickerHandler {
    fn handle_command(&mut self, cmd: &Command, state: &SharedState) -> Result<ModalResult> {
        match cmd {
            Command::ModalUp => {
                let selected = state.theme_picker_selected();
                if selected > 0 {
                    state.set_theme_picker_selected(selected - 1);

                    // Live preview the theme
                    let filtered = self.get_filtered_themes(state);
                    if let Some(&theme) = filtered.get(selected - 1) {
                        state.set_theme(theme);
                    }
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalDown => {
                let filtered = self.get_filtered_themes(state);
                let selected = state.theme_picker_selected();
                if selected + 1 < filtered.len() {
                    state.set_theme_picker_selected(selected + 1);

                    // Live preview the theme
                    if let Some(&theme) = filtered.get(selected + 1) {
                        state.set_theme(theme);
                    }
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalFilter(text) => {
                if text.is_empty() {
                    // Backspace - remove last char
                    let current = state.theme_picker_filter();
                    if !current.is_empty() {
                        let new_filter = current[..current.len() - 1].to_string();
                        state.set_theme_picker_filter(new_filter);
                        state.set_theme_picker_selected(0);
                    }
                } else {
                    // Add to filter
                    let mut current = state.theme_picker_filter();
                    current.push_str(text);
                    state.set_theme_picker_filter(current);
                    state.set_theme_picker_selected(0);
                }
                Ok(ModalResult::Handled)
            }
            Command::ConfirmModal => {
                // Theme already applied during preview, just clear preview state
                state.set_theme_before_preview(None);
                Ok(ModalResult::Close)
            }
            Command::CloseModal => {
                // Restore original theme if canceling
                if let Some(original_theme) = state.theme_before_preview() {
                    state.set_theme(original_theme);
                }
                state.set_theme_before_preview(None);
                Ok(ModalResult::Close)
            }
            _ => Ok(ModalResult::NotHandled),
        }
    }

    fn is_active(&self, state: &SharedState) -> bool {
        state.active_modal() == Some(ModalType::ThemePicker)
    }

    fn modal_type(&self) -> ModalType {
        ModalType::ThemePicker
    }
}
