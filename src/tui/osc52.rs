//! OSC 52 clipboard support for terminal-based clipboard access
//!
//! OSC 52 is a terminal escape sequence that allows reading and writing
//! to the system clipboard through the terminal. This works over SSH
//! because the local terminal interprets the escape sequences.
//!
//! Supported terminals: iTerm2, kitty, WezTerm, Alacritty, Windows Terminal,
//! foot, contour, and many others.
//!
//! Format:
//! - Write: `\x1b]52;c;BASE64_DATA\x07` or `\x1b]52;c;BASE64_DATA\x1b\\`
//! - Read:  `\x1b]52;c;?\x07` (response async, limited support)
//!
//! Note: Reading from clipboard via OSC 52 is rarely supported by terminals
//! for security reasons. Writing (copying TO clipboard) is more widely supported.

#![allow(dead_code)]

use std::io::{self, Write};
use std::time::Duration;

use super::attachments::base64_encode;

/// OSC 52 clipboard handler
///
/// Provides clipboard access through terminal escape sequences,
/// which works over SSH when the terminal supports it.
pub struct Osc52Clipboard;

impl Osc52Clipboard {
    /// Write text to clipboard using OSC 52
    ///
    /// This sends an escape sequence to the terminal, which (if supported)
    /// will update the system clipboard on the user's local machine.
    ///
    /// Works over SSH because the terminal interprets the escape sequence.
    pub fn write_text(text: &str) -> io::Result<()> {
        let encoded = base64_encode(text.as_bytes());
        let sequence = format!("\x1b]52;c;{}\x07", encoded);

        let mut stdout = io::stdout();
        stdout.write_all(sequence.as_bytes())?;
        stdout.flush()?;

        Ok(())
    }

    /// Write binary data (e.g., image) to clipboard using OSC 52
    ///
    /// Note: Large data may be truncated by some terminals.
    /// The practical limit is usually around 100KB-1MB depending on terminal.
    pub fn write_binary(data: &[u8]) -> io::Result<()> {
        let encoded = base64_encode(data);
        let sequence = format!("\x1b]52;c;{}\x07", encoded);

        let mut stdout = io::stdout();
        stdout.write_all(sequence.as_bytes())?;
        stdout.flush()?;

        Ok(())
    }

    /// Attempt to read text from clipboard using OSC 52
    ///
    /// **Warning**: Reading is NOT widely supported. Most terminals only
    /// support writing to clipboard via OSC 52 for security reasons.
    ///
    /// This is a no-op that always returns None because:
    /// 1. Most terminals don't support OSC 52 read (iTerm2, Alacritty, etc.)
    /// 2. Reading requires raw terminal mode which conflicts with ratatui
    /// 3. The /attach command is a more reliable alternative
    ///
    /// For actual clipboard reading, use the native arboard backend.
    pub fn read_text(_timeout: Duration) -> io::Result<Option<String>> {
        // OSC 52 read is not implemented because:
        // 1. It requires raw terminal mode to read the response
        // 2. Most terminals don't support it anyway (security concern)
        // 3. It would conflict with ratatui's terminal handling
        //
        // Users should use:
        // - Native clipboard (arboard) when available
        // - /attach command for files
        Ok(None)
    }

    /// Check if the terminal likely supports OSC 52 (for writing)
    ///
    /// This is a heuristic based on the TERM environment variable.
    /// It's not 100% accurate but covers common cases.
    pub fn is_likely_supported() -> bool {
        if let Ok(term) = std::env::var("TERM") {
            let term_lower = term.to_lowercase();

            // Terminals known to support OSC 52 write
            let supported = [
                "xterm",
                "xterm-256color",
                "screen",
                "tmux",
                "tmux-256color",
                "alacritty",
                "kitty",
                "wezterm",
                "foot",
                "contour",
                "iterm",
                "iterm2",
                "vte", // GNOME Terminal, Tilix, etc.
            ];

            for s in &supported {
                if term_lower.contains(s) {
                    return true;
                }
            }
        }

        // Also check TERM_PROGRAM for macOS terminals
        if let Ok(term_program) = std::env::var("TERM_PROGRAM") {
            let prog_lower = term_program.to_lowercase();
            if prog_lower.contains("iterm")
                || prog_lower.contains("wezterm")
                || prog_lower.contains("alacritty")
                || prog_lower.contains("kitty")
            {
                return true;
            }
        }

        // Default to true - most modern terminals support it
        true
    }

    /// Get a user-friendly message about clipboard support
    pub fn support_message() -> String {
        if Self::is_likely_supported() {
            "Use /attach <file> to attach files. Clipboard paste may not work over SSH."
                .to_string()
        } else {
            "Clipboard not available. Use /attach <file> to attach files.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_likely_supported() {
        // This test just ensures the function doesn't panic
        let _ = Osc52Clipboard::is_likely_supported();
    }

    #[test]
    fn test_support_message() {
        let msg = Osc52Clipboard::support_message();
        assert!(!msg.is_empty());
    }
}

