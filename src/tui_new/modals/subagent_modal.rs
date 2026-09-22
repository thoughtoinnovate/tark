//! Subagent modals: detail viewer, session-start grant, settings.
//!
//! Plan §C3. All three follow the `TrustModal` chrome (centered, rounded
//! border, `↑↓ Navigate / Enter Select / Esc Cancel` hint line).

use crate::tui_new::theme::Theme;
use crate::ui_backend::{SessionGrant, SubagentInfo, SubagentSettingsState, SubagentStatus};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
};

/// Center `w`×`h` inside `area` (clamped).
fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    }
}

fn modal_block<'a>(theme: &Theme, title: &str) -> Block<'a> {
    Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(Style::default().fg(theme.purple))
        .title(Line::from(vec![
            Span::raw(" "),
            Span::styled(
                title.to_string(),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ]))
        .title_alignment(ratatui::layout::Alignment::Center)
        .style(Style::default().bg(theme.bg_dark))
}

fn hint_line() -> Line<'static> {
    Line::from(vec![Span::raw("↑↓ Navigate   Enter Select   Esc Close")])
}

fn status_span(status: SubagentStatus) -> (&'static str, &'static str) {
    match status {
        SubagentStatus::Queued => ("○", "QUEUED"),
        SubagentStatus::Running => ("●", "Running"),
        SubagentStatus::WaitingInput => ("◌", "Waiting"),
        SubagentStatus::Completed => ("✓", "Done"),
        SubagentStatus::Failed => ("✗", "Failed"),
        SubagentStatus::Killed => ("⊗", "Killed"),
    }
}

// ========== Detail modal ==========

/// Per-subagent drill-down: status header, log tail, input box, key hints.
///
/// Data comes from `SharedState` (`subagents()` + `subagent_detail_input()`);
/// the controller owns focus/editing, the service poller owns refresh.
pub struct SubagentDetailModal<'a> {
    theme: &'a Theme,
    info: Option<SubagentInfo>,
    input: String,
    follow_tail: bool,
}

impl<'a> SubagentDetailModal<'a> {
    pub fn new(
        theme: &'a Theme,
        info: Option<SubagentInfo>,
        input: String,
        follow_tail: bool,
    ) -> Self {
        Self {
            theme,
            info,
            input,
            follow_tail,
        }
    }
}

impl Widget for SubagentDetailModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;
        let modal = centered(area, 76, 24);
        Clear.render(modal, buf);
        let title = match &self.info {
            Some(info) => format!("⑂ {}", info.title),
            None => "⑂ Subagent (gone)".to_string(),
        };
        let block = modal_block(theme, &title);
        let inner = block.inner(modal);
        block.render(modal, buf);

        let mut lines: Vec<Line> = vec![hint_line(), Line::from("")];
        match &self.info {
            None => {
                lines.push(Line::from("This subagent finished and was cleared."));
                lines.push(Line::from("Its summary is in the parent chat."));
            }
            Some(info) => {
                let (glyph, label) = status_span(info.status);
                let model = if info.overridden {
                    format!("{}·{} *", info.model, info.effort)
                } else {
                    format!("{}·{}", info.model, info.effort)
                };
                lines.push(Line::from(vec![
                    Span::raw(format!("{glyph} {label}  ")),
                    Span::styled(model, Style::default().fg(theme.text_muted)),
                    Span::raw(format!(
                        "  {}s  ⇉ {}/{}  ",
                        info.elapsed_s, info.tools_used, info.tools_cap
                    )),
                    Span::styled(info.id.clone(), Style::default().fg(theme.text_muted)),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(vec![Span::styled(
                    "─ Logs ─".to_string(),
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                )]));
                if !self.follow_tail {
                    lines.push(Line::from(vec![Span::styled(
                        "─ Paused (s resumes tail) ─",
                        Style::default().fg(theme.yellow),
                    )]));
                }
                let show_from = info.log_tail.len().saturating_sub(10);
                if info.log_tail.is_empty() {
                    lines.push(Line::from(vec![Span::styled(
                        "(no output yet)",
                        Style::default().fg(theme.text_muted),
                    )]));
                }
                for line in info.log_tail.iter().skip(show_from) {
                    lines.push(Line::from(format!("│ {line}")));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(vec![
                    Span::styled("> ", Style::default().fg(theme.cyan)),
                    Span::raw(self.input.clone()),
                    Span::styled("█", Style::default().fg(theme.cyan)),
                ]));
                lines.push(Line::from(vec![Span::styled(
                    "[f Followup=turn] [n Nudge=no turn] [x Kill] [s Tail]  ·  done: [Enter]=paste summary",
                    Style::default().fg(theme.text_muted),
                )]));
            }
        }

        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .render(inner, buf);
    }
}

// ========== Session-start grant modal ==========

/// First-spawn permission grant for a parent session (plan §C2/C3).
///
/// `selected`: 0 Allow session, 1 Allow once, 2 Deny. Scope/checkbox state
/// lives in the controller while choosing; the confirmed answer is stored
/// as `SessionGrant` via `Command::GrantSubagentScope`.
pub struct SubagentGrantModal<'a> {
    theme: &'a Theme,
    pub scope_all: bool,
    pub write_proxy: bool,
    pub shell_proxy: bool,
    pub never_ask: bool,
    pub selected: usize,
}

