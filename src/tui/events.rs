//! Event handling for the TUI
//!
//! Handles keyboard, mouse, and terminal events using crossterm.

// Allow dead code for intentionally unused API methods that are part of the public interface
#![allow(dead_code)]

use std::time::Duration;

use crossterm::event::{self, KeyCode, KeyEvent, KeyModifiers, MouseEvent};

/// Events that can occur in the TUI
#[derive(Debug, Clone)]
pub enum Event {
    /// A key was pressed
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal was resized
    Resize(u16, u16),
    /// Paste event (bracketed paste)
    Paste(String),
    /// Tick event for periodic updates
    Tick,
}

/// Handles events from the terminal
#[derive(Debug)]
pub struct EventHandler {
    /// Tick rate for periodic updates
    tick_rate: Duration,
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl EventHandler {
    /// Create a new event handler with default tick rate (250ms)
    pub fn new() -> Self {
        Self {
            tick_rate: Duration::from_millis(250),
        }
    }

    /// Create a new event handler with custom tick rate
    pub fn with_tick_rate(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    /// Poll for the next event
    ///
    /// Returns `Some(Event)` if an event occurred, or `None` if the tick rate elapsed.
    pub fn poll(&self) -> anyhow::Result<Option<Event>> {
        if event::poll(self.tick_rate)? {
            let event = event::read()?;
            Ok(Some(self.convert_event(event)))
        } else {
            Ok(Some(Event::Tick))
        }
    }

    /// Poll for the next event with a custom timeout
    ///
    /// Returns `Some(Event)` if an event occurred, or `None` if the timeout elapsed.
    pub fn poll_with_timeout(&self, timeout: Duration) -> anyhow::Result<Option<Event>> {
        if event::poll(timeout)? {
            let event = event::read()?;
            Ok(Some(self.convert_event(event)))
        } else {
            Ok(None)
        }
    }

    /// Convert a crossterm event to our Event type
    fn convert_event(&self, event: event::Event) -> Event {
        match event {
            event::Event::Key(key) => Event::Key(key),
            event::Event::Mouse(mouse) => Event::Mouse(mouse),
            event::Event::Resize(cols, rows) => Event::Resize(cols, rows),
            event::Event::Paste(text) => Event::Paste(text),
            // FocusGained, FocusLost are treated as ticks
            _ => Event::Tick,
        }
    }
}

/// Helper functions for key event matching
impl Event {
    /// Check if this is a quit key (Ctrl-C or Ctrl-Q or 'q')
    pub fn is_quit(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Char('c'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) | Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::CONTROL,
                ..
            }) | Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers: KeyModifiers::NONE,
                ..
            })
        )
    }

    /// Check if this is an escape key
    pub fn is_escape(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Esc,
                ..
            })
        )
    }

    /// Check if this is an enter key
    pub fn is_enter(&self) -> bool {
        matches!(
            self,
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            })
        )
    }

    /// Check if this is a resize event
    pub fn is_resize(&self) -> bool {
        matches!(self, Event::Resize(_, _))
    }

    /// Get resize dimensions if this is a resize event
    pub fn resize_dimensions(&self) -> Option<(u16, u16)> {
        match self {
            Event::Resize(cols, rows) => Some((*cols, *rows)),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEventKind, KeyEventState};

    fn make_key_event(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn test_is_quit_ctrl_c() {
        let event = make_key_event(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert!(event.is_quit());
    }

    #[test]
    fn test_is_quit_ctrl_q() {
        let event = make_key_event(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert!(event.is_quit());
    }

    #[test]
    fn test_is_quit_q() {
        let event = make_key_event(KeyCode::Char('q'), KeyModifiers::NONE);
        assert!(event.is_quit());
    }

    #[test]
    fn test_is_not_quit() {
        let event = make_key_event(KeyCode::Char('a'), KeyModifiers::NONE);
        assert!(!event.is_quit());
    }

    #[test]
    fn test_is_escape() {
        let event = make_key_event(KeyCode::Esc, KeyModifiers::NONE);
        assert!(event.is_escape());
    }

    #[test]
    fn test_is_enter() {
        let event = make_key_event(KeyCode::Enter, KeyModifiers::NONE);
        assert!(event.is_enter());
    }

    #[test]
    fn test_resize_event() {
        let event = Event::Resize(80, 24);
        assert!(event.is_resize());
        assert_eq!(event.resize_dimensions(), Some((80, 24)));
    }

    #[test]
    fn test_non_resize_event() {
        let event = Event::Tick;
        assert!(!event.is_resize());
        assert_eq!(event.resize_dimensions(), None);
    }

    #[test]
    fn test_event_handler_default() {
        let handler = EventHandler::new();
        assert_eq!(handler.tick_rate, Duration::from_millis(250));
    }

    #[test]
    fn test_event_handler_custom_tick_rate() {
        let handler = EventHandler::with_tick_rate(Duration::from_millis(100));
        assert_eq!(handler.tick_rate, Duration::from_millis(100));
    }
}
