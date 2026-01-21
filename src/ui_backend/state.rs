//! Shared Application State
//!
//! Thread-safe state that can be safely shared between the backend and frontend.

use std::sync::{Arc, RwLock};

use super::approval::ApprovalCardState;
use super::commands::{AgentMode, BuildMode};
use super::questionnaire::QuestionnaireState;
use super::types::{
    AttachmentInfo, ContextFile, GitChangeInfo, Message, MessageRole, ModelInfo, ProviderInfo,
    SessionInfo, TaskInfo, ThemePreset,
};
use crate::tools::TrustLevel;

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
    pub status_message: Option<String>,

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
    pub sidebar_expanded_panels: [bool; 4],
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            focused_component: FocusedComponent::default(),
            active_modal: None,
            sidebar_visible: true,
            theme: ThemePreset::CatppuccinMocha,
            status_message: None,
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
            sidebar_expanded_panels: [false; 4],
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

    // ========== Messages ==========
    pub messages: Vec<Message>,
    pub focused_message: usize,
    pub messages_scroll_offset: usize,
    pub messages_total_lines: usize,
    pub messages_viewport_height: usize,

    // ========== Active Tools ==========
    /// Currently executing tools (for loading indicators)
    pub active_tools: Vec<crate::ui_backend::types::ActiveToolInfo>,

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

    // ========== UI State ==========
    pub sidebar_visible: bool,
    pub theme: ThemePreset,
    pub status_message: Option<String>,
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

    // ========== File Picker State ==========
    pub file_picker_files: Vec<String>,
    pub file_picker_filter: String,
    pub file_picker_selected: usize,

    // ========== Tools Modal State ==========
    pub tools_selected: usize,

    // ========== Sidebar State ==========
    pub sidebar_selected_panel: usize,
    pub sidebar_selected_item: Option<usize>,
    pub sidebar_expanded_panels: [bool; 4],
    pub sidebar_scroll_offset: usize,
    pub sidebar_panel_scrolls: [usize; 4],

    // ========== Context ==========
    pub context_files: Vec<ContextFile>,
    pub tokens_used: usize,
    pub tokens_total: usize,

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

    // ========== Questionnaire (ask_user) ==========
    pub active_questionnaire: Option<QuestionnaireState>,

    // ========== Approval Cards ==========
    pub pending_approval: Option<ApprovalCardState>,

    // ========== Rate Limiting ==========
    pub rate_limit_retry_at: Option<std::time::Instant>,
    pub rate_limit_pending_message: Option<String>,

    // ========== Message Queue ==========
    pub message_queue: Vec<String>,

    // ========== Command Autocomplete ==========
    pub autocomplete_active: bool,
    pub autocomplete_filter: String,
    pub autocomplete_selected: usize,

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
                llm_connected: false,
                llm_processing: false,
                device_flow_session: None,
                messages: Vec::new(),
                focused_message: 0,
                messages_scroll_offset: 0,
                messages_total_lines: 0,
                messages_viewport_height: 0,
                active_tools: Vec::new(),
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
                sidebar_visible: true,
                theme: ThemePreset::CatppuccinMocha,
                status_message: None,
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
                file_picker_files: Vec::new(),
                file_picker_filter: String::new(),
                file_picker_selected: 0,
                tools_selected: 0,
                sidebar_selected_panel: 0,
                sidebar_selected_item: None,
                sidebar_expanded_panels: [true, true, true, true],
                sidebar_scroll_offset: 0,
                sidebar_panel_scrolls: [0, 0, 0, 0],
                context_files: Vec::new(),
                tokens_used: 0,
                tokens_total: 1_000_000,
                session: None,
                tasks: Vec::new(),
                git_changes: Vec::new(),
                available_providers: Vec::new(),
                available_models: Vec::new(),
                available_sessions: Vec::new(),
                error_notification: None,
                attachments: Vec::new(),
                attachment_dropdown_visible: false,
                active_questionnaire: None,
                pending_approval: None,
                rate_limit_retry_at: None,
                rate_limit_pending_message: None,
                message_queue: Vec::new(),
                autocomplete_active: false,
                autocomplete_filter: String::new(),
                autocomplete_selected: 0,
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
            status_message: inner.status_message.clone(),
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

    pub fn sidebar_expanded_panels(&self) -> [bool; 4] {
        self.read_inner().sidebar_expanded_panels
    }

    pub fn sidebar_scroll_offset(&self) -> usize {
        self.read_inner().sidebar_scroll_offset
    }

    pub fn sidebar_panel_scrolls(&self) -> [usize; 4] {
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

    pub fn error_notification(&self) -> Option<ErrorNotification> {
        self.read_inner().error_notification.clone()
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
        self.write_inner().llm_processing = processing;
    }

    pub fn processing_correlation_id(&self) -> Option<String> {
        self.read_inner().processing_correlation_id.clone()
    }

    pub fn set_processing_correlation_id(&self, id: Option<String>) {
        self.write_inner().processing_correlation_id = id;
    }

    pub fn set_provider(&self, provider: Option<String>) {
        self.write_inner().current_provider = provider;
    }

    pub fn set_model(&self, model: Option<String>) {
        self.write_inner().current_model = model;
    }

    pub fn set_input_text(&self, text: String) {
        self.write_inner().input_text = text;
    }

    pub fn set_input_cursor(&self, cursor: usize) {
        self.write_inner().input_cursor = cursor;
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

    pub fn set_sidebar_visible(&self, visible: bool) {
        self.write_inner().sidebar_visible = visible;
    }

    pub fn set_theme(&self, theme: ThemePreset) {
        self.write_inner().theme = theme;
    }

    pub fn set_status_message(&self, message: Option<String>) {
        self.write_inner().status_message = message;
    }

    pub fn add_message(&self, message: Message) {
        self.write_inner().messages.push(message);
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
        self.write_inner().messages = messages;
    }

    pub fn clear_messages(&self) {
        self.write_inner().messages.clear();
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
    }

    /// Scroll to bottom (most recent message)
    pub fn scroll_to_bottom(&self) {
        let count = self.message_count();
        if count > 0 {
            self.set_messages_scroll_offset(usize::MAX);
            self.set_focused_message(count.saturating_sub(1));
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
        self.write_inner().focused_component = component;
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

    pub fn set_sidebar_expanded_panels(&self, panels: [bool; 4]) {
        self.write_inner().sidebar_expanded_panels = panels;
    }

    pub fn set_sidebar_scroll_offset(&self, offset: usize) {
        self.write_inner().sidebar_scroll_offset = offset;
    }

    pub fn set_sidebar_panel_scroll(&self, panel: usize, offset: usize) {
        if panel < 4 {
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
    }

    pub fn clear_streaming(&self) {
        let mut inner = self.write_inner();
        inner.streaming_content = None;
        inner.streaming_thinking = None;
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

    /// Clear all queued messages (used on cancel/abort)
    pub fn clear_message_queue(&self) -> usize {
        let mut inner = self.write_inner();
        let count = inner.message_queue.len();
        inner.message_queue.clear();
        count
    }

    // ========== Command Autocomplete ==========

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

    /// Activate autocomplete with initial filter
    pub fn activate_autocomplete(&self, filter: &str) {
        let mut inner = self.write_inner();
        inner.autocomplete_active = true;
        inner.autocomplete_filter = filter.to_string();
        inner.autocomplete_selected = 0;
    }

    /// Deactivate autocomplete
    pub fn deactivate_autocomplete(&self) {
        let mut inner = self.write_inner();
        inner.autocomplete_active = false;
        inner.autocomplete_filter.clear();
        inner.autocomplete_selected = 0;
    }

    /// Update autocomplete filter
    pub fn update_autocomplete_filter(&self, filter: &str) {
        self.write_inner().autocomplete_filter = filter.to_string();
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
        }
    }

    /// Move autocomplete selection down (bounded by max_items)
    pub fn autocomplete_move_down(&self, max_items: usize) {
        let mut inner = self.write_inner();
        if inner.autocomplete_selected + 1 < max_items {
            inner.autocomplete_selected += 1;
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
