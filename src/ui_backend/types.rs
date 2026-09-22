//! Shared Types for UI Backend
//!
//! Common data structures used across the BFF layer and frontends.

use serde::{Deserialize, Serialize};

/// Message role (user, assistant, system)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
    Tool,
    Thinking,
}

/// Tool call information for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallInfo {
    pub tool: String,
    pub args: serde_json::Value,
    pub result_preview: String,
    pub error: Option<String>,
}

/// Status of a tool execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolStatus {
    /// Tool is currently executing
    Running,
    /// Tool completed successfully
    Success,
    /// Tool failed with an error
    Failed,
}

/// Active tool information for loading display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActiveToolInfo {
    /// Tool name
    pub name: String,
    /// Tool arguments (for shell: command, for file ops: path, etc.)
    pub args: serde_json::Value,
    /// Human-readable description of what the tool is doing
    pub description: String,
    /// Execution status
    pub status: ToolStatus,
    /// Result or error message (when completed)
    pub result: Option<String>,
    /// Start timestamp
    pub started_at: String,
    /// End timestamp (when completed)
    pub ended_at: Option<String>,
}

impl ActiveToolInfo {
    /// Create a new active tool
    pub fn new(name: String, args: serde_json::Value) -> Self {
        let description = Self::generate_description(&name, &args);
        Self {
            name,
            args,
            description,
            status: ToolStatus::Running,
            result: None,
            started_at: chrono::Local::now().format("%H:%M:%S").to_string(),
            ended_at: None,
        }
    }

    /// Generate a human-readable description of the tool action
    fn generate_description(name: &str, args: &serde_json::Value) -> String {
        match name {
            "shell" | "execute_command" | "run_command" => {
                if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                    // Truncate long commands
                    if cmd.len() > 60 {
                        format!("$ {}...", crate::core::truncate_at_char_boundary(cmd, 57))
                    } else {
                        format!("$ {}", cmd)
                    }
                } else {
                    "Executing command...".to_string()
                }
            }
            "read_file" | "file_preview" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let filename = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path);
                    format!("Reading {}", filename)
                } else {
                    "Reading file...".to_string()
                }
            }
            "write_file" | "edit_file" | "patch_file" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    let filename = std::path::Path::new(path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(path);
                    format!("Writing {}", filename)
                } else {
                    "Writing file...".to_string()
                }
            }
            "list_directory" => {
                if let Some(path) = args.get("path").and_then(|v| v.as_str()) {
                    format!("Listing {}", path)
                } else {
                    "Listing directory...".to_string()
                }
            }
            "grep" | "ripgrep" | "search" => {
                if let Some(pattern) = args.get("pattern").and_then(|v| v.as_str()) {
                    format!("Searching for \"{}\"", pattern)
                } else {
                    "Searching...".to_string()
                }
            }
            "ask_user" => "Waiting for your response...".to_string(),
            _ => format!("Running {}...", name),
        }
    }

    /// Mark the tool as completed
    pub fn complete(&mut self, result: String, success: bool) {
        self.status = if success {
            ToolStatus::Success
        } else {
            ToolStatus::Failed
        };
        self.result = Some(result);
        self.ended_at = Some(chrono::Local::now().format("%H:%M:%S").to_string());
    }

    /// Calculate elapsed time in seconds (returns None if not completed)
    pub fn elapsed_time(&self) -> Option<f64> {
        use chrono::NaiveTime;

        if let Some(ref ended) = self.ended_at {
            let start = NaiveTime::parse_from_str(&self.started_at, "%H:%M:%S").ok()?;
            let end = NaiveTime::parse_from_str(ended, "%H:%M:%S").ok()?;

            // Calculate duration (handle day boundary wraparound)
            let duration = if end >= start {
                end.signed_duration_since(start)
            } else {
                // Wrapped around midnight
                let duration_to_midnight = NaiveTime::from_hms_opt(23, 59, 59)?
                    .signed_duration_since(start)
                    + chrono::Duration::seconds(1);
                let duration_from_midnight =
                    end.signed_duration_since(NaiveTime::from_hms_opt(0, 0, 0)?);
                duration_to_midnight + duration_from_midnight
            };

            Some(duration.num_milliseconds() as f64 / 1000.0)
        } else {
            None
        }
    }

    /// Get status icon
    pub fn status_icon(&self) -> &'static str {
        match self.status {
            ToolStatus::Running => "⋯",
            ToolStatus::Success => "✓",
            ToolStatus::Failed => "✗",
        }
    }
}

/// Message segment for interleaved display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageSegment {
    Text(String),
    Tool(ToolCallInfo),
    Thinking(String),
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub thinking: Option<String>,
    pub collapsed: bool,
    pub timestamp: String,
    #[serde(default)]
    pub remote: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default)]
    pub context_transient: bool,
    /// Tool calls made during this message
    #[serde(default)]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Segments for interleaved rendering (text, tools, thinking)
    #[serde(default)]
    pub segments: Vec<MessageSegment>,
    /// Original tool arguments (for rich rendering of tools like think)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_args: Option<serde_json::Value>,
}

