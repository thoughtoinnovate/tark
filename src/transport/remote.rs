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
use crate::channels::request_channel_shutdown;
use crate::config::Config;
use crate::storage::TarkStorage;
use crate::transport::http::run_http_server;

const EVENT_BUFFER: usize = 300;

pub async fn run_remote_tui(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
    remote_provider_override: Option<String>,
    remote_model_override: Option<String>,
) -> Result<()> {
    let (remote, server_handle, project_root) = start_remote_server(
        working_dir.clone(),
        plugin_id,
        debug_full,
        remote_provider_override,
        remote_model_override,
    )
    .await?;

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
    let config = Config::load().unwrap_or_default();

    loop {
        while let Ok(event) = rx.try_recv() {
            state.push_event(event);
        }

        if last_refresh.elapsed() >= Duration::from_secs(2) {
            state.refresh_sessions()?;
            last_refresh = Instant::now();
        }
        state.refresh_widgets(&config).await?;

        terminal.draw(|frame| {
            let size = frame.area();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(50),
                    Constraint::Percentage(20),
                ])
                .split(size);

            let sessions = state.render_sessions();
            frame.render_widget(sessions, chunks[0]);

            let events = state.render_events(remote.runtime_id());
            frame.render_widget(events, chunks[1]);

            let widgets = state.render_widgets();
            frame.render_widget(widgets, chunks[2]);
        })?;

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        request_channel_shutdown();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();

    request_channel_shutdown();
    server_handle.abort();
    Ok(())
}

pub async fn run_remote_headless(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
    remote_provider_override: Option<String>,
    remote_model_override: Option<String>,
) -> Result<()> {
    let (remote, server_handle, _project_root) = start_remote_server(
        working_dir.clone(),
        plugin_id,
        debug_full,
        remote_provider_override,
        remote_model_override,
    )
    .await?;
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
                request_channel_shutdown();
                break;
            }
            event = rx.recv() => {
                if let Ok(event) = event {
                    print_event(&event);
                }
            }
        }
    }

    request_channel_shutdown();
    server_handle.abort();
    Ok(())
}

async fn start_remote_server(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
    remote_provider_override: Option<String>,
    remote_model_override: Option<String>,
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
    crate::channels::remote::set_global_runtime(Arc::clone(&remote));
    let remote_for_server = Arc::clone(&remote);

    let host = config.server.host.clone();
    let port = config.server.port;
    let http_enabled = config.remote.http_enabled;
    let plugin_id = plugin_id.to_string();

    let server_handle = tokio::spawn(async move {
        if http_enabled {
            if let Err(err) = run_http_server(
                &host,
                port,
                working_dir,
                Some(remote_for_server),
                Some(plugin_id),
                remote_provider_override,
                remote_model_override,
            )
            .await
            {
                tracing::error!("Remote HTTP server failed: {}", err);
            }
        } else {
            tracing::info!("Remote HTTP server disabled (remote.http_enabled=false)");
        }
    });

    Ok((remote, server_handle, project_root))
}

pub async fn start_remote_runtime(
    working_dir: PathBuf,
    plugin_id: &str,
    debug_full: bool,
    remote_provider_override: Option<String>,
    remote_model_override: Option<String>,
) -> Result<(tokio::task::JoinHandle<()>, PathBuf)> {
    let (_remote, server_handle, project_root) = start_remote_server(
        working_dir,
        plugin_id,
        debug_full,
        remote_provider_override,
        remote_model_override,
    )
    .await?;
    Ok((server_handle, project_root))
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
    plugin_widgets: Vec<crate::ui_backend::PluginWidgetInfo>,
    last_widget_refresh: Instant,
    widget_refresh_inflight: bool,
}

