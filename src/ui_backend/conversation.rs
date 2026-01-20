//! Conversation Service - Manages conversation lifecycle
//!
//! Orchestrates ConversationManager, ContextManager, and ChatAgent
//! to handle the full chat flow including streaming and context management.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::agent::ChatAgent;
use crate::core::context_manager::ContextManager;
use crate::core::conversation_manager::ConversationManager;
use crate::core::tokenizer::ApproximateTokenizer;
use crate::core::types::AgentMode;
use crate::llm::LlmProvider;
use crate::storage::ChatSession;
use crate::tools::InteractionSender;
use crate::tools::ToolRegistry;

use super::errors::ConversationError;
use super::events::AppEvent;

/// Context usage information
#[derive(Debug, Clone)]
pub struct ContextUsage {
    pub used_tokens: usize,
    pub max_tokens: usize,
    pub percent: f32,
}

/// Result of context compaction
#[derive(Debug, Clone)]
pub struct CompactionResult {
    pub old_tokens: usize,
    pub new_tokens: usize,
    pub messages_removed: usize,
}

/// Conversation Service
///
/// Orchestrates the chat flow by coordinating:
/// - ConversationManager (message history + streaming)
/// - ContextManager (token counting + compaction)
/// - ChatAgent (LLM communication)
pub struct ConversationService {
    /// Conversation manager for message history and streaming
    conversation_mgr: Arc<tokio::sync::RwLock<ConversationManager>>,
    /// Context manager for token counting
    context_mgr: Arc<tokio::sync::RwLock<ContextManager>>,
    /// Chat agent for LLM communication
    chat_agent: Arc<tokio::sync::RwLock<ChatAgent>>,
    /// Event channel for streaming updates
    event_tx: mpsc::UnboundedSender<AppEvent>,
    /// Interaction channel (ask_user / approval)
    interaction_tx: Option<InteractionSender>,
    /// Approval storage path (per session)
    approvals_path: tokio::sync::RwLock<Option<std::path::PathBuf>>,
    /// Interrupt flag
    interrupt_flag: Arc<AtomicBool>,
    /// Processing flag
    is_processing: Arc<AtomicBool>,
}

