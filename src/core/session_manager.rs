//! Session Manager - Chat session lifecycle and persistence
//!
//! Handles:
//! - Session creation and initialization
//! - Session loading and switching
//! - Session persistence (auto-save)
//! - Session history management

use super::errors::SessionError;
use crate::core::types::AgentMode;
use crate::storage::{ArchiveChunkMeta, ChatSession, SessionMessage, SessionMeta, TarkStorage};
use chrono::Utc;

/// Manages chat session lifecycle and persistence
pub struct SessionManager {
    /// Storage backend
    storage: TarkStorage,
    /// Currently active session
    current_session: ChatSession,
    /// Whether the current session has unsaved changes
    dirty: bool,
}

impl SessionManager {
    /// Create a new session manager with default session
    pub fn new(storage: TarkStorage) -> Self {
        let current_session = match storage.load_current_session() {
            Ok(session) => session,
            Err(err) => {
                tracing::warn!("Failed to load current session: {}", err);
                storage
                    .create_new_session()
                    .unwrap_or_else(|_| ChatSession::new())
            }
        };
        Self {
            storage,
            current_session,
            dirty: false,
        }
    }

    /// Create from an existing session (typically loaded from storage)
    pub fn with_session(storage: TarkStorage, session: ChatSession) -> Self {
        Self {
            storage,
            current_session: session,
            dirty: false,
        }
    }

    /// Get the current session
    pub fn current(&self) -> &ChatSession {
        &self.current_session
    }

    /// Get mutable reference to current session
    pub fn current_mut(&mut self) -> &mut ChatSession {
        self.dirty = true;
        &mut self.current_session
    }

    /// Mark session as having unsaved changes
    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Check if session has unsaved changes
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Check if a given session ID is the current session
    pub fn is_current(&self, session_id: &str) -> bool {
        self.current_session.id == session_id
    }

    /// Archive older messages from the current session, keeping the most recent N.
    pub fn archive_old_messages(
        &mut self,
        keep_recent: usize,
    ) -> Result<Option<ArchiveChunkMeta>, SessionError> {
        let total = self.current_session.messages.len();
        if total <= keep_recent {
            return Ok(None);
        }

        let cutoff = total - keep_recent;
        let mut archived: Vec<SessionMessage> =
            self.current_session.messages.drain(0..cutoff).collect();
        for msg in &mut archived {
            msg.context_transient = true;
        }

        let meta = self
            .storage
            .archive_session_messages(&self.current_session.id, &archived)
            .map_err(|e| SessionError::Storage(e.to_string()))?;

        self.dirty = true;
        self.current_session.updated_at = chrono::Utc::now();
        Ok(Some(meta))
    }

    /// List archived chunks for the current session.
    pub fn list_archive_chunks(&self) -> Result<Vec<ArchiveChunkMeta>, SessionError> {
        self.storage
            .list_session_archives(&self.current_session.id)
            .map_err(|e| SessionError::Storage(e.to_string()))
    }

    /// Load a specific archive chunk by filename.
    pub fn load_archive_chunk(&self, filename: &str) -> Result<Vec<SessionMessage>, SessionError> {
        self.storage
            .load_session_archive(&self.current_session.id, filename)
            .map_err(|e| SessionError::Storage(e.to_string()))
    }

    /// Create a new session
    ///
    /// This creates a new empty session and switches to it, saving the previous session if needed.
    pub fn create_new(&mut self) -> Result<&ChatSession, SessionError> {
        // Save current session if dirty
        if self.dirty {
            self.save()?;
        }

        // Create new session
        self.current_session = self
            .storage
            .create_new_session()
            .map_err(|e| SessionError::Storage(e.to_string()))?;

        self.dirty = false;

        // Save the new session
        self.save()?;

        Ok(&self.current_session)
    }

    /// Switch to a different session by ID
    ///
    /// This saves the current session (if dirty), loads the target session,
    /// and returns it for the caller to restore into their agent/conversation state.
    pub fn switch_to(&mut self, session_id: &str) -> Result<&ChatSession, SessionError> {
        // Don't switch if already current
        if self.current_session.id == session_id {
            return Ok(&self.current_session);
        }

        // Save current session if dirty
        if self.dirty {
            self.save()?;
        }

        // Load the target session
        let session = self
            .storage
            .load_session(session_id)
            .map_err(|e| SessionError::Storage(e.to_string()))?;

        self.current_session = session;
        self.dirty = false;

        // Set as current in storage
        let _ = self.storage.set_current_session(session_id);

        Ok(&self.current_session)
    }

    /// Save the current session to storage
    pub fn save(&self) -> Result<(), SessionError> {
        self.storage
            .save_session(&self.current_session)
            .map_err(|e| SessionError::Storage(e.to_string()))?;
        Ok(())
    }

    /// Auto-save if dirty
    pub fn auto_save_if_needed(&mut self) -> Result<(), SessionError> {
        if self.dirty {
            self.save()?;
            self.dirty = false;
        }
        Ok(())
    }

    /// List all available sessions
    pub fn list_all(&self) -> Result<Vec<SessionMeta>, SessionError> {
        self.storage
            .list_sessions()
            .map_err(|e| SessionError::Storage(e.to_string()))
    }

