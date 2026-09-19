//! Document management for the LSP server
//!
//! All client-supplied positions use **UTF-16 code-unit offsets**, as required by
//! the LSP specification. The helpers in this module convert between UTF-16
//! columns and byte/char indexes so multibyte content (e.g. `é`, `中`) and
//! non-BMP characters (e.g. `𝄞` U+1D11E, which occupies two UTF-16 code units)
//! map to the correct text spans (requirement R9, scenario S23).

use dashmap::DashMap;
use tower_lsp::lsp_types::*;

/// Convert a UTF-16 code-unit column (as sent by LSP clients) to a byte index
/// into `line`.
///
/// Out-of-range columns clamp to the end of the line. Columns that fall inside
/// a surrogate pair resolve to the next character boundary.
pub fn utf16_to_byte_index(line: &str, utf16_col: u32) -> usize {
    let mut utf16 = 0u32;
    for (byte_idx, c) in line.char_indices() {
        if utf16 >= utf16_col {
            return byte_idx;
        }
        utf16 += c.len_utf16() as u32;
    }
    line.len()
}

/// Convert a byte index into `line` to a UTF-16 code-unit column.
///
/// Out-of-range indexes clamp to the end of the line; indexes in the middle of
/// a character clamp down to that character's start.
///
/// Required R9/S23 API surface; exercised by the unit tests below (no current
/// production caller needs the inverse mapping yet).
#[allow(dead_code)]
pub fn byte_to_utf16_col(line: &str, byte_idx: usize) -> u32 {
    let mut idx = byte_idx.min(line.len());
    while idx > 0 && !line.is_char_boundary(idx) {
        idx -= 1;
    }
    line[..idx].chars().map(|c| c.len_utf16() as u32).sum()
}

/// Convert a UTF-16 code-unit column to a Unicode-scalar (`char`) index into
/// `line`, for consumers that index by character rather than by byte.
pub fn utf16_to_char_index(line: &str, utf16_col: u32) -> usize {
    line[..utf16_to_byte_index(line, utf16_col)].chars().count()
}

/// Split `content` into `(byte_offset_of_line_start, line_text)` pairs.
///
/// Line breaks are `\n` (a trailing `\r` is stripped, matching `str::lines`);
/// offsets always refer to the original `content` bytes.
fn split_lines_keep_offsets(content: &str) -> Vec<(usize, &str)> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for line in content.split_inclusive('\n') {
        let text = line.strip_suffix('\n').unwrap_or(line);
        let text = text.strip_suffix('\r').unwrap_or(text);
        out.push((start, text));
        start += line.len();
    }
    out
}

/// Resolve an LSP position (UTF-16 column) to a byte offset into `content`.
///
/// Returns `None` when the line number is out of range. Columns are clamped to
/// the end of the line by [`utf16_to_byte_index`].
pub fn position_to_byte_offset(content: &str, position: &Position) -> Option<usize> {
    let lines = split_lines_keep_offsets(content);
    let (start, text) = *lines.get(position.line as usize)?;
    Some(start + utf16_to_byte_index(text, position.character))
}

/// Manages open documents
pub struct DocumentStore {
    documents: DashMap<Url, Document>,
}

/// A tracked document
#[derive(Debug, Clone)]
pub struct Document {
    pub uri: Url,
    pub language_id: String,
    pub version: i32,
    pub content: String,
}

impl Document {
    /// Get the content of a specific line (0-indexed)
    pub fn get_line(&self, line: usize) -> Option<&str> {
        self.content.lines().nth(line)
    }

    /// Resolve an LSP range (UTF-16 columns) to a byte span of [`Self::content`].
    ///
    /// The start line must exist; an end line past EOF is clamped to EOF so a
    /// client asking "to the end of the document" keeps working. Returns `None`
    /// for an out-of-range start line or an inverted range.
    pub fn range_to_byte_span(&self, range: &Range) -> Option<(usize, usize)> {
        let start = position_to_byte_offset(&self.content, &range.start)?;
        let end = position_to_byte_offset(&self.content, &range.end).unwrap_or(self.content.len());
        if end < start {
            return None;
        }
        Some((start, end))
    }

