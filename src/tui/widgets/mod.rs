//! UI Widgets for the TUI
//!
//! This module contains reusable UI components for the terminal chat interface.

mod attachment_bar;
pub mod collapsible;
mod command_dropdown;
mod file_dropdown;
mod input;
mod message_list;
pub mod panel;
mod picker;
mod status_bar;
pub mod thinking_block;
pub mod tool_block;

pub use attachment_bar::{AttachmentBar, AttachmentDropdownState, AttachmentPreview};
pub use collapsible::{
    BlockType, CollapsibleBlock, CollapsibleBlockState, ContentSegment, ParsedMessageContent,
    ToolCallInfo,
};
pub use command_dropdown::{CommandDropdown, CommandDropdownItem, CommandDropdownWidget};
pub use input::{InputWidget, InputWidgetRenderer};
pub use message_list::{ChatMessage, MessageList, MessageListWidget, Role};
pub use panel::{
    ContextInfo, CostBreakdownEntry, EnhancedPanelData, EnhancedPanelSection, EnhancedPanelWidget,
    FileItem, NotificationLevel, PanelDataProvider, PanelNavMode, PanelSection, PanelSectionState,
    PanelWidget, SectionItem, SessionInfo, TaskItem, TaskStatus,
};
pub use picker::{Picker, PickerItem, PickerWidget};
pub use status_bar::StatusBar;
pub use thinking_block::{ThinkingBlock, ThinkingBlockManager, ThinkingBlockWidget};
pub use tool_block::{ToolBlock, ToolBlockManager, ToolBlockWidget, ToolStatus};
mod auth_dialog;
mod questionnaire;
pub use auth_dialog::{AuthDialog, AuthDialogWidget, AuthStatus};
pub use file_dropdown::{FileDropdown, FileDropdownItem, FileDropdownWidget};
pub use questionnaire::{QuestionnaireState, QuestionnaireWidget};
