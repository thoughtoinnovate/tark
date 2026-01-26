//! Shared Application State
//!
//! Thread-safe state that can be safely shared between the backend and frontend.

use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};

use serde_json::json;

use super::approval::ApprovalCardState;
use super::commands::{AgentMode, BuildMode};
use super::questionnaire::{QuestionType, QuestionnaireState};
use super::types::{
    ArchiveChunkInfo, AttachmentInfo, AttachmentToken, ContextFile, DiffViewMode, GitChangeInfo,
    Message, MessageRole, ModelInfo, ProviderInfo, SessionInfo, TaskInfo, ThemePreset,
};
use crate::core::context_tracker::ContextBreakdown;
use crate::tools::TrustLevel;
use crate::tui_new::widgets::FlashBarState;

const MAX_FLASH_BAR_FRAME: u8 = 20;

/// Error notification level
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    Info,
    Warning,
    Error,
}

/// Error notification displayed to the user
#[derive(Debug, Clone)]
pub struct ErrorNotification {
    pub message: String,
    pub level: ErrorLevel,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

#[derive(Debug, Clone)]
pub struct PasteBlock {
    pub placeholder: String,
    pub content: String,
}

/// Currently focused UI component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FocusedComponent {
    #[default]
    Input,
    Messages,
    Panel,
    Modal,
}

impl FocusedComponent {
    /// Cycle to next component
    pub fn next(self) -> Self {
        match self {
            Self::Input => Self::Messages,
            Self::Messages => Self::Panel,
            Self::Panel => Self::Input,
            Self::Modal => Self::Modal, // Modal keeps focus
        }
    }

    /// Cycle to previous component
    pub fn previous(self) -> Self {
        match self {
            Self::Input => Self::Panel,
            Self::Messages => Self::Input,
            Self::Panel => Self::Messages,
            Self::Modal => Self::Modal,
        }
    }
}

/// Vim editing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VimMode {
    /// Insert mode - typing
    #[default]
    Insert,
    /// Normal mode - navigation
    Normal,
    /// Visual mode - selection
    Visual,
    /// Command mode - slash commands
    Command,
}

/// Types of modals that can be displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalType {
    ProviderPicker,
    ModelPicker,
    SessionPicker,
    FilePicker,
    ThemePicker,
    Help,
    Approval,
    TrustLevel,
    Tools,
    Plugin,
    DeviceFlow,
    /// Confirmation dialog when switching sessions while agent is processing
    SessionSwitchConfirm,
    /// Modal for editing a queued task
    TaskEdit,
    /// Confirmation dialog for deleting a queued task
    TaskDeleteConfirm,
    /// Policy manager modal for viewing approval/denial patterns
    Policy,
}

/// Active OAuth device flow session
#[derive(Debug, Clone)]
pub struct DeviceFlowSession {
    pub provider: String,
    pub device_code: String,
    pub user_code: String,
    pub verification_url: String,
    pub expires_at: std::time::Instant,
    pub interval: std::time::Duration,
}

/// Conversation-related state (grouped)
#[derive(Debug, Clone, Default)]
pub struct ConversationState {
    pub messages: Vec<Message>,
    pub archive_chunks: Vec<ArchiveChunkInfo>,
    pub focused_message: usize,
    pub messages_scroll_offset: usize,
    pub messages_total_lines: usize,
    pub messages_viewport_height: usize,
    pub streaming_content: Option<String>,
    pub streaming_thinking: Option<String>,
}

/// Provider/model catalog state (grouped)
#[derive(Debug, Clone, Default)]
pub struct CatalogState {
    pub current_provider: Option<String>,
    pub current_model: Option<String>,
    pub available_providers: Vec<ProviderInfo>,
    pub available_models: Vec<ModelInfo>,
    pub available_sessions: Vec<crate::storage::SessionMeta>,
    pub device_flow_session: Option<DeviceFlowSession>,
}

/// UI-specific state (grouped)
#[derive(Debug, Clone)]
pub struct UiState {
    pub focused_component: FocusedComponent,
    pub active_modal: Option<ModalType>,
    pub sidebar_visible: bool,
    pub theme: ThemePreset,
    pub diff_view_mode: DiffViewMode,
    pub status_message: Option<String>,
    pub flash_bar_state: FlashBarState,
    pub flash_bar_animation_frame: u8,

    // Picker states
    pub theme_picker_selected: usize,
    pub theme_picker_filter: String,
    pub theme_before_preview: Option<ThemePreset>,
    pub provider_picker_selected: usize,
    pub provider_picker_filter: String,
    pub model_picker_selected: usize,
    pub model_picker_filter: String,
    pub session_picker_selected: usize,
    pub session_picker_filter: String,

    // Sidebar state
    pub sidebar_selected_panel: usize,
    pub sidebar_selected_item: Option<usize>,
    pub sidebar_expanded_panels: [bool; 5],
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            focused_component: FocusedComponent::default(),
            active_modal: None,
            sidebar_visible: true,
            theme: ThemePreset::CatppuccinMocha,
            diff_view_mode: DiffViewMode::Auto,
            status_message: None,
            flash_bar_state: FlashBarState::Idle,
            flash_bar_animation_frame: 0,
            theme_picker_selected: 0,
            theme_picker_filter: String::new(),
            theme_before_preview: None,
            provider_picker_selected: 0,
            provider_picker_filter: String::new(),
            model_picker_selected: 0,
            model_picker_filter: String::new(),
            session_picker_selected: 0,
            session_picker_filter: String::new(),
            sidebar_selected_panel: 0,
            sidebar_selected_item: None,
            sidebar_expanded_panels: [true, true, false, false, false],
        }
    }
}

/// Shared application state (thread-safe)
///
/// This state is shared between the AppService (backend) and the UI (frontend).
/// Uses RwLock for thread-safe concurrent access.
#[derive(Debug, Clone)]
pub struct SharedState {
    inner: Arc<RwLock<StateInner>>,
}

#[derive(Debug)]
struct StateInner {
    // ========== Application State ==========
    pub should_quit: bool,

    // ========== Agent Configuration ==========
    pub agent_mode: AgentMode,
    pub build_mode: BuildMode,
    pub thinking_enabled: bool,
    pub trust_level: TrustLevel,
    pub trust_level_selected: usize,

    // ========== LLM State ==========
    pub current_provider: Option<String>,
    pub current_model: Option<String>,
    pub llm_connected: bool,
    pub llm_processing: bool,
    pub device_flow_session: Option<DeviceFlowSession>,
    pub current_correlation_id: Option<String>,
    /// Correlation ID of the currently processing message (for race condition prevention)
    pub processing_correlation_id: Option<String>,
    /// Session ID that initiated the current processing request (for session switch protection)
    pub processing_session_id: Option<String>,

    // ========== Messages ==========
    pub messages: Vec<Message>,
    pub archive_chunks: Vec<ArchiveChunkInfo>,
    pub focused_message: usize,
    /// Cursor position (char offset) within the focused message content
    pub message_cursor: usize,
    /// Visual selection start (char offset) for focused message content
    pub message_selection_start: Option<usize>,
    /// Visual selection end (char offset) for focused message content
    pub message_selection_end: Option<usize>,
    /// Sub-index for hierarchical navigation within tool groups
    /// None = at message level, Some(n) = focused on tool n within a group
    pub focused_sub_index: Option<usize>,
    pub messages_scroll_offset: usize,
    pub messages_total_lines: usize,
    pub messages_viewport_height: usize,

    // ========== Active Tools ==========
    /// Currently executing tools (for loading indicators)
    pub active_tools: Vec<crate::ui_backend::types::ActiveToolInfo>,

    // ========== Tool Group Collapse State ==========
    /// Collapsed tool groups by their starting message index
    /// Each entry is the index of the first tool message in a collapsed group
    pub collapsed_tool_groups: std::collections::HashSet<usize>,

    // ========== Session Todo List ==========
    /// Current session todo list (live-updating widget)
    pub todo_tracker: Arc<std::sync::Mutex<crate::tools::TodoTracker>>,

    // ========== Thinking Tool ==========
    /// Thinking tracker for structured reasoning
    pub thinking_tracker: Arc<std::sync::Mutex<crate::tools::ThinkingTracker>>,
    /// Whether the think tool is enabled (system prompt injection)
    pub thinking_tool_enabled: bool,

    // ========== Streaming State (BFF owns this, renderer reads only) ==========
    pub streaming_content: Option<String>,
    pub streaming_thinking: Option<String>,

    // ========== Input State ==========
    pub input_text: String,
    pub input_cursor: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub saved_input: String,
    /// Visual selection start (byte offset) for input
    pub input_selection_start: Option<usize>,
    /// Visual selection end (byte offset) for input
    pub input_selection_end: Option<usize>,
    /// Pending vim operator (e.g., 'd' for delete)
    pub pending_operator: Option<char>,
    /// Paste blocks stored for placeholder replacement in input
    pub paste_blocks: Vec<PasteBlock>,

    // ========== UI State ==========
    pub sidebar_visible: bool,
    pub theme: ThemePreset,
    pub diff_view_mode: DiffViewMode,
    pub status_message: Option<String>,
    pub flash_bar_state: FlashBarState,
    pub flash_bar_animation_frame: u8,
    pub flash_bar_expanding: bool,
    pub last_activity_at: Instant,
    pub focused_component: FocusedComponent,
    pub active_modal: Option<ModalType>,
    pub vim_mode: VimMode,

    // ========== Theme Picker State ==========
    pub theme_picker_selected: usize,
    pub theme_picker_filter: String,
    pub theme_before_preview: Option<ThemePreset>,

    // ========== Provider/Model Picker State ==========
    pub provider_picker_selected: usize,
    pub provider_picker_filter: String,
    pub model_picker_selected: usize,
    pub model_picker_filter: String,
    pub session_picker_selected: usize,
    pub session_picker_filter: String,
    /// Pending session ID when awaiting user confirmation to switch (agent is processing)
    pub pending_session_switch: Option<String>,
    /// Selected option in session switch confirm dialog (0 = Wait, 1 = Abort & Switch)
    pub session_switch_confirm_selected: usize,

    // ========== File Picker State ==========
    pub file_picker_files: Vec<String>,
    pub file_picker_filter: String,
    pub file_picker_selected: usize,

    // ========== Tools Modal State ==========
    pub tools_selected: usize,
    pub tools_scroll_offset: usize,
    pub tools_for_modal: Vec<crate::ui_backend::tool_execution::ToolInfo>,

    // ========== Plugin Modal State ==========
    pub plugin_selected: usize,

    // ========== Policy Modal State ==========
    pub policy_modal: Option<crate::tui_new::modals::policy_modal::PolicyModal>,

    // ========== Sidebar State ==========
    pub sidebar_selected_panel: usize,
    pub sidebar_selected_item: Option<usize>,
    pub sidebar_expanded_panels: [bool; 5],
    pub sidebar_scroll_offset: usize,
    pub sidebar_panel_scrolls: [usize; 5],

    // ========== Context ==========
    pub context_files: Vec<ContextFile>,
    pub tokens_used: usize,
    pub tokens_total: usize,
    /// Detailed breakdown of context token usage by source
    pub context_breakdown: ContextBreakdown,

    // ========== Session ==========
    pub session: Option<SessionInfo>,

    // ========== Tasks ==========
    pub tasks: Vec<TaskInfo>,

    // ========== Git ==========
    pub git_changes: Vec<GitChangeInfo>,

    // ========== Available Options ==========
    pub available_providers: Vec<ProviderInfo>,
    pub available_models: Vec<ModelInfo>,
    pub available_sessions: Vec<crate::storage::SessionMeta>,

    // ========== Error Notification ==========
    pub error_notification: Option<ErrorNotification>,

    // ========== Attachments ==========
    pub attachments: Vec<AttachmentInfo>,
    pub attachment_dropdown_visible: bool,
    pub attachment_tokens: Vec<AttachmentToken>,

    // ========== Questionnaire (ask_user) ==========
    pub active_questionnaire: Option<QuestionnaireState>,

    // ========== Approval Cards ==========
    pub pending_approval: Option<ApprovalCardState>,

    // ========== Rate Limiting ==========
    pub rate_limit_retry_at: Option<std::time::Instant>,
    pub rate_limit_pending_message: Option<String>,

    // ========== Pending Mode Switch ==========
    /// Mode switch requested during LLM streaming (applied when streaming completes)
    pub pending_mode_switch: Option<AgentMode>,
    /// Provider switch requested during LLM streaming (applied when streaming completes)
    pub pending_provider: Option<String>,
    /// Model switch requested during LLM streaming (applied when streaming completes)
    pub pending_model: Option<String>,
    /// Thinking tool toggle requested during LLM streaming (applied when streaming completes)
    pub pending_thinking_tool_enabled: Option<bool>,
    /// Thinking mode toggle requested during LLM streaming (applied when streaming completes)
    pub pending_thinking_enabled: Option<bool>,

    // ========== Message Queue ==========
    pub message_queue: Vec<String>,
    /// Index of task currently being edited (None = not editing)
    pub editing_task_index: Option<usize>,
    /// Content buffer for task being edited
    pub editing_task_content: String,
    /// Index of task pending deletion (for confirmation dialog)
    pub pending_delete_task_index: Option<usize>,
    /// Index of task being dragged for reordering (None = not dragging)
    pub dragging_task_index: Option<usize>,
    /// Target position for the dragged task
    pub drag_target_index: Option<usize>,

