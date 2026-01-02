//! Attachment preview bar widget
//!
//! Displays pending attachments above the input area with filename, type, and size.
//! Supports selection and removal of attachments.
//! Includes dropdown support for managing multiple attachments.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Widget},
};
use uuid::Uuid;

use crate::tui::attachments::{format_size, Attachment, AttachmentType};

/// State for the attachments dropdown
///
/// Manages the dropdown visibility, selection, and delete confirmation state.
#[derive(Debug, Clone, Default)]
pub struct AttachmentDropdownState {
    /// Whether the dropdown is open
    pub is_open: bool,
    /// Currently selected attachment index (for keyboard navigation)
    pub selected_index: Option<usize>,
    /// Whether we're showing a delete confirmation (index of attachment to delete)
    pub pending_delete: Option<usize>,
}

impl AttachmentDropdownState {
    /// Create a new dropdown state
    pub fn new() -> Self {
        Self::default()
    }

    /// Toggle dropdown visibility
    pub fn toggle(&mut self) {
        self.is_open = !self.is_open;
        if !self.is_open {
            self.selected_index = None;
            self.pending_delete = None;
        }
    }

    /// Open the dropdown
    pub fn open(&mut self) {
        self.is_open = true;
    }

    /// Close the dropdown
    pub fn close(&mut self) {
        self.is_open = false;
        self.selected_index = None;
        self.pending_delete = None;
    }

    /// Select next attachment (wraps around)
    pub fn select_next(&mut self, max: usize) {
        if max == 0 {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => (i + 1) % max,
            None => 0,
        });
    }

    /// Select previous attachment (wraps around)
    pub fn select_prev(&mut self, max: usize) {
        if max == 0 {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => {
                if i == 0 {
                    max - 1
                } else {
                    i - 1
                }
            }
            None => max - 1,
        });
    }

    /// Request deletion of selected attachment (shows confirmation)
    pub fn request_delete(&mut self) {
        if let Some(idx) = self.selected_index {
            self.pending_delete = Some(idx);
        }
    }

    /// Confirm pending deletion
    ///
    /// Returns the index of the attachment to delete, if any.
    pub fn confirm_delete(&mut self) -> Option<usize> {
        self.pending_delete.take()
    }

    /// Cancel pending deletion
    pub fn cancel_delete(&mut self) {
        self.pending_delete = None;
    }

    /// Check if there's a pending delete confirmation
    pub fn has_pending_delete(&self) -> bool {
        self.pending_delete.is_some()
    }

    /// Check if the dropdown is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Get the currently selected index
    pub fn selected(&self) -> Option<usize> {
        self.selected_index
    }
}

/// Render the attachment bar with dropdown support
///
/// This function renders a compact attachment display in the status bar area.
/// - For a single file: shows "🖇️ filename.ext ✕"
/// - For multiple files: shows "🖇️ N files ▼" with dropdown support
///
/// The dropdown appears above the status bar when opened.
pub fn render_attachment_bar(
    attachments: &[Attachment],
    state: &AttachmentDropdownState,
    area: Rect,
    buf: &mut Buffer,
) {
    if attachments.is_empty() {
        return;
    }

    // Compact display: "🖇️ N files ▼" or "🖇️ filename ✕"
    let display = if attachments.len() == 1 {
        format!("🖇️ {} ✕", attachments[0].filename)
    } else {
        let indicator = if state.is_open { "▲" } else { "▼" };
        format!("🖇️ {} files {}", attachments.len(), indicator)
    };

    // Render compact bar
    let span = Span::styled(display, Style::default().fg(Color::Cyan));
    buf.set_span(area.x, area.y, &span, area.width);

    // Render dropdown if open and multiple files
    if state.is_open && attachments.len() > 1 {
        render_dropdown(attachments, state, area, buf);
    }
}

