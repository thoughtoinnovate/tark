//! Modal Widget
//!
//! Base modal component for dialogs and pickers
//! Features: 05-09 (modals)

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::tui_new::theme::{Theme, ThemePreset};

/// A modal dialog that renders centered on screen
pub struct Modal<'a> {
    /// Modal title
    title: &'a str,
    /// Modal content lines
    content: Vec<Line<'a>>,
    /// Theme for styling
    theme: &'a Theme,
    /// Width percentage (0.0 - 1.0)
    width_percent: u16,
    /// Height percentage (0.0 - 1.0)
    height_percent: u16,
}

impl<'a> Modal<'a> {
    /// Create a new modal
    pub fn new(title: &'a str, theme: &'a Theme) -> Self {
        Self {
            title,
            content: Vec::new(),
            theme,
            width_percent: 60,
            height_percent: 60,
        }
    }

    /// Set content lines
    pub fn content(mut self, content: Vec<Line<'a>>) -> Self {
        self.content = content;
        self
    }

    /// Set width percentage
    pub fn width(mut self, percent: u16) -> Self {
        self.width_percent = percent;
        self
    }

    /// Set height percentage
    pub fn height(mut self, percent: u16) -> Self {
        self.height_percent = percent;
        self
    }

    /// Calculate the centered area for the modal
    fn centered_rect(&self, area: Rect) -> Rect {
        let popup_width = area.width * self.width_percent / 100;
        let popup_height = area.height * self.height_percent / 100;

        let x = (area.width.saturating_sub(popup_width)) / 2;
        let y = (area.height.saturating_sub(popup_height)) / 2;

        Rect::new(x, y, popup_width, popup_height)
    }
}

impl Widget for Modal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_area = self.centered_rect(area);

        // Clear the area behind the modal
        Clear.render(modal_area, buf);

        // Render modal with rounded corners
        let block = Block::default()
            .title(Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    self.title,
                    Style::default()
                        .fg(self.theme.text_primary)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ]))
            .title_alignment(ratatui::layout::Alignment::Left)
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.cyan))
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Render content
        let paragraph = Paragraph::new(self.content.clone())
            .style(Style::default().fg(self.theme.text_primary));
        paragraph.render(inner, buf);
    }
}

/// Help modal showing keyboard shortcuts
pub struct HelpModal<'a> {
    theme: &'a Theme,
}

impl<'a> HelpModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self { theme }
    }

    fn shortcuts(&self) -> Vec<(&'static str, &'static str)> {
        vec![
            ("Ctrl+C / Ctrl+Q", "Quit application"),
            ("Enter", "Submit message"),
            ("Escape", "Clear input / Close modal"),
            ("Tab", "Next focus"),
            ("Shift+Tab", "Previous focus"),
            ("Ctrl+T", "Toggle thinking mode"),
            ("Ctrl+B", "Toggle sidebar"),
            ("?", "Open this help"),
            ("Page Up/Down", "Scroll messages"),
            ("g g", "Scroll to top"),
            ("G", "Scroll to bottom"),
        ]
    }
}

impl Widget for HelpModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let shortcuts = self.shortcuts();
        let mut content: Vec<Line> = vec![
            Line::from(vec![Span::styled(
                "Keyboard Shortcuts",
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            )]),
            Line::from(""),
        ];

        for (key, desc) in shortcuts.iter() {
            content.push(Line::from(vec![
                Span::styled(
                    format!("{:22}", key),
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(*desc, Style::default().fg(self.theme.text_secondary)),
            ]));
        }

        let modal = Modal::new("Help", self.theme)
            .content(content)
            .width(60)
            .height(65);

        modal.render(area, buf);
    }
}

/// Provider picker modal
pub struct ProviderPickerModal<'a> {
    theme: &'a Theme,
    providers: Vec<(&'a str, &'a str)>, // (name, icon)
    selected: usize,
}

impl<'a> ProviderPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            providers: vec![
                ("OpenAI", "🔑"),
                ("Claude", "🤖"),
                ("GitHub Copilot", "🐙"),
                ("Google Gemini", "💎"),
                ("OpenRouter", "🔀"),
                ("Ollama", "🦙"),
            ],
            selected: 0,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for ProviderPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("▸ ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Type to filter...",
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::DIM),
                ),
            ]),
            Line::from(""),
        ];

        for (i, (name, icon)) in self.providers.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let name_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let icon_style = if is_selected {
                Style::default().fg(self.theme.cyan)
            } else {
                Style::default().fg(self.theme.text_secondary)
            };

            content.push(Line::from(vec![
                Span::styled(prefix, name_style),
                Span::styled(format!("{} ", icon), icon_style),
                Span::styled(*name, name_style),
            ]));
        }

        let modal = Modal::new("Select Provider", self.theme)
            .content(content)
            .width(45)
            .height(55);

        modal.render(area, buf);
    }
}

/// Theme picker modal
pub struct ThemePickerModal<'a> {
    theme: &'a Theme,
    themes: Vec<ThemePreset>,
    selected: usize,
    current_theme: ThemePreset,
    filter: &'a str,
}

