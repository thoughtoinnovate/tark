//! Workspace grant modal: explicit permission interaction for extra roots (R1).
//!
//! Clearly identifies the grant target (original input + resolved absolute
//! path) and offers a session-scoped grant or denial. Follows the approval
//! modal visual language without reusing its multi-pattern list.

use crate::tui_new::theme::Theme;
use crate::ui_backend::WorkspaceGrantRequest;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// Workspace grant modal widget
pub struct WorkspaceGrantModal<'a> {
    theme: &'a Theme,
    request: &'a WorkspaceGrantRequest,
}

impl<'a> WorkspaceGrantModal<'a> {
    /// Create a new workspace grant modal
    pub fn new(theme: &'a Theme, request: &'a WorkspaceGrantRequest) -> Self {
        Self { theme, request }
    }
}

impl Widget for WorkspaceGrantModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let modal_width = area.width.min(65);
        let modal_height = 13u16.min(area.height.saturating_sub(2));
        let modal_area = Rect {
            x: area.width.saturating_sub(modal_width) / 2,
            y: area.height.saturating_sub(modal_height) / 2,
            width: modal_width,
            height: modal_height,
        };

        Clear.render(modal_area, buf);

        let title = Line::from(vec![
            Span::raw(" "),
            Span::styled(
                "⛨ Workspace Access Request ",
                Style::default()
                    .fg(self.theme.yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);
        let footer = Line::from(vec![
            Span::styled(" Enter ", Style::default().fg(self.theme.green)),
            Span::styled("grant  ", Style::default().fg(self.theme.text_muted)),
            Span::styled("Esc ", Style::default().fg(self.theme.red)),
            Span::styled("deny", Style::default().fg(self.theme.text_muted)),
        ]);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.yellow))
            .title(title)
            .title_alignment(Alignment::Center)
            .title_bottom(footer)
            .style(Style::default().bg(self.theme.bg_dark));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        let chunks = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Min(2),
            ])
            .split(inner);

        let intro = vec![
            Line::from(Span::styled(
                "  The agent requested access outside the granted workspace.",
                Style::default().fg(self.theme.text_primary),
            )),
            Line::from(Span::styled(
                "  Granting adds this root for the rest of the session.",
                Style::default().fg(self.theme.text_secondary),
            )),
        ];
        Paragraph::new(intro)
            .wrap(Wrap { trim: true })
            .render(chunks[0], buf);

        let target_block = Block::default()
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.cyan))
            .title(Span::styled(
                " Grant target ",
                Style::default().fg(self.theme.cyan),
            ));
        let target_inner = target_block.inner(chunks[1]);
        target_block.render(chunks[1], buf);
        let target_lines = vec![Line::from(vec![Span::styled(
            self.request.canonical_target.as_str(),
            Style::default()
                .fg(self.theme.cyan)
                .add_modifier(Modifier::BOLD),
        )])];
        Paragraph::new(target_lines)
            .wrap(Wrap { trim: false })
            .render(target_inner, buf);

        let detail = Line::from(vec![
            Span::styled("  Requested: ", Style::default().fg(self.theme.text_muted)),
            Span::styled(
                self.request.requested_path.as_str(),
                Style::default().fg(self.theme.text_secondary),
            ),
        ]);
        Paragraph::new(detail).render(chunks[2], buf);

        let actions = vec![Line::from(vec![
            Span::styled(
                " ▶ [G]rant for session ",
                Style::default()
                    .fg(self.theme.bg_dark)
                    .bg(self.theme.green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                "[D]eny",
                Style::default()
                    .fg(self.theme.red)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        Paragraph::new(actions).render(chunks[3], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui_new::theme::Theme;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    #[test]
    fn workspace_grant_modal_shows_canonical_target() {
        let theme = Theme::default();
        let request = WorkspaceGrantRequest {
            requested_path: "/tmp/grant-target".to_string(),
            canonical_target: "/tmp/grant-target".to_string(),
            reason: "Workspace access requested".to_string(),
        };
        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).expect("terminal");
        terminal
            .draw(|frame| {
                frame.render_widget(WorkspaceGrantModal::new(&theme, &request), frame.area());
            })
            .expect("draw");
        let content = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol().to_string())
            .collect::<String>();
        assert!(content.contains("/tmp/grant-target"));
        assert!(content.contains("Workspace Access Request"));
    }
}
