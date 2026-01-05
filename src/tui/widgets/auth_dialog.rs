//! Authentication dialog widget for OAuth device flow
//!
//! Displays a centered modal dialog for GitHub Copilot authentication
//! with device code, URL, and status updates.

#![allow(dead_code)]

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget},
};
use std::time::Instant;

/// Authentication status
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthStatus {
    /// Waiting for user to authorize
    WaitingForUser,
    /// Polling for token
    Polling,
    /// Successfully authenticated
    Success,
    /// Authentication failed
    Failed(String),
    /// Authentication timed out
    TimedOut,
}

/// Authentication dialog state
#[derive(Debug, Clone)]
pub struct AuthDialog {
    /// Whether the dialog is visible
    visible: bool,
    /// Dialog title
    title: String,
    /// Provider name (e.g., "GitHub Copilot")
    provider: String,
    /// Verification URL
    verification_url: String,
    /// User code to enter
    user_code: String,
    /// Current status
    status: AuthStatus,
    /// Timeout in seconds
    timeout_secs: u64,
    /// When authentication started
    started_at: Option<Instant>,
}

impl Default for AuthDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthDialog {
    /// Create a new authentication dialog
    pub fn new() -> Self {
        Self {
            visible: false,
            title: "Authentication Required".to_string(),
            provider: String::new(),
            verification_url: String::new(),
            user_code: String::new(),
            status: AuthStatus::WaitingForUser,
            timeout_secs: 300, // 5 minutes default
            started_at: None,
        }
    }

    /// Show Copilot authentication dialog
    pub fn show_copilot_auth(&mut self, url: &str, code: &str, timeout: u64) {
        self.visible = true;
        self.provider = "GitHub Copilot".to_string();
        self.verification_url = url.to_string();
        self.user_code = code.to_string();
        self.status = AuthStatus::WaitingForUser;
        self.timeout_secs = timeout;
        self.started_at = Some(Instant::now());
    }

    /// Set the authentication status
    pub fn set_status(&mut self, status: AuthStatus) {
        self.status = status;
    }

    /// Hide the dialog
    pub fn hide(&mut self) {
        self.visible = false;
        self.started_at = None;
    }

    /// Check if the dialog is visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Get the user code (for clipboard copy)
    pub fn user_code(&self) -> &str {
        &self.user_code
    }

    /// Get elapsed time percentage (0.0 to 1.0)
    fn elapsed_percent(&self) -> f32 {
        if let Some(started) = self.started_at {
            let elapsed = started.elapsed().as_secs();
            (elapsed as f32 / self.timeout_secs as f32).min(1.0)
        } else {
            0.0
        }
    }

    /// Get remaining time in seconds
    fn remaining_secs(&self) -> u64 {
        if let Some(started) = self.started_at {
            let elapsed = started.elapsed().as_secs();
            self.timeout_secs.saturating_sub(elapsed)
        } else {
            self.timeout_secs
        }
    }
}

/// Renderable authentication dialog widget
pub struct AuthDialogWidget<'a> {
    dialog: &'a AuthDialog,
}

impl<'a> AuthDialogWidget<'a> {
    /// Create a new auth dialog widget
    pub fn new(dialog: &'a AuthDialog) -> Self {
        Self { dialog }
    }

    /// Calculate the area for the dialog (centered modal)
    fn calculate_area(&self, area: Rect) -> Rect {
        let width = 60.min(area.width.saturating_sub(4));
        let height = 18.min(area.height.saturating_sub(4));

        let x = (area.width.saturating_sub(width)) / 2;
        let y = (area.height.saturating_sub(height)) / 2;

        Rect::new(x, y, width, height)
    }

    /// Render progress bar
    fn render_progress_bar(&self, width: usize) -> String {
        let percent = self.dialog.elapsed_percent();
        let filled = (width as f32 * percent) as usize;
        let empty = width.saturating_sub(filled);

        format!("{}{}", "█".repeat(filled), "░".repeat(empty))
    }
}

