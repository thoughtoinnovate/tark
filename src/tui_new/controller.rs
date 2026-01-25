//! TUI Controller - Orchestrates AppService and TuiRenderer
//!
//! The controller owns both the business logic (AppService) and the UI (TuiRenderer),
//! coordinating between them via Commands and AppEvents.

use anyhow::Result;
use ratatui::backend::Backend;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use uuid::Uuid;

use crate::llm::models_db;
use crate::tools::questionnaire::{
    AnswerValue, ApprovalChoice, ApprovalResponse, InteractionRequest, UserResponse,
};
use crate::tui_new::widgets::FlashBarState;
use crate::ui_backend::approval::ApprovalCardState;
use crate::ui_backend::questionnaire::QuestionnaireState;
use crate::ui_backend::UiRenderer;
use crate::ui_backend::{AppEvent, AppService, Command, SharedState};

use super::modals::{ModalManager, ModalResult};
use super::renderer::TuiRenderer;

/// TUI Controller
///
/// Main event loop coordinator that:
/// 1. Polls input from renderer
/// 2. Processes commands through AppService
/// 3. Handles async events from AppService
/// 4. Renders UI with current state
pub struct TuiController<B: Backend> {
    /// Application service (business logic)
    service: AppService,
    /// UI renderer
    renderer: TuiRenderer<B>,
    /// Event receiver for async AppEvents
    event_rx: mpsc::UnboundedReceiver<AppEvent>,
    /// Interaction receiver for ask_user/approval
    interaction_rx: Option<crate::tools::InteractionReceiver>,
    /// Pending questionnaire responder
    questionnaire_responder:
        std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<UserResponse>>>>,
    /// Pending approval responder
    approval_responder:
        std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<ApprovalResponse>>>>,
    /// Modal manager for delegating modal-specific commands
    modal_manager: ModalManager,
    /// Pending questionnaire data (for multi-question support)
    pending_questionnaire:
        std::sync::Arc<tokio::sync::Mutex<Option<crate::tools::questionnaire::Questionnaire>>>,
    /// Last tick time for flash bar animation
    flash_bar_last_tick: Instant,
}

impl<B: Backend> TuiController<B> {
    /// Create a new TUI controller
    pub fn new(
        service: AppService,
        renderer: TuiRenderer<B>,
        event_rx: mpsc::UnboundedReceiver<AppEvent>,
        interaction_rx: Option<crate::tools::InteractionReceiver>,
    ) -> Self {
        Self {
            service,
            renderer,
            event_rx,
            interaction_rx,
            questionnaire_responder: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            approval_responder: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            modal_manager: ModalManager::new(),
            pending_questionnaire: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            flash_bar_last_tick: Instant::now(),
        }
    }

    /// Run the main event loop
    pub async fn run(&mut self) -> Result<()> {
        // Initialize sidebar data on startup
        self.service.refresh_sidebar_data().await;

        let state = self.service.state().clone();
        self.spawn_interaction_task(state.clone());

        loop {
            // 1. Render current state
            self.renderer.render(&state)?;

            // 2. Poll for user input (non-blocking)
            if let Some(command) = self.renderer.poll_input(&state)? {
                self.handle_command(command).await?;
            }

            // 3. Process async events (non-blocking)
            self.poll_events(&state).await?;

            // 3b. Advance flash bar animation on tick
            self.tick_flash_bar(&state);

            // 4. Check quit condition
            if self.renderer.should_quit(&state) {
                break;
            }

            // Adaptive sleep: shorter during streaming for faster UI updates,
            // longer when idle to reduce CPU usage
            let sleep_duration = if state.llm_processing() {
                Duration::from_millis(2) // Minimal sleep during streaming
            } else {
                Duration::from_millis(10) // Normal idle sleep
            };
            tokio::time::sleep(sleep_duration).await;
        }

        Ok(())
    }

    fn tick_flash_bar(&mut self, state: &SharedState) {
        let tick_rate = Duration::from_millis(50);
        if state.flash_bar_state() == FlashBarState::Working {
            if self.flash_bar_last_tick.elapsed() >= tick_rate {
                state.advance_flash_bar_animation();
                self.flash_bar_last_tick = Instant::now();
            }
        } else {
            self.flash_bar_last_tick = Instant::now();
        }
    }

    fn maybe_handle_rate_limit_message(state: &SharedState, chunk: &str) -> bool {
        if !is_rate_limit_message(chunk) {
            return false;
        }

        let secs = parse_rate_limit_delay_secs(chunk).unwrap_or(1);
        let retry_at = Instant::now() + Duration::from_secs(secs.max(1));
        state.set_rate_limit(retry_at, None);
        true
    }

    fn spawn_interaction_task(&mut self, state: SharedState) {
        let Some(mut interaction_rx) = self.interaction_rx.take() else {
            return;
        };

        let questionnaire_responder = self.questionnaire_responder.clone();
        let approval_responder = self.approval_responder.clone();
        let pending_questionnaire = self.pending_questionnaire.clone();

        tokio::spawn(async move {
            while let Some(request) = interaction_rx.recv().await {
                match request {
                    InteractionRequest::Questionnaire { data, responder } => {
                        let Some(question) = data.questions.first() else {
                            let _ = responder.send(UserResponse::cancelled());
                            continue;
                        };

                        let total_questions = data.questions.len();
                        let title = data.title.clone();

                        let (question_type, options, default_value) = match &question.kind {
                            crate::tools::questionnaire::QuestionType::SingleSelect {
                                options,
                                default,
                            } => (
                                crate::ui_backend::questionnaire::QuestionType::SingleChoice,
                                options
                                    .iter()
                                    .map(|opt| crate::ui_backend::questionnaire::QuestionOption {
                                        text: opt.label.clone(),
                                        value: opt.value.clone(),
                                    })
                                    .collect::<Vec<_>>(),
                                default.clone(),
                            ),
                            crate::tools::questionnaire::QuestionType::MultiSelect {
                                options,
                                ..
                            } => (
                                crate::ui_backend::questionnaire::QuestionType::MultipleChoice,
                                options
                                    .iter()
                                    .map(|opt| crate::ui_backend::questionnaire::QuestionOption {
                                        text: opt.label.clone(),
                                        value: opt.value.clone(),
                                    })
                                    .collect(),
                                None,
                            ),
                            crate::tools::questionnaire::QuestionType::FreeText { .. } => (
                                crate::ui_backend::questionnaire::QuestionType::FreeText,
                                vec![],
                                None,
                            ),
                        };

                        // Create state for first question with multi-question info
                        let mut question_state = QuestionnaireState::new_multi(
                            title,
                            question.id.clone(),
                            question.text.clone(),
                            question_type,
                            options.clone(),
                            0,               // current index
                            total_questions, // total
                        );

                        // Pre-select default option for single choice questions
                        if question_type
                            == crate::ui_backend::questionnaire::QuestionType::SingleChoice
                        {
                            let mut selected_idx = None;
                            // Try to find the default value in options
                            if let Some(ref default_val) = default_value {
                                selected_idx = options.iter().position(|o| &o.value == default_val);
                            }
                            // Fallback to first option if no default matched
                            if selected_idx.is_none() && !options.is_empty() {
                                selected_idx = Some(0);
                            }
                            if let Some(idx) = selected_idx {
                                question_state.selected = vec![idx];
                                question_state.focused_index = idx;
                            }
                        }

                        // Store responder FIRST (before questionnaire is visible)
                        // This prevents a race condition where user submits before responder is stored
                        let mut guard = questionnaire_responder.lock().await;
                        *guard = Some(responder);
                        drop(guard);

                        // Store full questionnaire for multi-question navigation
                        let mut pending_guard = pending_questionnaire.lock().await;
                        *pending_guard = Some(data);
                        drop(pending_guard);

                        // THEN make questionnaire visible (user can now respond safely)
                        state.set_active_questionnaire(Some(question_state));
                    }
                    InteractionRequest::Approval { request, responder } => {
                        let risk_level = match request.risk_level {
                            crate::tools::risk::RiskLevel::ReadOnly => {
                                crate::ui_backend::approval::RiskLevel::Safe
                            }
                            crate::tools::risk::RiskLevel::Write => {
                                crate::ui_backend::approval::RiskLevel::Write
                            }
                            crate::tools::risk::RiskLevel::Risky => {
                                crate::ui_backend::approval::RiskLevel::Risky
                            }
                            crate::tools::risk::RiskLevel::Dangerous => {
                                crate::ui_backend::approval::RiskLevel::Dangerous
                            }
                        };

                        let suggested_patterns: Vec<
                            crate::ui_backend::approval::ApprovalPatternOption,
                        > = request
                            .suggested_patterns
                            .into_iter()
                            .map(
                                |pattern| crate::ui_backend::approval::ApprovalPatternOption {
                                    pattern: pattern.pattern,
                                    match_type: pattern.match_type,
                                    description: pattern.description,
                                },
                            )
                            .collect();
                        let mut approval_state = ApprovalCardState::new(
                            request.tool.clone(),
                            risk_level,
                            format!("Approval required for {}", request.tool),
                            request.command.clone(),
                            Vec::new(),
                            suggested_patterns,
                        );
                        if let Some(idx) = approval_state
                            .suggested_patterns
                            .iter()
                            .position(|p| matches!(p.match_type, crate::tools::MatchType::Exact))
                        {
                            approval_state.selected_pattern = idx;
                        }
                        state.set_pending_approval(Some(approval_state));
                        state.set_active_modal(Some(crate::ui_backend::ModalType::Approval));
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                        let mut guard = approval_responder.lock().await;
                        *guard = Some(responder);
                    }
                }
            }
        });
    }

    async fn send_approval_response(&self, choice: ApprovalChoice) {
        let mut guard = self.approval_responder.lock().await;
        let Some(responder) = guard.take() else {
            return;
        };
        let response = match choice {
            ApprovalChoice::ApproveOnce => ApprovalResponse::approve_once(),
            ApprovalChoice::ApproveSession => {
                if let Some(pattern) = self.selected_approval_pattern().await {
                    ApprovalResponse::approve_session(pattern)
                } else {
                    ApprovalResponse::approve_once()
                }
            }
            ApprovalChoice::ApproveAlways => {
                if let Some(pattern) = self.selected_approval_pattern().await {
                    ApprovalResponse::approve_always(pattern)
                } else {
                    ApprovalResponse::approve_once()
                }
            }
            ApprovalChoice::Deny => ApprovalResponse::deny(),
            ApprovalChoice::DenyAlways => {
                if let Some(pattern) = self.selected_approval_pattern().await {
                    ApprovalResponse::deny_always(pattern)
                } else {
                    ApprovalResponse::deny()
                }
            }
        };
        let _ = responder.send(response);
    }

    async fn selected_approval_pattern(&self) -> Option<crate::tools::ApprovalPattern> {
        let approval = self.service.state().pending_approval()?;
        let option = approval.suggested_patterns.get(approval.selected_pattern)?;
        Some(crate::tools::ApprovalPattern::new(
            approval.operation,
            option.pattern.clone(),
            option.match_type,
        ))
    }

