//! Terminal guard: best-effort terminal restore on drop / panic.
//!
//! `TerminalGuard` is a small RAII helper usable from `src/transport/cli.rs`
//! (which owns terminal setup/teardown). Dropping the guard restores the
//! terminal (disable raw mode, leave alternate screen, disable mouse capture
//! and bracketed paste) best-effort: all errors are ignored and it never
//! panics. [`install_panic_hook`] installs a panic hook that restores the
//! terminal before delegating to the previously installed hook.

use std::io::stdout;

/// RAII guard that restores the terminal when dropped.
///
/// Create it *after* terminal setup (raw mode + alternate screen) in `cli.rs`
/// and hold it for the lifetime of the TUI run:
///
/// ```ignore
/// let _guard = TerminalGuard::new();
/// install_panic_hook();
/// ```
pub struct TerminalGuard {
    /// Whether the guard is armed. Disarmed guards do nothing on drop.
    armed: bool,
}

impl TerminalGuard {
    /// Create a new armed guard.
    pub fn new() -> Self {
        Self { armed: true }
    }

    /// Disarm the guard so dropping it does nothing.
    /// Useful when the caller already restored the terminal explicitly.
    pub fn disarm(&mut self) {
        self.armed = false;
    }

    /// Restore the terminal best-effort. Never panics, ignores all errors.
    pub fn restore() {
        restore_terminal();
    }
}

impl Default for TerminalGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.armed {
            // Best-effort only: never panic from Drop.
            restore_terminal();
        }
    }
}

/// Best-effort terminal restore. Ignores all errors and never panics.
fn restore_terminal() {
    // Each step is best-effort; a failing step must not prevent the rest.
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        stdout(),
        crossterm::terminal::LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste,
    );
    // Ensure the cursor is visible again; ignore errors.
    let _ = crossterm::execute!(stdout(), crossterm::cursor::Show);
}

/// Install a panic hook that restores the terminal before calling the
/// previously installed (default) hook.
///
/// Safe to call multiple times: each call chains onto the previous hook, but
/// restoration itself is idempotent and best-effort.
pub fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Best-effort restore; never panic inside the hook.
        restore_terminal();
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guard_restore_never_panics_without_tty() {
        // Must not panic even when there is no TTY / raw mode was never enabled.
        TerminalGuard::restore();
        let guard = TerminalGuard::new();
        drop(guard);
    }

    #[test]
    fn disarmed_guard_drop_does_nothing() {
        let mut guard = TerminalGuard::new();
        guard.disarm();
        drop(guard);
        // If we reach here without panic, the test passes.
    }

    #[test]
    fn default_guard_is_armed() {
        let guard = TerminalGuard::default();
        assert!(guard.armed);
    }
}
