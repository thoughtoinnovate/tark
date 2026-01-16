//! Shared streaming utilities for LLM providers
//!
//! This module provides reusable streaming parsers and state management
//! to avoid duplicating logic across providers.

mod tool_tracker;

pub use tool_tracker::ToolCallTracker;

/// Server-Sent Events (SSE) decoder
///
/// Buffers incoming bytes and extracts complete SSE `data:` payloads.
/// Handles edge cases like:
/// - Events split across multiple chunks
/// - Multiple events in a single chunk
/// - Final event without trailing newline
///
/// # Example
/// ```
/// use tark_cli::llm::streaming::SseDecoder;
///
/// let mut decoder = SseDecoder::new();
///
/// // Push chunk 1
/// let payloads1 = decoder.push(b"data: {\"text\":\"hello\"}\n\n");
/// assert_eq!(payloads1, vec!["{\"text\":\"hello\"}"]);
///
/// // Push chunk 2 (split event)
/// let payloads2 = decoder.push(b"data: {\"text\"");
/// assert_eq!(payloads2, Vec::<String>::new());
///
/// // Push chunk 3 (completes event)
/// let payloads3 = decoder.push(b":\"world\"}\n\n");
/// assert_eq!(payloads3, vec!["{\"text\":\"world\"}"]);
///
/// // Finish (no trailing newline)
/// let payloads4 = decoder.push(b"data: {\"done\":true}");
/// let remaining = decoder.finish();
/// assert_eq!(remaining, vec!["{\"done\":true}"]);
/// ```
#[derive(Debug, Default)]
pub struct SseDecoder {
    buffer: String,
}

impl SseDecoder {
    /// Create a new SSE decoder
    pub fn new() -> Self {
        Self::default()
    }

    /// Push incoming bytes and extract complete SSE `data:` payloads
    ///
    /// Returns a vector of JSON payload strings (without the `data:` prefix).
    /// Incomplete events remain buffered for the next `push()` or `finish()`.
    pub fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        // Append to buffer (lossy UTF-8 conversion for robustness)
        self.buffer.push_str(&String::from_utf8_lossy(bytes));

        let mut payloads = Vec::new();

        // Process complete lines (ending in \n)
        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].trim().to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();

            // Skip empty lines (SSE event separators)
            if line.is_empty() {
                continue;
            }

            // Extract payload from "data: {...}" lines
            if let Some(payload) = line.strip_prefix("data:") {
                payloads.push(payload.trim().to_string());
            }
        }

        payloads
    }

    /// Flush any remaining buffered content
    ///
    /// Call this when the stream ends to extract the final event
    /// if it doesn't have a trailing newline.
    ///
    /// Returns remaining `data:` payloads.
    pub fn finish(&mut self) -> Vec<String> {
        let mut payloads = Vec::new();

        // Process any remaining lines in the buffer (even without trailing \n)
        for line in self.buffer.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            if let Some(payload) = line.strip_prefix("data:") {
                payloads.push(payload.trim().to_string());
            }
        }

        // Clear buffer after flushing
        self.buffer.clear();

        payloads
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_complete_event() {
        let mut decoder = SseDecoder::new();
        let payloads = decoder.push(b"data: {\"hello\":\"world\"}\n\n");
        assert_eq!(payloads, vec!["{\"hello\":\"world\"}"]);
    }

    #[test]
    fn test_multiple_events_in_one_chunk() {
        let mut decoder = SseDecoder::new();
        let payloads = decoder.push(b"data: {\"a\":1}\n\ndata: {\"b\":2}\n\n");
        assert_eq!(payloads, vec!["{\"a\":1}", "{\"b\":2}"]);
    }

    #[test]
    fn test_event_split_across_chunks() {
        let mut decoder = SseDecoder::new();

        // First chunk: incomplete event
        let payloads1 = decoder.push(b"data: {\"text\":\"hel");
        assert_eq!(payloads1, vec![] as Vec<String>);

        // Second chunk: completes event
        let payloads2 = decoder.push(b"lo\"}\n\n");
        assert_eq!(payloads2, vec!["{\"text\":\"hello\"}"]);
    }

    #[test]
    fn test_final_event_without_trailing_newline() {
        let mut decoder = SseDecoder::new();

        // Event 1: complete
        let payloads1 = decoder.push(b"data: {\"a\":1}\n\n");
        assert_eq!(payloads1, vec!["{\"a\":1}"]);

        // Event 2: no trailing newline
        let payloads2 = decoder.push(b"data: {\"b\":2}");
        assert_eq!(payloads2, vec![] as Vec<String>);

        // Finish extracts the final event
        let remaining = decoder.finish();
        assert_eq!(remaining, vec!["{\"b\":2}"]);
    }

    #[test]
    fn test_empty_lines_ignored() {
        let mut decoder = SseDecoder::new();
        let payloads = decoder.push(b"\n\ndata: {\"x\":1}\n\n\n");
        assert_eq!(payloads, vec!["{\"x\":1}"]);
    }

    #[test]
    fn test_non_data_lines_ignored() {
        let mut decoder = SseDecoder::new();
        let payloads =
            decoder.push(b": comment\ndata: {\"x\":1}\nevent: message\ndata: {\"y\":2}\n\n");
        assert_eq!(payloads, vec!["{\"x\":1}", "{\"y\":2}"]);
    }

    #[test]
    fn test_finish_clears_buffer() {
        let mut decoder = SseDecoder::new();
        decoder.push(b"data: {\"a\":1}");
        let remaining1 = decoder.finish();
        assert_eq!(remaining1, vec!["{\"a\":1}"]);

        // Second finish returns empty (buffer was cleared)
        let remaining2 = decoder.finish();
        assert_eq!(remaining2, vec![] as Vec<String>);
    }

    #[test]
    fn test_multiple_events_no_final_newline() {
        let mut decoder = SseDecoder::new();
        let payloads1 = decoder.push(b"data: {\"a\":1}\n\ndata: {\"b\":2}");
        assert_eq!(payloads1, vec!["{\"a\":1}"]);

        let remaining = decoder.finish();
        assert_eq!(remaining, vec!["{\"b\":2}"]);
    }

    #[test]
    fn test_utf8_lossy_conversion() {
        let mut decoder = SseDecoder::new();
        // Invalid UTF-8 sequence (will be replaced with replacement char)
        let payloads = decoder.push(b"data: {\"text\":\"\xFF\"}\n");
        assert_eq!(payloads.len(), 1);
        assert!(payloads[0].contains("text"));
    }
}
