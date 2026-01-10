//! Approval card widget for risky operation confirmation.
//!
//! Displays an interactive card when a risky operation needs user approval.
//! Users can approve once, for session, permanently, or deny the operation.
//! Pattern selection allows users to define approval patterns.

use crate::tools::{
    ApprovalChoice, ApprovalPattern, ApprovalRequest, ApprovalResponse, MatchType, SuggestedPattern,
};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use tokio::sync::oneshot;

/// State for the approval card popup
#[derive(Debug)]
pub struct ApprovalCardState {
    /// The approval request being displayed
    request: ApprovalRequest,
    /// Channel to send the response
    responder: Option<oneshot::Sender<ApprovalResponse>>,
    /// Selected pattern index
    selected_pattern: usize,
    /// Selected action index
    selected_action: usize,
    /// Whether editing custom pattern
    editing_pattern: bool,
    /// Custom pattern text
    custom_pattern: String,
    /// Custom pattern match type
    custom_match_type: MatchType,
    /// Whether the card is visible
    pub visible: bool,
}

impl ApprovalCardState {
    /// Create a new hidden approval card
    pub fn new() -> Self {
        Self {
            request: ApprovalRequest {
                tool: String::new(),
                command: String::new(),
                risk_level: crate::tools::RiskLevel::ReadOnly,
                suggested_patterns: Vec::new(),
            },
            responder: None,
            selected_pattern: 0,
            selected_action: 0,
            editing_pattern: false,
            custom_pattern: String::new(),
            custom_match_type: MatchType::Exact,
            visible: false,
        }
    }

    /// Open the approval card with a request
    pub fn open(&mut self, request: ApprovalRequest, responder: oneshot::Sender<ApprovalResponse>) {
        // Initialize custom pattern from command
        self.custom_pattern = request.command.clone();
        self.request = request;
        self.responder = Some(responder);
        self.selected_pattern = 0;
        self.selected_action = 0;
        self.editing_pattern = false;
        self.custom_match_type = MatchType::Exact;
        self.visible = true;
    }

    /// Close the approval card
    pub fn close(&mut self) {
        self.visible = false;
        self.responder = None;
    }

    /// Get the currently selected pattern
    fn get_selected_pattern(&self) -> ApprovalPattern {
        if self.editing_pattern {
            ApprovalPattern::new(
                self.request.tool.clone(),
                self.custom_pattern.clone(),
                self.custom_match_type,
            )
        } else if let Some(suggestion) = self.request.suggested_patterns.get(self.selected_pattern)
        {
            ApprovalPattern::new(
                self.request.tool.clone(),
                suggestion.pattern.clone(),
                suggestion.match_type,
            )
        } else {
            // Fallback to exact match of command
            ApprovalPattern::new(
                self.request.tool.clone(),
                self.request.command.clone(),
                MatchType::Exact,
            )
        }
    }

    /// Send response and close
    fn send_response(&mut self, choice: ApprovalChoice) {
        if let Some(responder) = self.responder.take() {
            let response = match choice {
                ApprovalChoice::ApproveOnce => ApprovalResponse::approve_once(),
                ApprovalChoice::ApproveSession => {
                    ApprovalResponse::approve_session(self.get_selected_pattern())
                }
                ApprovalChoice::ApproveAlways => {
                    ApprovalResponse::approve_always(self.get_selected_pattern())
                }
                ApprovalChoice::Deny => ApprovalResponse::deny(),
                ApprovalChoice::DenyAlways => {
                    ApprovalResponse::deny_always(self.get_selected_pattern())
                }
            };
            let _ = responder.send(response);
        }
        self.close();
    }

    /// Handle key input
    /// Returns true if the card was closed (response sent or cancelled)
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if !self.visible {
            return false;
        }

        // Handle pattern editing mode
        if self.editing_pattern {
            match key.code {
                KeyCode::Esc => {
                    self.editing_pattern = false;
                }
                KeyCode::Enter => {
                    self.editing_pattern = false;
                }
                KeyCode::Backspace => {
                    self.custom_pattern.pop();
                }
                KeyCode::Char(c) => {
                    self.custom_pattern.push(c);
                }
                KeyCode::Tab => {
                    // Cycle match type
                    self.custom_match_type = match self.custom_match_type {
                        MatchType::Exact => MatchType::Prefix,
                        MatchType::Prefix => MatchType::Glob,
                        MatchType::Glob => MatchType::Exact,
                    };
                }
                _ => {}
            }
            return false;
        }

