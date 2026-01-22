//! Chat agent implementation with tool execution loop

#![allow(dead_code)]

use super::ConversationContext;
use crate::llm::{
    ContentPart, LlmProvider, LlmResponse, Message, MessageContent, Role, StreamCallback,
    StreamEvent, ThinkSettings,
};
use crate::services::PlanService;
use crate::storage::ExecutionPlan;
use crate::tools::{AgentMode, ToolRegistry};
use crate::transport::update_status;
use anyhow::Result;
use std::sync::Arc;

/// Lightweight plan context for system prompt injection in Build mode
///
/// This captures the essential information needed to keep the agent
/// focused on the current plan task.
#[derive(Debug, Clone)]
pub struct PlanContext {
    /// Plan title
    pub title: String,
    /// Completed tasks count
    pub completed: usize,
    /// Total tasks count
    pub total: usize,
    /// Current task index (0-based)
    pub current_task_index: usize,
    /// Current task description
    pub current_task_description: String,
    /// Files associated with current task
    pub current_task_files: Vec<String>,
    /// Remaining task descriptions (limited to 5)
    pub remaining_tasks: Vec<String>,
}

impl PlanContext {
    /// Create PlanContext from an ExecutionPlan
    ///
    /// Returns None if the plan has no pending tasks.
    pub fn from_plan(plan: &ExecutionPlan) -> Option<Self> {
        let (completed, total) = plan.progress();
        let (task_idx, _subtask_idx) = plan.get_next_pending()?;
        let task = plan.tasks.get(task_idx)?;

        // Get remaining tasks (up to 5)
        let remaining_tasks: Vec<_> = plan.tasks[task_idx..]
            .iter()
            .filter(|t| !t.is_complete())
            .take(5)
            .map(|t| t.description.clone())
            .collect();

        Some(Self {
            title: plan.title.clone(),
            completed,
            total,
            current_task_index: task_idx,
            current_task_description: task.description.clone(),
            current_task_files: task.files.clone(),
            remaining_tasks,
        })
    }

    /// Format plan context as markdown for system prompt injection
    pub fn to_prompt_section(&self) -> String {
        let mut section = format!(
            r#"
═══════════════════════════════════════════════════════════
📋 ACTIVE PLAN: {}
───────────────────────────────────────────────────────────
Progress: {}/{} tasks complete

🎯 CURRENT TASK (Task {}):
{}"#,
            self.title,
            self.completed,
            self.total,
            self.current_task_index + 1,
            self.current_task_description
        );

        if !self.current_task_files.is_empty() {
            section.push_str(&format!(
                "\n   Files: {}",
                self.current_task_files.join(", ")
            ));
        }

        if self.remaining_tasks.len() > 1 {
            section.push_str("\n\n📝 Remaining Tasks:");
            for (i, task) in self.remaining_tasks.iter().skip(1).enumerate() {
                section.push_str(&format!(
                    "\n   {}. {}",
                    self.current_task_index + i + 2,
                    task
                ));
            }
        }

        section.push_str(&format!(
            r#"

⚠️ PLAN EXECUTION RULES:
1. Focus on the CURRENT TASK above
2. When done, call mark_task_done(task_index={}, summary="...")
3. Do NOT skip ahead to other tasks
4. If blocked, explain why and use ask_user
═══════════════════════════════════════════════════════════
"#,
            self.current_task_index
        ));

        section
    }
}

