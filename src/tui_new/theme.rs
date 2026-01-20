//! Theme system for the TUI
//!
//! Provides color schemes and styling for all UI components.
//!
//! ## Adding Custom Themes
//!
//! You can add themes in three ways:
//!
//! 1. **Add a preset** - Add variant to `ThemePreset` enum and implement the theme function
//! 2. **Load from Neovim** - Use `Theme::from_nvim_highlights()` with highlight groups
//! 3. **Load from config** - Define theme in TOML config file
//!
//! ## Loading Neovim Themes
//!
//! From Neovim, you can export current colorscheme:
//! ```vim
//! :lua vim.api.nvim_exec_lua('return vim.api.nvim_get_hl(0, {})', {})
//! ```
//!
//! Or programmatically via the Lua plugin:
//! ```lua
//! require('tark').export_theme()
//! ```

use ratatui::style::Color;

// Re-export ThemePreset from ui_backend
pub use crate::ui_backend::ThemePreset;

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

    // Bubble background colors (derived from theme colors)
    pub user_bubble_bg: Color,
    pub agent_bubble_bg: Color,
    pub thinking_bubble_bg: Color,
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

            // Bubble backgrounds - blue-tinted for user, green-tinted for agent
            user_bubble_bg: Color::Rgb(35, 40, 60),
            agent_bubble_bg: Color::Rgb(35, 50, 45),
            thinking_bubble_bg: Color::Rgb(42, 43, 55),
        }
    }

    /// Nord theme
    pub fn nord() -> Self {
        Self {
            bg_main: Color::Rgb(46, 52, 64), // nord0
            bg_dark: Color::Rgb(40, 44, 52), // darker variant
            bg_sidebar: Color::Rgb(46, 52, 64),
            bg_code: Color::Rgb(59, 66, 82), // nord1

            border: Color::Rgb(76, 86, 106),           // nord3
            border_focused: Color::Rgb(136, 192, 208), // nord8

            text_primary: Color::Rgb(236, 239, 244), // nord6
            text_secondary: Color::Rgb(229, 233, 240), // nord5
            text_muted: Color::Rgb(143, 157, 179),   // nord4

            cyan: Color::Rgb(136, 192, 208),   // nord8
            blue: Color::Rgb(129, 161, 193),   // nord9
            green: Color::Rgb(163, 190, 140),  // nord14
            yellow: Color::Rgb(235, 203, 139), // nord13
            red: Color::Rgb(191, 97, 106),     // nord11
            purple: Color::Rgb(180, 142, 173), // nord15

            system_fg: Color::Rgb(136, 192, 208),
            user_bubble: Color::Rgb(129, 161, 193),
            agent_bubble: Color::Rgb(163, 190, 140),
            tool_fg: Color::Rgb(143, 157, 179),
            thinking_fg: Color::Rgb(235, 203, 139),
            question_fg: Color::Rgb(136, 192, 208),
            command_fg: Color::Rgb(163, 190, 140),

            // Nord bubble backgrounds - frost blue for user, aurora green for agent
            user_bubble_bg: Color::Rgb(46, 55, 70),
            agent_bubble_bg: Color::Rgb(50, 60, 55),
            thinking_bubble_bg: Color::Rgb(55, 62, 75),
        }
    }

    /// Dracula theme
    pub fn dracula() -> Self {
        Self {
            bg_main: Color::Rgb(40, 42, 54), // background
            bg_dark: Color::Rgb(33, 34, 44), // darker variant
            bg_sidebar: Color::Rgb(40, 42, 54),
            bg_code: Color::Rgb(68, 71, 90), // current line

            border: Color::Rgb(68, 71, 90),
            border_focused: Color::Rgb(189, 147, 249), // purple

            text_primary: Color::Rgb(248, 248, 242), // foreground
            text_secondary: Color::Rgb(241, 250, 140), // yellow
            text_muted: Color::Rgb(98, 114, 164),    // comment

            cyan: Color::Rgb(139, 233, 253),   // cyan
            blue: Color::Rgb(189, 147, 249),   // purple
            green: Color::Rgb(80, 250, 123),   // green
            yellow: Color::Rgb(241, 250, 140), // yellow
            red: Color::Rgb(255, 85, 85),      // red
            purple: Color::Rgb(189, 147, 249), // purple

            system_fg: Color::Rgb(139, 233, 253),
            user_bubble: Color::Rgb(189, 147, 249),
            agent_bubble: Color::Rgb(80, 250, 123),
            tool_fg: Color::Rgb(98, 114, 164),
            thinking_fg: Color::Rgb(241, 250, 140),
            question_fg: Color::Rgb(139, 233, 253),
            command_fg: Color::Rgb(80, 250, 123),

            // Dracula bubble backgrounds - purple-tinted for user, green-tinted for agent
            user_bubble_bg: Color::Rgb(55, 45, 70),
            agent_bubble_bg: Color::Rgb(40, 55, 45),
            thinking_bubble_bg: Color::Rgb(55, 55, 65),
        }
    }

    /// GitHub Dark theme
    pub fn github_dark() -> Self {
        Self {
            bg_main: Color::Rgb(13, 17, 23), // canvas default
            bg_dark: Color::Rgb(1, 4, 9),    // canvas inset
            bg_sidebar: Color::Rgb(13, 17, 23),
            bg_code: Color::Rgb(22, 27, 34), // neutral muted

            border: Color::Rgb(48, 54, 61),           // border default
            border_focused: Color::Rgb(31, 111, 235), // accent emphasis

            text_primary: Color::Rgb(230, 237, 243), // fg default
            text_secondary: Color::Rgb(139, 148, 158), // fg muted
            text_muted: Color::Rgb(110, 118, 129),   // fg subtle

            cyan: Color::Rgb(125, 196, 228),   // accent fg
            blue: Color::Rgb(88, 166, 255),    // done emphasis
            green: Color::Rgb(87, 171, 90),    // success fg
            yellow: Color::Rgb(201, 137, 16),  // attention fg
            red: Color::Rgb(248, 81, 73),      // danger fg
            purple: Color::Rgb(163, 113, 247), // sponsors fg

            system_fg: Color::Rgb(125, 196, 228),
            user_bubble: Color::Rgb(88, 166, 255),
            agent_bubble: Color::Rgb(87, 171, 90),
            tool_fg: Color::Rgb(139, 148, 158),
            thinking_fg: Color::Rgb(201, 137, 16),
            question_fg: Color::Rgb(125, 196, 228),
            command_fg: Color::Rgb(87, 171, 90),

            // GitHub bubble backgrounds - blue-tinted for user, green-tinted for agent
            user_bubble_bg: Color::Rgb(20, 30, 45),
            agent_bubble_bg: Color::Rgb(20, 35, 28),
            thinking_bubble_bg: Color::Rgb(30, 35, 45),
        }
    }

    /// One Dark theme
    pub fn one_dark() -> Self {
        Self {
            bg_main: Color::Rgb(40, 44, 52), // background
            bg_dark: Color::Rgb(33, 37, 43), // darker variant
            bg_sidebar: Color::Rgb(40, 44, 52),
            bg_code: Color::Rgb(53, 59, 69), // gutter

            border: Color::Rgb(53, 59, 69),
            border_focused: Color::Rgb(97, 175, 239), // blue

            text_primary: Color::Rgb(171, 178, 191), // foreground
            text_secondary: Color::Rgb(152, 160, 173), // gray
            text_muted: Color::Rgb(92, 99, 112),     // comment

            cyan: Color::Rgb(86, 182, 194),    // cyan
            blue: Color::Rgb(97, 175, 239),    // blue
            green: Color::Rgb(152, 195, 121),  // green
            yellow: Color::Rgb(229, 192, 123), // yellow
            red: Color::Rgb(224, 108, 117),    // red
            purple: Color::Rgb(198, 120, 221), // purple

            system_fg: Color::Rgb(86, 182, 194),
            user_bubble: Color::Rgb(97, 175, 239),
            agent_bubble: Color::Rgb(152, 195, 121),
            tool_fg: Color::Rgb(152, 160, 173),
            thinking_fg: Color::Rgb(229, 192, 123),
            question_fg: Color::Rgb(86, 182, 194),
            command_fg: Color::Rgb(152, 195, 121),

            // One Dark bubble backgrounds - blue-tinted for user, green-tinted for agent
            user_bubble_bg: Color::Rgb(40, 50, 65),
            agent_bubble_bg: Color::Rgb(45, 55, 48),
            thinking_bubble_bg: Color::Rgb(50, 55, 62),
        }
    }

    /// Gruvbox Dark theme
    pub fn gruvbox_dark() -> Self {
        Self {
            bg_main: Color::Rgb(40, 40, 40), // bg0
            bg_dark: Color::Rgb(29, 32, 33), // bg0_h
            bg_sidebar: Color::Rgb(40, 40, 40),
            bg_code: Color::Rgb(60, 56, 54), // bg1

            border: Color::Rgb(80, 73, 69),            // bg2
            border_focused: Color::Rgb(131, 165, 152), // aqua

            text_primary: Color::Rgb(235, 219, 178),   // fg0
            text_secondary: Color::Rgb(213, 196, 161), // fg1
            text_muted: Color::Rgb(146, 131, 116),     // fg4

            cyan: Color::Rgb(131, 165, 152),   // aqua
            blue: Color::Rgb(131, 165, 152),   // blue
            green: Color::Rgb(184, 187, 38),   // green
            yellow: Color::Rgb(250, 189, 47),  // yellow
            red: Color::Rgb(251, 73, 52),      // red
            purple: Color::Rgb(211, 134, 155), // purple

            system_fg: Color::Rgb(131, 165, 152),
            user_bubble: Color::Rgb(131, 165, 152),
            agent_bubble: Color::Rgb(184, 187, 38),
            tool_fg: Color::Rgb(213, 196, 161),
            thinking_fg: Color::Rgb(250, 189, 47),
            question_fg: Color::Rgb(131, 165, 152),
            command_fg: Color::Rgb(184, 187, 38),

            // Gruvbox bubble backgrounds - aqua-tinted for user, olive-tinted for agent
            user_bubble_bg: Color::Rgb(50, 55, 55),
            agent_bubble_bg: Color::Rgb(55, 55, 40),
            thinking_bubble_bg: Color::Rgb(55, 50, 45),
        }
    }

    /// Tokyo Night theme
    pub fn tokyo_night() -> Self {
        Self {
            bg_main: Color::Rgb(26, 27, 38), // bg
            bg_dark: Color::Rgb(22, 22, 30), // bg_dark
            bg_sidebar: Color::Rgb(26, 27, 38),
            bg_code: Color::Rgb(30, 31, 44), // bg_highlight

            border: Color::Rgb(41, 46, 66),            // border
            border_focused: Color::Rgb(122, 162, 247), // blue

            text_primary: Color::Rgb(192, 202, 245),   // fg
            text_secondary: Color::Rgb(169, 177, 214), // fg_dark
            text_muted: Color::Rgb(86, 95, 137),       // comment

            cyan: Color::Rgb(125, 207, 255),   // cyan
            blue: Color::Rgb(122, 162, 247),   // blue
            green: Color::Rgb(158, 206, 106),  // green
            yellow: Color::Rgb(224, 175, 104), // yellow
            red: Color::Rgb(247, 118, 142),    // red
            purple: Color::Rgb(187, 154, 247), // purple

            system_fg: Color::Rgb(125, 207, 255),
            user_bubble: Color::Rgb(122, 162, 247),
            agent_bubble: Color::Rgb(158, 206, 106),
            tool_fg: Color::Rgb(169, 177, 214),
            thinking_fg: Color::Rgb(224, 175, 104),
            question_fg: Color::Rgb(125, 207, 255),
            command_fg: Color::Rgb(158, 206, 106),

            // Tokyo Night bubble backgrounds - blue-tinted for user, green-tinted for agent
            user_bubble_bg: Color::Rgb(30, 35, 55),
            agent_bubble_bg: Color::Rgb(32, 45, 38),
            thinking_bubble_bg: Color::Rgb(38, 40, 52),
        }
    }

    /// Get theme from preset
    pub fn from_preset(preset: ThemePreset) -> Self {
        match preset {
            ThemePreset::CatppuccinMocha => Self::catppuccin_mocha(),
            ThemePreset::CatppuccinMacchiato => Self::catppuccin_mocha(), // TODO: implement proper theme
            ThemePreset::CatppuccinFrappe => Self::catppuccin_mocha(), // TODO: implement proper theme
            ThemePreset::CatppuccinLatte => Self::catppuccin_mocha(), // TODO: implement proper theme
            ThemePreset::Nord => Self::nord(),
            ThemePreset::Dracula => Self::dracula(),
            ThemePreset::OneDark => Self::one_dark(),
            ThemePreset::GruvboxDark => Self::gruvbox_dark(),
            ThemePreset::GruvboxLight => Self::gruvbox_dark(), // TODO: implement proper theme
            ThemePreset::SolarizedDark => Self::nord(),        // TODO: implement proper theme
            ThemePreset::SolarizedLight => Self::nord(),       // TODO: implement proper theme
            ThemePreset::TokyoNight => Self::tokyo_night(),
        }
    }
}