        match key.code {
            // Escape closes with deny
            KeyCode::Esc => {
                self.send_response(ApprovalChoice::Deny);
                true
            }
            // Quick approval shortcuts
            KeyCode::Char('y') | KeyCode::Char('1') => {
                self.send_response(ApprovalChoice::ApproveOnce);
                true
            }
            KeyCode::Char('s') | KeyCode::Char('2') => {
                self.send_response(ApprovalChoice::ApproveSession);
                true
            }
            KeyCode::Char('p') | KeyCode::Char('3') => {
                self.send_response(ApprovalChoice::ApproveAlways);
                true
            }
            KeyCode::Char('n') | KeyCode::Char('4') => {
                self.send_response(ApprovalChoice::Deny);
                true
            }
            KeyCode::Char('N') | KeyCode::Char('5') => {
                self.send_response(ApprovalChoice::DenyAlways);
                true
            }
            // Edit pattern
            KeyCode::Char('e') => {
                self.editing_pattern = true;
                self.custom_pattern =
                    if let Some(p) = self.request.suggested_patterns.get(self.selected_pattern) {
                        p.pattern.clone()
                    } else {
                        self.request.command.clone()
                    };
                false
            }
            // Pattern navigation
            KeyCode::Up | KeyCode::Char('k') => {
                if !self.request.suggested_patterns.is_empty() {
                    self.selected_pattern = self.selected_pattern.saturating_sub(1);
                }
                false
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if !self.request.suggested_patterns.is_empty() {
                    self.selected_pattern =
                        (self.selected_pattern + 1).min(self.request.suggested_patterns.len() - 1);
                }
                false
            }
            // Enter confirms with currently selected action
            KeyCode::Enter => {
                let choice = match self.selected_action {
                    0 => ApprovalChoice::ApproveOnce,
                    1 => ApprovalChoice::ApproveSession,
                    2 => ApprovalChoice::ApproveAlways,
                    3 => ApprovalChoice::Deny,
                    4 => ApprovalChoice::DenyAlways,
                    _ => ApprovalChoice::Deny,
                };
                self.send_response(choice);
                true
            }
            // Tab to cycle through actions
            KeyCode::Tab => {
                self.selected_action = (self.selected_action + 1) % 5;
                false
            }
            KeyCode::BackTab => {
                self.selected_action = if self.selected_action == 0 {
                    4
                } else {
                    self.selected_action - 1
                };
                false
            }
            _ => false,
        }
    }

    /// Render the approval card popup
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        // Calculate popup size and position
        let popup_width = 60.min(area.width.saturating_sub(4));
        let popup_height = 20.min(area.height.saturating_sub(4));
        let popup_x = (area.width.saturating_sub(popup_width)) / 2;
        let popup_y = (area.height.saturating_sub(popup_height)) / 2;

        let popup_area = Rect::new(popup_x, popup_y, popup_width, popup_height);

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Risk level color
        let risk_color = self.request.risk_level.color();

        // Create the block
        let block = Block::default()
            .title(format!(
                " {} Approval Required ",
                self.request.risk_level.icon()
            ))
            .title_alignment(Alignment::Center)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(risk_color));

        let inner_area = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Layout
        let chunks = Layout::vertical([
            Constraint::Length(3), // Command info
            Constraint::Length(1), // Separator
            Constraint::Length(4), // Pattern selection
            Constraint::Length(1), // Separator
            Constraint::Length(6), // Actions
            Constraint::Min(1),    // Help
        ])
        .split(inner_area);

        // Command info
        let command_info = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Tool: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    &self.request.tool,
                    Style::default().add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("Command: ", Style::default().fg(Color::Gray)),
                Span::styled(
                    truncate_str(&self.request.command, (popup_width - 12) as usize),
                    Style::default().fg(Color::White),
                ),
            ]),
        ])
        .wrap(Wrap { trim: true });
        frame.render_widget(command_info, chunks[0]);

        // Pattern selection
        if self.editing_pattern {
            // Show editing interface
            let edit_lines = vec![
                Line::from(" Edit Pattern:"),
                Line::from(vec![
                    Span::raw(" > "),
                    Span::styled(
                        &self.custom_pattern,
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::UNDERLINED),
                    ),
                    Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
                ]),
                Line::from(vec![
                    Span::styled(" Type: ", Style::default().fg(Color::Gray)),
                    Span::styled(
                        format!("[Tab to change: {}]", self.custom_match_type.label()),
                        Style::default().fg(Color::Cyan),
                    ),
                ]),
            ];
            let edit_para = Paragraph::new(edit_lines);
            frame.render_widget(edit_para, chunks[2]);
        } else {
            // Show pattern list
            let pattern_items: Vec<ListItem> = self
                .request
                .suggested_patterns
                .iter()
                .enumerate()
                .map(|(i, pattern)| {
                    let marker = if i == self.selected_pattern {
                        "●"
                    } else {
                        "○"
                    };
                    let style = if i == self.selected_pattern {
                        Style::default().bg(Color::DarkGray)
                    } else {
                        Style::default()
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!(" {} ", marker), style),
                        Span::styled(
                            truncate_str(&pattern.pattern, 30),
                            style.add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!(" ({})", pattern.match_type.label()),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                    .style(style)
                })
                .collect();

            if !pattern_items.is_empty() {
                let pattern_list = List::new(pattern_items);
                frame.render_widget(pattern_list, chunks[2]);
            }
        }

        // Actions
        let actions = ApprovalChoice::all();
        let action_items: Vec<ListItem> = actions
            .iter()
            .enumerate()
            .map(|(i, choice)| {
                let style = if i == self.selected_action {
                    Style::default()
                        .bg(Color::DarkGray)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let color = match choice {
                    ApprovalChoice::ApproveOnce
                    | ApprovalChoice::ApproveSession
                    | ApprovalChoice::ApproveAlways => Color::Green,
                    ApprovalChoice::Deny | ApprovalChoice::DenyAlways => Color::Red,
                };

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!(" [{}] ", choice.shortcut()),
                        Style::default().fg(Color::Yellow),
                    ),
                    Span::styled(choice.label(), style.fg(color)),
                ]))
            })
            .collect();

        let action_list = List::new(action_items);
        frame.render_widget(action_list, chunks[4]);

        // Help text
        let help = if self.editing_pattern {
            Paragraph::new(Line::from(vec![
                Span::styled("Enter", Style::default().fg(Color::Yellow)),
                Span::raw(" confirm  "),
                Span::styled("Tab", Style::default().fg(Color::Yellow)),
                Span::raw(" change type  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(" cancel edit"),
            ]))
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled("j/k", Style::default().fg(Color::Yellow)),
                Span::raw(" pattern  "),
                Span::styled("1-5", Style::default().fg(Color::Yellow)),
                Span::raw(" action  "),
                Span::styled("e", Style::default().fg(Color::Yellow)),
                Span::raw(" edit  "),
                Span::styled("Esc", Style::default().fg(Color::Yellow)),
                Span::raw(" deny"),
            ]))
        }
        .alignment(Alignment::Center);
        frame.render_widget(help, chunks[5]);
    }
}