    async fn send_questionnaire_response(&self, state: &SharedState) {
        let Some(questionnaire) = state.active_questionnaire() else {
            return;
        };

        // Build the answer for current question
        let (answer, display_answer) = self.build_answer(&questionnaire);

        // Collect the answered question
        let answered = crate::ui_backend::questionnaire::AnsweredQuestion {
            question_id: questionnaire.question_id.clone(),
            question_text: questionnaire.question.clone(),
            answer_display: display_answer.clone(),
            answer_value: match &answer {
                AnswerValue::Single(v) => serde_json::json!(v),
                AnswerValue::Multi(v) => serde_json::json!(v),
            },
        };

        // Get current state for multi-question tracking
        let current_index = questionnaire.current_question_index;
        let total_questions = questionnaire.total_questions;
        let title = questionnaire.title.clone();
        let mut collected = questionnaire.collected_answers.clone();
        collected.push(answered.clone());

        // Check if there are more questions
        let pending_guard = self.pending_questionnaire.lock().await;
        let has_more = if let Some(ref pending) = *pending_guard {
            current_index + 1 < pending.questions.len()
        } else {
            false
        };

        if has_more {
            // More questions - advance to next question
            let pending = pending_guard.as_ref().unwrap();
            let next_index = current_index + 1;
            let next_question = &pending.questions[next_index];

            let (question_type, options, default_value) = match &next_question.kind {
                crate::tools::questionnaire::QuestionType::SingleSelect { options, default } => (
                    crate::ui_backend::questionnaire::QuestionType::SingleChoice,
                    options
                        .iter()
                        .map(|opt| crate::ui_backend::questionnaire::QuestionOption {
                            text: opt.label.clone(),
                            value: opt.value.clone(),
                        })
                        .collect::<Vec<_>>(),
                    default.clone(),
                ),
                crate::tools::questionnaire::QuestionType::MultiSelect { options, .. } => (
                    crate::ui_backend::questionnaire::QuestionType::MultipleChoice,
                    options
                        .iter()
                        .map(|opt| crate::ui_backend::questionnaire::QuestionOption {
                            text: opt.label.clone(),
                            value: opt.value.clone(),
                        })
                        .collect(),
                    None,
                ),
                crate::tools::questionnaire::QuestionType::FreeText { .. } => (
                    crate::ui_backend::questionnaire::QuestionType::FreeText,
                    vec![],
                    None,
                ),
            };

            // Create state for next question, preserving collected answers
            let mut next_state = QuestionnaireState::new_multi(
                title,
                next_question.id.clone(),
                next_question.text.clone(),
                question_type,
                options.clone(),
                next_index,
                total_questions,
            );
            next_state.collected_answers = collected;

            // Pre-select default option for single choice questions
            if question_type == crate::ui_backend::questionnaire::QuestionType::SingleChoice {
                let mut selected_idx = None;
                // Try to find the default value in options
                if let Some(ref default_val) = default_value {
                    selected_idx = options.iter().position(|o| &o.value == default_val);
                }
                // Fallback to first option if no default matched
                if selected_idx.is_none() && !options.is_empty() {
                    selected_idx = Some(0);
                }
                if let Some(idx) = selected_idx {
                    next_state.selected = vec![idx];
                    next_state.focused_index = idx;
                }
            }

            state.set_active_questionnaire(Some(next_state));

            // Don't release the lock - we still have more questions
            drop(pending_guard);
        } else {
            // Last question - show summary and send all answers
            drop(pending_guard);

            // Build visual feedback showing all answers
            let mut summary_lines = vec![format!(
                "📝 **{}**",
                if title.is_empty() {
                    "Questionnaire"
                } else {
                    &title
                }
            )];
            for ans in &collected {
                summary_lines.push(format!(
                    "**Q:** {}\n**A:** {}",
                    ans.question_text, ans.answer_display
                ));
            }
            let summary = summary_lines.join("\n\n");

            state.add_message(crate::ui_backend::Message {
                role: crate::ui_backend::MessageRole::User,
                content: summary,
                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                provider: None,
                model: None,
                collapsed: false,
                thinking: None,
                context_transient: false,
                tool_calls: Vec::new(),
                segments: Vec::new(),
                tool_args: None,
            });

            // Build response with all answers
            let mut all_answers = std::collections::HashMap::new();
            for ans in &collected {
                let answer_val = match ans.answer_value.clone() {
                    serde_json::Value::Array(arr) => {
                        let values: Vec<String> = arr
                            .into_iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect();
                        AnswerValue::Multi(values)
                    }
                    serde_json::Value::String(s) => AnswerValue::Single(s),
                    other => AnswerValue::Single(other.to_string()),
                };
                all_answers.insert(ans.question_id.clone(), answer_val);
            }
            let response = UserResponse::with_answers(all_answers);

            // Clear the questionnaire state and pending data
            state.set_active_questionnaire(None);
            let mut pending_guard = self.pending_questionnaire.lock().await;
            *pending_guard = None;

            // Send response back to tool
            let mut guard = self.questionnaire_responder.lock().await;
            if let Some(responder) = guard.take() {
                let _ = responder.send(response);
            }
        }
    }

    /// Build answer value and display text from current questionnaire state
    fn build_answer(&self, questionnaire: &QuestionnaireState) -> (AnswerValue, String) {
        match questionnaire.question_type {
            crate::ui_backend::questionnaire::QuestionType::SingleChoice => {
                if questionnaire.other_selected && !questionnaire.other_text.trim().is_empty() {
                    let custom = questionnaire.other_text.trim().to_string();
                    (
                        AnswerValue::Single(format!("other:{}", custom)),
                        format!("Other: {}", custom),
                    )
                } else {
                    let (value, label) = questionnaire
                        .selected
                        .first()
                        .and_then(|idx| {
                            questionnaire
                                .options
                                .get(*idx)
                                .map(|opt| (opt.value.clone(), opt.text.clone()))
                        })
                        .unwrap_or_default();
                    (AnswerValue::Single(value), label)
                }
            }
            crate::ui_backend::questionnaire::QuestionType::MultipleChoice => {
                let mut values: Vec<String> = Vec::new();
                let mut labels: Vec<String> = Vec::new();

                for idx in &questionnaire.selected {
                    if let Some(opt) = questionnaire.options.get(*idx) {
                        values.push(opt.value.clone());
                        labels.push(opt.text.clone());
                    }
                }

                if questionnaire.other_selected && !questionnaire.other_text.trim().is_empty() {
                    let custom = questionnaire.other_text.trim().to_string();
                    values.push(format!("other:{}", custom));
                    labels.push(format!("Other: {}", custom));
                }

                (AnswerValue::Multi(values), labels.join(", "))
            }
            crate::ui_backend::questionnaire::QuestionType::FreeText => {
                let text = questionnaire.free_text_answer.clone();
                (AnswerValue::Single(text.clone()), text)
            }
        }
    }

    /// Handle a user command
    async fn handle_command(&mut self, command: Command) -> Result<()> {
        // Log TUI command with correlation_id if available
        if let Some(correlation_id) = self.service.state().current_correlation_id() {
            if let Some(logger) = crate::debug_logger() {
                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                    correlation_id,
                    crate::LogCategory::Tui,
                    "command_processing",
                )
                .with_data(serde_json::json!({
                    "command": format!("{:?}", command)
                }));
                logger.log(entry);
            }
        }

        if let Command::QuestionSubmit = command {
            self.send_questionnaire_response(self.service.state()).await;
            return Ok(()); // Prevent falling through to service.handle_command() - fixes race condition
        }
        match command {
            Command::ApproveOperation => {
                self.send_approval_response(ApprovalChoice::ApproveOnce)
                    .await;
            }
            Command::ApproveSession => {
                self.send_approval_response(ApprovalChoice::ApproveSession)
                    .await;
            }
            Command::ApproveAlways => {
                self.send_approval_response(ApprovalChoice::ApproveAlways)
                    .await;
            }
            Command::DenyOperation => {
                self.send_approval_response(ApprovalChoice::Deny).await;
            }
            Command::DenyAlways => {
                self.send_approval_response(ApprovalChoice::DenyAlways)
                    .await;
            }
            Command::CloseModal => {
                if self.service.state().active_modal()
                    == Some(crate::ui_backend::ModalType::Approval)
                {
                    self.send_approval_response(ApprovalChoice::Deny).await;

                    // Add a message showing the user skipped the approval
                    self.service
                        .state()
                        .add_message(crate::ui_backend::Message {
                            role: crate::ui_backend::MessageRole::System,
                            content: "ℹ️ Operation skipped by user".to_string(),
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            provider: None,
                            model: None,
                            collapsed: false,
                            thinking: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                        });
                }
            }
            _ => {}
        }

        // Handle Quit with interruption if processing
        if let Command::Quit = command {
            // If LLM is processing, interrupt first
            let is_processing = self.service.state().llm_processing();
            if is_processing {
                self.service.handle_command(Command::Interrupt).await?;
            }
        }

        // Intercept SendMessage to check for slash commands
        if let Command::SendMessage(ref text) = command {
            if text.starts_with('/') {
                return self.handle_slash_command(text).await;
            }
        }

        // Delegate to modal manager if a modal is active
        if self.service.state().active_modal().is_some() {
            // Clone state to avoid borrow issues
            let state = self.service.state().clone();
            match self.modal_manager.handle_command(&command, &state)? {
                ModalResult::Handled => return Ok(()),
                ModalResult::Close => {
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                    return Ok(());
                }
                ModalResult::Transition(next_modal) => {
                    // Handle modal transitions (e.g., ProviderPicker -> ModelPicker)
                    if let crate::ui_backend::ModalType::ModelPicker = next_modal {
                        // Select the current provider before transitioning
                        let all_providers = self.service.get_providers().await;
                        let filter = state.provider_picker_filter();
                        let filtered_providers: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if let Some(provider_info) = filtered_providers.get(selected) {
                            let provider_id = provider_info.id.clone();
                            self.service
                                .handle_command(Command::SelectProvider(provider_id.clone()))
                                .await?;

                            // Load models for this provider and open model picker
                            let models = self.service.get_models(&provider_id).await;
                            state.set_available_models(models);

                            // Initialize selection to current model if applicable
                            let current_model = state.current_model();
                            let models = state.available_models();
                            let mut selected = 0;
                            if let Some(current) = current_model {
                                if let Some(idx) = models.iter().position(|m| m.id == current) {
                                    selected = idx;
                                }
                            }
                            state.set_model_picker_selected(selected);

                            state.set_model_picker_filter(String::new());
                        }
                    }
                    state.set_active_modal(Some(next_modal));
                    return Ok(());
                }
                ModalResult::NotHandled => {
                    // Fall through to general command handling
                }
            }
        }

        let state = self.service.state().clone();

