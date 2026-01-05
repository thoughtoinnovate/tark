//! Terminal User Interface (TUI) for tark chat
//!
//! This module provides a rich terminal-based chat interface using ratatui/crossterm.
//! It can run standalone or integrate with Neovim via Unix socket RPC.

// Allow unused imports for public API re-exports that may not be used internally
#![allow(unused_imports)]

pub mod agent_bridge;
pub mod app;
pub mod attachments;
pub mod clipboard;
pub mod commands;
pub mod config;
pub mod editor_bridge;
mod events;
pub mod keybindings;
pub mod osc52;
pub mod plan_manager;
pub mod prompt_history;
pub mod usage_manager;
pub mod widgets;

pub use agent_bridge::{
    AgentBridge, AgentEvent, AgentMode, AgentResponseInfo, SessionMessageInfo, ToolCallLogInfo,
};
pub use app::{AppState, CompletionState, TuiApp};
pub use attachments::{
    base64_decode, base64_encode, detect_file_type, detect_language, format_size,
    parse_file_references, remove_file_references, resolve_file_path, search_workspace_files,
    Attachment, AttachmentConfig, AttachmentContent, AttachmentError, AttachmentManager,
    AttachmentType, DataFormat, DocumentFormat, ImageFormat, MessageAttachment,
};
pub use clipboard::{ClipboardContent, ClipboardHandler, ImageData};
pub use commands::{
    AgentModeChange, Command, CommandCategory, CommandHandler, CommandResult, PickerType,
    ToggleSetting,
};
pub use config::{AttachmentsConfig, KeybindingsConfig, ThemeColors, ThemeConfig, TuiConfig};
pub use editor_bridge::{
    BufferContentResponse, BufferInfo, BuffersResponse, ContextReceiver, CursorResponse,
    Diagnostic, DiagnosticSeverity, DiagnosticsResponse, EditorBridge, EditorBridgeConfig,
    EditorBridgeError, EditorBridgeResult, EditorEvent, EditorState, RpcMessage, SuccessResponse,
};
pub use events::{Event, EventHandler};
pub use keybindings::{Action, FocusedComponent, InputMode, KeybindingHandler};
pub use plan_manager::{
    NextTask, PanelTask, PlanManager, PlanStatusSummary, TaskInfo, TaskTransitionResult,
};
pub use prompt_history::PromptHistory;
pub use usage_manager::{UsageDisplayInfo, UsageManager};
pub use widgets::{
    AttachmentBar, AttachmentDropdownState, AttachmentPreview, BlockType, ChatMessage,
    CollapsibleBlock, CollapsibleBlockState, ContentSegment, ContextInfo, EnhancedPanelData,
    EnhancedPanelSection, EnhancedPanelWidget, FileItem, InputWidget, MessageList,
    NotificationLevel, PanelDataProvider, PanelSection, PanelSectionState, PanelWidget,
    ParsedMessageContent, Picker, PickerItem, PickerWidget, Role, SectionItem, SessionInfo,
    StatusBar, TaskItem, TaskStatus as PanelTaskStatus, ToolCallInfo,
};
