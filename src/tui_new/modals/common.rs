//! Common modal handler traits and types

use anyhow::Result;

use crate::ui_backend::{Command, ModalType, SharedState};

/// Result of modal command handling
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModalResult {
    /// Command was handled by the modal
    Handled,
    /// Command was not handled (let controller handle it)
    NotHandled,
    /// Close this modal
    Close,
    /// Transition to another modal
    Transition(ModalType),
}

/// Trait for modal handlers
pub trait ModalHandler {
    /// Handle a command within the modal context
    fn handle_command(&mut self, cmd: &Command, state: &SharedState) -> Result<ModalResult>;

    /// Check if this modal is currently active
    fn is_active(&self, state: &SharedState) -> bool;

    /// Get the modal type this handler manages
    fn modal_type(&self) -> ModalType;
}
