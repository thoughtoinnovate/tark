//! Editor Bridge for Neovim RPC communication
//!
//! This module provides the communication layer between the TUI and Neovim
//! via Unix socket RPC protocol. It handles sending commands to Neovim
//! (open file, apply diff, etc.) and receiving notifications (buffer changes,
//! diagnostics updates, cursor movements).
//!
//! Note: Unix socket communication is only available on Unix platforms.
//! On Windows, the editor bridge provides stub implementations that return
//! appropriate errors.

// Allow dead code for intentionally unused API methods that are part of the public interface
// These methods are designed for future use when the TUI is fully integrated with Neovim
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
#[cfg(unix)]
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot, Mutex};

// ============================================================================
// RPC Protocol Types
// ============================================================================

/// Diagnostic severity levels (matching LSP)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    #[default]
    Error,
    Warning,
    Info,
    Hint,
}

/// A diagnostic message from the editor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// File path
    pub path: String,
    /// Line number (1-indexed)
    pub line: u32,
    /// Column number (1-indexed)
    pub col: u32,
    /// End line (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    /// End column (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_col: Option<u32>,
    /// Severity level
    pub severity: DiagnosticSeverity,
    /// Diagnostic message
    pub message: String,
    /// Source (e.g., "rust-analyzer", "eslint")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Diagnostic code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// Buffer information from the editor
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferInfo {
    /// Buffer ID
    pub id: u32,
    /// File path (if file-backed)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Buffer name
    pub name: String,
    /// Whether the buffer is modified
    pub modified: bool,
    /// File type (e.g., "rust", "python")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filetype: Option<String>,
}

/// RPC Message types for TUI ↔ Neovim communication
///
/// Messages are JSON-encoded with a "type" field for discrimination.
/// Each message is sent as a single line terminated by newline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum RpcMessage {
    // ========================================================================
    // TUI -> Neovim Commands (Requests)
    // ========================================================================
    /// Open a file in the editor
    #[serde(rename = "open_file")]
    OpenFile {
        /// File path to open
        path: String,
        /// Optional line number to jump to (1-indexed)
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<u32>,
        /// Optional column number (1-indexed)
        #[serde(skip_serializing_if = "Option::is_none")]
        col: Option<u32>,
    },

    /// Go to a specific line in the current buffer
    #[serde(rename = "goto_line")]
    GotoLine {
        /// Line number (1-indexed)
        line: u32,
        /// Optional column number (1-indexed)
        #[serde(skip_serializing_if = "Option::is_none")]
        col: Option<u32>,
    },

    /// Apply a diff/patch to a file
    #[serde(rename = "apply_diff")]
    ApplyDiff {
        /// File path to apply diff to
        path: String,
        /// Unified diff content
        diff: String,
    },

    /// Show a side-by-side diff view
    #[serde(rename = "show_diff")]
    ShowDiff {
        /// File path to show diff for
        path: String,
        /// Optional: original content (if not from disk)
        #[serde(skip_serializing_if = "Option::is_none")]
        original: Option<String>,
        /// Optional: modified content
        #[serde(skip_serializing_if = "Option::is_none")]
        modified: Option<String>,
    },

    /// Request list of open buffers
    #[serde(rename = "get_buffers")]
    GetBuffers,

    /// Request current diagnostics
    #[serde(rename = "get_diagnostics")]
    GetDiagnostics {
        /// Optional: filter by file path
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },

    /// Request current buffer content
    #[serde(rename = "get_buffer_content")]
    GetBufferContent {
        /// File path or buffer ID
        path: String,
    },

    /// Request current cursor position
    #[serde(rename = "get_cursor")]
    GetCursor,

    // ========================================================================
    // Neovim -> TUI Notifications
    // ========================================================================
    /// Buffer content changed
    #[serde(rename = "buffer_changed")]
    BufferChanged {
        /// File path
        path: String,
        /// New content (may be truncated for large files)
        #[serde(skip_serializing_if = "Option::is_none")]
        content: Option<String>,
        /// Whether content was truncated
        #[serde(default)]
        truncated: bool,
    },

    /// Diagnostics updated
    #[serde(rename = "diagnostics_updated")]
    DiagnosticsUpdated {
        /// List of diagnostics
        diagnostics: Vec<Diagnostic>,
    },

    /// Cursor moved
    #[serde(rename = "cursor_moved")]
    CursorMoved {
        /// File path
        path: String,
        /// Line number (1-indexed)
        line: u32,
        /// Column number (1-indexed)
        col: u32,
    },

    /// Buffer entered (switched to)
    #[serde(rename = "buffer_entered")]
    BufferEntered {
        /// File path
        path: String,
        /// File type
        #[serde(skip_serializing_if = "Option::is_none")]
        filetype: Option<String>,
    },

    /// Editor is closing
    #[serde(rename = "editor_closed")]
    EditorClosed,

    // ========================================================================
    // Response Messages
    // ========================================================================
    /// Successful response to a request
    #[serde(rename = "response")]
    Response {
        /// Request ID this responds to
        id: u64,
        /// Result data (type depends on request)
        result: serde_json::Value,
    },

    /// Error response to a request
    #[serde(rename = "error")]
    Error {
        /// Request ID this responds to (if applicable)
        #[serde(skip_serializing_if = "Option::is_none")]
        id: Option<u64>,
        /// Error message
        message: String,
        /// Error code (optional)
        #[serde(skip_serializing_if = "Option::is_none")]
        code: Option<i32>,
    },

    // ========================================================================
    // Request Wrapper (for requests that need responses)
    // ========================================================================
    /// Request wrapper with ID for tracking responses
    #[serde(rename = "request")]
    Request {
        /// Unique request ID
        id: u64,
        /// The actual command
        command: Box<RpcMessage>,
    },

    /// Ping/heartbeat
    #[serde(rename = "ping")]
    Ping {
        /// Timestamp or sequence number
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },

    /// Pong response to ping
    #[serde(rename = "pong")]
    Pong {
        /// Echo back the sequence number
        #[serde(skip_serializing_if = "Option::is_none")]
        seq: Option<u64>,
    },
}

impl RpcMessage {
    /// Create an OpenFile message
    pub fn open_file(path: impl Into<String>, line: Option<u32>) -> Self {
        Self::OpenFile {
            path: path.into(),
            line,
            col: None,
        }
    }

    /// Create a GotoLine message
    pub fn goto_line(line: u32, col: Option<u32>) -> Self {
        Self::GotoLine { line, col }
    }

    /// Create an ApplyDiff message
    pub fn apply_diff(path: impl Into<String>, diff: impl Into<String>) -> Self {
        Self::ApplyDiff {
            path: path.into(),
            diff: diff.into(),
        }
    }

    /// Create a ShowDiff message
    pub fn show_diff(path: impl Into<String>) -> Self {
        Self::ShowDiff {
            path: path.into(),
            original: None,
            modified: None,
        }
    }

    /// Create a GetBuffers message
    pub fn get_buffers() -> Self {
        Self::GetBuffers
    }

    /// Create a GetDiagnostics message
    pub fn get_diagnostics(path: Option<String>) -> Self {
        Self::GetDiagnostics { path }
    }

    /// Serialize to JSON string (single line)
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Deserialize from JSON string
    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Check if this is a notification (doesn't expect a response)
    pub fn is_notification(&self) -> bool {
        matches!(
            self,
            Self::BufferChanged { .. }
                | Self::DiagnosticsUpdated { .. }
                | Self::CursorMoved { .. }
                | Self::BufferEntered { .. }
                | Self::EditorClosed
                | Self::Pong { .. }
        )
    }

    /// Check if this is a response
    pub fn is_response(&self) -> bool {
        matches!(self, Self::Response { .. } | Self::Error { .. })
    }
}

// ============================================================================
// Response Types
// ============================================================================

/// Response to GetBuffers request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuffersResponse {
    pub buffers: Vec<BufferInfo>,
}

/// Response to GetDiagnostics request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsResponse {
    pub diagnostics: Vec<Diagnostic>,
}

