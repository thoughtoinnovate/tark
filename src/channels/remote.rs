//! Remote channel runtime helpers (registry, logging, event bus)
//!
//! Used by --remote / --headless modes to track active sessions, emit
//! live events, and persist rolling logs.

use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::oneshot;

use crate::debug_logger::SensitiveDataRedactor;
use crate::plugins::ChannelInboundMessage;
use crate::tools::questionnaire::{ApprovalRequest, ApprovalResponse, Questionnaire, UserResponse};

const REMOTE_LOG_MAX_BYTES: u64 = 5 * 1024 * 1024;
const REMOTE_LOG_PREFIX: &str = "remote";

static REMOTE_RUNTIME: OnceLock<Arc<RemoteRuntime>> = OnceLock::new();
static LOCAL_INTERACTION: OnceLock<Arc<Mutex<Option<LocalInteraction>>>> = OnceLock::new();
static REMOTE_CHANNEL_MANAGER: OnceLock<Arc<crate::channels::ChannelManager>> = OnceLock::new();

#[derive(Clone)]
pub enum LocalInteraction {
    Questionnaire {
        data: Questionnaire,
        responder: Arc<tokio::sync::Mutex<Option<oneshot::Sender<UserResponse>>>>,
    },
    Approval {
        request: ApprovalRequest,
        responder: Arc<tokio::sync::Mutex<Option<oneshot::Sender<ApprovalResponse>>>>,
    },
}

