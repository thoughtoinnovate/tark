//! UI Renderer Trait
//!
//! Defines the interface that all frontends must implement.

use anyhow::Result;

use super::commands::Command;
use super::events::AppEvent;
use super::state::SharedState;

/// Trait that all UI renderers must implement
///
/// This abstracts the UI layer from business logic, allowing the same
/// backend to work with different frontends (TUI, Web, Desktop).
pub trait UiRenderer {
    /// Render the current application state
    ///
    /// This is called on every frame to display the UI.
    fn render(&mut self, state: &SharedState) -> Result<()>;

    /// Poll for user input and convert to commands
    ///
    /// Returns a Command if user input is available, None otherwise.
    /// This method should not block.
    fn poll_input(&mut self, state: &SharedState) -> Result<Option<Command>>;

    /// Handle an application event
    ///
    /// Events are async notifications from the backend (e.g., LLM streaming).
    fn handle_event(&mut self, event: &AppEvent, state: &SharedState) -> Result<()>;

    /// Get the current UI size
    ///
    /// Returns (width, height) in terminal columns/rows or pixels.
    fn get_size(&self) -> (u16, u16);

    /// Check if the UI should quit
    ///
    /// Returns true if the user requested to quit the application.
    fn should_quit(&self, state: &SharedState) -> bool;
}