impl Theme {
    /// Create a theme from Neovim highlight groups
    ///
    /// This allows loading any Neovim colorscheme by reading its highlight groups
    /// Usage: Call from Neovim with `:lua vim.api.nvim_exec_lua('return vim.api.nvim_get_hl(0, {})', {})`
    #[allow(dead_code)]
    pub fn from_nvim_highlights(groups: &std::collections::HashMap<String, NvimHighlight>) -> Self {
        // Extract common highlight groups
        let normal = groups.get("Normal").cloned().unwrap_or_default();
        let comment = groups.get("Comment").cloned().unwrap_or_default();
        let constant = groups.get("Constant").cloned().unwrap_or_default();
        let identifier = groups.get("Identifier").cloned().unwrap_or_default();
        let statement = groups.get("Statement").cloned().unwrap_or_default();
        let type_hl = groups.get("Type").cloned().unwrap_or_default();
        let special = groups.get("Special").cloned().unwrap_or_default();
        let error = groups.get("Error").cloned().unwrap_or_default();

        Self {
            bg_main: normal.bg.unwrap_or(Color::Rgb(30, 30, 46)),
            bg_dark: Color::Rgb(24, 24, 37),
            bg_sidebar: normal.bg.unwrap_or(Color::Rgb(30, 30, 46)),
            bg_code: Color::Rgb(49, 50, 68),

            border: comment.fg.unwrap_or(Color::Rgb(49, 50, 68)),
            border_focused: identifier.fg.unwrap_or(Color::Rgb(137, 180, 250)),

            text_primary: normal.fg.unwrap_or(Color::Rgb(205, 214, 244)),
            text_secondary: comment.fg.unwrap_or(Color::Rgb(166, 173, 200)),
            text_muted: comment.fg.unwrap_or(Color::Rgb(108, 112, 134)),

            cyan: special.fg.unwrap_or(Color::Rgb(148, 226, 213)),
            blue: identifier.fg.unwrap_or(Color::Rgb(137, 180, 250)),
            green: constant.fg.unwrap_or(Color::Rgb(166, 227, 161)),
            yellow: type_hl.fg.unwrap_or(Color::Rgb(249, 226, 175)),
            red: error.fg.unwrap_or(Color::Rgb(243, 139, 168)),
            purple: statement.fg.unwrap_or(Color::Rgb(203, 166, 247)),

            system_fg: special.fg.unwrap_or(Color::Rgb(148, 226, 213)),
            user_bubble: identifier.fg.unwrap_or(Color::Rgb(137, 180, 250)),
            agent_bubble: constant.fg.unwrap_or(Color::Rgb(166, 227, 161)),
            tool_fg: comment.fg.unwrap_or(Color::Rgb(166, 173, 200)),
            thinking_fg: type_hl.fg.unwrap_or(Color::Rgb(249, 226, 175)),
            question_fg: special.fg.unwrap_or(Color::Rgb(137, 220, 235)),
            command_fg: constant.fg.unwrap_or(Color::Rgb(166, 227, 161)),

            // Default bubble backgrounds based on the loaded theme
            user_bubble_bg: Color::Rgb(35, 40, 60),
            agent_bubble_bg: Color::Rgb(35, 50, 45),
            thinking_bubble_bg: Color::Rgb(42, 43, 55),
        }
    }
}

/// Neovim highlight group representation
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct NvimHighlight {
    pub fg: Option<Color>,
    pub bg: Option<Color>,
    pub bold: bool,
    pub italic: bool,
}

impl NvimHighlight {
    /// Parse color from Neovim hex string (e.g., "#89b4fa")
    pub fn parse_color(hex: &str) -> Option<Color> {
        if !hex.starts_with('#') || hex.len() != 7 {
            return None;
        }

        let r = u8::from_str_radix(&hex[1..3], 16).ok()?;
        let g = u8::from_str_radix(&hex[3..5], 16).ok()?;
        let b = u8::from_str_radix(&hex[5..7], 16).ok()?;

        Some(Color::Rgb(r, g, b))
    }
}