        // Special handling for modal toggle commands
        match &command {
            Command::ToggleHelp => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::Help) {
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                } else {
                    state.set_active_modal(Some(crate::ui_backend::ModalType::Help));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
                return Ok(());
            }
            Command::ToggleThinkingTool => {
                // Toggle thinking tool + display
                let enabled = !state.thinking_tool_enabled();
                if state.llm_processing() {
                    state.set_pending_thinking_tool_enabled(Some(enabled));
                    state.set_status_message(Some(
                        "Thinking tool change queued (will apply after current response)"
                            .to_string(),
                    ));
                    return Ok(());
                }

                state.set_thinking_tool_enabled(enabled);
                // Notify service to refresh agent system prompt
                self.service.set_thinking_tool_enabled(enabled).await;

                use crate::ui_backend::{Message, MessageRole};
                let msg = Message {
                    role: MessageRole::System,
                    content: if enabled {
                        "✓ Thinking tool enabled. Agent will use structured reasoning.".to_string()
                    } else {
                        "✗ Thinking tool disabled.".to_string()
                    },
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                return Ok(());
            }
            Command::CloseModal => {
                // Restore original theme if canceling theme picker
                if state.active_modal() == Some(crate::ui_backend::ModalType::ThemePicker) {
                    if let Some(original_theme) = state.theme_before_preview() {
                        state.set_theme(original_theme);
                    }
                    state.set_theme_before_preview(None);
                }
                // Clear pending session switch if canceling confirmation dialog
                if state.active_modal() == Some(crate::ui_backend::ModalType::SessionSwitchConfirm)
                {
                    state.set_pending_session_switch(None);
                }
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                return Ok(());
            }
            Command::DeleteSessionSelected => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::SessionPicker) {
                    let all_sessions = state.available_sessions();
                    let filter = state.session_picker_filter();
                    let filtered_sessions: Vec<_> = if filter.is_empty() {
                        all_sessions
                    } else {
                        let filter_lower = filter.to_lowercase();
                        all_sessions
                            .into_iter()
                            .filter(|s| {
                                s.name.to_lowercase().contains(&filter_lower)
                                    || s.id.to_lowercase().contains(&filter_lower)
                            })
                            .collect()
                    };

                    let selected = state.session_picker_selected();
                    if let Some(session_meta) = filtered_sessions.get(selected) {
                        if let Err(err) = self.service.delete_session(&session_meta.id).await {
                            state.set_error_notification(Some(
                                crate::ui_backend::ErrorNotification {
                                    message: format!("Failed to delete session: {}", err),
                                    level: crate::ui_backend::ErrorLevel::Error,
                                    timestamp: chrono::Local::now(),
                                },
                            ));
                            return Ok(());
                        }

                        if let Ok(mut sessions) = self.service.list_sessions() {
                            let current_id = state.session().map(|s| s.session_id);
                            for session in &mut sessions {
                                session.is_current = current_id
                                    .as_deref()
                                    .map(|id| id == session.id)
                                    .unwrap_or(false);
                            }
                            state.set_available_sessions(sessions);
                        }

                        let new_selected = selected.saturating_sub(1);
                        state.set_session_picker_selected(new_selected);
                    }
                }
                return Ok(());
            }
            Command::DeletePolicyPattern => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::Policy) {
                    if let Some(mut modal) = state.policy_modal() {
                        // Get the pattern ID before removing from UI
                        if let Some(pattern_id) = modal.get_selected_pattern_id() {
                            // Delete from database
                            if let Err(err) = self.service.delete_policy_pattern(pattern_id).await {
                                state.set_error_notification(Some(
                                    crate::ui_backend::ErrorNotification {
                                        message: format!("Failed to delete pattern: {}", err),
                                        level: crate::ui_backend::ErrorLevel::Error,
                                        timestamp: chrono::Local::now(),
                                    },
                                ));
                            } else {
                                // Remove from local UI state
                                modal.remove_selected();
                                state.set_policy_modal(Some(modal));
                            }
                        }
                    }
                }
                return Ok(());
            }
            Command::ConfirmModal => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        // Theme already applied during preview, just clear preview state
                        state.set_theme_before_preview(None);
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Select provider and open model picker
                        // Apply filter to get the actual displayed list
                        let all_providers = self.service.get_providers().await;
                        let filter = state.provider_picker_filter();
                        let filtered_providers: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if let Some(provider_info) = filtered_providers.get(selected) {
                            let provider_id = provider_info.id.clone();

                            // Load models for this provider first
                            let models = self.service.get_models(&provider_id).await;

                            if let Some(logger) = crate::debug_logger() {
                                let correlation_id = state
                                    .current_correlation_id()
                                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                                    correlation_id,
                                    crate::LogCategory::Tui,
                                    "provider_picker_confirm",
                                )
                                .with_data(serde_json::json!({
                                    "filter": filter,
                                    "selected_index": selected,
                                    "provider_id": provider_id.clone(),
                                    "provider_name": provider_info.name,
                                    "active_modal": "ProviderPicker"
                                }));
                                logger.log(entry);
                            }

                            // Auto-select first model as default if available
                            let default_model = models.first().cloned();

                            // Set the provider
                            self.service
                                .handle_command(Command::SelectProvider(provider_id.clone()))
                                .await?;

                            // Set default model if available
                            if let Some(ref model_info) = default_model {
                                self.service
                                    .handle_command(Command::SelectModel(model_info.id.clone()))
                                    .await?;
                            }

                            // Update state with models and open model picker for user to confirm/change
                            let state = self.service.state();
                            state.set_available_models(models);

                            // Initialize selection to current model if it exists in the list
                            let current_model = state.current_model();
                            let models = state.available_models();
                            let mut selected = 0;
                            if let Some(current) = current_model {
                                if let Some(idx) = models.iter().position(|m| m.id == current) {
                                    selected = idx;
                                }
                            }
                            state.set_model_picker_selected(selected);

                            state.set_model_picker_filter(String::new());
                            state.set_active_modal(Some(crate::ui_backend::ModalType::ModelPicker));
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Select model and close
                        // Apply filter to get the actual displayed list
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered_models: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if let Some(model_info) = filtered_models.get(selected) {
                            let model_id = model_info.id.clone();
                            let model_name = model_info.name.clone();

                            if let Some(logger) = crate::debug_logger() {
                                let correlation_id = state
                                    .current_correlation_id()
                                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                                    correlation_id,
                                    crate::LogCategory::Tui,
                                    "model_picker_confirm",
                                )
                                .with_data(serde_json::json!({
                                    "filter": filter,
                                    "selected_index": selected,
                                    "model_id": model_id.clone(),
                                    "model_name": model_name.clone(),
                                    "active_modal": "ModelPicker"
                                }));
                                logger.log(entry);
                            }

                            self.service
                                .handle_command(Command::SelectModel(model_id))
                                .await?;
                        }
                        let state = self.service.state();
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionPicker) => {
                        // Switch to selected session
                        let all_sessions = state.available_sessions();
                        let filter = state.session_picker_filter();
                        let filtered_sessions: Vec<_> = if filter.is_empty() {
                            all_sessions
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_sessions
                                .into_iter()
                                .filter(|s| {
                                    s.name.to_lowercase().contains(&filter_lower)
                                        || s.id.to_lowercase().contains(&filter_lower)
                                })
                                .collect()
                        };

                        let selected = state.session_picker_selected();
                        if let Some(session_meta) = filtered_sessions.get(selected) {
                            if let Some(logger) = crate::debug_logger() {
                                let correlation_id = state
                                    .current_correlation_id()
                                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                                    correlation_id,
                                    crate::LogCategory::Tui,
                                    "session_picker_confirm",
                                )
                                .with_data(serde_json::json!({
                                    "filter": filter,
                                    "selected_index": selected,
                                    "session_id": session_meta.id.clone(),
                                    "session_name": session_meta.name.clone(),
                                    "active_modal": "SessionPicker"
                                }));
                                logger.log(entry);
                            }

                            // If agent is currently processing, show confirmation dialog
                            // instead of immediately switching
                            if state.llm_processing() {
                                tracing::info!(
                                    "Agent processing - showing session switch confirmation for {}",
                                    session_meta.id
                                );
                                // Store the pending session ID and show confirmation dialog
                                state.set_pending_session_switch(Some(session_meta.id.clone()));
                                state.set_session_switch_confirm_selected(0); // Default to "Wait"
                                state.set_active_modal(Some(
                                    crate::ui_backend::ModalType::SessionSwitchConfirm,
                                ));
                                return Ok(());
                            }

                            // No agent processing - switch directly
                            self.service
                                .handle_command(Command::SwitchSession(session_meta.id.clone()))
                                .await?;
                        }
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionSwitchConfirm) => {
                        // Handle confirmation dialog for session switch
                        let selected = state.session_switch_confirm_selected();
                        let pending_session = state.pending_session_switch();

                        if selected == 0 {
                            // "Wait" selected - just close the dialog and let agent finish
                            tracing::info!("User chose to wait for agent to finish");
                            state.set_pending_session_switch(None);
                            state.set_active_modal(None);
                            state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        } else if let Some(session_id) = pending_session {
                            // "Abort & Switch" selected - abort agent and switch
                            tracing::info!(
                                "User chose to abort agent and switch to session {}",
                                session_id
                            );
                            // Silently abort the agent
                            self.service.silent_interrupt().await;
                            state.clear_streaming();
                            state.set_llm_processing(false);
                            state.set_processing_correlation_id(None);
                            state.set_processing_session_id(None);

                            // Clear pending state
                            state.set_pending_session_switch(None);

                            // Switch to the new session
                            self.service
                                .handle_command(Command::SwitchSession(session_id))
                                .await?;
                            state.set_active_modal(None);
                            state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Tools)
                    | Some(crate::ui_backend::ModalType::Plugin) => {
                        // Tools and Plugin modals - just close on Enter
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                    _ => {
                        state.set_active_modal(None);
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                        return Ok(());
                    }
                }
            }
            Command::ModalUp => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        let selected = state.theme_picker_selected();
                        if selected > 0 {
                            state.set_theme_picker_selected(selected - 1);
                            // Live preview the theme
                            let all_themes = crate::ui_backend::ThemePreset::all();
                            let filter = state.theme_picker_filter();
                            let filtered: Vec<_> = if filter.is_empty() {
                                all_themes
                            } else {
                                let filter_lower = filter.to_lowercase();
                                all_themes
                                    .into_iter()
                                    .filter(|t| {
                                        t.display_name().to_lowercase().contains(&filter_lower)
                                    })
                                    .collect()
                            };
                            if let Some(&theme) = filtered.get(selected - 1) {
                                state.set_theme(theme);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_providers = state.available_providers();
                        let filter = state.provider_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if selected > 0 && selected <= filtered.len() {
                            state.set_provider_picker_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if selected > 0 && selected <= filtered.len() {
                            state.set_model_picker_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionPicker) => {
                        // Navigate up in session list
                        let selected = state.session_picker_selected();
                        if selected > 0 {
                            state.set_session_picker_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionSwitchConfirm) => {
                        // Navigate between options (0 = Wait, 1 = Abort & Switch)
                        let selected = state.session_switch_confirm_selected();
                        if selected == 1 {
                            state.set_session_switch_confirm_selected(0);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::TrustLevel) => {
                        // Navigate up in trust level selector (3 options: Manual, Balanced, Careful)
                        let selected = state.trust_level_selected();
                        if selected > 0 {
                            state.set_trust_level_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Tools) => {
                        // Navigate up in tools list
                        let selected = state.tools_selected();
                        if selected > 0 {
                            let new_selected = selected - 1;
                            state.set_tools_selected(new_selected);

                            // Auto-scroll up if selection goes above viewport
                            let scroll = state.tools_scroll_offset();
                            if new_selected < scroll {
                                state.set_tools_scroll_offset(new_selected);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Plugin) => {
                        // Navigate up in plugin list
                        let selected = state.plugin_selected();
                        if selected > 0 {
                            state.set_plugin_selected(selected - 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Policy) => {
                        // Navigate up in policy modal
                        if let Some(mut modal) = state.policy_modal() {
                            modal.move_up();
                            state.set_policy_modal(Some(modal));
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::ModalDown => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        let all_themes = crate::ui_backend::ThemePreset::all();
                        let filter = state.theme_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_themes
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_themes
                                .into_iter()
                                .filter(|t| t.display_name().to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.theme_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_theme_picker_selected(selected + 1);
                            // Live preview the theme
                            if let Some(&theme) = filtered.get(selected + 1) {
                                state.set_theme(theme);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_providers = state.available_providers();
                        let filter = state.provider_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_providers
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_providers
                                .into_iter()
                                .filter(|p| p.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.provider_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_provider_picker_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        // Apply filter to get the actual list being displayed
                        let all_models = state.available_models();
                        let filter = state.model_picker_filter();
                        let filtered: Vec<_> = if filter.is_empty() {
                            all_models
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_models
                                .into_iter()
                                .filter(|m| m.name.to_lowercase().contains(&filter_lower))
                                .collect()
                        };

                        let selected = state.model_picker_selected();
                        if selected + 1 < filtered.len() {
                            state.set_model_picker_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionPicker) => {
                        // Navigate down in session list
                        let all_sessions = state.available_sessions();
                        let filter = state.session_picker_filter();
                        let filtered_sessions: Vec<_> = if filter.is_empty() {
                            all_sessions
                        } else {
                            let filter_lower = filter.to_lowercase();
                            all_sessions
                                .into_iter()
                                .filter(|s| {
                                    s.name.to_lowercase().contains(&filter_lower)
                                        || s.id.to_lowercase().contains(&filter_lower)
                                })
                                .collect()
                        };

                        let selected = state.session_picker_selected();
                        if selected + 1 < filtered_sessions.len() {
                            state.set_session_picker_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::SessionSwitchConfirm) => {
                        // Navigate between options (0 = Wait, 1 = Abort & Switch)
                        let selected = state.session_switch_confirm_selected();
                        if selected == 0 {
                            state.set_session_switch_confirm_selected(1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::TrustLevel) => {
                        // Navigate down in trust level selector (3 options: Manual, Balanced, Careful)
                        let selected = state.trust_level_selected();
                        if selected < 2 {
                            // 0, 1, 2 for 3 options
                            state.set_trust_level_selected(selected + 1);
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Tools) => {
                        // Navigate down in tools list
                        let tools_count = state.tools_for_modal().len();
                        let selected = state.tools_selected();
                        if selected + 1 < tools_count {
                            let new_selected = selected + 1;
                            state.set_tools_selected(new_selected);

                            // Auto-scroll down if selection goes below viewport
                            // Visible tools = ~6 (based on modal height minus header)
                            let scroll = state.tools_scroll_offset();
                            let visible_count = 6;
                            if new_selected >= scroll + visible_count {
                                state.set_tools_scroll_offset(new_selected - visible_count + 1);
                            }
                        }
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Plugin) => {
                        // Navigate down in plugin list
                        let selected = state.plugin_selected();
                        // Note: The max limit will be checked by the plugin list itself
                        state.set_plugin_selected(selected + 1);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::Policy) => {
                        // Navigate down in policy modal
                        if let Some(mut modal) = state.policy_modal() {
                            modal.move_down();
                            state.set_policy_modal(Some(modal));
                        }
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::ModalFilter(filter) => {
                match state.active_modal() {
                    Some(crate::ui_backend::ModalType::ThemePicker) => {
                        if filter.is_empty() {
                            // Backspace - pop last character
                            let mut current = state.theme_picker_filter();
                            current.pop();
                            state.set_theme_picker_filter(current);
                        } else {
                            // Add character
                            let mut current = state.theme_picker_filter();
                            current.push_str(filter);
                            state.set_theme_picker_filter(current);
                        }
                        state.set_theme_picker_selected(0);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ProviderPicker) => {
                        if filter.is_empty() {
                            let mut current = state.provider_picker_filter();
                            current.pop();
                            state.set_provider_picker_filter(current);
                        } else {
                            let mut current = state.provider_picker_filter();
                            current.push_str(filter);
                            state.set_provider_picker_filter(current);
                        }
                        state.set_provider_picker_selected(0);
                        return Ok(());
                    }
                    Some(crate::ui_backend::ModalType::ModelPicker) => {
                        if filter.is_empty() {
                            let mut current = state.model_picker_filter();
                            current.pop();
                            state.set_model_picker_filter(current);
                        } else {
                            let mut current = state.model_picker_filter();
                            current.push_str(filter);
                            state.set_model_picker_filter(current);
                        }
                        state.set_model_picker_selected(0);
                        return Ok(());
                    }
                    _ => {}
                }
            }
            Command::FocusNext => {
                use crate::ui_backend::{FocusedComponent, VimMode};
                let current = state.focused_component();
                let (next, vim_mode) = match current {
                    FocusedComponent::Input => (FocusedComponent::Messages, VimMode::Normal),
                    FocusedComponent::Messages => (FocusedComponent::Panel, VimMode::Normal),
                    FocusedComponent::Panel => (FocusedComponent::Input, VimMode::Insert),
                    FocusedComponent::Modal => (current, state.vim_mode()),
                };
                state.set_focused_component(next);
                state.set_vim_mode(vim_mode);
                return Ok(());
            }
            Command::FocusInput => {
                use crate::ui_backend::{FocusedComponent, VimMode};
                state.set_focused_component(FocusedComponent::Input);
                state.set_vim_mode(VimMode::Insert); // Auto-switch to Insert mode
                return Ok(());
            }
            Command::FocusMessages => {
                use crate::ui_backend::{FocusedComponent, VimMode};
                state.set_focused_component(FocusedComponent::Messages);
                state.set_vim_mode(VimMode::Normal); // Messages panel uses Normal mode
                return Ok(());
            }
            Command::FocusPanel => {
                use crate::ui_backend::{FocusedComponent, VimMode};
                state.set_focused_component(FocusedComponent::Panel);
                state.set_vim_mode(VimMode::Normal); // Sidebar uses Normal mode
                return Ok(());
            }
            Command::OpenProviderPicker => {
                // Load providers and initialize picker state
                let providers = self.service.get_providers().await;
                state.set_available_providers(providers);

                // Initialize selection to current provider
                let current_provider = state.current_provider();
                let providers = state.available_providers();
                let mut selected = 0;
                if let Some(current) = current_provider {
                    if let Some(idx) = providers.iter().position(|p| p.id == current) {
                        selected = idx;
                    }
                }
                state.set_provider_picker_selected(selected);

                state.set_provider_picker_filter(String::new());
                state.set_active_modal(Some(crate::ui_backend::ModalType::ProviderPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                return Ok(());
            }
            Command::OpenModelPicker => {
                // Initialize selection to current model
                let current_model = state.current_model();
                let models = state.available_models();
                let mut selected = 0;
                if let Some(current) = current_model {
                    if let Some(idx) = models.iter().position(|m| m.id == current) {
                        selected = idx;
                    }
                }
                state.set_model_picker_selected(selected);
                state.set_model_picker_filter(String::new());

                state.set_active_modal(Some(crate::ui_backend::ModalType::ModelPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                return Ok(());
            }
            Command::ToggleThemePicker => {
                if state.active_modal() == Some(crate::ui_backend::ModalType::ThemePicker) {
                    // Close and restore original theme
                    if let Some(original_theme) = state.theme_before_preview() {
                        state.set_theme(original_theme);
                    }
                    state.set_active_modal(None);
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Input);
                } else {
                    // Open theme picker
                    state.set_theme_before_preview(Some(state.theme()));
                    state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                    // Initialize selection to current theme
                    let all_themes = crate::ui_backend::ThemePreset::all();
                    let current_theme = state.theme();
                    if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                        state.set_theme_picker_selected(idx);
                    }
                    state.set_theme_picker_filter(String::new());
                }
                return Ok(());
            }
            Command::ScrollUp => {
                // Scroll up in messages area
                let total_lines = state.messages_total_lines();
                let viewport_height = state.messages_viewport_height();
                let max_offset = total_lines.saturating_sub(viewport_height);
                let current_offset = state.messages_scroll_offset();
                let normalized = if current_offset == usize::MAX {
                    max_offset
                } else {
                    current_offset
                };
                if normalized > 0 {
                    state.set_messages_scroll_offset(normalized.saturating_sub(1));
                }
                return Ok(());
            }
            Command::ScrollDown => {
                // Scroll down in messages area
                let current_offset = state.messages_scroll_offset();
                let total_lines = state.messages_total_lines();
                let viewport_height = state.messages_viewport_height();
                let max_offset = total_lines.saturating_sub(viewport_height);
                let next = current_offset.saturating_add(1).min(max_offset);
                state.set_messages_scroll_offset(next);
                return Ok(());
            }
            Command::ToggleSidebarPanel(panel_idx) => {
                // Toggle the expansion state of a sidebar panel
                if *panel_idx < 4 {
                    let mut panels = state.sidebar_expanded_panels();
                    panels[*panel_idx] = !panels[*panel_idx];
                    state.set_sidebar_expanded_panels(panels);
                    state.set_sidebar_selected_panel(*panel_idx);
                }
                return Ok(());
            }
            Command::SidebarUp => {
                let selected_item = state.sidebar_selected_item();
                let panel_idx = state.sidebar_selected_panel();

                if selected_item.is_some() {
                    // Inside a panel - navigate items up
                    if let Some(item) = selected_item {
                        if item > 0 {
                            state.set_sidebar_selected_item(Some(item - 1));
                        } else {
                            // At first item, go back to panel header
                            state.set_sidebar_selected_item(None);
                        }
                    }
                } else {
                    // At panel level - navigate to previous panel
                    let new_panel = if panel_idx == 0 { 4 } else { panel_idx - 1 };
                    state.set_sidebar_selected_panel(new_panel);
                }
                return Ok(());
            }
            Command::SidebarDown => {
                let selected_item = state.sidebar_selected_item();
                let panel_idx = state.sidebar_selected_panel();

                if selected_item.is_some() {
                    // Inside a panel - navigate items down
                    // Get max items for current panel
                    let max_items = match panel_idx {
                        0 => 3, // Session: name, cost, tokens
                        1 => state.context_files().len(),
                        2 => state.tasks().len(),
                        3 => state.git_changes().len(),
                        _ => 0,
                    };
                    if let Some(item) = selected_item {
                        if item + 1 < max_items {
                            state.set_sidebar_selected_item(Some(item + 1));
                        }
                    }
                } else {
                    // At panel level - navigate to next panel
                    let new_panel = (panel_idx + 1) % 5;
                    state.set_sidebar_selected_panel(new_panel);
                }
                return Ok(());
            }
            Command::SidebarSelect => {
                let panel_idx = state.sidebar_selected_panel();
                let selected_item = state.sidebar_selected_item();

                if panel_idx == 4 {
                    // Theme panel - open theme picker
                    state.set_theme_before_preview(Some(state.theme()));
                    state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                    // Initialize selection to current theme
                    let all_themes = crate::ui_backend::ThemePreset::all();
                    let current_theme = state.theme();
                    if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                        state.set_theme_picker_selected(idx);
                    }
                    state.set_theme_picker_filter(String::new());
                } else if selected_item.is_none() {
                    // At panel header - toggle expansion
                    let mut panels = state.sidebar_expanded_panels();
                    panels[panel_idx] = !panels[panel_idx];
                    state.set_sidebar_expanded_panels(panels);
                }
                // If inside a panel with an item selected, Enter could trigger an action
                // (e.g., open file, show task details) - for now, no action
                return Ok(());
            }
            Command::SidebarEnter => {
                let panel_idx = state.sidebar_selected_panel();
                let panels = state.sidebar_expanded_panels();

                // Theme panel (4) opens theme picker instead of entering
                if panel_idx == 4 {
                    state.set_theme_before_preview(Some(state.theme()));
                    state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                    let all_themes = crate::ui_backend::ThemePreset::all();
                    let current_theme = state.theme();
                    if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                        state.set_theme_picker_selected(idx);
                    }
                    state.set_theme_picker_filter(String::new());
                } else if panel_idx < 4 && panels[panel_idx] {
                    // Panel is expanded - enter and select first item
                    let max_items = match panel_idx {
                        0 => 3, // Session: name, cost, tokens
                        1 => state.context_files().len(),
                        2 => state.tasks().len(),
                        3 => state.git_changes().len(),
                        _ => 0,
                    };
                    if max_items > 0 {
                        state.set_sidebar_selected_item(Some(0));
                    }
                } else if panel_idx < 4 {
                    // Panel is collapsed - expand it first
                    let mut panels = state.sidebar_expanded_panels();
                    panels[panel_idx] = true;
                    state.set_sidebar_expanded_panels(panels);
                }
                return Ok(());
            }
            Command::SidebarExit => {
                // Exit from inside a panel back to panel header
                if state.sidebar_selected_item().is_some() {
                    state.set_sidebar_selected_item(None);
                }
                return Ok(());
            }
            Command::QuestionCancel => {
                // Cancel questionnaire and send "skipped" response to LLM
                if state.active_questionnaire().is_some() {
                    // Clear UI state
                    state.set_active_questionnaire(None);

                    // Clear pending questionnaire data
                    let mut pending_guard = self.pending_questionnaire.lock().await;
                    *pending_guard = None;

                    // Send cancelled response back to the tool
                    let mut guard = self.questionnaire_responder.lock().await;
                    if let Some(responder) = guard.take() {
                        let _ = responder.send(UserResponse::cancelled());
                    }

                    // Add a message showing the user skipped
                    state.add_message(crate::ui_backend::Message {
                        role: crate::ui_backend::MessageRole::System,
                        content: "ℹ️ Question skipped by user".to_string(),
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        provider: None,
                        model: None,
                        collapsed: false,
                        thinking: None,
                        context_transient: true,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                    });
                }
                return Ok(());
            }

            // ========== Task Queue Management ==========
            Command::EditQueuedTask(index) => {
                // Start editing the task - open edit modal
                if state.start_task_edit(*index) {
                    state.set_active_modal(Some(crate::ui_backend::ModalType::TaskEdit));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
                return Ok(());
            }
            Command::DeleteQueuedTask(index) => {
                // Set pending delete and open confirm modal
                state.set_pending_delete_task(Some(*index));
                state.set_active_modal(Some(crate::ui_backend::ModalType::TaskDeleteConfirm));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                return Ok(());
            }
            Command::ConfirmDeleteTask => {
                // Confirm deletion of pending task
                if let Some(_deleted) = state.confirm_task_delete() {
                    // Task deleted - update sidebar tasks display
                    self.service.refresh_sidebar_data().await;
                }
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Panel);
                return Ok(());
            }
            Command::CancelDeleteTask => {
                // Cancel task deletion
                state.set_pending_delete_task(None);
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Panel);
                return Ok(());
            }
            Command::MoveTaskUp(index) => {
                // Move task up in queue (swap with previous)
                let idx = *index;
                if idx > 0 && state.move_queued_message(idx, idx - 1) {
                    // Update selection to follow the moved task
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        state.set_sidebar_selected_item(Some(item_idx.saturating_sub(1)));
                    }
                    // Refresh sidebar
                    self.service.refresh_sidebar_data().await;
                }
                return Ok(());
            }
            Command::MoveTaskDown(index) => {
                // Move task down in queue (swap with next)
                let idx = *index;
                let queue_len = state.queued_message_count();
                if idx < queue_len.saturating_sub(1) && state.move_queued_message(idx, idx + 1) {
                    // Update selection to follow the moved task
                    if let Some(item_idx) = state.sidebar_selected_item() {
                        state.set_sidebar_selected_item(Some(item_idx + 1));
                    }
                    // Refresh sidebar
                    self.service.refresh_sidebar_data().await;
                }
                return Ok(());
            }
            Command::RefreshSidebar => {
                // Refresh sidebar data (e.g., after drag-to-reorder)
                self.service.refresh_sidebar_data().await;
                return Ok(());
            }
            Command::UpdateTaskEditContent(content) => {
                // Update editing content
                state.set_editing_task_content(content.clone());
                return Ok(());
            }
            Command::ConfirmTaskEdit => {
                // Check if we were editing task 0 (next to be processed)
                let was_editing_task_0 = state.editing_task_index() == Some(0);

                // Confirm the edit
                if state.confirm_task_edit() {
                    // Edit saved - refresh sidebar
                    self.service.refresh_sidebar_data().await;
                }
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Panel);

                // If we were editing task 0 and LLM is not processing,
                // resume queue processing
                if was_editing_task_0 && !state.llm_processing() {
                    if let Some(next_msg) = state.pop_queued_message() {
                        state.remove_system_messages_containing("Message queued");
                        self.service
                            .handle_command(Command::SendMessage(next_msg))
                            .await?;
                    }
                }
                return Ok(());
            }
            Command::CancelTaskEdit => {
                // Check if we were editing task 0 (next to be processed)
                let was_editing_task_0 = state.editing_task_index() == Some(0);

                // Cancel the edit
                state.cancel_task_edit();
                state.set_active_modal(None);
                state.set_focused_component(crate::ui_backend::FocusedComponent::Panel);

                // If we were editing task 0 and LLM is not processing,
                // resume queue processing
                if was_editing_task_0 && !state.llm_processing() {
                    if let Some(next_msg) = state.pop_queued_message() {
                        state.remove_system_messages_containing("Message queued");
                        self.service
                            .handle_command(Command::SendMessage(next_msg))
                            .await?;
                    }
                }
                return Ok(());
            }

            _ => {}
        }

        // Pass command to AppService for processing
        self.service.handle_command(command).await?;

        Ok(())
    }

    /// Poll for async events (non-blocking)
    /// Processes events in batches, but stops after ToolStarted to ensure
    /// loading states are visible before completion events arrive.
    async fn poll_events(&mut self, state: &SharedState) -> Result<()> {
        let mut events_processed = 0;
        const MAX_EVENTS_PER_CYCLE: usize = 20; // Batch text chunks, but limit overall

        while let Ok(event) = self.event_rx.try_recv() {
            events_processed += 1;

            // Track if this is a ToolStarted event (we'll render after it)
            let is_tool_started = matches!(&event, AppEvent::ToolStarted { .. });

            // Check if session has changed during LLM processing
            // If so, discard LLM events from the old session
            let is_llm_event = matches!(
                &event,
                AppEvent::LlmTextChunk(_)
                    | AppEvent::LlmThinkingChunk(_)
                    | AppEvent::LlmCompleted { .. }
                    | AppEvent::ToolStarted { .. }
                    | AppEvent::ToolCompleted { .. }
                    | AppEvent::ToolFailed { .. }
            );

            if is_llm_event {
                let current_session_id = state.session().map(|s| s.session_id.clone());
                let processing_session_id = state.processing_session_id();

                // If we have a processing session and it doesn't match the current session,
                // discard this event - it's from a different session
                if let Some(ref proc_session) = processing_session_id {
                    if current_session_id.as_ref() != Some(proc_session) {
                        if let Some(logger) = crate::debug_logger() {
                            let entry = crate::DebugLogEntry::new(
                                state
                                    .current_correlation_id()
                                    .unwrap_or_else(|| "unknown".to_string()),
                                crate::LogCategory::Tui,
                                "llm_event_discarded_session_mismatch",
                            )
                            .with_data(serde_json::json!({
                                "processing_session_id": proc_session,
                                "current_session_id": current_session_id,
                                "event_type": format!("{:?}", std::mem::discriminant(&event))
                            }));
                            logger.log(entry);
                        }
                        tracing::warn!(
                            "Discarding LLM event from session {} (current session: {:?})",
                            proc_session,
                            current_session_id
                        );
                        // Still need to check for completion to clean up state
                        if matches!(&event, AppEvent::LlmCompleted { .. }) {
                            state.set_llm_processing(false);
                            state.set_processing_correlation_id(None);
                            state.set_processing_session_id(None);
                            state.clear_streaming();
                        }
                        continue; // Skip this event
                    }
                }
            }

            // Handle CommitIntermediateResponse with owned match (zero-copy: String moves directly)
            if let AppEvent::CommitIntermediateResponse(content) = event {
                if !content.is_empty() {
                    use crate::ui_backend::{Message, MessageRole};
                    let provider = state.current_provider();
                    let model = state.current_model();
                    let msg = Message {
                        role: MessageRole::Assistant,
                        content, // Moved directly, no clone
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        provider,
                        model,
                        context_transient: false,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                    };
                    // Clear streaming_content to avoid duplication in live display
                    state.set_streaming_content(None);
                    // Persist first (needs reference), then move into state
                    self.service.record_session_message(&msg).await;
                    state.add_message(msg);
                    state.scroll_to_bottom();

                    // Fast-path debug logging
                    if crate::is_debug_logging_enabled() {
                        if let Some(logger) = crate::debug_logger() {
                            logger.log(
                                crate::DebugLogEntry::new(
                                    state
                                        .current_correlation_id()
                                        .unwrap_or_else(|| "unknown".to_string()),
                                    crate::LogCategory::Tui,
                                    "commit_intermediate_response",
                                )
                                .with_data(serde_json::json!({ "committed": true })),
                            );
                        }
                    }
                }
                continue; // Skip the borrow match below
            }

            // Update state based on event type (borrow match for other events)
            match &event {
                AppEvent::LlmTextChunk(chunk) => {
                    if Self::maybe_handle_rate_limit_message(state, chunk) {
                        continue;
                    }
                    state.append_streaming_content(chunk);
                    state.scroll_to_bottom();
                }
                AppEvent::LlmThinkingChunk(chunk) => {
                    state.append_streaming_thinking(chunk);
                    state.scroll_to_bottom();
                }
                AppEvent::CommitIntermediateResponse(_) => unreachable!(), // Handled above
                AppEvent::LlmCompleted {
                    text,
                    thinking,
                    input_tokens,
                    output_tokens,
                } => {
                    // Add assistant message with final text
                    use crate::ui_backend::{Message, MessageRole};
                    // Prefer thinking from AgentResponse, fallback to streaming_thinking
                    let thinking_content = thinking.clone().or_else(|| state.streaming_thinking());
                    let provider = state.current_provider();
                    let model = state.current_model();

                    // Fast-path debug logging
                    if crate::is_debug_logging_enabled() {
                        if let Some(logger) = crate::debug_logger() {
                            logger.log(crate::DebugLogEntry::new(
                                state.current_correlation_id().unwrap_or_else(|| "unknown".to_string()),
                                crate::LogCategory::Tui,
                                "llm_completed_state",
                            ).with_data(serde_json::json!({
                                "text_len": text.len(),
                                "thinking_content_len": thinking_content.as_ref().map(|t| t.len()),
                                "has_thinking": thinking_content.as_ref().is_some_and(|t| !t.is_empty())
                            })));
                        }
                    }

                    let msg = Message {
                        role: MessageRole::Assistant,
                        content: text.clone(),
                        thinking: thinking_content.clone(),
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        provider: provider.clone(),
                        model: model.clone(),
                        context_transient: false,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                    };
                    state.add_message(msg.clone());
                    self.service.record_session_message(&msg).await;
                    state.scroll_to_bottom();

                    if let Some(thinking) = thinking_content {
                        if !thinking.is_empty() {
                            let thinking_msg = Message {
                                role: MessageRole::Thinking,
                                content: thinking,
                                thinking: None,
                                collapsed: true,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                provider: provider.clone(),
                                model: model.clone(),
                                context_transient: false,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                            };
                            state.add_message(thinking_msg.clone());
                            self.service.record_session_message(&thinking_msg).await;
                            state.scroll_to_bottom();
                        }
                    }

                    // Clear streaming state
                    state.clear_streaming();

                    // Update session cost (models.dev pricing)
                    if let (Some(provider), Some(model_id)) =
                        (state.current_provider(), state.current_model())
                    {
                        let cost = models_db()
                            .calculate_cost(
                                &provider,
                                &model_id,
                                *input_tokens as u32,
                                *output_tokens as u32,
                            )
                            .await;
                        state.add_session_cost(&model_id, cost);
                        state.add_session_tokens(&model_id, *input_tokens + *output_tokens);

                        // Persist usage to session file so it survives session switches
                        self.service
                            .update_session_usage(cost, *input_tokens, *output_tokens)
                            .await;
                    }

                    // Apply any pending changes queued during streaming
                    self.apply_pending_changes(state).await?;

                    // Refresh sidebar data (updates costs and tokens)
                    self.service.refresh_sidebar_data().await;

                    // Process next queued message if any
                    // BUT pause if we're currently editing the next task (index 0)
                    let is_editing_next_task = state.editing_task_index() == Some(0);
                    if !is_editing_next_task {
                        if let Some(next_msg) = state.pop_queued_message() {
                            // Remove the "Message queued" notification since we're processing it now
                            state.remove_system_messages_containing("Message queued");
                            self.service
                                .handle_command(Command::SendMessage(next_msg))
                                .await?;
                        }
                    }
                    // If editing the next task, queue processing is paused
                    // It will resume when the user confirms/cancels the edit
                }
                AppEvent::LlmError(error) => {
                    let error_text = error.to_string();
                    let is_rate_limit = is_rate_limit_message(&error_text)
                        || error_text.contains("429")
                        || error_text.contains("Too Many Requests");

                    if let Err(log_error) =
                        append_error_log(state.current_correlation_id(), &error_text)
                    {
                        tracing::warn!("Failed to write error log: {}", log_error);
                    }

                    if is_rate_limit {
                        state.set_flash_bar_state(FlashBarState::Warning);
                        state.set_status_message(Some("Rate limited".to_string()));
                        state.clear_streaming();
                        continue;
                    }

                    // Add error message
                    use crate::ui_backend::{Message, MessageRole};
                    state.set_flash_bar_state(FlashBarState::Error);
                    state.set_status_message(Some(summarize_error_for_flash_bar(&error_text)));
                    let msg = Message {
                        role: MessageRole::System,
                        content: format!("❌ Error: {}", error_text),
                        thinking: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        provider: None,
                        model: None,
                        context_transient: true,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                    };
                    state.add_message(msg.clone());
                    self.service.record_session_message(&msg).await;
                    state.scroll_to_bottom();

                    // Clear streaming state
                    state.clear_streaming();

                    // Apply any pending changes queued during streaming
                    self.apply_pending_changes(state).await?;
                }
                AppEvent::ToolStarted { name, args } => {
                    use crate::ui_backend::ActiveToolInfo;
                    use crate::ui_backend::{Message, MessageRole};

                    // Debug log to trace tool event reception
                    if crate::is_debug_logging_enabled() {
                        if let Some(logger) = crate::debug_logger() {
                            logger.log(
                                crate::DebugLogEntry::new(
                                    state
                                        .current_correlation_id()
                                        .unwrap_or_else(|| "unknown".to_string()),
                                    crate::LogCategory::Tui,
                                    "tool_started_received",
                                )
                                .with_data(serde_json::json!({
                                    "tool_name": name,
                                    "args_keys": args.as_object().map(|o| o.keys().collect::<Vec<_>>())
                                })),
                            );
                        }
                    }

                    // Collapse all previous tool messages before adding the new one
                    state.collapse_all_tools_except_last();

                    // Track active tool for loading indicator
                    let tool_info = ActiveToolInfo::new(name.clone(), args.clone());
                    let description = tool_info.description.clone();
                    state.add_active_tool(tool_info);

                    // Look up risk level for the tool (default to ReadOnly if unknown)
                    let risk_group = self
                        .service
                        .tool_risk_level(name)
                        .map(|r| r.group_name())
                        .unwrap_or("Exploration");

                    // Create a professional tool invocation message
                    // Format: "⋯|tool_name|risk_group|description"
                    // Running tools will be rendered expanded by message_area
                    let msg = Message {
                        role: MessageRole::Tool,
                        content: format!("⋯|{}|{}|{}", name, risk_group, description),
                        thinking: None,
                        provider: None,
                        model: None,
                        context_transient: false,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        collapsed: false, // Running tools are expanded (renderer also forces this)
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        tool_args: Some(args.clone()), // Store original args for rich rendering
                    };

                    // Debug log before adding message
                    if crate::is_debug_logging_enabled() {
                        if let Some(logger) = crate::debug_logger() {
                            logger.log(
                                crate::DebugLogEntry::new(
                                    state
                                        .current_correlation_id()
                                        .unwrap_or_else(|| "unknown".to_string()),
                                    crate::LogCategory::Tui,
                                    "tool_message_adding",
                                )
                                .with_data(serde_json::json!({
                                    "tool_name": name,
                                    "message_content": msg.content.clone(),
                                    "message_count_before": state.messages().len()
                                })),
                            );
                        }
                    }

                    state.add_message(msg.clone());
                    self.service.record_session_message(&msg).await;
                    state.scroll_to_bottom();
                }
                AppEvent::ToolCompleted { ref name, result } => {
                    // Get elapsed time before cleanup
                    let elapsed_time = state
                        .active_tools()
                        .iter()
                        .find(|t| &t.name == name)
                        .and_then(|t| t.elapsed_time());

                    // Complete the active tool tracking
                    state.complete_active_tool(name, result.clone(), true);
                    state.cleanup_completed_tools();

                    // Look up risk level for the tool (default to ReadOnly if unknown)
                    let risk_group = self
                        .service
                        .tool_risk_level(name)
                        .map(|r| r.group_name())
                        .unwrap_or("Exploration");

                    // Format the result nicely
                    // Format: "✓|tool_name|risk_group|result (elapsed)"
                    let result_preview = if result.len() > 500 {
                        format!("{}...", crate::core::truncate_at_char_boundary(result, 497))
                    } else {
                        result.clone()
                    };

                    // Add elapsed time to result if available
                    let result_with_time = if let Some(elapsed) = elapsed_time {
                        format!("{} ({:.1}s)", result_preview, elapsed)
                    } else {
                        result_preview
                    };

                    // Update the existing tool message in-place (animation effect)
                    // Keep the completed tool expanded initially, collapse all others
                    let new_content = format!("✓|{}|{}|{}", name, risk_group, result_with_time);
                    state.update_tool_message(name, new_content.clone(), true); // Collapse completed tool
                    state.collapse_all_tools_except_last(); // Make sure only most recent is visible
                    state.scroll_to_bottom();

                    // Persist the updated tool status to session storage
                    self.service
                        .update_session_tool_message(name, new_content)
                        .await;

                    // Handle mode switch confirmation
                    if name == "switch_mode" && result.contains("MODE_SWITCH_CONFIRMED:") {
                        if let Some(json_start) = result.find("MODE_SWITCH_CONFIRMED:") {
                            let json_str = &result[json_start + "MODE_SWITCH_CONFIRMED:".len()..];
                            let json_end = json_str.find('\n').unwrap_or(json_str.len());
                            let json_payload = &json_str[..json_end];

                            if let Ok(payload) =
                                serde_json::from_str::<serde_json::Value>(json_payload)
                            {
                                if let Some(target_mode) =
                                    payload.get("target_mode").and_then(|v| v.as_str())
                                {
                                    let mode: crate::core::types::AgentMode = target_mode.into();

                                    // IMPORTANT: Don't block on handle_command if LLM is still processing
                                    // This would cause a deadlock because handle_command needs a write lock
                                    // on chat_agent, which is already held by the streaming operation.
                                    // Instead, queue the mode switch to be applied after streaming completes.
                                    if state.llm_processing() {
                                        tracing::info!(
                                            "Queuing mode switch to {:?} (will apply after streaming completes)",
                                            mode
                                        );
                                        state.set_pending_mode_switch(Some(mode));
                                        state.set_status_message(Some(format!(
                                            "Mode switch to {} queued",
                                            mode.display_name()
                                        )));
                                    } else {
                                        // Not streaming, apply immediately
                                        if let Err(e) = self
                                            .service
                                            .handle_command(Command::SetAgentMode(mode))
                                            .await
                                        {
                                            tracing::error!("Failed to apply mode switch: {}", e);
                                            state.set_status_message(Some(format!(
                                                "Failed to switch mode: {}",
                                                e
                                            )));
                                        } else {
                                            tracing::info!(
                                                "Mode switched to {:?} via switch_mode tool",
                                                mode
                                            );
                                            state.set_status_message(Some(format!(
                                                "Switched to {} mode",
                                                mode.display_name()
                                            )));
                                        }
                                    }
                                }
                            }
                        }
                    }

                    if name == "write_file" || name == "delete_file" || name == "shell" {
                        self.service.refresh_sidebar_data().await;
                    }
                }
                AppEvent::ToolFailed { ref name, error } => {
                    // Get elapsed time before cleanup
                    let elapsed_time = state
                        .active_tools()
                        .iter()
                        .find(|t| &t.name == name)
                        .and_then(|t| t.elapsed_time());

                    // Complete the active tool as failed
                    state.complete_active_tool(name, error.clone(), false);
                    state.cleanup_completed_tools();

                    // Look up risk level for the tool (default to ReadOnly if unknown)
                    let risk_group = self
                        .service
                        .tool_risk_level(name)
                        .map(|r| r.group_name())
                        .unwrap_or("Exploration");

                    // Add elapsed time to error if available
                    let error_with_time = if let Some(elapsed) = elapsed_time {
                        format!("{} ({:.1}s)", error, elapsed)
                    } else {
                        error.clone()
                    };

                    // Update the existing tool message in-place (animation effect)
                    // Format: "✗|tool_name|risk_group|error (elapsed)"
                    let new_content = format!("✗|{}|{}|{}", name, risk_group, error_with_time);
                    state.update_tool_message(name, new_content.clone(), true); // Collapse failed tool
                    state.collapse_all_tools_except_last(); // Make sure only most recent is visible
                    state.scroll_to_bottom();

                    // Persist the updated tool status to session storage
                    self.service
                        .update_session_tool_message(name, new_content)
                        .await;
                }
                AppEvent::ContextCompacted {
                    old_tokens,
                    new_tokens,
                    messages_removed,
                    ref summary,
                } => {
                    // Show auto-compaction message in UI
                    use crate::ui_backend::{Message, MessageRole};

                    if let Err(err) = self.service.archive_old_messages(4).await {
                        tracing::warn!("Failed to archive compacted messages: {}", err);
                    }

                    let saved_tokens = old_tokens.saturating_sub(*new_tokens);
                    let saved_pct = if *old_tokens > 0 {
                        (saved_tokens as f64 / *old_tokens as f64 * 100.0) as usize
                    } else {
                        0
                    };

                    let summary_text = summary
                        .as_ref()
                        .map(|s| format!("\n\n📝 Summary: {}", s))
                        .unwrap_or_default();

                    let msg = Message {
                        role: MessageRole::System,
                        content: format!(
                            "📦 Auto-compacted: {}k → {}k tokens (saved {}%, {} messages){}",
                            old_tokens / 1000,
                            new_tokens / 1000,
                            saved_pct,
                            messages_removed,
                            summary_text
                        ),
                        thinking: None,
                        provider: None,
                        model: None,
                        context_transient: false,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    };
                    state.add_message(msg);
                    state.scroll_to_bottom();

                    // Refresh sidebar to show updated context usage
                    self.service.refresh_sidebar_data().await;
                }
                AppEvent::SessionSwitched { session_id } => {
                    // Load full session state from the switched session
                    use crate::ui_backend::SessionService;
                    tracing::info!("SessionSwitched event received for session: {}", session_id);
                    match self.service.storage().load_session(session_id) {
                        Ok(session) => {
                            tracing::info!(
                                "Session loaded successfully: {} ({})",
                                session.name,
                                session.id
                            );
                            // Restore UI messages
                            let ui_messages = SessionService::session_messages_to_ui(&session);
                            state.clear_messages();
                            for msg in ui_messages {
                                state.add_message(msg);
                            }
                            state.scroll_to_bottom();
                            self.service.refresh_archive_chunks().await;

                            // Clear session-specific runtime state
                            let _ = state.clear_message_queue();
                            // Clear per-model cost/token breakdowns (runtime-only, not persisted)
                            state.set_session_cost_by_model(Vec::new());
                            state.set_session_tokens_by_model(Vec::new());
                            // Clear context files (session-specific, should not persist across sessions)
                            state.clear_context_files();
                            // Note: Tasks are NOT cleared - they are workspace-level, not session-specific

                            // Restore provider and model selection
                            if !session.provider.is_empty() {
                                state.set_provider(Some(session.provider.clone()));
                            }
                            if !session.model.is_empty() {
                                state.set_model(Some(session.model.clone()));
                            }

                            // Load available models for the provider
                            if !session.provider.is_empty() {
                                let models = self.service.get_models(&session.provider).await;
                                state.set_available_models(models);
                            }

                            // Load available providers to ensure dropdown state
                            let providers = self.service.get_providers().await;
                            state.set_available_providers(providers);

                            // Update session info in state
                            let session_name = if session.name.is_empty() {
                                format!("Session {}", session.created_at.format("%Y-%m-%d %H:%M"))
                            } else {
                                session.name.clone()
                            };
                            state.set_session(Some(crate::ui_backend::SessionInfo {
                                session_id: session.id.clone(),
                                session_name: session_name.clone(),
                                total_cost: session.total_cost,
                                model_count: 1,
                                created_at: session
                                    .created_at
                                    .format("%Y-%m-%d %H:%M:%S")
                                    .to_string(),
                            }));

                            // Restore token/cost tracking from session
                            state.set_session_cost_total(session.total_cost);
                            state.set_session_tokens_total(
                                session.input_tokens + session.output_tokens,
                            );

                            // Restore trust level and build mode from approval_mode
                            let trust_level = match session.approval_mode.as_str() {
                                "zero_trust" => crate::tools::TrustLevel::Manual,
                                "only_reads" => crate::tools::TrustLevel::Careful,
                                _ => crate::tools::TrustLevel::Balanced, // "ask_risky" default
                            };
                            self.service.set_trust_level(trust_level).await;

                            // Restore agent mode from session
                            if !session.mode.is_empty() {
                                let mode = session
                                    .mode
                                    .parse::<crate::core::types::AgentMode>()
                                    .unwrap_or_default();
                                state.set_agent_mode(mode);
                            }

                            // Update LLM provider for conversation
                            if !session.provider.is_empty() {
                                if let Err(e) = self
                                    .service
                                    .set_provider_internal(&session.provider, Some(&session.model))
                                    .await
                                {
                                    tracing::warn!(
                                        "Failed to update LLM provider on session switch: {}",
                                        e
                                    );
                                }
                            } else {
                                // Session has no provider set - use default from config
                                let config = crate::config::Config::load().unwrap_or_default();
                                let default_provider = config.llm.default_provider.clone();
                                let default_model = config.llm.tark_sim.model.clone();
                                if !default_provider.is_empty() {
                                    if let Err(e) = self
                                        .service
                                        .set_provider_internal(
                                            &default_provider,
                                            Some(&default_model),
                                        )
                                        .await
                                    {
                                        tracing::warn!(
                                            "Failed to set default provider on session switch: {}",
                                            e
                                        );
                                    }
                                    // Update state to reflect default provider
                                    state.set_provider(Some(default_provider));
                                    state.set_model(Some(default_model));
                                }
                            }

                            tracing::info!("Session switched: {} ({})", session_name, session.id);
                        }
                        Err(e) => {
                            tracing::error!("Failed to load session {}: {}", session_id, e);
                        }
                    }
                }
                _ => {}
            }

            // Let renderer handle the event (for triggering refresh)
            self.renderer.handle_event(&event, state)?;

            // Stop after ToolStarted to ensure loading state is rendered
            // before ToolCompleted arrives and updates the message
            if is_tool_started {
                break;
            }

            // Also stop if we've processed too many events (prevent UI freeze)
            if events_processed >= MAX_EVENTS_PER_CYCLE {
                break;
            }
        }
        Ok(())
    }

    /// Get a reference to the renderer (for testing)
    #[allow(dead_code)]
    pub fn renderer(&self) -> &TuiRenderer<B> {
        &self.renderer
    }

    /// Get a mutable reference to the renderer (for testing)
    #[allow(dead_code)]
    pub fn renderer_mut(&mut self) -> &mut TuiRenderer<B> {
        &mut self.renderer
    }

    /// Get a reference to the service (for testing)
    #[allow(dead_code)]
    pub fn service(&self) -> &AppService {
        &self.service
    }

    /// Handle slash commands (e.g., /theme, /help, /model)
    async fn handle_slash_command(&mut self, text: &str) -> Result<()> {
        let state = self.service.state().clone();

        match text.trim() {
            "/help" | "/?" => {
                state.set_active_modal(Some(crate::ui_backend::ModalType::Help));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            "/model" | "/provider" => {
                // Load providers and initialize picker state
                let providers = self.service.get_providers().await;
                state.set_available_providers(providers);

                // Initialize selection to current provider
                let current_provider = state.current_provider();
                let providers = state.available_providers();
                let mut selected = 0;
                if let Some(current) = current_provider {
                    if let Some(idx) = providers.iter().position(|p| p.id == current) {
                        selected = idx;
                    }
                }
                state.set_provider_picker_selected(selected);

                state.set_provider_picker_filter(String::new());
                state.set_active_modal(Some(crate::ui_backend::ModalType::ProviderPicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            "/theme" => {
                // Open theme picker with preview
                state.set_theme_before_preview(Some(state.theme()));
                state.set_active_modal(Some(crate::ui_backend::ModalType::ThemePicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);

                // Initialize selection to current theme
                let all_themes = crate::ui_backend::ThemePreset::all();
                let current_theme = state.theme();
                if let Some(idx) = all_themes.iter().position(|&t| t == current_theme) {
                    state.set_theme_picker_selected(idx);
                }
                state.set_theme_picker_filter(String::new());
            }
            "/diff" => {
                let next_mode = state.diff_view_mode().next();
                state.set_diff_view_mode(next_mode);

                use crate::ui_backend::{DiffViewMode, Message, MessageRole};
                let hint = match next_mode {
                    DiffViewMode::Auto => "Auto (responsive by width)",
                    DiffViewMode::Inline => "Inline",
                    DiffViewMode::Split => "Split",
                };
                let msg = Message {
                    role: MessageRole::System,
                    content: format!("Diff view: {}", hint),
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                state.clear_input();
                return Ok(());
            }
            cmd if cmd.starts_with("/diff ") => {
                use crate::ui_backend::{DiffViewMode, Message, MessageRole};
                let mode = cmd.trim_start_matches("/diff ").trim();
                let selected = match mode {
                    "auto" => Some(DiffViewMode::Auto),
                    "inline" => Some(DiffViewMode::Inline),
                    "split" => Some(DiffViewMode::Split),
                    _ => None,
                };

                let msg = Message {
                    role: MessageRole::System,
                    content: if let Some(selected) = selected {
                        state.set_diff_view_mode(selected);
                        format!("Diff view: {}", selected.display_name())
                    } else {
                        "Usage: /diff [auto|inline|split]".to_string()
                    },
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                state.clear_input();
                return Ok(());
            }
            "/file" | "/files" => {
                // Refresh file picker with workspace files
                self.service.refresh_file_picker("");
                let state = self.service.state();
                state.set_file_picker_filter(String::new());
                state.set_file_picker_selected(0);
                state.set_active_modal(Some(crate::ui_backend::ModalType::FilePicker));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
            }
            cmd if cmd.starts_with("/attach ") => {
                // Attach file directly by path: /attach path/to/file
                let path = cmd.trim_start_matches("/attach ").trim().to_string();
                if !path.is_empty() {
                    // Use BFF API to add attachment - drop state borrow first
                    let result = self.service.add_attachment(&path);
                    let state = self.service.state();
                    match result {
                        Ok(info) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let success_msg = Message {
                                role: MessageRole::System,
                                content: format!(
                                    "Attached: {} {} ({})",
                                    info.type_icon, info.filename, info.size_display
                                ),
                                thinking: None,
                                provider: None,
                                model: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(success_msg);
                        }
                        Err(e) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let error_msg = Message {
                                role: MessageRole::System,
                                content: format!("Failed to attach file: {}", e),
                                thinking: None,
                                provider: None,
                                model: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            };
                            state.add_message(error_msg);
                        }
                    }
                    // Re-clear input after adding state message
                    state.clear_input();
                    return Ok(());
                } else {
                    // No path provided, open file picker
                    state.set_active_modal(Some(crate::ui_backend::ModalType::FilePicker));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
            }
            "/attachments" | "/files-clear" => {
                // Clear all attachments - drop state borrow first
                self.service.clear_attachments();
                let state = self.service.state();
                use crate::ui_backend::{Message, MessageRole};
                let clear_msg = Message {
                    role: MessageRole::System,
                    content: "Cleared all attachments.".to_string(),
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(clear_msg);
                state.clear_input();
                return Ok(());
            }
            "/clear" => {
                // Clear UI messages
                state.clear_messages();
                state.set_messages_scroll_offset(0);

                // Clear context files (attached files)
                state.clear_context_files();

                // Clear queued messages
                state.clear_message_queue();

                // Clear backend conversation history and context (including plan context)
                self.service.clear_conversation_and_context().await;

                // Update tasks display (will be empty after queue clear)
                self.service.refresh_sidebar_data().await;

                // Add confirmation message
                use crate::ui_backend::{Message, MessageRole};
                let msg = Message {
                    role: MessageRole::System,
                    content: "Chat and context cleared.".to_string(),
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                state.clear_input();
            }
            "/quit" | "/exit" | "/q" => {
                state.set_should_quit(true);
            }
            "/think" => {
                // Toggle model-level thinking (API parameter)
                let enabled = !state.thinking_enabled();
                state.set_thinking_enabled(enabled);

                use crate::ui_backend::{Message, MessageRole};
                let msg = Message {
                    role: MessageRole::System,
                    content: if enabled {
                        "✓ Model thinking enabled. Extended reasoning will be used.".to_string()
                    } else {
                        "✗ Model thinking disabled.".to_string()
                    },
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                state.clear_input();
                return Ok(());
            }
            "/thinking" => {
                // Toggle thinking tool + display
                let enabled = !state.thinking_tool_enabled();
                if state.llm_processing() {
                    state.set_pending_thinking_tool_enabled(Some(enabled));
                    state.set_status_message(Some(
                        "Thinking tool change queued (will apply after current response)"
                            .to_string(),
                    ));
                    state.clear_input();
                    return Ok(());
                }

                state.set_thinking_tool_enabled(enabled);
                // Notify service to refresh agent system prompt
                self.service.set_thinking_tool_enabled(enabled).await;

                use crate::ui_backend::{Message, MessageRole};
                let msg = Message {
                    role: MessageRole::System,
                    content: if enabled {
                        "✓ Thinking tool enabled. Agent will use structured reasoning.".to_string()
                    } else {
                        "✗ Thinking tool disabled.".to_string()
                    },
                    thinking: None,
                    provider: None,
                    model: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                };
                state.add_message(msg);
                state.clear_input();
                return Ok(());
            }
            "/compact" => {
                // Force context compaction
                use crate::ui_backend::{Message, MessageRole};
                state.clear_input();

                match self.service.compact_context().await {
                    Ok(result) => {
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!(
                                "📦 Context compacted: {} → {} tokens ({} messages removed){}",
                                result.old_tokens,
                                result.new_tokens,
                                result.messages_removed,
                                result
                                    .summary
                                    .as_ref()
                                    .map(|s| format!("\n\nSummary: {}", s))
                                    .unwrap_or_default()
                            ),
                            thinking: None,
                            provider: None,
                            model: None,
                            context_transient: false,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        };
                        state.add_message(msg);
                        // Refresh sidebar to show updated context usage
                        self.service.refresh_sidebar_data().await;
                    }
                    Err(e) => {
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!("⚠️ Compaction failed: {}", e),
                            thinking: None,
                            provider: None,
                            model: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        };
                        state.add_message(msg);
                    }
                }
                return Ok(());
            }
            "/tools" => {
                // External MCP tools only
                use crate::tools::ToolCategory;
                let all_tools = self.service.get_tools();
                let tools: Vec<_> = all_tools
                    .into_iter()
                    .filter(|t| t.category == ToolCategory::External)
                    .collect();

                if tools.is_empty() {
                    use crate::ui_backend::{Message, MessageRole};
                    let msg = Message {
                        role: MessageRole::System,
                        content: "No external tools configured.\n\n\
                            Configure MCP servers in:\n\
                              ~/.config/tark/mcp/servers.toml (global)\n\
                              .tark/mcp/servers.toml (project)\n\n\
                            See: tark help mcp"
                            .to_string(),
                        thinking: None,
                        context_transient: false,
                        tool_calls: Vec::new(),
                        segments: Vec::new(),
                        tool_args: None,
                        collapsed: false,
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        provider: None,
                        model: None,
                    };
                    state.add_message(msg);
                } else {
                    state.set_tools_for_modal(tools);
                    state.set_tools_selected(0);
                    state.set_tools_scroll_offset(0);
                    state.set_active_modal(Some(crate::ui_backend::ModalType::Tools));
                    state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                }
                state.clear_input();
                return Ok(());
            }
            "/_tools" => {
                // Internal tools only (Core + Builtin) - developer command
                use crate::tools::ToolCategory;
                let all_tools = self.service.get_tools();
                let tools: Vec<_> = all_tools
                    .into_iter()
                    .filter(|t| t.category != ToolCategory::External)
                    .collect();

                state.set_tools_for_modal(tools);
                state.set_tools_selected(0);
                state.set_tools_scroll_offset(0);
                state.set_active_modal(Some(crate::ui_backend::ModalType::Tools));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                state.clear_input();
                return Ok(());
            }
            "/policy" => {
                // Show approval/denial patterns for this session
                let session_id = state
                    .session()
                    .map(|s| s.session_id.clone())
                    .unwrap_or_else(|| "session".to_string());

                match self.service.list_session_patterns(&session_id) {
                    Ok((approvals, denials)) => {
                        use crate::tui_new::modals::policy_modal::{
                            PolicyModal, PolicyPatternEntry,
                        };

                        // Convert from policy types to modal types
                        let approval_entries: Vec<PolicyPatternEntry> = approvals
                            .into_iter()
                            .map(|p| PolicyPatternEntry {
                                id: p.id,
                                tool: p.tool,
                                pattern: p.pattern,
                                match_type: p.match_type,
                                is_denial: p.is_denial,
                                description: p.description,
                            })
                            .collect();

                        let denial_entries: Vec<PolicyPatternEntry> = denials
                            .into_iter()
                            .map(|p| PolicyPatternEntry {
                                id: p.id,
                                tool: p.tool,
                                pattern: p.pattern,
                                match_type: p.match_type,
                                is_denial: p.is_denial,
                                description: p.description,
                            })
                            .collect();

                        let modal = PolicyModal::new(approval_entries, denial_entries);
                        state.set_policy_modal(Some(modal));
                        state.set_active_modal(Some(crate::ui_backend::ModalType::Policy));
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                    }
                    Err(e) => {
                        use crate::ui_backend::{Message, MessageRole};
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!("⚠️ Failed to load policy patterns: {}", e),
                            thinking: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            provider: None,
                            model: None,
                        };
                        state.add_message(msg);
                    }
                }
                state.clear_input();
                return Ok(());
            }
            "/plugins" => {
                // Open plugin management modal
                state.set_active_modal(Some(crate::ui_backend::ModalType::Plugin));
                state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                state.clear_input();
                return Ok(());
            }
            "/sessions" => {
                match self.service.list_sessions() {
                    Ok(mut sessions) => {
                        let current_id = state.session().map(|s| s.session_id);
                        for session in &mut sessions {
                            session.is_current = current_id
                                .as_deref()
                                .map(|id| id == session.id)
                                .unwrap_or(false);
                        }
                        let selected = sessions.iter().position(|s| s.is_current).unwrap_or(0);
                        state.set_available_sessions(sessions);
                        state.set_session_picker_selected(selected);
                        state.set_session_picker_filter(String::new());
                        state.set_active_modal(Some(crate::ui_backend::ModalType::SessionPicker));
                        state.set_focused_component(crate::ui_backend::FocusedComponent::Modal);
                    }
                    Err(e) => {
                        use crate::ui_backend::{Message, MessageRole};
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!("Failed to list sessions: {}", e),
                            thinking: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            provider: None,
                            model: None,
                        };
                        state.add_message(msg);
                    }
                }
                state.clear_input();
                return Ok(());
            }
            "/new" => {
                self.service.handle_command(Command::NewSession).await?;
                state.clear_input();
                return Ok(());
            }
            cmd if cmd.starts_with("/session ") => {
                // Switch to session: /session <id>
                let session_id = cmd.trim_start_matches("/session ").trim().to_string();
                if !session_id.is_empty() {
                    // Need to release state borrow before calling mutable method
                    let result = {
                        // state goes out of scope here
                        self.service.switch_session(&session_id)
                    };
                    let state = self.service.state();

                    match result {
                        Ok(_) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let msg = Message {
                                role: MessageRole::System,
                                content: format!("Switched to session: {}", session_id),
                                thinking: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                provider: None,
                                model: None,
                            };
                            state.add_message(msg);
                        }
                        Err(e) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let msg = Message {
                                role: MessageRole::System,
                                content: format!("Failed to switch session: {}", e),
                                thinking: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                provider: None,
                                model: None,
                            };
                            state.add_message(msg);
                        }
                    }
                    state.clear_input();
                    return Ok(());
                }
            }
            cmd if cmd.starts_with("/export") => {
                // Export session: /export [path]
                let path_str = cmd.trim_start_matches("/export").trim();
                let path = if path_str.is_empty() {
                    format!(
                        "session_{}.json",
                        chrono::Local::now().format("%Y%m%d_%H%M%S")
                    )
                } else {
                    path_str.to_string()
                };
                match self.service.export_session(std::path::Path::new(&path)) {
                    Ok(_) => {
                        use crate::ui_backend::{Message, MessageRole};
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!("Session exported to: {}", path),
                            thinking: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            provider: None,
                            model: None,
                        };
                        state.add_message(msg);
                    }
                    Err(e) => {
                        use crate::ui_backend::{Message, MessageRole};
                        let msg = Message {
                            role: MessageRole::System,
                            content: format!("Failed to export session: {}", e),
                            thinking: None,
                            context_transient: true,
                            tool_calls: Vec::new(),
                            segments: Vec::new(),
                            tool_args: None,
                            collapsed: false,
                            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                            provider: None,
                            model: None,
                        };
                        state.add_message(msg);
                    }
                }
                state.clear_input();
                return Ok(());
            }
            cmd if cmd.starts_with("/import ") => {
                // Import session: /import <path>
                let path = cmd.trim_start_matches("/import ").trim().to_string();
                if !path.is_empty() {
                    // Need to release state borrow before calling mutable method
                    let result = {
                        // state goes out of scope here
                        self.service.import_session(std::path::Path::new(&path))
                    };
                    let state = self.service.state();

                    match result {
                        Ok(session) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let msg = Message {
                                role: MessageRole::System,
                                content: format!(
                                    "Session imported: {} ({})",
                                    session.session_id, session.created_at
                                ),
                                thinking: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                provider: None,
                                model: None,
                            };
                            state.add_message(msg);
                        }
                        Err(e) => {
                            use crate::ui_backend::{Message, MessageRole};
                            let msg = Message {
                                role: MessageRole::System,
                                content: format!("Failed to import session: {}", e),
                                thinking: None,
                                context_transient: true,
                                tool_calls: Vec::new(),
                                segments: Vec::new(),
                                tool_args: None,
                                collapsed: false,
                                timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                                provider: None,
                                model: None,
                            };
                            state.add_message(msg);
                        }
                    }
                    state.clear_input();
                    return Ok(());
                }
            }
            _ => {
                // Unknown command - add system message
                use crate::ui_backend::{Message, MessageRole};
                let system_msg = Message {
                    role: MessageRole::System,
                    content: format!(
                        "Unknown command: {}. Type /help for available commands.",
                        text
                    ),
                    thinking: None,
                    context_transient: true,
                    tool_calls: Vec::new(),
                    segments: Vec::new(),
                    tool_args: None,
                    collapsed: false,
                    timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                    provider: None,
                    model: None,
                };
                state.add_message(system_msg);
            }
        }

        // Clear input after processing slash command
        state.clear_input();

        Ok(())
    }

    async fn apply_pending_changes(&mut self, state: &SharedState) -> Result<()> {
        if let Some(provider) = state.take_pending_provider() {
            tracing::info!(
                "Applying queued provider switch to {} after streaming completed",
                provider
            );
            if let Err(e) = self
                .service
                .handle_command(Command::SelectProvider(provider.clone()))
                .await
            {
                tracing::error!("Failed to apply queued provider switch: {}", e);
                state.set_status_message(Some(format!("Failed to switch provider: {}", e)));
            } else {
                state.set_status_message(Some(format!("Provider switched to {}", provider)));
            }
        }

        if let Some(model) = state.take_pending_model() {
            tracing::info!(
                "Applying queued model switch to {} after streaming completed",
                model
            );
            if let Err(e) = self
                .service
                .handle_command(Command::SelectModel(model.clone()))
                .await
            {
                tracing::error!("Failed to apply queued model switch: {}", e);
                state.set_status_message(Some(format!("Failed to switch model: {}", e)));
            } else {
                state.set_status_message(Some(format!("Model switched to {}", model)));
            }
        }

        if let Some(enabled) = state.take_pending_thinking_tool_enabled() {
            tracing::info!(
                "Applying queued thinking tool toggle to {} after streaming completed",
                enabled
            );
            self.service.set_thinking_tool_enabled(enabled).await;
            state.set_status_message(Some(if enabled {
                "Thinking tool enabled".to_string()
            } else {
                "Thinking tool disabled".to_string()
            }));
        }

        if let Some(pending_mode) = state.take_pending_mode_switch() {
            tracing::info!(
                "Applying queued mode switch to {:?} after streaming completed",
                pending_mode
            );
            if let Err(e) = self
                .service
                .handle_command(Command::SetAgentMode(pending_mode))
                .await
            {
                tracing::error!("Failed to apply queued mode switch: {}", e);
                state.set_status_message(Some(format!("Failed to switch mode: {}", e)));
            } else {
                state.set_status_message(Some(format!(
                    "Switched to {} mode",
                    pending_mode.display_name()
                )));
            }
        }

        Ok(())
    }
}

fn parse_rate_limit_delay_secs(text: &str) -> Option<u64> {
    let lower = text.to_ascii_lowercase();
    let markers = ["retrying in ", "retry in ", "quota resets in "];
    for marker in markers {
        if let Some(idx) = lower.find(marker) {
            let start = idx + marker.len();
            let digits: String = lower[start..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if !digits.is_empty() {
                return digits.parse().ok();
            }
        }
    }
    None
}

fn is_rate_limit_message(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("rate limited") || lower.contains("rate limit")
}

fn summarize_error_for_flash_bar(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let code = parse_error_code(error);
    let label = if lower.contains("bad request") {
        "Bad request"
    } else if lower.contains("unauthorized") {
        "Unauthorized"
    } else if lower.contains("forbidden") {
        "Forbidden"
    } else if lower.contains("not found") {
        "Not found"
    } else if lower.contains("rate limit") {
        "Rate limited"
    } else if lower.contains("timeout") {
        "Timeout"
    } else {
        "Error"
    };

    if let Some(code) = code {
        format!("{} ({})", label, code)
    } else if label != "Error" {
        label.to_string()
    } else {
        let trimmed = error.trim();
        if trimmed.len() > 80 {
            format!("{}...", &trimmed[..80])
        } else if trimmed.is_empty() {
            "Error".to_string()
        } else {
            trimmed.to_string()
        }
    }
}

fn parse_error_code(error: &str) -> Option<u16> {
    for marker in ["\"code\":", "code:"] {
        if let Some(idx) = error.find(marker) {
            let mut digits = String::new();
            for ch in error[idx + marker.len()..].chars() {
                if ch.is_ascii_digit() {
                    digits.push(ch);
                } else if !digits.is_empty() {
                    break;
                }
            }
            if let Ok(code) = digits.parse::<u16>() {
                return Some(code);
            }
        }
    }
    None
}

fn append_error_log(correlation_id: Option<String>, error: &str) -> anyhow::Result<()> {
    use std::io::Write;

    let log_dir = std::path::Path::new(".tark");
    std::fs::create_dir_all(log_dir)?;
    let log_path = log_dir.join("err");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    let corr = correlation_id.unwrap_or_else(|| "unknown".to_string());
    writeln!(
        file,
        "[{}] correlation_id={} error={}",
        timestamp, corr, error
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        is_rate_limit_message, parse_error_code, parse_rate_limit_delay_secs,
        summarize_error_for_flash_bar,
    };

    #[test]
    fn parse_rate_limit_retrying_in_seconds() {
        let text = "Rate limited. Retrying in 5 seconds (attempt 2/7)...";
        assert_eq!(parse_rate_limit_delay_secs(text), Some(5));
    }

    #[test]
    fn parse_rate_limit_quota_resets_in() {
        let text = "Rate limit exceeded. Quota resets in 10s.";
        assert_eq!(parse_rate_limit_delay_secs(text), Some(10));
    }

    #[test]
    fn parse_rate_limit_no_match() {
        let text = "All good, no rate limit here.";
        assert_eq!(parse_rate_limit_delay_secs(text), None);
    }

    #[test]
    fn detect_rate_limit_message() {
        assert!(is_rate_limit_message(
            "Rate limited. Retrying in 5 seconds (attempt 2/7)..."
        ));
        assert!(is_rate_limit_message("Rate limit exceeded."));
        assert!(!is_rate_limit_message("All good, no limits."));
    }

    #[test]
    fn summarize_error_includes_code_when_present() {
        let err = "Bad request: {\"code\": 400, \"message\": \"nope\"}";
        assert_eq!(summarize_error_for_flash_bar(err), "Bad request (400)");
    }

    #[test]
    fn parse_error_code_handles_json_style() {
        let err = "{\"code\": 429, \"message\": \"Rate limit\"}";
        assert_eq!(parse_error_code(err), Some(429));
    }
}