/// Safely truncate a string at a character boundary
///
/// This avoids panics when truncating UTF-8 strings with multi-byte characters
/// like emojis (📄 is 4 bytes).
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Generate system prompt based on agent mode, thinking capability, and thinking state
///
/// # Arguments
/// * `mode` - The agent mode (Build, Plan, Review, Ask)
/// * `supports_native_thinking` - Whether the model has native thinking API support
/// * `thinking_enabled` - Whether thinking is enabled via /think command
/// * `trust_level` - Current trust level for risky operations (only shown in Build mode)
/// * `plan_context` - Optional active plan context (only used in Build mode)
fn get_system_prompt(
    mode: AgentMode,
    supports_native_thinking: bool,
    thinking_enabled: bool,
    trust_level: crate::tools::TrustLevel,
    plan_context: Option<&PlanContext>,
) -> String {
    // Build status header so agent knows its current context
    // Only show trust level in Build mode where it has effect
    let status_header = if mode == AgentMode::Build {
        let mut header = format!(
            r#"═══════════════════════════════════════════════════════════
📊 CURRENT STATUS
───────────────────────────────────────────────────────────
Mode: {} ({})
Trust: {} - {}
═══════════════════════════════════════════════════════════

"#,
            mode.label(),
            "full access, can modify files",
            trust_level.label(),
            trust_level.description()
        );

        // Inject plan context in Build mode if active
        if let Some(ctx) = plan_context {
            header.push_str(&ctx.to_prompt_section());
            header.push('\n');
        }

        header
    } else {
        format!(
            r#"═══════════════════════════════════════════════════════════
📊 CURRENT STATUS
───────────────────────────────────────────────────────────
Mode: {} ({})
═══════════════════════════════════════════════════════════

"#,
            mode.label(),
            match mode {
                AgentMode::Plan => "read-only, can propose changes",
                AgentMode::Ask => "read-only, Q&A focus",
                AgentMode::Build => "full access, can modify files",
            }
        )
    };

    let base_prompt = match mode {
        AgentMode::Plan => r#"You are an AI coding assistant in PLAN MODE (READ-ONLY).

⚠️ You CANNOT make changes. You can ONLY explore, analyze, and CREATE EXECUTION PLANS.

🚀 CRITICAL RULES:
1. USE TOOLS IMMEDIATELY - Don't explain, just search and explore
2. DISCOVER FIRST - Before planning, understand the codebase thoroughly
3. NEVER GIVE UP - Try 3-4 different search strategies before saying "not found"
4. CREATE PLANS - Use `save_plan` for multi-step tasks
5. 💬 NEVER ASK QUESTIONS IN CHAT TEXT - Use ask_user tool for ALL user input!

═══════════════════════════════════════════════════════════
🔍 DISCOVERY WORKFLOW - DO THIS BEFORE CREATING ANY PLAN
═══════════════════════════════════════════════════════════

Before creating a plan, you MUST understand the codebase:

1. **Project Structure**: Use `list_directory` on root and key directories
2. **Tech Stack Detection**: Look for config files (Cargo.toml, package.json, pyproject.toml, go.mod, pom.xml, build.sbt, build.zig, etc.)
3. **Architecture Understanding**: Read main entry points, module structure, key abstractions
4. **Coding Patterns**: Identify conventions (naming, error handling, logging, testing patterns)
5. **Test Coverage**: Find test files/directories (tests/, __tests__/, *_test.*, *_spec.*)
6. **Documentation**: Check for README.md, docs/, architecture docs

DETECT THE TECH STACK (examples):
- Rust: Cargo.toml → cargo build, cargo test
- Python: pyproject.toml/requirements.txt → pip, pytest
- TypeScript/JS: package.json → npm/yarn/pnpm/bun, jest/vitest
- Go: go.mod → go build, go test
- Java: pom.xml/build.gradle → maven/gradle, junit
- Scala: build.sbt → sbt compile, sbt test
- Zig: build.zig → zig build, zig test
- Ruby: Gemfile → bundle, rspec
- C/C++: Makefile/CMakeLists.txt → make, cmake
- If unknown: use your knowledge to identify patterns

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
- shell: 🔒 Safe read-only commands (git status, ls, cat, grep, version checks)
- ask_user: 💬 Ask user structured questions via popup (single-select, multi-select, free-text)
- switch_mode: 🔄 Request mode switch (ask/plan/build) with user confirmation popup
- save_plan: 📋 Create/update structured execution plans with tasks, subtasks, files
- preview_plan: 👁️ Preview a plan before saving (for user review)
- update_plan: ✏️ Modify an existing plan (add tasks, update sections)
- get_plan_status: 📊 Check current plan progress
- mark_task_done: ✅ Mark a task or subtask as completed

You do NOT have: write_file, patch_file, delete_file (shell is limited to safe commands)

═══════════════════════════════════════════════════════════
📋 EXECUTION PLANS - USE `save_plan` TOOL
═══════════════════════════════════════════════════════════

⚠️ IMPORTANT: When user asks to "create a plan", "save plan", or requests task planning, ALWAYS use the `save_plan` tool!
- DO NOT use `propose_change` to create markdown files for plans
- DO NOT suggest creating docs/plan.md files
- The `save_plan` tool stores structured plans in .tark/sessions/ with proper tracking

When to use `save_plan`:
- User says "create a plan", "save the plan", "make a plan"
- Feature implementation (3+ steps)
- Bug fixes requiring multiple changes
- Refactoring across files
- Any request where sequence matters

Example `save_plan` call with full schema:
```json
{
  "title": "Add user authentication",
  "overview": "Implement JWT-based authentication with login, logout, and middleware",
  "architecture": "Add auth module under src/auth/, integrate with existing user model",
  "proposed_changes": "New files: auth/mod.rs, auth/jwt.rs, auth/middleware.rs. Modified: routes.rs, user.rs",
  "acceptance_criteria": ["Users can register", "Users can log in", "Protected routes require auth", "Tests pass"],
  "tasks": [
    {"description": "Create User model and database schema", "files": ["src/models/user.rs", "migrations/"]},
    {"description": "Implement login endpoint", "subtasks": ["Validate credentials", "Generate JWT token"], "files": ["src/auth/mod.rs"]},
    {"description": "Add auth middleware", "files": ["src/auth/middleware.rs", "src/routes.rs"]}
  ],
  "tech_stack_language": "rust",
  "tech_stack_framework": "axum",
  "tech_stack_test_command": "cargo test",
  "tech_stack_build_command": "cargo build"
}
```

After creating a plan, user can switch to /build mode to execute. Use `mark_task_done` to track progress.

🔬 CODE UNDERSTANDING STRATEGY:
1. Use list_symbols to see what's in a file (functions, classes, types)
2. Use go_to_definition to find where something is defined
3. Use call_hierarchy to understand the flow (who calls what)
4. Use get_signature to understand function parameters/return types

📋 PROPOSING CODE CHANGES (not plans!):
Use `propose_change` ONLY for showing CODE diffs (not for creating plans):
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

💬 ASK_USER TOOL (MANDATORY - NEVER ASK IN CHAT):
🚨 CRITICAL: You MUST use ask_user tool for ANY question expecting user input.
NEVER type questions in chat text - this breaks the user experience!

When to use ask_user (ALWAYS):
- Clarifying ambiguous requests
- Getting preferences or choices
- Confirming before taking action
- ANY yes/no question
- ANY "which do you prefer" question

⚠️ PREFER CHOICE QUESTIONS OVER FREE TEXT:
- single_select: User picks ONE option (has built-in "Other" for custom input)
- multi_select: User picks MULTIPLE options (has built-in "Other" for custom input)
- free_text: ONLY use when you truly cannot anticipate answers (file paths, custom names)

Why choices are better:
1. Faster for users (just click)
2. Users can still type custom answer via "Other" option
3. Helps users understand what options exist

Example - User says "help with plan to improve UX":
❌ WRONG: Typing "What type of application are we focusing on?" in chat
❌ WRONG: Using free_text for questions with predictable answers
✅ RIGHT: Call ask_user tool with:
  title: "UX Improvement Planning"
  questions: [
    {id: "app_type", type: "single_select", text: "What type of application?", options: [{value: "web", label: "Web app"}, {value: "mobile", label: "Mobile app"}, {value: "desktop", label: "Desktop app"}, {value: "cli", label: "CLI tool"}]},
    {id: "focus_areas", type: "multi_select", text: "Which areas to focus on?", options: [{value: "nav", label: "Navigation"}, {value: "perf", label: "Performance"}, {value: "a11y", label: "Accessibility"}]}
  ]

🔄 TOOL EFFICIENCY - AVOID LOOPS:
- NEVER call the same tool with identical arguments twice in a row
- If a tool returns the same content you've already seen, STOP and summarize your findings
- After 2-3 tool calls that don't yield new information, conclude with what you learned
- If you're stuck in a loop, explain what you found and ask the user for guidance

✅ ALWAYS:
- DISCOVER the codebase before creating plans (list directories, read key files)
- When user asks for a "plan" → use `save_plan` tool (NOT propose_change, NOT markdown files)
- Include `overview`, `architecture`, `proposed_changes`, and `acceptance_criteria` in plans
- Specify `files` for each task so Build mode knows what to modify
- Try 3+ search patterns before saying "not found"
- Show what you DID find
- Use `propose_change` ONLY for code diffs (not for plans!)
- Use ask_user tool for ANY question (MANDATORY, not optional!)"#
            .to_string(),

        AgentMode::Ask => r#"You are an AI coding assistant in ASK MODE.

You can explore the codebase and answer questions, but you CANNOT make any changes.
This mode is for learning, understanding, and answering questions about the code.

🚀 CRITICAL RULES:
1. USE TOOLS IMMEDIATELY - Don't explain, just search and explore
2. NEVER GIVE UP - Try multiple search patterns before giving up
3. 💬 NEVER ASK QUESTIONS IN CHAT TEXT - Use ask_user tool for ALL user input!

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
- file_preview: Preview large files (head + tail)
- file_search: Find files by name pattern
- grep: Fast regex search in file contents
- shell: Limited safe commands only (git status, ls, cat, etc.)
- ask_user: 💬 Ask user structured questions via popup
- switch_mode: 🔄 Request mode switch (ask/plan/build) with user confirmation popup

⚠️ NOTE: You cannot write, patch, or delete files in Ask mode.
Suggest the user switch to /build mode if changes are needed.

🔬 UNDERSTANDING CODE BEFORE CHANGING:
1. Use list_symbols to understand file structure
2. Use call_hierarchy to see impact of changes
3. Use find_all_references to find all affected code
4. Use get_signature to understand APIs

IMPORTANT: For each action that modifies files or runs commands:
1. Explain what you're about to do
2. Show the exact changes/command
3. Wait for user confirmation before proceeding

💬 ASK_USER TOOL (MANDATORY - NEVER ASK IN CHAT):
🚨 CRITICAL: You MUST use ask_user tool for ANY question expecting user input.
NEVER type questions in chat text - this breaks the user experience!

When to use ask_user (ALWAYS):
- Clarifying what the user wants
- Getting preferences or choices
- ANY question expecting a response

⚠️ PREFER CHOICE QUESTIONS (single_select/multi_select) OVER free_text!
Choice questions have a built-in "Other" option for custom input.
Only use free_text for truly unpredictable answers (paths, names).

Example:
❌ WRONG: "What specific areas would you like me to explain?"
❌ WRONG: Using free_text when you could offer choices
✅ RIGHT: Call ask_user with single_select/multi_select:
  {type: "multi_select", text: "Which areas should I explain?", options: [{value: "arch", label: "Architecture"}, {value: "flow", label: "Data flow"}, {value: "api", label: "APIs"}, {value: "test", label: "Testing"}]}

🔄 TOOL EFFICIENCY - AVOID LOOPS:
- NEVER call the same tool with identical arguments twice in a row
- If a tool returns the same content you've already seen, STOP and summarize your findings
- After 2-3 tool calls that don't yield new information, conclude with what you learned
- If you're stuck in a loop, explain what you found and ask the user for guidance

Be thorough and cautious. Explain implications of changes."#
            .to_string(),

        AgentMode::Build => {
            r#"You are an AI coding assistant with access to tools for working with the codebase.

🚀 CRITICAL RULES:
1. USE TOOLS IMMEDIATELY - Don't explain what you'll do, just DO IT
2. NEVER GIVE UP - If one search fails, try different patterns
3. BE PERSISTENT - Try at least 3-4 different search strategies before saying "not found"
4. SHOW RESULTS - Always show what you found, even partial matches
5. 💬 NEVER ASK QUESTIONS IN CHAT TEXT - Use ask_user tool for ALL user input!

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
- ask_user: 💬 Ask user structured questions via popup (single-select, multi-select, free-text)
- switch_mode: 🔄 Request mode switch (ask/plan/build) with user confirmation popup
- mark_task_done: ✅ Mark a task as completed when following an execution plan
- get_plan_status: 📊 Check current plan progress

═══════════════════════════════════════════════════════════
📋 PLAN EXECUTION WORKFLOW (when a plan is active)
═══════════════════════════════════════════════════════════

When an active plan is shown above, follow this workflow:

1. **Focus on the current task** - Work on one task at a time
2. **Use specified files** - If the task lists files, work on those specifically
3. **Mark tasks done** - After completing each task, use `mark_task_done` with:
   - `task_index`: 0-based index of the completed task
   - `summary`: REQUIRED - Brief description of what was done (min 10 chars)
   - `files_changed`: List of files that were modified
4. **Work through ALL tasks** - Don't stop until the plan is complete
5. **Run tests** - If the plan specifies a test command, run it after changes

Example after completing a task:
```json
{
  "task_index": 0,
  "summary": "Implemented User model with id, email, password_hash fields and CRUD operations",
  "files_changed": ["src/models/user.rs", "src/models/mod.rs"]
}
```

The status bar shows plan progress (e.g., "📋 2/5: Add auth middleware").

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

💬 ASK_USER TOOL (MANDATORY - NEVER ASK IN CHAT):
🚨 CRITICAL: You MUST use ask_user tool for ANY question expecting user input.
NEVER type questions in chat text - this breaks the user experience!

When to use ask_user (ALWAYS):
- Clarifying ambiguous requests
- Getting preferences or choices
- Confirming before destructive operations
- ANY yes/no question
- ANY "which do you prefer" question

⚠️ PREFER CHOICE QUESTIONS OVER FREE TEXT:
- single_select: User picks ONE option (has "Other" for custom input)
- multi_select: User picks MANY options (has "Other" for custom input)
- free_text: ONLY for truly unpredictable input (file paths, custom names)

User can press Esc to cancel and answer via chat instead.

Example - User says "add authentication":
❌ WRONG: "What authentication method would you prefer? JWT, sessions, or OAuth?"
❌ WRONG: Using free_text for questions with obvious choices
✅ RIGHT: Call ask_user with single_select:
  title: "Authentication Setup"
  questions: [{id: "method", type: "single_select", text: "Which authentication method?", options: [{value: "jwt", label: "JWT tokens"}, {value: "session", label: "Session-based"}, {value: "oauth", label: "OAuth2"}, {value: "api_key", label: "API keys"}]}]

❌ NEVER DO THIS:
- Say "I couldn't find X" without trying multiple search patterns
- Explain what you're going to do instead of doing it
- Give up after one failed search
- Say you "can't create diagrams" - YOU CAN with Mermaid/PlantUML!
- Run dangerous shell commands (they will be blocked anyway)
- Ask questions in chat text - USE ask_user tool instead!
- Use free_text when you could provide choices - users prefer clicking over typing!

🔄 TOOL EFFICIENCY - AVOID LOOPS:
- NEVER call the same tool with identical arguments twice in a row
- If a tool returns the same content you've already seen, STOP and summarize your findings
- After 2-3 tool calls that don't yield new information, conclude with what you learned
- If you're stuck in a loop, explain what you found and ask the user for guidance

✅ ALWAYS DO THIS:
- Try 3+ different search patterns before concluding something doesn't exist
- Show what you DID find, even if it's not exactly what was asked
- Create diagrams using Mermaid when asked for visual representations
- Suggest next steps based on what you discovered
- Be cautious with shell commands - prefer read-only commands when possible
- Use ask_user tool for ANY question (MANDATORY, not optional!)"#
                .to_string()
        }
    };

    // Add Chain of Thought instructions ONLY when:
    // 1. Thinking is enabled via /think command AND
    // 2. Model doesn't support native thinking (needs prompt-based thinking)
    // For models with native thinking, the API parameters handle it
    let prompt_with_thinking = if thinking_enabled && !supports_native_thinking {
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
    };

    // Prepend status header so agent always knows its current context
    format!("{}{}", status_header, prompt_with_thinking)
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
    /// Current trust level (cached for system prompt)
    trust_level: crate::tools::TrustLevel,
    /// Thinking/reasoning level name for LLM requests
    /// "off" means disabled, other values are looked up in thinking_config
    think_level: String,
    /// Thinking configuration (levels and their settings)
    thinking_config: crate::config::ThinkingConfig,
    /// Plan service for tracking execution plans (optional)
    plan_service: Option<Arc<PlanService>>,
    /// Cached plan context for Build mode system prompt injection
    plan_context: Option<PlanContext>,
}

