//! Core domain modules
//!
//! This module contains domain logic and types shared across the application,
//! including the ui_backend layer and presentation layers.

#![allow(dead_code)]

pub mod attachments;
pub mod context_tracker;
pub mod conversation_manager;
pub mod errors;
pub mod session_manager;
pub mod tokenizer;
pub mod traits;
pub mod types;

// Re-export canonical types
pub use types::AgentMode;

// Re-export context tracking types (public API)
#[allow(unused_imports)]
pub use context_tracker::{ContextBreakdown, ContextTracker};

// Re-export main types for convenience

/// Safely truncate a string at a character boundary
///
/// This avoids panics when truncating UTF-8 strings with multi-byte characters
/// like emojis (📄 is 4 bytes). Returns a slice up to `max_bytes` bytes,
/// always ending at a valid UTF-8 character boundary.
///
/// # Example
/// ```
/// use tark_cli::core::truncate_at_char_boundary;
/// let text = "Hello 📄 World";
/// let truncated = truncate_at_char_boundary(text, 10);
/// assert!(truncated.len() <= 10);
/// assert!(truncated.is_char_boundary(truncated.len())); // No panic
/// ```
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid character boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}