impl RemoteUiState {
    fn new(project_root: PathBuf) -> Result<Self> {
        let sessions = RemoteRegistry::snapshot(&project_root).unwrap_or_default();
        Ok(Self {
            project_root,
            sessions,
            events: VecDeque::with_capacity(EVENT_BUFFER),
            plugin_widgets: Vec::new(),
            last_widget_refresh: Instant::now() - Duration::from_secs(60),
            widget_refresh_inflight: false,
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

    async fn refresh_widgets(&mut self, config: &Config) -> Result<()> {
        let poll_ms = config.tui.plugin_widget_poll_ms;
        if self.last_widget_refresh.elapsed() < Duration::from_millis(poll_ms) {
            return Ok(());
        }
        if self.widget_refresh_inflight {
            return Ok(());
        }
        self.widget_refresh_inflight = true;
        let project_root = self.project_root.clone();
        let widget_states = match tokio::time::timeout(
            Duration::from_millis(500),
            tokio::task::spawn_blocking(move || {
                crate::plugins::collect_channel_widgets(&project_root)
            }),
        )
        .await
        {
            Ok(Ok(Ok(states))) => states,
            Ok(Ok(Err(err))) => {
                tracing::warn!("Plugin widgets refresh failed: {}", err);
                Vec::new()
            }
            Ok(Err(err)) => {
                tracing::warn!("Plugin widgets refresh task failed: {}", err);
                Vec::new()
            }
            Err(_) => {
                tracing::warn!("Plugin widgets refresh timed out");
                Vec::new()
            }
        };
        let updated_at = Utc::now().format("%H:%M:%S").to_string();
        let mut widgets = Vec::new();
        for state in widget_states {
            let mut error = state.error;
            let mut status = None;
            let mut attributes = serde_json::Value::Object(Default::default());
            if let Some(payload) = state.payload {
                match serde_json::from_str::<serde_json::Value>(&payload) {
                    Ok(serde_json::Value::Object(map)) => {
                        if let Some(serde_json::Value::String(s)) = map.get("status") {
                            status = Some(s.clone());
                        }
                        attributes = serde_json::Value::Object(map);
                    }
                    Ok(value) => {
                        error = Some("invalid_widget_shape".to_string());
                        attributes = value;
                    }
                    Err(_) => {
                        error = Some("invalid_widget_json".to_string());
                    }
                }
            }
            widgets.push(crate::ui_backend::PluginWidgetInfo {
                plugin_id: state.plugin_id,
                attributes,
                status,
                error,
                updated_at: Some(updated_at.clone()),
            });
        }
        self.plugin_widgets = widgets;
        self.last_widget_refresh = Instant::now();
        self.widget_refresh_inflight = false;
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

    fn render_widgets(&self) -> Paragraph<'_> {
        let mut lines: Vec<Line<'_>> = Vec::new();
        for widget in &self.plugin_widgets {
            let status = widget.status.as_deref().unwrap_or("unknown");
            lines.push(Line::from(Span::raw(format!(
                "{} [{}]",
                widget.plugin_id, status
            ))));
            if let Some(err) = widget.error.as_ref() {
                lines.push(Line::from(Span::raw(format!("  error: {}", err))));
                continue;
            }
            let mut fields = Vec::new();
            flatten_json(&widget.attributes, "", &mut fields);
            for (key, value) in fields {
                lines.push(Line::from(Span::raw(format!("  {}: {}", key, value))));
            }
        }
        Paragraph::new(lines)
            .block(Block::default().title("Plugins").borders(Borders::ALL))
            .style(Style::default().fg(Color::Gray))
    }
}

fn flatten_json(value: &serde_json::Value, prefix: &str, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let next = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{}.{}", prefix, key)
                };
                flatten_json(val, &next, out);
            }
        }
        serde_json::Value::Array(list) => {
            let mut rendered = Vec::new();
            for item in list {
                rendered.push(match item {
                    serde_json::Value::String(s) => s.clone(),
                    _ => item.to_string(),
                });
            }
            out.push((prefix.to_string(), rendered.join(", ")));
        }
        serde_json::Value::Null => out.push((prefix.to_string(), "null".to_string())),
        serde_json::Value::Bool(b) => out.push((prefix.to_string(), b.to_string())),
        serde_json::Value::Number(n) => out.push((prefix.to_string(), n.to_string())),
        serde_json::Value::String(s) => out.push((prefix.to_string(), s.clone())),
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
