//! Trust level selector widget.
//!
//! Allows the user to switch between trust levels (Balanced, Careful, Manual)
//! using Shift+A keybinding. Displays a popup with level options.
//! Only shown in Build mode where trust levels have effect.

#![allow(dead_code)]

use crate::tools::TrustLevel;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph},
    Frame,
};

/// State for the trust level selector popup
#[derive(Debug, Clone, Default)]
pub struct TrustLevelSelector {
    /// Currently active trust level
    pub current_level: TrustLevel,
    /// Selected index in the popup (0-2)
    selected_index: usize,
    /// Whether the popup is visible
    pub visible: bool,
}

impl TrustLevelSelector {
    /// Create a new selector with default level
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new selector with specified level
    pub fn with_level(level: TrustLevel) -> Self {
        Self {
            current_level: level,
            selected_index: level.index(),
            visible: false,
        }
    }

    /// Open the selector popup
    pub fn open(&mut self) {
        self.visible = true;
        self.selected_index = self.current_level.index();
    }

    /// Close the selector popup
    pub fn close(&mut self) {
        self.visible = false;
    }

    /// Toggle popup visibility
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// Handle key input
    /// Returns Some(level) if a level was selected, None otherwise
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<TrustLevel> {
        if !self.visible {
            return None;
        }

        match key.code {
            // Escape closes without selecting
            KeyCode::Esc => {
                self.close();
                None
            }
            // Enter confirms selection
            KeyCode::Enter => {
                let level = TrustLevel::from_index(self.selected_index);
                self.current_level = level;
                self.close();
                Some(level)
            }
            // Number keys for direct selection
            KeyCode::Char('1') => {
                self.current_level = TrustLevel::Balanced;
                self.close();
                Some(TrustLevel::Balanced)
            }
            KeyCode::Char('2') => {
                self.current_level = TrustLevel::Careful;
                self.close();
                Some(TrustLevel::Careful)
            }
            KeyCode::Char('3') => {
                self.current_level = TrustLevel::Manual;
                self.close();
                Some(TrustLevel::Manual)
            }
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => {
                self.selected_index = self.selected_index.saturating_sub(1);
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected_index = (self.selected_index + 1).min(2);
                None
            }
            _ => None,
        }
    }

    /// Render the level indicator (for status bar)
    pub fn render_indicator(&self) -> Span<'static> {
        let level = self.current_level;
        Span::styled(
            format!("[{} {}]", level.icon(), level.label()),
            Style::default().fg(match level {
                TrustLevel::Balanced => Color::Yellow,
                TrustLevel::Careful => Color::Blue,
                TrustLevel::Manual => Color::Red,
            }),
        )
    }

    /// Render the popup if visible
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate popup size and position
        let popup_width = 55;
        let popup_height = 11;
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Build the list items
        let levels = TrustLevel::all();
        let items: Vec<ListItem> = levels
            .iter()
            .enumerate()
            .map(|(i, level)| {
                let selected_marker = if *level == self.current_level {
                    "●"
                } else {
                    "○"
                };

                let style = if i == self.selected_index {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!(" [{}] ", i + 1),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{} ", level.icon()),
                        Style::default().fg(match level {
                            TrustLevel::Balanced => Color::Yellow,
                            TrustLevel::Careful => Color::Blue,
                            TrustLevel::Manual => Color::Red,
                        }),
                    ),
                    Span::styled(format!("{} ", selected_marker), style),
                    Span::styled(format!("{:12}", level.label()), style),
                    Span::styled(
                        format!(" - {}", level.description()),
                        Style::default().fg(Color::Gray),
                    ),
                ]);

                ListItem::new(line).style(style)
            })
            .collect();

        let list = List::new(items);

        // Create the block with title and border
        let block = Block::default()
            .title(" Trust Level (Shift+A) ")
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        // Layout for header, list, and footer
        let inner_area = block.inner(popup_area);
        let chunks = Layout::vertical([
            Constraint::Length(1), // Header spacing
            Constraint::Length(3), // List
            Constraint::Length(1), // Spacing
            Constraint::Length(2), // Help text
        ])
        .split(inner_area);

        // Render the block
        frame.render_widget(block, popup_area);

        // Render the list
        frame.render_widget(list, chunks[1]);

        // Render help text
        let help = Paragraph::new(Line::from(vec![
            Span::styled("  1-3", Style::default().fg(Color::Yellow)),
            Span::raw(" select  "),
            Span::styled("↑/↓/j/k", Style::default().fg(Color::Yellow)),
            Span::raw(" navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(" confirm  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(" cancel"),
        ]))
        .alignment(Alignment::Center);

        frame.render_widget(help, chunks[3]);
    }

    /// Get the current level
    pub fn level(&self) -> TrustLevel {
        self.current_level
    }

    /// Set the current level
    pub fn set_level(&mut self, level: TrustLevel) {
        self.current_level = level;
        self.selected_index = level.index();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let selector = TrustLevelSelector::new();
        assert_eq!(selector.current_level, TrustLevel::Balanced);
        assert!(!selector.visible);
    }

    #[test]
    fn test_toggle() {
        let mut selector = TrustLevelSelector::new();
        assert!(!selector.visible);
        selector.toggle();
        assert!(selector.visible);
        selector.toggle();
        assert!(!selector.visible);
    }

    #[test]
    fn test_key_selection() {
        let mut selector = TrustLevelSelector::new();
        selector.open();

        // Test number key selection
        let result = selector.handle_key(KeyEvent::from(KeyCode::Char('2')));
        assert_eq!(result, Some(TrustLevel::Careful));
        assert_eq!(selector.current_level, TrustLevel::Careful);
        assert!(!selector.visible);
    }

    #[test]
    fn test_navigation() {
        let mut selector = TrustLevelSelector::new();
        selector.open();

        // Navigate down
        selector.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(selector.selected_index, 1);

        selector.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(selector.selected_index, 2);

        // Can't go past end
        selector.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(selector.selected_index, 2);

        // Navigate up
        selector.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(selector.selected_index, 1);
    }

    #[test]
    fn test_escape_closes() {
        let mut selector = TrustLevelSelector::new();
        selector.open();
        assert!(selector.visible);

        let result = selector.handle_key(KeyEvent::from(KeyCode::Esc));
        assert!(result.is_none());
        assert!(!selector.visible);
    }
}