impl<'a> SubagentGrantModal<'a> {
    pub fn new(theme: &'a Theme) -> Self {
        Self {
            theme,
            scope_all: false,
            write_proxy: false,
            shell_proxy: false,
            never_ask: false,
            selected: 0,
        }
    }

    pub fn from_grant(theme: &'a Theme, grant: &SessionGrant) -> Self {
        Self {
            theme,
            scope_all: grant.write_agents.iter().any(|a| a == "*"),
            write_proxy: !grant.write_agents.is_empty(),
            shell_proxy: !grant.shell_agents.is_empty(),
            never_ask: grant.never_ask,
            selected: 0,
        }
    }
}

impl Widget for SubagentGrantModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;
        let modal = centered(area, 64, 18);
        Clear.render(modal, buf);
        let block = modal_block(theme, "⑂ Subagent permissions");
        let inner = block.inner(modal);
        block.render(modal, buf);

        let check = |on: bool| if on { "☑" } else { "☐" };
        let actions = ["Allow session", "Allow once", "Deny"];
        let mut lines: Vec<Line> = vec![
            hint_line(),
            Line::from(""),
            Line::from(format!(
                "Scope: {} this agent   {} all subagents in session",
                if !self.scope_all { "•" } else { " " },
                if self.scope_all { "•" } else { " " },
            )),
            Line::from(format!(
                "{} Read + SafeShell — auto, always on",
                check(true)
            )),
            Line::from(format!(
                "{} Write (subroot only) — prompt per use",
                check(self.write_proxy)
            )),
            Line::from(format!(
                "{} Shell (subroot only) — prompt per use",
                check(self.shell_proxy)
            )),
            Line::from(format!(
                "{} Never ask again this session (auto-deny extras)",
                check(self.never_ask)
            )),
            Line::from(""),
        ];
        for (i, action) in actions.iter().enumerate() {
            let style = if i == self.selected {
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let marker = if i == self.selected { "▸ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), style),
                Span::styled(action.to_string(), style),
            ]));
        }
        Paragraph::new(lines).render(inner, buf);
    }
}

// ========== Settings modal ==========

/// Settings rows (must stay in sync with controller key handling).
pub const SUBAGENT_SETTINGS_ROWS: usize = 6;

pub struct SubagentSettingsModal<'a> {
    theme: &'a Theme,
    settings: SubagentSettingsState,
    selected: usize,
}

impl<'a> SubagentSettingsModal<'a> {
    pub fn new(theme: &'a Theme, settings: SubagentSettingsState, selected: usize) -> Self {
        Self {
            theme,
            settings,
            selected,
        }
    }
}

impl Widget for SubagentSettingsModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let theme = self.theme;
        let modal = centered(area, 64, 20);
        Clear.render(modal, buf);
        let block = modal_block(theme, "⑂ Subagent settings");
        let inner = block.inner(modal);
        block.render(modal, buf);

        let s = &self.settings;
        let rows: Vec<(String, String)> = vec![
            (
                "Mode".to_string(),
                format!("{} [{}/{}]", s.mode, s.min_subagents, s.max_subagents),
            ),
            (
                "Tools/agent".to_string(),
                format!("{} [{}]", s.tools_mode, s.max_parallel_tools),
            ),
            (
                "Model".to_string(),
                if s.pin_mode == "pinned" {
                    format!(
                        "pinned {}/{} {}",
                        if s.pin_provider.is_empty() {
                            "inherit"
                        } else {
                            &s.pin_provider
                        },
                        if s.pin_model.is_empty() {
                            "inherit"
                        } else {
                            &s.pin_model
                        },
                        if s.pin_effort.is_empty() {
                            "inherit"
                        } else {
                            &s.pin_effort
                        },
                    )
                } else {
                    "inherit (parent snapshot)".to_string()
                },
            ),
            ("Pin provider".to_string(), s.pin_provider.clone()),
            ("Pin model".to_string(), s.pin_model.clone()),
            ("Pin effort".to_string(), s.pin_effort.clone()),
        ];
        let mut lines: Vec<Line> = vec![
            hint_line(),
            Line::from(vec![Span::styled(
                "Tab cycles value · w writes config.toml · live-applies to session",
                Style::default().fg(theme.text_muted),
            )]),
            Line::from(""),
        ];
        for (i, (label, value)) in rows.iter().enumerate() {
            let style = if i == self.selected {
                Style::default().fg(theme.cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_primary)
            };
            let marker = if i == self.selected { "▸ " } else { "  " };
            lines.push(Line::from(vec![
                Span::styled(marker.to_string(), style),
                Span::styled(format!("{label:14}"), style),
                Span::styled(value.clone(), style),
            ]));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "[Apply session] [Write file] [Reset defaults]",
            Style::default().fg(theme.text_muted),
        )]));
        Paragraph::new(lines).render(inner, buf);
    }
}