/// Archived conversation chunk metadata (UI-friendly)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveChunkInfo {
    pub filename: String,
    pub created_at: String,
    pub sequence: usize,
    pub message_count: usize,
}

/// Source of an LLM provider
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProviderSource {
    /// Built-in native provider (OpenAI, Claude, etc.)
    #[default]
    Native,
    /// Provider from an installed plugin
    Plugin,
}

/// LLM Provider information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub configured: bool,
    pub icon: String,
    /// Source of this provider (native or plugin)
    #[serde(default)]
    pub source: ProviderSource,
}

/// LLM Model information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provider: String,
    pub context_window: usize,
    pub max_tokens: usize,
}

/// Session information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub session_name: String,
    pub total_cost: f64,
    pub model_count: usize,
    pub created_at: String,
}

/// Context file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub size: usize,
    pub token_count: usize,
    pub added_at: String,
}

/// Attachment token tracked in the input buffer
#[derive(Debug, Clone)]
pub struct AttachmentToken {
    pub token: String,
    pub paths: Vec<String>,
}

/// Attachment information for display in UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    /// Original filename
    pub filename: String,
    /// Full path to the file
    pub path: String,
    /// Human-readable file size (e.g., "1.5KB")
    pub size_display: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// File type indicator (e.g., "📄" for text, "📷" for image)
    pub type_icon: String,
    /// MIME type
    pub mime_type: String,
    /// Whether this is an image attachment
    pub is_image: bool,
    /// Timestamp when added
    pub added_at: String,
}

/// Task information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub name: String,
    pub status: TaskStatus,
    pub created_at: String,
}

/// Lifecycle status of a lightweight subagent (mirrors `agent::subagent`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentStatus {
    Queued,
    Running,
    WaitingInput,
    Completed,
    Failed,
    Killed,
}

impl SubagentStatus {
    /// Single-glyph status marker for the sidebar (matches plan §7 mocks).
    pub fn glyph(&self) -> &'static str {
        match self {
            SubagentStatus::Queued => "○",
            SubagentStatus::Running | SubagentStatus::WaitingInput => "●",
            SubagentStatus::Completed => "✓",
            SubagentStatus::Failed => "✗",
            SubagentStatus::Killed => "⊗",
        }
    }
}

/// One subagent row for the sidebar (immutable display snapshot).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentInfo {
    /// Full child id (`S:sub:uuid`).
    pub id: String,
    pub parent_session: String,
    pub title: String,
    pub status: SubagentStatus,
    pub provider: String,
    pub model: String,
    pub effort: String,
    /// `true` when model/effort differ from the parent (shows `*` marker).
    pub overridden: bool,
    /// Last log line, truncated for the row preview.
    pub preview: String,
    /// Retained tail for the detail modal (bounded, oldest dropped first).
    #[serde(default)]
    pub log_tail: Vec<String>,
    /// Unread log chunks since the detail modal was last opened.
    pub unread: u32,
    pub elapsed_s: u64,
    /// Parallel tool fan-out in use (`⇉ n/5`).
    pub tools_used: usize,
    pub tools_cap: usize,
}

/// A queued (not yet started) spawn, for the sidebar queue section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueuedSubagent {
    pub id: String,
    pub title: String,
    pub position: usize,
}

/// Sidebar filter tabs for the Subagents section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SubagentFilter {
    All,
    /// Running + waiting + queued (default: quiet view).
    #[default]
    Active,
    /// Completed + failed + killed.
    Done,
}

/// Session-scoped subagent permission grant (plan §B5/C2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionGrant {
    /// Agent ids with write-proxy approval (`*` = all agents in session).
    pub write_agents: Vec<String>,
    /// Agent ids with shell-proxy approval.
    pub shell_agents: Vec<String>,
    /// Auto-deny anything beyond read-only + safe shell, no prompts.
    pub never_ask: bool,
}

/// Live-editable subagent settings snapshot (plan §C3 settings modal).
///
/// Mirrors `[agent.subagents]`, `[agent.parallel_tools]`, and
/// `[agent.subagents.models]` from `config.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubagentSettingsState {
    /// `auto` or `manual` (cap mode).
    pub mode: String,
    pub max_subagents: usize,
    pub min_subagents: usize,
    /// `auto` or `manual` (tool fan-out mode).
    pub tools_mode: String,
    pub max_parallel_tools: usize,
    /// `inherit` or `pinned` (model inheritance).
    pub pin_mode: String,
    pub pin_provider: String,
    pub pin_model: String,
    pub pin_effort: String,
}