    // ========== Command Autocomplete ==========
    pub autocomplete_active: bool,
    pub autocomplete_filter: String,
    pub autocomplete_selected: usize,
    pub autocomplete_scroll_offset: usize,

    // ========== Session Cost Tracking ==========
    pub session_cost_total: f64,
    pub session_cost_by_model: Vec<(String, f64)>,
    pub session_tokens_total: usize,
    pub session_tokens_by_model: Vec<(String, usize)>,
}

impl Default for SharedState {
    fn default() -> Self {
        Self::new()
    }
}

impl SharedState {
    /// Create a new shared state
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StateInner {
                should_quit: false,
                agent_mode: AgentMode::Build,
                build_mode: BuildMode::Balanced,
                thinking_enabled: false,
                trust_level: TrustLevel::default(),
                trust_level_selected: 1, // Default to Balanced (index 1)
                current_provider: None,
                current_model: None,
                current_correlation_id: None,
                processing_correlation_id: None,
                processing_session_id: None,
                llm_connected: false,
                llm_processing: false,
                device_flow_session: None,
                messages: Vec::new(),
                archive_chunks: Vec::new(),
                focused_message: 0,
                message_cursor: 0,
                message_selection_start: None,
                message_selection_end: None,
                focused_sub_index: None,
                messages_scroll_offset: 0,
                messages_total_lines: 0,
                messages_viewport_height: 0,
                active_tools: Vec::new(),
                collapsed_tool_groups: std::collections::HashSet::new(),
                todo_tracker: Arc::new(std::sync::Mutex::new(crate::tools::TodoTracker::new())),
                thinking_tracker: Arc::new(std::sync::Mutex::new(
                    crate::tools::ThinkingTracker::new(),
                )),
                thinking_tool_enabled: false,
                streaming_content: None,
                streaming_thinking: None,
                input_text: String::new(),
                input_cursor: 0,
                input_history: Vec::new(),
                history_index: None,
                saved_input: String::new(),
                input_selection_start: None,
                input_selection_end: None,
                pending_operator: None,
                paste_blocks: Vec::new(),
                sidebar_visible: true,
                theme: ThemePreset::CatppuccinMocha,
                diff_view_mode: DiffViewMode::Auto,
                status_message: None,
                flash_bar_state: FlashBarState::Idle,
                flash_bar_animation_frame: 0,
                flash_bar_expanding: true,
                last_activity_at: Instant::now(),
                focused_component: FocusedComponent::Input,
                active_modal: None,
                vim_mode: VimMode::Insert,
                theme_picker_selected: 0,
                theme_picker_filter: String::new(),
                theme_before_preview: None,
                provider_picker_selected: 0,
                provider_picker_filter: String::new(),
                model_picker_selected: 0,
                model_picker_filter: String::new(),
                session_picker_selected: 0,
                session_picker_filter: String::new(),
                pending_session_switch: None,
                session_switch_confirm_selected: 0,
                file_picker_files: Vec::new(),
                file_picker_filter: String::new(),
                file_picker_selected: 0,
                tools_selected: 0,
                tools_scroll_offset: 0,
                tools_for_modal: Vec::new(),
                plugin_selected: 0,
                policy_modal: None,
                sidebar_selected_panel: 0,
                sidebar_selected_item: None,
                sidebar_expanded_panels: [true, true, false, false, false],
                sidebar_scroll_offset: 0,
                sidebar_panel_scrolls: [0, 0, 0, 0, 0],
                context_files: Vec::new(),
                tokens_used: 0,
                tokens_total: 1_000_000,
                context_breakdown: ContextBreakdown::default(),
                session: None,
                tasks: Vec::new(),
                git_changes: Vec::new(),
                available_providers: Vec::new(),
                available_models: Vec::new(),
                available_sessions: Vec::new(),
                error_notification: None,
                attachments: Vec::new(),
                attachment_dropdown_visible: false,
                attachment_tokens: Vec::new(),
                active_questionnaire: None,
                pending_approval: None,
                rate_limit_retry_at: None,
                rate_limit_pending_message: None,
                pending_mode_switch: None,
                pending_provider: None,
                pending_model: None,
                pending_thinking_tool_enabled: None,
                pending_thinking_enabled: None,
                message_queue: Vec::new(),
                editing_task_index: None,
                editing_task_content: String::new(),
                pending_delete_task_index: None,
                dragging_task_index: None,
                drag_target_index: None,
                autocomplete_active: false,
                autocomplete_filter: String::new(),
                autocomplete_selected: 0,
                autocomplete_scroll_offset: 0,
                session_cost_total: 0.0,
                session_cost_by_model: Vec::new(),
                session_tokens_total: 0,
                session_tokens_by_model: Vec::new(),
            })),
        }
    }

    // ========== Private Helpers ==========

    /// Get a read lock on the inner state, recovering from poison
    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, StateInner> {
        self.inner.read().unwrap_or_else(|poisoned| {
            tracing::warn!("SharedState read lock was poisoned, recovering");
            poisoned.into_inner()
        })
    }

    /// Get a write lock on the inner state, recovering from poison
    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, StateInner> {
        self.inner.write().unwrap_or_else(|poisoned| {
            tracing::warn!("SharedState write lock was poisoned, recovering");
            poisoned.into_inner()
        })
    }

    // ========== Getters ==========

    pub fn should_quit(&self) -> bool {
        self.read_inner().should_quit
    }

    pub fn agent_mode(&self) -> AgentMode {
        self.read_inner().agent_mode
    }

    pub fn build_mode(&self) -> BuildMode {
        self.read_inner().build_mode
    }

    pub fn trust_level(&self) -> TrustLevel {
        self.read_inner().trust_level
    }

    pub fn trust_level_selected(&self) -> usize {
        self.read_inner().trust_level_selected
    }

    pub fn thinking_enabled(&self) -> bool {
        self.read_inner().thinking_enabled
    }

    pub fn llm_connected(&self) -> bool {
        self.read_inner().llm_connected
    }

    pub fn llm_processing(&self) -> bool {
        self.read_inner().llm_processing
    }

    pub fn idle_elapsed(&self) -> Duration {
        let inner = self.read_inner();
        Instant::now().saturating_duration_since(inner.last_activity_at)
    }

    pub fn current_provider(&self) -> Option<String> {
        self.read_inner().current_provider.clone()
    }

    pub fn current_model(&self) -> Option<String> {
        self.read_inner().current_model.clone()
    }

    pub fn device_flow_session(&self) -> Option<DeviceFlowSession> {
        self.read_inner().device_flow_session.clone()
    }

    pub fn current_correlation_id(&self) -> Option<String> {
        self.read_inner().current_correlation_id.clone()
    }

    /// Get the session todo tracker
    pub fn todo_tracker(&self) -> Arc<std::sync::Mutex<crate::tools::TodoTracker>> {
        self.read_inner().todo_tracker.clone()
    }

    /// Get the thinking tracker
    pub fn thinking_tracker(&self) -> Arc<std::sync::Mutex<crate::tools::ThinkingTracker>> {
        self.read_inner().thinking_tracker.clone()
    }

    /// Get whether the thinking tool is enabled
    pub fn thinking_tool_enabled(&self) -> bool {
        self.read_inner().thinking_tool_enabled
    }

    /// Set whether the thinking tool is enabled
    pub fn set_thinking_tool_enabled(&self, enabled: bool) {
        self.write_inner().thinking_tool_enabled = enabled;
    }

    pub fn generate_new_correlation_id(&self) -> String {
        let correlation_id = uuid::Uuid::new_v4().to_string();
        self.write_inner().current_correlation_id = Some(correlation_id.clone());
        correlation_id
    }

    pub fn set_correlation_id(&self, id: Option<String>) {
        self.write_inner().current_correlation_id = id;
    }

    pub fn set_device_flow_session(&self, session: Option<DeviceFlowSession>) {
        self.write_inner().device_flow_session = session;
    }

    // ========== Grouped State Accessors ==========
    // New grouped accessors for better state organization
    // Old individual accessors remain for backward compatibility

    /// Get conversation state (messages, streaming)
    pub fn conversation(&self) -> ConversationState {
        let inner = self.read_inner();
        ConversationState {
            messages: inner.messages.clone(),
            archive_chunks: inner.archive_chunks.clone(),
            focused_message: inner.focused_message,
            messages_scroll_offset: inner.messages_scroll_offset,
            messages_total_lines: inner.messages_total_lines,
            messages_viewport_height: inner.messages_viewport_height,
            streaming_content: inner.streaming_content.clone(),
            streaming_thinking: inner.streaming_thinking.clone(),
        }
    }

    /// Get catalog state (providers, models, device flow)
    pub fn catalog(&self) -> CatalogState {
        let inner = self.read_inner();
        CatalogState {
            current_provider: inner.current_provider.clone(),
            current_model: inner.current_model.clone(),
            available_providers: inner.available_providers.clone(),
            available_models: inner.available_models.clone(),
            available_sessions: inner.available_sessions.clone(),
            device_flow_session: inner.device_flow_session.clone(),
        }
    }

    /// Get UI state (focus, modals, theme, pickers, sidebar)
    pub fn ui(&self) -> UiState {
        let inner = self.read_inner();
        UiState {
            focused_component: inner.focused_component,
            active_modal: inner.active_modal,
            sidebar_visible: inner.sidebar_visible,
            theme: inner.theme,
            diff_view_mode: inner.diff_view_mode,
            status_message: inner.status_message.clone(),
            flash_bar_state: inner.flash_bar_state,
            flash_bar_animation_frame: inner.flash_bar_animation_frame,
            theme_picker_selected: inner.theme_picker_selected,
            theme_picker_filter: inner.theme_picker_filter.clone(),
            theme_before_preview: inner.theme_before_preview,
            provider_picker_selected: inner.provider_picker_selected,
            provider_picker_filter: inner.provider_picker_filter.clone(),
            model_picker_selected: inner.model_picker_selected,
            model_picker_filter: inner.model_picker_filter.clone(),
            session_picker_selected: inner.session_picker_selected,
            session_picker_filter: inner.session_picker_filter.clone(),
            sidebar_selected_panel: inner.sidebar_selected_panel,
            sidebar_selected_item: inner.sidebar_selected_item,
            sidebar_expanded_panels: inner.sidebar_expanded_panels,
        }
    }

    // ========== Legacy Individual Accessors (Backward Compatible) ==========
    // These continue to work and delegate to the flat fields
    // Eventually, new code should use grouped accessors above

    pub fn messages(&self) -> Vec<Message> {
        self.read_inner().messages.clone()
    }

    /// Access messages with zero-copy callback pattern (more efficient for rendering)
    pub fn with_messages<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&[Message]) -> R,
    {
        let inner = self.read_inner();
        f(&inner.messages)
    }

    /// Get message count without cloning
    pub fn message_count(&self) -> usize {
        self.read_inner().messages.len()
    }

    pub fn messages_scroll_offset(&self) -> usize {
        self.read_inner().messages_scroll_offset
    }

    pub fn messages_total_lines(&self) -> usize {
        self.read_inner().messages_total_lines
    }

    pub fn messages_viewport_height(&self) -> usize {
        self.read_inner().messages_viewport_height
    }

    pub fn focused_message(&self) -> usize {
        self.read_inner().focused_message
    }

    pub fn input_text(&self) -> String {
        self.read_inner().input_text.clone()
    }

    pub fn input_cursor(&self) -> usize {
        self.read_inner().input_cursor
    }

    pub fn sidebar_visible(&self) -> bool {
        self.read_inner().sidebar_visible
    }

    pub fn theme(&self) -> ThemePreset {
        self.read_inner().theme
    }

    pub fn diff_view_mode(&self) -> DiffViewMode {
        self.read_inner().diff_view_mode
    }

    pub fn context_files(&self) -> Vec<ContextFile> {
        self.read_inner().context_files.clone()
    }

    pub fn available_providers(&self) -> Vec<ProviderInfo> {
        self.read_inner().available_providers.clone()
    }

    pub fn available_models(&self) -> Vec<ModelInfo> {
        self.read_inner().available_models.clone()
    }

    pub fn focused_component(&self) -> FocusedComponent {
        self.read_inner().focused_component
    }

    pub fn active_modal(&self) -> Option<ModalType> {
        self.read_inner().active_modal
    }

    pub fn vim_mode(&self) -> VimMode {
        self.read_inner().vim_mode
    }

    /// Flag to enable/disable vim-style key handling based on active context.
    pub fn is_vim_key_enabled(&self) -> bool {
        let inner = self.read_inner();
        if matches!(
            inner.active_modal,
            Some(ModalType::ThemePicker)
                | Some(ModalType::ProviderPicker)
                | Some(ModalType::ModelPicker)
                | Some(ModalType::SessionPicker)
                | Some(ModalType::FilePicker)
                | Some(ModalType::TaskEdit)
        ) {
            return false;
        }

        if let Some(q) = &inner.active_questionnaire {
            if q.question_type == QuestionType::FreeText || q.allow_other {
                return false;
            }
        }

        true
    }

    pub fn theme_picker_selected(&self) -> usize {
        self.read_inner().theme_picker_selected
    }

    pub fn theme_picker_filter(&self) -> String {
        self.read_inner().theme_picker_filter.clone()
    }

    pub fn sidebar_selected_panel(&self) -> usize {
        self.read_inner().sidebar_selected_panel
    }

    pub fn sidebar_selected_item(&self) -> Option<usize> {
        self.read_inner().sidebar_selected_item
    }

    pub fn sidebar_expanded_panels(&self) -> [bool; 5] {
        self.read_inner().sidebar_expanded_panels
    }

    pub fn sidebar_scroll_offset(&self) -> usize {
        self.read_inner().sidebar_scroll_offset
    }

    pub fn sidebar_panel_scrolls(&self) -> [usize; 5] {
        self.read_inner().sidebar_panel_scrolls
    }

    pub fn sidebar_panel_scroll(&self, panel: usize) -> usize {
        self.read_inner()
            .sidebar_panel_scrolls
            .get(panel)
            .copied()
            .unwrap_or(0)
    }

    pub fn theme_before_preview(&self) -> Option<ThemePreset> {
        self.read_inner().theme_before_preview
    }

    pub fn provider_picker_selected(&self) -> usize {
        self.read_inner().provider_picker_selected
    }

    pub fn provider_picker_filter(&self) -> String {
        self.read_inner().provider_picker_filter.clone()
    }

    pub fn model_picker_selected(&self) -> usize {
        self.read_inner().model_picker_selected
    }

    pub fn model_picker_filter(&self) -> String {
        self.read_inner().model_picker_filter.clone()
    }

    pub fn session_picker_selected(&self) -> usize {
        self.read_inner().session_picker_selected
    }

    pub fn session_picker_filter(&self) -> String {
        self.read_inner().session_picker_filter.clone()
    }

    pub fn pending_session_switch(&self) -> Option<String> {
        self.read_inner().pending_session_switch.clone()
    }

    pub fn session_switch_confirm_selected(&self) -> usize {
        self.read_inner().session_switch_confirm_selected
    }

    pub fn file_picker_files(&self) -> Vec<String> {
        self.read_inner().file_picker_files.clone()
    }

    pub fn file_picker_filter(&self) -> String {
        self.read_inner().file_picker_filter.clone()
    }

    pub fn file_picker_selected(&self) -> usize {
        self.read_inner().file_picker_selected
    }

    pub fn tools_selected(&self) -> usize {
        self.read_inner().tools_selected
    }

    pub fn tools_scroll_offset(&self) -> usize {
        self.read_inner().tools_scroll_offset
    }

    pub fn set_tools_scroll_offset(&self, offset: usize) {
        self.write_inner().tools_scroll_offset = offset;
    }

    pub fn tools_for_modal(&self) -> Vec<crate::ui_backend::tool_execution::ToolInfo> {
        self.read_inner().tools_for_modal.clone()
    }

    pub fn set_tools_for_modal(&self, tools: Vec<crate::ui_backend::tool_execution::ToolInfo>) {
        self.write_inner().tools_for_modal = tools;
    }

    pub fn plugin_selected(&self) -> usize {
        self.read_inner().plugin_selected
    }

    pub fn policy_modal(&self) -> Option<crate::tui_new::modals::policy_modal::PolicyModal> {
        self.read_inner().policy_modal.clone()
    }

    pub fn set_policy_modal(
        &self,
        modal: Option<crate::tui_new::modals::policy_modal::PolicyModal>,
    ) {
        self.write_inner().policy_modal = modal;
    }

    pub fn error_notification(&self) -> Option<ErrorNotification> {
        self.read_inner().error_notification.clone()
    }

    pub fn status_message(&self) -> Option<String> {
        self.read_inner().status_message.clone()
    }

    pub fn flash_bar_state(&self) -> FlashBarState {
        self.read_inner().flash_bar_state
    }

    pub fn flash_bar_animation_frame(&self) -> u8 {
        self.read_inner().flash_bar_animation_frame
    }

    pub fn attachments(&self) -> Vec<AttachmentInfo> {
        self.read_inner().attachments.clone()
    }

    pub fn attachment_dropdown_visible(&self) -> bool {
        self.read_inner().attachment_dropdown_visible
    }

    pub fn active_questionnaire(&self) -> Option<QuestionnaireState> {
        self.read_inner().active_questionnaire.clone()
    }

    pub fn pending_approval(&self) -> Option<ApprovalCardState> {
        self.read_inner().pending_approval.clone()
    }

    pub fn rate_limit_retry_at(&self) -> Option<std::time::Instant> {
        self.read_inner().rate_limit_retry_at
    }

    pub fn rate_limit_pending_message(&self) -> Option<String> {
        self.read_inner().rate_limit_pending_message.clone()
    }

    pub fn is_rate_limited(&self) -> bool {
        if let Some(retry_at) = self.read_inner().rate_limit_retry_at {
            retry_at > std::time::Instant::now()
        } else {
            false
        }
    }

    pub fn session(&self) -> Option<SessionInfo> {
        self.read_inner().session.clone()
    }

    pub fn tasks(&self) -> Vec<TaskInfo> {
        self.read_inner().tasks.clone()
    }

    pub fn git_changes(&self) -> Vec<GitChangeInfo> {
        self.read_inner().git_changes.clone()
    }

    pub fn tokens_used(&self) -> usize {
        self.read_inner().tokens_used
    }

    pub fn tokens_total(&self) -> usize {
        self.read_inner().tokens_total
    }

    pub fn streaming_content(&self) -> Option<String> {
        self.read_inner().streaming_content.clone()
    }

    pub fn streaming_thinking(&self) -> Option<String> {
        self.read_inner().streaming_thinking.clone()
    }

    // ========== Setters ==========

    pub fn set_should_quit(&self, value: bool) {
        self.write_inner().should_quit = value;
    }

    pub fn set_agent_mode(&self, mode: AgentMode) {
        self.write_inner().agent_mode = mode;
    }

    pub fn set_build_mode(&self, mode: BuildMode) {
        self.write_inner().build_mode = mode;
    }

    pub fn set_trust_level(&self, level: TrustLevel) {
        self.write_inner().trust_level = level;
    }

    pub fn set_trust_level_selected(&self, selected: usize) {
        self.write_inner().trust_level_selected = selected;
    }

    pub fn set_thinking_enabled(&self, enabled: bool) {
        self.write_inner().thinking_enabled = enabled;
    }

    pub fn set_llm_connected(&self, connected: bool) {
        self.write_inner().llm_connected = connected;
    }

    pub fn set_llm_processing(&self, processing: bool) {
        let mut inner = self.write_inner();
        inner.llm_processing = processing;

        if processing {
            if !matches!(
                inner.flash_bar_state,
                FlashBarState::Error | FlashBarState::Warning
            ) {
                inner.flash_bar_state = FlashBarState::Working;
                inner.flash_bar_animation_frame = 0;
                inner.flash_bar_expanding = true;
            }
        } else if inner.flash_bar_state == FlashBarState::Working {
            inner.flash_bar_state = FlashBarState::Idle;
            inner.flash_bar_animation_frame = 0;
            inner.flash_bar_expanding = true;
        }

        inner.last_activity_at = Instant::now();
    }

    pub fn processing_correlation_id(&self) -> Option<String> {
        self.read_inner().processing_correlation_id.clone()
    }

    pub fn set_processing_correlation_id(&self, id: Option<String>) {
        self.write_inner().processing_correlation_id = id;
    }

    pub fn processing_session_id(&self) -> Option<String> {
        self.read_inner().processing_session_id.clone()
    }

    pub fn set_processing_session_id(&self, id: Option<String>) {
        self.write_inner().processing_session_id = id;
    }

    pub fn set_provider(&self, provider: Option<String>) {
        self.write_inner().current_provider = provider;
    }

    pub fn set_model(&self, model: Option<String>) {
        self.write_inner().current_model = model;
    }

    pub fn set_input_text(&self, text: String) {
        let mut inner = self.write_inner();
        inner.input_text = text;
        inner.last_activity_at = Instant::now();
    }

    pub fn set_input_cursor(&self, cursor: usize) {
        self.write_inner().input_cursor = cursor;
    }

    pub fn paste_blocks(&self) -> Vec<PasteBlock> {
        self.read_inner().paste_blocks.clone()
    }

    pub fn paste_placeholders(&self) -> Vec<String> {
        self.read_inner()
            .paste_blocks
            .iter()
            .map(|block| block.placeholder.clone())
            .collect()
    }

    pub fn add_paste_block(&self, placeholder: String, content: String) {
        self.write_inner().paste_blocks.push(PasteBlock {
            placeholder,
            content,
        });
    }

    pub fn remove_paste_block(&self, index: usize) {
        let mut inner = self.write_inner();
        if index < inner.paste_blocks.len() {
            inner.paste_blocks.remove(index);
        }
    }

    pub fn clear_paste_blocks(&self) {
        self.write_inner().paste_blocks.clear();
    }

    pub fn expand_paste_blocks(&self, text: &str) -> String {
        let blocks = self.paste_blocks();
        if blocks.is_empty() {
            return text.to_string();
        }

        let mut expanded = text.to_string();
        for block in blocks {
            if let Some(pos) = expanded.find(&block.placeholder) {
                expanded.replace_range(pos..pos + block.placeholder.len(), &block.content);
            }
        }
        expanded
    }

    /// Get current input selection range (start, end)
    pub fn input_selection(&self) -> Option<(usize, usize)> {
        let inner = self.read_inner();
        match (inner.input_selection_start, inner.input_selection_end) {
            (Some(start), Some(end)) => Some((start.min(end), start.max(end))),
            _ => None,
        }
    }

    /// Set input selection range
    pub fn set_input_selection(&self, start: usize, end: usize) {
        let mut inner = self.write_inner();
        inner.input_selection_start = Some(start);
        inner.input_selection_end = Some(end);
    }

    /// Clear input selection
    pub fn clear_input_selection(&self) {
        let mut inner = self.write_inner();
        inner.input_selection_start = None;
        inner.input_selection_end = None;
    }

    /// Get current message selection range (start, end)
    pub fn message_selection(&self) -> Option<(usize, usize)> {
        let inner = self.read_inner();
        match (inner.message_selection_start, inner.message_selection_end) {
            (Some(start), Some(end)) => Some((start.min(end), start.max(end))),
            _ => None,
        }
    }

    /// Set message selection range
    pub fn set_message_selection(&self, start: usize, end: usize) {
        let mut inner = self.write_inner();
        let max = focused_message_char_len(&inner);
        inner.message_selection_start = Some(start.min(max));
        inner.message_selection_end = Some(end.min(max));
    }

    /// Clear message selection
    pub fn clear_message_selection(&self) {
        let mut inner = self.write_inner();
        inner.message_selection_start = None;
        inner.message_selection_end = None;
    }

    /// Get current message cursor position
    pub fn message_cursor(&self) -> usize {
        self.read_inner().message_cursor
    }

    /// Set message cursor position
    pub fn set_message_cursor(&self, cursor: usize) {
        let mut inner = self.write_inner();
        let max = focused_message_char_len(&inner);
        inner.message_cursor = cursor.min(max);
    }

    /// Set pending operator (vim)
    pub fn set_pending_operator(&self, op: Option<char>) {
        self.write_inner().pending_operator = op;
    }

    /// Get pending operator
    pub fn pending_operator(&self) -> Option<char> {
        self.read_inner().pending_operator
    }

    // ========== Vim Navigation Methods ==========

    /// Move cursor right by one character
    pub fn move_cursor_right(&self) {
        let mut inner = self.write_inner();
        if inner.input_cursor < inner.input_text.len() {
            // Move by one character
            let text = &inner.input_text;
            if let Some(c) = text[inner.input_cursor..].chars().next() {
                inner.input_cursor += c.len_utf8();
            }
        }
    }

    /// Move cursor left by one character
    pub fn move_cursor_left(&self) {
        let mut inner = self.write_inner();
        if inner.input_cursor > 0 {
            // Move by one character backward
            let text = &inner.input_text[..inner.input_cursor];
            if let Some(c) = text.chars().next_back() {
                inner.input_cursor -= c.len_utf8();
            }
        }
    }

    /// Move cursor to end of current line
    pub fn move_cursor_to_line_end(&self) {
        let mut inner = self.write_inner();
        let text = &inner.input_text;
        // Find next newline or end of text
        if let Some(pos) = text[inner.input_cursor..].find('\n') {
            inner.input_cursor += pos;
        } else {
            inner.input_cursor = text.len();
        }
    }

    /// Move cursor to start of current line
    pub fn move_cursor_to_line_start(&self) {
        let mut inner = self.write_inner();
        let text = &inner.input_text;
        // Find previous newline or start of text
        if let Some(pos) = text[..inner.input_cursor].rfind('\n') {
            inner.input_cursor = pos + 1;
        } else {
            inner.input_cursor = 0;
        }
    }

    /// Move cursor forward by one word
    pub fn move_cursor_word_forward(&self) {
        let mut inner = self.write_inner();
        let text = &inner.input_text;
        let pos = inner.input_cursor;

        // Skip current word characters
        let rest = &text[pos..];
        let skip_word = rest
            .find(|c: char| !c.is_alphanumeric() && c != '_')
            .unwrap_or(rest.len());

        // Skip whitespace after word
        let after_word = &text[pos + skip_word..];
        let skip_space = after_word
            .find(|c: char| !c.is_whitespace())
            .unwrap_or(after_word.len());

        inner.input_cursor = (pos + skip_word + skip_space).min(text.len());
    }

    /// Move cursor backward by one word
    pub fn move_cursor_word_backward(&self) {
        let mut inner = self.write_inner();
        let text = &inner.input_text;
        let pos = inner.input_cursor;

        if pos == 0 {
            return;
        }

        // Skip whitespace before cursor
        let before = &text[..pos];
        let trimmed = before.trim_end();
        let trimmed_len = trimmed.len();

        if trimmed_len == 0 {
            inner.input_cursor = 0;
            return;
        }

        // Find start of previous word
        let word_start = trimmed
            .rfind(|c: char| !c.is_alphanumeric() && c != '_')
            .map(|p| p + 1)
            .unwrap_or(0);

        inner.input_cursor = word_start;
    }

    /// Move cursor up one line
    pub fn move_cursor_up(&self) {
        let mut inner = self.write_inner();
        let text = &inner.input_text;
        let pos = inner.input_cursor;

        // Find start of current line
        let line_start = text[..pos].rfind('\n').map(|p| p + 1).unwrap_or(0);
        let col = pos - line_start;

        // Find previous line
        if line_start > 0 {
            let prev_line_end = line_start - 1; // Position of \n
            let prev_line_start = text[..prev_line_end]
                .rfind('\n')
                .map(|p| p + 1)
                .unwrap_or(0);
            let prev_line_len = prev_line_end - prev_line_start;

            // Move to same column or end of previous line
            inner.input_cursor = prev_line_start + col.min(prev_line_len);
        }
    }

    /// Insert a newline at cursor position
    pub fn insert_newline(&self) {
        let mut inner = self.write_inner();
        let pos = inner.input_cursor;
        inner.input_text.insert(pos, '\n');
        inner.input_cursor = pos + 1;
    }

    /// Delete character after cursor
    pub fn delete_char_after(&self) {
        let mut inner = self.write_inner();
        let pos = inner.input_cursor;
        if pos < inner.input_text.len() {
            if let Some(c) = inner.input_text[pos..].chars().next() {
                inner.input_text.drain(pos..pos + c.len_utf8());
            }
        }
    }

    /// Delete character before cursor
    pub fn delete_char_before(&self) {
        let mut inner = self.write_inner();
        if inner.input_cursor > 0 {
            let pos = inner.input_cursor;
            if let Some(c) = inner.input_text[..pos].chars().next_back() {
                let char_len = c.len_utf8();
                inner.input_text.drain(pos - char_len..pos);
                inner.input_cursor -= char_len;
            }
        }
    }

    // ========== Message Cursor Navigation ==========

    pub fn move_message_cursor_left(&self) {
        let mut inner = self.write_inner();
        if inner.message_cursor > 0 {
            inner.message_cursor -= 1;
        }
    }

    pub fn move_message_cursor_right(&self) {
        let mut inner = self.write_inner();
        let max = focused_message_char_len(&inner);
        if inner.message_cursor < max {
            inner.message_cursor += 1;
        }
    }

    pub fn move_message_cursor_to_line_start(&self) {
        let mut inner = self.write_inner();
        let Some(content) = focused_message_content(&inner) else {
            return;
        };
        let chars: Vec<char> = content.chars().collect();
        let mut idx = inner.message_cursor.min(chars.len());
        while idx > 0 && chars[idx - 1] != '\n' {
            idx -= 1;
        }
        inner.message_cursor = idx;
    }

    pub fn move_message_cursor_to_line_end(&self) {
        let mut inner = self.write_inner();
        let Some(content) = focused_message_content(&inner) else {
            return;
        };
        let chars: Vec<char> = content.chars().collect();
        let mut idx = inner.message_cursor.min(chars.len());
        while idx < chars.len() && chars[idx] != '\n' {
            idx += 1;
        }
        inner.message_cursor = idx;
    }

    pub fn move_message_cursor_word_forward(&self) {
        let mut inner = self.write_inner();
        let Some(content) = focused_message_content(&inner) else {
            return;
        };
        let chars: Vec<char> = content.chars().collect();
        let mut idx = inner.message_cursor.min(chars.len());
        while idx < chars.len() && !chars[idx].is_whitespace() {
            idx += 1;
        }
        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }
        inner.message_cursor = idx.min(chars.len());
    }

    pub fn move_message_cursor_word_backward(&self) {
        let mut inner = self.write_inner();
        let Some(content) = focused_message_content(&inner) else {
            return;
        };
        if inner.message_cursor == 0 {
            return;
        }
        let chars: Vec<char> = content.chars().collect();
        let mut idx = inner.message_cursor.saturating_sub(1).min(chars.len());
        while idx > 0 && chars[idx].is_whitespace() {
            idx -= 1;
        }
        while idx > 0 && !chars[idx - 1].is_whitespace() {
            idx -= 1;
        }
        inner.message_cursor = idx;
    }

    pub fn set_sidebar_visible(&self, visible: bool) {
        self.write_inner().sidebar_visible = visible;
    }

    pub fn set_theme(&self, theme: ThemePreset) {
        self.write_inner().theme = theme;
    }

    pub fn set_diff_view_mode(&self, mode: DiffViewMode) {
        self.write_inner().diff_view_mode = mode;
    }

    pub fn set_status_message(&self, message: Option<String>) {
        self.write_inner().status_message = message;
    }

    pub fn set_flash_bar_state(&self, state: FlashBarState) {
        let mut inner = self.write_inner();
        if inner.flash_bar_state != state {
            inner.flash_bar_state = state;
            if state == FlashBarState::Working {
                inner.flash_bar_animation_frame = 0;
                inner.flash_bar_expanding = true;
            }
        }
    }

    pub fn set_flash_bar_animation_frame(&self, frame: u8) {
        let mut inner = self.write_inner();
        inner.flash_bar_animation_frame = frame.min(MAX_FLASH_BAR_FRAME);
    }

    pub fn advance_flash_bar_animation(&self) {
        let mut inner = self.write_inner();
        let mut frame = inner.flash_bar_animation_frame.min(MAX_FLASH_BAR_FRAME);

        if inner.flash_bar_expanding {
            if frame >= MAX_FLASH_BAR_FRAME {
                inner.flash_bar_expanding = false;
                frame = frame.saturating_sub(1);
            } else {
                frame += 1;
            }
        } else if frame == 0 {
            inner.flash_bar_expanding = true;
            frame = if MAX_FLASH_BAR_FRAME > 0 { 1 } else { 0 };
        } else {
            frame = frame.saturating_sub(1);
        }

        inner.flash_bar_animation_frame = frame;
    }

    pub fn add_message(&self, message: Message) {
        self.write_inner().messages.push(message);
    }

    pub fn insert_messages_at(&self, index: usize, messages: Vec<Message>) {
        let mut inner = self.write_inner();
        let idx = index.min(inner.messages.len());
        inner.messages.splice(idx..idx, messages);
    }

    pub fn set_archive_chunks(&self, chunks: Vec<ArchiveChunkInfo>) {
        let mut inner = self.write_inner();
        inner.archive_chunks = chunks;
        update_archive_marker(&mut inner);
    }

    pub fn archive_chunks(&self) -> Vec<ArchiveChunkInfo> {
        self.read_inner().archive_chunks.clone()
    }

    pub fn pop_next_archive_chunk(&self) -> Option<ArchiveChunkInfo> {
        let mut inner = self.write_inner();
        let chunk = if inner.archive_chunks.is_empty() {
            None
        } else {
            Some(inner.archive_chunks.remove(0))
        };
        update_archive_marker(&mut inner);
        chunk
    }

    pub fn is_focused_archive_marker(&self) -> bool {
        let inner = self.read_inner();
        inner
            .messages
            .get(inner.focused_message)
            .is_some_and(is_archive_marker_message)
    }

    pub fn archive_marker_index(&self) -> Option<usize> {
        let inner = self.read_inner();
        inner.messages.iter().position(is_archive_marker_message)
    }

    pub fn remove_oldest_messages(&self, count: usize) {
        let mut inner = self.write_inner();
        let mut remaining = count;
        let mut idx = 0usize;
        while remaining > 0 && idx < inner.messages.len() {
            if is_archive_marker_message(&inner.messages[idx]) {
                idx += 1;
                continue;
            }
            inner.messages.remove(idx);
            remaining -= 1;
        }
        if count > remaining {
            inner.focused_message = inner
                .focused_message
                .saturating_sub(count.saturating_sub(remaining));
        }
    }

    /// Update the last tool message for a given tool name (for in-place animation)
    /// Returns true if a message was updated, false if no matching message found
    pub fn update_tool_message(
        &self,
        tool_name: &str,
        new_content: String,
        collapsed: bool,
    ) -> bool {
        let mut inner = self.write_inner();
        // Find the last Tool message that matches this tool name
        // Tool message format: "status|name|content"
        for msg in inner.messages.iter_mut().rev() {
            if msg.role == crate::ui_backend::MessageRole::Tool {
                // Check if this message is for the same tool
                let parts: Vec<&str> = msg.content.splitn(3, '|').collect();
                if parts.len() >= 2 && parts[1] == tool_name {
                    // Update this message in place
                    msg.content = new_content;
                    msg.collapsed = collapsed;
                    return true;
                }
            }
        }
        false
    }

    /// Collapse all tool messages except the most recent one
    /// This provides a clean view where only the current/active tool is expanded
    pub fn collapse_all_tools_except_last(&self) {
        let mut inner = self.write_inner();
        let messages = &mut inner.messages;

        // Find the index of the last tool message
        let last_tool_idx = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, msg)| msg.role == crate::ui_backend::MessageRole::Tool)
            .map(|(idx, _)| idx);

        // Collapse all tool messages except the last one
        for (idx, msg) in messages.iter_mut().enumerate() {
            if msg.role == crate::ui_backend::MessageRole::Tool {
                msg.collapsed = Some(idx) != last_tool_idx;
            }
        }
    }

    /// Collapse all thinking messages except the most recent one
    pub fn collapse_all_thinking_except_last(&self) {
        let mut inner = self.write_inner();
        let messages = &mut inner.messages;

        let last_thinking_idx = messages
            .iter()
            .enumerate()
            .rev()
            .find(|(_, msg)| msg.role == crate::ui_backend::MessageRole::Thinking)
            .map(|(idx, _)| idx);

        for (idx, msg) in messages.iter_mut().enumerate() {
            if msg.role == crate::ui_backend::MessageRole::Thinking {
                msg.collapsed = Some(idx) != last_thinking_idx;
            }
        }
    }

    /// Toggle collapse state of a specific message by index
    /// Returns true if the message was found and toggled
    pub fn toggle_message_collapse(&self, index: usize) -> bool {
        let mut inner = self.write_inner();
        if let Some(msg) = inner.messages.get_mut(index) {
            msg.collapsed = !msg.collapsed;
            true
        } else {
            false
        }
    }

    /// Toggle collapse state of a tool group by its starting index
    pub fn toggle_tool_group_collapse(&self, group_start_index: usize) {
        let mut inner = self.write_inner();
        if inner.collapsed_tool_groups.contains(&group_start_index) {
            inner.collapsed_tool_groups.remove(&group_start_index);
        } else {
            inner.collapsed_tool_groups.insert(group_start_index);
        }
    }

    /// Check if a tool group is collapsed
    pub fn is_tool_group_collapsed(&self, group_start_index: usize) -> bool {
        self.read_inner()
            .collapsed_tool_groups
            .contains(&group_start_index)
    }

    /// Expand a tool group (used for auto-expand when new tool starts)
    pub fn expand_tool_group(&self, group_start_index: usize) {
        let mut inner = self.write_inner();
        inner.collapsed_tool_groups.remove(&group_start_index);
    }

    /// Get a clone of the collapsed tool groups set
    pub fn collapsed_tool_groups(&self) -> std::collections::HashSet<usize> {
        self.read_inner().collapsed_tool_groups.clone()
    }

    /// Replace collapsed tool groups set
    pub fn set_collapsed_tool_groups(&self, groups: std::collections::HashSet<usize>) {
        self.write_inner().collapsed_tool_groups = groups;
    }

    /// Get the role of the focused message
    pub fn focused_message_role(&self) -> Option<MessageRole> {
        let inner = self.read_inner();
        inner
            .messages
            .get(inner.focused_message)
            .map(|msg| msg.role)
    }

    /// Check if the focused message is a tool message
    pub fn is_focused_message_tool(&self) -> bool {
        let inner = self.read_inner();
        if let Some(msg) = inner.messages.get(inner.focused_message) {
            msg.role == crate::ui_backend::MessageRole::Tool
        } else {
            false
        }
    }

    pub fn set_messages(&self, messages: Vec<Message>) {
        let mut inner = self.write_inner();
        inner.messages = messages;

        let msg_count = inner.messages.len();
        if msg_count == 0 {
            inner.focused_message = 0;
            inner.focused_sub_index = None;
            inner.message_cursor = 0;
            inner.message_selection_start = None;
            inner.message_selection_end = None;
            return;
        }

        if inner.focused_message >= msg_count {
            inner.focused_message = msg_count.saturating_sub(1);
            inner.focused_sub_index = None;
        } else if let Some(sub_idx) = inner.focused_sub_index {
            if inner.focused_message.saturating_add(sub_idx) >= msg_count {
                inner.focused_sub_index = None;
            } else {
                let focused = inner.focused_message;
                let is_tool = inner
                    .messages
                    .get(focused)
                    .is_some_and(|msg| msg.role == MessageRole::Tool);
                if !is_tool {
                    inner.focused_sub_index = None;
                }
            }
        }
        inner.message_cursor = inner.message_cursor.min(focused_message_char_len(&inner));
        inner.message_selection_start = None;
        inner.message_selection_end = None;
    }

    pub fn clear_messages(&self) {
        let mut inner = self.write_inner();
        inner.messages.clear();
        inner.focused_message = 0;
        inner.message_cursor = 0;
        inner.message_selection_start = None;
        inner.message_selection_end = None;
    }

    /// Remove system messages containing specific text (used for transient notifications)
    pub fn remove_system_messages_containing(&self, text: &str) {
        let mut inner = self.write_inner();
        inner
            .messages
            .retain(|msg| !(msg.role == MessageRole::System && msg.content.contains(text)));
    }

    // ========== Active Tools ==========

    /// Get active tools
    pub fn active_tools(&self) -> Vec<crate::ui_backend::types::ActiveToolInfo> {
        self.read_inner().active_tools.clone()
    }

    /// Add a new active tool (tool started)
    pub fn add_active_tool(&self, tool: crate::ui_backend::types::ActiveToolInfo) {
        self.write_inner().active_tools.push(tool);
    }

    /// Complete an active tool by name (tool finished)
    pub fn complete_active_tool(&self, name: &str, result: String, success: bool) {
        let mut inner = self.write_inner();
        if let Some(tool) = inner.active_tools.iter_mut().find(|t| t.name == name) {
            tool.complete(result, success);
        }
    }

    /// Remove completed tools from active list
    pub fn cleanup_completed_tools(&self) {
        let mut inner = self.write_inner();
        inner
            .active_tools
            .retain(|t| matches!(t.status, crate::ui_backend::types::ToolStatus::Running));
    }

    /// Clear all active tools
    pub fn clear_active_tools(&self) {
        self.write_inner().active_tools.clear();
    }

    pub fn set_messages_scroll_offset(&self, offset: usize) {
        self.write_inner().messages_scroll_offset = offset;
    }

    pub fn set_messages_metrics(&self, total_lines: usize, viewport_height: usize) {
        let mut inner = self.write_inner();
        inner.messages_total_lines = total_lines;
        inner.messages_viewport_height = viewport_height;
    }

    /// Set focused message index
    pub fn set_focused_message(&self, index: usize) {
        let mut inner = self.write_inner();
        inner.focused_message = index;
        inner.message_cursor = 0;
        inner.message_selection_start = None;
        inner.message_selection_end = None;
        // Reset sub-index when moving to a new message
        inner.focused_sub_index = None;
    }

    /// Get the sub-index for hierarchical navigation
    pub fn focused_sub_index(&self) -> Option<usize> {
        self.read_inner().focused_sub_index
    }

    /// Set the sub-index for hierarchical navigation (dive into a group)
    pub fn set_focused_sub_index(&self, sub: Option<usize>) {
        self.write_inner().focused_sub_index = sub;
    }

    /// Dive into a tool group (set sub-index to 0)
    pub fn dive_into_group(&self) {
        self.write_inner().focused_sub_index = Some(0);
    }

    /// Exit from a tool group (reset sub-index)
    pub fn exit_group(&self) {
        self.write_inner().focused_sub_index = None;
    }

    /// Check if currently navigating within a group
    pub fn is_in_group(&self) -> bool {
        self.read_inner().focused_sub_index.is_some()
    }

    /// Scroll to bottom (most recent message)
    pub fn scroll_to_bottom(&self) {
        let count = self.message_count();
        if count > 0 {
            self.set_messages_scroll_offset(usize::MAX);
            self.set_focused_message(count.saturating_sub(1));
        }
    }

    /// Ensure the focused message is visible in the viewport
    /// Uses a heuristic: ~4 lines per message on average
    pub fn ensure_focused_message_visible(&self) {
        let inner = self.read_inner();
        let focused = inner.focused_message;
        let viewport_height = inner.messages_viewport_height;
        let total_lines = inner.messages_total_lines;
        let msg_count = inner.messages.len();

        if msg_count == 0 || viewport_height == 0 {
            return;
        }

        // Estimate lines per message
        let lines_per_msg = if msg_count > 0 {
            (total_lines as f32 / msg_count as f32).max(3.0)
        } else {
            4.0
        };

        // Estimate the line position of the focused message
        let estimated_line = (focused as f32 * lines_per_msg) as usize;

        // Current scroll offset (normalize usize::MAX)
        let max_offset = total_lines.saturating_sub(viewport_height);
        let current_offset = if inner.messages_scroll_offset == usize::MAX {
            max_offset
        } else {
            inner.messages_scroll_offset.min(max_offset)
        };

        // Calculate visible range
        let visible_start = current_offset;
        let visible_end = current_offset + viewport_height;

        // Add margin (keep cursor away from edges)
        let margin = (viewport_height / 4).max(2);

        drop(inner); // Release read lock before writing

        // Scroll up if focused message is above visible area
        if estimated_line < visible_start + margin {
            let new_offset = estimated_line.saturating_sub(margin);
            self.set_messages_scroll_offset(new_offset);
        }
        // Scroll down if focused message is below visible area
        else if estimated_line > visible_end.saturating_sub(margin) {
            let new_offset = (estimated_line + margin).saturating_sub(viewport_height);
            self.set_messages_scroll_offset(new_offset.min(max_offset));
        }
    }

    /// Snap focused message to the current visible viewport if it is off-screen.
    /// This keeps the cursor visible when entering the Messages panel.
    pub fn snap_focused_message_to_viewport(&self) {
        let inner = self.read_inner();
        let msg_count = inner.messages.len();
        let viewport_height = inner.messages_viewport_height;
        let total_lines = inner.messages_total_lines;

        if msg_count == 0 || viewport_height == 0 {
            return;
        }

        let lines_per_msg = (total_lines as f32 / msg_count as f32).max(3.0);
        let max_offset = total_lines.saturating_sub(viewport_height);
        let current_offset = if inner.messages_scroll_offset == usize::MAX {
            max_offset
        } else {
            inner.messages_scroll_offset.min(max_offset)
        };

        let visible_start = current_offset;
        let visible_end = current_offset.saturating_add(viewport_height);
        let estimated_line = (inner.focused_message as f32 * lines_per_msg) as usize;

        if estimated_line < visible_start || estimated_line >= visible_end {
            let target_line = if estimated_line < visible_start {
                visible_start
            } else {
                visible_end.saturating_sub(1)
            };
            let target_msg = ((target_line as f32) / lines_per_msg).floor() as usize;
            drop(inner);
            self.set_focused_message(target_msg.min(msg_count.saturating_sub(1)));
        }
    }

    pub fn add_context_file(&self, file: ContextFile) {
        self.write_inner().context_files.push(file);
    }

    pub fn remove_context_file(&self, path: &str) {
        self.write_inner().context_files.retain(|f| f.path != path);
    }

    pub fn clear_context_files(&self) {
        self.write_inner().context_files.clear();
    }

    pub fn set_available_providers(&self, providers: Vec<ProviderInfo>) {
        self.write_inner().available_providers = providers;
    }

    pub fn set_available_models(&self, models: Vec<ModelInfo>) {
        self.write_inner().available_models = models;
    }

    pub fn add_to_history(&self, text: String) {
        self.write_inner().input_history.push(text);
    }

    pub fn clear_input(&self) {
        let mut inner = self.write_inner();
        inner.input_text.clear();
        inner.input_cursor = 0;
        inner.history_index = None;
        inner.saved_input.clear();
        inner.input_selection_start = None;
        inner.input_selection_end = None;
        inner.pending_operator = None;
        inner.paste_blocks.clear();
    }

    pub fn input_history(&self) -> Vec<String> {
        self.read_inner().input_history.clone()
    }

    pub fn history_index(&self) -> Option<usize> {
        self.read_inner().history_index
    }

    pub fn navigate_history_prev(&self) {
        let mut inner = self.write_inner();
        if inner.input_history.is_empty() {
            return;
        }

        match inner.history_index {
            None => {
                // Save current input and go to most recent history
                inner.saved_input = inner.input_text.clone();
                inner.history_index = Some(inner.input_history.len() - 1);
                inner.input_text = inner.input_history[inner.input_history.len() - 1].clone();
                inner.input_cursor = inner.input_text.len();
            }
            Some(idx) if idx > 0 => {
                // Go to older history
                inner.history_index = Some(idx - 1);
                inner.input_text = inner.input_history[idx - 1].clone();
                inner.input_cursor = inner.input_text.len();
            }
            _ => {} // Already at oldest
        }
    }

    pub fn navigate_history_next(&self) {
        let mut inner = self.write_inner();
        if let Some(idx) = inner.history_index {
            if idx + 1 < inner.input_history.len() {
                // Go to newer history
                inner.history_index = Some(idx + 1);
                inner.input_text = inner.input_history[idx + 1].clone();
                inner.input_cursor = inner.input_text.len();
            } else {
                // Return to current input
                inner.history_index = None;
                inner.input_text = inner.saved_input.clone();
                inner.input_cursor = inner.input_text.len();
            }
        }
        // If history_index is None, we're already at current input - do nothing
    }

    pub fn set_focused_component(&self, component: FocusedComponent) {
        let mut inner = self.write_inner();
        if component != FocusedComponent::Messages {
            inner.message_selection_start = None;
            inner.message_selection_end = None;
        }
        inner.focused_component = component;
    }

    pub fn set_active_modal(&self, modal: Option<ModalType>) {
        self.write_inner().active_modal = modal;
    }

    pub fn set_vim_mode(&self, mode: VimMode) {
        self.write_inner().vim_mode = mode;
    }

    pub fn set_theme_picker_selected(&self, selected: usize) {
        self.write_inner().theme_picker_selected = selected;
    }

    pub fn set_theme_picker_filter(&self, filter: String) {
        self.write_inner().theme_picker_filter = filter;
    }

    pub fn set_theme_before_preview(&self, theme: Option<ThemePreset>) {
        self.write_inner().theme_before_preview = theme;
    }

    pub fn set_sidebar_selected_panel(&self, panel: usize) {
        self.write_inner().sidebar_selected_panel = panel;
    }

    pub fn set_sidebar_selected_item(&self, item: Option<usize>) {
        self.write_inner().sidebar_selected_item = item;
    }

    pub fn set_sidebar_expanded_panels(&self, panels: [bool; 5]) {
        self.write_inner().sidebar_expanded_panels = panels;
    }

    pub fn set_sidebar_scroll_offset(&self, offset: usize) {
        self.write_inner().sidebar_scroll_offset = offset;
    }

    pub fn set_sidebar_panel_scroll(&self, panel: usize, offset: usize) {
        if panel < 5 {
            self.write_inner().sidebar_panel_scrolls[panel] = offset;
        }
    }

    pub fn set_provider_picker_selected(&self, selected: usize) {
        self.write_inner().provider_picker_selected = selected;
    }

    pub fn set_provider_picker_filter(&self, filter: String) {
        self.write_inner().provider_picker_filter = filter;
    }

    pub fn set_model_picker_selected(&self, selected: usize) {
        self.write_inner().model_picker_selected = selected;
    }

    pub fn set_model_picker_filter(&self, filter: String) {
        self.write_inner().model_picker_filter = filter;
    }

    pub fn set_session_picker_selected(&self, selected: usize) {
        self.write_inner().session_picker_selected = selected;
    }

    pub fn set_session_picker_filter(&self, filter: String) {
        self.write_inner().session_picker_filter = filter;
    }

    pub fn set_pending_session_switch(&self, session_id: Option<String>) {
        self.write_inner().pending_session_switch = session_id;
    }

    pub fn set_session_switch_confirm_selected(&self, selected: usize) {
        self.write_inner().session_switch_confirm_selected = selected;
    }

    pub fn set_available_sessions(&self, sessions: Vec<crate::storage::SessionMeta>) {
        self.write_inner().available_sessions = sessions;
    }

    pub fn available_sessions(&self) -> Vec<crate::storage::SessionMeta> {
        self.read_inner().available_sessions.clone()
    }

    pub fn set_file_picker_files(&self, files: Vec<String>) {
        self.write_inner().file_picker_files = files;
    }

    pub fn set_file_picker_filter(&self, filter: String) {
        self.write_inner().file_picker_filter = filter;
    }

    pub fn set_file_picker_selected(&self, selected: usize) {
        self.write_inner().file_picker_selected = selected;
    }

    pub fn set_tools_selected(&self, selected: usize) {
        self.write_inner().tools_selected = selected;
    }

    pub fn set_plugin_selected(&self, selected: usize) {
        self.write_inner().plugin_selected = selected;
    }

    pub fn set_session(&self, session: Option<SessionInfo>) {
        self.write_inner().session = session;
    }

    pub fn set_tasks(&self, tasks: Vec<TaskInfo>) {
        self.write_inner().tasks = tasks;
    }

    pub fn set_git_changes(&self, changes: Vec<GitChangeInfo>) {
        self.write_inner().git_changes = changes;
    }

    pub fn set_tokens(&self, used: usize, total: usize) {
        let mut inner = self.write_inner();
        inner.tokens_used = used;
        inner.tokens_total = total;
    }

    /// Get the detailed context breakdown
    pub fn context_breakdown(&self) -> ContextBreakdown {
        self.read_inner().context_breakdown.clone()
    }

    /// Set the context breakdown (also updates tokens_used and tokens_total)
    pub fn set_context_breakdown(&self, breakdown: ContextBreakdown) {
        let mut inner = self.write_inner();
        inner.tokens_used = breakdown.total;
        inner.tokens_total = breakdown.max_tokens;
        inner.context_breakdown = breakdown;
    }

    pub fn set_error_notification(&self, notification: Option<ErrorNotification>) {
        self.write_inner().error_notification = notification;
    }

    pub fn notify_error(&self, message: String, level: ErrorLevel) {
        let notification = ErrorNotification {
            message,
            level,
            timestamp: chrono::Local::now(),
        };
        self.write_inner().error_notification = Some(notification);
    }

    pub fn clear_error_notification(&self) {
        self.write_inner().error_notification = None;
    }

    pub fn add_attachment(&self, attachment: AttachmentInfo) {
        self.write_inner().attachments.push(attachment);
    }

    pub fn remove_attachment(&self, path: &str) {
        self.write_inner().attachments.retain(|a| a.path != path);
    }

    pub fn clear_attachments(&self) {
        self.write_inner().attachments.clear();
    }

    pub fn attachment_tokens(&self) -> Vec<AttachmentToken> {
        self.read_inner().attachment_tokens.clone()
    }

    pub fn add_attachment_token(&self, token: AttachmentToken) {
        self.write_inner().attachment_tokens.push(token);
    }

    pub fn remove_attachment_token(&self, token: &str) -> Option<AttachmentToken> {
        let mut inner = self.write_inner();
        if let Some(pos) = inner
            .attachment_tokens
            .iter()
            .position(|t| t.token == token)
        {
            return Some(inner.attachment_tokens.remove(pos));
        }
        None
    }

    pub fn clear_attachment_tokens(&self) {
        self.write_inner().attachment_tokens.clear();
    }

    pub fn set_attachment_dropdown_visible(&self, visible: bool) {
        self.write_inner().attachment_dropdown_visible = visible;
    }

    pub fn set_active_questionnaire(&self, questionnaire: Option<QuestionnaireState>) {
        self.write_inner().active_questionnaire = questionnaire;
    }

    pub fn answer_questionnaire(&self, _answer: serde_json::Value) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.mark_answered();
        }
    }

    /// Cancel/skip the questionnaire without answering
    pub fn cancel_questionnaire(&self) {
        // Clear the questionnaire - user can answer via chat instead
        self.write_inner().active_questionnaire = None;
    }

    pub fn questionnaire_focus_prev(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.focus_prev();
        }
    }

    pub fn questionnaire_focus_next(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.focus_next();
        }
    }

    pub fn questionnaire_toggle_focused(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.toggle_focused();
        }
    }

    pub fn questionnaire_insert_char(&self, ch: char) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.insert_char(ch);
        }
    }

    pub fn questionnaire_backspace(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.backspace();
        }
    }

    /// Start editing in questionnaire (called when user presses Enter)
    /// For FreeText questions: starts free text edit mode
    /// For choice questions with "Other" selected: starts "Other" text edit mode
    pub fn questionnaire_start_edit(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            use crate::ui_backend::questionnaire::QuestionType;
            if q.question_type == QuestionType::FreeText {
                q.start_editing_free_text();
            } else if q.is_focused_on_other() && q.other_selected {
                q.start_editing_other_text();
            }
        }
    }

    /// Stop editing in questionnaire (called when user presses Escape while editing)
    /// For FreeText questions: stops free text edit mode
    /// For choice questions: stops "Other" text edit mode
    pub fn questionnaire_stop_edit(&self) {
        if let Some(ref mut q) = self.write_inner().active_questionnaire {
            q.stop_editing_free_text();
            q.stop_editing_other_text();
        }
    }

    pub fn set_pending_approval(&self, approval: Option<ApprovalCardState>) {
        self.write_inner().pending_approval = approval;
    }

    pub fn approval_select_prev(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            // Navigate actions (always available)
            approval.select_prev_action();
        }
    }

    pub fn approval_select_next(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            // Navigate actions (always available)
            approval.select_next_action();
        }
    }

    /// Navigate to previous pattern (for pattern selection)
    pub fn approval_select_prev_pattern(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            approval.select_prev_pattern();
        }
    }

    /// Navigate to next pattern (for pattern selection)
    pub fn approval_select_next_pattern(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            approval.select_next_pattern();
        }
    }

    pub fn approve_operation(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            approval.approve();
        }
    }

    pub fn reject_operation(&self) {
        if let Some(ref mut approval) = self.write_inner().pending_approval {
            approval.reject();
        }
    }

    pub fn set_rate_limit(&self, retry_at: std::time::Instant, pending_message: Option<String>) {
        let mut inner = self.write_inner();
        inner.rate_limit_retry_at = Some(retry_at);
        inner.rate_limit_pending_message = pending_message;
    }

    pub fn clear_rate_limit(&self) {
        let mut inner = self.write_inner();
        inner.rate_limit_retry_at = None;
        inner.rate_limit_pending_message = None;
    }

    pub fn check_rate_limit_expired(&self) -> Option<String> {
        let mut inner = self.write_inner();
        if let Some(retry_at) = inner.rate_limit_retry_at {
            if retry_at <= std::time::Instant::now() {
                // Rate limit expired, get pending message and clear
                let pending = inner.rate_limit_pending_message.clone();
                inner.rate_limit_retry_at = None;
                inner.rate_limit_pending_message = None;
                return pending;
            }
        }
        None
    }

    /// Get pending mode switch (queued during LLM streaming)
    pub fn pending_mode_switch(&self) -> Option<AgentMode> {
        self.read_inner().pending_mode_switch
    }

    /// Set pending mode switch (to be applied after streaming completes)
    pub fn set_pending_mode_switch(&self, mode: Option<AgentMode>) {
        self.write_inner().pending_mode_switch = mode;
    }

    /// Take and clear pending mode switch (returns the mode if one was pending)
    pub fn take_pending_mode_switch(&self) -> Option<AgentMode> {
        self.write_inner().pending_mode_switch.take()
    }

    pub fn set_pending_provider(&self, provider: Option<String>) {
        self.write_inner().pending_provider = provider;
    }

    pub fn take_pending_provider(&self) -> Option<String> {
        self.write_inner().pending_provider.take()
    }

    pub fn set_pending_model(&self, model: Option<String>) {
        self.write_inner().pending_model = model;
    }

    pub fn take_pending_model(&self) -> Option<String> {
        self.write_inner().pending_model.take()
    }

    pub fn set_pending_thinking_tool_enabled(&self, enabled: Option<bool>) {
        self.write_inner().pending_thinking_tool_enabled = enabled;
    }

    pub fn take_pending_thinking_tool_enabled(&self) -> Option<bool> {
        self.write_inner().pending_thinking_tool_enabled.take()
    }

    pub fn set_pending_thinking_enabled(&self, enabled: Option<bool>) {
        self.write_inner().pending_thinking_enabled = enabled;
    }

    pub fn take_pending_thinking_enabled(&self) -> Option<bool> {
        self.write_inner().pending_thinking_enabled.take()
    }

    pub fn set_streaming_content(&self, content: Option<String>) {
        self.write_inner().streaming_content = content;
    }

    pub fn append_streaming_content(&self, chunk: &str) {
        let mut inner = self.write_inner();
        if let Some(ref mut content) = inner.streaming_content {
            content.push_str(chunk);
        } else {
            inner.streaming_content = Some(chunk.to_string());
        }
        inner.last_activity_at = Instant::now();
    }

    pub fn set_streaming_thinking(&self, thinking: Option<String>) {
        self.write_inner().streaming_thinking = thinking;
    }

    pub fn append_streaming_thinking(&self, chunk: &str) {
        let mut inner = self.write_inner();
        if let Some(ref mut thinking) = inner.streaming_thinking {
            thinking.push_str(chunk);
        } else {
            inner.streaming_thinking = Some(chunk.to_string());
        }
        inner.last_activity_at = Instant::now();
    }

    pub fn clear_streaming(&self) {
        let mut inner = self.write_inner();
        inner.streaming_content = None;
        inner.streaming_thinking = None;
    }

    #[cfg(test)]
    pub fn set_last_activity_at(&self, instant: Instant) {
        self.write_inner().last_activity_at = instant;
    }

    pub fn queue_message(&self, msg: String) {
        self.write_inner().message_queue.push(msg);
    }

    pub fn pop_queued_message(&self) -> Option<String> {
        let mut inner = self.write_inner();
        if inner.message_queue.is_empty() {
            None
        } else {
            Some(inner.message_queue.remove(0))
        }
    }

    pub fn queued_message_count(&self) -> usize {
        self.read_inner().message_queue.len()
    }

    /// Get a copy of all queued messages (for display in sidebar)
    pub fn queued_messages(&self) -> Vec<String> {
        self.read_inner().message_queue.clone()
    }

    /// Clear all queued messages (used on cancel/abort)
    pub fn clear_message_queue(&self) -> usize {
        let mut inner = self.write_inner();
        let count = inner.message_queue.len();
        inner.message_queue.clear();
        count
    }

    /// Edit a queued message at the given index
    pub fn edit_queued_message(&self, index: usize, new_content: String) -> bool {
        let mut inner = self.write_inner();
        if index < inner.message_queue.len() {
            inner.message_queue[index] = new_content;
            true
        } else {
            false
        }
    }

    /// Delete a queued message at the given index
    pub fn delete_queued_message(&self, index: usize) -> Option<String> {
        let mut inner = self.write_inner();
        if index < inner.message_queue.len() {
            Some(inner.message_queue.remove(index))
        } else {
            None
        }
    }

    /// Move a queued message from one index to another (reorder)
    pub fn move_queued_message(&self, from: usize, to: usize) -> bool {
        let mut inner = self.write_inner();
        let len = inner.message_queue.len();
        if from >= len || to >= len || from == to {
            return false;
        }
        let item = inner.message_queue.remove(from);
        inner.message_queue.insert(to, item);
        true
    }

    /// Start editing a task at the given index
    pub fn start_task_edit(&self, index: usize) -> bool {
        let mut inner = self.write_inner();
        if index < inner.message_queue.len() {
            inner.editing_task_content = inner.message_queue[index].clone();
            inner.editing_task_index = Some(index);
            true
        } else {
            false
        }
    }

    /// Get the index of the task being edited
    pub fn editing_task_index(&self) -> Option<usize> {
        self.read_inner().editing_task_index
    }

    /// Get the content of the task being edited
    pub fn editing_task_content(&self) -> String {
        self.read_inner().editing_task_content.clone()
    }

    /// Update the editing task content
    pub fn set_editing_task_content(&self, content: String) {
        self.write_inner().editing_task_content = content;
    }

    /// Confirm the edit and apply changes to the queue
    pub fn confirm_task_edit(&self) -> bool {
        let mut inner = self.write_inner();
        if let Some(index) = inner.editing_task_index {
            if index < inner.message_queue.len() && !inner.editing_task_content.trim().is_empty() {
                inner.message_queue[index] = inner.editing_task_content.clone();
                inner.editing_task_index = None;
                inner.editing_task_content.clear();
                return true;
            }
        }
        inner.editing_task_index = None;
        inner.editing_task_content.clear();
        false
    }

    /// Cancel the current task edit
    pub fn cancel_task_edit(&self) {
        let mut inner = self.write_inner();
        inner.editing_task_index = None;
        inner.editing_task_content.clear();
    }

    /// Set the task pending deletion (for confirmation dialog)
    pub fn set_pending_delete_task(&self, index: Option<usize>) {
        self.write_inner().pending_delete_task_index = index;
    }

    /// Get the task pending deletion
    pub fn pending_delete_task_index(&self) -> Option<usize> {
        self.read_inner().pending_delete_task_index
    }

    /// Confirm deletion of the pending task
    pub fn confirm_task_delete(&self) -> Option<String> {
        let mut inner = self.write_inner();
        if let Some(index) = inner.pending_delete_task_index.take() {
            if index < inner.message_queue.len() {
                return Some(inner.message_queue.remove(index));
            }
        }
        None
    }

    /// Check if currently editing a task
    pub fn is_editing_task(&self) -> bool {
        self.read_inner().editing_task_index.is_some()
    }

    // ========== Task Drag-to-Reorder ==========

    /// Start dragging a task for reordering
    pub fn start_task_drag(&self, index: usize) {
        let mut inner = self.write_inner();
        inner.dragging_task_index = Some(index);
        inner.drag_target_index = Some(index);
    }

    /// Update the drag target position
    pub fn update_drag_target(&self, target_index: usize) {
        self.write_inner().drag_target_index = Some(target_index);
    }

    /// Get the index of the task being dragged
    pub fn dragging_task_index(&self) -> Option<usize> {
        self.read_inner().dragging_task_index
    }

    /// Get the current drag target position
    pub fn drag_target_index(&self) -> Option<usize> {
        self.read_inner().drag_target_index
    }

    /// Complete the drag operation and reorder the task
    pub fn complete_task_drag(&self) -> bool {
        let mut inner = self.write_inner();
        if let (Some(from), Some(to)) = (inner.dragging_task_index, inner.drag_target_index) {
            inner.dragging_task_index = None;
            inner.drag_target_index = None;
            if from != to && from < inner.message_queue.len() && to < inner.message_queue.len() {
                let item = inner.message_queue.remove(from);
                inner.message_queue.insert(to, item);
                return true;
            }
        }
        inner.dragging_task_index = None;
        inner.drag_target_index = None;
        false
    }

    /// Cancel the drag operation
    pub fn cancel_task_drag(&self) {
        let mut inner = self.write_inner();
        inner.dragging_task_index = None;
        inner.drag_target_index = None;
    }

    /// Check if currently dragging a task
    pub fn is_dragging_task(&self) -> bool {
        self.read_inner().dragging_task_index.is_some()
    }

    // ========== Command Autocomplete ==========
    const AUTOCOMPLETE_VISIBLE_COUNT: usize = 8;

    /// Check if autocomplete is active
    pub fn autocomplete_active(&self) -> bool {
        self.read_inner().autocomplete_active
    }

    /// Get autocomplete filter
    pub fn autocomplete_filter(&self) -> String {
        self.read_inner().autocomplete_filter.clone()
    }

    /// Get autocomplete selected index
    pub fn autocomplete_selected(&self) -> usize {
        self.read_inner().autocomplete_selected
    }

    /// Get autocomplete scroll offset
    pub fn autocomplete_scroll_offset(&self) -> usize {
        self.read_inner().autocomplete_scroll_offset
    }

    /// Activate autocomplete with initial filter
    pub fn activate_autocomplete(&self, filter: &str) {
        let mut inner = self.write_inner();
        inner.autocomplete_active = true;
        inner.autocomplete_filter = filter.to_string();
        inner.autocomplete_selected = 0;
        inner.autocomplete_scroll_offset = 0;
    }

    /// Deactivate autocomplete
    pub fn deactivate_autocomplete(&self) {
        let mut inner = self.write_inner();
        inner.autocomplete_active = false;
        inner.autocomplete_filter.clear();
        inner.autocomplete_selected = 0;
        inner.autocomplete_scroll_offset = 0;
    }

    /// Update autocomplete filter
    pub fn update_autocomplete_filter(&self, filter: &str) {
        let mut inner = self.write_inner();
        inner.autocomplete_filter = filter.to_string();
        inner.autocomplete_scroll_offset = 0;
    }

    /// Set autocomplete selected index
    pub fn set_autocomplete_selected(&self, index: usize) {
        self.write_inner().autocomplete_selected = index;
    }

    /// Move autocomplete selection up
    pub fn autocomplete_move_up(&self) {
        let mut inner = self.write_inner();
        if inner.autocomplete_selected > 0 {
            inner.autocomplete_selected -= 1;
            if inner.autocomplete_selected < inner.autocomplete_scroll_offset {
                inner.autocomplete_scroll_offset = inner.autocomplete_selected;
            }
        }
    }

    /// Move autocomplete selection down (bounded by max_items)
    pub fn autocomplete_move_down(&self, max_items: usize) {
        let mut inner = self.write_inner();
        if inner.autocomplete_selected + 1 < max_items {
            inner.autocomplete_selected += 1;
            if inner.autocomplete_selected
                >= inner.autocomplete_scroll_offset + Self::AUTOCOMPLETE_VISIBLE_COUNT
            {
                inner.autocomplete_scroll_offset = inner
                    .autocomplete_selected
                    .saturating_sub(Self::AUTOCOMPLETE_VISIBLE_COUNT - 1);
            }
        }
    }

    // ========== Session Cost Tracking ==========

    /// Get total session cost
    pub fn session_cost_total(&self) -> f64 {
        self.read_inner().session_cost_total
    }

    /// Get per-model cost breakdown
    pub fn session_cost_by_model(&self) -> Vec<(String, f64)> {
        self.read_inner().session_cost_by_model.clone()
    }

    pub fn session_tokens_total(&self) -> usize {
        self.read_inner().session_tokens_total
    }

    pub fn session_tokens_by_model(&self) -> Vec<(String, usize)> {
        self.read_inner().session_tokens_by_model.clone()
    }

    /// Set total session cost (used when loading session)
    pub fn set_session_cost_total(&self, cost: f64) {
        self.write_inner().session_cost_total = cost;
    }

    /// Set total session tokens (used when loading session)
    pub fn set_session_tokens_total(&self, tokens: usize) {
        self.write_inner().session_tokens_total = tokens;
    }

    /// Set per-model cost breakdown (used when loading session)
    pub fn set_session_cost_by_model(&self, costs: Vec<(String, f64)>) {
        self.write_inner().session_cost_by_model = costs;
    }

    /// Set per-model tokens breakdown (used when loading session)
    pub fn set_session_tokens_by_model(&self, tokens: Vec<(String, usize)>) {
        self.write_inner().session_tokens_by_model = tokens;
    }

    /// Add cost for a model and update totals
    pub fn add_session_cost(&self, model_name: &str, cost: f64) {
        let mut inner = self.write_inner();
        inner.session_cost_total += cost;

        if let Some(entry) = inner
            .session_cost_by_model
            .iter_mut()
            .find(|(name, _)| name == model_name)
        {
            entry.1 += cost;
        } else {
            inner
                .session_cost_by_model
                .push((model_name.to_string(), cost));
        }
    }

    pub fn add_session_tokens(&self, model_name: &str, tokens: usize) {
        let mut inner = self.write_inner();
        inner.session_tokens_total += tokens;

        if let Some(entry) = inner
            .session_tokens_by_model
            .iter_mut()
            .find(|(name, _)| name == model_name)
        {
            entry.1 += tokens;
        } else {
            inner
                .session_tokens_by_model
                .push((model_name.to_string(), tokens));
        }
    }
}

