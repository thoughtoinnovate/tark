//! Application configuration for the TUI
//!
//! Configurable settings for agent name, icons, and display options.

/// Application configuration
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Full agent name displayed in header
    pub agent_name: String,
    /// Short agent name for messages
    pub agent_name_short: String,
    /// Version string
    pub version: String,
    /// Default working directory path
    pub default_path: String,
    /// Icon for header
    pub header_icon: String,
    /// Icon for agent messages
    pub agent_icon: String,
    /// User's display name
    pub user_name: String,
    /// Icon for user messages
    pub user_icon: String,
    /// Max visible lines for thinking blocks
    pub thinking_max_lines: usize,
}

impl Default for AppConfig {
    fn default() -> Self {
        // Auto-detect username from environment
        let user_name = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "You".to_string());

        Self {
            agent_name: "Tark".to_string(),
            agent_name_short: "Tark".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            default_path: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "~/".to_string()),
            header_icon: "🖥".to_string(),
            agent_icon: "🤖".to_string(),
            user_name,
            user_icon: "👤".to_string(),
            thinking_max_lines: crate::config::Config::load()
                .unwrap_or_default()
                .thinking
                .max_visible_lines,
        }
    }
}