fn local_interaction_state() -> &'static Arc<Mutex<Option<LocalInteraction>>> {
    LOCAL_INTERACTION.get_or_init(|| Arc::new(Mutex::new(None)))
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoteEvent {
    pub timestamp: String,
    pub event: String,
    pub runtime_id: String,
    pub plugin_id: String,
    pub session_id: String,
    pub conversation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl RemoteEvent {
    pub fn new(
        event: impl Into<String>,
        plugin_id: impl Into<String>,
        session_id: impl Into<String>,
        conversation_id: impl Into<String>,
        runtime_id: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Utc::now().to_rfc3339(),
            event: event.into(),
            runtime_id: runtime_id.into(),
            plugin_id: plugin_id.into(),
            session_id: session_id.into(),
            conversation_id: conversation_id.into(),
            user_id: None,
            message: None,
            metadata: None,
        }
    }

    pub fn with_user(mut self, user_id: Option<String>) -> Self {
        self.user_id = user_id;
        self
    }

    pub fn with_message(mut self, message: String) -> Self {
        self.message = Some(message);
        self
    }

    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteSessionEntry {
    pub session_id: String,
    pub runtime_id: String,
    pub plugin_id: String,
    pub conversation_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub trust_level: Option<String>,
    #[serde(default)]
    pub last_event_at: Option<String>,
    #[serde(default)]
    pub last_event: Option<String>,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub queued_count: usize,
    #[serde(default)]
    pub last_queued_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct RemoteRegistryData {
    sessions: HashMap<String, RemoteSessionEntry>,
}

#[derive(Debug, Clone)]
pub struct QueuedRemoteMessage {
    pub plugin_id: String,
    pub message: ChannelInboundMessage,
    pub received_at: String,
}

#[derive(Clone)]
pub struct RemoteRegistry {
    path: PathBuf,
    stop_dir: PathBuf,
    stop_all_path: PathBuf,
    interrupt_dir: PathBuf,
    interrupt_all_path: PathBuf,
    queues: Arc<Mutex<HashMap<String, VecDeque<QueuedRemoteMessage>>>>,
    state: Arc<Mutex<RemoteRegistryData>>,
}

impl RemoteRegistry {
    pub fn new(project_root: &Path) -> Result<Self> {
        let remote_dir = project_root.join("remote");
        let stop_dir = remote_dir.join("stops");
        let interrupt_dir = remote_dir.join("interrupts");
        std::fs::create_dir_all(&stop_dir)?;
        std::fs::create_dir_all(&interrupt_dir)?;
        let path = remote_dir.join("registry.json");
        let stop_all_path = remote_dir.join("stop_all");
        let interrupt_all_path = remote_dir.join("interrupt_all");
        let data = load_registry(&path)?;
        Ok(Self {
            path,
            stop_dir,
            stop_all_path,
            interrupt_dir,
            interrupt_all_path,
            queues: Arc::new(Mutex::new(HashMap::new())),
            state: Arc::new(Mutex::new(data)),
        })
    }

    pub fn snapshot(project_root: &Path) -> Result<Vec<RemoteSessionEntry>> {
        let path = project_root.join("remote").join("registry.json");
        let _lock = acquire_registry_lock(&path)?;
        Ok(load_registry(&path)?.sessions.into_values().collect())
    }

    pub fn update(&self, entry: RemoteSessionEntry) -> Result<()> {
        self.with_locked_registry(|data| {
            data.sessions.insert(entry.session_id.clone(), entry);
            Ok(())
        })
    }

    pub fn get(&self, session_id: &str) -> Option<RemoteSessionEntry> {
        self.state
            .lock()
            .ok()
            .and_then(|data| data.sessions.get(session_id).cloned())
    }

    pub fn sessions(&self) -> Vec<RemoteSessionEntry> {
        self.state
            .lock()
            .ok()
            .map(|data| data.sessions.values().cloned().collect())
            .unwrap_or_default()
    }

    pub fn mark_status(
        &self,
        session_id: &str,
        runtime_id: &str,
        plugin_id: &str,
        conversation_id: &str,
        status: &str,
    ) -> Result<RemoteSessionEntry> {
        self.with_locked_registry(|data| {
            let entry = data
                .sessions
                .entry(session_id.to_string())
                .or_insert_with(|| RemoteSessionEntry {
                    session_id: session_id.to_string(),
                    runtime_id: runtime_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: status.to_string(),
                    user_id: None,
                    channel_id: None,
                    guild_id: None,
                    provider: None,
                    model: None,
                    mode: None,
                    trust_level: None,
                    last_event_at: None,
                    last_event: None,
                    last_message: None,
                    queued_count: 0,
                    last_queued_message: None,
                });

            entry.status = status.to_string();
            entry.runtime_id = runtime_id.to_string();
            entry.last_event_at = Some(Utc::now().to_rfc3339());
            entry.last_event = Some(format!("status:{}", status));
            Ok(entry.clone())
        })
    }

    pub fn try_mark_running(
        &self,
        session_id: &str,
        runtime_id: &str,
        plugin_id: &str,
        conversation_id: &str,
    ) -> Result<bool> {
        self.with_locked_registry(|data| {
            let entry = data
                .sessions
                .entry(session_id.to_string())
                .or_insert_with(|| RemoteSessionEntry {
                    session_id: session_id.to_string(),
                    runtime_id: runtime_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: "idle".to_string(),
                    user_id: None,
                    channel_id: None,
                    guild_id: None,
                    provider: None,
                    model: None,
                    mode: None,
                    trust_level: None,
                    last_event_at: None,
                    last_event: None,
                    last_message: None,
                    queued_count: 0,
                    last_queued_message: None,
                });

            if entry.status == "running" {
                return Ok(false);
            }

            entry.status = "running".to_string();
            entry.runtime_id = runtime_id.to_string();
            entry.plugin_id = plugin_id.to_string();
            entry.conversation_id = conversation_id.to_string();
            entry.last_event_at = Some(Utc::now().to_rfc3339());
            entry.last_event = Some("status:running".to_string());
            Ok(true)
        })
    }

    pub fn set_last_message(
        &self,
        session_id: &str,
        runtime_id: &str,
        plugin_id: &str,
        conversation_id: &str,
        message: Option<String>,
    ) -> Result<()> {
        self.with_locked_registry(|data| {
            let entry = data
                .sessions
                .entry(session_id.to_string())
                .or_insert_with(|| RemoteSessionEntry {
                    session_id: session_id.to_string(),
                    runtime_id: runtime_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: "idle".to_string(),
                    user_id: None,
                    channel_id: None,
                    guild_id: None,
                    provider: None,
                    model: None,
                    mode: None,
                    trust_level: None,
                    last_event_at: None,
                    last_event: None,
                    last_message: None,
                    queued_count: 0,
                    last_queued_message: None,
                });

            entry.last_message = message;
            entry.runtime_id = runtime_id.to_string();
            entry.last_event_at = Some(Utc::now().to_rfc3339());
            Ok(())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_context(
        &self,
        session_id: &str,
        runtime_id: &str,
        plugin_id: &str,
        conversation_id: &str,
        provider: Option<String>,
        model: Option<String>,
        mode: Option<String>,
        trust_level: Option<String>,
        user_id: Option<String>,
        channel_id: Option<String>,
        guild_id: Option<String>,
    ) -> Result<()> {
        self.with_locked_registry(|data| {
            let entry = data
                .sessions
                .entry(session_id.to_string())
                .or_insert_with(|| RemoteSessionEntry {
                    session_id: session_id.to_string(),
                    runtime_id: runtime_id.to_string(),
                    plugin_id: plugin_id.to_string(),
                    conversation_id: conversation_id.to_string(),
                    status: "idle".to_string(),
                    user_id: None,
                    channel_id: None,
                    guild_id: None,
                    provider: None,
                    model: None,
                    mode: None,
                    trust_level: None,
                    last_event_at: None,
                    last_event: None,
                    last_message: None,
                    queued_count: 0,
                    last_queued_message: None,
                });

            if provider.is_some() {
                entry.provider = provider;
            }
            if model.is_some() {
                entry.model = model;
            }
            if mode.is_some() {
                entry.mode = mode;
            }
            if trust_level.is_some() {
                entry.trust_level = trust_level;
            }
            if user_id.is_some() {
                entry.user_id = user_id;
            }
            if channel_id.is_some() {
                entry.channel_id = channel_id;
            }
            if guild_id.is_some() {
                entry.guild_id = guild_id;
            }

            entry.runtime_id = runtime_id.to_string();
            entry.last_event_at = Some(Utc::now().to_rfc3339());
            entry.last_event = Some("context_update".to_string());
            Ok(())
        })
    }

    pub fn stop_session(&self, session_id: &str) -> Result<()> {
        std::fs::create_dir_all(&self.stop_dir)?;
        std::fs::write(self.stop_dir.join(session_id), "")?;
        let _ = self.with_locked_registry(|data| {
            if let Some(entry) = data.sessions.get_mut(session_id) {
                entry.status = "stopped".to_string();
                entry.last_event_at = Some(Utc::now().to_rfc3339());
                entry.last_event = Some("status:stopped".to_string());
            }
            Ok(())
        });
        Ok(())
    }

    pub fn resume_session(&self, session_id: &str) -> Result<()> {
        let stop_path = self.stop_dir.join(session_id);
        if stop_path.exists() {
            std::fs::remove_file(stop_path)?;
        }
        Ok(())
    }

    pub fn stop_all(&self) -> Result<()> {
        std::fs::write(&self.stop_all_path, "")?;
        Ok(())
    }

    pub fn resume_all(&self) -> Result<()> {
        if self.stop_all_path.exists() {
            std::fs::remove_file(&self.stop_all_path)?;
        }
        Ok(())
    }

    pub fn is_stopped(&self, session_id: &str) -> bool {
        self.stop_all_path.exists() || self.stop_dir.join(session_id).exists()
    }

    pub fn interrupt_session(&self, session_id: &str) -> Result<()> {
        std::fs::create_dir_all(&self.interrupt_dir)?;
        std::fs::write(self.interrupt_dir.join(session_id), "")?;
        let _ = self.with_locked_registry(|data| {
            if let Some(entry) = data.sessions.get_mut(session_id) {
                entry.last_event_at = Some(Utc::now().to_rfc3339());
                entry.last_event = Some("interrupt".to_string());
            }
            Ok(())
        });
        Ok(())
    }

    pub fn clear_interrupt(&self, session_id: &str) -> Result<()> {
        let interrupt_path = self.interrupt_dir.join(session_id);
        if interrupt_path.exists() {
            std::fs::remove_file(interrupt_path)?;
        }
        Ok(())
    }

    pub fn interrupt_all(&self) -> Result<()> {
        std::fs::write(&self.interrupt_all_path, "")?;
        Ok(())
    }

    pub fn clear_interrupt_all(&self) -> Result<()> {
        if self.interrupt_all_path.exists() {
            std::fs::remove_file(&self.interrupt_all_path)?;
        }
        Ok(())
    }

    pub fn is_interrupted(&self, session_id: &str) -> bool {
        self.interrupt_all_path.exists() || self.interrupt_dir.join(session_id).exists()
    }

    pub fn enqueue_message(&self, session_id: &str, message: QueuedRemoteMessage) -> Result<usize> {
        let preview = normalize_message_preview(&message.message.text);
        let count = {
            let mut queues = self
                .queues
                .lock()
                .map_err(|_| anyhow::anyhow!("Remote queue lock poisoned"))?;
            let entry = queues.entry(session_id.to_string()).or_default();
            entry.push_back(message);
            entry.len()
        };

        let _ = self.with_locked_registry(|data| {
            let entry = data
                .sessions
                .entry(session_id.to_string())
                .or_insert_with(|| RemoteSessionEntry {
                    session_id: session_id.to_string(),
                    runtime_id: String::new(),
                    plugin_id: String::new(),
                    conversation_id: String::new(),
                    status: "idle".to_string(),
                    user_id: None,
                    channel_id: None,
                    guild_id: None,
                    provider: None,
                    model: None,
                    mode: None,
                    trust_level: None,
                    last_event_at: None,
                    last_event: None,
                    last_message: None,
                    queued_count: 0,
                    last_queued_message: None,
                });
            entry.queued_count = count;
            entry.last_queued_message = Some(preview);
            entry.last_event_at = Some(Utc::now().to_rfc3339());
            entry.last_event = Some("queued".to_string());
            Ok(())
        });

        Ok(count)
    }

    pub fn drain_queue(&self, session_id: &str) -> Vec<QueuedRemoteMessage> {
        let drained = {
            let mut queues = match self.queues.lock() {
                Ok(guard) => guard,
                Err(_) => return Vec::new(),
            };
            queues
                .remove(session_id)
                .unwrap_or_else(VecDeque::new)
                .into_iter()
                .collect::<Vec<_>>()
        };

        if !drained.is_empty() {
            let _ = self.with_locked_registry(|data| {
                if let Some(entry) = data.sessions.get_mut(session_id) {
                    entry.queued_count = 0;
                    entry.last_queued_message = None;
                    entry.last_event_at = Some(Utc::now().to_rfc3339());
                    entry.last_event = Some("queue_drained".to_string());
                }
                Ok(())
            });
        }

        drained
    }

    pub fn queued_count(&self, session_id: &str) -> usize {
        self.queues
            .lock()
            .ok()
            .and_then(|queues| queues.get(session_id).map(|q| q.len()))
            .unwrap_or(0)
    }

    fn with_locked_registry<T>(
        &self,
        update: impl FnOnce(&mut RemoteRegistryData) -> Result<T>,
    ) -> Result<T> {
        let _lock = acquire_registry_lock(&self.path)?;
        let mut data = load_registry(&self.path)?;
        let result = update(&mut data)?;
        save_registry_unlocked(&self.path, &data)?;
        if let Ok(mut guard) = self.state.lock() {
            *guard = data;
        }
        Ok(result)
    }
}

fn load_registry(path: &Path) -> Result<RemoteRegistryData> {
    if !path.exists() {
        return Ok(RemoteRegistryData::default());
    }
    let content = std::fs::read_to_string(path)?;
    let data = serde_json::from_str(&content).unwrap_or_default();
    Ok(data)
}

fn save_registry_unlocked(path: &Path, data: &RemoteRegistryData) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(data)?;
    std::fs::write(path, content)?;
    Ok(())
}

struct RegistryLock {
    path: PathBuf,
}

impl Drop for RegistryLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_registry_lock(registry_path: &Path) -> Result<RegistryLock> {
    let lock_path = registry_path.with_extension("lock");
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    for _ in 0..50 {
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock_path)
        {
            Ok(_) => return Ok(RegistryLock { path: lock_path }),
            Err(err) if err.kind() == ErrorKind::AlreadyExists => {
                if let Ok(metadata) = std::fs::metadata(&lock_path) {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.elapsed() {
                            if elapsed > Duration::from_secs(30) {
                                let _ = std::fs::remove_file(&lock_path);
                                continue;
                            }
                        }
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(err) => return Err(err.into()),
        }
    }
    anyhow::bail!(
        "Timed out waiting for registry lock at {}",
        lock_path.display()
    );
}

struct RemoteLogger {
    log_dir: PathBuf,
    runtime_id: String,
    seq: u64,
    current_path: PathBuf,
    file: std::fs::File,
    current_size: u64,
    debug_full: bool,
}

impl RemoteLogger {
    fn new(log_dir: PathBuf, runtime_id: String, debug_full: bool) -> Result<Self> {
        std::fs::create_dir_all(&log_dir)?;
        let seq = 1;
        let (current_path, file) = open_new_log(&log_dir, &runtime_id, seq)?;
        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            log_dir,
            runtime_id,
            seq,
            current_path,
            file,
            current_size,
            debug_full,
        })
    }

    fn log(&mut self, entry: &RemoteEvent, redactor: &SensitiveDataRedactor) -> Result<()> {
        if !self.debug_full && !entry.event.starts_with("error") {
            return Ok(());
        }
        let mut value = serde_json::to_value(entry)?;
        redactor.redact_json(&mut value);
        let line = serde_json::to_string(&value)?;
        writeln!(self.file, "{}", line)?;
        self.current_size = self
            .current_size
            .saturating_add(line.len() as u64)
            .saturating_add(1);
        if self.current_size >= REMOTE_LOG_MAX_BYTES {
            self.rotate()?;
        }
        Ok(())
    }

    fn rotate(&mut self) -> Result<()> {
        self.seq = self.seq.saturating_add(1);
        let (path, file) = open_new_log(&self.log_dir, &self.runtime_id, self.seq)?;
        self.current_path = path;
        self.file = file;
        self.current_size = 0;
        Ok(())
    }
}

fn open_new_log(log_dir: &Path, runtime_id: &str, seq: u64) -> Result<(PathBuf, std::fs::File)> {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!(
        "{}-{}-{}-{:04}.log",
        REMOTE_LOG_PREFIX, runtime_id, timestamp, seq
    );
    let path = log_dir.join(filename);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("Failed to open remote log file {}", path.display()))?;
    Ok((path, file))
}

#[derive(Clone)]
pub struct RemoteRuntime {
    inner: Arc<RemoteRuntimeInner>,
}

struct RemoteRuntimeInner {
    allowed_plugins: Option<HashSet<String>>,
    events: broadcast::Sender<RemoteEvent>,
    logger: Mutex<RemoteLogger>,
    registry: RemoteRegistry,
    redactor: SensitiveDataRedactor,
    runtime_id: String,
}

impl RemoteRuntime {
    pub fn new(
        project_root: &Path,
        allowed_plugins: Option<HashSet<String>>,
        debug_full_logs: bool,
    ) -> Result<Self> {
        let log_dir = project_root.join("logs").join("remote");
        let runtime_id = format!(
            "{}-{}",
            Utc::now().format("%Y%m%d%H%M%S"),
            std::process::id()
        );
        let logger = RemoteLogger::new(log_dir, runtime_id.clone(), debug_full_logs)?;
        let registry = RemoteRegistry::new(project_root)?;
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            inner: Arc::new(RemoteRuntimeInner {
                allowed_plugins,
                events,
                logger: Mutex::new(logger),
                registry,
                redactor: SensitiveDataRedactor::new(),
                runtime_id,
            }),
        })
    }

    pub fn events(&self) -> broadcast::Sender<RemoteEvent> {
        self.inner.events.clone()
    }

    pub fn registry(&self) -> &RemoteRegistry {
        &self.inner.registry
    }

    pub fn runtime_id(&self) -> &str {
        &self.inner.runtime_id
    }

    pub fn allows_plugin(&self, plugin_id: &str) -> bool {
        self.inner
            .allowed_plugins
            .as_ref()
            .map(|set| set.contains(plugin_id))
            .unwrap_or(true)
    }

    pub fn emit(&self, event: RemoteEvent) {
        if let Ok(mut guard) = self.inner.logger.lock() {
            let _ = guard.log(&event, &self.inner.redactor);
        }
        let _ = self.inner.events.send(event);
    }
}

