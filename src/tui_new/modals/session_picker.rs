//! Session picker modal handler

use anyhow::Result;

use crate::ui_backend::{Command, ModalType, SharedState};

use super::common::{ModalHandler, ModalResult};

/// Session picker modal handler
pub struct SessionPickerHandler;

impl SessionPickerHandler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SessionPickerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ModalHandler for SessionPickerHandler {
    fn handle_command(&mut self, cmd: &Command, state: &SharedState) -> Result<ModalResult> {
        match cmd {
            Command::ModalUp => {
                let selected = state.session_picker_selected();
                if selected > 0 {
                    state.set_session_picker_selected(selected - 1);
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalDown => {
                let all_sessions = state.available_sessions();
                let filter = state.session_picker_filter();
                let filtered_count = if filter.is_empty() {
                    all_sessions.len()
                } else {
                    let filter_lower = filter.to_lowercase();
                    all_sessions
                        .iter()
                        .filter(|s| {
                            s.name.to_lowercase().contains(&filter_lower)
                                || s.id.to_lowercase().contains(&filter_lower)
                        })
                        .count()
                };

                let selected = state.session_picker_selected();
                if selected + 1 < filtered_count {
                    state.set_session_picker_selected(selected + 1);
                }
                Ok(ModalResult::Handled)
            }
            Command::ModalFilter(text) => {
                if text.is_empty() {
                    let current = state.session_picker_filter();
                    if !current.is_empty() {
                        let new_filter = current[..current.len() - 1].to_string();
                        state.set_session_picker_filter(new_filter);
                        state.set_session_picker_selected(0);
                    }
                } else {
                    let mut current = state.session_picker_filter();
                    current.push_str(text);
                    state.set_session_picker_filter(current);
                    state.set_session_picker_selected(0);
                }
                Ok(ModalResult::Handled)
            }
            Command::ConfirmModal => Ok(ModalResult::NotHandled),
            Command::CloseModal => Ok(ModalResult::Close),
            _ => Ok(ModalResult::NotHandled),
        }
    }

    fn is_active(&self, state: &SharedState) -> bool {
        state.active_modal() == Some(ModalType::SessionPicker)
    }

    fn modal_type(&self) -> ModalType {
        ModalType::SessionPicker
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::SessionMeta;
    use crate::ui_backend::SharedState;
    use chrono::Utc;

    fn sample_session(id: &str, name: &str) -> SessionMeta {
        SessionMeta {
            id: id.to_string(),
            name: name.to_string(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            provider: "tark_sim".to_string(),
            model: "tark_llm".to_string(),
            mode: "build".to_string(),
            message_count: 0,
            is_current: false,
            agent_running: false,
        }
    }

    #[test]
    fn session_picker_filters_by_name_or_id() {
        let state = SharedState::default();
        state.set_available_sessions(vec![
            sample_session("session_alpha", "Alpha"),
            sample_session("session_beta", "Beta"),
        ]);

        let mut handler = SessionPickerHandler::new();
        handler
            .handle_command(&Command::ModalFilter("beta".to_string()), &state)
            .unwrap();

        assert_eq!(state.session_picker_filter(), "beta");
        handler.handle_command(&Command::ModalDown, &state).unwrap();
        // Only one match, so selection stays at 0
        assert_eq!(state.session_picker_selected(), 0);
    }

    #[test]
    fn session_picker_moves_selection_with_filter() {
        let state = SharedState::default();
        state.set_available_sessions(vec![
            sample_session("session_a", "Session A"),
            sample_session("session_b", "Session B"),
            sample_session("session_c", "Other"),
        ]);

        let mut handler = SessionPickerHandler::new();
        handler
            .handle_command(&Command::ModalFilter("session".to_string()), &state)
            .unwrap();
        handler.handle_command(&Command::ModalDown, &state).unwrap();
        assert_eq!(state.session_picker_selected(), 1);
    }
}
