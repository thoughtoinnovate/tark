//! Attachment preview bar widget
//!
//! Displays pending attachments above the input area with filename, type, and size.
//! Supports selection and removal of attachments.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Widget},
};
use uuid::Uuid;

use crate::tui::attachments::{format_size, Attachment, AttachmentType};

/// Preview information for an attachment
#[derive(Debug, Clone)]
pub struct AttachmentPreview {
    /// Unique identifier
    pub id: Uuid,
    /// Display filename
    pub filename: String,
    /// Type icon
    pub icon: String,
    /// Size string
    pub size: String,
    /// Whether this is an image
    pub is_image: bool,
}

impl AttachmentPreview {
    /// Create a preview from an attachment
    pub fn from_attachment(attachment: &Attachment) -> Self {
        Self {
            id: attachment.id,
            filename: attachment.filename.clone(),
            icon: attachment.file_type.icon().to_string(),
            size: format_size(attachment.size),
            is_image: matches!(attachment.file_type, AttachmentType::Image { .. }),
        }
    }

    /// Get a display string for this preview
    pub fn display(&self) -> String {
        format!("{} {} ({})", self.icon, self.filename, self.size)
    }
}

/// Attachment preview bar widget
#[derive(Debug, Default)]
pub struct AttachmentBar {
    /// Attachment previews
    attachments: Vec<AttachmentPreview>,
    /// Currently selected index (for removal)
    selected: Option<usize>,
    /// Whether the bar is focused
    focused: bool,
}

impl AttachmentBar {
    /// Create a new attachment bar
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the attachments to display
    pub fn set_attachments(&mut self, attachments: Vec<AttachmentPreview>) {
        self.attachments = attachments;
        // Reset selection if out of bounds
        if let Some(idx) = self.selected {
            if idx >= self.attachments.len() {
                self.selected = if self.attachments.is_empty() {
                    None
                } else {
                    Some(self.attachments.len() - 1)
                };
            }
        }
    }

    /// Update from a list of attachments
    pub fn update_from_attachments(&mut self, attachments: &[Attachment]) {
        self.attachments = attachments
            .iter()
            .map(AttachmentPreview::from_attachment)
            .collect();
        // Reset selection if out of bounds
        if let Some(idx) = self.selected {
            if idx >= self.attachments.len() {
                self.selected = if self.attachments.is_empty() {
                    None
                } else {
                    Some(self.attachments.len() - 1)
                };
            }
        }
    }

    /// Get the number of attachments
    pub fn count(&self) -> usize {
        self.attachments.len()
    }

    /// Check if there are any attachments
    pub fn is_empty(&self) -> bool {
        self.attachments.is_empty()
    }

    /// Get the currently selected attachment ID
    pub fn selected_id(&self) -> Option<Uuid> {
        self.selected
            .and_then(|idx| self.attachments.get(idx).map(|a| a.id))
    }

    /// Get the currently selected index
    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    /// Set focus state
    pub fn set_focused(&mut self, focused: bool) {
        self.focused = focused;
        // Select first item when focused if nothing selected
        if focused && self.selected.is_none() && !self.attachments.is_empty() {
            self.selected = Some(0);
        }
    }

    /// Check if focused
    pub fn is_focused(&self) -> bool {
        self.focused
    }

