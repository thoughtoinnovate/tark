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
            ("Shift+Enter", "Insert newline"),
            ("Escape", "Clear input / Close modal / Normal mode"),
            ("Tab", "Next focus / Autocomplete"),
            ("Shift+Tab", "Cycle agent mode"),
            ("Ctrl+T", "Toggle thinking mode"),
            ("Ctrl+B", "Toggle sidebar"),
            ("Ctrl+Shift+M", "Cycle build mode"),
            ("Ctrl+Shift+B", "Trust level selector (Build mode)"),
            ("Ctrl+?", "Open this help"),
            ("@filename", "Add file to context"),
            ("/command", "Slash commands (try /help)"),
            ("j/k", "Navigate messages (Normal mode)"),
            ("g / G", "Scroll to top / bottom"),
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
    providers: Vec<(String, String, String, bool, bool)>, // (name, icon, description, configured, is_plugin)
    selected: usize,
    filter: String,
}

impl<'a> ProviderPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            providers: vec![],
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn providers(mut self, providers: Vec<(String, String, String, bool, bool)>) -> Self {
        self.providers = providers;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn filter(mut self, filter: String) -> Self {
        self.filter = filter;
        self
    }
}

impl Widget for ProviderPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate viewport dimensions
        let popup_height = area.height * 70 / 100;
        let inner_height = popup_height.saturating_sub(2);
        let header_lines = 4usize;
        let footer_lines = if self.filter.is_empty() { 0 } else { 2 };
        let available_rows = inner_height
            .saturating_sub((header_lines + footer_lines) as u16)
            .max(1) as usize;

        let mut content: Vec<Line> = vec![
            // Search bar header
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    if self.filter.is_empty() {
                        "▏".to_string()
                    } else {
                        format!("{}▏", self.filter)
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
            // Navigation hints
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.green)),
                Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.red)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        // Apply filter to providers
        let filtered_providers: Vec<_> = if self.filter.is_empty() {
            self.providers.clone()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.providers
                .iter()
                .filter(|(name, _, _, _, _)| name.to_lowercase().contains(&filter_lower))
                .cloned()
                .collect()
        };

        if filtered_providers.is_empty() {
            content.push(Line::from(vec![Span::styled(
                "  No providers match your search",
                Style::default().fg(self.theme.text_muted),
            )]));
        } else {
            // Calculate viewport window for scrolling
            let total = filtered_providers.len();
            let selected = self.selected.min(total.saturating_sub(1));

            // Estimate lines per item (1 for name, potentially 2 more for description)
            let max_lines_per_item = 3;
            let items_visible = available_rows / max_lines_per_item;

            let window_start = selected.saturating_sub(items_visible / 2);
            let window_end = (window_start + items_visible).min(total);

            for (i, (name, icon, description, configured, is_plugin)) in filtered_providers
                .iter()
                .enumerate()
                .skip(window_start)
                .take(window_end.saturating_sub(window_start))
            {
                let is_selected = i == selected;
                let prefix = if is_selected { "▸ " } else { "  " };

                let name_style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let icon_style = if is_selected {
                    Style::default().fg(self.theme.cyan)
                } else {
                    Style::default().fg(self.theme.text_secondary)
                };

                let status_icon = if *configured { "✓" } else { "⚠" };
                let status_color = if *configured {
                    self.theme.green
                } else {
                    self.theme.yellow
                };

                // Build the provider line with optional plugin indicator
                let mut spans = vec![
                    Span::styled(prefix, name_style),
                    Span::styled(format!("{} ", icon), icon_style),
                    Span::styled(name.clone(), name_style),
                ];

                // Add plugin indicator in muted/ghost color
                if *is_plugin {
                    spans.push(Span::styled(
                        " [plugin]",
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM),
                    ));
                }

                spans.push(Span::raw(" "));
                spans.push(Span::styled(status_icon, Style::default().fg(status_color)));

                content.push(Line::from(spans));

                // Add description line if selected
                if is_selected && !description.is_empty() {
                    content.push(Line::from(vec![Span::styled(
                        format!("    {}", description),
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM),
                    )]));
                    if !*configured {
                        content.push(Line::from(vec![Span::styled(
                            "    Not configured - run 'tark auth <provider>'",
                            Style::default().fg(self.theme.yellow),
                        )]));
                    }
                }
            }

            // Show scroll indicator if needed
            if total > items_visible {
                content.push(Line::from(""));
                content.push(Line::from(vec![Span::styled(
                    format!(
                        "  Showing {} of {} providers",
                        window_end.saturating_sub(window_start),
                        total
                    ),
                    Style::default().fg(self.theme.text_muted),
                )]));
            }
        }

        let modal = Modal::new("Select LLM Provider", self.theme)
            .content(content)
            .width(60)
            .height(70);

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
    models: Vec<(String, String, bool)>, // (name, description, is_current)
    selected: usize,
    filter: String,
}

