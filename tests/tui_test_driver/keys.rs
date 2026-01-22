//! Key parsing utilities
//!
//! Converts human-readable key strings to KeyEvent or terminal bytes

use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Parse a key string into a KeyEvent
///
/// Supports:
/// - Single characters: "a", "1", "/"
/// - Special keys: "Enter", "Escape", "Tab", "Backspace", "Space"
/// - Arrow keys: "Up", "Down", "Left", "Right"
/// - Modifiers: "Ctrl+C", "Shift+Tab", "Alt+Enter", "Ctrl+Shift+S"
/// - Function keys: "F1", "F12"
pub fn parse(key: &str) -> Result<KeyEvent> {
    let key = key.trim();

    // Parse modifiers
    let mut modifiers = KeyModifiers::NONE;
    let mut remaining = key;

    loop {
        if remaining.starts_with("Ctrl+") || remaining.starts_with("ctrl+") {
            modifiers |= KeyModifiers::CONTROL;
            remaining = &remaining[5..];
        } else if remaining.starts_with("Shift+") || remaining.starts_with("shift+") {
            modifiers |= KeyModifiers::SHIFT;
            remaining = &remaining[6..];
        } else if remaining.starts_with("Alt+") || remaining.starts_with("alt+") {
            modifiers |= KeyModifiers::ALT;
            remaining = &remaining[4..];
        } else {
            break;
        }
    }

    let code = parse_key_code(remaining)?;
    Ok(KeyEvent::new(code, modifiers))
}

fn parse_key_code(s: &str) -> Result<KeyCode> {
    match s.to_lowercase().as_str() {
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "tab" => Ok(KeyCode::Tab),
        "backspace" | "bs" => Ok(KeyCode::Backspace),
        "space" | " " => Ok(KeyCode::Char(' ')),
        "up" | "uparrow" => Ok(KeyCode::Up),
        "down" | "downarrow" => Ok(KeyCode::Down),
        "left" | "leftarrow" => Ok(KeyCode::Left),
        "right" | "rightarrow" => Ok(KeyCode::Right),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCode::PageDown),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "delete" | "del" => Ok(KeyCode::Delete),
        "f1" => Ok(KeyCode::F(1)),
        "f2" => Ok(KeyCode::F(2)),
        "f3" => Ok(KeyCode::F(3)),
        "f4" => Ok(KeyCode::F(4)),
        "f5" => Ok(KeyCode::F(5)),
        "f6" => Ok(KeyCode::F(6)),
        "f7" => Ok(KeyCode::F(7)),
        "f8" => Ok(KeyCode::F(8)),
        "f9" => Ok(KeyCode::F(9)),
        "f10" => Ok(KeyCode::F(10)),
        "f11" => Ok(KeyCode::F(11)),
        "f12" => Ok(KeyCode::F(12)),
        s if s.len() == 1 => Ok(KeyCode::Char(s.chars().next().unwrap())),
        _ => Err(anyhow!("Unknown key: '{}'", s)),
    }
}

/// Convert a key string to terminal bytes (for PTY)
///
/// This converts human-readable keys to the actual bytes a terminal would receive
pub fn to_terminal_bytes(key: &str) -> Result<Vec<u8>> {
    let key = key.trim();

    // Check for modifiers first
    if key.starts_with("Ctrl+") || key.starts_with("ctrl+") {
        let remaining = &key[5..];
        if remaining.len() == 1 {
            let c = remaining.chars().next().unwrap().to_ascii_lowercase();
            // Ctrl+A = 0x01, Ctrl+B = 0x02, etc.
            let code = (c as u8) - b'a' + 1;
            return Ok(vec![code]);
        }
    }

    if key.starts_with("Shift+") || key.starts_with("shift+") {
        let remaining = &key[6..];
        if remaining.eq_ignore_ascii_case("tab") {
            // Shift+Tab = ESC [ Z
            return Ok(vec![0x1b, b'[', b'Z']);
        }
    }

    // Special keys
    match key.to_lowercase().as_str() {
        "enter" | "return" => Ok(vec![b'\r']),
        "esc" | "escape" => Ok(vec![0x1b]),
        "tab" => Ok(vec![b'\t']),
        "backspace" | "bs" => Ok(vec![0x7f]),
        "space" | " " => Ok(vec![b' ']),
        "up" | "uparrow" => Ok(vec![0x1b, b'[', b'A']),
        "down" | "downarrow" => Ok(vec![0x1b, b'[', b'B']),
        "right" | "rightarrow" => Ok(vec![0x1b, b'[', b'C']),
        "left" | "leftarrow" => Ok(vec![0x1b, b'[', b'D']),
        "home" => Ok(vec![0x1b, b'[', b'H']),
        "end" => Ok(vec![0x1b, b'[', b'F']),
        "pageup" | "pgup" => Ok(vec![0x1b, b'[', b'5', b'~']),
        "pagedown" | "pgdn" => Ok(vec![0x1b, b'[', b'6', b'~']),
        "delete" | "del" => Ok(vec![0x1b, b'[', b'3', b'~']),
        "?" => Ok(vec![b'?']),
        "/" => Ok(vec![b'/']),
        s if s.len() == 1 => Ok(vec![s.as_bytes()[0]]),
        _ => Err(anyhow!("Unknown key: '{}'", key)),
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::{parse, to_terminal_bytes};
    #[allow(unused_imports)]
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn test_parse_simple_keys() {
        assert_eq!(parse("a").unwrap().code, KeyCode::Char('a'));
        assert_eq!(parse("Enter").unwrap().code, KeyCode::Enter);
        assert_eq!(parse("Escape").unwrap().code, KeyCode::Esc);
    }

    #[test]
    fn test_parse_modifiers() {
        let event = parse("Ctrl+C").unwrap();
        assert_eq!(event.code, KeyCode::Char('c'));
        assert!(event.modifiers.contains(KeyModifiers::CONTROL));

        let event = parse("Ctrl+Shift+S").unwrap();
        assert_eq!(event.code, KeyCode::Char('s'));
        assert!(event.modifiers.contains(KeyModifiers::CONTROL));
        assert!(event.modifiers.contains(KeyModifiers::SHIFT));
    }

    #[test]
    fn test_to_terminal_bytes() {
        assert_eq!(to_terminal_bytes("a").unwrap(), vec![b'a']);
        assert_eq!(to_terminal_bytes("Enter").unwrap(), vec![b'\r']);
        assert_eq!(to_terminal_bytes("Ctrl+B").unwrap(), vec![0x02]);
        assert_eq!(to_terminal_bytes("?").unwrap(), vec![b'?']);
    }
}
