//! Modal Widget
//!
//! Base modal component for dialogs and pickers
//! Features: 05-09 (modals)

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};

use crate::tui_new::theme::Theme;

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

        // Render modal border
        let block = Block::default()
            .title(format!(" {} ", self.title))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.border_focused))
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
        let content: Vec<Line> = shortcuts
            .iter()
            .map(|(key, desc)| {
                Line::from(vec![
                    Span::styled(
                        format!("{:20}", key),
                        Style::default()
                            .fg(self.theme.cyan)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(*desc, Style::default().fg(self.theme.text_primary)),
                ])
            })
            .collect();

        let modal = Modal::new("Help - Keyboard Shortcuts", self.theme)
            .content(content)
            .width(50)
            .height(60);

        modal.render(area, buf);
    }
}

/// Provider picker modal
pub struct ProviderPickerModal<'a> {
    theme: &'a Theme,
    providers: Vec<&'a str>,
    selected: usize,
}

impl<'a> ProviderPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            providers: vec![
                "OpenAI",
                "Anthropic",
                "Google",
                "Copilot",
                "OpenRouter",
                "Ollama",
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
        let content: Vec<Line> = self
            .providers
            .iter()
            .enumerate()
            .map(|(i, provider)| {
                let prefix = if i == self.selected { "▶ " } else { "  " };
                let style = if i == self.selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };
                Line::from(Span::styled(format!("{}{}", prefix, provider), style))
            })
            .collect();

        let modal = Modal::new("Select Provider", self.theme)
            .content(content)
            .width(40)
            .height(50);

        modal.render(area, buf);
    }
}

/// Theme picker modal
pub struct ThemePickerModal<'a> {
    theme: &'a Theme,
    themes: Vec<&'a str>,
    selected: usize,
}

impl<'a> ThemePickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            themes: vec![
                "Catppuccin Mocha",
                "Nord",
                "Dracula",
                "GitHub Dark",
                "One Dark",
                "Gruvbox Dark",
                "Tokyo Night",
            ],
            selected: 0,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for ThemePickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let content: Vec<Line> = self
            .themes
            .iter()
            .enumerate()
            .map(|(i, theme_name)| {
                let prefix = if i == self.selected { "▶ " } else { "  " };
                let style = if i == self.selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };
                Line::from(Span::styled(format!("{}{}", prefix, theme_name), style))
            })
            .collect();

        let modal = Modal::new("Select Theme", self.theme)
            .content(content)
            .width(40)
            .height(50);

        modal.render(area, buf);
    }
}

/// Model picker modal
pub struct ModelPickerModal<'a> {
    theme: &'a Theme,
    models: Vec<&'a str>,
    selected: usize,
}

impl<'a> ModelPickerModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            models: vec![
                "gpt-4o",
                "gpt-4o-mini",
                "gpt-4-turbo",
                "claude-3-5-sonnet",
                "claude-3-opus",
                "gemini-pro",
            ],
            selected: 0,
        }
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = index;
        self
    }
}

impl Widget for ModelPickerModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let content: Vec<Line> = self
            .models
            .iter()
            .enumerate()
            .map(|(i, model_name)| {
                let prefix = if i == self.selected { "▶ " } else { "  " };
                let style = if i == self.selected {
                    Style::default()
                        .fg(self.theme.cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(self.theme.text_primary)
                };
                Line::from(Span::styled(format!("{}{}", prefix, model_name), style))
            })
            .collect();

        let modal = Modal::new("Select Model", self.theme)
            .content(content)
            .width(40)
            .height(50);

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
            Line::from(Span::styled(
                "Search: ",
                Style::default().fg(self.theme.text_muted),
            )),
            Line::from(""),
        ];

        for (i, file_name) in self.files.iter().enumerate() {
            let prefix = if i == self.selected { "▶ " } else { "  " };
            let style = if i == self.selected {
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
            .width(50)
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