impl<'a> ModelPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            models: vec![],
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn models(mut self, models: Vec<(String, String, bool)>) -> Self {
        self.models = models;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn filter(mut self, filter: String) -> Self {
        self.filter = filter;
        self
    }
}

impl Widget for ModelPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Calculate viewport dimensions
        let popup_height = area.height * 70 / 100;
        let inner_height = popup_height.saturating_sub(2);
        let header_lines = 4usize;
        let footer_lines = if self.filter.is_empty() { 0 } else { 2 };
        let available_rows = inner_height
            .saturating_sub((header_lines + footer_lines) as u16)
            .max(1) as usize;

        let mut content: Vec<Line> = vec![
            // Search bar header
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    if self.filter.is_empty() {
                        "▏".to_string()
                    } else {
                        format!("{}▏", self.filter)
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
            // Navigation hints
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.green)),
                Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.red)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        // Apply filter to models
        let filtered_models: Vec<_> = if self.filter.is_empty() {
            self.models.clone()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.models
                .iter()
                .filter(|(name, _, _)| name.to_lowercase().contains(&filter_lower))
                .cloned()
                .collect()
        };

        if filtered_models.is_empty() {
            content.push(Line::from(Span::styled(
                "  No models match your search",
                Style::default().fg(self.theme.text_muted),
            )));
        } else {
            // Calculate viewport window for scrolling
            let total = filtered_models.len();
            let selected = self.selected.min(total.saturating_sub(1));

            // Estimate lines per item (1 for name, potentially 1 more for description)
            let max_lines_per_item = 2;
            let items_visible = available_rows / max_lines_per_item;

            let window_start = selected.saturating_sub(items_visible / 2);
            let window_end = (window_start + items_visible).min(total);

            for (i, (name, description, is_current)) in filtered_models
                .iter()
                .enumerate()
                .skip(window_start)
                .take(window_end.saturating_sub(window_start))
            {
                let is_selected = i == selected;
                let prefix = if is_selected { "▸ " } else { "  " };

                let name_style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                let mut spans = vec![Span::styled(prefix, name_style)];

                // Add current indicator
                if *is_current {
                    spans.push(Span::styled("● ", Style::default().fg(self.theme.green)));
                } else {
                    spans.push(Span::raw("  "));
                }

                spans.push(Span::styled(name.clone(), name_style));

                content.push(Line::from(spans));

                // Add description on next line if selected
                if is_selected && !description.is_empty() {
                    content.push(Line::from(vec![Span::styled(
                        format!("    {}", description),
                        Style::default()
                            .fg(self.theme.text_muted)
                            .add_modifier(Modifier::DIM),
                    )]));
                }
            }

            // Show scroll indicator if needed
            if total > items_visible {
                content.push(Line::from(""));
                content.push(Line::from(vec![Span::styled(
                    format!(
                        "  Showing {} of {} models",
                        window_end.saturating_sub(window_start),
                        total
                    ),
                    Style::default().fg(self.theme.text_muted),
                )]));
            }
        }

        let modal = Modal::new("Select Model", self.theme)
            .content(content)
            .width(65)
            .height(70);

        modal.render(area, buf);
    }
}

/// Session picker modal
pub struct SessionPickerModal<'a> {
    theme: &'a Theme,
    sessions: Vec<(String, String, String, bool)>, // (name, id, meta, is_current)
    selected: usize,
    filter: String,
}

impl<'a> SessionPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            sessions: vec![],
            selected: 0,
            filter: String::new(),
        }
    }

    pub fn sessions(mut self, sessions: Vec<(String, String, String, bool)>) -> Self {
        self.sessions = sessions;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    pub fn filter(mut self, filter: String) -> Self {
        self.filter = filter;
        self
    }
}