    /// Apply a batch of `textDocument/didChange` content changes **in order**.
    ///
    /// Each change applies to the document state produced by the previous one
    /// (per the LSP specification); overlapping ranges are therefore resolved
    /// sequentially and must not be collapsed to last-change-wins. A change
    /// with `range == None` is a full-document replacement, which this server
    /// also accepts even though it advertises incremental sync. Out-of-range
    /// ranged edits are skipped with a warning instead of corrupting the text.
    pub fn apply_content_changes(
        &mut self,
        version: i32,
        changes: Vec<TextDocumentContentChangeEvent>,
    ) {
        self.version = version;
        for change in changes {
            match change.range {
                None => {
                    self.content = change.text;
                }
                Some(range) => match self.range_to_byte_span(&range) {
                    Some((start, end)) => {
                        self.content.replace_range(start..end, &change.text);
                    }
                    None => {
                        tracing::warn!(
                            uri = %self.uri,
                            version,
                            ?range,
                            "ignoring out-of-range incremental edit"
                        );
                    }
                },
            }
        }
    }

    /// Get content in a range (UTF-16 columns per the LSP specification)
    pub fn get_range(&self, range: &Range) -> Option<String> {
        let (start, end) = self.range_to_byte_span(range)?;
        self.content.get(start..end).map(|s| s.to_string())
    }

    /// Get the word at a position (UTF-16 column per the LSP specification)
    pub fn get_word_at(&self, position: &Position) -> Option<String> {
        let line = self.get_line(position.line as usize)?;
        let col = utf16_to_char_index(line, position.character);

        // Find word boundaries
        let chars: Vec<char> = line.chars().collect();
        if col > chars.len() {
            return None;
        }

        // Find start of word
        let mut start = col;
        while start > 0 && is_word_char(chars.get(start - 1).copied().unwrap_or(' ')) {
            start -= 1;
        }

        // Find end of word
        let mut end = col;
        while end < chars.len() && is_word_char(chars.get(end).copied().unwrap_or(' ')) {
            end += 1;
        }

        if start == end {
            None
        } else {
            Some(chars[start..end].iter().collect())
        }
    }

    /// Get position as byte offset (UTF-16 column per the LSP specification)
    #[allow(dead_code)]
    pub fn position_to_offset(&self, position: &Position) -> Option<usize> {
        position_to_byte_offset(&self.content, position)
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: DashMap::new(),
        }
    }

    pub fn open(&self, params: DidOpenTextDocumentParams) {
        let doc = Document {
            uri: params.text_document.uri.clone(),
            language_id: params.text_document.language_id,
            version: params.text_document.version,
            content: params.text_document.text,
        };
        self.documents.insert(params.text_document.uri, doc);
    }

    pub fn change(&self, params: DidChangeTextDocumentParams) {
        if let Some(mut doc) = self.documents.get_mut(&params.text_document.uri) {
            doc.apply_content_changes(params.text_document.version, params.content_changes);
        }
    }

    pub fn close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    pub fn get(&self, uri: &Url) -> Option<Document> {
        self.documents.get(uri).map(|d| d.clone())
    }
}