impl ConversationService {
    /// Create a new conversation service
    pub fn new(chat_agent: ChatAgent, event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        let tokenizer = Arc::new(ApproximateTokenizer::default_128k());
        let conversation_mgr = Arc::new(tokio::sync::RwLock::new(ConversationManager::new()));
        let context_mgr = Arc::new(tokio::sync::RwLock::new(ContextManager::new(tokenizer)));
        let chat_agent = Arc::new(tokio::sync::RwLock::new(chat_agent));

        Self {
            conversation_mgr,
            context_mgr,
            chat_agent,
            event_tx,
            interaction_tx: None,
            approvals_path: tokio::sync::RwLock::new(None),
            interrupt_flag: Arc::new(AtomicBool::new(false)),
            is_processing: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new conversation service with interaction support
    pub fn new_with_interaction(
        chat_agent: ChatAgent,
        event_tx: mpsc::UnboundedSender<AppEvent>,
        interaction_tx: Option<InteractionSender>,
        approvals_path: Option<std::path::PathBuf>,
    ) -> Self {
        let service = Self::new(chat_agent, event_tx);
        Self {
            interaction_tx,
            approvals_path: tokio::sync::RwLock::new(approvals_path),
            ..service
        }
    }

    /// Update the active LLM provider for the chat agent
    pub async fn update_llm_provider(
        &self,
        llm: Arc<dyn LlmProvider>,
    ) -> Result<(), ConversationError> {
        if self.is_processing.load(Ordering::SeqCst) {
            return Err(ConversationError::Other(anyhow::anyhow!(
                "Cannot switch provider while a message is streaming"
            )));
        }

        let mut agent = self.chat_agent.write().await;
        agent.update_provider(llm);
        agent.refresh_system_prompt_async().await;
        Ok(())
    }

    /// Update tool approval trust level for the chat agent
    pub async fn set_trust_level(
        &self,
        level: crate::tools::TrustLevel,
    ) -> Result<(), ConversationError> {
        let mut agent = self.chat_agent.write().await;
        agent.set_trust_level(level).await;
        Ok(())
    }

    /// Update approval storage path for the tool registry
    pub async fn update_approval_storage_path(
        &self,
        storage_path: std::path::PathBuf,
    ) -> Result<(), ConversationError> {
        {
            let mut guard = self.approvals_path.write().await;
            *guard = Some(storage_path.clone());
        }
        let mut agent = self.chat_agent.write().await;
        agent.set_approval_storage_path(storage_path).await;
        Ok(())
    }

    /// Send a message and stream the response
    pub async fn send_message(
        &self,
        content: &str,
        correlation_id: Option<String>,
    ) -> Result<(), ConversationError> {
        // Check if already processing
        if self.is_processing.load(Ordering::SeqCst) {
            return Err(ConversationError::Other(anyhow::anyhow!(
                "Already processing a message"
            )));
        }

        self.is_processing.store(true, Ordering::SeqCst);
        self.interrupt_flag.store(false, Ordering::SeqCst);

        // Get correlation_id (use provided or generate new)
        let correlation_id = correlation_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Log LLM request start
        if let Some(logger) = crate::debug_logger() {
            let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                correlation_id.clone(),
                crate::LogCategory::Service,
                "llm_request_start",
            )
            .with_data(serde_json::json!({
                "content_length": content.len()
            }));
            logger.log(entry);
        }

        // Add user message
        {
            let mut conv = self.conversation_mgr.write().await;
            conv.add_user_message(content.to_string());
        }

        // Start streaming
        {
            let mut conv = self.conversation_mgr.write().await;
            conv.start_streaming().map_err(|e| {
                ConversationError::Other(anyhow::anyhow!("Failed to start streaming: {}", e))
            })?;
        }

        // Emit started event
        let _ = self.event_tx.send(AppEvent::LlmStarted);

        // Create streaming callbacks
        let event_tx_text = self.event_tx.clone();
        let event_tx_thinking = self.event_tx.clone();
        let event_tx_tool_start = self.event_tx.clone();
        let event_tx_tool_complete = self.event_tx.clone();
        let interrupt_flag = self.interrupt_flag.clone();

        let check_interrupt = move || interrupt_flag.load(Ordering::SeqCst);

        let on_text = move |chunk: String| {
            let _ = event_tx_text.send(AppEvent::LlmTextChunk(chunk));
        };

        let on_thinking = move |chunk: String| {
            let _ = event_tx_thinking.send(AppEvent::LlmThinkingChunk(chunk));
        };

        let correlation_id_tool_start = correlation_id.clone();
        let on_tool_call = move |tool_name: String, tool_args: String| {
            if let Ok(args_json) = serde_json::from_str::<serde_json::Value>(&tool_args) {
                // Log tool invocation
                if let Some(logger) = crate::debug_logger() {
                    let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                        correlation_id_tool_start.clone(),
                        crate::LogCategory::Service,
                        "tool_invocation",
                    )
                    .with_data(serde_json::json!({
                        "tool": tool_name.clone(),
                        "args": args_json.clone()
                    }));
                    logger.log(entry);
                }

                let _ = event_tx_tool_start.send(AppEvent::ToolStarted {
                    name: tool_name,
                    args: args_json,
                });
            }
        };

        let correlation_id_tool_complete = correlation_id.clone();
        let on_tool_complete = move |tool_name: String, result: String, success: bool| {
            // Log tool response
            let result_preview = if result.len() > 500 {
                format!("{}...", &result[..500])
            } else {
                result.clone()
            };

            if let Some(logger) = crate::debug_logger() {
                let entry: crate::DebugLogEntry = crate::DebugLogEntry::new(
                    correlation_id_tool_complete.clone(),
                    crate::LogCategory::Service,
                    "tool_response",
                )
                .with_data(serde_json::json!({
                    "tool": tool_name.clone(),
                    "success": success,
                    "result_preview": result_preview,
                    "result_length": result.len()
                }));
                logger.log(entry);
            }

            if success {
                let _ = event_tx_tool_complete.send(AppEvent::ToolCompleted {
                    name: tool_name,
                    result,
                });
            } else {
                let _ = event_tx_tool_complete.send(AppEvent::ToolFailed {
                    name: tool_name,
                    error: result,
                });
            }
        };

        // Send to LLM with streaming
        let result = {
            let mut agent = self.chat_agent.write().await;
            agent
                .chat_streaming(
                    content,
                    check_interrupt,
                    on_text,
                    on_thinking,
                    on_tool_call,
                    on_tool_complete,
                )
                .await
        };

        // Finalize streaming
        {
            let mut conv = self.conversation_mgr.write().await;
            match result {
                Ok(response) => {
                    // Log LLM response
                    if let Some(logger) = crate::debug_logger() {
                        logger.log(
                            crate::DebugLogEntry::new(
                                correlation_id.clone(),
                                crate::LogCategory::Service,
                                "llm_response",
                            )
                            .with_data(serde_json::json!({
                                "text_length": response.text.len(),
                                "tool_calls_made": response.tool_calls_made,
                                "input_tokens": response.usage.as_ref().map(|u| u.input_tokens),
                                "output_tokens": response.usage.as_ref().map(|u| u.output_tokens),
                            })),
                        );
                    }

                    // Add assistant message
                    conv.add_assistant_message(response.text.clone(), None);
                    conv.clear_streaming();

                    // Emit completion event
                    let (input_tokens, output_tokens) = response
                        .usage
                        .map(|u| (u.input_tokens as usize, u.output_tokens as usize))
                        .unwrap_or((0, 0));

                    let _ = self.event_tx.send(AppEvent::LlmCompleted {
                        text: response.text,
                        input_tokens,
                        output_tokens,
                    });
                }
                Err(e) => {
                    // Log LLM error with full context
                    let error_context = crate::debug_logger::ErrorContext::from_error(e.as_ref());
                    if let Some(logger) = crate::debug_logger() {
                        let entry1: crate::DebugLogEntry = crate::DebugLogEntry::new(
                            correlation_id.clone(),
                            crate::LogCategory::Service,
                            "llm_error",
                        )
                        .with_data(serde_json::json!({
                            "error_message": e.to_string()
                        }));
                        logger.log(entry1);

                        let entry2: crate::DebugLogEntry = crate::DebugLogEntry::new(
                            correlation_id.clone(),
                            crate::LogCategory::Service,
                            "llm_error_detail",
                        )
                        .with_error_context(error_context);
                        logger.log(entry2);
                    }

                    conv.handle_error(e.to_string(), false);
                    conv.clear_streaming();
                    let _ = self.event_tx.send(AppEvent::LlmError(e.to_string()));
                }
            }
        }

        self.is_processing.store(false, Ordering::SeqCst);
        Ok(())
    }

