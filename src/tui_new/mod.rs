//! New TUI Module - Built with TDD
//!
//! This module implements the terminal user interface for `tark tui` command.
//! Built from scratch following Test-Driven Development with BDD feature files.
//!
//! ## TDD Approach
//! - Feature files: tests/visual/tui/features/*.feature
//! - Screenshots: web/ui/mocks/screenshots/*.png
//! - React reference: web/ui/mocks/src/app/components/*.tsx

// Allow dead code for intentionally unused API methods that are part of the public interface
// These will be used as we implement more features
#![allow(dead_code)]

pub mod app;
pub mod config;
mod controller;
mod events;
pub mod git_info;
pub mod modals;
mod renderer;
pub mod session_prefs;
mod theme;
mod utils;
pub mod widgets;

// Re-export main types
#[allow(unused_imports)]
pub use app::{AgentMode, AppState, BuildMode, FocusedComponent, InputMode, ModalType, TuiApp};
#[allow(unused_imports)]
pub use config::AppConfig;
#[allow(unused_imports)]
pub use controller::TuiController;
#[allow(unused_imports)]
pub use events::{Event, EventHandler};
#[allow(unused_imports)]
pub use renderer::TuiRenderer;
#[allow(unused_imports)]
pub use theme::{Theme, ThemePreset};

// Re-export widgets
#[allow(unused_imports)]
pub use widgets::{
    Header, InputWidget, Message, MessageArea, MessageRole, StatusBar, TerminalFrame,
};

// Re-export session preferences
#[allow(unused_imports)]
pub use session_prefs::{PreferencesManager, TuiPreferences};