/// Response to GetBufferContent request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BufferContentResponse {
    pub path: String,
    pub content: String,
    pub truncated: bool,
}

/// Response to GetCursor request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CursorResponse {
    pub path: String,
    pub line: u32,
    pub col: u32,
}

/// Generic success response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuccessResponse {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

// ============================================================================
// Editor State
// ============================================================================

/// Current state of the editor as known by the TUI
#[derive(Debug, Clone, Default)]
pub struct EditorState {
    /// Open buffers
    pub buffers: Vec<BufferInfo>,
    /// Current file path
    pub current_file: Option<String>,
    /// Current cursor position
    pub cursor: Option<(u32, u32)>,
    /// Current diagnostics
    pub diagnostics: Vec<Diagnostic>,
    /// Whether connected to editor
    pub connected: bool,
    /// Files modified in this session
    pub modified_files: Vec<String>,
}

impl EditorState {
    /// Create a new empty editor state
    pub fn new() -> Self {
        Self::default()
    }

    /// Update state from a buffer changed notification
    pub fn handle_buffer_changed(&mut self, path: &str, _content: Option<&str>) {
        if !self.modified_files.contains(&path.to_string()) {
            self.modified_files.push(path.to_string());
        }
    }

    /// Update state from a diagnostics updated notification
    pub fn handle_diagnostics_updated(&mut self, diagnostics: Vec<Diagnostic>) {
        self.diagnostics = diagnostics;
    }

    /// Update state from a cursor moved notification
    pub fn handle_cursor_moved(&mut self, path: &str, line: u32, col: u32) {
        self.current_file = Some(path.to_string());
        self.cursor = Some((line, col));
    }

    /// Update state from a buffer entered notification
    pub fn handle_buffer_entered(&mut self, path: &str, _filetype: Option<&str>) {
        self.current_file = Some(path.to_string());
    }

    /// Get diagnostics for a specific file
    pub fn diagnostics_for_file(&self, path: &str) -> Vec<&Diagnostic> {
        self.diagnostics.iter().filter(|d| d.path == path).collect()
    }

    /// Get error count
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    /// Get warning count
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count()
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_message_open_file_serialization() {
        let msg = RpcMessage::open_file("/path/to/file.rs", Some(42));
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"open_file\""));
        assert!(json.contains("\"path\":\"/path/to/file.rs\""));
        assert!(json.contains("\"line\":42"));

        let parsed = RpcMessage::from_json(&json).unwrap();
        match parsed {
            RpcMessage::OpenFile { path, line, col } => {
                assert_eq!(path, "/path/to/file.rs");
                assert_eq!(line, Some(42));
                assert_eq!(col, None);
            }
            _ => panic!("Expected OpenFile message"),
        }
    }

    #[test]
    fn test_rpc_message_apply_diff_serialization() {
        let msg =
            RpcMessage::apply_diff("/path/to/file.rs", "--- a\n+++ b\n@@ -1 +1 @@\n-old\n+new");
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"apply_diff\""));

        let parsed = RpcMessage::from_json(&json).unwrap();
        match parsed {
            RpcMessage::ApplyDiff { path, diff } => {
                assert_eq!(path, "/path/to/file.rs");
                assert!(diff.contains("-old"));
                assert!(diff.contains("+new"));
            }
            _ => panic!("Expected ApplyDiff message"),
        }
    }

    #[test]
    fn test_rpc_message_diagnostics_updated_serialization() {
        let msg = RpcMessage::DiagnosticsUpdated {
            diagnostics: vec![Diagnostic {
                path: "/path/to/file.rs".to_string(),
                line: 10,
                col: 5,
                end_line: Some(10),
                end_col: Some(15),
                severity: DiagnosticSeverity::Error,
                message: "expected `;`".to_string(),
                source: Some("rust-analyzer".to_string()),
                code: Some("E0001".to_string()),
            }],
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"diagnostics_updated\""));
        assert!(json.contains("\"severity\":\"error\""));

        let parsed = RpcMessage::from_json(&json).unwrap();
        match parsed {
            RpcMessage::DiagnosticsUpdated { diagnostics } => {
                assert_eq!(diagnostics.len(), 1);
                assert_eq!(diagnostics[0].line, 10);
                assert_eq!(diagnostics[0].severity, DiagnosticSeverity::Error);
            }
            _ => panic!("Expected DiagnosticsUpdated message"),
        }
    }

    #[test]
    fn test_rpc_message_response_serialization() {
        let msg = RpcMessage::Response {
            id: 123,
            result: serde_json::json!({"success": true}),
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"response\""));
        assert!(json.contains("\"id\":123"));

        let parsed = RpcMessage::from_json(&json).unwrap();
        match parsed {
            RpcMessage::Response { id, result } => {
                assert_eq!(id, 123);
                assert_eq!(result["success"], true);
            }
            _ => panic!("Expected Response message"),
        }
    }

    #[test]
    fn test_rpc_message_error_serialization() {
        let msg = RpcMessage::Error {
            id: Some(456),
            message: "File not found".to_string(),
            code: Some(-1),
        };
        let json = msg.to_json().unwrap();
        assert!(json.contains("\"type\":\"error\""));
        assert!(json.contains("\"message\":\"File not found\""));

        let parsed = RpcMessage::from_json(&json).unwrap();
        match parsed {
            RpcMessage::Error { id, message, code } => {
                assert_eq!(id, Some(456));
                assert_eq!(message, "File not found");
                assert_eq!(code, Some(-1));
            }
            _ => panic!("Expected Error message"),
        }
    }

    #[test]
    fn test_rpc_message_is_notification() {
        assert!(RpcMessage::BufferChanged {
            path: "test".to_string(),
            content: None,
            truncated: false
        }
        .is_notification());
        assert!(RpcMessage::DiagnosticsUpdated {
            diagnostics: vec![]
        }
        .is_notification());
        assert!(RpcMessage::CursorMoved {
            path: "test".to_string(),
            line: 1,
            col: 1
        }
        .is_notification());
        assert!(RpcMessage::EditorClosed.is_notification());

        assert!(!RpcMessage::OpenFile {
            path: "test".to_string(),
            line: None,
            col: None
        }
        .is_notification());
        assert!(!RpcMessage::GetBuffers.is_notification());
    }

    #[test]
    fn test_rpc_message_is_response() {
        assert!(RpcMessage::Response {
            id: 1,
            result: serde_json::Value::Null
        }
        .is_response());
        assert!(RpcMessage::Error {
            id: Some(1),
            message: "error".to_string(),
            code: None
        }
        .is_response());

        assert!(!RpcMessage::OpenFile {
            path: "test".to_string(),
            line: None,
            col: None
        }
        .is_response());
        assert!(!RpcMessage::BufferChanged {
            path: "test".to_string(),
            content: None,
            truncated: false
        }
        .is_response());
    }

    #[test]
    fn test_editor_state_handle_buffer_changed() {
        let mut state = EditorState::new();
        assert!(state.modified_files.is_empty());

        state.handle_buffer_changed("/path/to/file.rs", Some("content"));
        assert_eq!(state.modified_files.len(), 1);
        assert_eq!(state.modified_files[0], "/path/to/file.rs");

        // Adding same file again shouldn't duplicate
        state.handle_buffer_changed("/path/to/file.rs", Some("new content"));
        assert_eq!(state.modified_files.len(), 1);
    }

    #[test]
    fn test_editor_state_handle_cursor_moved() {
        let mut state = EditorState::new();
        assert!(state.current_file.is_none());
        assert!(state.cursor.is_none());

        state.handle_cursor_moved("/path/to/file.rs", 10, 5);
        assert_eq!(state.current_file, Some("/path/to/file.rs".to_string()));
        assert_eq!(state.cursor, Some((10, 5)));
    }

    #[test]
    fn test_editor_state_diagnostics_counts() {
        let mut state = EditorState::new();
        state.handle_diagnostics_updated(vec![
            Diagnostic {
                path: "/file.rs".to_string(),
                line: 1,
                col: 1,
                end_line: None,
                end_col: None,
                severity: DiagnosticSeverity::Error,
                message: "error 1".to_string(),
                source: None,
                code: None,
            },
            Diagnostic {
                path: "/file.rs".to_string(),
                line: 2,
                col: 1,
                end_line: None,
                end_col: None,
                severity: DiagnosticSeverity::Error,
                message: "error 2".to_string(),
                source: None,
                code: None,
            },
            Diagnostic {
                path: "/file.rs".to_string(),
                line: 3,
                col: 1,
                end_line: None,
                end_col: None,
                severity: DiagnosticSeverity::Warning,
                message: "warning 1".to_string(),
                source: None,
                code: None,
            },
        ]);

        assert_eq!(state.error_count(), 2);
        assert_eq!(state.warning_count(), 1);
    }

    #[test]
    fn test_diagnostic_severity_default() {
        let severity: DiagnosticSeverity = Default::default();
        assert_eq!(severity, DiagnosticSeverity::Error);
    }
}