/// Render the dropdown list of attachments
fn render_dropdown(
    attachments: &[Attachment],
    state: &AttachmentDropdownState,
    anchor: Rect,
    buf: &mut Buffer,
) {
    // Calculate dropdown dimensions
    let max_visible = 5;
    let dropdown_height = std::cmp::min(attachments.len() as u16, max_visible) + 2; // +2 for borders
    let dropdown_width = 40u16;

    // Position dropdown above the anchor area
    let dropdown_area = Rect {
        x: anchor.x,
        y: anchor.y.saturating_sub(dropdown_height),
        width: std::cmp::min(dropdown_width, anchor.width),
        height: dropdown_height,
    };

    // Clear the area first
    Clear.render(dropdown_area, buf);

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Attachments ");
    let inner = block.inner(dropdown_area);
    block.render(dropdown_area, buf);

    // Render each attachment
    for (i, attachment) in attachments.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.y + inner.height {
            break;
        }

        let is_selected = state.selected_index == Some(i);
        let is_pending_delete = state.pending_delete == Some(i);

        let style = if is_pending_delete {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else if is_selected {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let text = if is_pending_delete {
            format!(
                " {} [y/n]",
                truncate_filename(&attachment.filename, inner.width as usize - 8)
            )
        } else {
            format!(
                " {} {} ✕",
                attachment.file_type.icon(),
                truncate_filename(&attachment.filename, inner.width as usize - 6)
            )
        };

        buf.set_string(inner.x, y, &text, style);
    }
}

/// Truncate a filename to fit within a given width
fn truncate_filename(filename: &str, max_width: usize) -> String {
    if filename.len() <= max_width {
        filename.to_string()
    } else if max_width <= 3 {
        "...".to_string()
    } else {
        format!("{}...", &filename[..max_width - 3])
    }
}

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

    // AttachmentDropdownState tests

    #[test]
    fn test_dropdown_state_new() {
        let state = AttachmentDropdownState::new();
        assert!(!state.is_open());
        assert!(state.selected().is_none());
        assert!(!state.has_pending_delete());
    }

    #[test]
    fn test_dropdown_state_toggle() {
        let mut state = AttachmentDropdownState::new();

        state.toggle();
        assert!(state.is_open());

        state.toggle();
        assert!(!state.is_open());
    }

    #[test]
    fn test_dropdown_state_open_close() {
        let mut state = AttachmentDropdownState::new();

        state.open();
        assert!(state.is_open());

        state.close();
        assert!(!state.is_open());
    }

    #[test]
    fn test_dropdown_state_select_next() {
        let mut state = AttachmentDropdownState::new();

        // Empty list should not change selection
        state.select_next(0);
        assert!(state.selected().is_none());

        // First selection
        state.select_next(3);
        assert_eq!(state.selected(), Some(0));

        // Navigate forward
        state.select_next(3);
        assert_eq!(state.selected(), Some(1));

        state.select_next(3);
        assert_eq!(state.selected(), Some(2));

        // Wrap around
        state.select_next(3);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn test_dropdown_state_select_prev() {
        let mut state = AttachmentDropdownState::new();

        // Empty list should not change selection
        state.select_prev(0);
        assert!(state.selected().is_none());

        // First selection (from end)
        state.select_prev(3);
        assert_eq!(state.selected(), Some(2));

        // Navigate backward
        state.select_prev(3);
        assert_eq!(state.selected(), Some(1));

        state.select_prev(3);
        assert_eq!(state.selected(), Some(0));

        // Wrap around
        state.select_prev(3);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn test_dropdown_state_delete_flow() {
        let mut state = AttachmentDropdownState::new();

        // Select an item first
        state.select_next(3);
        assert_eq!(state.selected(), Some(0));

        // Request delete
        state.request_delete();
        assert!(state.has_pending_delete());
        assert_eq!(state.pending_delete, Some(0));

        // Cancel delete
        state.cancel_delete();
        assert!(!state.has_pending_delete());
        assert!(state.pending_delete.is_none());

        // Request delete again
        state.request_delete();
        assert!(state.has_pending_delete());

        // Confirm delete
        let deleted_idx = state.confirm_delete();
        assert_eq!(deleted_idx, Some(0));
        assert!(!state.has_pending_delete());
    }

    #[test]
    fn test_dropdown_state_request_delete_without_selection() {
        let mut state = AttachmentDropdownState::new();

        // Request delete without selection should do nothing
        state.request_delete();
        assert!(!state.has_pending_delete());
    }

    #[test]
    fn test_dropdown_state_close_clears_state() {
        let mut state = AttachmentDropdownState::new();

        state.open();
        state.select_next(3);
        state.request_delete();

        assert!(state.is_open());
        assert!(state.selected().is_some());
        assert!(state.has_pending_delete());

        state.close();

        assert!(!state.is_open());
        assert!(state.selected().is_none());
        assert!(!state.has_pending_delete());
    }

    #[test]
    fn test_dropdown_state_toggle_clears_state_when_closing() {
        let mut state = AttachmentDropdownState::new();

        state.toggle(); // Open
        state.select_next(3);
        state.request_delete();

        state.toggle(); // Close

        assert!(!state.is_open());
        assert!(state.selected().is_none());
        assert!(!state.has_pending_delete());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: enhanced-tui-layout, Property 14: Attachment Dropdown Navigation
    // **Validates: Requirements 11.3, 11.4**
    //
    // *For any* list of attachments, navigating with select_next/select_prev
    // should cycle through all items and wrap around at boundaries.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_dropdown_navigation_cycles_through_all_items(
            list_size in 1usize..20,
            nav_steps in 1usize..50
        ) {
            let mut state = AttachmentDropdownState::new();

            // Navigate forward nav_steps times
            for _ in 0..nav_steps {
                state.select_next(list_size);
            }

            // Selection should be within bounds
            let selected = state.selected().unwrap();
            prop_assert!(selected < list_size, "Selection {} should be < list_size {}", selected, list_size);

            // Selection should be nav_steps % list_size (accounting for initial None -> 0)
            let expected = (nav_steps - 1) % list_size;
            prop_assert_eq!(selected, expected, "After {} steps in list of {}, expected {} but got {}", nav_steps, list_size, expected, selected);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_dropdown_navigation_prev_cycles_through_all_items(
            list_size in 1usize..20,
            nav_steps in 1usize..50
        ) {
            let mut state = AttachmentDropdownState::new();

            // Navigate backward nav_steps times
            for _ in 0..nav_steps {
                state.select_prev(list_size);
            }

            // Selection should be within bounds
            let selected = state.selected().unwrap();
            prop_assert!(selected < list_size, "Selection {} should be < list_size {}", selected, list_size);

            // First select_prev goes to list_size - 1, then each subsequent call decrements by 1
            // After nav_steps calls: (list_size - nav_steps % list_size) % list_size
            // This handles the wrap-around correctly
            let expected = (list_size - (nav_steps % list_size)) % list_size;
            prop_assert_eq!(selected, expected, "After {} backward steps in list of {}, expected {} but got {}", nav_steps, list_size, expected, selected);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_dropdown_navigation_wraps_at_boundaries(list_size in 1usize..20) {
            let mut state = AttachmentDropdownState::new();

            // Navigate to the end
            for _ in 0..list_size {
                state.select_next(list_size);
            }

            // Should wrap to 0 after going through all items
            // First select_next sets to 0, then list_size-1 more steps
            // So after list_size steps, we're at (list_size - 1) % list_size = list_size - 1
            // Wait, let's trace: None -> 0 -> 1 -> ... -> list_size-1 -> 0
            // After list_size steps: we're at list_size - 1
            let selected = state.selected().unwrap();
            let expected = (list_size - 1) % list_size;
            prop_assert_eq!(selected, expected);

            // One more step should wrap to 0
            state.select_next(list_size);
            prop_assert_eq!(state.selected().unwrap(), 0);
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_dropdown_empty_list_navigation_safe(nav_steps in 0usize..50) {
            let mut state = AttachmentDropdownState::new();

            // Navigation on empty list should not panic and selection should remain None
            for _ in 0..nav_steps {
                state.select_next(0);
            }
            prop_assert!(state.selected().is_none());

            for _ in 0..nav_steps {
                state.select_prev(0);
            }
            prop_assert!(state.selected().is_none());
        }
    }

    // Feature: enhanced-tui-layout, Property 15: Attachment Delete Confirmation
    // **Validates: Requirements 11.6, 11.7, 11.8**
    //
    // *For any* attachment deletion request, the attachment should only be removed
    // after explicit confirmation (confirm_delete returns the index).
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_delete_requires_confirmation(
            list_size in 1usize..20,
            selected_idx in 0usize..20
        ) {
            // Ensure selected_idx is within bounds
            let selected_idx = selected_idx % list_size;

            let mut state = AttachmentDropdownState::new();

            // Navigate to the selected index
            for _ in 0..=selected_idx {
                state.select_next(list_size);
            }

            // Request delete
            state.request_delete();

            // Pending delete should be set
            prop_assert!(state.has_pending_delete());
            prop_assert_eq!(state.pending_delete, Some(selected_idx));

            // Without confirmation, pending_delete should remain
            prop_assert!(state.has_pending_delete());

            // Confirm delete should return the index and clear pending
            let deleted = state.confirm_delete();
            prop_assert_eq!(deleted, Some(selected_idx));
            prop_assert!(!state.has_pending_delete());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_delete_cancel_preserves_state(
            list_size in 1usize..20,
            selected_idx in 0usize..20
        ) {
            // Ensure selected_idx is within bounds
            let selected_idx = selected_idx % list_size;

            let mut state = AttachmentDropdownState::new();

            // Navigate to the selected index
            for _ in 0..=selected_idx {
                state.select_next(list_size);
            }

            // Request delete
            state.request_delete();
            prop_assert!(state.has_pending_delete());

            // Cancel delete
            state.cancel_delete();

            // Pending delete should be cleared
            prop_assert!(!state.has_pending_delete());

            // Selection should still be valid
            prop_assert_eq!(state.selected(), Some(selected_idx));
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_delete_without_selection_does_nothing(_list_size in 0usize..20) {
            let mut state = AttachmentDropdownState::new();

            // Don't select anything, just request delete
            state.request_delete();

            // Should have no pending delete
            prop_assert!(!state.has_pending_delete());

            // Confirm should return None
            let deleted = state.confirm_delete();
            prop_assert!(deleted.is_none());
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        #[test]
        fn prop_confirm_delete_only_returns_once(
            list_size in 1usize..20,
            selected_idx in 0usize..20
        ) {
            // Ensure selected_idx is within bounds
            let selected_idx = selected_idx % list_size;

            let mut state = AttachmentDropdownState::new();

            // Navigate to the selected index
            for _ in 0..=selected_idx {
                state.select_next(list_size);
            }

            // Request delete
            state.request_delete();

            // First confirm should return the index
            let first_confirm = state.confirm_delete();
            prop_assert_eq!(first_confirm, Some(selected_idx));

            // Second confirm should return None (already consumed)
            let second_confirm = state.confirm_delete();
            prop_assert!(second_confirm.is_none());
        }
    }
}