impl Widget for SessionPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let popup_height = area.height * 70 / 100;
        let inner_height = popup_height.saturating_sub(2);
        let header_lines = 4usize;
        let footer_lines = if self.filter.is_empty() { 0 } else { 2 };
        let available_rows = inner_height
            .saturating_sub((header_lines + footer_lines) as u16)
            .max(1) as usize;

        let mut content: Vec<Line> = vec![
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    if self.filter.is_empty() {
                        "▏".to_string()
                    } else {
                        format!("{}▏", self.filter)
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
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.green)),
                Span::styled(" Switch  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Del", Style::default().fg(self.theme.red)),
                Span::styled(" Delete  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.yellow)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        let filtered_sessions: Vec<_> = if self.filter.is_empty() {
            self.sessions.clone()
        } else {
            let filter_lower = self.filter.to_lowercase();
            self.sessions
                .iter()
                .filter(|(name, id, _, _)| {
                    name.to_lowercase().contains(&filter_lower)
                        || id.to_lowercase().contains(&filter_lower)
                })
                .cloned()
                .collect()
        };

        if filtered_sessions.is_empty() {
            content.push(Line::from(Span::styled(
                "  No sessions match your search",
                Style::default().fg(self.theme.text_muted),
            )));
        }

        let total = filtered_sessions.len();
        let selected = self.selected.min(total.saturating_sub(1));
        let window_start = selected.saturating_sub(available_rows / 2);
        let window_end = (window_start + available_rows).min(total);

        for (i, (name, id, meta, is_current)) in filtered_sessions
            .iter()
            .enumerate()
            .skip(window_start)
            .take(window_end.saturating_sub(window_start))
        {
            let is_selected = i == selected;
            let prefix = if is_selected { "▸ " } else { "  " };
            let row_style = if is_selected {
                Style::default()
                    .fg(self.theme.cyan)
                    .add_modifier(Modifier::BOLD)
                    .bg(self.theme.selection_bg)
            } else {
                Style::default().fg(self.theme.text_primary)
            };

            let mut spans = vec![Span::styled(prefix, row_style)];
            if *is_current {
                spans.push(Span::styled("● ", Style::default().fg(self.theme.green)));
            } else {
                spans.push(Span::raw("  "));
            }

            let display_name = if name.is_empty() {
                id.clone()
            } else {
                name.clone()
            };
            spans.push(Span::styled(display_name, row_style));
            content.push(Line::from(spans));

            if is_selected {
                let detail = if meta.is_empty() {
                    id.clone()
                } else {
                    format!("{} · {}", id, meta)
                };
                content.push(Line::from(vec![Span::styled(
                    format!("    {}", detail),
                    Style::default()
                        .fg(self.theme.text_muted)
                        .add_modifier(Modifier::DIM),
                )]));
            }
        }

        if !self.filter.is_empty() {
            content.push(Line::from(""));
            content.push(Line::from(vec![Span::styled(
                format!(
                    "  Showing {} of {} sessions",
                    filtered_sessions.len(),
                    self.sessions.len()
                ),
                Style::default().fg(self.theme.text_muted),
            )]));
        }

        let modal = Modal::new("Sessions", self.theme)
            .content(content)
            .width(70)
            .height(70);
        modal.render(area, buf);
    }
}

/// File picker modal
pub struct FilePickerModal<'a> {
    theme: &'a Theme,
    files: &'a [String],
    filter: &'a str,
    selected: usize,
}

impl<'a> FilePickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            files: &[],
            filter: "",
            selected: 0,
        }
    }

    pub fn files(mut self, files: &'a [String]) -> Self {
        self.files = files;
        self
    }

    pub fn filter(mut self, filter: &'a str) -> Self {
        self.filter = filter;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }

    fn get_file_icon(&self, path: &str) -> &'static str {
        if path.ends_with('/') {
            "📁"
        } else if path.ends_with(".rs") {
            "🦀"
        } else if path.ends_with(".toml") {
            "📦"
        } else if path.ends_with(".md") {
            "📄"
        } else if path.ends_with(".lua") {
            "🌙"
        } else if path.ends_with(".ts") || path.ends_with(".tsx") {
            "📜"
        } else if path.ends_with(".json") {
            "📋"
        } else {
            "📄"
        }
    }
}

impl Widget for FilePickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut content: Vec<Line> = vec![
            // Search/filter header
            Line::from(vec![
                Span::styled("Search: ", Style::default().fg(self.theme.text_muted)),
                Span::styled(
                    if self.filter.is_empty() {
                        "▏".to_string()
                    } else {
                        format!("{}▏", self.filter)
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
            // Navigation hints
            Line::from(vec![
                Span::styled("↑↓", Style::default().fg(self.theme.cyan)),
                Span::styled(" Navigate  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Enter", Style::default().fg(self.theme.green)),
                Span::styled(" Select  ", Style::default().fg(self.theme.text_muted)),
                Span::styled("Esc", Style::default().fg(self.theme.yellow)),
                Span::styled(" Cancel", Style::default().fg(self.theme.text_muted)),
            ]),
            Line::from(""),
        ];

        if self.files.is_empty() {
            content.push(Line::from(Span::styled(
                "  No files found",
                Style::default().fg(self.theme.text_muted),
            )));
        } else {
            for (i, file_path) in self.files.iter().enumerate().take(15) {
                let is_selected = i == self.selected;
                let prefix = if is_selected { "▸ " } else { "  " };
                let icon = self.get_file_icon(file_path);

                let style = if is_selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                        .bg(self.theme.selection_bg)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };

                content.push(Line::from(Span::styled(
                    format!("{}{} {}", prefix, icon, file_path),
                    style,
                )));
            }

            if self.files.len() > 15 {
                content.push(Line::from(Span::styled(
                    format!("  ... {} more files", self.files.len() - 15),
                    Style::default().fg(self.theme.text_muted),
                )));
            }
        }

        let title = if self.files.is_empty() {
            "Add Context File (No matches)"
        } else {
            "Add Context File"
        };

        let modal = Modal::new(title, self.theme)
            .content(content)
            .width(60)
            .height(65);

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
