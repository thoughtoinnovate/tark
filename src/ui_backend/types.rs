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
                        format!("$ {}...", &cmd[..57])
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
    /// Tool calls made during this message
    #[serde(default)]
    pub tool_calls: Vec<ToolCallInfo>,
    /// Segments for interleaved rendering (text, tools, thinking)
    #[serde(default)]
    pub segments: Vec<MessageSegment>,
}

/// LLM Provider information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub configured: bool,
    pub icon: String,
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
    Dracula,
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
            ThemePreset::Dracula => "Dracula",
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
            ThemePreset::Dracula => "🧛",
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
            ThemePreset::Dracula,
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