impl<'a> ThemePickerModal<'a> {
    pub fn new(theme: &'a Theme, current_theme: ThemePreset, filter: &'a str) -> Self {
        let all_themes = ThemePreset::all();

        // Apply filter
        let themes: Vec<ThemePreset> = if filter.is_empty() {
            all_themes
        } else {
            let filter_lower = filter.to_lowercase();
            all_themes
                .into_iter()
                .filter(|t| t.display_name().to_lowercase().contains(&filter_lower))
                .collect()
        };

        let selected = themes.iter().position(|&t| t == current_theme).unwrap_or(0);

        Self {
            theme,
            themes,
            selected,
            current_theme,
            filter,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn get_selected_theme(&self) -> Option<ThemePreset> {
        self.themes.get(self.selected).copied()
    }
}

impl Widget for ThemePickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content: Vec<Line> = vec![
            // Search filter input
            Line::from(vec![
                Span::styled("▸ ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    if self.filter.is_empty() {
                        "Type to filter themes...".to_string()
                    } else {
                        self.filter.to_string()
                    },
                    if self.filter.is_empty() {
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM)
                    } else {
                        Style::default().fg(self.theme.text_primary)
                    },
                ),
            ]),
            Line::from(""),
            // Instructions
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.cyan)),
                Span::styled(" Apply  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.red)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        // Show filtered themes
        for (i, &theme_preset) in self.themes.iter().enumerate() {
            let theme_name = theme_preset.display_name();
            let is_current = theme_preset == self.current_theme;
            let is_selected = i == self.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let mut spans = vec![];

            // Selection indicator
            let name_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            spans.push(Span::styled(prefix, name_style));

            // Add current indicator (saved theme)
            if is_current {
                spans.push(Span::styled("✓ ", Style::default().fg(self.theme.green)));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(theme_name, name_style));

            // Add "PREVIEWING" indicator for selected theme
            if is_selected && theme_preset != self.current_theme {
                spans.push(Span::raw("  "));
                spans.push(Span::styled(
                    "(previewing)",
                    Style::default()
                        .fg(self.theme.yellow)
                        .add_modifier(Modifier::DIM),
                ));
            }

            content.push(Line::from(spans));
        }

        // Show count if filtered
        if !self.filter.is_empty() {
            content.push(Line::from(""));
            content.push(Line::from(Span::styled(
                format!("Found {} theme(s)", self.themes.len()),
                Style::default().fg(self.theme.text_muted),
            )));
        }

        let modal = Modal::new("Theme Selector", self.theme)
            .content(content)
            .width(50)
            .height(60);

        modal.render(area, buf);
    }
}

/// Model picker modal
pub struct ModelPickerModal<'a> {
    theme: &'a Theme,
    models: Vec<(&'a str, &'a str, bool)>, // (name, capabilities, is_current)
    selected: usize,
}

impl<'a> ModelPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            models: vec![
                ("GPT-4o", "tools, vision", false),
                ("GPT-4 Turbo", "tools, vision", false),
                ("Claude 3 Opus", "tools, vision", false),
                ("Claude 3.5 Sonnet", "tools, vision", true),
                ("Gemini Pro", "tools", false),
                ("o1-preview", "reasoning", false),
            ],
            selected: 3, // Claude 3.5 Sonnet
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for ModelPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("▸ ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Type to filter...",
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::DIM),
                ),
            ]),
            Line::from(""),
        ];

        for (i, (name, caps, is_current)) in self.models.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let name_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let mut spans = vec![Span::styled(prefix, name_style)];

            // Add current indicator
            if *is_current {
                spans.push(Span::styled("● ", Style::default().fg(self.theme.yellow)));
            } else {
                spans.push(Span::raw("  "));
            }

            spans.push(Span::styled(*name, name_style));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                format!("- {}", caps),
                Style::default()
                    .fg(self.theme.text_muted)
                    .add_modifier(Modifier::DIM),
            ));

            content.push(Line::from(spans));
        }

        let modal = Modal::new("Select Model", self.theme)
            .content(content)
            .width(55)
            .height(55);

        modal.render(area, buf);
    }
}

/// File picker modal
pub struct FilePickerModal<'a> {
    theme: &'a Theme,
    files: Vec<&'a str>,
    selected: usize,
}

impl<'a> FilePickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            files: vec![
                "📁 src/",
                "📁 tests/",
                "📄 Cargo.toml",
                "📄 README.md",
                "📄 main.rs",
            ],
            selected: 0,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for FilePickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("▸ ", Style::default().fg(self.theme.cyan)),
                Span::styled(
                    "Type to filter...",
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::DIM),
                ),
            ]),
            Line::from(""),
        ];

        for (i, file_name) in self.files.iter().enumerate() {
            let is_selected = i == self.selected;
            let prefix = if is_selected { "▸ " } else { "  " };

            let style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            content.push(Line::from(Span::styled(
                format!("{}{}", prefix, file_name),
                style,
            )));
        }

        let modal = Modal::new("Select File", self.theme)
            .content(content)
            .width(55)
            .height(60);

        modal.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn test_help_modal_renders() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = Theme::default();

        terminal
            .draw(|f| {
                let modal = HelpModal::new(&theme);
                f.render_widget(modal, f.area());
            })
            .unwrap();

        let buffer = terminal.backend().buffer();
        // Check modal title is rendered
        let has_help = (0..80)
            .any(|x| (0..24).any(|y| buffer.cell((x, y)).map(|c| c.symbol()).unwrap_or("") == "H"));
        assert!(has_help, "Help modal should render");
    }
}