// ============================================================================
// Editor Bridge (Unix Socket Client)
// ============================================================================

/// Error type for editor bridge operations
#[derive(Debug, thiserror::Error)]
pub enum EditorBridgeError {
    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Socket not connected")]
    NotConnected,

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Request timeout")]
    Timeout,

    #[error("Request failed: {0}")]
    RequestFailed(String),

    #[error("Channel closed")]
    ChannelClosed,

    #[error("Unix sockets not supported on this platform")]
    NotSupported,
}

/// Result type for editor bridge operations
pub type EditorBridgeResult<T> = Result<T, EditorBridgeError>;

/// Type alias for pending requests map to reduce complexity
type PendingRequestsMap = HashMap<u64, oneshot::Sender<EditorBridgeResult<RpcMessage>>>;

/// Configuration for the editor bridge
#[derive(Debug, Clone)]
pub struct EditorBridgeConfig {
    /// Socket path
    pub socket_path: PathBuf,
    /// Request timeout in milliseconds
    pub request_timeout_ms: u64,
    /// Reconnect attempts on disconnect
    pub reconnect_attempts: u32,
    /// Delay between reconnect attempts in milliseconds
    pub reconnect_delay_ms: u64,
}

impl Default for EditorBridgeConfig {
    fn default() -> Self {
        Self {
            socket_path: PathBuf::from("/tmp/tark-nvim.sock"),
            request_timeout_ms: 5000,
            reconnect_attempts: 3,
            reconnect_delay_ms: 1000,
        }
    }
}

/// Editor bridge for communicating with Neovim via Unix socket
///
/// The bridge handles:
/// - Connecting to the Neovim socket server
/// - Sending commands and receiving responses
/// - Receiving notifications from Neovim
/// - Graceful disconnection handling
///
/// Note: On Windows, this struct provides stub implementations since Unix sockets
/// are not available. All connection attempts will fail with NotSupported error.
pub struct EditorBridge {
    /// Configuration
    config: EditorBridgeConfig,
    /// Socket connection (wrapped in Arc<Mutex> for async access)
    #[cfg(unix)]
    socket: Arc<Mutex<Option<UnixStream>>>,
    #[cfg(not(unix))]
    socket: Arc<Mutex<Option<()>>>,
    /// Request ID counter
    request_id: AtomicU64,
    /// Pending requests waiting for responses
    pending_requests: Arc<Mutex<PendingRequestsMap>>,
    /// Channel for sending notifications to the TUI
    notification_tx: mpsc::Sender<RpcMessage>,
    /// Channel for receiving notifications in the TUI
    notification_rx: Arc<Mutex<mpsc::Receiver<RpcMessage>>>,
    /// Current editor state
    state: Arc<Mutex<EditorState>>,
    /// Whether the bridge is connected
    connected: Arc<std::sync::atomic::AtomicBool>,
}

impl EditorBridge {
    /// Create a new editor bridge with the given configuration
    pub fn new(config: EditorBridgeConfig) -> Self {
        let (notification_tx, notification_rx) = mpsc::channel(100);

        Self {
            config,
            socket: Arc::new(Mutex::new(None)),
            request_id: AtomicU64::new(1),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            notification_tx,
            notification_rx: Arc::new(Mutex::new(notification_rx)),
            state: Arc::new(Mutex::new(EditorState::new())),
            connected: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    /// Create a new editor bridge with default configuration
    pub fn with_socket_path(socket_path: impl Into<PathBuf>) -> Self {
        let config = EditorBridgeConfig {
            socket_path: socket_path.into(),
            ..Default::default()
        };
        Self::new(config)
    }

    /// Check if the bridge is connected
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Get the current editor state
    pub async fn state(&self) -> EditorState {
        self.state.lock().await.clone()
    }

    /// Connect to the Neovim socket
    #[cfg(unix)]
    pub async fn connect(&self) -> EditorBridgeResult<()> {
        let socket_path = &self.config.socket_path;

        // Check if socket file exists
        if !socket_path.exists() {
            return Err(EditorBridgeError::ConnectionFailed(format!(
                "Socket file does not exist: {}",
                socket_path.display()
            )));
        }

        // Connect to the socket
        let stream = UnixStream::connect(socket_path).await.map_err(|e| {
            EditorBridgeError::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                socket_path.display(),
                e
            ))
        })?;

        // Store the socket
        {
            let mut socket_guard = self.socket.lock().await;
            *socket_guard = Some(stream);
        }

        self.connected.store(true, Ordering::SeqCst);

        // Update state
        {
            let mut state = self.state.lock().await;
            state.connected = true;
        }

        // Start the reader task
        self.start_reader_task().await;

        Ok(())
    }

    /// Connect to the Neovim socket (stub for non-Unix platforms)
    #[cfg(not(unix))]
    pub async fn connect(&self) -> EditorBridgeResult<()> {
        Err(EditorBridgeError::NotSupported)
    }

    /// Disconnect from the Neovim socket
    pub async fn disconnect(&self) {
        self.connected.store(false, Ordering::SeqCst);

        // Close the socket
        {
            let mut socket_guard = self.socket.lock().await;
            *socket_guard = None;
        }

        // Update state
        {
            let mut state = self.state.lock().await;
            state.connected = false;
        }

        // Clear pending requests
        {
            let mut pending = self.pending_requests.lock().await;
            for (_, sender) in pending.drain() {
                let _ = sender.send(Err(EditorBridgeError::NotConnected));
            }
        }
    }

    /// Start the background reader task
    #[cfg(unix)]
    async fn start_reader_task(&self) {
        let socket = self.socket.clone();
        let pending_requests = self.pending_requests.clone();
        let notification_tx = self.notification_tx.clone();
        let state = self.state.clone();
        let connected = self.connected.clone();

        tokio::spawn(async move {
            let mut buffer = String::new();

            loop {
                if !connected.load(Ordering::SeqCst) {
                    break;
                }

                // Read from socket
                let read_result = {
                    let mut socket_guard = socket.lock().await;
                    if let Some(ref mut stream) = *socket_guard {
                        let mut reader = BufReader::new(stream);
                        reader.read_line(&mut buffer).await
                    } else {
                        break;
                    }
                };

                match read_result {
                    Ok(0) => {
                        // Connection closed
                        connected.store(false, Ordering::SeqCst);
                        let mut state_guard = state.lock().await;
                        state_guard.connected = false;
                        break;
                    }
                    Ok(_) => {
                        // Parse the message
                        let line = buffer.trim();
                        if !line.is_empty() {
                            if let Ok(msg) = RpcMessage::from_json(line) {
                                Self::handle_incoming_message(
                                    msg,
                                    &pending_requests,
                                    &notification_tx,
                                    &state,
                                )
                                .await;
                            }
                        }
                        buffer.clear();
                    }
                    Err(_) => {
                        // Read error, disconnect
                        connected.store(false, Ordering::SeqCst);
                        let mut state_guard = state.lock().await;
                        state_guard.connected = false;
                        break;
                    }
                }
            }
        });
    }

