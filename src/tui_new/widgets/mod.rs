//! TUI Widgets - Reusable UI components
//!
//! Built with TDD approach following feature files in tests/visual/tui/features/

mod header;
mod input;
mod message_area;
mod modal;
mod status_bar;
mod terminal_frame;

pub use header::Header;
pub use input::InputWidget;
pub use message_area::{Message, MessageArea, MessageRole};
pub use modal::{FilePickerModal, HelpModal, ModelPickerModal, ProviderPickerModal, ThemePickerModal};
pub use status_bar::StatusBar;
pub use terminal_frame::TerminalFrame;
