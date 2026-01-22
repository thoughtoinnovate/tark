//! Event handling for the TUI
//!
//! Manages keyboard, mouse, and terminal events.

use crossterm::event::{self, Event as CrosstermEvent, KeyEvent, MouseEvent};
use std::time::Duration;

/// TUI events
#[derive(Debug, Clone)]
pub enum Event {
    /// Keyboard event
    Key(KeyEvent),
    /// Mouse event
    Mouse(MouseEvent),
    /// Terminal resize
    Resize(u16, u16),
    /// Tick for animations
    Tick,
}

/// Event handler for polling terminal events
pub struct EventHandler {
    /// Tick rate for animations (milliseconds)
    tick_rate: Duration,
}

impl Default for EventHandler {
    fn default() -> Self {
        Self::new(Duration::from_millis(100))
    }
}

impl EventHandler {
    /// Create a new event handler with specified tick rate
    pub fn new(tick_rate: Duration) -> Self {
        Self { tick_rate }
    }

    /// Poll for the next event
    pub fn next(&self) -> std::io::Result<Event> {
        if event::poll(self.tick_rate)? {
            match event::read()? {
                CrosstermEvent::Key(key) => Ok(Event::Key(key)),
                CrosstermEvent::Mouse(mouse) => Ok(Event::Mouse(mouse)),
                CrosstermEvent::Resize(w, h) => Ok(Event::Resize(w, h)),
                _ => Ok(Event::Tick),
            }
        } else {
            Ok(Event::Tick)
        }
    }
}