    /// Update agent mode and tool registry
    pub async fn update_mode(&self, working_dir: std::path::PathBuf, mode: AgentMode) {
        let approvals_path = { self.approvals_path.read().await.clone() };
        let tools = ToolRegistry::for_mode_with_services(
            working_dir,
            mode,
            true,
            self.interaction_tx.clone(),
            None,
            approvals_path,
        );
        let mut agent = self.chat_agent.write().await;
        agent.update_mode(tools, mode);
    }

    /// Interrupt the current operation
    pub fn interrupt(&self) {
        self.interrupt_flag.store(true, Ordering::SeqCst);
        // Also reset processing flag to allow new messages after interruption
        // The streaming loop will see the interrupt flag and exit early
        self.is_processing.store(false, Ordering::SeqCst);
    }

    /// Interrupt and reset all streaming state (async version)
    /// This should be called when user explicitly cancels to ensure clean state
    pub async fn interrupt_and_reset(&self) {
        // Set interrupt flags
        self.interrupt_flag.store(true, Ordering::SeqCst);
        self.is_processing.store(false, Ordering::SeqCst);

        // Also clear the streaming state in conversation manager
        // This is crucial to prevent "Invalid streaming state transition" errors
        let mut conv = self.conversation_mgr.write().await;
        conv.clear_streaming();
    }

    /// Check if currently processing
    pub fn is_processing(&self) -> bool {
        self.is_processing.load(Ordering::SeqCst)
    }