    /// Select the next attachment
    pub fn select_next(&mut self) {
        if self.attachments.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(idx) => (idx + 1) % self.attachments.len(),
            None => 0,
        });
    }

    /// Select the previous attachment
    pub fn select_previous(&mut self) {
        if self.attachments.is_empty() {
            return;
        }
        self.selected = Some(match self.selected {
            Some(idx) => {
                if idx == 0 {
                    self.attachments.len() - 1
                } else {
                    idx - 1
                }
            }
            None => self.attachments.len() - 1,
        });
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Get the height needed to display the bar
    pub fn height(&self) -> u16 {
        if self.attachments.is_empty() {
            0
        } else {
            3 // Border + content + border
        }
    }

    /// Render the attachment bar
    fn render_impl(&self, area: Rect, buf: &mut Buffer) {
        if self.attachments.is_empty() {
            return;
        }

        // Create block with title
        let title = format!(" Attachments ({}) ", self.attachments.len());
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(if self.focused {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            });

        // Render block
        let inner = block.inner(area);
        block.render(area, buf);

        // Build the content line
        let mut spans = Vec::new();
        for (idx, attachment) in self.attachments.iter().enumerate() {
            if idx > 0 {
                spans.push(Span::raw(" │ "));
            }

            let style = if Some(idx) == self.selected {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if attachment.is_image {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };

            spans.push(Span::styled(attachment.display(), style));
        }

        // Add hint for removal if focused
        if self.focused && self.selected.is_some() {
            spans.push(Span::styled(
                " [Ctrl-x to remove]",
                Style::default().fg(Color::DarkGray),
            ));
        }

        let line = Line::from(spans);

        // Render content (truncate if needed)
        if inner.width > 0 && inner.height > 0 {
            buf.set_line(inner.x, inner.y, &line, inner.width);
        }
    }
}

impl Widget for AttachmentBar {
    fn render(self, area: Rect, buf: &mut Buffer) {
        self.render_impl(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::attachments::{AttachmentContent, ImageFormat};

    fn create_test_attachment(name: &str, size: u64) -> Attachment {
        Attachment::new(
            name.to_string(),
            AttachmentType::Text { language: None },
            size,
            AttachmentContent::Text("test".to_string()),
        )
    }

    fn create_image_attachment(name: &str, size: u64) -> Attachment {
        Attachment::new(
            name.to_string(),
            AttachmentType::Image {
                format: ImageFormat::Png,
                width: 100,
                height: 100,
            },
            size,
            AttachmentContent::Base64("test".to_string()),
        )
    }

    #[test]
    fn test_attachment_bar_new() {
        let bar = AttachmentBar::new();
        assert!(bar.is_empty());
        assert_eq!(bar.count(), 0);
        assert!(!bar.is_focused());
    }

    #[test]
    fn test_attachment_bar_set_attachments() {
        let mut bar = AttachmentBar::new();
        let attachments = vec![
            create_test_attachment("file1.txt", 100),
            create_test_attachment("file2.txt", 200),
        ];

        bar.update_from_attachments(&attachments);
        assert_eq!(bar.count(), 2);
        assert!(!bar.is_empty());
    }

    #[test]
    fn test_attachment_bar_selection() {
        let mut bar = AttachmentBar::new();
        let attachments = vec![
            create_test_attachment("file1.txt", 100),
            create_test_attachment("file2.txt", 200),
            create_test_attachment("file3.txt", 300),
        ];

        bar.update_from_attachments(&attachments);
        bar.set_focused(true);

        // Should select first item when focused
        assert_eq!(bar.selected_index(), Some(0));

        // Navigate forward
        bar.select_next();
        assert_eq!(bar.selected_index(), Some(1));

        bar.select_next();
        assert_eq!(bar.selected_index(), Some(2));

        // Wrap around
        bar.select_next();
        assert_eq!(bar.selected_index(), Some(0));

        // Navigate backward
        bar.select_previous();
        assert_eq!(bar.selected_index(), Some(2));
    }

    #[test]
    fn test_attachment_bar_selected_id() {
        let mut bar = AttachmentBar::new();
        let attachments = vec![
            create_test_attachment("file1.txt", 100),
            create_test_attachment("file2.txt", 200),
        ];
        let expected_id = attachments[0].id;

        bar.update_from_attachments(&attachments);
        bar.set_focused(true);

        assert_eq!(bar.selected_id(), Some(expected_id));
    }

    #[test]
    fn test_attachment_bar_height() {
        let mut bar = AttachmentBar::new();
        assert_eq!(bar.height(), 0);

        let attachments = vec![create_test_attachment("file.txt", 100)];
        bar.update_from_attachments(&attachments);
        assert_eq!(bar.height(), 3);
    }

    #[test]
    fn test_attachment_bar_clear_selection() {
        let mut bar = AttachmentBar::new();
        let attachments = vec![create_test_attachment("file.txt", 100)];

        bar.update_from_attachments(&attachments);
        bar.set_focused(true);
        assert!(bar.selected_index().is_some());

        bar.clear_selection();
        assert!(bar.selected_index().is_none());
    }

    #[test]
    fn test_attachment_preview_from_attachment() {
        let attachment = create_test_attachment("test.txt", 1024);
        let preview = AttachmentPreview::from_attachment(&attachment);

        assert_eq!(preview.id, attachment.id);
        assert_eq!(preview.filename, "test.txt");
        assert_eq!(preview.size, "1.0KB");
        assert!(!preview.is_image);
    }

    #[test]
    fn test_attachment_preview_from_image() {
        let attachment = create_image_attachment("image.png", 2048);
        let preview = AttachmentPreview::from_attachment(&attachment);

        assert_eq!(preview.filename, "image.png");
        assert!(preview.is_image);
        assert!(preview.icon.contains("📷"));
    }

    #[test]
    fn test_attachment_preview_display() {
        let attachment = create_test_attachment("document.txt", 512);
        let preview = AttachmentPreview::from_attachment(&attachment);
        let display = preview.display();

        assert!(display.contains("document.txt"));
        assert!(display.contains("512B"));
    }

    #[test]
    fn test_attachment_bar_focus() {
        let mut bar = AttachmentBar::new();
        assert!(!bar.is_focused());

        bar.set_focused(true);
        assert!(bar.is_focused());

        bar.set_focused(false);
        assert!(!bar.is_focused());
    }

    #[test]
    fn test_attachment_bar_selection_bounds_on_update() {
        let mut bar = AttachmentBar::new();
        let attachments = vec![
            create_test_attachment("file1.txt", 100),
            create_test_attachment("file2.txt", 200),
            create_test_attachment("file3.txt", 300),
        ];

        bar.update_from_attachments(&attachments);
        bar.set_focused(true);
        bar.select_next();
        bar.select_next();
        assert_eq!(bar.selected_index(), Some(2));

        // Update with fewer attachments
        let fewer = vec![create_test_attachment("file1.txt", 100)];
        bar.update_from_attachments(&fewer);

        // Selection should be adjusted
        assert_eq!(bar.selected_index(), Some(0));
    }

    #[test]
    fn test_attachment_bar_empty_navigation() {
        let mut bar = AttachmentBar::new();

        // Navigation on empty bar should not panic
        bar.select_next();
        bar.select_previous();

        assert!(bar.selected_index().is_none());
    }
}
