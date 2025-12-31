//! TUI Configuration management
//!
//! Handles loading and merging TUI-specific configuration from config files.
//! Configuration is read from `~/.config/tark/config.toml` and merged with defaults.

#![allow(dead_code)]

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// TUI-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct TuiConfig {
    /// Attachment configuration
    pub attachments: AttachmentsConfig,
    /// Theme configuration
    pub theme: ThemeConfig,
    /// Keybinding configuration
    pub keybindings: KeybindingsConfig,
}

/// Attachment-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AttachmentsConfig {
    /// Maximum file size in bytes (default: 10MB)
    pub max_attachment_size: u64,
    /// Maximum number of attachments per message (default: 10)
    pub max_attachments: usize,
}

impl Default for AttachmentsConfig {
    fn default() -> Self {
        Self {
            max_attachment_size: 10 * 1024 * 1024, // 10MB
            max_attachments: 10,
        }
    }
}

/// Theme configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    /// Theme name: "dark" or "light"
    pub name: String,
    /// Custom colors (optional overrides)
    pub colors: ThemeColors,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "dark".to_string(),
            colors: ThemeColors::default(),
        }
    }
}

/// Theme color configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ThemeColors {
    /// Primary accent color (e.g., "cyan", "#00FFFF")
    pub primary: Option<String>,
    /// Secondary accent color
    pub secondary: Option<String>,
    /// Background color
    pub background: Option<String>,
    /// Foreground/text color
    pub foreground: Option<String>,
    /// Error color
    pub error: Option<String>,
    /// Warning color
    pub warning: Option<String>,
    /// Success color
    pub success: Option<String>,
}

/// Keybinding configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct KeybindingsConfig {
    /// Custom keybindings (action -> key)
    pub custom: HashMap<String, String>,
}

impl TuiConfig {
    /// Load TUI configuration from the default config file
    ///
    /// Reads from `~/.config/tark/config.toml` and extracts the `[tui]` section.
    /// If the file doesn't exist or the section is missing, returns defaults.
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;