impl ChatAgent {
    pub fn new(llm: Arc<dyn LlmProvider>, tools: ToolRegistry) -> Self {
        Self::with_mode(llm, tools, AgentMode::Build)
    }

    pub fn with_mode(llm: Arc<dyn LlmProvider>, tools: ToolRegistry, mode: AgentMode) -> Self {
        let mut context = ConversationContext::new();
        let supports_thinking = llm.supports_native_thinking();
        let trust_level = crate::tools::TrustLevel::default();
        // Thinking is off by default, so pass false here
        // No plan context at initialization
        context.add_system(get_system_prompt(
            mode,
            supports_thinking,
            false,
            trust_level,
            None,
        ));

        Self {
            llm,
            tools,
            context,
            max_iterations: 10,
            mode,
            trust_level,
            think_level: "off".to_string(), // Off by default, enabled via /think command
            thinking_config: crate::config::ThinkingConfig::default(),
            plan_service: None,
            plan_context: None,
        }
    }

    /// Set the plan service for Build mode plan context injection
    pub fn with_plan_service(mut self, service: Arc<PlanService>) -> Self {
        self.plan_service = Some(service);
        self
    }

    /// Set the plan service (mutable version for existing instances)
    pub fn set_plan_service(&mut self, service: Arc<PlanService>) {
        self.plan_service = Some(service);
    }

