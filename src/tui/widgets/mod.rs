//! UI Widgets for the TUI
//!
//! This module contains reusable UI components for the terminal chat interface.

mod attachment_bar;
mod input;
mod message_list;
pub mod panel;
mod picker;
mod status_bar;

pub use attachment_bar::{AttachmentBar, AttachmentPreview};
pub use input::{InputWidget, InputWidgetRenderer};
pub use message_list::{ChatMessage, MessageList, Role};
pub use panel::{NotificationLevel, PanelSection, PanelWidget, SectionItem, TaskStatus};
pub use picker::{Picker, PickerItem, PickerWidget};
pub use status_bar::StatusBar;