    /// Start the background reader task (stub for non-Unix platforms)
    #[cfg(not(unix))]
    async fn start_reader_task(&self) {
        // No-op on non-Unix platforms
    }

    /// Handle an incoming message from the socket
    async fn handle_incoming_message(
        msg: RpcMessage,
        pending_requests: &Arc<Mutex<PendingRequestsMap>>,
        notification_tx: &mpsc::Sender<RpcMessage>,
        state: &Arc<Mutex<EditorState>>,
    ) {
        match &msg {
            // Handle responses to pending requests
            RpcMessage::Response { id, .. } | RpcMessage::Error { id: Some(id), .. } => {
                let mut pending = pending_requests.lock().await;
                if let Some(sender) = pending.remove(id) {
                    let _ = sender.send(Ok(msg));
                }
            }

            // Handle notifications and update state
            RpcMessage::BufferChanged {
                path,
                content,
                truncated: _,
            } => {
                let mut state_guard = state.lock().await;
                state_guard.handle_buffer_changed(path, content.as_deref());
                let _ = notification_tx.send(msg).await;
            }

            RpcMessage::DiagnosticsUpdated { diagnostics } => {
                let mut state_guard = state.lock().await;
                state_guard.handle_diagnostics_updated(diagnostics.clone());
                let _ = notification_tx.send(msg).await;
            }

            RpcMessage::CursorMoved { path, line, col } => {
                let mut state_guard = state.lock().await;
                state_guard.handle_cursor_moved(path, *line, *col);
                let _ = notification_tx.send(msg).await;
            }

            RpcMessage::BufferEntered { path, filetype } => {
                let mut state_guard = state.lock().await;
                state_guard.handle_buffer_entered(path, filetype.as_deref());
                let _ = notification_tx.send(msg).await;
            }

            RpcMessage::EditorClosed => {
                let mut state_guard = state.lock().await;
                state_guard.connected = false;
                let _ = notification_tx.send(msg).await;
            }

            // Forward other notifications
            _ => {
                let _ = notification_tx.send(msg).await;
            }
        }
    }

    /// Send a message to Neovim (fire and forget)
    #[cfg(unix)]
    pub async fn send(&self, msg: &RpcMessage) -> EditorBridgeResult<()> {
        if !self.is_connected() {
            return Err(EditorBridgeError::NotConnected);
        }

        let json = msg.to_json()?;
        let line = format!("{}\n", json);

        let mut socket_guard = self.socket.lock().await;
        if let Some(ref mut stream) = *socket_guard {
            stream
                .write_all(line.as_bytes())
                .await
                .map_err(|e| EditorBridgeError::SendFailed(e.to_string()))?;
            stream
                .flush()
                .await
                .map_err(|e| EditorBridgeError::SendFailed(e.to_string()))?;
            Ok(())
        } else {
            Err(EditorBridgeError::NotConnected)
        }
    }

    /// Send a message to Neovim (stub for non-Unix platforms)
    #[cfg(not(unix))]
    pub async fn send(&self, _msg: &RpcMessage) -> EditorBridgeResult<()> {
        Err(EditorBridgeError::NotSupported)
    }

    /// Send a request and wait for a response
    pub async fn request(&self, msg: RpcMessage) -> EditorBridgeResult<RpcMessage> {
        if !self.is_connected() {
            return Err(EditorBridgeError::NotConnected);
        }

        // Generate request ID
        let id = self.request_id.fetch_add(1, Ordering::SeqCst);

        // Create response channel
        let (tx, rx) = oneshot::channel();

        // Register pending request
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(id, tx);
        }

        // Wrap the message in a request
        let request = RpcMessage::Request {
            id,
            command: Box::new(msg),
        };

        // Send the request
        self.send(&request).await?;

        // Wait for response with timeout
        let timeout = tokio::time::Duration::from_millis(self.config.request_timeout_ms);
        match tokio::time::timeout(timeout, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => Err(EditorBridgeError::ChannelClosed),
            Err(_) => {
                // Remove from pending on timeout
                let mut pending = self.pending_requests.lock().await;
                pending.remove(&id);
                Err(EditorBridgeError::Timeout)
            }
        }
    }

    /// Try to receive a notification (non-blocking)
    pub async fn try_recv_notification(&self) -> Option<RpcMessage> {
        let mut rx = self.notification_rx.lock().await;
        rx.try_recv().ok()
    }

    /// Receive a notification (blocking)
    pub async fn recv_notification(&self) -> Option<RpcMessage> {
        let mut rx = self.notification_rx.lock().await;
        rx.recv().await
    }

    /// Get the next request ID (for testing)
    #[cfg(test)]
    pub fn next_request_id(&self) -> u64 {
        self.request_id.load(Ordering::SeqCst)
    }
}

// ============================================================================
// Editor Bridge Tests
// ============================================================================

#[cfg(test)]
mod bridge_tests {
    use super::*;

    #[test]
    fn test_editor_bridge_config_default() {
        let config = EditorBridgeConfig::default();
        assert_eq!(config.socket_path, PathBuf::from("/tmp/tark-nvim.sock"));
        assert_eq!(config.request_timeout_ms, 5000);
        assert_eq!(config.reconnect_attempts, 3);
        assert_eq!(config.reconnect_delay_ms, 1000);
    }

    #[test]
    fn test_editor_bridge_new() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        assert!(!bridge.is_connected());
        assert_eq!(bridge.next_request_id(), 1);
    }

    #[tokio::test]
    async fn test_editor_bridge_connect_nonexistent_socket() {
        let bridge = EditorBridge::with_socket_path("/tmp/nonexistent-socket-12345.sock");
        let result = bridge.connect().await;
        assert!(result.is_err());
        match result {
            Err(EditorBridgeError::ConnectionFailed(msg)) => {
                assert!(msg.contains("does not exist"));
            }
            _ => panic!("Expected ConnectionFailed error"),
        }
    }

    #[tokio::test]
    async fn test_editor_bridge_send_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let msg = RpcMessage::Ping { seq: Some(1) };
        let result = bridge.send(&msg).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_editor_bridge_request_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let msg = RpcMessage::GetBuffers;
        let result = bridge.request(msg).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_editor_bridge_state_initial() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let state = bridge.state().await;
        assert!(!state.connected);
        assert!(state.buffers.is_empty());
        assert!(state.current_file.is_none());
        assert!(state.diagnostics.is_empty());
    }

    #[test]
    fn test_editor_bridge_error_display() {
        let err = EditorBridgeError::NotConnected;
        assert_eq!(err.to_string(), "Socket not connected");

        let err = EditorBridgeError::Timeout;
        assert_eq!(err.to_string(), "Request timeout");

        let err = EditorBridgeError::ConnectionFailed("test".to_string());
        assert_eq!(err.to_string(), "Connection failed: test");
    }
}

// ============================================================================
// Editor Commands (High-Level API)
// ============================================================================