    /// Clear the plan service
    pub fn clear_plan_service(&mut self) {
        self.plan_service = None;
        self.plan_context = None;
    }

    /// Refresh plan context from the plan service
    ///
    /// Call this before processing messages in Build mode to ensure
    /// the system prompt includes current plan context.
    pub async fn refresh_plan_context(&mut self) {
        if self.mode != AgentMode::Build {
            self.plan_context = None;
            return;
        }

        if let Some(ref service) = self.plan_service {
            if let Ok(Some(plan)) = service.get_current_plan().await {
                self.plan_context = PlanContext::from_plan(&plan);

                if let Some(ref ctx) = self.plan_context {
                    tracing::debug!(
                        "Plan context refreshed: {} - Task {}: {}",
                        ctx.title,
                        ctx.current_task_index + 1,
                        ctx.current_task_description
                    );
                }
            } else {
                self.plan_context = None;
            }
        } else {
            self.plan_context = None;
        }
    }

    /// Get the current plan context (if any)
    pub fn plan_context(&self) -> Option<&PlanContext> {
        self.plan_context.as_ref()
    }

    /// Set the thinking configuration
    pub fn set_thinking_config(&mut self, config: crate::config::ThinkingConfig) {
        self.thinking_config = config;
    }

    /// Set the thinking/reasoning level by name
    ///
    /// Level names are defined in thinking_config.levels
    /// "off" disables thinking
    ///
    /// This also refreshes the system prompt to add/remove thinking instructions
    /// based on the new level and model capabilities
    pub async fn set_think_level(&mut self, level_name: String) {
        self.think_level = level_name;
        // Refresh system prompt based on new thinking state
        self.refresh_system_prompt_async().await;
    }

