//! UI Renderer Trait
//!
//! Defines the interface that all frontends must implement.

use anyhow::Result;

use super::state::SharedState;
use super::types::{ModalContent, StatusInfo};

/// Trait that all UI renderers must implement
///
/// This abstracts the UI layer from business logic, allowing the same
/// backend to work with different frontends (TUI, Web, Desktop).
pub trait UiRenderer {
    /// Render the current application state
    ///
    /// This is called on every frame to display the UI.
    fn render(&mut self, state: &SharedState) -> Result<()>;

    /// Show a modal/dialog
    ///
    /// Displays a modal overlay (e.g., provider picker, model picker, help).
    fn show_modal(&mut self, modal: ModalContent) -> Result<()>;

    /// Update the status bar
    ///
    /// Sets the status message and indicators.
    fn set_status(&mut self, status: StatusInfo) -> Result<()>;

    /// Get the current UI size
    ///
    /// Returns (width, height) in terminal columns/rows or pixels.
    fn get_size(&self) -> (u16, u16);

    /// Check if the UI should quit
    ///
    /// Returns true if the user requested to quit the application.
    fn should_quit(&self) -> bool;
}