impl Default for ApprovalCardState {
    fn default() -> Self {
        Self::new()
    }
}

/// Truncate a string to a maximum length, adding "..." if truncated
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len > 3 {
        format!("{}...", &s[..max_len - 3])
    } else {
        s[..max_len].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::RiskLevel;

    fn create_test_request() -> ApprovalRequest {
        ApprovalRequest {
            tool: "shell".to_string(),
            command: "npm install lodash".to_string(),
            risk_level: RiskLevel::Risky,
            suggested_patterns: vec![
                SuggestedPattern {
                    pattern: "npm install lodash".to_string(),
                    match_type: MatchType::Exact,
                    description: "Exact command".to_string(),
                },
                SuggestedPattern {
                    pattern: "npm install".to_string(),
                    match_type: MatchType::Prefix,
                    description: "All npm install".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_default_state() {
        let state = ApprovalCardState::new();
        assert!(!state.visible);
    }

    #[test]
    fn test_open_close() {
        let mut state = ApprovalCardState::new();
        let (tx, _rx) = oneshot::channel();
        state.open(create_test_request(), tx);
        assert!(state.visible);
        state.close();
        assert!(!state.visible);
    }

    #[test]
    fn test_pattern_navigation() {
        let mut state = ApprovalCardState::new();
        let (tx, _rx) = oneshot::channel();
        state.open(create_test_request(), tx);

        assert_eq!(state.selected_pattern, 0);
        state.handle_key(KeyEvent::from(KeyCode::Down));
        assert_eq!(state.selected_pattern, 1);
        state.handle_key(KeyEvent::from(KeyCode::Up));
        assert_eq!(state.selected_pattern, 0);
    }

    #[test]
    fn test_quick_approve() {
        let mut state = ApprovalCardState::new();
        let (tx, rx) = oneshot::channel();
        state.open(create_test_request(), tx);

        // Quick approve with 'y'
        let closed = state.handle_key(KeyEvent::from(KeyCode::Char('y')));
        assert!(closed);
        assert!(!state.visible);

        // Check response
        let response = rx.blocking_recv().unwrap();
        assert!(matches!(response.choice, ApprovalChoice::ApproveOnce));
    }
}