    /// Set think level with sync prompt refresh
    ///
    /// This uses the sync fallback for capability detection
    /// (hardcoded model checks rather than models.dev query)
    pub fn set_think_level_sync(&mut self, level_name: String) {
        self.think_level = level_name;
        // Refresh system prompt using sync capability check
        self.refresh_system_prompt();
    }

    /// Refresh system prompt based on current thinking state (async version)
    ///
    /// This queries models.dev for accurate capability detection
    pub async fn refresh_system_prompt_async(&mut self) {
        // Refresh plan context first if in Build mode
        self.refresh_plan_context().await;

        let supports_thinking = self.llm.supports_native_thinking_async().await;
        let thinking_enabled = self.is_thinking_enabled();
        let new_prompt = get_system_prompt(
            self.mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            self.plan_context.as_ref(),
        );
        self.context.update_system_prompt(&new_prompt);
        tracing::debug!(
            "System prompt refreshed: mode={:?}, approval={:?}, thinking_enabled={}, supports_native={}, has_plan={}",
            self.mode,
            self.trust_level,
            thinking_enabled,
            supports_thinking,
            self.plan_context.is_some()
        );
    }

    /// Refresh system prompt (sync version, uses fallback capability check)
    ///
    /// Note: This does NOT refresh plan context since that requires async.
    /// Use refresh_system_prompt_async for full refresh including plan context.
    pub fn refresh_system_prompt(&mut self) {
        let supports_thinking = self.llm.supports_native_thinking();
        let thinking_enabled = self.is_thinking_enabled();
        let new_prompt = get_system_prompt(
            self.mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            self.plan_context.as_ref(),
        );
        self.context.update_system_prompt(&new_prompt);
    }

    /// Get the current thinking level name
    pub fn think_level(&self) -> &str {
        &self.think_level
    }

    /// Check if thinking is enabled (any level other than "off")
    pub fn is_thinking_enabled(&self) -> bool {
        self.think_level != "off"
    }

    /// Resolve think settings from current config
    fn resolve_think_settings(&self) -> ThinkSettings {
        ThinkSettings::resolve(&self.think_level, &self.thinking_config)
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

        // Clear plan context when not in Build mode
        if mode != AgentMode::Build {
            self.plan_context = None;
        }

        // Update the system prompt in context (replace the first system message)
        let supports_thinking = self.llm.supports_native_thinking();
        let thinking_enabled = self.is_thinking_enabled();
        self.context.update_system_prompt(&get_system_prompt(
            mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            self.plan_context.as_ref(),
        ));
    }

    /// Update just the LLM provider while preserving conversation history
    pub fn update_provider(&mut self, llm: Arc<dyn LlmProvider>) {
        self.llm = llm;
    }

    /// Update approval storage path (per session)
    pub async fn set_approval_storage_path(&mut self, storage_path: std::path::PathBuf) {
        self.tools.set_approval_storage_path(storage_path).await;
    }