fn is_archive_marker_message(message: &Message) -> bool {
    message
        .tool_args
        .as_ref()
        .and_then(|args| args.get("kind"))
        .and_then(|value| value.as_str())
        == Some("archive_marker")
}

fn archive_marker_message(info: &ArchiveChunkInfo) -> Message {
    Message {
        role: MessageRole::System,
        content: String::new(),
        thinking: None,
        collapsed: false,
        timestamp: String::new(),
        provider: None,
        model: None,
        context_transient: true,
        tool_calls: Vec::new(),
        segments: Vec::new(),
        tool_args: Some(json!({
            "kind": "archive_marker",
            "filename": info.filename,
            "sequence": info.sequence,
            "created_at": info.created_at,
            "message_count": info.message_count,
        })),
    }
}

fn focused_message_char_len(inner: &StateInner) -> usize {
    focused_message_content(inner)
        .map(|content| content.chars().count())
        .unwrap_or(0)
}

fn focused_message_content(inner: &StateInner) -> Option<&str> {
    inner
        .messages
        .get(inner.focused_message)
        .map(|msg| msg.content.as_str())
}

fn update_archive_marker(inner: &mut StateInner) {
    if inner.archive_chunks.is_empty() {
        if inner
            .messages
            .first()
            .is_some_and(is_archive_marker_message)
        {
            inner.messages.remove(0);
            if inner.focused_message > 0 {
                inner.focused_message = inner.focused_message.saturating_sub(1);
            }
        }
        return;
    }

    let next = inner.archive_chunks[0].clone();
    if let Some(first) = inner.messages.first_mut() {
        if is_archive_marker_message(first) {
            *first = archive_marker_message(&next);
            return;
        }
    }

    inner.messages.insert(0, archive_marker_message(&next));
    inner.focused_message = inner.focused_message.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_message(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            thinking: None,
            context_transient: false,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "00:00:00".to_string(),
            provider: None,
            model: None,
            tool_args: None,
        }
    }

    #[test]
    fn test_history_navigation_prev() {
        let state = SharedState::new();

        // Add some history
        state.add_to_history("first".to_string());
        state.add_to_history("second".to_string());
        state.add_to_history("third".to_string());

        // Set current input
        state.set_input_text("current".to_string());

        // Navigate to previous (most recent)
        state.navigate_history_prev();
        assert_eq!(state.input_text(), "third");
        assert_eq!(state.history_index(), Some(2));

        // Navigate to older
        state.navigate_history_prev();
        assert_eq!(state.input_text(), "second");
        assert_eq!(state.history_index(), Some(1));

        // Navigate to oldest
        state.navigate_history_prev();
        assert_eq!(state.input_text(), "first");
        assert_eq!(state.history_index(), Some(0));

        // Can't go further back
        state.navigate_history_prev();
        assert_eq!(state.input_text(), "first");
        assert_eq!(state.history_index(), Some(0));
    }

    #[test]
    fn test_history_navigation_next() {
        let state = SharedState::new();

        // Add some history
        state.add_to_history("first".to_string());
        state.add_to_history("second".to_string());
        state.add_to_history("third".to_string());

        // Set current input
        state.set_input_text("current".to_string());

        // Navigate back
        state.navigate_history_prev();
        state.navigate_history_prev();

        // Now at "second", navigate forward
        state.navigate_history_next();
        assert_eq!(state.input_text(), "third");
        assert_eq!(state.history_index(), Some(2));

        // Navigate to current input
        state.navigate_history_next();
        assert_eq!(state.input_text(), "current");
        assert_eq!(state.history_index(), None);

        // Can't go further forward
        state.navigate_history_next();
        assert_eq!(state.input_text(), "current");
        assert_eq!(state.history_index(), None);
    }

    #[test]
    fn test_context_files() {
        let state = SharedState::new();

        // Add context files
        use super::super::types::ContextFile;
        let file1 = ContextFile {
            path: "file1.rs".to_string(),
            size: 100,
            token_count: 25,
            added_at: "12:00:00".to_string(),
        };
        let file2 = ContextFile {
            path: "file2.rs".to_string(),
            size: 200,
            token_count: 50,
            added_at: "12:01:00".to_string(),
        };

        state.add_context_file(file1.clone());
        state.add_context_file(file2.clone());

        let files = state.context_files();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "file1.rs");
        assert_eq!(files[1].path, "file2.rs");

        // Remove a file
        state.remove_context_file("file1.rs");
        let files = state.context_files();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "file2.rs");
    }

    #[test]
    fn test_autocomplete_scroll_offset_updates() {
        let state = SharedState::new();
        state.activate_autocomplete("");

        for _ in 0..8 {
            state.autocomplete_move_down(20);
        }

        assert_eq!(state.autocomplete_selected(), 8);
        assert_eq!(state.autocomplete_scroll_offset(), 1);

        state.autocomplete_move_up();
        assert_eq!(state.autocomplete_selected(), 7);
        assert_eq!(state.autocomplete_scroll_offset(), 1);
    }

    #[test]
    fn test_set_messages_clamps_focus_and_clears_sub_index() {
        let state = SharedState::new();
        state.set_focused_message(5);
        state.set_focused_sub_index(Some(2));

        state.set_messages(vec![
            test_message(MessageRole::User, "a"),
            test_message(MessageRole::Assistant, "b"),
        ]);

        assert_eq!(state.focused_message(), 1);
        assert_eq!(state.focused_sub_index(), None);

        state.set_focused_message(0);
        state.set_focused_sub_index(Some(1));
        state.set_messages(vec![test_message(MessageRole::User, "c")]);

        assert_eq!(state.focused_message(), 0);
        assert_eq!(state.focused_sub_index(), None);

        state.set_focused_message(1);
        state.set_focused_sub_index(Some(3));
        state.set_messages(vec![
            test_message(MessageRole::Tool, "tool-1"),
            test_message(MessageRole::Tool, "tool-2"),
        ]);

        assert_eq!(state.focused_message(), 1);
        assert_eq!(state.focused_sub_index(), None);
    }

    #[test]
    fn test_message_cursor_resets_on_focus_change() {
        let state = SharedState::new();
        state.set_messages(vec![
            test_message(MessageRole::User, "hello"),
            test_message(MessageRole::Assistant, "world"),
        ]);
        state.set_message_cursor(4);
        state.set_message_selection(1, 3);

        state.set_focused_message(1);

        assert_eq!(state.message_cursor(), 0);
        assert_eq!(state.message_selection(), None);
    }

    #[test]
    fn test_message_selection_clamps_to_content_len() {
        let state = SharedState::new();
        state.set_messages(vec![test_message(MessageRole::User, "short")]);
        state.set_message_selection(2, 99);

        assert_eq!(state.message_selection(), Some((2, 5)));
    }

    #[test]
    fn test_snap_focused_message_to_viewport() {
        let state = SharedState::new();
        let messages = (0..10)
            .map(|idx| test_message(MessageRole::User, &format!("msg-{idx}")))
            .collect();
        state.set_messages(messages);
        state.set_messages_metrics(100, 10);

        state.set_focused_message(0);
        state.set_messages_scroll_offset(usize::MAX);
        state.snap_focused_message_to_viewport();
        assert_eq!(state.focused_message(), 9);

        state.set_focused_message(0);
        state.set_messages_scroll_offset(40);
        state.snap_focused_message_to_viewport();
        assert_eq!(state.focused_message(), 4);
    }

    #[test]
    fn test_agent_mode_build_mode() {
        let state = SharedState::new();

        // Default is Build mode
        assert_eq!(state.agent_mode(), AgentMode::Build);

        // Change to Plan mode
        state.set_agent_mode(AgentMode::Plan);
        assert_eq!(state.agent_mode(), AgentMode::Plan);

        // Change to Ask mode
        state.set_agent_mode(AgentMode::Ask);
        assert_eq!(state.agent_mode(), AgentMode::Ask);

        // Test build mode
        assert_eq!(state.build_mode(), BuildMode::Balanced);

        state.set_build_mode(BuildMode::Manual);
        assert_eq!(state.build_mode(), BuildMode::Manual);

        state.set_build_mode(BuildMode::Careful);
        assert_eq!(state.build_mode(), BuildMode::Careful);
    }

    #[test]
    fn test_thinking_toggle() {
        let state = SharedState::new();

        // Default is disabled
        assert!(!state.thinking_enabled());

        // Enable thinking
        state.set_thinking_enabled(true);
        assert!(state.thinking_enabled());

        // Disable thinking
        state.set_thinking_enabled(false);
        assert!(!state.thinking_enabled());
    }

    #[test]
    fn test_pending_thinking_toggle() {
        let state = SharedState::new();
        assert!(state.take_pending_thinking_enabled().is_none());

        state.set_pending_thinking_enabled(Some(true));
        assert_eq!(state.take_pending_thinking_enabled(), Some(true));

        state.set_pending_thinking_enabled(Some(false));
        assert_eq!(state.take_pending_thinking_enabled(), Some(false));
        assert!(state.take_pending_thinking_enabled().is_none());
    }

    #[test]
    fn test_collapse_all_thinking_except_last() {
        let state = SharedState::new();
        let mut msg1 = test_message(MessageRole::Thinking, "thought-1");
        msg1.collapsed = false;
        let mut msg2 = test_message(MessageRole::Thinking, "thought-2");
        msg2.collapsed = false;
        let msg3 = test_message(MessageRole::User, "user");

        state.set_messages(vec![msg1, msg3, msg2]);
        state.collapse_all_thinking_except_last();

        let messages = state.messages();
        let thinking_states: Vec<bool> = messages
            .iter()
            .filter(|msg| msg.role == MessageRole::Thinking)
            .map(|msg| msg.collapsed)
            .collect();

        assert_eq!(thinking_states, vec![true, false]);
    }

    #[test]
    fn test_message_queue_fifo_order() {
        let state = SharedState::new();

        // Queue should be empty initially
        assert_eq!(state.queued_message_count(), 0);
        assert!(state.pop_queued_message().is_none());

        // Queue messages in order
        state.queue_message("first".to_string());
        state.queue_message("second".to_string());
        state.queue_message("third".to_string());

        // Verify count
        assert_eq!(state.queued_message_count(), 3);

        // Pop should return in FIFO order
        assert_eq!(state.pop_queued_message(), Some("first".to_string()));
        assert_eq!(state.queued_message_count(), 2);

        assert_eq!(state.pop_queued_message(), Some("second".to_string()));
        assert_eq!(state.queued_message_count(), 1);

        assert_eq!(state.pop_queued_message(), Some("third".to_string()));
        assert_eq!(state.queued_message_count(), 0);

        // Queue is now empty
        assert!(state.pop_queued_message().is_none());
    }

    #[test]
    fn test_message_queue_clear() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("msg1".to_string());
        state.queue_message("msg2".to_string());
        state.queue_message("msg3".to_string());
        assert_eq!(state.queued_message_count(), 3);

        // Clear should return count and empty the queue
        let cleared = state.clear_message_queue();
        assert_eq!(cleared, 3);
        assert_eq!(state.queued_message_count(), 0);
        assert!(state.pop_queued_message().is_none());
    }

    #[test]
    fn test_llm_processing_state() {
        let state = SharedState::new();

        // Default is not processing
        assert!(!state.llm_processing());

        // Set processing
        state.set_llm_processing(true);
        assert!(state.llm_processing());

        // Clear processing
        state.set_llm_processing(false);
        assert!(!state.llm_processing());
    }

    #[test]
    fn test_message_queue_with_llm_processing() {
        // This test verifies the scenario where messages should be queued
        // when LLM is processing
        let state = SharedState::new();

        // Simulate LLM starting to process
        state.set_llm_processing(true);
        assert!(state.llm_processing());

        // Queue messages while processing (simulating user typing fast)
        state.queue_message("queued_1".to_string());
        state.queue_message("queued_2".to_string());
        state.queue_message("queued_3".to_string());
        assert_eq!(state.queued_message_count(), 3);

        // Simulate LLM completing
        state.set_llm_processing(false);

        // Process queued messages in order (simulating controller behavior)
        // IMPORTANT: Only ONE pop per LLM completion cycle
        let next = state.pop_queued_message();
        assert_eq!(next, Some("queued_1".to_string()));
        assert_eq!(state.queued_message_count(), 2);

        // Simulate sending queued_1 and completing
        state.set_llm_processing(true);
        // ... LLM processes queued_1 ...
        state.set_llm_processing(false);

        // Pop next message
        let next = state.pop_queued_message();
        assert_eq!(next, Some("queued_2".to_string()));
        assert_eq!(state.queued_message_count(), 1);

        // Continue until empty
        state.set_llm_processing(true);
        state.set_llm_processing(false);
        let next = state.pop_queued_message();
        assert_eq!(next, Some("queued_3".to_string()));
        assert_eq!(state.queued_message_count(), 0);

        // Queue is empty
        assert!(state.pop_queued_message().is_none());
    }

    #[test]
    fn test_remove_system_messages_containing() {
        let state = SharedState::new();

        // Add various messages
        state.add_message(Message {
            role: MessageRole::User,
            content: "Hello".to_string(),
            thinking: None,
            context_transient: false,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "12:00:00".to_string(),
            provider: None,
            model: None,
            tool_args: None,
        });
        state.add_message(Message {
            role: MessageRole::System,
            content: "⏳ Message queued (1 in queue)".to_string(),
            thinking: None,
            context_transient: true,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "12:00:01".to_string(),
            provider: None,
            model: None,
            tool_args: None,
        });
        state.add_message(Message {
            role: MessageRole::System,
            content: "⏳ Message queued (2 in queue)".to_string(),
            thinking: None,
            context_transient: true,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "12:00:02".to_string(),
            provider: None,
            model: None,
            tool_args: None,
        });
        state.add_message(Message {
            role: MessageRole::Assistant,
            content: "Response".to_string(),
            thinking: None,
            context_transient: false,
            tool_calls: Vec::new(),
            segments: Vec::new(),
            collapsed: false,
            timestamp: "12:00:03".to_string(),
            provider: None,
            model: None,
            tool_args: None,
        });

        assert_eq!(state.messages().len(), 4);

        // Remove system messages containing "Message queued"
        state.remove_system_messages_containing("Message queued");

        // Should only have user and assistant messages left
        let messages = state.messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[1].role, MessageRole::Assistant);
    }

    #[test]
    fn test_idle_elapsed_reflects_activity() {
        let state = SharedState::new();
        state.set_last_activity_at(Instant::now() - Duration::from_secs(3));
        assert!(
            state.idle_elapsed() >= Duration::from_secs(3),
            "idle_elapsed should reflect last activity timestamp"
        );
    }

    #[test]
    fn test_edit_queued_message() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("original_1".to_string());
        state.queue_message("original_2".to_string());
        state.queue_message("original_3".to_string());

        // Edit message at index 1
        assert!(state.edit_queued_message(1, "edited_2".to_string()));

        // Verify the edit
        let messages = state.queued_messages();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0], "original_1");
        assert_eq!(messages[1], "edited_2");
        assert_eq!(messages[2], "original_3");

        // Edit at invalid index should fail
        assert!(!state.edit_queued_message(10, "should_fail".to_string()));
    }

    #[test]
    fn test_delete_queued_message() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("msg_1".to_string());
        state.queue_message("msg_2".to_string());
        state.queue_message("msg_3".to_string());
        assert_eq!(state.queued_message_count(), 3);

        // Delete message at index 1
        let deleted = state.delete_queued_message(1);
        assert_eq!(deleted, Some("msg_2".to_string()));
        assert_eq!(state.queued_message_count(), 2);

        // Remaining messages should be msg_1 and msg_3
        let messages = state.queued_messages();
        assert_eq!(messages[0], "msg_1");
        assert_eq!(messages[1], "msg_3");

        // Delete at invalid index should return None
        let result = state.delete_queued_message(10);
        assert!(result.is_none());
    }

    #[test]
    fn test_move_queued_message() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("a".to_string());
        state.queue_message("b".to_string());
        state.queue_message("c".to_string());
        state.queue_message("d".to_string());

        // Move message from index 0 to index 2
        assert!(state.move_queued_message(0, 2));
        let messages = state.queued_messages();
        assert_eq!(messages, vec!["b", "c", "a", "d"]);

        // Move message from index 3 to index 1
        assert!(state.move_queued_message(3, 1));
        let messages = state.queued_messages();
        assert_eq!(messages, vec!["b", "d", "c", "a"]);

        // Move to same position should fail (no-op)
        assert!(!state.move_queued_message(1, 1));

        // Move from invalid index should fail
        assert!(!state.move_queued_message(10, 0));

        // Move to invalid index should fail
        assert!(!state.move_queued_message(0, 10));
    }

    #[test]
    fn test_task_edit_workflow() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());
        state.queue_message("task_3".to_string());

        // Initially not editing
        assert!(!state.is_editing_task());
        assert!(state.editing_task_index().is_none());

        // Start editing task at index 1
        assert!(state.start_task_edit(1));
        assert!(state.is_editing_task());
        assert_eq!(state.editing_task_index(), Some(1));
        assert_eq!(state.editing_task_content(), "task_2");

        // Modify the content
        state.set_editing_task_content("task_2_modified".to_string());
        assert_eq!(state.editing_task_content(), "task_2_modified");

        // Confirm the edit
        assert!(state.confirm_task_edit());
        assert!(!state.is_editing_task());

        // Verify the queue was updated
        let messages = state.queued_messages();
        assert_eq!(messages[1], "task_2_modified");
    }

    #[test]
    fn test_task_edit_cancel() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());

        // Start editing task at index 0
        assert!(state.start_task_edit(0));
        state.set_editing_task_content("modified_but_cancelled".to_string());

        // Cancel the edit
        state.cancel_task_edit();
        assert!(!state.is_editing_task());
        assert!(state.editing_task_index().is_none());

        // Queue should be unchanged
        let messages = state.queued_messages();
        assert_eq!(messages[0], "task_1");
    }

    #[test]
    fn test_task_delete_workflow() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());
        state.queue_message("task_3".to_string());

        // Set pending delete
        state.set_pending_delete_task(Some(1));
        assert_eq!(state.pending_delete_task_index(), Some(1));

        // Confirm delete
        let deleted = state.confirm_task_delete();
        assert_eq!(deleted, Some("task_2".to_string()));
        assert!(state.pending_delete_task_index().is_none());

        // Verify queue
        let messages = state.queued_messages();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "task_1");
        assert_eq!(messages[1], "task_3");
    }

    #[test]
    fn test_task_delete_cancel() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());

        // Set pending delete then cancel
        state.set_pending_delete_task(Some(0));
        state.set_pending_delete_task(None);

        // Nothing should be deleted
        assert_eq!(state.queued_message_count(), 2);
    }

    #[test]
    fn test_task_drag_reorder() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_a".to_string());
        state.queue_message("task_b".to_string());
        state.queue_message("task_c".to_string());
        state.queue_message("task_d".to_string());

        // Initially not dragging
        assert!(!state.is_dragging_task());
        assert!(state.dragging_task_index().is_none());
        assert!(state.drag_target_index().is_none());

        // Start drag on task at index 1 (task_b)
        state.start_task_drag(1);
        assert!(state.is_dragging_task());
        assert_eq!(state.dragging_task_index(), Some(1));
        assert_eq!(state.drag_target_index(), Some(1)); // Initial target is same as source

        // Update drag target to index 3
        state.update_drag_target(3);
        assert_eq!(state.drag_target_index(), Some(3));

        // Complete the drag
        assert!(state.complete_task_drag());
        assert!(!state.is_dragging_task());

        // Verify task_b moved from index 1 to index 3
        let messages = state.queued_messages();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0], "task_a");
        assert_eq!(messages[1], "task_c");
        assert_eq!(messages[2], "task_d");
        assert_eq!(messages[3], "task_b");
    }

    #[test]
    fn test_task_drag_cancel() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());

        // Start drag
        state.start_task_drag(0);
        state.update_drag_target(1);
        assert!(state.is_dragging_task());

        // Cancel drag
        state.cancel_task_drag();
        assert!(!state.is_dragging_task());

        // Queue should be unchanged
        let messages = state.queued_messages();
        assert_eq!(messages[0], "task_1");
        assert_eq!(messages[1], "task_2");
    }

    #[test]
    fn test_task_drag_no_movement() {
        let state = SharedState::new();

        // Queue some messages
        state.queue_message("task_1".to_string());
        state.queue_message("task_2".to_string());

        // Start drag and complete without changing target
        state.start_task_drag(0);
        assert_eq!(state.drag_target_index(), Some(0)); // Target equals source

        // Complete should return false (no actual movement)
        assert!(!state.complete_task_drag());

        // Queue should be unchanged
        let messages = state.queued_messages();
        assert_eq!(messages[0], "task_1");
        assert_eq!(messages[1], "task_2");
    }
}
