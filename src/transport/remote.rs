//! Remote runtime entrypoints (TUI/headless)

use anyhow::{Context, Result};
use chrono::Utc;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Terminal,
};
use std::collections::VecDeque;
use std::io::stdout;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::channels::remote::{RemoteEvent, RemoteRegistry, RemoteRuntime};
use crate::config::Config;
use crate::storage::TarkStorage;
use crate::transport::http::run_http_server;

const EVENT_BUFFER: usize = 300;

pub async fn run_remote_tui(working_dir: PathBuf, plugin_id: &str, debug_full: bool) -> Result<()> {
    let (remote, server_handle, project_root) =
        start_remote_server(working_dir.clone(), plugin_id, debug_full).await?;

    let mut rx = remote.events().subscribe();

    if !crossterm::tty::IsTty::is_tty(&std::io::stdout()) {
        anyhow::bail!("Remote TUI requires a real terminal (TTY).");
    }

    enable_raw_mode().context("Failed to enable terminal raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("Failed to enter alternate screen")?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("Failed to create terminal backend")?;

    let mut state = RemoteUiState::new(project_root)?;
    let mut last_refresh = Instant::now();

    loop {
        while let Ok(event) = rx.try_recv() {
            state.push_event(event);
        }

        if last_refresh.elapsed() >= Duration::from_secs(2) {
            state.refresh_sessions()?;
            last_refresh = Instant::now();
        }

        terminal.draw(|frame| {
            let size = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
                .split(size);

            let sessions = state.render_sessions();
            frame.render_widget(sessions, chunks[0]);

            let events = state.render_events(remote.runtime_id());
            frame.render_widget(events, chunks[1]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    server_handle.abort();
    Ok(())
}

pub async fn run_remote_headless(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
) -> Result<()> {
    let (remote, server_handle, _project_root) =
        start_remote_server(working_dir.clone(), plugin_id, debug_full).await?;
    let mut rx = remote.events().subscribe();

    println!(
        "[remote] runtime={} plugin={} started (debug_full={})",
        remote.runtime_id(),
        plugin_id,
        debug_full
    );

    let ctrl_c = tokio::signal::ctrl_c();
    tokio::pin!(ctrl_c);

    loop {
        tokio::select! {
            _ = &mut ctrl_c => {
                println!("[remote] shutdown requested");
                break;
            }
            event = rx.recv() => {
                if let Ok(event) = event {
                    print_event(&event);
                }
            }
        }
    }

    server_handle.abort();
    Ok(())
}

async fn start_remote_server(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
) -> Result<(Arc<RemoteRuntime>, tokio::task::JoinHandle<()>, PathBuf)> {
    let config = Config::load().unwrap_or_default();
    let storage = TarkStorage::new(&working_dir).context("Failed to initialize tark storage")?;
    let project_root = storage.project_root().to_path_buf();

    let allowed_plugins = if config.remote.allowed_plugins.is_empty() {
        Some(std::iter::once(plugin_id.to_string()).collect())
    } else {
        Some(config.remote.allowed_plugins.iter().cloned().collect())
    };

    let remote = Arc::new(RemoteRuntime::new(
        &project_root,
        allowed_plugins,
        debug_full,
    )?);
    let remote_for_server = Arc::clone(&remote);

    let host = config.server.host.clone();
    let port = config.server.port;

    let server_handle = tokio::spawn(async move {
        if let Err(err) = run_http_server(&host, port, working_dir, Some(remote_for_server)).await {
            tracing::error!("Remote HTTP server failed: {}", err);
        }
    });

    Ok((remote, server_handle, project_root))
}

fn print_event(event: &RemoteEvent) {
    let ts = &event.timestamp;
    let msg = event.message.as_deref().unwrap_or("");
    if msg.is_empty() {
        println!(
            "[{}] {} session={} event={}",
            ts, event.plugin_id, event.session_id, event.event
        );
    } else {
        println!(
            "[{}] {} session={} event={} msg={}",
            ts, event.plugin_id, event.session_id, event.event, msg
        );
    }
}

struct RemoteUiState {
    project_root: PathBuf,
    sessions: Vec<crate::channels::remote::RemoteSessionEntry>,
    events: VecDeque<RemoteEvent>,
}

impl RemoteUiState {
    fn new(project_root: PathBuf) -> Result<Self> {
        let sessions = RemoteRegistry::snapshot(&project_root).unwrap_or_default();
        Ok(Self {
            project_root,
            sessions,
            events: VecDeque::with_capacity(EVENT_BUFFER),
        })
    }

    fn push_event(&mut self, event: RemoteEvent) {
        if self.events.len() >= EVENT_BUFFER {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    fn refresh_sessions(&mut self) -> Result<()> {
        self.sessions = RemoteRegistry::snapshot(&self.project_root).unwrap_or_default();
        Ok(())
    }

    fn render_sessions(&self) -> List<'_> {
        let items: Vec<ListItem<'_>> = self
            .sessions
            .iter()
            .map(|session| {
                let line = format!(
                    "{} {} {} {}",
                    session.status,
                    session.session_id,
                    session.mode.clone().unwrap_or_else(|| "-".to_string()),
                    session.model.clone().unwrap_or_else(|| "-".to_string()),
                );
                ListItem::new(line)
            })
            .collect();

        List::new(items).block(Block::default().title("Sessions").borders(Borders::ALL))
    }

    fn render_events(&self, runtime_id: &str) -> Paragraph<'_> {
        let mut lines: Vec<Line<'_>> = Vec::new();
        for event in self.events.iter().rev().take(100) {
            let line = if let Some(msg) = &event.message {
                format!("{} {} {}", event.event, event.session_id, msg)
            } else {
                format!("{} {}", event.event, event.session_id)
            };
            lines.push(Line::from(Span::raw(line)));
        }

        let title = format!("Events [{}] {}", runtime_id, Utc::now().format("%H:%M:%S"));
        Paragraph::new(lines)
            .block(Block::default().title(title).borders(Borders::ALL))
            .style(Style::default().fg(Color::Gray))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_ui_state_event_buffer_limit() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let mut state = RemoteUiState::new(temp_dir.path().to_path_buf()).expect("state");

        for idx in 0..(EVENT_BUFFER + 1) {
            let event = RemoteEvent::new(
                format!("event{}", idx),
                "plugin",
                "session",
                "conversation",
                "runtime",
            );
            state.push_event(event);
        }

        assert_eq!(state.events.len(), EVENT_BUFFER);
        let first = state.events.front().expect("event");
        assert_eq!(first.event, "event1");
    }
}
