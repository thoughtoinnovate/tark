//! TUI Widgets - Reusable UI components
//!
//! Built with TDD approach following feature files in tests/visual/tui/features/

pub mod command_autocomplete;
mod flash_bar;
pub mod header;
mod input;
pub mod markdown;
mod message_area;
mod modal;
pub mod question;
mod sidebar;
mod status_bar;
mod terminal_frame;
pub mod thinking_block;
pub mod todo;

pub use command_autocomplete::{AutocompleteState, CommandAutocomplete, SlashCommand};
pub use flash_bar::{FlashBar, FlashBarState};
pub use header::Header;
#[allow(unused_imports)]
pub use input::AttachmentBadge;
pub use input::InputWidget;
pub use message_area::{
    parse_tool_risk_group, Message, MessageArea, MessageLineTarget, MessageRole,
};
pub use modal::{
    FilePickerModal, HelpModal, ModelPickerModal, ProviderPickerModal, SessionPickerModal,
    ThemePickerModal,
};
#[allow(unused_imports)]
pub use question::{QuestionOption, QuestionType, QuestionWidget, ThemedQuestion};
#[allow(unused_imports)]
pub use sidebar::{GitChange, GitStatus, SessionInfo, Sidebar, SidebarPanel, Task, TaskStatus};
pub use status_bar::StatusBar;
pub use terminal_frame::TerminalFrame;
#[allow(unused_imports)]
pub use thinking_block::ThinkingBlockWidget;
#[allow(unused_imports)]
pub use todo::TodoWidget;
