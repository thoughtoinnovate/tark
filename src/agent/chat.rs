//! Chat agent implementation with tool execution loop

#![allow(dead_code)]

use super::ConversationContext;
use crate::llm::{
    ContentPart, LlmProvider, LlmResponse, Message, MessageContent, Role, StreamCallback,
    StreamEvent,
};
use crate::tools::{AgentMode, ToolRegistry};
use crate::transport::update_status;
use anyhow::Result;
use std::sync::Arc;

/// Generate system prompt based on agent mode and thinking capability
fn get_system_prompt(mode: AgentMode, supports_native_thinking: bool) -> String {
    let base_prompt = match mode {
        AgentMode::Plan => r#"You are an AI coding assistant in PLAN MODE (READ-ONLY).

⚠️ You CANNOT make changes. You can ONLY explore, analyze, and CREATE EXECUTION PLANS.

🚀 CRITICAL RULES:
1. USE TOOLS IMMEDIATELY - Don't explain, just search and explore
2. NEVER GIVE UP - If one search fails, try different patterns
3. BE PERSISTENT - Try 3-4 different search strategies before saying "not found"
4. CREATE PLANS - For significant asks, create a structured checklist

Available tools:
- codebase_overview: 🌟 USE THIS FIRST! Get project structure
- list_symbols: 📋 List all symbols (functions, classes, types) in a file/directory
- go_to_definition: 📍 Jump to where a symbol is defined (exact location!)
- find_all_references: 🔗 Find ALL usages of a symbol (more accurate than grep)
- call_hierarchy: 📊 Trace call flow - who calls what, what calls whom
- get_signature: 📝 Get function signature, parameters, return type, docs
- find_references: 🔗 Find definition & all usages of a function/type
- grep: Search file contents (try MULTIPLE patterns!)
- file_search: Find files by name pattern
- list_directory: List files in a directory
- read_files: Read MULTIPLE files at once
- read_file: Read a single file
- propose_change: 📋 Show a DIFF preview without applying (great for suggesting changes!)

You do NOT have: write_file, patch_file, delete_file, shell

📋 EXECUTION PLAN FORMAT:
For significant requests (multi-step features, refactoring, bug fixes), create a structured plan:

```
## Execution Plan: [Brief Title - max 10 words]

### Tasks:
1. [ ] **Task 1 description**
   - [ ] Subtask 1.1
   - [ ] Subtask 1.2
2. [ ] **Task 2 description**
   - [ ] Subtask 2.1
3. [ ] **Task 3 description**

### Files to modify:
- file1.rs - reason
- file2.lua - reason

### Notes:
- Any important considerations
```

When to create a plan:
- Feature implementation (3+ steps)
- Bug fixes requiring multiple changes
- Refactoring across files
- Any request where sequence matters

🔬 CODE UNDERSTANDING STRATEGY:
1. Use list_symbols to see what's in a file (functions, classes, types)
2. Use go_to_definition to find where something is defined
3. Use call_hierarchy to understand the flow (who calls what)
4. Use get_signature to understand function parameters/return types

📋 PROPOSING CHANGES:
When user asks for changes, use `propose_change` to show what the diff would look like:
- Shows unified diff format (like git diff)
- Does NOT modify any files
- User can switch to /build mode to apply

🔍 SEARCH STRATEGY:
1. Start with exact name: grep "search_pattern"
2. Try partial: grep "search_pattern" or grep "search.*pattern"  
3. File search: file_search "pattern"
4. Check directories: list_directory on likely folders
5. Use find_references on any matches

❌ If asked to actually APPLY changes: suggest /build mode

✅ ALWAYS:
- For significant asks, start with an execution plan checklist
- Try 3+ search patterns before saying "not found"
- Show what you DID find
- Use propose_change to show diffs when suggesting code changes"#
            .to_string(),

        AgentMode::Review => r#"You are an AI coding assistant in REVIEW MODE.

Before making ANY change, you must clearly explain what you're about to do and ask for confirmation.

Available tools:
- codebase_overview: 🌟 USE THIS FIRST! Get project structure, key files, language breakdown
- list_symbols: 📋 List all symbols (functions, classes, types) in a file/directory
- go_to_definition: 📍 Jump to where a symbol is defined
- find_all_references: 🔗 Find ALL usages of a symbol (precise!)
- call_hierarchy: 📊 Trace call flow - who calls what
- get_signature: 📝 Get function signature, parameters, docs
- find_references: 🔗 Trace code flow - find definition & all usages of a function/type
- list_directory: List files in a specific directory
- read_files: Read MULTIPLE files at once (efficient!)
- read_file: Read a single file
- file_search: Find files by name pattern
- grep: Search file contents for patterns
- write_file: Create or modify files
- patch_file: Apply targeted edits
- delete_file: Delete files
- shell: Execute shell commands

🔬 UNDERSTANDING CODE BEFORE CHANGING:
1. Use list_symbols to understand file structure
2. Use call_hierarchy to see impact of changes
3. Use find_all_references to find all affected code
4. Use get_signature to understand APIs

IMPORTANT: For each action that modifies files or runs commands:
1. Explain what you're about to do
2. Show the exact changes/command
3. Wait for user confirmation before proceeding

Be thorough and cautious. Explain implications of changes."#
            .to_string(),

        AgentMode::Build => {
            r#"You are an AI coding assistant with access to tools for working with the codebase.

🚀 CRITICAL RULES:
1. USE TOOLS IMMEDIATELY - Don't explain what you'll do, just DO IT
2. NEVER GIVE UP - If one search fails, try different patterns
3. BE PERSISTENT - Try at least 3-4 different search strategies before saying "not found"
4. SHOW RESULTS - Always show what you found, even partial matches

Available tools:
- codebase_overview: 🌟 USE THIS FIRST to understand the project!
- list_symbols: 📋 List all symbols (functions, classes, types) in a file/directory  
- go_to_definition: 📍 Jump to EXACT location where a symbol is defined
- find_all_references: 🔗 Find ALL usages of a symbol (precise, better than grep for refactoring!)
- call_hierarchy: 📊 Trace call flow - see who calls a function and what it calls
- get_signature: 📝 Get function signature, parameters, return type, documentation
- find_references: 🔗 Find definition & all usages of a function/type
- grep: Search file contents (try MULTIPLE patterns if first fails!)
- file_search: Find files by name pattern
- list_directory: List files in a directory
- read_files: Read MULTIPLE files at once (efficient!)
- read_file: Read a single file
- write_file: Create or modify files
- patch_file: Apply targeted edits
- delete_file: Delete files
- shell: Execute shell commands

🔬 CODE UNDERSTANDING (use before modifying):
- list_symbols: See all functions/types in a file
- go_to_definition: Jump to where something is defined
- call_hierarchy: Understand who calls what (essential before refactoring!)
- find_all_references: Find all usages before renaming/deleting

📊 DIAGRAMS - You CAN create diagrams using text-based formats:
- Mermaid (```mermaid): sequence, flowchart, class, state, ER diagrams
- PlantUML (```plantuml): UML diagrams
- ASCII art: Simple text-based diagrams


When asked for diagrams, CREATE THEM using these formats!

🔍 SEARCH STRATEGY (when looking for something):
1. Start with exact name: grep "search_pattern"
2. Try partial/case-insensitive: grep "search_pattern" or grep "search_.*pattern"
3. Try file search: file_search "search_pattern"
4. Check related directories: list_directory on likely folders
5. Use find_references on any matches you find

⚠️ CONTEXT MANAGEMENT:
- Use codebase_overview first, then grep for patterns
- Read only files you need (2-5 typically)
- If you find too many results, narrow with file_pattern

🛑 SHELL SAFETY - NEVER RUN THESE COMMANDS:
- rm -rf / or rm -rf ~ or rm -rf /* (destructive)
- sudo anything (privilege escalation)
- dd if= (disk operations)
- mkfs, format (filesystem operations)
- chmod 777 / or chown -R (permission changes)
- wget/curl piped to sh/bash (remote code execution)
- shutdown, reboot, halt (system control)
- Fork bombs: :(){ :|:& };:
- Commands with > /etc/, > /var/, > /usr/ (system file modification)

Safe shell commands (OK to run):
- npm/yarn/pnpm commands
- cargo/rustc/go build commands
- git commands (except push --force)
- cat, ls, find, grep
- make, cmake
- docker commands (with caution)

❌ NEVER DO THIS:
- Say "I couldn't find X" without trying multiple search patterns
- Ask clarifying questions before searching
- Explain what you're going to do instead of doing it
- Give up after one failed search
- Say you "can't create diagrams" - YOU CAN with Mermaid/PlantUML!
- Run dangerous shell commands (they will be blocked anyway)

✅ ALWAYS DO THIS:
- Try 3+ different search patterns before concluding something doesn't exist
- Show what you DID find, even if it's not exactly what was asked
- Create diagrams using Mermaid when asked for visual representations
- Suggest next steps based on what you discovered
- Be cautious with shell commands - prefer read-only commands when possible"#
                .to_string()
        }
    };

    // Add Chain of Thought instructions for models without native thinking
    if !supports_native_thinking {
        format!(
            "{}\n\n\
            ⚙️ THINKING PROCESS:\n\
            Before providing your final response, reason through the problem step by step.\n\
            Wrap your reasoning process in <thinking> tags like this:\n\
            <thinking>\n\
            1. First, I'll analyze the problem...\n\
            2. Then I need to consider...\n\
            3. The best approach is...\n\
            </thinking>\n\n\
            This ensures thorough analysis before taking action.",
            base_prompt
        )
    } else {
        base_prompt
    }
}

/// A single tool call log entry
#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolCallLog {
    pub tool: String,
    pub args: serde_json::Value,
    pub result_preview: String,
}

/// Response from the agent
#[derive(Debug)]
pub struct AgentResponse {
    pub text: String,
    pub tool_calls_made: usize,
    pub tool_call_log: Vec<ToolCallLog>,
    pub auto_compacted: bool,
    pub context_usage_percent: usize,
    pub usage: Option<crate::llm::TokenUsage>,
}

/// Result of manual context compaction
#[derive(Debug, Clone)]
pub struct CompactResult {
    /// The summary generated by the LLM
    pub summary: String,
    /// Token count before compaction
    pub old_tokens: usize,
    /// Token count after compaction
    pub new_tokens: usize,
    /// Message count before compaction
    pub old_messages: usize,
    /// Message count after compaction
    pub new_messages: usize,
}

/// Chat agent that can use tools to accomplish tasks
pub struct ChatAgent {
    llm: Arc<dyn LlmProvider>,
    tools: ToolRegistry,
    context: ConversationContext,
    max_iterations: usize,
    mode: AgentMode,
}

impl ChatAgent {
    pub fn new(llm: Arc<dyn LlmProvider>, tools: ToolRegistry) -> Self {
        Self::with_mode(llm, tools, AgentMode::Build)
    }

    pub fn with_mode(llm: Arc<dyn LlmProvider>, tools: ToolRegistry, mode: AgentMode) -> Self {
        let mut context = ConversationContext::new();
        let supports_thinking = llm.supports_native_thinking();
        context.add_system(get_system_prompt(mode, supports_thinking));

        Self {
            llm,
            tools,
            context,
            max_iterations: 10,
            mode,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    /// Update the agent's mode and tools while preserving conversation history
    pub fn update_mode(&mut self, tools: ToolRegistry, mode: AgentMode) {
        let tool_names: Vec<_> = tools.definitions().iter().map(|t| t.name.clone()).collect();
        tracing::info!(
            "Mode changed to {:?} - available tools: {:?}",
            mode,
            tool_names
        );
        self.tools = tools;
        self.mode = mode;
        // Update the system prompt in context (replace the first system message)
        let supports_thinking = self.llm.supports_native_thinking();
        self.context
            .update_system_prompt(&get_system_prompt(mode, supports_thinking));
    }

    /// Update just the LLM provider while preserving conversation history
    pub fn update_provider(&mut self, llm: Arc<dyn LlmProvider>) {
        self.llm = llm;
    }

    /// Update the max context tokens for the agent
    ///
    /// Call this when switching to a model with a different context window.
    /// If current context exceeds the new limit, it will be automatically trimmed.
    pub fn set_max_context_tokens(&mut self, max_tokens: usize) {
        tracing::info!(
            "Updating max context tokens: {} -> {}",
            self.context.max_context_tokens(),
            max_tokens
        );
        self.context.set_max_context_tokens(max_tokens);

        // Check if we're now near limit and need to compact
        if self.context.is_near_limit() {
            tracing::warn!(
                "Context at {}% after model switch, may need compaction",
                self.context.usage_percentage()
            );
        }
    }

    /// Get the current context usage percentage
    pub fn context_usage_percentage(&self) -> usize {
        self.context.usage_percentage()
    }

    /// Get estimated tokens in current context
    pub fn estimated_context_tokens(&self) -> usize {
        self.context.estimate_total_tokens()
    }

    /// Get the max context tokens for the current model
    pub fn max_context_tokens(&self) -> usize {
        self.context.max_context_tokens()
    }

    /// Check if context is near limit (80%+)
    pub fn is_context_near_limit(&self) -> bool {
        self.context.is_near_limit()
    }

    /// Get the current mode
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Clear conversation history (keeps system prompt)
    pub fn clear_history(&mut self) {
        // Get the current system prompt
        let supports_thinking = self.llm.supports_native_thinking();
        let system_prompt = get_system_prompt(self.mode, supports_thinking);

        // Clear and reinitialize with system prompt
        self.context.clear();
        self.context.add_system(system_prompt);

        tracing::info!("Conversation history cleared");
    }

    /// Restore conversation from a saved session
    pub fn restore_from_session(&mut self, session: &crate::storage::ChatSession) {
        // Clear existing history
        let supports_thinking = self.llm.supports_native_thinking();
        let system_prompt = get_system_prompt(self.mode, supports_thinking);
        self.context.clear();
        self.context.add_system(system_prompt);

        // Restore messages from session
        for msg in &session.messages {
            match msg.role.as_str() {
                "user" => self.context.add_user(&msg.content),
                "assistant" => self.context.add_assistant(&msg.content),
                "tool" => {
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        self.context.add_tool_result(tool_call_id, &msg.content);
                    }
                }
                _ => {}
            }
        }

        tracing::info!(
            "Restored {} messages from session '{}'",
            session.messages.len(),
            session.name
        );
    }

    /// Get current conversation messages for saving
    pub fn get_messages_for_session(&self) -> Vec<crate::storage::SessionMessage> {
        use crate::storage::SessionMessage;

        self.context
            .messages()
            .iter()
            .filter(|m| m.role != Role::System) // Don't save system prompt
            .filter_map(|m| {
                let role = match m.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    Role::Tool => "tool",
                    Role::System => return None,
                };

                let content = match &m.content {
                    MessageContent::Text(t) => t.clone(),
                    MessageContent::Parts(parts) => {
                        // Extract text from parts
                        parts
                            .iter()
                            .map(|p| match p {
                                ContentPart::Text { text } => text.clone(),
                                ContentPart::ToolUse { name, input, .. } => {
                                    format!("[Tool: {} with {:?}]", name, input)
                                }
                                ContentPart::ToolResult { content, .. } => content.clone(),
                            })
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                };

                Some(SessionMessage {
                    role: role.to_string(),
                    content,
                    timestamp: chrono::Utc::now(),
                    tool_call_id: m.tool_call_id.clone(),
                })
            })
            .collect()
    }

    /// Auto-compact context by summarizing older messages
    async fn auto_compact(&mut self) -> Result<()> {
        let messages = self.context.messages();

        // Need at least 4 messages to compact (system + some history)
        if messages.len() < 4 {
            return Ok(());
        }

        // Build a summary request from the conversation
        let mut summary_content = String::from(
            "Summarize this conversation in 2-3 paragraphs, focusing on:\n\
             1. What the user asked for\n\
             2. What actions were taken (files read/modified, commands run)\n\
             3. Current state and any pending tasks\n\n\
             Conversation:\n",
        );

        // Collect messages to summarize (skip system prompt, keep recent 2 exchanges)
        let keep_recent = 4; // Keep last 2 user+assistant pairs
        let to_summarize = messages.len().saturating_sub(keep_recent + 1); // +1 for system

        if to_summarize < 2 {
            return Ok(()); // Not enough to summarize
        }

        for (_i, msg) in messages.iter().enumerate().skip(1).take(to_summarize) {
            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => continue,
                Role::Tool => "Tool Result",
            };

            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => {
                    parts
                        .iter()
                        .filter_map(|p| match p {
                            ContentPart::Text { text } => Some(text.clone()),
                            ContentPart::ToolResult { content, .. } => {
                                // Truncate tool results in summary
                                Some(if content.len() > 200 {
                                    format!("{}...(truncated)", &content[..200])
                                } else {
                                    content.clone()
                                })
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            };

            // Truncate long messages
            let truncated = if content.len() > 500 {
                format!("{}...(truncated)", &content[..500])
            } else {
                content
            };

            summary_content.push_str(&format!("\n{}: {}\n", role, truncated));
        }

        // Ask LLM to summarize
        let summary_messages = vec![Message::user(summary_content)];

        match self.llm.chat(&summary_messages, None).await {
            Ok(LlmResponse::Text { text: summary, .. }) => {
                // Compact the context with the summary
                self.context.compact_with_summary(&summary, keep_recent);
                tracing::info!(
                    "Auto-compacted context. New size: ~{} tokens",
                    self.context.estimate_total_tokens()
                );
            }
            Ok(_) => {
                tracing::warn!("Auto-compact got unexpected response type");
            }
            Err(e) => {
                tracing::warn!("Auto-compact failed: {}, falling back to truncation", e);
                // Fallback: just trim old messages
                self.context.trim_to_recent(keep_recent);
            }
        }

        Ok(())
    }

    /// Manually compact the conversation context by summarizing older messages.
    /// Returns the summary text if compaction was performed, or None if not enough messages.
    pub async fn compact(&mut self) -> Result<Option<CompactResult>> {
        let messages = self.context.messages();
        let old_tokens = self.context.estimate_total_tokens();
        let old_messages = messages.len();

        // Need at least 4 messages to compact (system + some history)
        if messages.len() < 4 {
            return Ok(None);
        }

        // Build a summary request from the conversation
        let mut summary_content = String::from(
            "Summarize this conversation concisely, focusing on:\n\
             1. What the user asked for\n\
             2. What actions were taken (files read/modified, commands run)\n\
             3. Current state and any pending tasks\n\n\
             Conversation:\n",
        );

        // Collect messages to summarize (skip system prompt, keep recent 2 exchanges)
        let keep_recent = 4; // Keep last 2 user+assistant pairs
        let to_summarize = messages.len().saturating_sub(keep_recent + 1); // +1 for system

        if to_summarize < 2 {
            return Ok(None); // Not enough to summarize
        }

        for (_i, msg) in messages.iter().enumerate().skip(1).take(to_summarize) {
            let role = match msg.role {
                Role::User => "User",
                Role::Assistant => "Assistant",
                Role::System => continue,
                Role::Tool => "Tool Result",
            };

            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        ContentPart::Text { text } => Some(text.clone()),
                        ContentPart::ToolResult { content, .. } => Some(if content.len() > 200 {
                            format!("{}...(truncated)", &content[..200])
                        } else {
                            content.clone()
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };

            let truncated = if content.len() > 500 {
                format!("{}...(truncated)", &content[..500])
            } else {
                content
            };

            summary_content.push_str(&format!("\n{}: {}\n", role, truncated));
        }

        // Ask LLM to summarize
        let summary_messages = vec![Message::user(summary_content)];

        match self.llm.chat(&summary_messages, None).await {
            Ok(LlmResponse::Text { text: summary, .. }) => {
                self.context.compact_with_summary(&summary, keep_recent);
                let new_tokens = self.context.estimate_total_tokens();
                let new_messages = self.context.messages().len();
                tracing::info!(
                    "Manual compact: {} -> {} tokens, {} -> {} messages",
                    old_tokens,
                    new_tokens,
                    old_messages,
                    new_messages
                );
                Ok(Some(CompactResult {
                    summary,
                    old_tokens,
                    new_tokens,
                    old_messages,
                    new_messages,
                }))
            }
            Ok(_) => {
                anyhow::bail!("Unexpected response type from LLM during compaction");
            }
            Err(e) => {
                tracing::warn!("Compact failed: {}, falling back to truncation", e);
                self.context.trim_to_recent(keep_recent);
                let new_tokens = self.context.estimate_total_tokens();
                let new_messages = self.context.messages().len();
                Ok(Some(CompactResult {
                    summary: "(Compacted by truncation - LLM summarization failed)".to_string(),
                    old_tokens,
                    new_tokens,
                    old_messages,
                    new_messages,
                }))
            }
        }
    }

    /// Process a user message and return the agent's response
    pub async fn chat(&mut self, user_message: &str) -> Result<AgentResponse> {
        // Track if auto-compaction happens
        let mut auto_compacted = false;

        // Auto-compact if context is near limit (80%+)
        if self.context.is_near_limit() {
            let usage = self.context.usage_percentage();
            tracing::info!("Context at {}%, triggering auto-compaction...", usage);
            self.auto_compact().await?;
            auto_compacted = true;
        }

        self.context.add_user(user_message);

        let tool_definitions = self.tools.definitions();
        let mut iterations = 0;
        let mut total_tool_calls = 0;
        let mut tool_call_log: Vec<ToolCallLog> = Vec::new();
        let mut accumulated_usage = crate::llm::TokenUsage::default();

        loop {
            if iterations >= self.max_iterations {
                self.context.add_assistant(
                    "I've reached the maximum number of steps. Here's what I've done so far. Let me know if you'd like me to continue.",
                );
                break;
            }

            // Check context size before each LLM call
            let estimated_tokens = self.context.estimate_total_tokens();
            tracing::debug!("Context size: ~{} tokens", estimated_tokens);

            let response = self
                .llm
                .chat(self.context.messages(), Some(&tool_definitions))
                .await?;

            // Accumulate usage from this response
            if let Some(usage) = response.usage() {
                accumulated_usage.input_tokens += usage.input_tokens;
                accumulated_usage.output_tokens += usage.output_tokens;
                accumulated_usage.total_tokens += usage.total_tokens;
            }

            iterations += 1;

            match response {
                LlmResponse::Text { text, .. } => {
                    self.context.add_assistant(&text);
                    let context_usage_percent = self.context.usage_percentage();
                    return Ok(AgentResponse {
                        text,
                        tool_calls_made: total_tool_calls,
                        tool_call_log,
                        auto_compacted,
                        context_usage_percent,
                        usage: Some(accumulated_usage),
                    });
                }
                LlmResponse::ToolCalls { calls, .. } => {
                    total_tool_calls += calls.len();

                    // First, add the assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(&calls);

                    // Execute each tool call and add results
                    for (i, call) in calls.iter().enumerate() {
                        // Update status with current tool and argument
                        let tool_arg = match &call.name[..] {
                            "grep" | "file_search" => call
                                .arguments
                                .get("pattern")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "read_file" | "write_file" | "delete_file" => call
                                .arguments
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "read_files" => call
                                .arguments
                                .get("paths")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    let files: Vec<_> =
                                        arr.iter().filter_map(|v| v.as_str()).take(3).collect();
                                    if arr.len() > 3 {
                                        format!("{} +{} more", files.join(", "), arr.len() - 3)
                                    } else {
                                        files.join(", ")
                                    }
                                }),
                            "list_directory" | "codebase_overview" => call
                                .arguments
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "shell" => call
                                .arguments
                                .get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            _ => None,
                        };

                        let action = match &call.name[..] {
                            "grep" => "Grepping",
                            "file_search" => "Searching",
                            "read_file" | "read_files" => "Reading",
                            "write_file" | "patch_file" => "Writing",
                            "delete_file" => "Deleting",
                            "list_directory" => "Listing",
                            "codebase_overview" => "Analyzing",
                            "find_references" => "Finding references",
                            "shell" => "Executing",
                            _ => "Processing",
                        };

                        crate::transport::update_status(
                            action,
                            Some(&call.name),
                            tool_arg.as_deref(),
                            total_tool_calls + i + 1,
                        )
                        .await;

                        tracing::debug!(
                            "Executing tool: {} with args: {} (mode: {:?})",
                            call.name,
                            call.arguments,
                            self.mode
                        );

                        // Execute tool - registry isolation ensures only available tools can run
                        // If tool doesn't exist in registry, execute() returns "Unknown tool" error
                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        // Log if a tool was rejected (helps debug hallucinated tool calls)
                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        tracing::debug!("Tool result: {:?}", result);

                        // Log the tool call
                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        // Add tool result to context
                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
                LlmResponse::Mixed {
                    text, tool_calls, ..
                } => {
                    if tool_calls.is_empty() {
                        // No tool calls, just add text
                        if let Some(text) = text {
                            self.context.add_assistant(&text);
                        }

                        let last_text = self
                            .context
                            .messages()
                            .last()
                            .and_then(|m| m.content.as_text())
                            .unwrap_or("Done.")
                            .to_string();

                        let context_usage_percent = self.context.usage_percentage();
                        return Ok(AgentResponse {
                            text: last_text,
                            tool_calls_made: total_tool_calls,
                            tool_call_log,
                            auto_compacted,
                            context_usage_percent,
                            usage: Some(accumulated_usage),
                        });
                    }

                    total_tool_calls += tool_calls.len();

                    // Add assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(&tool_calls);

                    // Execute tool calls
                    for (i, call) in tool_calls.iter().enumerate() {
                        // Update status with current tool
                        let tool_arg = match &call.name[..] {
                            "grep" | "file_search" => call
                                .arguments
                                .get("pattern")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "read_file" | "write_file" | "delete_file" => call
                                .arguments
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "read_files" => call
                                .arguments
                                .get("paths")
                                .and_then(|v| v.as_array())
                                .map(|a| format!("{} files", a.len())),
                            "list_directory" => call
                                .arguments
                                .get("path")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "find_references" => call
                                .arguments
                                .get("symbol")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            "codebase_overview" => Some("project".to_string()),
                            "shell" => call
                                .arguments
                                .get("command")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            _ => None,
                        };

                        let action = match &call.name[..] {
                            "grep" => "Searching",
                            "file_search" => "Finding files",
                            "read_file" | "read_files" => "Reading",
                            "write_file" => "Writing",
                            "delete_file" => "Deleting",
                            "list_directory" => "Listing",
                            "find_references" => "Tracing",
                            "codebase_overview" => "Analyzing",
                            "shell" => "Running",
                            "patch_file" => "Patching",
                            _ => "Processing",
                        };

                        update_status(
                            action,
                            Some(&call.name),
                            tool_arg.as_deref(),
                            total_tool_calls + i + 1,
                        )
                        .await;

                        tracing::debug!(
                            "Executing tool: {} with args: {} (mode: {:?})",
                            call.name,
                            call.arguments,
                            self.mode
                        );

                        // Execute tool - registry isolation ensures only available tools can run
                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        // Log if a tool was rejected
                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        tracing::debug!("Tool result: {:?}", result);

                        // Log the tool call
                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
            }
        }

        let last_text = self
            .context
            .messages()
            .last()
            .and_then(|m| m.content.as_text())
            .unwrap_or("Done.")
            .to_string();

        let context_usage_percent = self.context.usage_percentage();
        Ok(AgentResponse {
            text: last_text,
            tool_calls_made: total_tool_calls,
            tool_call_log,
            auto_compacted,
            context_usage_percent,
            usage: Some(accumulated_usage),
        })
    }

    /// Clear conversation history (keeps system prompt)
    pub fn reset(&mut self) {
        self.context.clear();
        let supports_thinking = self.llm.supports_native_thinking();
        self.context
            .add_system(get_system_prompt(self.mode, supports_thinking));
    }

    /// Process a user message with interrupt checking
    /// The interrupt_check function is called before each LLM call and tool execution
    pub async fn chat_with_interrupt<F>(
        &mut self,
        user_message: &str,
        interrupt_check: F,
    ) -> Result<AgentResponse>
    where
        F: Fn() -> bool + Send + Sync,
    {
        // Track if auto-compaction happens
        let mut auto_compacted = false;

        // Check for interrupt before starting
        if interrupt_check() {
            return Ok(AgentResponse {
                text: "⚠️ *Operation interrupted*".to_string(),
                tool_calls_made: 0,
                tool_call_log: vec![],
                auto_compacted: false,
                context_usage_percent: self.context.usage_percentage(),
                usage: None,
            });
        }

        // Auto-compact if context is near limit (80%+)
        if self.context.is_near_limit() {
            let usage = self.context.usage_percentage();
            tracing::info!("Context at {}%, triggering auto-compaction...", usage);
            self.auto_compact().await?;
            auto_compacted = true;
        }

        self.context.add_user(user_message);

        let tool_definitions = self.tools.definitions();
        let mut iterations = 0;
        let mut total_tool_calls = 0;
        let mut tool_call_log: Vec<ToolCallLog> = Vec::new();
        let mut accumulated_usage = crate::llm::TokenUsage::default();

        loop {
            // Check for interrupt at start of each iteration
            if interrupt_check() {
                tracing::info!("Agent interrupted during processing");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return Ok(AgentResponse {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    tool_calls_made: total_tool_calls,
                    tool_call_log,
                    auto_compacted,
                    context_usage_percent: self.context.usage_percentage(),
                    usage: Some(accumulated_usage),
                });
            }

            if iterations >= self.max_iterations {
                self.context.add_assistant(
                    "I've reached the maximum number of steps. Here's what I've done so far. Let me know if you'd like me to continue.",
                );
                break;
            }

            // Check context size before each LLM call
            let estimated_tokens = self.context.estimate_total_tokens();
            tracing::debug!("Context size: ~{} tokens", estimated_tokens);

            let response = self
                .llm
                .chat(self.context.messages(), Some(&tool_definitions))
                .await?;

            // Check for interrupt after LLM response
            if interrupt_check() {
                tracing::info!("Agent interrupted after LLM response");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return Ok(AgentResponse {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    tool_calls_made: total_tool_calls,
                    tool_call_log,
                    auto_compacted,
                    context_usage_percent: self.context.usage_percentage(),
                    usage: Some(accumulated_usage),
                });
            }

            // Accumulate usage from this response
            if let Some(usage) = response.usage() {
                accumulated_usage.input_tokens += usage.input_tokens;
                accumulated_usage.output_tokens += usage.output_tokens;
                accumulated_usage.total_tokens += usage.total_tokens;
            }

            iterations += 1;

            match response {
                LlmResponse::Text { text, .. } => {
                    self.context.add_assistant(&text);
                    let context_usage_percent = self.context.usage_percentage();
                    return Ok(AgentResponse {
                        text,
                        tool_calls_made: total_tool_calls,
                        tool_call_log,
                        auto_compacted,
                        context_usage_percent,
                        usage: Some(accumulated_usage),
                    });
                }
                LlmResponse::ToolCalls { calls, .. } => {
                    total_tool_calls += calls.len();

                    // First, add the assistant message with tool calls
                    self.context.add_assistant_tool_calls(&calls);

                    // Execute each tool call with interrupt checking
                    for (i, call) in calls.iter().enumerate() {
                        // Check for interrupt before each tool execution
                        if interrupt_check() {
                            tracing::info!(
                                "Agent interrupted before tool execution: {}",
                                call.name
                            );
                            self.context
                                .add_tool_result(&call.id, "⚠️ Interrupted by user");
                            return Ok(AgentResponse {
                                text: format!(
                                    "⚠️ *Operation interrupted before executing {}*",
                                    call.name
                                ),
                                tool_calls_made: total_tool_calls,
                                tool_call_log,
                                auto_compacted,
                                context_usage_percent: self.context.usage_percentage(),
                                usage: Some(accumulated_usage),
                            });
                        }

                        // Update status
                        let tool_arg = call
                            .arguments
                            .get("path")
                            .or_else(|| call.arguments.get("pattern"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        update_status(
                            &format!("[{}/{}] {}", i + 1, calls.len(), call.name),
                            tool_arg.as_deref(),
                            None,
                            0,
                        )
                        .await;

                        tracing::debug!(
                            "Executing tool: {} with args: {} (mode: {:?})",
                            call.name,
                            call.arguments,
                            self.mode
                        );

                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        // Log the tool call
                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
                LlmResponse::Mixed {
                    text, tool_calls, ..
                } => {
                    if tool_calls.is_empty() {
                        if let Some(text) = text {
                            self.context.add_assistant(&text);
                        }

                        let last_text = self
                            .context
                            .messages()
                            .last()
                            .and_then(|m| m.content.as_text())
                            .unwrap_or("Done.")
                            .to_string();

                        let context_usage_percent = self.context.usage_percentage();
                        return Ok(AgentResponse {
                            text: last_text,
                            tool_calls_made: total_tool_calls,
                            tool_call_log,
                            auto_compacted,
                            context_usage_percent,
                            usage: Some(accumulated_usage),
                        });
                    }

                    total_tool_calls += tool_calls.len();
                    self.context.add_assistant_tool_calls(&tool_calls);

                    // Execute tool calls with interrupt checking
                    for (i, call) in tool_calls.iter().enumerate() {
                        if interrupt_check() {
                            tracing::info!(
                                "Agent interrupted before tool execution: {}",
                                call.name
                            );
                            self.context
                                .add_tool_result(&call.id, "⚠️ Interrupted by user");
                            return Ok(AgentResponse {
                                text: format!(
                                    "⚠️ *Operation interrupted before executing {}*",
                                    call.name
                                ),
                                tool_calls_made: total_tool_calls,
                                tool_call_log,
                                auto_compacted,
                                context_usage_percent: self.context.usage_percentage(),
                                usage: Some(accumulated_usage),
                            });
                        }

                        let tool_arg = call
                            .arguments
                            .get("path")
                            .or_else(|| call.arguments.get("pattern"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        update_status(
                            &format!("[{}/{}] {}", i + 1, tool_calls.len(), call.name),
                            tool_arg.as_deref(),
                            None,
                            0,
                        )
                        .await;

                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
            }
        }

        let last_text = self
            .context
            .messages()
            .last()
            .and_then(|m| m.content.as_text())
            .unwrap_or("Done.")
            .to_string();

        let context_usage_percent = self.context.usage_percentage();
        Ok(AgentResponse {
            text: last_text,
            tool_calls_made: total_tool_calls,
            tool_call_log,
            auto_compacted,
            context_usage_percent,
            usage: Some(accumulated_usage),
        })
    }

    /// Process a user message with streaming callbacks and interrupt checking
    ///
    /// This method enables real-time streaming of LLM responses. The callbacks
    /// are invoked as each chunk of text or thinking content arrives.
    ///
    /// # Arguments
    /// * `user_message` - The user's input message
    /// * `interrupt_check` - Function called to check if processing should stop
    /// * `on_text` - Callback for each text chunk from the assistant
    /// * `on_thinking` - Callback for each thinking/reasoning chunk
    /// * `on_tool_call` - Callback when a tool call starts (name, args preview)
    pub async fn chat_streaming<F, T, K, C>(
        &mut self,
        user_message: &str,
        interrupt_check: F,
        on_text: T,
        on_thinking: K,
        on_tool_call: C,
    ) -> Result<AgentResponse>
    where
        F: Fn() -> bool + Send + Sync,
        T: Fn(String) + Send + Sync + 'static,
        K: Fn(String) + Send + Sync + 'static,
        C: Fn(String, String) + Send + Sync + 'static,
    {
        // Wrap callbacks in Arc so they can be shared with the streaming callback
        let on_text = std::sync::Arc::new(on_text);
        let on_thinking = std::sync::Arc::new(on_thinking);

        // Track if auto-compaction happens
        let mut auto_compacted = false;

        // Check for interrupt before starting
        if interrupt_check() {
            return Ok(AgentResponse {
                text: "⚠️ *Operation interrupted*".to_string(),
                tool_calls_made: 0,
                tool_call_log: vec![],
                auto_compacted: false,
                context_usage_percent: self.context.usage_percentage(),
                usage: None,
            });
        }

        // Auto-compact if context is near limit (80%+)
        if self.context.is_near_limit() {
            let usage = self.context.usage_percentage();
            tracing::info!("Context at {}%, triggering auto-compaction...", usage);
            self.auto_compact().await?;
            auto_compacted = true;
        }

        self.context.add_user(user_message);

        let tool_definitions = self.tools.definitions();
        let mut iterations = 0;
        let mut total_tool_calls = 0;
        let mut tool_call_log: Vec<ToolCallLog> = Vec::new();
        let mut accumulated_usage = crate::llm::TokenUsage::default();
        let mut accumulated_text = String::new();

        loop {
            // Check for interrupt at start of each iteration
            if interrupt_check() {
                tracing::info!("Agent interrupted during processing");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return Ok(AgentResponse {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    tool_calls_made: total_tool_calls,
                    tool_call_log,
                    auto_compacted,
                    context_usage_percent: self.context.usage_percentage(),
                    usage: Some(accumulated_usage),
                });
            }

            if iterations >= self.max_iterations {
                self.context.add_assistant(
                    "I've reached the maximum number of steps. Here's what I've done so far. Let me know if you'd like me to continue.",
                );
                break;
            }

            // Reset accumulated text for this iteration
            accumulated_text.clear();

            // Wrap accumulated_text in Arc<Mutex> so we can share it with the callback
            let accumulated_text_shared = std::sync::Arc::new(std::sync::Mutex::new(String::new()));

            // Create streaming callback that IMMEDIATELY forwards events to the UI
            // This is the key to real-time streaming - events are forwarded as they arrive
            let on_text_clone = on_text.clone();
            let on_thinking_clone = on_thinking.clone();
            let accumulated_clone = accumulated_text_shared.clone();

            let callback: StreamCallback = Box::new(move |event: StreamEvent| {
                match event {
                    StreamEvent::TextDelta(text) => {
                        // Accumulate for context
                        if let Ok(mut acc) = accumulated_clone.lock() {
                            acc.push_str(&text);
                        }
                        // IMMEDIATELY forward to UI - this is the key fix!
                        on_text_clone(text);
                    }
                    StreamEvent::ThinkingDelta(thinking) => {
                        // IMMEDIATELY forward thinking to UI
                        on_thinking_clone(thinking);
                    }
                    StreamEvent::ToolCallStart { name, .. } => {
                        tracing::debug!("Tool call starting: {}", name);
                    }
                    StreamEvent::Error(err) => {
                        tracing::error!("Streaming error: {}", err);
                    }
                    _ => {}
                }
            });

            // Call LLM with streaming - callbacks fire in real-time during this await
            let interrupt_check_ref: &(dyn Fn() -> bool + Send + Sync) = &interrupt_check;
            let response = self
                .llm
                .chat_streaming(
                    self.context.messages(),
                    Some(&tool_definitions),
                    callback,
                    Some(interrupt_check_ref),
                )
                .await;

            // Get accumulated text from shared state
            if let Ok(acc) = accumulated_text_shared.lock() {
                accumulated_text = acc.clone();
            }

            // If the user interrupted during streaming, stop immediately without
            // incorporating partial output into the agent's context.
            if interrupt_check() {
                tracing::info!("Agent interrupted during streaming");
                self.context
                    .add_assistant("⚠️ *Operation interrupted by user*");
                return Ok(AgentResponse {
                    text: "⚠️ *Operation interrupted by user*".to_string(),
                    tool_calls_made: total_tool_calls,
                    tool_call_log,
                    auto_compacted,
                    context_usage_percent: self.context.usage_percentage(),
                    usage: Some(accumulated_usage),
                });
            }

            let response = response?;

            // Accumulate usage from this response
            if let Some(usage) = response.usage() {
                accumulated_usage.input_tokens += usage.input_tokens;
                accumulated_usage.output_tokens += usage.output_tokens;
                accumulated_usage.total_tokens += usage.total_tokens;
            }

            iterations += 1;

            match response {
                LlmResponse::Text { text, .. } => {
                    // Use accumulated_text if available (from streaming), otherwise use final text
                    let final_text = if accumulated_text.is_empty() {
                        text
                    } else {
                        accumulated_text.clone()
                    };
                    self.context.add_assistant(&final_text);
                    let context_usage_percent = self.context.usage_percentage();
                    return Ok(AgentResponse {
                        text: final_text,
                        tool_calls_made: total_tool_calls,
                        tool_call_log,
                        auto_compacted,
                        context_usage_percent,
                        usage: Some(accumulated_usage),
                    });
                }
                LlmResponse::ToolCalls { calls, .. } => {
                    total_tool_calls += calls.len();

                    // First, add the assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(&calls);

                    // Execute each tool call and add results
                    for call in calls.iter() {
                        // Check for interrupt before each tool
                        if interrupt_check() {
                            tracing::info!("Agent interrupted before tool: {}", call.name);
                            self.context
                                .add_assistant("⚠️ *Operation interrupted by user*");
                            return Ok(AgentResponse {
                                text: "⚠️ *Operation interrupted by user*".to_string(),
                                tool_calls_made: total_tool_calls,
                                tool_call_log,
                                auto_compacted,
                                context_usage_percent: self.context.usage_percentage(),
                                usage: Some(accumulated_usage),
                            });
                        }

                        // Notify about tool call
                        let args_preview = serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string());
                        on_tool_call(call.name.clone(), args_preview);

                        tracing::info!(
                            "Executing tool: {} with args: {} (mode: {:?})",
                            call.name,
                            call.arguments,
                            self.mode
                        );

                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
                LlmResponse::Mixed {
                    text, tool_calls, ..
                } => {
                    // Handle mixed response with both text and tool calls
                    if let Some(t) = &text {
                        // Text was already streamed, just log it
                        tracing::debug!("Mixed response text: {}", t);
                    }

                    total_tool_calls += tool_calls.len();
                    self.context.add_assistant_tool_calls(&tool_calls);

                    for call in &tool_calls {
                        if interrupt_check() {
                            tracing::info!("Agent interrupted before tool: {}", call.name);
                            self.context
                                .add_assistant("⚠️ *Operation interrupted by user*");
                            return Ok(AgentResponse {
                                text: "⚠️ *Operation interrupted by user*".to_string(),
                                tool_calls_made: total_tool_calls,
                                tool_call_log,
                                auto_compacted,
                                context_usage_percent: self.context.usage_percentage(),
                                usage: Some(accumulated_usage),
                            });
                        }

                        let args_preview = serde_json::to_string(&call.arguments)
                            .unwrap_or_else(|_| "{}".to_string());
                        on_tool_call(call.name.clone(), args_preview);

                        let result = self
                            .tools
                            .execute(&call.name, call.arguments.clone())
                            .await?;

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", &result.output[..200])
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview,
                        });

                        self.context.add_tool_result(&call.id, &result.output);
                    }
                }
            }
        }

        let last_text = self
            .context
            .messages()
            .last()
            .and_then(|m| m.content.as_text())
            .unwrap_or("Done.")
            .to_string();

        let context_usage_percent = self.context.usage_percentage();
        Ok(AgentResponse {
            text: last_text,
            tool_calls_made: total_tool_calls,
            tool_call_log,
            auto_compacted,
            context_usage_percent,
            usage: Some(accumulated_usage),
        })
    }
}
