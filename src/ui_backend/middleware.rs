//! Command Middleware
//!
//! Provides a pipeline pattern for processing commands with logging,
//! validation, and transformation capabilities.

use super::commands::Command;
use super::state::SharedState;

/// Result of middleware processing
#[derive(Debug, Clone)]
pub enum MiddlewareResult {
    /// Continue processing with this command
    Continue(Command),
    /// Transform the command into another command
    Transform(Command),
    /// Block this command from being processed
    Block,
}

/// Middleware function type
pub type MiddlewareFn =
    Box<dyn Fn(&Command, &SharedState) -> MiddlewareResult + Send + Sync + 'static>;

/// Command pipeline that applies middlewares in sequence
pub struct CommandPipeline {
    middlewares: Vec<MiddlewareFn>,
}

impl CommandPipeline {
    /// Create a new empty pipeline
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    /// Add a middleware to the pipeline
    pub fn with_middleware(mut self, middleware: MiddlewareFn) -> Self {
        self.middlewares.push(middleware);
        self
    }

    /// Process a command through the middleware pipeline
    ///
    /// Returns Some(Command) if the command should be processed,
    /// None if it was blocked by a middleware.
    pub fn process(&self, cmd: Command, state: &SharedState) -> Option<Command> {
        let mut current = cmd;

        for middleware in &self.middlewares {
            match middleware(&current, state) {
                MiddlewareResult::Continue(c) => current = c,
                MiddlewareResult::Transform(c) => current = c,
                MiddlewareResult::Block => return None,
            }
        }

        Some(current)
    }
}

impl Default for CommandPipeline {
    fn default() -> Self {
        Self::new()
    }
}

// ========== Built-in Middlewares ==========

/// Logging middleware - logs all commands
pub fn logging_middleware(cmd: &Command, _state: &SharedState) -> MiddlewareResult {
    tracing::debug!(command = ?cmd, "Processing command");
    MiddlewareResult::Continue(cmd.clone())
}

/// Validation middleware - blocks invalid commands
pub fn validation_middleware(cmd: &Command, state: &SharedState) -> MiddlewareResult {
    match cmd {
        // Block SendMessage if LLM not connected
        Command::SendMessage(_) => {
            if !state.llm_connected() {
                tracing::warn!("Blocked SendMessage: LLM not connected");
                return MiddlewareResult::Block;
            }
        }
        // Block SelectProvider/SelectModel if LLM not connected
        Command::SelectProvider(_) | Command::SelectModel(_) => {
            if !state.llm_connected() {
                tracing::warn!("Blocked provider/model selection: LLM not connected");
                return MiddlewareResult::Block;
            }
        }
        _ => {}
    }

    MiddlewareResult::Continue(cmd.clone())
}

/// Transformation middleware example - normalize commands
pub fn normalization_middleware(cmd: &Command, _state: &SharedState) -> MiddlewareResult {
    // Example: Transform empty SendMessage to ClearInput
    if let Command::SendMessage(text) = cmd {
        if text.trim().is_empty() {
            return MiddlewareResult::Transform(Command::ClearInput);
        }
    }

    MiddlewareResult::Continue(cmd.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_backend::SharedState;

    #[test]
    fn test_pipeline_empty() {
        let pipeline = CommandPipeline::new();
        let state = SharedState::new();
        let cmd = Command::Quit;

        let result = pipeline.process(cmd.clone(), &state);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), Command::Quit));
    }

    #[test]
    fn test_pipeline_with_logging() {
        let pipeline = CommandPipeline::new().with_middleware(Box::new(logging_middleware));
        let state = SharedState::new();
        let cmd = Command::Quit;

        let result = pipeline.process(cmd, &state);
        assert!(result.is_some());
    }

    #[test]
    fn test_validation_blocks_send_message_when_disconnected() {
        let pipeline = CommandPipeline::new().with_middleware(Box::new(validation_middleware));
        let state = SharedState::new();
        // LLM is not connected by default
        assert!(!state.llm_connected());

        let cmd = Command::SendMessage("test".to_string());
        let result = pipeline.process(cmd, &state);
        assert!(result.is_none()); // Should be blocked
    }

    #[test]
    fn test_validation_allows_send_message_when_connected() {
        let pipeline = CommandPipeline::new().with_middleware(Box::new(validation_middleware));
        let state = SharedState::new();
        state.set_llm_connected(true);

        let cmd = Command::SendMessage("test".to_string());
        let result = pipeline.process(cmd, &state);
        assert!(result.is_some()); // Should be allowed
    }

    #[test]
    fn test_normalization_transforms_empty_message() {
        let pipeline = CommandPipeline::new().with_middleware(Box::new(normalization_middleware));
        let state = SharedState::new();

        let cmd = Command::SendMessage("   ".to_string());
        let result = pipeline.process(cmd, &state);
        assert!(result.is_some());
        assert!(matches!(result.unwrap(), Command::ClearInput));
    }
}
