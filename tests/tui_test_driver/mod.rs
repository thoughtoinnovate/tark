//! TUI Test Driver - Shared test utilities
//!
//! Provides utilities for testing the TUI at different levels:
//! - Key parsing: "Ctrl+B" → KeyEvent or terminal bytes
//! - Buffer reading: TestBackend buffer → String
//! - PTY wrapper: Spawn real process for E2E tests

pub mod keys;
pub mod buffer;
pub mod pty;

use anyhow::Result;
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use tark_cli::tui_new::TuiRenderer;
use tark_cli::ui_backend::SharedState;

/// In-memory test driver using TestBackend
/// Used for integration tests with Cucumber
pub struct TestDriver {
    renderer: TuiRenderer<TestBackend>,
    state: SharedState,
}

impl TestDriver {
    /// Create a new test driver with specified terminal size
    pub fn new(width: u16, height: u16) -> Result<Self> {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend)?;
        let renderer = TuiRenderer::new(terminal);
        let state = SharedState::new();
        
        Ok(Self { renderer, state })
    }

    /// Press a key (e.g., "Ctrl+B", "?", "Enter")
    pub fn press(&mut self, key: &str) -> Result<()> {
        let key_event = keys::parse(key)?;
        
        // Convert key to command
        if let Some(command) = self.renderer.poll_input(&self.state)? {
            // In a real scenario, we'd execute the command
            // For now, we'll need to integrate with TuiController
            // This is a simplified version
        }
        
        Ok(())
    }

    /// Type a string of text
    pub fn type_text(&mut self, text: &str) -> Result<()> {
        for c in text.chars() {
            self.press(&c.to_string())?;
        }
        Ok(())
    }

    /// Get the rendered buffer as a string
    pub fn buffer_text(&mut self) -> Result<String> {
        self.renderer.render(&self.state)?;
        let buf = self.renderer.terminal().backend().buffer();
        let (width, height) = self.renderer.get_size();
        Ok(buffer::to_string(buf, width, height))
    }

    /// Check if sidebar is visible
    pub fn is_sidebar_visible(&self) -> bool {
        self.state.sidebar_visible()
    }

    /// Get reference to state
    pub fn state(&self) -> &SharedState {
        &self.state
    }

    /// Get mutable reference to state
    pub fn state_mut(&mut self) -> &mut SharedState {
        &mut self.state
    }
}