    /// Set the trust level for risky tool operations
    ///
    /// This updates both the tool registry and the cached trust level,
    /// and refreshes the system prompt so the agent knows the new level.
    pub async fn set_trust_level(&mut self, level: crate::tools::TrustLevel) {
        self.trust_level = level;
        self.tools.set_trust_level(level).await;
        // Refresh system prompt so agent knows the new trust level
        self.refresh_system_prompt_async().await;
    }

    /// Get the current trust level
    pub fn trust_level(&self) -> crate::tools::TrustLevel {
        self.trust_level
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

    /// Get system prompt token count
    ///
    /// Returns the estimated tokens used by the system prompt message.
    pub fn system_prompt_tokens(&self) -> usize {
        use crate::llm::Role;
        self.context
            .messages()
            .iter()
            .find(|m| m.role == Role::System)
            .map(|m| match &m.content {
                crate::llm::MessageContent::Text(t) => ConversationContext::estimate_tokens(t),
                crate::llm::MessageContent::Parts(parts) => parts
                    .iter()
                    .map(|p| match p {
                        crate::llm::ContentPart::Text { text } => {
                            ConversationContext::estimate_tokens(text)
                        }
                        _ => 0,
                    })
                    .sum(),
            })
            .unwrap_or(0)
    }

    /// Get estimated token count for tool schemas
    ///
    /// Estimates approximately 100 tokens per tool definition based on
    /// average tool schema size (name, description, parameters).
    pub fn tool_schema_tokens(&self) -> usize {
        // Average tool definition is approximately 100 tokens
        // (name ~5, description ~30, parameters ~65)
        self.tools.definitions().len() * 100
    }

    /// Get conversation history token count (excludes system prompt)
    ///
    /// Returns estimated tokens for all user/assistant/tool messages.
    pub fn conversation_history_tokens(&self) -> usize {
        use crate::llm::Role;
        self.context
            .messages()
            .iter()
            .filter(|m| m.role != Role::System)
            .map(|m| match &m.content {
                crate::llm::MessageContent::Text(t) => ConversationContext::estimate_tokens(t),
                crate::llm::MessageContent::Parts(parts) => parts
                    .iter()
                    .map(|p| match p {
                        crate::llm::ContentPart::Text { text } => {
                            ConversationContext::estimate_tokens(text)
                        }
                        crate::llm::ContentPart::ToolUse { input, .. } => {
                            ConversationContext::estimate_tokens(&input.to_string())
                        }
                        crate::llm::ContentPart::ToolResult { content, .. } => {
                            ConversationContext::estimate_tokens(content)
                        }
                    })
                    .sum(),
            })
            .sum()
    }

    /// Get the current mode
    pub fn mode(&self) -> AgentMode {
        self.mode
    }

    /// Clear conversation history (keeps system prompt)
    ///
    /// Also clears the plan context to ensure a fresh start.
    pub fn clear_history(&mut self) {
        // Clear plan context for a complete fresh start
        self.plan_context = None;

        // Get the current system prompt (now without plan context)
        let supports_thinking = self.llm.supports_native_thinking();
        let thinking_enabled = self.is_thinking_enabled();
        let system_prompt = get_system_prompt(
            self.mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            None, // No plan context after clear
        );

        // Clear and reinitialize with system prompt
        self.context.clear();
        self.context.add_system(system_prompt);

        tracing::info!("Conversation history and plan context cleared");
    }

    /// Restore conversation from a saved session
    pub fn restore_from_session(&mut self, session: &crate::storage::ChatSession) {
        // Clear existing history
        let supports_thinking = self.llm.supports_native_thinking();
        let thinking_enabled = self.is_thinking_enabled();
        let system_prompt = get_system_prompt(
            self.mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            self.plan_context.as_ref(),
        );
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
                    tool_calls: Vec::new(),
                    thinking_content: None,
                    segments: Vec::new(),
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
                                    format!(
                                        "{}...(truncated)",
                                        truncate_at_char_boundary(content, 200)
                                    )
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
                format!("{}...(truncated)", truncate_at_char_boundary(&content, 500))
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
                            format!("{}...(truncated)", truncate_at_char_boundary(content, 200))
                        } else {
                            content.clone()
                        }),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            };

            let truncated = if content.len() > 500 {
                format!("{}...(truncated)", truncate_at_char_boundary(&content, 500))
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
        // Refresh plan context for Build mode before processing
        // This ensures the system prompt has current plan state
        if self.mode == AgentMode::Build && self.plan_service.is_some() {
            self.refresh_plan_context().await;
            self.refresh_system_prompt();
        }

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

        // Track duplicate tool results to prevent infinite loops
        let mut last_tool_results: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut consecutive_duplicate_calls = 0;
        const MAX_CONSECUTIVE_DUPLICATES: usize = 2;
        const MAX_TOOL_CALLS_PER_TURN: usize = 5;

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

                    // Limit tool calls per turn to prevent runaway loops
                    if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_calls = if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &calls[..]
                    };

                    // First, add the assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(limited_calls);

                    // Execute each tool call and add results
                    for (i, call) in limited_calls.iter().enumerate() {
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

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

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
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
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

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
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

                    // Limit tool calls per turn to prevent runaway loops
                    if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            tool_calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_tool_calls = if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &tool_calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &tool_calls[..]
                    };

                    // CRITICAL FIX: Persist text BEFORE adding tool calls to context
                    // This ensures the assistant's explanation is not lost when tools are invoked
                    if let Some(t) = &text {
                        if !t.is_empty() {
                            self.context.add_assistant(t);
                            tracing::debug!("Persisted mixed response text before tools: {}", t);
                        }
                    }

                    // Add assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(limited_tool_calls);

                    // Execute tool calls
                    for (i, call) in limited_tool_calls.iter().enumerate() {
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

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

                        tracing::debug!("Tool result: {:?}", result);

                        // Log the tool call
                        let preview = if result.output.len() > 200 {
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
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

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
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
        let thinking_enabled = self.is_thinking_enabled();
        self.context.add_system(get_system_prompt(
            self.mode,
            supports_thinking,
            thinking_enabled,
            self.trust_level,
            self.plan_context.as_ref(),
        ));
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

        // Track duplicate tool results to prevent infinite loops
        let mut last_tool_results: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut consecutive_duplicate_calls = 0;
        const MAX_CONSECUTIVE_DUPLICATES: usize = 2;
        const MAX_TOOL_CALLS_PER_TURN: usize = 5;

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

                    // Limit tool calls per turn to prevent runaway loops
                    if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_calls = if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &calls[..]
                    };

                    // First, add the assistant message with tool calls
                    self.context.add_assistant_tool_calls(limited_calls);

                    // Execute each tool call with interrupt checking
                    for (i, call) in limited_calls.iter().enumerate() {
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

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        // Log the tool call
                        let preview = if result.output.len() > 200 {
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
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

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
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

                    // Limit tool calls per turn to prevent runaway loops
                    if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            tool_calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_tool_calls = if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &tool_calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &tool_calls[..]
                    };

                    // CRITICAL FIX: Persist text BEFORE adding tool calls to context
                    // This ensures the assistant's explanation is not lost when tools are invoked
                    if let Some(t) = &text {
                        if !t.is_empty() {
                            self.context.add_assistant(t);
                            tracing::debug!("Persisted mixed response text before tools: {}", t);
                        }
                    }

                    self.context.add_assistant_tool_calls(limited_tool_calls);

                    // Execute tool calls with interrupt checking
                    for (i, call) in limited_tool_calls.iter().enumerate() {
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

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
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

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
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
    /// * `on_tool_complete` - Callback when a tool call completes (name, result preview, success)
    pub async fn chat_streaming<F, T, K, C, D>(
        &mut self,
        user_message: &str,
        interrupt_check: F,
        on_text: T,
        on_thinking: K,
        on_tool_call: C,
        on_tool_complete: D,
    ) -> Result<AgentResponse>
    where
        F: Fn() -> bool + Send + Sync,
        T: Fn(String) + Send + Sync + 'static,
        K: Fn(String) + Send + Sync + 'static,
        C: Fn(String, String) + Send + Sync + 'static,
        D: Fn(String, String, bool) + Send + Sync + 'static,
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

        // Track duplicate tool results to prevent infinite loops
        let mut last_tool_results: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut consecutive_duplicate_calls = 0;
        const MAX_CONSECUTIVE_DUPLICATES: usize = 2;
        const MAX_TOOL_CALLS_PER_TURN: usize = 5;

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
            let think_settings = self.resolve_think_settings();
            let response = self
                .llm
                .chat_streaming_with_thinking(
                    self.context.messages(),
                    Some(&tool_definitions),
                    callback,
                    Some(interrupt_check_ref),
                    &think_settings,
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

                    // Limit tool calls per turn to prevent runaway loops
                    if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_calls = if calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &calls[..]
                    };

                    // First, add the assistant message with tool calls (required for OpenAI)
                    self.context.add_assistant_tool_calls(limited_calls);

                    // CRITICAL FIX: Emit a preamble before the first tool call if no text has been emitted yet
                    // This prevents the UI from showing a blank assistant message before tools
                    if accumulated_text.is_empty() && total_tool_calls == 0 {
                        let preamble = "Let me check the codebase for you.";
                        on_text(preamble.to_string());
                        accumulated_text = preamble.to_string();
                        tracing::debug!("Emitted preamble before first tool execution");
                    }

                    // Execute each tool call and add results
                    for call in limited_calls.iter() {
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

                        // Yield to allow the UI to render the "tool started" state
                        tokio::task::yield_now().await;

                        tracing::info!(
                            "Executing tool: {} with args: {} (mode: {:?})",
                            call.name,
                            call.arguments,
                            self.mode
                        );

                        let result =
                            match self.tools.execute(&call.name, call.arguments.clone()).await {
                                Ok(r) => r,
                                Err(e) => {
                                    // Surface error as tool result so model can recover
                                    let msg = format!("Tool '{}' failed: {}", call.name, e);
                                    on_tool_complete(call.name.clone(), msg.clone(), false);
                                    self.context.add_tool_result(&call.id, &msg);
                                    let preview = if msg.len() > 200 {
                                        format!("{}...", truncate_at_char_boundary(&msg, 200))
                                    } else {
                                        msg
                                    };
                                    tool_call_log.push(ToolCallLog {
                                        tool: call.name.clone(),
                                        args: call.arguments.clone(),
                                        result_preview: preview,
                                    });
                                    continue;
                                }
                            };

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode - model hallucinated this call",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview.clone(),
                        });

                        // Notify that tool completed immediately
                        on_tool_complete(call.name.clone(), preview, result.success);

                        self.context.add_tool_result(&call.id, &result.output);
                    }

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
                    }
                }
                LlmResponse::Mixed {
                    text, tool_calls, ..
                } => {
                    // Handle mixed response with both text and tool calls
                    // CRITICAL FIX: Use accumulated text from streaming if available, otherwise use final text
                    let final_text = if !accumulated_text.is_empty() {
                        accumulated_text.clone()
                    } else if let Some(t) = &text {
                        t.clone()
                    } else {
                        String::new()
                    };

                    // Persist text BEFORE adding tool calls to context
                    // This ensures the assistant's explanation is not lost when tools are invoked
                    if !final_text.is_empty() {
                        self.context.add_assistant(&final_text);
                        tracing::debug!(
                            "Persisted mixed response text before tools: {}",
                            final_text
                        );
                    }

                    total_tool_calls += tool_calls.len();

                    // Limit tool calls per turn to prevent runaway loops
                    if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        tracing::warn!(
                            "LLM requested {} tools in single turn, limiting to {}",
                            tool_calls.len(),
                            MAX_TOOL_CALLS_PER_TURN
                        );
                    }
                    let limited_tool_calls = if tool_calls.len() > MAX_TOOL_CALLS_PER_TURN {
                        &tool_calls[..MAX_TOOL_CALLS_PER_TURN]
                    } else {
                        &tool_calls[..]
                    };

                    self.context.add_assistant_tool_calls(limited_tool_calls);

                    // CRITICAL FIX: Emit a preamble before the first tool call if no text has been emitted yet
                    // This prevents the UI from showing a blank assistant message before tools
                    // In Mixed responses, final_text might be non-empty if text was streamed/provided
                    if final_text.is_empty() && total_tool_calls == 0 {
                        let preamble = "Let me check the codebase for you.";
                        on_text(preamble.to_string());
                        accumulated_text = preamble.to_string();
                        tracing::debug!(
                            "Emitted preamble before first tool execution (Mixed response)"
                        );
                    }

                    for call in limited_tool_calls {
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

                        // Yield to allow the UI to render the "tool started" state
                        tokio::task::yield_now().await;

                        let result =
                            match self.tools.execute(&call.name, call.arguments.clone()).await {
                                Ok(r) => r,
                                Err(e) => {
                                    // Surface error as tool result so model can recover
                                    let msg = format!("Tool '{}' failed: {}", call.name, e);
                                    on_tool_complete(call.name.clone(), msg.clone(), false);
                                    self.context.add_tool_result(&call.id, &msg);
                                    let preview = if msg.len() > 200 {
                                        format!("{}...", truncate_at_char_boundary(&msg, 200))
                                    } else {
                                        msg
                                    };
                                    tool_call_log.push(ToolCallLog {
                                        tool: call.name.clone(),
                                        args: call.arguments.clone(),
                                        result_preview: preview,
                                    });
                                    continue;
                                }
                            };

                        // Check for duplicate results to prevent infinite loops
                        let result_key = format!(
                            "{}:{}",
                            call.name,
                            serde_json::to_string(&call.arguments).unwrap_or_default()
                        );
                        if let Some(last_result) = last_tool_results.get(&result_key) {
                            if last_result == &result.output {
                                consecutive_duplicate_calls += 1;
                                tracing::warn!(
                                    "Tool '{}' returned identical result ({}/{})",
                                    call.name,
                                    consecutive_duplicate_calls,
                                    MAX_CONSECUTIVE_DUPLICATES
                                );
                                if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                                    // Force the agent to summarize instead of looping
                                    self.context.add_assistant(
                                        "I'm seeing repeated results from tools. Please let me know how you'd like to proceed."
                                    );
                                    // Break out of the tool execution loop
                                    break;
                                }
                            }
                        } else {
                            consecutive_duplicate_calls = 0;
                        }
                        last_tool_results.insert(result_key, result.output.clone());

                        if !result.success && result.output.contains("Unknown tool") {
                            tracing::warn!(
                                "Tool '{}' not available in {:?} mode",
                                call.name,
                                self.mode
                            );
                        }

                        let preview = if result.output.len() > 200 {
                            format!("{}...", truncate_at_char_boundary(&result.output, 200))
                        } else {
                            result.output.clone()
                        };
                        tool_call_log.push(ToolCallLog {
                            tool: call.name.clone(),
                            args: call.arguments.clone(),
                            result_preview: preview.clone(),
                        });

                        // Notify that tool completed immediately
                        on_tool_complete(call.name.clone(), preview, result.success);

                        self.context.add_tool_result(&call.id, &result.output);
                    }

                    // If we broke out of tool execution due to duplicates, break main loop too
                    if consecutive_duplicate_calls >= MAX_CONSECUTIVE_DUPLICATES {
                        break;
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

#[cfg(test)]
mod tests {
    use super::ChatAgent;
    use crate::llm::{LlmProvider, LlmResponse, Message, ToolDefinition};
    use async_trait::async_trait;
    use std::sync::Arc;

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

    #[test]
    fn update_provider_replaces_llm() {
        let provider_a = Arc::new(TestProvider { name: "provider_a" });
        let provider_b = Arc::new(TestProvider { name: "provider_b" });
        let tools = crate::tools::ToolRegistry::new(std::path::PathBuf::from("."));

        let mut agent = ChatAgent::new(provider_a, tools);
        assert_eq!(agent.llm.name(), "provider_a");

        agent.update_provider(provider_b);
        assert_eq!(agent.llm.name(), "provider_b");
    }
}