    /// Get current context usage
    pub async fn context_usage(&self) -> ContextUsage {
        let context = self.context_mgr.read().await;
        let used_tokens = context.current_tokens();
        let max_tokens = context.max_tokens();
        let percent = context.usage_percent();

        ContextUsage {
            used_tokens,
            max_tokens,
            percent,
        }
    }

    /// Compact context (remove older messages)
    pub async fn compact_context(&self) -> Result<CompactionResult, ConversationError> {
        let mut context = self.context_mgr.write().await;
        let conv = self.conversation_mgr.read().await;

        let message_count = conv.message_count();
        let estimated_tokens_per_msg = if message_count > 0 {
            context.current_tokens() / message_count
        } else {
            100 // Default estimate
        };

        let compact_result = context
            .compact(message_count, estimated_tokens_per_msg)
            .map_err(|e| ConversationError::Other(anyhow::anyhow!(e.to_string())))?;

        Ok(CompactionResult {
            old_tokens: context.current_tokens() + compact_result.tokens_freed,
            new_tokens: compact_result.new_token_count,
            messages_removed: compact_result.messages_removed,
        })
    }

    /// Restore conversation from a session
    pub async fn restore_from_session(&self, session: &ChatSession) {
        let mut conv = self.conversation_mgr.write().await;
        conv.restore_from_session(session);
    }

    /// Clear conversation history
    pub async fn clear_history(&self) {
        let mut conv = self.conversation_mgr.write().await;
        conv.clear_messages();
        conv.clear_streaming();
    }

    /// Get message count
    pub async fn message_count(&self) -> usize {
        let conv = self.conversation_mgr.read().await;
        conv.message_count()
    }
}

#[cfg(test)]
mod tests {
    use super::ConversationService;
    use crate::agent::ChatAgent;
    use crate::llm::{LlmProvider, LlmResponse, Message, ToolDefinition};
    use async_trait::async_trait;
    use std::sync::atomic::Ordering;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    struct TestProvider {
        name: &'static str,
    }

    #[async_trait]
    impl LlmProvider for TestProvider {
        fn name(&self) -> &str {
            self.name
        }

        async fn chat(
            &self,
            _messages: &[Message],
            _tools: Option<&[ToolDefinition]>,
        ) -> anyhow::Result<LlmResponse> {
            Ok(LlmResponse::Text {
                text: self.name.to_string(),
                usage: None,
            })
        }

        async fn complete_fim(
            &self,
            _prefix: &str,
            _suffix: &str,
            _language: &str,
        ) -> anyhow::Result<crate::llm::CompletionResult> {
            unimplemented!("test provider")
        }

        async fn explain_code(&self, _code: &str, _context: &str) -> anyhow::Result<String> {
            unimplemented!("test provider")
        }

        async fn suggest_refactorings(
            &self,
            _code: &str,
            _context: &str,
        ) -> anyhow::Result<Vec<crate::llm::RefactoringSuggestion>> {
            unimplemented!("test provider")
        }

        async fn review_code(
            &self,
            _code: &str,
            _language: &str,
        ) -> anyhow::Result<Vec<crate::llm::CodeIssue>> {
            unimplemented!("test provider")
        }
    }

    #[tokio::test]
    async fn update_llm_provider_rejects_while_processing() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let tools = crate::tools::ToolRegistry::new(std::path::PathBuf::from("."));
        let agent = ChatAgent::new(Arc::new(TestProvider { name: "a" }), tools);
        let service = ConversationService::new(agent, tx);

        service.is_processing.store(true, Ordering::SeqCst);

        let result = service
            .update_llm_provider(Arc::new(TestProvider { name: "b" }))
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_llm_provider_allows_when_idle() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let tools = crate::tools::ToolRegistry::new(std::path::PathBuf::from("."));
        let agent = ChatAgent::new(Arc::new(TestProvider { name: "a" }), tools);
        let service = ConversationService::new(agent, tx);

        let result = service
            .update_llm_provider(Arc::new(TestProvider { name: "b" }))
            .await;

        assert!(result.is_ok());
    }
}
