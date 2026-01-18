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
}

/// A chat message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub thinking: Option<String>,
    pub collapsed: bool,
    pub timestamp: String,
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
    pub branch: String,
    pub total_cost: f64,
    pub model_count: usize,
    pub created_at: String,
}

/// Context file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextFile {
    pub path: String,
    pub size: usize,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePreset {
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