impl EditorBridge {
    /// Open a file in the editor
    ///
    /// # Arguments
    /// * `path` - Path to the file to open
    /// * `line` - Optional line number to jump to (1-indexed)
    ///
    /// # Returns
    /// * `Ok(())` if the file was opened successfully
    /// * `Err(EditorBridgeError)` if the operation failed
    pub async fn open_file(
        &self,
        path: impl Into<String>,
        line: Option<u32>,
    ) -> EditorBridgeResult<()> {
        let msg = RpcMessage::open_file(path, line);
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                // Check if the response indicates success
                if let Ok(success) = serde_json::from_value::<SuccessResponse>(result) {
                    if success.success {
                        Ok(())
                    } else {
                        Err(EditorBridgeError::RequestFailed(
                            success
                                .message
                                .unwrap_or_else(|| "Unknown error".to_string()),
                        ))
                    }
                } else {
                    // Assume success if we got a response
                    Ok(())
                }
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Go to a specific line in the current buffer
    ///
    /// # Arguments
    /// * `line` - Line number to jump to (1-indexed)
    /// * `col` - Optional column number (1-indexed)
    pub async fn goto_line(&self, line: u32, col: Option<u32>) -> EditorBridgeResult<()> {
        let msg = RpcMessage::goto_line(line, col);
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { .. } => Ok(()),
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Apply a diff/patch to a file
    ///
    /// # Arguments
    /// * `path` - Path to the file to apply the diff to
    /// * `diff` - Unified diff content
    ///
    /// # Returns
    /// * `Ok(())` if the diff was applied successfully
    /// * `Err(EditorBridgeError)` if the operation failed
    pub async fn apply_diff(
        &self,
        path: impl Into<String>,
        diff: impl Into<String>,
    ) -> EditorBridgeResult<()> {
        let msg = RpcMessage::apply_diff(path, diff);
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                if let Ok(success) = serde_json::from_value::<SuccessResponse>(result) {
                    if success.success {
                        Ok(())
                    } else {
                        Err(EditorBridgeError::RequestFailed(
                            success
                                .message
                                .unwrap_or_else(|| "Failed to apply diff".to_string()),
                        ))
                    }
                } else {
                    Ok(())
                }
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Show a side-by-side diff view for a file
    ///
    /// # Arguments
    /// * `path` - Path to the file to show diff for
    /// * `original` - Optional original content (if not from disk)
    /// * `modified` - Optional modified content
    pub async fn show_diff(
        &self,
        path: impl Into<String>,
        original: Option<String>,
        modified: Option<String>,
    ) -> EditorBridgeResult<()> {
        let msg = RpcMessage::ShowDiff {
            path: path.into(),
            original,
            modified,
        };
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { .. } => Ok(()),
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Get list of open buffers from the editor
    pub async fn get_buffers(&self) -> EditorBridgeResult<Vec<BufferInfo>> {
        let msg = RpcMessage::get_buffers();
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                let buffers_response: BuffersResponse = serde_json::from_value(result)
                    .map_err(|e| EditorBridgeError::RequestFailed(e.to_string()))?;
                Ok(buffers_response.buffers)
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Get diagnostics from the editor
    ///
    /// # Arguments
    /// * `path` - Optional path to filter diagnostics by file
    pub async fn get_diagnostics(
        &self,
        path: Option<String>,
    ) -> EditorBridgeResult<Vec<Diagnostic>> {
        let msg = RpcMessage::get_diagnostics(path);
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                let diagnostics_response: DiagnosticsResponse = serde_json::from_value(result)
                    .map_err(|e| EditorBridgeError::RequestFailed(e.to_string()))?;
                Ok(diagnostics_response.diagnostics)
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Get content of a buffer
    ///
    /// # Arguments
    /// * `path` - Path to the file/buffer
    pub async fn get_buffer_content(
        &self,
        path: impl Into<String>,
    ) -> EditorBridgeResult<BufferContentResponse> {
        let msg = RpcMessage::GetBufferContent { path: path.into() };
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                let content_response: BufferContentResponse = serde_json::from_value(result)
                    .map_err(|e| EditorBridgeError::RequestFailed(e.to_string()))?;
                Ok(content_response)
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Get current cursor position
    pub async fn get_cursor(&self) -> EditorBridgeResult<CursorResponse> {
        let msg = RpcMessage::GetCursor;
        let response = self.request(msg).await?;

        match response {
            RpcMessage::Response { result, .. } => {
                let cursor_response: CursorResponse = serde_json::from_value(result)
                    .map_err(|e| EditorBridgeError::RequestFailed(e.to_string()))?;
                Ok(cursor_response)
            }
            RpcMessage::Error { message, .. } => Err(EditorBridgeError::RequestFailed(message)),
            _ => Err(EditorBridgeError::RequestFailed(
                "Unexpected response type".to_string(),
            )),
        }
    }

    /// Send a ping to check connection
    pub async fn ping(&self) -> EditorBridgeResult<()> {
        let seq = self.request_id.load(Ordering::SeqCst);
        let msg = RpcMessage::Ping { seq: Some(seq) };
        self.send(&msg).await
    }
}

// ============================================================================
// Editor Command Tests
// ============================================================================

#[cfg(test)]
mod command_tests {
    use super::*;

    #[tokio::test]
    async fn test_open_file_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.open_file("/path/to/file.rs", Some(10)).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_goto_line_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.goto_line(10, Some(5)).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_apply_diff_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.apply_diff("/path/to/file.rs", "diff content").await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_show_diff_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.show_diff("/path/to/file.rs", None, None).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_get_buffers_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.get_buffers().await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_get_diagnostics_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.get_diagnostics(None).await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_get_buffer_content_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.get_buffer_content("/path/to/file.rs").await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_get_cursor_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.get_cursor().await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }

    #[tokio::test]
    async fn test_ping_not_connected() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let result = bridge.ping().await;
        assert!(matches!(result, Err(EditorBridgeError::NotConnected)));
    }
}

// ============================================================================
// Context Receiver (Notification Handling)
// ============================================================================

/// Events that can be received from the editor
#[derive(Debug, Clone)]
pub enum EditorEvent {
    /// Buffer content changed
    BufferChanged {
        path: String,
        content: Option<String>,
        truncated: bool,
    },
    /// Diagnostics updated
    DiagnosticsUpdated { diagnostics: Vec<Diagnostic> },
    /// Cursor moved
    CursorMoved { path: String, line: u32, col: u32 },
    /// Buffer entered (switched to)
    BufferEntered {
        path: String,
        filetype: Option<String>,
    },
    /// Editor is closing
    EditorClosed,
    /// Connection lost
    Disconnected,
    /// Pong response received
    Pong { seq: Option<u64> },
}

impl From<RpcMessage> for Option<EditorEvent> {
    fn from(msg: RpcMessage) -> Self {
        match msg {
            RpcMessage::BufferChanged {
                path,
                content,
                truncated,
            } => Some(EditorEvent::BufferChanged {
                path,
                content,
                truncated,
            }),
            RpcMessage::DiagnosticsUpdated { diagnostics } => {
                Some(EditorEvent::DiagnosticsUpdated { diagnostics })
            }
            RpcMessage::CursorMoved { path, line, col } => {
                Some(EditorEvent::CursorMoved { path, line, col })
            }
            RpcMessage::BufferEntered { path, filetype } => {
                Some(EditorEvent::BufferEntered { path, filetype })
            }
            RpcMessage::EditorClosed => Some(EditorEvent::EditorClosed),
            RpcMessage::Pong { seq } => Some(EditorEvent::Pong { seq }),
            _ => None,
        }
    }
}

/// Context receiver for handling editor notifications
///
/// This provides a higher-level interface for receiving and processing
/// editor events in the TUI.
pub struct ContextReceiver {
    /// Reference to the editor bridge
    bridge: Arc<EditorBridge>,
}

impl ContextReceiver {
    /// Create a new context receiver
    pub fn new(bridge: Arc<EditorBridge>) -> Self {
        Self { bridge }
    }

    /// Try to receive an editor event (non-blocking)
    pub async fn try_recv(&self) -> Option<EditorEvent> {
        self.bridge
            .try_recv_notification()
            .await
            .and_then(|msg| msg.into())
    }

    /// Receive an editor event (blocking)
    pub async fn recv(&self) -> Option<EditorEvent> {
        loop {
            if let Some(msg) = self.bridge.recv_notification().await {
                if let Some(event) = msg.into() {
                    return Some(event);
                }
            } else {
                // Channel closed, editor disconnected
                return Some(EditorEvent::Disconnected);
            }
        }
    }

    /// Get the current editor state
    pub async fn state(&self) -> EditorState {
        self.bridge.state().await
    }

    /// Check if connected to editor
    pub fn is_connected(&self) -> bool {
        self.bridge.is_connected()
    }
}

impl EditorBridge {
    /// Create a context receiver for this bridge
    pub fn context_receiver(self: &Arc<Self>) -> ContextReceiver {
        ContextReceiver::new(Arc::clone(self))
    }

    /// Process a notification and update internal state
    ///
    /// This is called automatically by the reader task, but can also be
    /// called manually for testing or custom notification handling.
    pub async fn process_notification(&self, msg: &RpcMessage) {
        let mut state = self.state.lock().await;

        match msg {
            RpcMessage::BufferChanged {
                path,
                content,
                truncated: _,
            } => {
                state.handle_buffer_changed(path, content.as_deref());
            }
            RpcMessage::DiagnosticsUpdated { diagnostics } => {
                state.handle_diagnostics_updated(diagnostics.clone());
            }
            RpcMessage::CursorMoved { path, line, col } => {
                state.handle_cursor_moved(path, *line, *col);
            }
            RpcMessage::BufferEntered { path, filetype } => {
                state.handle_buffer_entered(path, filetype.as_deref());
            }
            RpcMessage::EditorClosed => {
                state.connected = false;
            }
            _ => {}
        }
    }

    /// Get current file path from editor state
    pub async fn current_file(&self) -> Option<String> {
        self.state.lock().await.current_file.clone()
    }

    /// Get current cursor position from editor state
    pub async fn cursor_position(&self) -> Option<(u32, u32)> {
        self.state.lock().await.cursor
    }

    /// Get diagnostics for a specific file from cached state
    pub async fn cached_diagnostics_for_file(&self, path: &str) -> Vec<Diagnostic> {
        self.state
            .lock()
            .await
            .diagnostics
            .iter()
            .filter(|d| d.path == path)
            .cloned()
            .collect()
    }

    /// Get all cached diagnostics
    pub async fn cached_diagnostics(&self) -> Vec<Diagnostic> {
        self.state.lock().await.diagnostics.clone()
    }

    /// Get list of modified files in this session
    pub async fn modified_files(&self) -> Vec<String> {
        self.state.lock().await.modified_files.clone()
    }

    /// Get error count from cached diagnostics
    pub async fn error_count(&self) -> usize {
        self.state.lock().await.error_count()
    }

    /// Get warning count from cached diagnostics
    pub async fn warning_count(&self) -> usize {
        self.state.lock().await.warning_count()
    }
}

// ============================================================================
// Context Receiver Tests
// ============================================================================

#[cfg(test)]
mod context_tests {
    use super::*;

    #[test]
    fn test_editor_event_from_buffer_changed() {
        let msg = RpcMessage::BufferChanged {
            path: "/path/to/file.rs".to_string(),
            content: Some("content".to_string()),
            truncated: false,
        };
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(
            event,
            Some(EditorEvent::BufferChanged {
                path,
                content: Some(_),
                truncated: false
            }) if path == "/path/to/file.rs"
        ));
    }

    #[test]
    fn test_editor_event_from_diagnostics_updated() {
        let msg = RpcMessage::DiagnosticsUpdated {
            diagnostics: vec![Diagnostic {
                path: "/file.rs".to_string(),
                line: 1,
                col: 1,
                end_line: None,
                end_col: None,
                severity: DiagnosticSeverity::Error,
                message: "error".to_string(),
                source: None,
                code: None,
            }],
        };
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(
            event,
            Some(EditorEvent::DiagnosticsUpdated { diagnostics }) if diagnostics.len() == 1
        ));
    }

    #[test]
    fn test_editor_event_from_cursor_moved() {
        let msg = RpcMessage::CursorMoved {
            path: "/path/to/file.rs".to_string(),
            line: 10,
            col: 5,
        };
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(
            event,
            Some(EditorEvent::CursorMoved { path, line: 10, col: 5 }) if path == "/path/to/file.rs"
        ));
    }

    #[test]
    fn test_editor_event_from_buffer_entered() {
        let msg = RpcMessage::BufferEntered {
            path: "/path/to/file.rs".to_string(),
            filetype: Some("rust".to_string()),
        };
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(
            event,
            Some(EditorEvent::BufferEntered { path, filetype: Some(ft) })
                if path == "/path/to/file.rs" && ft == "rust"
        ));
    }

    #[test]
    fn test_editor_event_from_editor_closed() {
        let msg = RpcMessage::EditorClosed;
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(event, Some(EditorEvent::EditorClosed)));
    }