        if config_path.exists() {
            Self::load_from_path(&config_path)
        } else {
            Ok(Self::default())
        }
    }

    /// Load TUI configuration from a specific path
    pub fn load_from_path(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::load_from_str(&content)
    }

    /// Load TUI configuration from a TOML string
    pub fn load_from_str(content: &str) -> Result<Self> {
        // Parse the full config file
        let full_config: toml::Value = toml::from_str(content)?;

        // Extract the [tui] section if it exists
        if let Some(tui_section) = full_config.get("tui") {
            let tui_config: TuiConfig = tui_section.clone().try_into()?;
            Ok(tui_config)
        } else {
            // No [tui] section, return defaults
            Ok(Self::default())
        }
    }

    /// Get the configuration file path
    pub fn config_path() -> Result<PathBuf> {
        if let Some(proj_dirs) = directories::ProjectDirs::from("", "", "tark") {
            let config_dir = proj_dirs.config_dir();
            Ok(config_dir.join("config.toml"))
        } else {
            // Fallback to home directory
            let home = dirs::home_dir()
                .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
            Ok(home.join(".config").join("tark").join("config.toml"))
        }
    }

    /// Merge this config with another, preferring values from `other`
    pub fn merge(&mut self, other: &TuiConfig) {
        // Merge attachments
        self.attachments.max_attachment_size = other.attachments.max_attachment_size;
        self.attachments.max_attachments = other.attachments.max_attachments;

        // Merge theme
        self.theme.name = other.theme.name.clone();
        if other.theme.colors.primary.is_some() {
            self.theme.colors.primary = other.theme.colors.primary.clone();
        }
        if other.theme.colors.secondary.is_some() {
            self.theme.colors.secondary = other.theme.colors.secondary.clone();
        }
        if other.theme.colors.background.is_some() {
            self.theme.colors.background = other.theme.colors.background.clone();
        }
        if other.theme.colors.foreground.is_some() {
            self.theme.colors.foreground = other.theme.colors.foreground.clone();
        }
        if other.theme.colors.error.is_some() {
            self.theme.colors.error = other.theme.colors.error.clone();
        }
        if other.theme.colors.warning.is_some() {
            self.theme.colors.warning = other.theme.colors.warning.clone();
        }
        if other.theme.colors.success.is_some() {
            self.theme.colors.success = other.theme.colors.success.clone();
        }

        // Merge keybindings (add/override custom bindings)
        for (action, key) in &other.keybindings.custom {
            self.keybindings.custom.insert(action.clone(), key.clone());
        }
    }

    /// Convert to AttachmentConfig for use with AttachmentManager
    pub fn to_attachment_config(&self) -> super::attachments::AttachmentConfig {
        super::attachments::AttachmentConfig {
            max_attachment_size: self.attachments.max_attachment_size,
            max_attachments: self.attachments.max_attachments,
            temp_dir: std::env::temp_dir().join("tark-attachments"),
        }
    }

    /// Check if the theme is dark
    pub fn is_dark_theme(&self) -> bool {
        self.theme.name.to_lowercase() == "dark"
    }

    /// Get a custom keybinding for an action
    pub fn get_keybinding(&self, action: &str) -> Option<&String> {
        self.keybindings.custom.get(action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TuiConfig::default();
        assert_eq!(config.attachments.max_attachment_size, 10 * 1024 * 1024);
        assert_eq!(config.attachments.max_attachments, 10);
        assert_eq!(config.theme.name, "dark");
        assert!(config.keybindings.custom.is_empty());
    }

    #[test]
    fn test_load_from_str_empty() {
        let content = "";
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(config.attachments.max_attachment_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_load_from_str_no_tui_section() {
        let content = r#"
[llm]
default_provider = "openai"
"#;
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(config.attachments.max_attachment_size, 10 * 1024 * 1024);
    }

    #[test]
    fn test_load_from_str_with_tui_section() {
        let content = r#"
[tui.attachments]
max_attachment_size = 5242880
max_attachments = 5

[tui.theme]
name = "light"
"#;
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(config.attachments.max_attachment_size, 5 * 1024 * 1024);
        assert_eq!(config.attachments.max_attachments, 5);
        assert_eq!(config.theme.name, "light");
    }

    #[test]
    fn test_load_from_str_partial_config() {
        let content = r#"
[tui.attachments]
max_attachment_size = 1048576
"#;
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(config.attachments.max_attachment_size, 1024 * 1024);
        // Default for max_attachments
        assert_eq!(config.attachments.max_attachments, 10);
    }

    #[test]
    fn test_load_from_str_with_keybindings() {
        let content = r#"
[tui.keybindings.custom]
quit = "Ctrl-q"
submit = "Ctrl-Enter"
"#;
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(
            config.keybindings.custom.get("quit"),
            Some(&"Ctrl-q".to_string())
        );
        assert_eq!(
            config.keybindings.custom.get("submit"),
            Some(&"Ctrl-Enter".to_string())
        );
    }

    #[test]
    fn test_load_from_str_with_theme_colors() {
        let content = r##"
[tui.theme]
name = "custom"

[tui.theme.colors]
primary = "#00FF00"
error = "red"
"##;
        let config = TuiConfig::load_from_str(content).unwrap();
        assert_eq!(config.theme.name, "custom");
        assert_eq!(config.theme.colors.primary, Some("#00FF00".to_string()));
        assert_eq!(config.theme.colors.error, Some("red".to_string()));
        assert!(config.theme.colors.secondary.is_none());
    }

    #[test]
    fn test_merge_configs() {
        let mut base = TuiConfig::default();
        let other = TuiConfig {
            attachments: AttachmentsConfig {
                max_attachment_size: 5 * 1024 * 1024,
                max_attachments: 5,
            },
            theme: ThemeConfig {
                name: "light".to_string(),
                colors: ThemeColors {
                    primary: Some("blue".to_string()),
                    ..Default::default()
                },
            },
            keybindings: KeybindingsConfig {
                custom: {
                    let mut map = HashMap::new();
                    map.insert("quit".to_string(), "Ctrl-q".to_string());
                    map
                },
            },
        };

        base.merge(&other);

        assert_eq!(base.attachments.max_attachment_size, 5 * 1024 * 1024);
        assert_eq!(base.attachments.max_attachments, 5);
        assert_eq!(base.theme.name, "light");
        assert_eq!(base.theme.colors.primary, Some("blue".to_string()));
        assert_eq!(
            base.keybindings.custom.get("quit"),
            Some(&"Ctrl-q".to_string())
        );
    }

    #[test]
    fn test_to_attachment_config() {
        let config = TuiConfig {
            attachments: AttachmentsConfig {
                max_attachment_size: 5 * 1024 * 1024,
                max_attachments: 3,
            },
            ..Default::default()
        };

        let attachment_config = config.to_attachment_config();
        assert_eq!(attachment_config.max_attachment_size, 5 * 1024 * 1024);
        assert_eq!(attachment_config.max_attachments, 3);
    }

    #[test]
    fn test_is_dark_theme() {
        let mut config = TuiConfig::default();
        assert!(config.is_dark_theme());

        config.theme.name = "light".to_string();
        assert!(!config.is_dark_theme());

        config.theme.name = "DARK".to_string();
        assert!(config.is_dark_theme());
    }

    #[test]
    fn test_get_keybinding() {
        let mut config = TuiConfig::default();
        assert!(config.get_keybinding("quit").is_none());

        config
            .keybindings
            .custom
            .insert("quit".to_string(), "Ctrl-q".to_string());
        assert_eq!(config.get_keybinding("quit"), Some(&"Ctrl-q".to_string()));
    }

    #[test]
    fn test_config_path() {
        let path = TuiConfig::config_path();
        assert!(path.is_ok());
        let path = path.unwrap();
        assert!(path.to_string_lossy().contains("config.toml"));
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Generate a random attachment size (1KB to 100MB)
    fn arb_attachment_size() -> impl Strategy<Value = u64> {
        1024u64..100 * 1024 * 1024
    }

    /// Generate a random attachment count (1 to 50)
    fn arb_attachment_count() -> impl Strategy<Value = usize> {
        1usize..50
    }

    /// Generate a random theme name
    fn arb_theme_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("dark".to_string()),
            Just("light".to_string()),
            Just("custom".to_string()),
        ]
    }

    /// Generate a random color string
    fn arb_color() -> impl Strategy<Value = Option<String>> {
        prop_oneof![
            Just(None),
            Just(Some("red".to_string())),
            Just(Some("blue".to_string())),
            Just(Some("green".to_string())),
            Just(Some("cyan".to_string())),
            Just(Some("magenta".to_string())),
            Just(Some("yellow".to_string())),
        ]
    }

    /// Generate a random keybinding action name
    fn arb_action_name() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("quit".to_string()),
            Just("submit".to_string()),
            Just("cancel".to_string()),
            Just("focus_next".to_string()),
            Just("focus_prev".to_string()),
        ]
    }

    /// Generate a random keybinding value
    fn arb_keybinding_value() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("Ctrl-q".to_string()),
            Just("Ctrl-c".to_string()),
            Just("Ctrl-Enter".to_string()),
            Just("Tab".to_string()),
            Just("Escape".to_string()),
        ]
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 10: Configuration Loading**
        /// **Validates: Requirements 11.1, 11.2, 11.3, 11.4, 11.5**
        ///
        /// For any valid TOML configuration file, loading SHALL produce a config
        /// struct with all specified values, and unspecified values SHALL use defaults.
        #[test]
        fn prop_config_loading_preserves_values(
            max_size in arb_attachment_size(),
            max_count in arb_attachment_count(),
            theme_name in arb_theme_name(),
        ) {
            // Create a TOML config string with the generated values
            let config_str = format!(
                r#"
[tui.attachments]
max_attachment_size = {}
max_attachments = {}

[tui.theme]
name = "{}"
"#,
                max_size, max_count, theme_name
            );

            // Load the config
            let config = TuiConfig::load_from_str(&config_str).unwrap();

            // Verify all specified values are preserved
            prop_assert_eq!(config.attachments.max_attachment_size, max_size);
            prop_assert_eq!(config.attachments.max_attachments, max_count);
            prop_assert_eq!(config.theme.name, theme_name);
        }

        /// Property: Unspecified values use defaults
        #[test]
        fn prop_config_unspecified_uses_defaults(
            max_size in arb_attachment_size(),
        ) {
            // Create a config with only max_attachment_size specified
            let config_str = format!(
                r#"
[tui.attachments]
max_attachment_size = {}
"#,
                max_size
            );

            let config = TuiConfig::load_from_str(&config_str).unwrap();

            // Specified value should be preserved
            prop_assert_eq!(config.attachments.max_attachment_size, max_size);

            // Unspecified values should use defaults
            let defaults = TuiConfig::default();
            prop_assert_eq!(config.attachments.max_attachments, defaults.attachments.max_attachments);
            prop_assert_eq!(config.theme.name, defaults.theme.name);
            prop_assert!(config.keybindings.custom.is_empty());
        }

        /// Property: Theme colors are preserved when specified
        #[test]
        fn prop_config_theme_colors_preserved(
            primary in arb_color(),
            error in arb_color(),
        ) {
            let mut config_str = "[tui.theme]\nname = \"custom\"\n".to_string();

            if primary.is_some() || error.is_some() {
                config_str.push_str("[tui.theme.colors]\n");
                if let Some(ref p) = primary {
                    config_str.push_str(&format!("primary = \"{}\"\n", p));
                }
                if let Some(ref e) = error {
                    config_str.push_str(&format!("error = \"{}\"\n", e));
                }
            }

            let config = TuiConfig::load_from_str(&config_str).unwrap();

            prop_assert_eq!(config.theme.colors.primary, primary);
            prop_assert_eq!(config.theme.colors.error, error);
        }

        /// Property: Custom keybindings are preserved
        #[test]
        fn prop_config_keybindings_preserved(
            action in arb_action_name(),
            binding in arb_keybinding_value(),
        ) {
            let config_str = format!(
                r#"
[tui.keybindings.custom]
{} = "{}"
"#,
                action, binding
            );

            let config = TuiConfig::load_from_str(&config_str).unwrap();

            prop_assert_eq!(
                config.keybindings.custom.get(&action),
                Some(&binding)
            );
        }

        /// Property: Empty config returns defaults
        #[test]
        fn prop_empty_config_returns_defaults(_dummy in 0..1i32) {
            let config = TuiConfig::load_from_str("").unwrap();
            let defaults = TuiConfig::default();

            prop_assert_eq!(config.attachments.max_attachment_size, defaults.attachments.max_attachment_size);
            prop_assert_eq!(config.attachments.max_attachments, defaults.attachments.max_attachments);
            prop_assert_eq!(config.theme.name, defaults.theme.name);
            prop_assert!(config.keybindings.custom.is_empty());
        }

        /// Property: Config without [tui] section returns defaults
        #[test]
        fn prop_no_tui_section_returns_defaults(
            provider in prop_oneof![
                Just("openai".to_string()),
                Just("claude".to_string()),
                Just("ollama".to_string()),
            ]
        ) {
            let config_str = format!(
                r#"
[llm]
default_provider = "{}"
"#,
                provider
            );

            let config = TuiConfig::load_from_str(&config_str).unwrap();
            let defaults = TuiConfig::default();

            prop_assert_eq!(config.attachments.max_attachment_size, defaults.attachments.max_attachment_size);
            prop_assert_eq!(config.attachments.max_attachments, defaults.attachments.max_attachments);
            prop_assert_eq!(config.theme.name, defaults.theme.name);
        }

        /// Property: Config file round-trip preserves values
        #[test]
        fn prop_config_file_roundtrip(
            max_size in arb_attachment_size(),
            max_count in arb_attachment_count(),
            theme_name in arb_theme_name(),
        ) {
            let dir = TempDir::new().unwrap();
            let config_path = dir.path().join("config.toml");

            // Create a config with the generated values
            let config_str = format!(
                r#"
[tui.attachments]
max_attachment_size = {}
max_attachments = {}

[tui.theme]
name = "{}"
"#,
                max_size, max_count, theme_name
            );

            // Write to file
            std::fs::write(&config_path, &config_str).unwrap();

            // Load from file
            let config = TuiConfig::load_from_path(&config_path).unwrap();

            // Verify values are preserved
            prop_assert_eq!(config.attachments.max_attachment_size, max_size);
            prop_assert_eq!(config.attachments.max_attachments, max_count);
            prop_assert_eq!(config.theme.name, theme_name);
        }

        /// Property: Merge preserves all values from other config
        #[test]
        fn prop_config_merge_preserves_other_values(
            base_size in arb_attachment_size(),
            other_size in arb_attachment_size(),
            base_count in arb_attachment_count(),
            other_count in arb_attachment_count(),
        ) {
            let mut base = TuiConfig {
                attachments: AttachmentsConfig {
                    max_attachment_size: base_size,
                    max_attachments: base_count,
                },
                ..Default::default()
            };

            let other = TuiConfig {
                attachments: AttachmentsConfig {
                    max_attachment_size: other_size,
                    max_attachments: other_count,
                },
                ..Default::default()
            };

            base.merge(&other);

            // After merge, base should have other's values
            prop_assert_eq!(base.attachments.max_attachment_size, other_size);
            prop_assert_eq!(base.attachments.max_attachments, other_count);
        }

        /// Property: to_attachment_config preserves values
        #[test]
        fn prop_to_attachment_config_preserves_values(
            max_size in arb_attachment_size(),
            max_count in arb_attachment_count(),
        ) {
            let config = TuiConfig {
                attachments: AttachmentsConfig {
                    max_attachment_size: max_size,
                    max_attachments: max_count,
                },
                ..Default::default()
            };

            let attachment_config = config.to_attachment_config();

            prop_assert_eq!(attachment_config.max_attachment_size, max_size);
            prop_assert_eq!(attachment_config.max_attachments, max_count);
        }
    }
}