    /// Delete a session by ID
    ///
    /// Cannot delete the current session.
    pub fn delete(&mut self, session_id: &str) -> Result<(), SessionError> {
        // Prevent deleting current session
        if self.current_session.id == session_id {
            return Err(SessionError::CannotDeleteCurrent);
        }

        self.storage
            .delete_session(session_id)
            .map_err(|e| SessionError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Update session metadata (provider, model, mode)
    ///
    /// This is typically called when the agent configuration changes.
    pub fn update_metadata(&mut self, provider: String, model: String, mode: AgentMode) {
        self.current_session.provider = provider;
        self.current_session.model = model;
        self.current_session.mode = mode.to_string();
        self.dirty = true;
    }

    /// Append a message to the current session
    pub fn append_message(&mut self, message: SessionMessage) {
        self.current_session.messages.push(message);
        self.current_session.updated_at = Utc::now();
        self.dirty = true;
    }

    /// Get session statistics
    pub fn stats(&self) -> SessionStats {
        SessionStats {
            session_id: self.current_session.id.clone(),
            message_count: self.current_session.messages.len(),
            provider: self.current_session.provider.clone(),
            model: self.current_session.model.clone(),
            mode: self.current_session.mode.clone(),
        }
    }

    /// Export current session to JSON
    pub fn export_json(&self) -> Result<String, SessionError> {
        serde_json::to_string_pretty(&self.current_session)
            .map_err(|e| SessionError::InvalidState(format!("Export failed: {}", e)))
    }

    /// Import session from JSON
    ///
    /// This creates a new session from the imported data and switches to it.
    pub fn import_from_json(&mut self, json: &str) -> Result<&ChatSession, SessionError> {
        let session: ChatSession = serde_json::from_str(json)
            .map_err(|e| SessionError::InvalidState(format!("Import failed: {}", e)))?;

        // Save current session if dirty
        if self.dirty {
            self.save()?;
        }

        // Switch to imported session
        self.current_session = session;
        self.dirty = true;
        self.save()?;
        self.dirty = false;

        Ok(&self.current_session)
    }
}

/// Session statistics for display
#[derive(Debug, Clone)]
pub struct SessionStats {
    pub session_id: String,
    pub message_count: usize,
    pub provider: String,
    pub model: String,
    pub mode: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_storage() -> (TempDir, TarkStorage) {
        let temp = TempDir::new().unwrap();
        let storage = TarkStorage::new(temp.path()).unwrap();
        (temp, storage)
    }

    #[test]
    fn test_session_manager_creation() {
        let (_temp, storage) = create_test_storage();
        let session = storage.create_new_session().unwrap();
        let session_id = session.id.clone();
        let mgr = SessionManager::new(storage);
        assert_eq!(mgr.current().id, session_id);
        assert!(!mgr.is_dirty());
    }

    #[test]
    fn test_create_new_session() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        let old_id = mgr.current().id.clone();
        let session = mgr.create_new().unwrap();
        let new_id = session.id.clone();

        assert_ne!(old_id, new_id);
        assert!(!mgr.is_dirty());
    }

    #[test]
    fn test_mark_dirty() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        assert!(!mgr.is_dirty());
        mgr.mark_dirty();
        assert!(mgr.is_dirty());
    }

    #[test]
    fn test_is_current() {
        let (_temp, storage) = create_test_storage();
        let mgr = SessionManager::new(storage);

        let current_id = mgr.current().id.clone();
        assert!(mgr.is_current(&current_id));
        assert!(!mgr.is_current("other-id"));
    }

    #[test]
    fn test_cannot_delete_current() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        let current_id = mgr.current().id.clone();
        let result = mgr.delete(&current_id);

        assert!(matches!(result, Err(SessionError::CannotDeleteCurrent)));
    }

    #[test]
    fn test_update_metadata() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        assert!(!mgr.is_dirty());
        mgr.update_metadata("openai".to_string(), "gpt-4".to_string(), AgentMode::Build);

        assert!(mgr.is_dirty());
        assert_eq!(mgr.current().provider, "openai");
        assert_eq!(mgr.current().model, "gpt-4");
        assert_eq!(mgr.current().mode, "build");
    }

    #[test]
    fn test_session_stats() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        mgr.update_metadata("claude".to_string(), "sonnet".to_string(), AgentMode::Plan);

        let stats = mgr.stats();
        assert!(!stats.session_id.is_empty());
        assert_eq!(stats.message_count, 0);
        assert_eq!(stats.provider, "claude");
        assert_eq!(stats.model, "sonnet");
        assert_eq!(stats.mode, "plan");
    }

    #[test]
    fn test_export_import_json() {
        let (_temp, storage) = create_test_storage();
        let mut mgr = SessionManager::new(storage);

        mgr.update_metadata("openai".to_string(), "gpt-4".to_string(), AgentMode::Build);

        // Export
        let json = mgr.export_json().unwrap();
        assert!(json.contains("openai"));
        assert!(json.contains("gpt-4"));

        // Import (creates new session)
        let imported = mgr.import_from_json(&json).unwrap();
        assert_eq!(imported.provider, "openai");
        assert_eq!(imported.model, "gpt-4");
    }
}