pub fn set_global_runtime(runtime: Arc<RemoteRuntime>) {
    let _ = REMOTE_RUNTIME.set(runtime);
}

pub fn global_runtime() -> Option<Arc<RemoteRuntime>> {
    REMOTE_RUNTIME.get().cloned()
}

pub fn set_global_channel_manager(manager: Arc<crate::channels::ChannelManager>) {
    let _ = REMOTE_CHANNEL_MANAGER.set(manager);
}

pub fn global_channel_manager() -> Option<Arc<crate::channels::ChannelManager>> {
    REMOTE_CHANNEL_MANAGER.get().cloned()
}

pub fn set_local_interaction(interaction: LocalInteraction) {
    if let Ok(mut guard) = local_interaction_state().lock() {
        *guard = Some(interaction);
    }
}

pub fn clear_local_interaction() {
    if let Ok(mut guard) = local_interaction_state().lock() {
        *guard = None;
    }
}

pub fn take_local_interaction() -> Option<LocalInteraction> {
    local_interaction_state()
        .lock()
        .ok()
        .and_then(|mut guard| guard.take())
}

pub fn normalize_message_preview(text: &str) -> String {
    const MAX_LEN: usize = 512;
    let trimmed = text.trim();
    if trimmed.len() <= MAX_LEN {
        return trimmed.to_string();
    }
    let mut end = MAX_LEN;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &trimmed[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn interrupt_flags_toggle() -> Result<()> {
        let dir = tempdir()?;
        let registry = RemoteRegistry::new(dir.path())?;
        let session_id = "sess-1";
        assert!(!registry.is_interrupted(session_id));
        registry.interrupt_session(session_id)?;
        assert!(registry.is_interrupted(session_id));
        registry.clear_interrupt(session_id)?;
        assert!(!registry.is_interrupted(session_id));
        Ok(())
    }

    #[test]
    fn queue_enqueue_and_drain() -> Result<()> {
        let dir = tempdir()?;
        let registry = RemoteRegistry::new(dir.path())?;
        let session_id = "sess-queue";
        let msg = ChannelInboundMessage {
            conversation_id: "conv-1".to_string(),
            user_id: "user-1".to_string(),
            text: "hello world".to_string(),
            metadata_json: String::new(),
        };
        let queued = registry.enqueue_message(
            session_id,
            QueuedRemoteMessage {
                plugin_id: "discord".to_string(),
                message: msg,
                received_at: Utc::now().to_rfc3339(),
            },
        )?;
        assert_eq!(queued, 1);
        assert_eq!(registry.queued_count(session_id), 1);
        let drained = registry.drain_queue(session_id);
        assert_eq!(drained.len(), 1);
        assert_eq!(registry.queued_count(session_id), 0);
        Ok(())
    }
}
