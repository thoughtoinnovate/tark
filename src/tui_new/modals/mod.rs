//! Modal handler modules
//!
//! This module provides state machine handlers for different modal types,
//! extracting the complex modal logic from the main controller.

pub mod approval_modal;
pub mod common;
pub mod device_flow_modal;
pub mod model_picker;
pub mod plugin_modal;
pub mod provider_picker;
pub mod session_picker;
pub mod theme_picker;
pub mod tools_modal;
pub mod trust_modal;

pub use approval_modal::ApprovalModal;
pub use common::{ModalHandler, ModalResult};
pub use device_flow_modal::DeviceFlowModal;
pub use model_picker::ModelPickerHandler;
pub use plugin_modal::PluginModal;
pub use provider_picker::ProviderPickerHandler;
pub use session_picker::SessionPickerHandler;
pub use theme_picker::ThemePickerHandler;
pub use tools_modal::ToolsModal;
pub use trust_modal::TrustModal;

use anyhow::Result;

use crate::ui_backend::{Command, ModalType, SharedState};

/// Modal manager that coordinates all modal handlers
pub struct ModalManager {
    theme_picker: ThemePickerHandler,
    provider_picker: ProviderPickerHandler,
    model_picker: ModelPickerHandler,
    session_picker: SessionPickerHandler,
}

impl ModalManager {
    pub fn new() -> Self {
        Self {
            theme_picker: ThemePickerHandler::new(),
            provider_picker: ProviderPickerHandler::new(),
            model_picker: ModelPickerHandler::new(),
            session_picker: SessionPickerHandler::new(),
        }
    }

    /// Route a command to the appropriate modal handler
    pub fn handle_command(&mut self, cmd: &Command, state: &SharedState) -> Result<ModalResult> {
        let active_modal = state.active_modal();

        match active_modal {
            Some(ModalType::ThemePicker) => self.theme_picker.handle_command(cmd, state),
            Some(ModalType::ProviderPicker) => self.provider_picker.handle_command(cmd, state),
            Some(ModalType::ModelPicker) => self.model_picker.handle_command(cmd, state),
            Some(ModalType::SessionPicker) => self.session_picker.handle_command(cmd, state),
            Some(ModalType::Help)
            | Some(ModalType::FilePicker)
            | Some(ModalType::Approval)
            | Some(ModalType::TrustLevel)
            | Some(ModalType::Tools)
            | Some(ModalType::Plugin)
            | Some(ModalType::DeviceFlow)
            | None => {
                // These modals are handled directly in controller/renderer
                Ok(ModalResult::NotHandled)
            }
        }
    }
}

impl Default for ModalManager {
    fn default() -> Self {
        Self::new()
    }
}
