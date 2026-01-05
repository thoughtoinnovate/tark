//! Slash command handler for the TUI
//!
//! Provides a command registry and parser for slash commands like /help, /clear, /model, etc.
//! Supports command aliases and tab completion.

#![allow(dead_code)]

use std::collections::HashMap;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandResult {
    /// Command executed successfully, continue normal operation
    Continue,
    /// Clear the input field
    ClearInput,
    /// Show a picker UI (provider, model, session)
    ShowPicker(PickerType),
    /// Change agent mode
    ChangeMode(AgentModeChange),
    /// Toggle a setting
    Toggle(ToggleSetting),
    /// Show help text
    ShowHelp(String),
    /// Show statistics
    ShowStats,
    /// Show cost information
    ShowCost,
    /// Show usage information
    ShowUsage,
    /// Show total (all-time) usage information
    ShowUsageTotal,
    /// Open usage dashboard in browser
    OpenUsageDashboard,
    /// Clear chat history
    ClearHistory,
    /// Compact/summarize conversation
    Compact,
    /// Create new session
    NewSession,
    /// Delete session
    DeleteSession,
    /// Exit the application
    Exit,
    /// Interrupt current operation
    Interrupt,
    /// Attach a file
    AttachFile(String),
    /// Clear all attachments
    ClearAttachments,
    /// Error with message
    Error(String),
    /// Message to display to user
    Message(String),
    /// Plan commands
    PlanStatus,
    PlanList,
    PlanDone(Option<String>),
    PlanSkip(Option<String>),
    PlanNext,
    PlanRefine(String),
    /// Diff commands (editor integration)
    ShowDiff(String),
    ToggleAutoDiff,
    /// Focus tasks panel
    FocusTasks,
}

/// Types of pickers that can be shown
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickerType {
    /// Provider selection
    Provider,
    /// Model selection
    Model,
    /// Session selection
    Session,
}

/// State for the two-step model picker flow
///
/// This enum tracks where we are in the unified `/model` command flow:
/// 1. First, user selects a provider
/// 2. Then, user selects a model within that provider
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelPickerState {
    /// Selecting a provider (step 1)
    SelectingProvider,
    /// Selecting a model for the chosen provider (step 2)
    SelectingModel {
        /// The provider selected in step 1
        provider: String,
    },
}

/// Information about a provider for display in the picker
///
/// Used to show provider options with availability status and configuration hints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInfo {
    /// Unique identifier for the provider (e.g., "openai", "claude", "ollama")
    pub id: String,
    /// Display name for the provider (e.g., "OpenAI", "Claude", "Ollama")
    pub name: String,
    /// Brief description of the provider
    pub description: String,
    /// Whether the provider is available (has API key configured)
    pub available: bool,
    /// Configuration hint if provider is not available
    pub hint: Option<String>,
}

impl ProviderInfo {
    /// Create a new ProviderInfo
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        available: bool,
        hint: Option<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            available,
            hint,
        }
    }

    /// Check if a provider is available based on environment variables
    ///
    /// Returns true if the provider's required API key is set, or for Ollama
    /// if the OLLAMA_MODEL env var is set (Ollama doesn't require an API key).
    pub fn check_provider_availability(provider_id: &str) -> bool {
        match provider_id {
            "openai" => std::env::var("OPENAI_API_KEY").is_ok(),
            "claude" => std::env::var("ANTHROPIC_API_KEY").is_ok(),
            "copilot" | "github" => {
                // Copilot is available if token file exists
                if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
                    let token_path = proj_dirs.config_dir().join("copilot_token.json");
                    token_path.exists()
                } else {
                    false
                }
            }
            "gemini" | "google" => std::env::var("GEMINI_API_KEY").is_ok(),
            "openrouter" => std::env::var("OPENROUTER_API_KEY").is_ok(),
            "ollama" => {
                // Ollama is available if OLLAMA_MODEL is set or if we assume local availability
                // For now, we check if OLLAMA_MODEL or OLLAMA_BASE_URL is set
                std::env::var("OLLAMA_MODEL").is_ok() || std::env::var("OLLAMA_BASE_URL").is_ok()
            }
            _ => false,
        }
    }

    /// Get the configuration hint for a provider
    pub fn get_provider_hint(provider_id: &str) -> Option<String> {
        match provider_id {
            "openai" => Some("Set OPENAI_API_KEY environment variable".to_string()),
            "claude" => Some("Set ANTHROPIC_API_KEY environment variable".to_string()),
            "copilot" | "github" => Some("Run 'tark auth copilot' to authenticate".to_string()),
            "gemini" | "google" => Some("Set GEMINI_API_KEY environment variable".to_string()),
            "openrouter" => Some("Set OPENROUTER_API_KEY environment variable".to_string()),
            "ollama" => Some("Set OLLAMA_MODEL or ensure Ollama is running locally".to_string()),
            _ => None,
        }
    }

    /// Get all known providers with their availability status
    pub fn get_all_providers() -> Vec<ProviderInfo> {
        vec![
            {
                let available = Self::check_provider_availability("openai");
                ProviderInfo::new(
                    "openai",
                    "OpenAI",
                    "GPT-4, GPT-4o, and other OpenAI models",
                    available,
                    if available {
                        None
                    } else {
                        Self::get_provider_hint("openai")
                    },
                )
            },
            {
                let available = Self::check_provider_availability("claude");
                ProviderInfo::new(
                    "claude",
                    "Claude",
                    "Anthropic's Claude models (Sonnet, Opus, Haiku)",
                    available,
                    if available {
                        None
                    } else {
                        Self::get_provider_hint("claude")
                    },
                )
            },
            {
                let available = Self::check_provider_availability("copilot");
                ProviderInfo::new(
                    "copilot",
                    "GitHub Copilot",
                    "GPT-4o via GitHub Copilot (Device Flow auth)",
                    available,
                    if available {
                        None
                    } else {
                        Some("Run 'tark auth copilot' to authenticate".to_string())
                    },
                )
            },
            {
                let available = Self::check_provider_availability("gemini");
                ProviderInfo::new(
                    "gemini",
                    "Google Gemini",
                    "Gemini 2.0 models with long context",
                    available,
                    if available {
                        None
                    } else {
                        Self::get_provider_hint("gemini")
                    },
                )
            },
            {
                let available = Self::check_provider_availability("openrouter");
                ProviderInfo::new(
                    "openrouter",
                    "OpenRouter",
                    "Access to 200+ models from various providers",
                    available,
                    if available {
                        None
                    } else {
                        Self::get_provider_hint("openrouter")
                    },
                )
            },
            {
                let available = Self::check_provider_availability("ollama");
                ProviderInfo::new(
                    "ollama",
                    "Ollama",
                    "Local models via Ollama (CodeLlama, Mistral, etc.)",
                    available,
                    if available {
                        None
                    } else {
                        Self::get_provider_hint("ollama")
                    },
                )
            },
        ]
    }
}