impl Widget for AuthDialogWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.dialog.visible {
            return;
        }

        let dialog_area = self.calculate_area(area);

        // Clear the background
        Clear.render(dialog_area, buf);

        // Draw border with title
        let title = format!(" {} ", self.dialog.title);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            );

        let inner = block.inner(dialog_area);
        block.render(dialog_area, buf);

        if inner.height < 10 {
            return;
        }

        // Build content lines based on status
        let mut lines: Vec<Line<'static>> = Vec::new();

        match &self.dialog.status {
            AuthStatus::WaitingForUser | AuthStatus::Polling => {
                // Empty line for spacing
                lines.push(Line::from(""));

                // Instructions header
                lines.push(Line::from(vec![Span::styled(
                    "📋 Please follow these steps:",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));

                // Step 1: Visit URL
                lines.push(Line::from(vec![
                    Span::styled("  1. ", Style::default().fg(Color::White)),
                    Span::styled("Visit: ", Style::default().fg(Color::White)),
                    Span::styled(
                        self.dialog.verification_url.clone(),
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                ]));
                lines.push(Line::from(""));

                // Step 2: Enter code
                lines.push(Line::from(vec![
                    Span::styled("  2. ", Style::default().fg(Color::White)),
                    Span::styled("Enter code: ", Style::default().fg(Color::White)),
                    Span::styled(
                        self.dialog.user_code.clone(),
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]));
                lines.push(Line::from(vec![Span::styled(
                    "      [Press 'c' to copy code]",
                    Style::default().fg(Color::DarkGray),
                )]));
                lines.push(Line::from(""));

                // Step 3: Authorize
                lines.push(Line::from(vec![
                    Span::styled("  3. ", Style::default().fg(Color::White)),
                    Span::styled(
                        format!("Authorize Tark to use {}", self.dialog.provider),
                        Style::default().fg(Color::White),
                    ),
                ]));
                lines.push(Line::from(""));

                // Status and progress
                let remaining = self.dialog.remaining_secs();
                let minutes = remaining / 60;
                let seconds = remaining % 60;
                let status_text = if self.dialog.status == AuthStatus::Polling {
                    format!("⏳ Polling... ({}:{:02} remaining)", minutes, seconds)
                } else {
                    format!("⏳ Waiting... ({}:{:02} remaining)", minutes, seconds)
                };

                lines.push(Line::from(vec![Span::styled(
                    status_text,
                    Style::default().fg(Color::Yellow),
                )]));

                // Progress bar
                let bar_width = (inner.width as usize).saturating_sub(4);
                let progress_bar = self.render_progress_bar(bar_width);
                let percent = (self.dialog.elapsed_percent() * 100.0) as u32;
                lines.push(Line::from(vec![
                    Span::styled(progress_bar, Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!(" {}%", percent),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]));
                lines.push(Line::from(""));

                // Cancel button
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Cancel",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::Success => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "✅ Authentication Successful!",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    format!("Successfully authenticated with {}", self.dialog.provider),
                    Style::default().fg(Color::White),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "(This dialog will close automatically)",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::Failed(error) => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "❌ Authentication Failed",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    error.clone(),
                    Style::default().fg(Color::Red),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Close",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
            AuthStatus::TimedOut => {
                lines.push(Line::from(""));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "⏱️  Authentication Timed Out",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "Please try again",
                    Style::default().fg(Color::White),
                )]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "[Esc] Close",
                    Style::default().fg(Color::DarkGray),
                )]));
            }
        }

        // Render content
        let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
        paragraph.render(inner, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_dialog_new() {
        let dialog = AuthDialog::new();
        assert!(!dialog.is_visible());
        assert_eq!(dialog.status, AuthStatus::WaitingForUser);
    }

    #[test]
    fn test_show_copilot_auth() {
        let mut dialog = AuthDialog::new();
        dialog.show_copilot_auth("https://github.com/login/device", "ABCD-1234", 300);

        assert!(dialog.is_visible());
        assert_eq!(dialog.provider, "GitHub Copilot");
        assert_eq!(dialog.user_code(), "ABCD-1234");
        assert_eq!(dialog.verification_url, "https://github.com/login/device");
    }

    #[test]
    fn test_set_status() {
        let mut dialog = AuthDialog::new();
        dialog.set_status(AuthStatus::Polling);
        assert_eq!(dialog.status, AuthStatus::Polling);

        dialog.set_status(AuthStatus::Success);
        assert_eq!(dialog.status, AuthStatus::Success);
    }

    #[test]
    fn test_hide() {
        let mut dialog = AuthDialog::new();
        dialog.show_copilot_auth("https://test.com", "CODE", 300);
        assert!(dialog.is_visible());

        dialog.hide();
        assert!(!dialog.is_visible());
    }
}