impl Default for DocumentStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `a` (1 UTF-16 unit, 1 byte), `é` (1 unit, 2 bytes), `中` (1 unit,
    /// 3 bytes), `𝄞` U+1D11E (2 units via a surrogate pair, 4 bytes),
    /// `z` (1 unit, 1 byte).
    const MIXED_LINE: &str = "aé中𝄞z";

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    fn range(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
        Range {
            start: pos(sl, sc),
            end: pos(el, ec),
        }
    }

    fn doc_with(content: &str) -> Document {
        Document {
            uri: Url::parse("file:///test.rs").unwrap(),
            language_id: "rust".to_string(),
            version: 1,
            content: content.to_string(),
        }
    }

    fn ranged_edit(
        sl: u32,
        sc: u32,
        el: u32,
        ec: u32,
        text: &str,
    ) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(range(sl, sc, el, ec)),
            range_length: None,
            text: text.to_string(),
        }
    }

    fn full_edit(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_string(),
        }
    }

    #[test]
    fn utf16_to_byte_index_maps_multibyte_and_non_bmp() {
        // UTF-16 columns: a=0..1, é=1..2, 中=2..3, 𝄞=3..5, z=5..6.
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 0), 0);
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 1), 1);
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 2), 3);
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 3), 6);
        // Column 4 is a lone low surrogate (degenerate input): it resolves
        // forward to the next character boundary.
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 4), 10);
    }

    #[test]
    fn utf16_surrogate_pair_boundaries() {
        // 𝄞 starts at byte 6 and is 4 bytes long; `z` starts at byte 10.
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 3), 6);
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 5), 10);
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 6), 11);
        // Columns past EOL clamp to the end of the line.
        assert_eq!(utf16_to_byte_index(MIXED_LINE, 100), 11);
        assert_eq!(utf16_to_byte_index("", 5), 0);
    }

    #[test]
    fn byte_to_utf16_col_round_trips() {
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 0), 0);
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 1), 1);
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 3), 2);
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 6), 3);
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 10), 5);
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 11), 6);
        // Mid-character bytes clamp down to the character start.
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 2), 1); // inside `é`
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 7), 3); // inside `𝄞`
                                                         // Out-of-range clamps to EOL.
        assert_eq!(byte_to_utf16_col(MIXED_LINE, 100), 6);
    }

    #[test]
    fn utf16_positions_round_trip() {
        // Well-formed positions round-trip. Column 4 is a lone low
        // surrogate (degenerate input): it normalizes forward to the next
        // character boundary instead of round-tripping.
        for col in [0, 1, 2, 3, 5, 6u32] {
            let byte = utf16_to_byte_index(MIXED_LINE, col);
            assert_eq!(byte_to_utf16_col(MIXED_LINE, byte), col);
        }
        assert_eq!(
            byte_to_utf16_col(MIXED_LINE, utf16_to_byte_index(MIXED_LINE, 4)),
            5
        );
    }

    #[test]
    fn incremental_single_ranged_edit() {
        let mut doc = doc_with("hello\nworld\n");
        doc.apply_content_changes(2, vec![ranged_edit(1, 0, 1, 5, "there")]);
        assert_eq!(doc.content, "hello\nthere\n");
        assert_eq!(doc.version, 2);
    }

    #[test]
    fn incremental_multiple_edits_apply_sequentially() {
        let mut doc = doc_with("aaa\nbbb\n");
        doc.apply_content_changes(
            2,
            vec![ranged_edit(0, 0, 0, 3, "xx"), ranged_edit(1, 0, 1, 3, "yy")],
        );
        assert_eq!(doc.content, "xx\nyy\n");
    }

    #[test]
    fn incremental_overlapping_edits_are_not_last_wins() {
        // Sequential application: "abcdef" -> "Xdef" -> "XYZf".
        // A last-change-wins collapse onto the original text would yield "aYZdef".
        let mut doc = doc_with("abcdef");
        doc.apply_content_changes(
            2,
            vec![ranged_edit(0, 0, 0, 3, "X"), ranged_edit(0, 1, 0, 3, "YZ")],
        );
        assert_eq!(doc.content, "XYZf");
    }

    #[test]
    fn full_replace_is_accepted_as_input() {
        let mut doc = doc_with("old content\n");
        doc.apply_content_changes(2, vec![full_edit("brand new")]);
        assert_eq!(doc.content, "brand new");
    }

    #[test]
    fn mixed_full_then_ranged_applies_to_new_content() {
        let mut doc = doc_with("old");
        doc.apply_content_changes(2, vec![full_edit("abcdef"), ranged_edit(0, 0, 0, 3, "X")]);
        assert_eq!(doc.content, "Xdef");
    }

    #[test]
    fn incremental_edit_with_multibyte_text_uses_utf16_columns() {
        // é=1 unit, 中=1 unit: UTF-16 span (0,1)-(0,3) covers `é中`.
        // Interpreted as bytes it would cover only `é` + the first byte of `中`.
        let mut doc = doc_with("aé中\nok\n");
        doc.apply_content_changes(2, vec![ranged_edit(0, 1, 0, 3, "XY")]);
        assert_eq!(doc.content, "aXY\nok\n");
    }

    #[test]
    fn incremental_edit_with_non_bmp_text_uses_utf16_columns() {
        // 𝄞 occupies UTF-16 columns 0..2, so `a` is at column 2.
        // Interpreted as bytes, column 2 would land inside 𝄞 and corrupt it.
        let mut doc = doc_with("𝄞abc");
        doc.apply_content_changes(2, vec![ranged_edit(0, 2, 0, 3, "Z")]);
        assert_eq!(doc.content, "𝄞Zbc");
    }

    #[test]
    fn out_of_range_edit_is_skipped_without_corruption() {
        let mut doc = doc_with("hi\n");
        doc.apply_content_changes(2, vec![ranged_edit(9, 0, 9, 2, "X")]);
        assert_eq!(doc.content, "hi\n");
        assert_eq!(doc.version, 2);
    }

    #[test]
    fn get_range_uses_utf16_columns() {
        let doc = doc_with("aé中𝄞z");
        assert_eq!(doc.get_range(&range(0, 1, 0, 3)), Some("é中".to_string()));
        assert_eq!(doc.get_range(&range(0, 3, 0, 5)), Some("𝄞".to_string()));
        assert_eq!(
            doc.get_range(&range(0, 0, 0, 6)),
            Some(MIXED_LINE.to_string())
        );
        assert_eq!(doc.get_range(&range(5, 0, 5, 1)), None);
    }

    #[test]
    fn get_word_at_uses_utf16_columns() {
        let doc = doc_with("héllo wörld");
        // UTF-16 column 1 is inside `héllo` (`é` is one UTF-16 unit).
        assert_eq!(doc.get_word_at(&pos(0, 1)), Some("héllo".to_string()));
        // UTF-16 column 7 is inside `wörld`.
        assert_eq!(doc.get_word_at(&pos(0, 7)), Some("wörld".to_string()));
        assert_eq!(doc.get_word_at(&pos(3, 0)), None);
    }

    #[test]
    fn position_to_offset_uses_utf16_columns() {
        let doc = doc_with("aé\nxy");
        // Line 0 is `aé` (3 bytes); line 1 starts at byte 4.
        assert_eq!(doc.position_to_offset(&pos(0, 2)), Some(3));
        assert_eq!(doc.position_to_offset(&pos(1, 1)), Some(5));
        assert_eq!(doc.position_to_offset(&pos(7, 0)), None);
    }

    #[test]
    fn document_store_applies_incremental_change() {
        use tower_lsp::lsp_types::{
            DidChangeTextDocumentParams, DidOpenTextDocumentParams, TextDocumentItem,
            VersionedTextDocumentIdentifier,
        };

        let store = DocumentStore::new();
        let uri = Url::parse("file:///edit.rs").unwrap();
        store.open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "rust".to_string(),
                version: 1,
                text: "let x = 1;\n".to_string(),
            },
        });
        store.change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![ranged_edit(0, 8, 0, 9, "2")],
        });
        let doc = store.get(&uri).unwrap();
        assert_eq!(doc.content, "let x = 2;\n");
        assert_eq!(doc.version, 2);
    }

    #[test]
    fn utf16_non_bmp_line_maps_surrogate_pair() {
        // `𝄞` U+1D11E: 2 UTF-16 units, 4 bytes. In `a𝄞b` a UTF-16 client
        // addresses a=0..1, 𝄞=1..3, b=3..4.
        let line = "a𝄞b";
        assert_eq!(line.encode_utf16().count(), 4);
        assert_eq!(utf16_to_byte_index(line, 0), 0);
        assert_eq!(utf16_to_byte_index(line, 1), 1);
        // Column 2 is the lone low surrogate: resolves forward to `b`.
        assert_eq!(utf16_to_byte_index(line, 2), 5);
        assert_eq!(utf16_to_byte_index(line, 3), 5);
        assert_eq!(utf16_to_byte_index(line, 4), 6);
        assert_eq!(utf16_to_byte_index(line, 99), 6);
        // A lone `𝄞` line: interior column and EOL clamping.
        assert_eq!(utf16_to_byte_index("𝄞", 0), 0);
        assert_eq!(utf16_to_byte_index("𝄞", 1), 4);
        assert_eq!(utf16_to_byte_index("𝄞", 2), 4);
        // The inverse mapping clamps mid-character bytes down to the char start.
        assert_eq!(byte_to_utf16_col(line, 0), 0);
        assert_eq!(byte_to_utf16_col(line, 1), 1);
        assert_eq!(byte_to_utf16_col(line, 2), 1);
        assert_eq!(byte_to_utf16_col(line, 5), 3);
        assert_eq!(byte_to_utf16_col(line, 6), 4);
        // Char indexes: 𝄞 is one scalar despite two UTF-16 units.
        assert_eq!(utf16_to_char_index(line, 0), 0);
        assert_eq!(utf16_to_char_index(line, 1), 1);
        assert_eq!(utf16_to_char_index(line, 3), 2);
        assert_eq!(utf16_to_char_index(line, 4), 3);
    }

    #[test]
    fn utf16_emoji_zwj_sequence_counts_per_scalar() {
        // 👨‍👩‍👧‍👦 is 7 scalars: each person emoji is non-BMP (2 units /
        // 4 bytes) and each ZWJ U+200D is BMP (1 unit / 3 bytes), for 11
        // UTF-16 units and 25 bytes. A UTF-16 client counts code units, not
        // grapheme clusters, and so must the server.
        let family = "👨‍👩‍👧‍👦";
        assert_eq!(family.chars().count(), 7);
        assert_eq!(family.encode_utf16().count(), 11);
        assert_eq!(family.len(), 25);
        // Well-formed boundary columns resolve to scalar starts.
        for (col, byte) in [
            (0, 0),
            (2, 4),
            (3, 7),
            (5, 11),
            (6, 14),
            (8, 18),
            (9, 21),
            (11, 25),
        ] {
            assert_eq!(utf16_to_byte_index(family, col), byte, "col {col}");
            assert_eq!(byte_to_utf16_col(family, byte), col, "byte {byte}");
        }
        // Interior (lone-surrogate) columns resolve forward, never panicking.
        assert_eq!(utf16_to_byte_index(family, 1), 4);
        assert_eq!(utf16_to_byte_index(family, 4), 11);
        assert_eq!(utf16_to_byte_index(family, 7), 18);
        assert_eq!(utf16_to_byte_index(family, 10), 25);
        assert_eq!(utf16_to_byte_index(family, 99), 25);
        // Char indexes advance per scalar.
        assert_eq!(utf16_to_char_index(family, 0), 0);
        assert_eq!(utf16_to_char_index(family, 2), 1);
        assert_eq!(utf16_to_char_index(family, 3), 2);
        assert_eq!(utf16_to_char_index(family, 11), 7);
    }

    #[test]
    fn utf16_combining_mark_counts_per_scalar_not_per_grapheme() {
        // `e` + U+0301 renders as one grapheme cluster but is two scalars of
        // one UTF-16 unit each. LSP columns count units, so the mark stays
        // individually addressable.
        let decomposed = "é";
        assert_eq!(decomposed.chars().count(), 2);
        assert_eq!(decomposed.encode_utf16().count(), 2);
        assert_eq!(utf16_to_byte_index(decomposed, 0), 0);
        assert_eq!(utf16_to_byte_index(decomposed, 1), 1);
        assert_eq!(utf16_to_byte_index(decomposed, 2), 3);
        assert_eq!(utf16_to_byte_index(decomposed, 99), 3);
        assert_eq!(byte_to_utf16_col(decomposed, 1), 1);
        assert_eq!(byte_to_utf16_col(decomposed, 3), 2);
    }

    #[test]
    fn utf16_mixed_cjk_latin_line() {
        // a, b, c, d: 1 unit / 1 byte each; 中, 文: 1 unit / 3 bytes each.
        let line = "ab中文cd";
        assert_eq!(line.encode_utf16().count(), 6);
        for (col, byte) in [(0, 0), (1, 1), (2, 2), (3, 5), (4, 8), (5, 9), (6, 10)] {
            assert_eq!(utf16_to_byte_index(line, col), byte, "col {col}");
            assert_eq!(byte_to_utf16_col(line, byte), col, "byte {byte}");
        }
        assert_eq!(utf16_to_byte_index(line, 99), 10);
        // Mid-CJK bytes clamp down to the character start.
        assert_eq!(byte_to_utf16_col(line, 3), 2);
        assert_eq!(byte_to_utf16_col(line, 6), 3);
        assert_eq!(utf16_to_char_index(line, 3), 3);
        assert_eq!(
            doc_with(line).get_range(&range(0, 2, 0, 4)),
            Some("中文".to_string())
        );
    }

    #[test]
    fn utf16_empty_lines_and_empty_text() {
        assert_eq!(utf16_to_byte_index("", 0), 0);
        assert_eq!(utf16_to_byte_index("", 7), 0);
        assert_eq!(byte_to_utf16_col("", 0), 0);
        assert_eq!(byte_to_utf16_col("", 99), 0);
        assert_eq!(utf16_to_char_index("", 5), 0);
        // `a\n\nb`: line 1 is empty at byte offset 2 with zero width.
        let doc = doc_with("a\n\nb");
        assert_eq!(doc.position_to_offset(&pos(1, 0)), Some(2));
        assert_eq!(doc.position_to_offset(&pos(1, 99)), Some(2));
        assert_eq!(doc.range_to_byte_span(&range(1, 0, 1, 0)), Some((2, 2)));
        assert_eq!(doc.get_range(&range(1, 0, 1, 5)), Some(String::new()));
        assert_eq!(doc.get_line(1), Some(""));
        // Empty content has no lines under `str::lines` semantics, so line 0
        // does not resolve; edits targeting it are skipped, not misapplied.
        assert_eq!(doc_with("").position_to_offset(&pos(0, 0)), None);
    }

    #[test]
    fn utf16_positions_at_line_boundaries() {
        // Line 0 `aé` (3 bytes, 2 units); line 1 `中𝄞` (7 bytes, 3 units,
        // starting at byte 4); line 2 `xy` starting at byte 12; 14 bytes total.
        let doc = doc_with("aé\n中𝄞\nxy");
        // Start-of-line columns.
        assert_eq!(doc.position_to_offset(&pos(0, 0)), Some(0));
        assert_eq!(doc.position_to_offset(&pos(1, 0)), Some(4));
        assert_eq!(doc.position_to_offset(&pos(2, 0)), Some(12));
        // End-of-line columns and clamping past EOL.
        assert_eq!(doc.position_to_offset(&pos(0, 2)), Some(3));
        assert_eq!(doc.position_to_offset(&pos(0, 99)), Some(3));
        assert_eq!(doc.position_to_offset(&pos(1, 3)), Some(11));
        assert_eq!(doc.position_to_offset(&pos(1, 99)), Some(11));
        // A lone-surrogate interior column normalizes forward within the line.
        assert_eq!(doc.position_to_offset(&pos(1, 2)), Some(11));
        // Out-of-range lines and inverted ranges resolve to `None`.
        assert_eq!(doc.position_to_offset(&pos(3, 0)), None);
        assert_eq!(doc.range_to_byte_span(&range(1, 1, 0, 0)), None);
        // An end line past EOF clamps to EOF; cross-line spans keep newlines.
        assert_eq!(doc.range_to_byte_span(&range(0, 0, 99, 0)), Some((0, 14)));
        assert_eq!(doc.get_range(&range(0, 1, 1, 1)), Some("é\n中".to_string()));
        assert_eq!(doc.get_range(&range(1, 1, 1, 3)), Some("𝄞".to_string()));
    }

    #[test]
    fn utf16_client_columns_match_encode_utf16_prefixes() {
        // A UTF-16 client addresses character `i` at the summed UTF-16
        // lengths of the preceding characters; the helpers must agree with
        // that addressing for every character of a mixed-script line.
        let line = "héllo世界𝄞!";
        let byte_offsets: Vec<usize> = line.char_indices().map(|(b, _)| b).collect();
        let mut col = 0u32;
        for (i, c) in line.chars().enumerate() {
            assert_eq!(utf16_to_byte_index(line, col), byte_offsets[i], "char {i}");
            assert_eq!(byte_to_utf16_col(line, byte_offsets[i]), col, "char {i}");
            assert_eq!(utf16_to_char_index(line, col), i, "char {i}");
            col += c.len_utf16() as u32;
        }
        assert_eq!(col, line.encode_utf16().count() as u32);
        assert_eq!(utf16_to_byte_index(line, col), line.len());
        assert_eq!(byte_to_utf16_col(line, line.len()), col);
        // A client-style replacement of `世界` (columns 5..7) keeps `𝄞` intact.
        let mut doc = doc_with(line);
        doc.apply_content_changes(2, vec![ranged_edit(0, 5, 0, 7, "W")]);
        assert_eq!(doc.content, "hélloW𝄞!");
    }

    #[test]
    fn utf16_edit_spanning_zwj_sequence_uses_utf16_columns() {
        // `a👨‍👩‍👧‍👦b`: the family occupies UTF-16 columns 1..12
        // (bytes 1..26). Read as byte offsets, column 12 would land
        // mid-sequence and corrupt it.
        let line = "a👨‍👩‍👧‍👦b";
        assert_eq!(line.encode_utf16().count(), 13);
        assert_eq!(utf16_to_byte_index(line, 1), 1);
        assert_eq!(utf16_to_byte_index(line, 12), 26);
        let mut doc = doc_with(line);
        doc.apply_content_changes(2, vec![ranged_edit(0, 1, 0, 12, "X")]);
        assert_eq!(doc.content, "aXb");
    }

    #[test]
    fn utf16_edit_spanning_cjk_uses_utf16_columns() {
        // `ab中文cd`: 中文 occupies UTF-16 columns 2..4 (bytes 2..8).
        let mut doc = doc_with("ab中文cd");
        doc.apply_content_changes(2, vec![ranged_edit(0, 2, 0, 4, "XY")]);
        assert_eq!(doc.content, "abXYcd");
        assert_eq!(
            doc_with("ab中文cd").get_range(&range(0, 2, 0, 4)),
            Some("中文".to_string())
        );
    }
}
