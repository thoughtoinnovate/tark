//! Buffer reading utilities
//!
//! Converts ratatui TestBackend buffer to strings for testing

use ratatui::buffer::Buffer;

/// Convert entire buffer to string
pub fn to_string(buf: &Buffer, width: u16, height: u16) -> String {
    let mut result = String::new();
    for y in 0..height {
        for x in 0..width {
            result.push_str(
                buf.cell((x, y))
                    .map(|c| c.symbol())
                    .unwrap_or(" ")
            );
        }
        result.push('\n');
    }
    result
}

/// Get a single line from buffer
pub fn line(buf: &Buffer, y: u16, width: u16) -> String {
    (0..width)
        .map(|x| buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "))
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// Get character at specific position
pub fn char_at(buf: &Buffer, x: u16, y: u16) -> String {
    buf.cell((x, y))
        .map(|c| c.symbol().to_string())
        .unwrap_or_default()
}

/// Get a rectangular region from the buffer
pub fn region(buf: &Buffer, start_x: u16, start_y: u16, width: u16, height: u16) -> String {
    let mut result = String::new();
    for y in start_y..(start_y + height) {
        for x in start_x..(start_x + width) {
            result.push_str(
                buf.cell((x, y))
                    .map(|c| c.symbol())
                    .unwrap_or(" ")
            );
        }
        result.push('\n');
    }
    result
}

/// Find all lines containing specific text
pub fn find_lines(buf: &Buffer, text: &str, width: u16, height: u16) -> Vec<(u16, String)> {
    let mut results = Vec::new();
    for y in 0..height {
        let line_text = line(buf, y, width);
        if line_text.contains(text) {
            results.push((y, line_text));
        }
    }
    results
}

/// Find the first occurrence of text and return its position
pub fn find_text(buf: &Buffer, text: &str, width: u16, height: u16) -> Option<(u16, u16)> {
    for y in 0..height {
        let line_text = line(buf, y, width);
        if let Some(x) = line_text.find(text) {
            return Some((x as u16, y));
        }
    }
    None
}

/// Check if buffer contains text anywhere
pub fn contains(buf: &Buffer, text: &str, width: u16, height: u16) -> bool {
    to_string(buf, width, height).contains(text)
}