/// Agent mode changes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentModeChange {
    Plan,
    Build,
    Review,
}

/// Settings that can be toggled
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToggleSetting {
    /// Thinking/verbose mode
    Thinking,
}

/// A registered command
#[derive(Debug, Clone)]
pub struct Command {
    /// Primary command name (without the leading /)
    pub name: String,
    /// Command description for help text
    pub description: String,
    /// Usage example
    pub usage: String,
    /// Category for grouping in help
    pub category: CommandCategory,
    /// Whether this command requires arguments
    pub requires_args: bool,
    /// Whether this command requires editor connection (Neovim)
    pub requires_editor: bool,
}

impl Command {
    /// Check if this command is available in standalone mode
    pub fn is_available_standalone(&self) -> bool {
        !self.requires_editor
    }
}

/// Command categories for help organization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCategory {
    /// General commands (help, clear, exit)
    General,
    /// Model/provider commands
    Model,
    /// Session management
    Session,
    /// Agent mode commands
    Mode,
    /// Utility commands
    Utility,
    /// Attachment commands
    Attachment,
    /// Plan commands
    Plan,
    /// Editor integration commands
    Editor,
}

impl CommandCategory {
    /// Get display name for the category
    pub fn display_name(&self) -> &'static str {
        match self {
            CommandCategory::General => "General",
            CommandCategory::Model => "Model & Provider",
            CommandCategory::Session => "Session",
            CommandCategory::Mode => "Agent Mode",
            CommandCategory::Utility => "Utility",
            CommandCategory::Attachment => "Attachments",
            CommandCategory::Plan => "Execution Plans",
            CommandCategory::Editor => "Editor Integration",
        }
    }
}

/// Command handler with registry
#[derive(Debug)]
pub struct CommandHandler {
    /// Registered commands by name
    commands: HashMap<String, Command>,
    /// Aliases mapping to primary command names
    aliases: HashMap<String, String>,
}

impl Default for CommandHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandHandler {
    /// Create a new command handler with all commands registered
    pub fn new() -> Self {
        let mut handler = Self {
            commands: HashMap::new(),
            aliases: HashMap::new(),
        };
        handler.register_all();
        handler
    }

