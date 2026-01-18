//! Theme system for the TUI
//!
//! Provides color schemes and styling for all UI components.

use ratatui::style::Color;

/// Available theme presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThemePreset {
    #[default]
    CatppuccinMocha,
    Nord,
    GithubDark,
    Dracula,
    OneDark,
    GruvboxDark,
    TokyoNight,
}

/// Theme colors for the TUI
#[derive(Debug, Clone)]
pub struct Theme {
    // Background colors
    pub bg_main: Color,
    pub bg_dark: Color,
    pub bg_sidebar: Color,
    pub bg_code: Color,

    // Border colors
    pub border: Color,
    pub border_focused: Color,

    // Text colors
    pub text_primary: Color,
    pub text_secondary: Color,
    pub text_muted: Color,

    // Accent colors
    pub cyan: Color,
    pub blue: Color,
    pub green: Color,
    pub yellow: Color,
    pub red: Color,
    pub purple: Color,

    // Message colors
    pub system_fg: Color,
    pub user_bubble: Color,
    pub agent_bubble: Color,
    pub tool_fg: Color,
    pub thinking_fg: Color,
    pub question_fg: Color,
    pub command_fg: Color,
}

impl Default for Theme {
    fn default() -> Self {
        Self::catppuccin_mocha()
    }
}

impl Theme {
    /// Catppuccin Mocha theme (default)
    pub fn catppuccin_mocha() -> Self {
        Self {
            bg_main: Color::Rgb(30, 30, 46),
            bg_dark: Color::Rgb(24, 24, 37),
            bg_sidebar: Color::Rgb(30, 30, 46),
            bg_code: Color::Rgb(49, 50, 68),

            border: Color::Rgb(49, 50, 68),
            border_focused: Color::Rgb(137, 180, 250),

            text_primary: Color::Rgb(205, 214, 244),
            text_secondary: Color::Rgb(166, 173, 200),
            text_muted: Color::Rgb(108, 112, 134),

            cyan: Color::Rgb(148, 226, 213),
            blue: Color::Rgb(137, 180, 250),
            green: Color::Rgb(166, 227, 161),
            yellow: Color::Rgb(249, 226, 175),
            red: Color::Rgb(243, 139, 168),
            purple: Color::Rgb(203, 166, 247),

            system_fg: Color::Rgb(148, 226, 213),
            user_bubble: Color::Rgb(137, 180, 250),
            agent_bubble: Color::Rgb(166, 227, 161),
            tool_fg: Color::Rgb(166, 173, 200),
            thinking_fg: Color::Rgb(249, 226, 175),
            question_fg: Color::Rgb(137, 220, 235),
            command_fg: Color::Rgb(166, 227, 161),
        }
    }

    /// Get theme from preset
    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::CatppuccinMocha => Self::catppuccin_mocha(),
            // TODO: Implement other themes in Phase 14
            _ => Self::catppuccin_mocha(),
        }
    }
}