    #[test]
    fn test_editor_event_from_pong() {
        let msg = RpcMessage::Pong { seq: Some(42) };
        let event: Option<EditorEvent> = msg.into();
        assert!(matches!(event, Some(EditorEvent::Pong { seq: Some(42) })));
    }

    #[test]
    fn test_editor_event_from_non_notification() {
        let msg = RpcMessage::OpenFile {
            path: "/path".to_string(),
            line: None,
            col: None,
        };
        let event: Option<EditorEvent> = msg.into();
        assert!(event.is_none());
    }

    #[tokio::test]
    async fn test_context_receiver_not_connected() {
        let bridge = Arc::new(EditorBridge::with_socket_path("/tmp/test.sock"));
        let receiver = bridge.context_receiver();
        assert!(!receiver.is_connected());
    }

    #[tokio::test]
    async fn test_editor_bridge_current_file_initial() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let current = bridge.current_file().await;
        assert!(current.is_none());
    }

    #[tokio::test]
    async fn test_editor_bridge_cursor_position_initial() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let cursor = bridge.cursor_position().await;
        assert!(cursor.is_none());
    }

    #[tokio::test]
    async fn test_editor_bridge_cached_diagnostics_initial() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let diagnostics = bridge.cached_diagnostics().await;
        assert!(diagnostics.is_empty());
    }

    #[tokio::test]
    async fn test_editor_bridge_modified_files_initial() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let files = bridge.modified_files().await;
        assert!(files.is_empty());
    }

    #[tokio::test]
    async fn test_editor_bridge_process_notification_buffer_changed() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let msg = RpcMessage::BufferChanged {
            path: "/path/to/file.rs".to_string(),
            content: Some("content".to_string()),
            truncated: false,
        };
        bridge.process_notification(&msg).await;

        let files = bridge.modified_files().await;
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "/path/to/file.rs");
    }

    #[tokio::test]
    async fn test_editor_bridge_process_notification_cursor_moved() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let msg = RpcMessage::CursorMoved {
            path: "/path/to/file.rs".to_string(),
            line: 10,
            col: 5,
        };
        bridge.process_notification(&msg).await;

        let current = bridge.current_file().await;
        assert_eq!(current, Some("/path/to/file.rs".to_string()));

        let cursor = bridge.cursor_position().await;
        assert_eq!(cursor, Some((10, 5)));
    }

    #[tokio::test]
    async fn test_editor_bridge_process_notification_diagnostics() {
        let bridge = EditorBridge::with_socket_path("/tmp/test.sock");
        let msg = RpcMessage::DiagnosticsUpdated {
            diagnostics: vec![
                Diagnostic {
                    path: "/file.rs".to_string(),
                    line: 1,
                    col: 1,
                    end_line: None,
                    end_col: None,
                    severity: DiagnosticSeverity::Error,
                    message: "error".to_string(),
                    source: None,
                    code: None,
                },
                Diagnostic {
                    path: "/file.rs".to_string(),
                    line: 2,
                    col: 1,
                    end_line: None,
                    end_col: None,
                    severity: DiagnosticSeverity::Warning,
                    message: "warning".to_string(),
                    source: None,
                    code: None,
                },
            ],
        };
        bridge.process_notification(&msg).await;

        assert_eq!(bridge.error_count().await, 1);
        assert_eq!(bridge.warning_count().await, 1);

        let diagnostics = bridge.cached_diagnostics_for_file("/file.rs").await;
        assert_eq!(diagnostics.len(), 2);
    }
}

// ============================================================================
// Property-Based Tests for RPC Round-Trip
// ============================================================================