    /// Register all built-in commands
    fn register_all(&mut self) {
        // General commands
        self.register(Command {
            name: "help".to_string(),
            description: "Show available commands".to_string(),
            usage: "/help [command]".to_string(),
            category: CommandCategory::General,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "clear".to_string(),
            description: "Clear chat history".to_string(),
            usage: "/clear".to_string(),
            category: CommandCategory::General,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("c", "clear");

        self.register(Command {
            name: "exit".to_string(),
            description: "Close the chat".to_string(),
            usage: "/exit".to_string(),
            category: CommandCategory::General,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("quit", "exit");
        self.add_alias("q", "exit");
        self.add_alias("close", "exit");

        self.register(Command {
            name: "interrupt".to_string(),
            description: "Stop current agent operation".to_string(),
            usage: "/interrupt".to_string(),
            category: CommandCategory::General,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("stop", "interrupt");
        self.add_alias("cancel", "interrupt");

        // Model/Provider commands
        // /model now provides unified two-step flow: provider → model selection
        // /provider is an alias to /model (Requirements: 1.1, 1.7)
        self.register(Command {
            name: "model".to_string(),
            description: "Select provider and model (two-step flow)".to_string(),
            usage: "/model".to_string(),
            category: CommandCategory::Model,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("m", "model");
        self.add_alias("provider", "model"); // Redirect /provider to /model (Requirements: 1.7)

        self.register(Command {
            name: "ollama".to_string(),
            description: "Switch to Ollama provider".to_string(),
            usage: "/ollama".to_string(),
            category: CommandCategory::Model,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "claude".to_string(),
            description: "Switch to Claude provider".to_string(),
            usage: "/claude".to_string(),
            category: CommandCategory::Model,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "openai".to_string(),
            description: "Switch to OpenAI provider".to_string(),
            usage: "/openai".to_string(),
            category: CommandCategory::Model,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("gpt", "openai");

        // Session commands
        self.register(Command {
            name: "new".to_string(),
            description: "Start a new session".to_string(),
            usage: "/new".to_string(),
            category: CommandCategory::Session,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "sessions".to_string(),
            description: "List and switch sessions".to_string(),
            usage: "/sessions".to_string(),
            category: CommandCategory::Session,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("session", "sessions");

        self.register(Command {
            name: "delete".to_string(),
            description: "Delete a session".to_string(),
            usage: "/delete".to_string(),
            category: CommandCategory::Session,
            requires_args: false,
            requires_editor: false,
        });

        // Agent mode commands
        self.register(Command {
            name: "plan".to_string(),
            description: "Switch to plan mode".to_string(),
            usage: "/plan".to_string(),
            category: CommandCategory::Mode,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "build".to_string(),
            description: "Switch to build mode".to_string(),
            usage: "/build".to_string(),
            category: CommandCategory::Mode,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "review".to_string(),
            description: "Switch to review mode".to_string(),
            usage: "/review".to_string(),
            category: CommandCategory::Mode,
            requires_args: false,
            requires_editor: false,
        });

        // Utility commands
        self.register(Command {
            name: "thinking".to_string(),
            description: "Toggle verbose/thinking mode".to_string(),
            usage: "/thinking".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("verbose", "thinking");
        self.add_alias("debug", "thinking");

        self.register(Command {
            name: "stats".to_string(),
            description: "Show session statistics".to_string(),
            usage: "/stats".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("s", "stats");

        self.register(Command {
            name: "cost".to_string(),
            description: "Show model pricing info".to_string(),
            usage: "/cost".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "compact".to_string(),
            description: "Summarize conversation to save context".to_string(),
            usage: "/compact".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "usage".to_string(),
            description: "Show usage statistics".to_string(),
            usage: "/usage".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "usage-total".to_string(),
            description: "Show total (all-time) usage statistics".to_string(),
            usage: "/usage-total".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "usage-open".to_string(),
            description: "Open usage dashboard in browser".to_string(),
            usage: "/usage-open".to_string(),
            category: CommandCategory::Utility,
            requires_args: false,
            requires_editor: false,
        });

        // Attachment commands
        self.register(Command {
            name: "attach".to_string(),
            description: "Attach a file".to_string(),
            usage: "/attach <filepath>".to_string(),
            category: CommandCategory::Attachment,
            requires_args: true,
            requires_editor: false,
        });

        self.register(Command {
            name: "clear-attachments".to_string(),
            description: "Remove all pending attachments".to_string(),
            usage: "/clear-attachments".to_string(),
            category: CommandCategory::Attachment,
            requires_args: false,
            requires_editor: false,
        });

        // Plan commands
        self.register(Command {
            name: "plan-status".to_string(),
            description: "Show current plan status".to_string(),
            usage: "/plan-status".to_string(),
            category: CommandCategory::Plan,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("ps", "plan-status");

        self.register(Command {
            name: "plan-list".to_string(),
            description: "List all plans".to_string(),
            usage: "/plan-list".to_string(),
            category: CommandCategory::Plan,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("pl", "plan-list");

        self.register(Command {
            name: "plan-done".to_string(),
            description: "Mark task as done".to_string(),
            usage: "/plan-done [task]".to_string(),
            category: CommandCategory::Plan,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("pd", "plan-done");

        self.register(Command {
            name: "plan-skip".to_string(),
            description: "Skip a task".to_string(),
            usage: "/plan-skip [task]".to_string(),
            category: CommandCategory::Plan,
            requires_args: false,
            requires_editor: false,
        });

        self.register(Command {
            name: "plan-next".to_string(),
            description: "Execute next pending task".to_string(),
            usage: "/plan-next".to_string(),
            category: CommandCategory::Plan,
            requires_args: false,
            requires_editor: false,
        });
        self.add_alias("pn", "plan-next");

        self.register(Command {
            name: "plan-refine".to_string(),
            description: "Add refinement to current plan".to_string(),
            usage: "/plan-refine <refinement>".to_string(),
            category: CommandCategory::Plan,
            requires_args: true,
            requires_editor: false,
        });
        self.add_alias("pr", "plan-refine");

        // Editor integration commands (only available when connected to Neovim)
        self.register(Command {
            name: "diff".to_string(),
            description: "Show side-by-side diff view (requires Neovim)".to_string(),
            usage: "/diff <file>".to_string(),
            category: CommandCategory::Editor,
            requires_args: true,
            requires_editor: true,
        });

        self.register(Command {
            name: "autodiff".to_string(),
            description: "Toggle inline diff display (requires Neovim)".to_string(),
            usage: "/autodiff".to_string(),
            category: CommandCategory::Editor,
            requires_args: false,
            requires_editor: true,
        });

        self.register(Command {
            name: "tasks".to_string(),
            description: "Focus the tasks panel".to_string(),
            usage: "/tasks".to_string(),
            category: CommandCategory::General,
            requires_args: false,
            requires_editor: false,
        });
    }

    /// Register a command
    fn register(&mut self, command: Command) {
        self.commands.insert(command.name.clone(), command);
    }

    /// Add an alias for a command
    fn add_alias(&mut self, alias: &str, target: &str) {
        self.aliases.insert(alias.to_string(), target.to_string());
    }

    /// Resolve an alias to its target command name
    pub fn resolve_alias<'a>(&'a self, name: &'a str) -> &'a str {
        self.aliases.get(name).map(|s| s.as_str()).unwrap_or(name)
    }

    /// Get a command by name (resolving aliases)
    pub fn get_command(&self, name: &str) -> Option<&Command> {
        let resolved = self.resolve_alias(name);
        self.commands.get(resolved)
    }

    /// Check if a string is a valid command (starts with /)
    pub fn is_command(input: &str) -> bool {
        input.trim_start().starts_with('/')
    }

    /// Parse a command string into command name and arguments
    ///
    /// Returns (command_name, arguments) where command_name does not include the leading /
    pub fn parse(input: &str) -> Option<(&str, &str)> {
        let input = input.trim();
        if !input.starts_with('/') {
            return None;
        }

        let without_slash = &input[1..];
        if without_slash.is_empty() {
            return None;
        }

        // Split on first whitespace
        if let Some(space_idx) = without_slash.find(char::is_whitespace) {
            let cmd = &without_slash[..space_idx];
            let args = without_slash[space_idx..].trim_start();
            Some((cmd, args))
        } else {
            Some((without_slash, ""))
        }
    }

    /// Execute a command and return the result
    pub fn execute(&self, input: &str) -> CommandResult {
        self.execute_with_editor_state(input, false)
    }

    /// Execute a command with editor connection state
    ///
    /// If `editor_connected` is false and the command requires editor,
    /// returns an error message instead of executing.
    pub fn execute_with_editor_state(&self, input: &str, editor_connected: bool) -> CommandResult {
        let Some((cmd_name, args)) = Self::parse(input) else {
            return CommandResult::Error("Invalid command format".to_string());
        };

        let resolved_name = self.resolve_alias(cmd_name);

        let Some(command) = self.commands.get(resolved_name) else {
            return CommandResult::Error(format!("Unknown command: /{}", cmd_name));
        };

        // Check if command requires editor connection
        if command.requires_editor && !editor_connected {
            return CommandResult::Error(format!(
                "Command /{} requires Neovim connection. Run with --socket to enable editor integration.",
                cmd_name
            ));
        }

        // Check if command requires arguments
        if command.requires_args && args.is_empty() {
            return CommandResult::Error(format!(
                "Command /{} requires arguments. Usage: {}",
                cmd_name, command.usage
            ));
        }

        // Execute based on command name
        match resolved_name {
            // General commands
            "help" => self.execute_help(args, editor_connected),
            "clear" => CommandResult::ClearHistory,
            "exit" => CommandResult::Exit,
            "interrupt" => CommandResult::Interrupt,
            "tasks" => CommandResult::FocusTasks,

            // Model/Provider commands
            // /model now starts the two-step flow: provider → model selection
            // Requirements: 1.1, 1.7
            "model" | "provider" => {
                if args.is_empty() {
                    // Start two-step flow with provider selection
                    CommandResult::ShowPicker(PickerType::Provider)
                } else {
                    CommandResult::Message(format!("Switching to model: {}", args))
                }
            }
            "ollama" => CommandResult::Message("Switching to Ollama provider".to_string()),
            "claude" => CommandResult::Message("Switching to Claude provider".to_string()),
            "openai" => CommandResult::Message("Switching to OpenAI provider".to_string()),

            // Session commands
            "new" => CommandResult::NewSession,
            "sessions" => CommandResult::ShowPicker(PickerType::Session),
            "delete" => CommandResult::DeleteSession,

            // Agent mode commands
            "plan" => CommandResult::ChangeMode(AgentModeChange::Plan),
            "build" => CommandResult::ChangeMode(AgentModeChange::Build),
            "review" => CommandResult::ChangeMode(AgentModeChange::Review),

            // Utility commands
            "thinking" => CommandResult::Toggle(ToggleSetting::Thinking),
            "stats" => CommandResult::ShowStats,
            "cost" => CommandResult::ShowCost,
            "compact" => CommandResult::Compact,
            "usage" => CommandResult::ShowUsage,
            "usage-total" => CommandResult::ShowUsageTotal,
            "usage-open" => CommandResult::OpenUsageDashboard,

            // Attachment commands
            "attach" => CommandResult::AttachFile(args.to_string()),
            "clear-attachments" => CommandResult::ClearAttachments,

            // Plan commands
            "plan-status" => CommandResult::PlanStatus,
            "plan-list" => CommandResult::PlanList,
            "plan-done" => {
                let task_arg = if args.is_empty() {
                    None
                } else {
                    Some(args.to_string())
                };
                CommandResult::PlanDone(task_arg)
            }
            "plan-skip" => {
                let task_arg = if args.is_empty() {
                    None
                } else {
                    Some(args.to_string())
                };
                CommandResult::PlanSkip(task_arg)
            }
            "plan-next" => CommandResult::PlanNext,
            "plan-refine" => CommandResult::PlanRefine(args.to_string()),

            // Editor commands
            "diff" => CommandResult::ShowDiff(args.to_string()),
            "autodiff" => CommandResult::ToggleAutoDiff,

            _ => CommandResult::Error(format!("Command not implemented: /{}", resolved_name)),
        }
    }

    /// Execute the help command
    fn execute_help(&self, args: &str, editor_connected: bool) -> CommandResult {
        if args.is_empty() {
            // Show all commands grouped by category
            CommandResult::ShowHelp(self.generate_help_text_with_editor_state(editor_connected))
        } else {
            // Show help for specific command
            let cmd_name = args.trim();
            let resolved = self.resolve_alias(cmd_name);
            if let Some(cmd) = self.commands.get(resolved) {
                let availability = if cmd.requires_editor && !editor_connected {
                    " (unavailable - requires Neovim)"
                } else {
                    ""
                };
                let help = format!(
                    "/{} - {}{}\nUsage: {}\nCategory: {}",
                    cmd.name,
                    cmd.description,
                    availability,
                    cmd.usage,
                    cmd.category.display_name()
                );
                CommandResult::ShowHelp(help)
            } else {
                CommandResult::Error(format!("Unknown command: {}", cmd_name))
            }
        }
    }

    /// Generate help text for all commands with editor state awareness
    fn generate_help_text_with_editor_state(&self, editor_connected: bool) -> String {
        let mut help = String::from("Available commands:\n\n");

        // Group commands by category
        let categories = [
            CommandCategory::General,
            CommandCategory::Model,
            CommandCategory::Session,
            CommandCategory::Mode,
            CommandCategory::Utility,
            CommandCategory::Attachment,
            CommandCategory::Plan,
            CommandCategory::Editor,
        ];

        for category in categories {
            let cmds: Vec<_> = self
                .commands
                .values()
                .filter(|c| c.category == category)
                .collect();

            if cmds.is_empty() {
                continue;
            }

            // For Editor category, indicate if commands are unavailable
            let category_suffix = if category == CommandCategory::Editor && !editor_connected {
                " (requires Neovim)"
            } else {
                ""
            };

            help.push_str(&format!(
                "{}{}:\n",
                category.display_name(),
                category_suffix
            ));
            for cmd in cmds {
                // Find aliases for this command
                let aliases: Vec<_> = self
                    .aliases
                    .iter()
                    .filter(|(_, target)| *target == &cmd.name)
                    .map(|(alias, _)| format!("/{}", alias))
                    .collect();

                let alias_str = if aliases.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", aliases.join(", "))
                };

                // Mark unavailable commands
                let unavailable = if cmd.requires_editor && !editor_connected {
                    " [unavailable]"
                } else {
                    ""
                };

                help.push_str(&format!(
                    "  /{}{} - {}{}\n",
                    cmd.name, alias_str, cmd.description, unavailable
                ));
            }
            help.push('\n');
        }

        if !editor_connected {
            help.push_str(
                "Note: Some commands require Neovim connection. Run with --socket to enable.\n",
            );
        }

        help
    }

    /// Generate help text for all commands (legacy, assumes no editor)
    fn generate_help_text(&self) -> String {
        self.generate_help_text_with_editor_state(false)
    }

    /// Get completions for a partial command
    pub fn get_completions(&self, prefix: &str) -> Vec<String> {
        let prefix = prefix.trim_start_matches('/');
        if prefix.is_empty() {
            // Return all command names
            let mut completions: Vec<_> = self.commands.keys().map(|k| format!("/{}", k)).collect();
            completions.sort();
            return completions;
        }

        let mut completions = Vec::new();

        // Match command names
        for name in self.commands.keys() {
            if name.starts_with(prefix) {
                completions.push(format!("/{}", name));
            }
        }

        // Match aliases
        for alias in self.aliases.keys() {
            if alias.starts_with(prefix) {
                completions.push(format!("/{}", alias));
            }
        }

        completions.sort();
        completions.dedup();
        completions
    }

    /// Get all registered commands
    pub fn commands(&self) -> impl Iterator<Item = &Command> {
        self.commands.values()
    }

    /// Get all aliases
    pub fn aliases(&self) -> &HashMap<String, String> {
        &self.aliases
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_handler_new() {
        let handler = CommandHandler::new();
        // Should have registered commands
        assert!(!handler.commands.is_empty());
        // Should have registered aliases
        assert!(!handler.aliases.is_empty());
    }

    #[test]
    fn test_is_command() {
        assert!(CommandHandler::is_command("/help"));
        assert!(CommandHandler::is_command("  /help"));
        assert!(CommandHandler::is_command("/model gpt-4"));
        assert!(!CommandHandler::is_command("help"));
        assert!(!CommandHandler::is_command("hello /world"));
        assert!(!CommandHandler::is_command(""));
    }

    #[test]
    fn test_parse_simple_command() {
        let result = CommandHandler::parse("/help");
        assert_eq!(result, Some(("help", "")));
    }

    #[test]
    fn test_parse_command_with_args() {
        let result = CommandHandler::parse("/model gpt-4");
        assert_eq!(result, Some(("model", "gpt-4")));
    }

    #[test]
    fn test_parse_command_with_multiple_args() {
        let result = CommandHandler::parse("/attach path/to/file.txt");
        assert_eq!(result, Some(("attach", "path/to/file.txt")));
    }

    #[test]
    fn test_parse_command_with_extra_whitespace() {
        // Input is trimmed, so trailing whitespace is removed
        // Then args has leading whitespace trimmed
        let result = CommandHandler::parse("  /model   gpt-4  ");
        assert_eq!(result, Some(("model", "gpt-4")));
    }

    #[test]
    fn test_parse_invalid_command() {
        assert_eq!(CommandHandler::parse("help"), None);
        assert_eq!(CommandHandler::parse("/"), None);
        assert_eq!(CommandHandler::parse(""), None);
    }

    #[test]
    fn test_resolve_alias() {
        let handler = CommandHandler::new();

        // Direct command
        assert_eq!(handler.resolve_alias("help"), "help");

        // Aliases
        assert_eq!(handler.resolve_alias("c"), "clear");
        assert_eq!(handler.resolve_alias("q"), "exit");
        assert_eq!(handler.resolve_alias("quit"), "exit");
        assert_eq!(handler.resolve_alias("m"), "model");
        assert_eq!(handler.resolve_alias("gpt"), "openai");
        assert_eq!(handler.resolve_alias("verbose"), "thinking");
        assert_eq!(handler.resolve_alias("debug"), "thinking");
        assert_eq!(handler.resolve_alias("s"), "stats");
    }

    #[test]
    fn test_get_command() {
        let handler = CommandHandler::new();

        // Direct command
        let cmd = handler.get_command("help");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "help");

        // Via alias
        let cmd = handler.get_command("c");
        assert!(cmd.is_some());
        assert_eq!(cmd.unwrap().name, "clear");

        // Unknown command
        assert!(handler.get_command("unknown").is_none());
    }

    #[test]
    fn test_execute_help() {
        let handler = CommandHandler::new();
        let result = handler.execute("/help");
        assert!(matches!(result, CommandResult::ShowHelp(_)));
    }

    #[test]
    fn test_execute_help_specific_command() {
        let handler = CommandHandler::new();
        let result = handler.execute("/help clear");
        if let CommandResult::ShowHelp(text) = result {
            assert!(text.contains("clear"));
            assert!(text.contains("Clear chat history"));
        } else {
            panic!("Expected ShowHelp result");
        }
    }

    #[test]
    fn test_execute_clear() {
        let handler = CommandHandler::new();
        assert_eq!(handler.execute("/clear"), CommandResult::ClearHistory);
        assert_eq!(handler.execute("/c"), CommandResult::ClearHistory);
    }

    #[test]
    fn test_execute_exit() {
        let handler = CommandHandler::new();
        assert_eq!(handler.execute("/exit"), CommandResult::Exit);
        assert_eq!(handler.execute("/quit"), CommandResult::Exit);
        assert_eq!(handler.execute("/q"), CommandResult::Exit);
        assert_eq!(handler.execute("/close"), CommandResult::Exit);
    }

    #[test]
    fn test_execute_mode_commands() {
        let handler = CommandHandler::new();
        assert_eq!(
            handler.execute("/plan"),
            CommandResult::ChangeMode(AgentModeChange::Plan)
        );
        assert_eq!(
            handler.execute("/build"),
            CommandResult::ChangeMode(AgentModeChange::Build)
        );
        assert_eq!(
            handler.execute("/review"),
            CommandResult::ChangeMode(AgentModeChange::Review)
        );
    }

    #[test]
    fn test_execute_model_picker() {
        let handler = CommandHandler::new();
        // /model now starts two-step flow with provider selection first
        assert_eq!(
            handler.execute("/model"),
            CommandResult::ShowPicker(PickerType::Provider)
        );
        assert_eq!(
            handler.execute("/m"),
            CommandResult::ShowPicker(PickerType::Provider)
        );
    }

    #[test]
    fn test_execute_provider_picker() {
        let handler = CommandHandler::new();
        // /provider is now an alias to /model, starts two-step flow
        assert_eq!(
            handler.execute("/provider"),
            CommandResult::ShowPicker(PickerType::Provider)
        );
    }

    #[test]
    fn test_execute_session_picker() {
        let handler = CommandHandler::new();
        assert_eq!(
            handler.execute("/sessions"),
            CommandResult::ShowPicker(PickerType::Session)
        );
        assert_eq!(
            handler.execute("/session"),
            CommandResult::ShowPicker(PickerType::Session)
        );
    }

    #[test]
    fn test_execute_thinking_toggle() {
        let handler = CommandHandler::new();
        assert_eq!(
            handler.execute("/thinking"),
            CommandResult::Toggle(ToggleSetting::Thinking)
        );
        assert_eq!(
            handler.execute("/verbose"),
            CommandResult::Toggle(ToggleSetting::Thinking)
        );
        assert_eq!(
            handler.execute("/debug"),
            CommandResult::Toggle(ToggleSetting::Thinking)
        );
    }

    #[test]
    fn test_execute_utility_commands() {
        let handler = CommandHandler::new();
        assert_eq!(handler.execute("/stats"), CommandResult::ShowStats);
        assert_eq!(handler.execute("/s"), CommandResult::ShowStats);
        assert_eq!(handler.execute("/cost"), CommandResult::ShowCost);
        assert_eq!(handler.execute("/usage"), CommandResult::ShowUsage);
        assert_eq!(
            handler.execute("/usage-total"),
            CommandResult::ShowUsageTotal
        );
        assert_eq!(
            handler.execute("/usage-open"),
            CommandResult::OpenUsageDashboard
        );
        assert_eq!(handler.execute("/compact"), CommandResult::Compact);
    }

    #[test]
    fn test_execute_session_commands() {
        let handler = CommandHandler::new();
        assert_eq!(handler.execute("/new"), CommandResult::NewSession);
        assert_eq!(handler.execute("/delete"), CommandResult::DeleteSession);
    }

    #[test]
    fn test_execute_interrupt() {
        let handler = CommandHandler::new();
        assert_eq!(handler.execute("/interrupt"), CommandResult::Interrupt);
        assert_eq!(handler.execute("/stop"), CommandResult::Interrupt);
        assert_eq!(handler.execute("/cancel"), CommandResult::Interrupt);
    }

    #[test]
    fn test_execute_unknown_command() {
        let handler = CommandHandler::new();
        let result = handler.execute("/unknown");
        assert!(matches!(result, CommandResult::Error(_)));
        if let CommandResult::Error(msg) = result {
            assert!(msg.contains("Unknown command"));
        }
    }

    #[test]
    fn test_execute_command_requires_args() {
        let handler = CommandHandler::new();
        let result = handler.execute("/attach");
        assert!(matches!(result, CommandResult::Error(_)));
        if let CommandResult::Error(msg) = result {
            assert!(msg.contains("requires arguments"));
        }
    }

    #[test]
    fn test_execute_command_with_args() {
        let handler = CommandHandler::new();
        let result = handler.execute("/attach test.txt");
        assert!(matches!(result, CommandResult::AttachFile(_)));
    }

    #[test]
    fn test_get_completions_empty() {
        let handler = CommandHandler::new();
        let completions = handler.get_completions("");
        assert!(!completions.is_empty());
        // Should include all commands
        assert!(completions.contains(&"/help".to_string()));
        assert!(completions.contains(&"/clear".to_string()));
    }

    #[test]
    fn test_get_completions_partial() {
        let handler = CommandHandler::new();

        // Commands starting with "c"
        let completions = handler.get_completions("c");
        assert!(completions.contains(&"/clear".to_string()));
        assert!(completions.contains(&"/claude".to_string()));
        assert!(completions.contains(&"/compact".to_string()));
        assert!(completions.contains(&"/cost".to_string()));

        // Commands starting with "plan"
        let completions = handler.get_completions("plan");
        assert!(completions.contains(&"/plan".to_string()));
        assert!(completions.contains(&"/plan-status".to_string()));
        assert!(completions.contains(&"/plan-list".to_string()));
    }

    #[test]
    fn test_get_completions_with_slash() {
        let handler = CommandHandler::new();
        let completions = handler.get_completions("/he");
        assert!(completions.contains(&"/help".to_string()));
    }

    #[test]
    fn test_get_completions_includes_aliases() {
        let handler = CommandHandler::new();
        let completions = handler.get_completions("q");
        assert!(completions.contains(&"/q".to_string()));
        assert!(completions.contains(&"/quit".to_string()));
    }

    #[test]
    fn test_command_categories() {
        let handler = CommandHandler::new();

        // Check that commands are in correct categories
        let help = handler.get_command("help").unwrap();
        assert_eq!(help.category, CommandCategory::General);

        let model = handler.get_command("model").unwrap();
        assert_eq!(model.category, CommandCategory::Model);

        let sessions = handler.get_command("sessions").unwrap();
        assert_eq!(sessions.category, CommandCategory::Session);

        let plan = handler.get_command("plan").unwrap();
        assert_eq!(plan.category, CommandCategory::Mode);

        let thinking = handler.get_command("thinking").unwrap();
        assert_eq!(thinking.category, CommandCategory::Utility);

        let attach = handler.get_command("attach").unwrap();
        assert_eq!(attach.category, CommandCategory::Attachment);

        let plan_status = handler.get_command("plan-status").unwrap();
        assert_eq!(plan_status.category, CommandCategory::Plan);

        let diff = handler.get_command("diff").unwrap();
        assert_eq!(diff.category, CommandCategory::Editor);
    }

    #[test]
    fn test_generate_help_text() {
        let handler = CommandHandler::new();
        let help = handler.generate_help_text();

        // Should contain category headers
        assert!(help.contains("General:"));
        assert!(help.contains("Model & Provider:"));
        assert!(help.contains("Session:"));
        assert!(help.contains("Agent Mode:"));
        assert!(help.contains("Utility:"));

        // Should contain commands
        assert!(help.contains("/help"));
        assert!(help.contains("/clear"));
        assert!(help.contains("/model"));

        // Should contain aliases
        assert!(help.contains("/c"));
        assert!(help.contains("/q"));
    }
}

/// Property-based tests for slash command parsing
///
/// **Property 9: Slash Command Parsing**
/// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a valid command name (alphanumeric with hyphens)
    fn arb_command_name() -> impl Strategy<Value = String> {
        "[a-z][a-z0-9-]{0,15}".prop_map(|s| s.to_string())
    }

    /// Generate optional arguments (any printable string)
    fn arb_args() -> impl Strategy<Value = String> {
        prop::option::of("[a-zA-Z0-9 ./\\-_]{0,50}").prop_map(|opt| opt.unwrap_or_default())
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// For any registered slash command with optional arguments, parsing the command
        /// string SHALL correctly extract the command name and arguments.
        #[test]
        fn prop_parse_extracts_command_and_args(
            cmd_name in arb_command_name(),
            args in arb_args(),
        ) {
            let input = if args.is_empty() {
                format!("/{}", cmd_name)
            } else {
                format!("/{} {}", cmd_name, args)
            };

            let result = CommandHandler::parse(&input);
            prop_assert!(result.is_some(), "Parse should succeed for valid command format");

            let (parsed_cmd, parsed_args) = result.unwrap();
            prop_assert_eq!(parsed_cmd, cmd_name.as_str(),
                "Command name should be extracted correctly");

            // Args should match (trimmed)
            let expected_args = args.trim();
            prop_assert_eq!(parsed_args.trim(), expected_args,
                "Arguments should be extracted correctly");
        }

        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// For any alias, resolving it SHALL return the target command name.
        #[test]
        fn prop_alias_resolution_is_consistent(alias_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let aliases: Vec<_> = handler.aliases().iter().collect();

            if aliases.is_empty() {
                return Ok(());
            }

            let idx = alias_idx % aliases.len();
            let (alias, target) = aliases[idx];

            // Resolving alias should return target
            let resolved = handler.resolve_alias(alias);
            prop_assert_eq!(resolved, target.as_str(),
                "Alias '{}' should resolve to '{}'", alias, target);

            // Target should be a valid command
            let cmd = handler.get_command(target);
            prop_assert!(cmd.is_some(),
                "Target '{}' should be a valid command", target);

            // Getting command via alias should work
            let cmd_via_alias = handler.get_command(alias);
            prop_assert!(cmd_via_alias.is_some(),
                "Getting command via alias '{}' should work", alias);
            prop_assert_eq!(cmd_via_alias.unwrap().name.as_str(), target.as_str(),
                "Command via alias should have correct name");
        }

        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// For any registered command, executing it SHALL not panic and SHALL
        /// return a valid CommandResult.
        #[test]
        fn prop_execute_registered_commands_no_panic(cmd_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let commands: Vec<_> = handler.commands().collect();

            if commands.is_empty() {
                return Ok(());
            }

            let idx = cmd_idx % commands.len();
            let cmd = &commands[idx];

            // Execute command (with dummy args if required)
            // Use execute_with_editor_state with editor_connected=true to test all commands
            let input = if cmd.requires_args {
                format!("/{} test_arg", cmd.name)
            } else {
                format!("/{}", cmd.name)
            };

            // Should not panic - execute with editor connected to allow all commands
            let result = handler.execute_with_editor_state(&input, true);

            // Result should not be an error for valid commands with proper args
            match result {
                CommandResult::Error(msg) => {
                    // Only acceptable error is "not implemented"
                    prop_assert!(
                        msg.contains("not implemented") || msg.contains("Unknown"),
                        "Unexpected error for command '{}': {}", cmd.name, msg
                    );
                }
                _ => {
                    // Any other result is fine
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// For any prefix, completions SHALL only contain commands that start
        /// with that prefix.
        #[test]
        fn prop_completions_match_prefix(prefix in "[a-z]{0,5}") {
            let handler = CommandHandler::new();
            let completions = handler.get_completions(&prefix);

            for completion in &completions {
                // Remove leading / for comparison
                let cmd_name = completion.trim_start_matches('/');
                prop_assert!(
                    cmd_name.starts_with(&prefix),
                    "Completion '{}' should start with prefix '{}'",
                    completion, prefix
                );
            }
        }

        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// Parsing non-command input (not starting with /) SHALL return None.
        #[test]
        fn prop_parse_rejects_non_commands(input in "[a-zA-Z0-9 ]{1,50}") {
            // Ensure input doesn't start with /
            if input.trim().starts_with('/') {
                return Ok(());
            }

            let result = CommandHandler::parse(&input);
            prop_assert!(result.is_none(),
                "Non-command input '{}' should not parse", input);
        }

        /// **Feature: terminal-tui-chat, Property 9: Slash Command Parsing**
        /// **Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5**
        ///
        /// For any command, is_command should correctly identify command strings.
        #[test]
        fn prop_is_command_consistent_with_parse(
            cmd_name in arb_command_name(),
            args in arb_args(),
            leading_spaces in 0usize..5,
        ) {
            let spaces = " ".repeat(leading_spaces);
            let input = if args.is_empty() {
                format!("{}/{}", spaces, cmd_name)
            } else {
                format!("{}/{} {}", spaces, cmd_name, args)
            };

            let is_cmd = CommandHandler::is_command(&input);
            let parses = CommandHandler::parse(&input).is_some();

            // If it parses, it should be identified as a command
            if parses {
                prop_assert!(is_cmd,
                    "Input '{}' parses but is_command returns false", input);
            }

            // If is_command is true, it should parse (for valid format)
            if is_cmd && !cmd_name.is_empty() {
                prop_assert!(parses,
                    "Input '{}' is_command but doesn't parse", input);
            }
        }

        /// **Feature: terminal-tui-chat, Property 15: Standalone Mode Feature Set**
        /// **Validates: Requirements 12.1, 12.2, 12.3, 12.5**
        ///
        /// For any TUI instance running without socket connection (standalone mode),
        /// all non-editor features SHALL be available, and editor-specific commands
        /// SHALL be gracefully disabled with an appropriate error message.
        #[test]
        fn prop_standalone_mode_non_editor_commands_available(cmd_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let commands: Vec<_> = handler.commands().collect();

            if commands.is_empty() {
                return Ok(());
            }

            let idx = cmd_idx % commands.len();
            let cmd = &commands[idx];

            // Skip editor-requiring commands for this test
            if cmd.requires_editor {
                return Ok(());
            }

            // Execute command in standalone mode (editor_connected = false)
            let input = if cmd.requires_args {
                format!("/{} test_arg", cmd.name)
            } else {
                format!("/{}", cmd.name)
            };

            let result = handler.execute_with_editor_state(&input, false);

            // Non-editor commands should NOT return an error about Neovim connection
            match result {
                CommandResult::Error(msg) => {
                    prop_assert!(
                        !msg.contains("requires Neovim"),
                        "Non-editor command '{}' should be available in standalone mode, got: {}",
                        cmd.name, msg
                    );
                }
                _ => {
                    // Any other result is fine for non-editor commands
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 15: Standalone Mode Feature Set**
        /// **Validates: Requirements 12.1, 12.2, 12.3, 12.5**
        ///
        /// For any editor-requiring command executed in standalone mode,
        /// the system SHALL return an error indicating Neovim connection is required.
        #[test]
        fn prop_standalone_mode_editor_commands_disabled(cmd_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let commands: Vec<_> = handler.commands().filter(|c| c.requires_editor).collect();

            if commands.is_empty() {
                return Ok(());
            }

            let idx = cmd_idx % commands.len();
            let cmd = &commands[idx];

            // Execute editor command in standalone mode (editor_connected = false)
            let input = if cmd.requires_args {
                format!("/{} test_arg", cmd.name)
            } else {
                format!("/{}", cmd.name)
            };

            let result = handler.execute_with_editor_state(&input, false);

            // Editor commands should return an error about Neovim connection
            match result {
                CommandResult::Error(msg) => {
                    prop_assert!(
                        msg.contains("requires Neovim"),
                        "Editor command '{}' should indicate Neovim requirement in standalone mode, got: {}",
                        cmd.name, msg
                    );
                }
                _ => {
                    prop_assert!(false,
                        "Editor command '{}' should return error in standalone mode, got: {:?}",
                        cmd.name, result
                    );
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 15: Standalone Mode Feature Set**
        /// **Validates: Requirements 12.1, 12.2, 12.3, 12.5**
        ///
        /// For any editor-requiring command executed with editor connected,
        /// the system SHALL execute the command normally (not return Neovim error).
        #[test]
        fn prop_connected_mode_editor_commands_available(cmd_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let commands: Vec<_> = handler.commands().filter(|c| c.requires_editor).collect();

            if commands.is_empty() {
                return Ok(());
            }

            let idx = cmd_idx % commands.len();
            let cmd = &commands[idx];

            // Execute editor command with editor connected
            let input = if cmd.requires_args {
                format!("/{} test_arg", cmd.name)
            } else {
                format!("/{}", cmd.name)
            };

            let result = handler.execute_with_editor_state(&input, true);

            // Editor commands should NOT return Neovim connection error when connected
            match result {
                CommandResult::Error(msg) => {
                    prop_assert!(
                        !msg.contains("requires Neovim"),
                        "Editor command '{}' should be available when connected, got: {}",
                        cmd.name, msg
                    );
                }
                _ => {
                    // Any other result is fine when connected
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 15: Standalone Mode Feature Set**
        /// **Validates: Requirements 12.1, 12.2, 12.3, 12.5**
        ///
        /// The is_available_standalone method SHALL correctly identify commands
        /// that can run without editor connection.
        #[test]
        fn prop_is_available_standalone_consistent(cmd_idx in 0usize..100) {
            let handler = CommandHandler::new();
            let commands: Vec<_> = handler.commands().collect();

            if commands.is_empty() {
                return Ok(());
            }

            let idx = cmd_idx % commands.len();
            let cmd = &commands[idx];

            // is_available_standalone should be the inverse of requires_editor
            prop_assert_eq!(
                cmd.is_available_standalone(),
                !cmd.requires_editor,
                "is_available_standalone should be inverse of requires_editor for '{}'",
                cmd.name
            );
        }

        /// **Feature: unified-model-selection, Property 6: Provider Availability Indication**
        /// **Validates: Requirements 3.1, 3.2**
        ///
        /// For any provider in the picker, if the provider's API key environment variable
        /// is not set, the provider SHALL be marked as unavailable with a configuration hint.
        #[test]
        fn prop_provider_availability_indication(provider_idx in 0usize..10) {
            let providers = ProviderInfo::get_all_providers();

            if providers.is_empty() {
                return Ok(());
            }

            let idx = provider_idx % providers.len();
            let provider = &providers[idx];

            // Check that availability matches the environment variable check
            let expected_available = ProviderInfo::check_provider_availability(&provider.id);
            prop_assert_eq!(
                provider.available,
                expected_available,
                "Provider '{}' availability should match env var check", provider.id
            );

            // If provider is unavailable, it should have a hint
            if !provider.available {
                prop_assert!(
                    provider.hint.is_some(),
                    "Unavailable provider '{}' should have a configuration hint", provider.id
                );

                // Hint should be non-empty
                let hint = provider.hint.as_ref().unwrap();
                prop_assert!(
                    !hint.is_empty(),
                    "Configuration hint for '{}' should not be empty", provider.id
                );
            }

            // If provider is available, hint should be None
            if provider.available {
                prop_assert!(
                    provider.hint.is_none(),
                    "Available provider '{}' should not have a hint", provider.id
                );
            }

            // Provider should have non-empty id, name, and description
            prop_assert!(
                !provider.id.is_empty(),
                "Provider id should not be empty"
            );
            prop_assert!(
                !provider.name.is_empty(),
                "Provider name should not be empty"
            );
            prop_assert!(
                !provider.description.is_empty(),
                "Provider description should not be empty"
            );
        }

        /// **Feature: unified-model-selection, Property 6: Provider Availability Indication**
        /// **Validates: Requirements 3.1, 3.2**
        ///
        /// For any known provider ID, check_provider_availability should return
        /// consistent results based on environment variables.
        #[test]
        fn prop_provider_availability_check_consistency(
            provider_id in prop::sample::select(vec!["openai", "claude", "ollama"])
        ) {
            // Call check twice - should return same result
            let first_check = ProviderInfo::check_provider_availability(provider_id);
            let second_check = ProviderInfo::check_provider_availability(provider_id);

            prop_assert_eq!(
                first_check,
                second_check,
                "Provider availability check should be consistent for '{}'", provider_id
            );

            // Get hint should return Some for known providers
            let hint = ProviderInfo::get_provider_hint(provider_id);
            prop_assert!(
                hint.is_some(),
                "Known provider '{}' should have a hint available", provider_id
            );
        }

        /// **Feature: unified-model-selection, Property 6: Provider Availability Indication**
        /// **Validates: Requirements 3.1, 3.2**
        ///
        /// For any unknown provider ID, check_provider_availability should return false.
        #[test]
        fn prop_unknown_provider_unavailable(
            unknown_id in "[a-z]{5,10}".prop_filter("Not a known provider",
                |s| s != "openai" && s != "claude" && s != "ollama")
        ) {
            let available = ProviderInfo::check_provider_availability(&unknown_id);
            prop_assert!(
                !available,
                "Unknown provider '{}' should not be available", unknown_id
            );

            let hint = ProviderInfo::get_provider_hint(&unknown_id);
            prop_assert!(
                hint.is_none(),
                "Unknown provider '{}' should not have a hint", unknown_id
            );
        }
    }
}