impl Default for SubagentSettingsState {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            max_subagents: 5,
            min_subagents: 1,
            tools_mode: "auto".to_string(),
            max_parallel_tools: 5,
            pin_mode: "inherit".to_string(),
            pin_provider: String::new(),
            pin_model: String::new(),
            pin_effort: String::new(),
        }
    }
}

impl SubagentSettingsState {
    /// One-line summary for the status bar / modal header.
    pub fn badge(&self) -> String {
        let model = if self.pin_mode == "pinned"
            && (!self.pin_provider.is_empty() || !self.pin_model.is_empty())
        {
            format!(
                "{}/{}",
                if self.pin_provider.is_empty() {
                    "inherit"
                } else {
                    &self.pin_provider
                },
                if self.pin_model.is_empty() {
                    "inherit"
                } else {
                    &self.pin_model
                }
            )
        } else {
            "inherit".to_string()
        };
        format!("{}·{}·{}", self.mode, self.tools_mode, model)
    }
}

/// Task status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TaskStatus {
    Queued,
    Active,
    Completed,
    Failed,
}

/// Git change information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitChangeInfo {
    pub file: String,
    pub status: GitStatus,
    pub additions: usize,
    pub deletions: usize,
}

/// Plugin widget information for sidebar display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginWidgetInfo {
    pub plugin_id: String,
    pub attributes: serde_json::Value,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Git file status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GitStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
}

/// Status bar information
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    pub message: Option<String>,
    pub llm_connected: bool,
    pub processing: bool,
    pub tokens_used: usize,
    pub tokens_total: usize,
}

/// Diff rendering mode for tool previews
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffViewMode {
    #[default]
    Auto,
    Inline,
    Split,
}

impl DiffViewMode {
    pub fn display_name(&self) -> &'static str {
        match self {
            DiffViewMode::Auto => "Auto",
            DiffViewMode::Inline => "Inline",
            DiffViewMode::Split => "Split",
        }
    }

    pub fn next(self) -> Self {
        match self {
            DiffViewMode::Auto => DiffViewMode::Split,
            DiffViewMode::Split => DiffViewMode::Inline,
            DiffViewMode::Inline => DiffViewMode::Auto,
        }
    }
}

/// Modal content
#[derive(Debug, Clone)]
pub enum ModalContent {
    Help,
    ProviderPicker { providers: Vec<ProviderInfo> },
    ModelPicker { models: Vec<ModelInfo> },
    FilePicker { current_dir: String },
    ThemePicker { themes: Vec<ThemePreset> },
    Question { text: String, options: Vec<String> },
}

/// Theme preset
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ThemePreset {
    #[default]
    CatppuccinMocha,
    CatppuccinMacchiato,
    CatppuccinFrappe,
    CatppuccinLatte,
    Nord,
    TokyoNight,
    GruvboxDark,
    GruvboxLight,
    SolarizedDark,
    SolarizedLight,
    OneDark,
}

impl ThemePreset {
    pub fn display_name(&self) -> &'static str {
        match self {
            ThemePreset::CatppuccinMocha => "Catppuccin Mocha",
            ThemePreset::CatppuccinMacchiato => "Catppuccin Macchiato",
            ThemePreset::CatppuccinFrappe => "Catppuccin Frappé",
            ThemePreset::CatppuccinLatte => "Catppuccin Latte",
            ThemePreset::Nord => "Nord",
            ThemePreset::TokyoNight => "Tokyo Night",
            ThemePreset::GruvboxDark => "Gruvbox Dark",
            ThemePreset::GruvboxLight => "Gruvbox Light",
            ThemePreset::SolarizedDark => "Solarized Dark",
            ThemePreset::SolarizedLight => "Solarized Light",
            ThemePreset::OneDark => "One Dark",
        }
    }

    /// Get icon for this theme preset
    pub fn icon(&self) -> &'static str {
        match self {
            ThemePreset::CatppuccinMocha => "🐱",
            ThemePreset::CatppuccinMacchiato => "🐱",
            ThemePreset::CatppuccinFrappe => "🐱",
            ThemePreset::CatppuccinLatte => "🐱",
            ThemePreset::Nord => "❄️",
            ThemePreset::TokyoNight => "🌃",
            ThemePreset::GruvboxDark => "🌰",
            ThemePreset::GruvboxLight => "🌰",
            ThemePreset::SolarizedDark => "☀️",
            ThemePreset::SolarizedLight => "☀️",
            ThemePreset::OneDark => "🌑",
        }
    }

    pub fn all() -> Vec<Self> {
        vec![
            ThemePreset::CatppuccinMocha,
            ThemePreset::CatppuccinMacchiato,
            ThemePreset::CatppuccinFrappe,
            ThemePreset::CatppuccinLatte,
            ThemePreset::Nord,
            ThemePreset::TokyoNight,
            ThemePreset::GruvboxDark,
            ThemePreset::GruvboxLight,
            ThemePreset::SolarizedDark,
            ThemePreset::SolarizedLight,
            ThemePreset::OneDark,
        ]
    }
}
