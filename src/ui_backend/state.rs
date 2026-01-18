//! Shared Application State
//!
//! Thread-safe state that can be safely shared between the backend and frontend.

use std::sync::{Arc, RwLock};

use super::approval::ApprovalCardState;
use super::commands::{AgentMode, BuildMode};
use super::questionnaire::QuestionnaireState;
use super::types::{
    AttachmentInfo, ContextFile, GitChangeInfo, Message, ModelInfo, ProviderInfo, SessionInfo,
    TaskInfo, ThemePreset,
};

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

/// Types of modals that can be displayed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalType {
    ProviderPicker,
    ModelPicker,
    FilePicker,
    ThemePicker,
    Help,
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

    // ========== LLM State ==========
    pub current_provider: Option<String>,
    pub current_model: Option<String>,
    pub llm_connected: bool,
    pub llm_processing: bool,

    // ========== Messages ==========
    pub messages: Vec<Message>,
    pub focused_message: usize,
    pub messages_scroll_offset: usize,

    // ========== Input State ==========
    pub input_text: String,
    pub input_cursor: usize,
    pub input_history: Vec<String>,
    pub history_index: Option<usize>,
    pub saved_input: String,

    // ========== UI State ==========
    pub sidebar_visible: bool,
    pub theme: ThemePreset,
    pub status_message: Option<String>,
    pub focused_component: FocusedComponent,
    pub active_modal: Option<ModalType>,

    // ========== Theme Picker State ==========
    pub theme_picker_selected: usize,
    pub theme_picker_filter: String,
    pub theme_before_preview: Option<ThemePreset>,

    // ========== Provider/Model Picker State ==========
    pub provider_picker_selected: usize,
    pub provider_picker_filter: String,
    pub model_picker_selected: usize,
    pub model_picker_filter: String,

    // ========== Sidebar State ==========
    pub sidebar_selected_panel: usize,
    pub sidebar_selected_item: Option<usize>,
    pub sidebar_expanded_panels: [bool; 4],

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
                current_provider: None,
                current_model: None,
                llm_connected: false,
                llm_processing: false,
                messages: Vec::new(),
                focused_message: 0,
                messages_scroll_offset: 0,
                input_text: String::new(),
                input_cursor: 0,
                input_history: Vec::new(),
                history_index: None,
                saved_input: String::new(),
                sidebar_visible: true,
                theme: ThemePreset::CatppuccinMocha,
                status_message: None,
                focused_component: FocusedComponent::Input,
                active_modal: None,
                theme_picker_selected: 0,
                theme_picker_filter: String::new(),
                theme_before_preview: None,
                provider_picker_selected: 0,
                provider_picker_filter: String::new(),
                model_picker_selected: 0,
                model_picker_filter: String::new(),
                sidebar_selected_panel: 0,
                sidebar_selected_item: None,
                sidebar_expanded_panels: [true, true, true, true],
                context_files: Vec::new(),
                tokens_used: 0,
                tokens_total: 1_000_000,
                session: None,
                tasks: Vec::new(),
                git_changes: Vec::new(),
                available_providers: Vec::new(),
                available_models: Vec::new(),
                error_notification: None,
                attachments: Vec::new(),
                attachment_dropdown_visible: false,
                active_questionnaire: None,
                pending_approval: None,
                rate_limit_retry_at: None,
                rate_limit_pending_message: None,
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

    pub fn set_thinking_enabled(&self, enabled: bool) {
        self.write_inner().thinking_enabled = enabled;
    }

    pub fn set_llm_connected(&self, connected: bool) {
        self.write_inner().llm_connected = connected;
    }

    pub fn set_llm_processing(&self, processing: bool) {
        self.write_inner().llm_processing = processing;
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

    pub fn set_messages_scroll_offset(&self, offset: usize) {
        self.write_inner().messages_scroll_offset = offset;
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

    pub fn set_pending_approval(&self, approval: Option<ApprovalCardState>) {
        self.write_inner().pending_approval = approval;
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
            added_at: "12:00:00".to_string(),
        };
        let file2 = ContextFile {
            path: "file2.rs".to_string(),
            size: 200,
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
