//! TUI Session Preferences
//!
//! Persists user preferences per session in `.tark/sessions/<session_id>/tui_prefs.json`
//! This includes build mode, agent mode, thinking mode, model/provider selection, and theme.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::ui_backend::{AgentMode, BuildMode, ThemePreset};

/// TUI preferences that persist across sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiPreferences {
    /// Current build mode (Careful, Balanced, Manual)
    pub build_mode: BuildMode,
    /// Current agent mode (Build, Plan, Ask)
    pub agent_mode: AgentMode,
    /// Whether thinking mode is enabled
    pub thinking_enabled: bool,
    /// Selected LLM provider ID
    pub selected_provider: Option<String>,
    /// Selected model ID
    pub selected_model: Option<String>,
    /// Selected model display name (for status bar)
    pub selected_model_name: Option<String>,
    /// Current theme preset
    pub theme: ThemePreset,
}

impl Default for TuiPreferences {
    fn default() -> Self {
        Self {
            build_mode: BuildMode::default(),
            agent_mode: AgentMode::default(),
            thinking_enabled: true,
            selected_provider: None,
            selected_model: None,
            selected_model_name: None,
            theme: ThemePreset::default(),
        }
    }
}

impl TuiPreferences {
    /// Load preferences from a session directory
    pub fn load(session_dir: &Path) -> Result<Self> {
        let prefs_path = session_dir.join("tui_prefs.json");
        if prefs_path.exists() {
            let content =
                std::fs::read_to_string(&prefs_path).context("Failed to read tui_prefs.json")?;
            serde_json::from_str(&content).context("Failed to parse tui_prefs.json")
        } else {
            Ok(Self::default())
        }
    }

    /// Save preferences to a session directory
    pub fn save(&self, session_dir: &Path) -> Result<()> {
        std::fs::create_dir_all(session_dir)?;
        let prefs_path = session_dir.join("tui_prefs.json");
        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&prefs_path, content).context("Failed to write tui_prefs.json")?;
        Ok(())
    }

    /// Load preferences for a specific session ID from the .tark directory
    pub fn load_for_session(tark_dir: &Path, session_id: &str) -> Result<Self> {
        let session_dir = tark_dir.join("sessions").join(session_id);
        Self::load(&session_dir)
    }

    /// Save preferences for a specific session ID to the .tark directory
    pub fn save_for_session(&self, tark_dir: &Path, session_id: &str) -> Result<()> {
        let session_dir = tark_dir.join("sessions").join(session_id);
        self.save(&session_dir)
    }
}

/// Manager for TUI preferences with auto-save functionality
pub struct PreferencesManager {
    /// Path to .tark directory
    tark_dir: PathBuf,
    /// Current session ID
    session_id: String,
    /// Current preferences
    prefs: TuiPreferences,
    /// Whether preferences have been modified since last save
    dirty: bool,
}

impl PreferencesManager {
    /// Create a new preferences manager for a session
    pub fn new(tark_dir: PathBuf, session_id: String) -> Self {
        let prefs = TuiPreferences::load_for_session(&tark_dir, &session_id).unwrap_or_default();

        Self {
            tark_dir,
            session_id,
            prefs,
            dirty: false,
        }
    }

    /// Get current preferences
    pub fn prefs(&self) -> &TuiPreferences {
        &self.prefs
    }

    /// Update build mode
    pub fn set_build_mode(&mut self, mode: BuildMode) {
        if self.prefs.build_mode != mode {
            self.prefs.build_mode = mode;
            self.dirty = true;
        }
    }

    /// Update agent mode
    pub fn set_agent_mode(&mut self, mode: AgentMode) {
        if self.prefs.agent_mode != mode {
            self.prefs.agent_mode = mode;
            self.dirty = true;
        }
    }

    /// Update thinking mode
    pub fn set_thinking_enabled(&mut self, enabled: bool) {
        if self.prefs.thinking_enabled != enabled {
            self.prefs.thinking_enabled = enabled;
            self.dirty = true;
        }
    }

    /// Update selected provider
    pub fn set_provider(&mut self, provider: Option<String>) {
        if self.prefs.selected_provider != provider {
            self.prefs.selected_provider = provider;
            self.dirty = true;
        }
    }

    /// Update selected model
    pub fn set_model(&mut self, model_id: Option<String>, model_name: Option<String>) {
        if self.prefs.selected_model != model_id || self.prefs.selected_model_name != model_name {
            self.prefs.selected_model = model_id;
            self.prefs.selected_model_name = model_name;
            self.dirty = true;
        }
    }

    /// Update theme
    pub fn set_theme(&mut self, theme: ThemePreset) {
        if self.prefs.theme != theme {
            self.prefs.theme = theme;
            self.dirty = true;
        }
    }

    /// Save preferences if modified
    pub fn save_if_dirty(&mut self) -> Result<()> {
        if self.dirty {
            self.prefs
                .save_for_session(&self.tark_dir, &self.session_id)?;
            self.dirty = false;
        }
        Ok(())
    }

    /// Force save preferences
    pub fn save(&mut self) -> Result<()> {
        self.prefs
            .save_for_session(&self.tark_dir, &self.session_id)?;
        self.dirty = false;
        Ok(())
    }

    /// Switch to a different session
    pub fn switch_session(&mut self, session_id: String) -> Result<()> {
        // Save current session first
        self.save_if_dirty()?;

        // Load new session preferences
        self.session_id = session_id;
        self.prefs =
            TuiPreferences::load_for_session(&self.tark_dir, &self.session_id).unwrap_or_default();
        self.dirty = false;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_preferences() {
        let prefs = TuiPreferences::default();
        assert!(prefs.thinking_enabled);
        assert_eq!(prefs.agent_mode, AgentMode::default());
        assert_eq!(prefs.build_mode, BuildMode::default());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let session_dir = temp_dir.path().join("sessions").join("test-session");
        std::fs::create_dir_all(&session_dir).unwrap();

        let prefs = TuiPreferences {
            thinking_enabled: false,
            selected_provider: Some("anthropic".to_string()),
            selected_model: Some("claude-3-sonnet".to_string()),
            ..Default::default()
        };

        prefs.save(&session_dir).unwrap();

        let loaded = TuiPreferences::load(&session_dir).unwrap();
        assert!(!loaded.thinking_enabled);
        assert_eq!(loaded.selected_provider, Some("anthropic".to_string()));
        assert_eq!(loaded.selected_model, Some("claude-3-sonnet".to_string()));
    }

    #[test]
    fn test_preferences_manager() {
        let temp_dir = TempDir::new().unwrap();
        let tark_dir = temp_dir.path().to_path_buf();

        let mut manager = PreferencesManager::new(tark_dir.clone(), "test-session".to_string());

        manager.set_thinking_enabled(false);
        manager.set_provider(Some("openai".to_string()));
        manager.save().unwrap();

        // Create new manager to test loading
        let manager2 = PreferencesManager::new(tark_dir, "test-session".to_string());
        assert!(!manager2.prefs().thinking_enabled);
        assert_eq!(
            manager2.prefs().selected_provider,
            Some("openai".to_string())
        );
    }
}