/// Property-based tests for RPC message round-trip
///
/// **Property 2: RPC Message Round-Trip**
/// **Validates: Requirements 2.1, 2.3, 2.4, 2.5, 2.6**
#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    /// Generate a random file path
    fn arb_path() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("/path/to/file.rs".to_string()),
            Just("/home/user/project/src/main.rs".to_string()),
            Just("./relative/path.py".to_string()),
            Just("file.txt".to_string()),
            "[a-zA-Z0-9_/\\.]{1,100}".prop_map(|s| format!("/{}", s)),
        ]
    }

    /// Generate a random line number
    fn arb_line() -> impl Strategy<Value = u32> {
        1u32..10000u32
    }

    /// Generate a random column number
    fn arb_col() -> impl Strategy<Value = u32> {
        1u32..1000u32
    }

    /// Generate a random diff content
    fn arb_diff() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new".to_string()),
            Just("--- a\n+++ b\n@@ -1,3 +1,3 @@\n line1\n-line2\n+modified\n line3".to_string()),
            "[a-zA-Z0-9\\-\\+@ \n]{1,500}",
        ]
    }

    /// Generate a random diagnostic severity
    fn arb_severity() -> impl Strategy<Value = DiagnosticSeverity> {
        prop_oneof![
            Just(DiagnosticSeverity::Error),
            Just(DiagnosticSeverity::Warning),
            Just(DiagnosticSeverity::Info),
            Just(DiagnosticSeverity::Hint),
        ]
    }

    /// Generate a random diagnostic message
    fn arb_diagnostic_message() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("expected `;`".to_string()),
            Just("unused variable".to_string()),
            Just("type mismatch".to_string()),
            "[a-zA-Z0-9 .,!?:;'\"\\-_]{1,200}",
        ]
    }

    /// Generate a random diagnostic
    fn arb_diagnostic() -> impl Strategy<Value = Diagnostic> {
        (
            arb_path(),
            arb_line(),
            arb_col(),
            prop::option::of(arb_line()),
            prop::option::of(arb_col()),
            arb_severity(),
            arb_diagnostic_message(),
            prop::option::of("[a-zA-Z\\-]{1,30}"),
            prop::option::of("[A-Z0-9]{1,10}"),
        )
            .prop_map(
                |(path, line, col, end_line, end_col, severity, message, source, code)| {
                    Diagnostic {
                        path,
                        line,
                        col,
                        end_line,
                        end_col,
                        severity,
                        message,
                        source,
                        code,
                    }
                },
            )
    }

    /// Generate a random buffer content
    fn arb_content() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("fn main() {\n    println!(\"Hello\");\n}".to_string()),
            Just("".to_string()),
            "[a-zA-Z0-9 \n\t{}();:,.<>\\-_=+*/%!&|^~@#$]{0,1000}",
        ]
    }

    /// Generate a random filetype
    fn arb_filetype() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("rust".to_string()),
            Just("python".to_string()),
            Just("typescript".to_string()),
            Just("javascript".to_string()),
            Just("go".to_string()),
            Just("lua".to_string()),
            "[a-z]{1,20}",
        ]
    }

    /// Generate a random request ID
    fn arb_request_id() -> impl Strategy<Value = u64> {
        1u64..u64::MAX
    }

    /// Generate a random JSON value for response results
    fn arb_json_value() -> impl Strategy<Value = serde_json::Value> {
        prop_oneof![
            Just(serde_json::json!({"success": true})),
            Just(serde_json::json!({"success": false, "message": "error"})),
            Just(serde_json::json!(null)),
            Just(serde_json::json!(42)),
            Just(serde_json::json!("string value")),
            Just(serde_json::json!([])),
            Just(serde_json::json!({})),
        ]
    }

    /// Generate a random RPC message
    fn arb_rpc_message() -> impl Strategy<Value = RpcMessage> {
        prop_oneof![
            // OpenFile
            (
                arb_path(),
                prop::option::of(arb_line()),
                prop::option::of(arb_col())
            )
                .prop_map(|(path, line, col)| RpcMessage::OpenFile { path, line, col }),
            // GotoLine
            (arb_line(), prop::option::of(arb_col()))
                .prop_map(|(line, col)| RpcMessage::GotoLine { line, col }),
            // ApplyDiff
            (arb_path(), arb_diff()).prop_map(|(path, diff)| RpcMessage::ApplyDiff { path, diff }),
            // ShowDiff
            (
                arb_path(),
                prop::option::of(arb_content()),
                prop::option::of(arb_content())
            )
                .prop_map(|(path, original, modified)| RpcMessage::ShowDiff {
                    path,
                    original,
                    modified
                }),
            // GetBuffers
            Just(RpcMessage::GetBuffers),
            // GetDiagnostics
            prop::option::of(arb_path()).prop_map(|path| RpcMessage::GetDiagnostics { path }),
            // GetBufferContent
            arb_path().prop_map(|path| RpcMessage::GetBufferContent { path }),
            // GetCursor
            Just(RpcMessage::GetCursor),
            // BufferChanged
            (arb_path(), prop::option::of(arb_content()), any::<bool>()).prop_map(
                |(path, content, truncated)| RpcMessage::BufferChanged {
                    path,
                    content,
                    truncated
                }
            ),
            // DiagnosticsUpdated
            prop::collection::vec(arb_diagnostic(), 0..5)
                .prop_map(|diagnostics| RpcMessage::DiagnosticsUpdated { diagnostics }),
            // CursorMoved
            (arb_path(), arb_line(), arb_col())
                .prop_map(|(path, line, col)| RpcMessage::CursorMoved { path, line, col }),
            // BufferEntered
            (arb_path(), prop::option::of(arb_filetype()))
                .prop_map(|(path, filetype)| RpcMessage::BufferEntered { path, filetype }),
            // EditorClosed
            Just(RpcMessage::EditorClosed),
            // Response
            (arb_request_id(), arb_json_value())
                .prop_map(|(id, result)| RpcMessage::Response { id, result }),
            // Error
            (
                prop::option::of(arb_request_id()),
                arb_diagnostic_message(),
                prop::option::of(-100i32..100i32)
            )
                .prop_map(|(id, message, code)| RpcMessage::Error {
                    id,
                    message,
                    code
                }),
            // Ping
            prop::option::of(arb_request_id()).prop_map(|seq| RpcMessage::Ping { seq }),
            // Pong
            prop::option::of(arb_request_id()).prop_map(|seq| RpcMessage::Pong { seq }),
        ]
    }

    proptest! {
        /// **Feature: terminal-tui-chat, Property 2: RPC Message Round-Trip**
        /// **Validates: Requirements 2.1, 2.3, 2.4, 2.5, 2.6**
        ///
        /// For any valid RPC message, serializing to JSON and deserializing back
        /// SHALL produce an equivalent message with all fields preserved.
        #[test]
        fn prop_rpc_message_round_trip(msg in arb_rpc_message()) {
            // Serialize to JSON
            let json = msg.to_json();
            prop_assert!(json.is_ok(), "Serialization failed: {:?}", json.err());
            let json = json.unwrap();

            // Deserialize back
            let parsed = RpcMessage::from_json(&json);
            prop_assert!(parsed.is_ok(), "Deserialization failed: {:?}", parsed.err());
            let parsed = parsed.unwrap();

            // Compare the messages
            // We need to compare field by field since we can't derive PartialEq easily
            // due to serde_json::Value
            match (&msg, &parsed) {
                (RpcMessage::OpenFile { path: p1, line: l1, col: c1 },
                 RpcMessage::OpenFile { path: p2, line: l2, col: c2 }) => {
                    prop_assert_eq!(p1, p2, "OpenFile path mismatch");
                    prop_assert_eq!(l1, l2, "OpenFile line mismatch");
                    prop_assert_eq!(c1, c2, "OpenFile col mismatch");
                }
                (RpcMessage::GotoLine { line: l1, col: c1 },
                 RpcMessage::GotoLine { line: l2, col: c2 }) => {
                    prop_assert_eq!(l1, l2, "GotoLine line mismatch");
                    prop_assert_eq!(c1, c2, "GotoLine col mismatch");
                }
                (RpcMessage::ApplyDiff { path: p1, diff: d1 },
                 RpcMessage::ApplyDiff { path: p2, diff: d2 }) => {
                    prop_assert_eq!(p1, p2, "ApplyDiff path mismatch");
                    prop_assert_eq!(d1, d2, "ApplyDiff diff mismatch");
                }
                (RpcMessage::ShowDiff { path: p1, original: o1, modified: m1 },
                 RpcMessage::ShowDiff { path: p2, original: o2, modified: m2 }) => {
                    prop_assert_eq!(p1, p2, "ShowDiff path mismatch");
                    prop_assert_eq!(o1, o2, "ShowDiff original mismatch");
                    prop_assert_eq!(m1, m2, "ShowDiff modified mismatch");
                }
                (RpcMessage::GetBuffers, RpcMessage::GetBuffers) => {}
                (RpcMessage::GetDiagnostics { path: p1 },
                 RpcMessage::GetDiagnostics { path: p2 }) => {
                    prop_assert_eq!(p1, p2, "GetDiagnostics path mismatch");
                }
                (RpcMessage::GetBufferContent { path: p1 },
                 RpcMessage::GetBufferContent { path: p2 }) => {
                    prop_assert_eq!(p1, p2, "GetBufferContent path mismatch");
                }
                (RpcMessage::GetCursor, RpcMessage::GetCursor) => {}
                (RpcMessage::BufferChanged { path: p1, content: c1, truncated: t1 },
                 RpcMessage::BufferChanged { path: p2, content: c2, truncated: t2 }) => {
                    prop_assert_eq!(p1, p2, "BufferChanged path mismatch");
                    prop_assert_eq!(c1, c2, "BufferChanged content mismatch");
                    prop_assert_eq!(t1, t2, "BufferChanged truncated mismatch");
                }
                (RpcMessage::DiagnosticsUpdated { diagnostics: d1 },
                 RpcMessage::DiagnosticsUpdated { diagnostics: d2 }) => {
                    prop_assert_eq!(d1.len(), d2.len(), "DiagnosticsUpdated count mismatch");
                    for (diag1, diag2) in d1.iter().zip(d2.iter()) {
                        prop_assert_eq!(&diag1.path, &diag2.path, "Diagnostic path mismatch");
                        prop_assert_eq!(diag1.line, diag2.line, "Diagnostic line mismatch");
                        prop_assert_eq!(diag1.col, diag2.col, "Diagnostic col mismatch");
                        prop_assert_eq!(diag1.severity, diag2.severity, "Diagnostic severity mismatch");
                        prop_assert_eq!(&diag1.message, &diag2.message, "Diagnostic message mismatch");
                    }
                }
                (RpcMessage::CursorMoved { path: p1, line: l1, col: c1 },
                 RpcMessage::CursorMoved { path: p2, line: l2, col: c2 }) => {
                    prop_assert_eq!(p1, p2, "CursorMoved path mismatch");
                    prop_assert_eq!(l1, l2, "CursorMoved line mismatch");
                    prop_assert_eq!(c1, c2, "CursorMoved col mismatch");
                }
                (RpcMessage::BufferEntered { path: p1, filetype: f1 },
                 RpcMessage::BufferEntered { path: p2, filetype: f2 }) => {
                    prop_assert_eq!(p1, p2, "BufferEntered path mismatch");
                    prop_assert_eq!(f1, f2, "BufferEntered filetype mismatch");
                }
                (RpcMessage::EditorClosed, RpcMessage::EditorClosed) => {}
                (RpcMessage::Response { id: i1, result: r1 },
                 RpcMessage::Response { id: i2, result: r2 }) => {
                    prop_assert_eq!(i1, i2, "Response id mismatch");
                    prop_assert_eq!(r1, r2, "Response result mismatch");
                }
                (RpcMessage::Error { id: i1, message: m1, code: c1 },
                 RpcMessage::Error { id: i2, message: m2, code: c2 }) => {
                    prop_assert_eq!(i1, i2, "Error id mismatch");
                    prop_assert_eq!(m1, m2, "Error message mismatch");
                    prop_assert_eq!(c1, c2, "Error code mismatch");
                }
                (RpcMessage::Ping { seq: s1 }, RpcMessage::Ping { seq: s2 }) => {
                    prop_assert_eq!(s1, s2, "Ping seq mismatch");
                }
                (RpcMessage::Pong { seq: s1 }, RpcMessage::Pong { seq: s2 }) => {
                    prop_assert_eq!(s1, s2, "Pong seq mismatch");
                }
                (RpcMessage::Request { id: i1, command: c1 },
                 RpcMessage::Request { id: i2, command: c2 }) => {
                    prop_assert_eq!(i1, i2, "Request id mismatch");
                    // Commands are boxed, compare their JSON representations
                    let json1 = c1.to_json().unwrap();
                    let json2 = c2.to_json().unwrap();
                    prop_assert_eq!(json1, json2, "Request command mismatch");
                }
                _ => {
                    // Different message types - this shouldn't happen
                    prop_assert!(false, "Message type mismatch: original={:?}, parsed={:?}", msg, parsed);
                }
            }
        }

        /// **Feature: terminal-tui-chat, Property 2: RPC Message Round-Trip**
        /// **Validates: Requirements 2.1, 2.3, 2.4, 2.5, 2.6**
        ///
        /// For any valid diagnostic, serializing to JSON and deserializing back
        /// SHALL produce an equivalent diagnostic with all fields preserved.
        #[test]
        fn prop_diagnostic_round_trip(diag in arb_diagnostic()) {
            // Serialize to JSON
            let json = serde_json::to_string(&diag);
            prop_assert!(json.is_ok(), "Serialization failed: {:?}", json.err());
            let json = json.unwrap();

            // Deserialize back
            let parsed: Result<Diagnostic, _> = serde_json::from_str(&json);
            prop_assert!(parsed.is_ok(), "Deserialization failed: {:?}", parsed.err());
            let parsed = parsed.unwrap();

            // Compare fields
            prop_assert_eq!(&diag.path, &parsed.path, "path mismatch");
            prop_assert_eq!(diag.line, parsed.line, "line mismatch");
            prop_assert_eq!(diag.col, parsed.col, "col mismatch");
            prop_assert_eq!(diag.end_line, parsed.end_line, "end_line mismatch");
            prop_assert_eq!(diag.end_col, parsed.end_col, "end_col mismatch");
            prop_assert_eq!(diag.severity, parsed.severity, "severity mismatch");
            prop_assert_eq!(&diag.message, &parsed.message, "message mismatch");
            prop_assert_eq!(&diag.source, &parsed.source, "source mismatch");
            prop_assert_eq!(&diag.code, &parsed.code, "code mismatch");
        }

        /// **Feature: terminal-tui-chat, Property 2: RPC Message Round-Trip**
        /// **Validates: Requirements 2.1, 2.3, 2.4, 2.5, 2.6**
        ///
        /// For any valid buffer info, serializing to JSON and deserializing back
        /// SHALL produce an equivalent buffer info with all fields preserved.
        #[test]
        fn prop_buffer_info_round_trip(
            id in 0u32..10000u32,
            path in prop::option::of(arb_path()),
            name in "[a-zA-Z0-9_\\-\\.]{1,50}",
            modified in any::<bool>(),
            filetype in prop::option::of(arb_filetype())
        ) {
            let info = BufferInfo {
                id,
                path,
                name,
                modified,
                filetype,
            };

            // Serialize to JSON
            let json = serde_json::to_string(&info);
            prop_assert!(json.is_ok(), "Serialization failed: {:?}", json.err());
            let json = json.unwrap();

            // Deserialize back
            let parsed: Result<BufferInfo, _> = serde_json::from_str(&json);
            prop_assert!(parsed.is_ok(), "Deserialization failed: {:?}", parsed.err());
            let parsed = parsed.unwrap();

            // Compare fields
            prop_assert_eq!(info.id, parsed.id, "id mismatch");
            prop_assert_eq!(&info.path, &parsed.path, "path mismatch");
            prop_assert_eq!(&info.name, &parsed.name, "name mismatch");
            prop_assert_eq!(info.modified, parsed.modified, "modified mismatch");
            prop_assert_eq!(&info.filetype, &parsed.filetype, "filetype mismatch");
        }
    }
}
