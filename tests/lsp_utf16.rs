//! LSP UTF-16 multibyte round-trip property tests (requirement R9, scenario S23).
//!
//! LSP clients address text with UTF-16 code-unit columns. The helpers in
//! `src/lsp/document.rs` translate those columns to byte offsets into document
//! text. The `lsp::document` module is private, so this integration test
//! includes it by path to exercise the real helpers (no logic is duplicated
//! and no production code is changed). Expected values come from an
//! independent oracle (`str::encode_utf16` / `char::len_utf16`), i.e. the same
//! arithmetic a UTF-16 client performs when it sends a position.

#[allow(dead_code)]
#[path = "../src/lsp/document.rs"]
mod document_under_test;

use document_under_test::{
    byte_to_utf16_col, position_to_byte_offset, utf16_to_byte_index, utf16_to_char_index, Document,
};
use proptest::prelude::*;
use std::collections::HashSet;
use tower_lsp::lsp_types::{Position, Range, TextDocumentContentChangeEvent, Url};

/// Glyph pool for generated lines: ASCII plus BMP multibyte (1 UTF-16 unit),
/// non-BMP (2 units via surrogate pair), and joiners/marks that form
/// multi-scalar grapheme clusters.
const GLYPHS: &[char] = &[
    'a', 'z', 'A', '0', ' ', // ASCII baseline
    'é', 'ü', 'ß', 'ñ', 'Ω', // BMP multibyte (1 unit, 2 bytes)
    '中', '界', 'あ', // CJK (1 unit, 3 bytes)
    '𝄞', '😀', '👨', '🌍', // non-BMP (2 units, 4 bytes)
    '\u{200D}', '\u{301}', '\u{FE0F}', // ZWJ / combining mark / variation selector
];

/// Random single-line multibyte text (never contains `\n` or `\r`).
fn multibyte_line() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(GLYPHS), 0..24)
        .prop_map(|chars| chars.into_iter().collect())
}

/// Random multi-line document; lines join with `\n` (no `\r` is generated).
fn multiline_doc() -> impl Strategy<Value = String> {
    prop::collection::vec(multibyte_line(), 0..5).prop_map(|lines| lines.join("\n"))
}

/// `(utf16_col, byte_offset)` of every character boundary in `s`, including
/// the `(total_units, len)` end boundary.
fn char_boundaries(s: &str) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut col = 0u32;
    for (byte, c) in s.char_indices() {
        out.push((col, byte));
        col += c.len_utf16() as u32;
    }
    out.push((col, s.len()));
    out
}

/// Line framing matching the server: `\n`-separated with no phantom trailing
/// line (mirrors `str::lines` semantics used by `Document::get_line`).
/// Per-line columns are still verified against the independent UTF-16 oracle.
fn server_lines(content: &str) -> Vec<(usize, &str)> {
    if content.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0usize;
    for piece in content.split_inclusive('\n') {
        let text = piece.strip_suffix('\n').unwrap_or(piece);
        out.push((start, text));
        start += piece.len();
    }
    out
}

fn test_doc(content: &str) -> Document {
    Document {
        uri: Url::parse("file:///prop.rs").expect("static test URL parses"),
        language_id: "rust".to_string(),
        version: 1,
        content: content.to_string(),
    }
}

proptest! {
    /// **Validates: R9/S23** — every character boundary of any multibyte line
    /// round-trips through UTF-16 columns to the same byte offset and back,
    /// and past-EOL inputs clamp to the end of the line.
    #[test]
    fn prop_utf16_boundaries_round_trip(line in multibyte_line()) {
        let bounds = char_boundaries(&line);
        let (total_units, total_bytes) = bounds[bounds.len() - 1];
        prop_assert_eq!(total_units, line.encode_utf16().count() as u32);
        prop_assert_eq!(total_bytes, line.len());
        for (i, (col, byte)) in bounds.iter().enumerate() {
            prop_assert_eq!(utf16_to_byte_index(&line, *col), *byte, "col {}", col);
            prop_assert_eq!(byte_to_utf16_col(&line, *byte), *col, "byte {}", byte);
            if i < bounds.len() - 1 {
                prop_assert_eq!(utf16_to_char_index(&line, *col), i, "char {}", i);
            }
        }
        prop_assert_eq!(utf16_to_byte_index(&line, total_units), line.len());
        prop_assert_eq!(byte_to_utf16_col(&line, line.len()), total_units);
        prop_assert_eq!(
            utf16_to_byte_index(&line, total_units.saturating_add(10)),
            line.len()
        );
        prop_assert_eq!(
            byte_to_utf16_col(&line, line.len().saturating_add(10)),
            total_units
        );
    }

    /// **Validates: R9/S23** — every UTF-16 column (including lone-surrogate
    /// interiors and past-EOL columns) resolves to a character boundary.
    /// Well-formed columns round-trip exactly; interiors normalize forward to
    /// the next boundary; past-EOL columns clamp to the end of the line.
    #[test]
    fn prop_utf16_columns_always_land_on_char_boundaries(line in multibyte_line()) {
        let bounds = char_boundaries(&line);
        let total = bounds[bounds.len() - 1].0;
        let well_formed: HashSet<u32> = bounds.iter().map(|(col, _)| *col).collect();
        for col in 0..=total.saturating_add(3) {
            let byte = utf16_to_byte_index(&line, col);
            prop_assert!(line.is_char_boundary(byte), "col {} -> byte {}", col, byte);
            prop_assert!(byte <= line.len(), "col {} -> byte {}", col, byte);
            let back = byte_to_utf16_col(&line, byte);
            if well_formed.contains(&col) {
                prop_assert_eq!(back, col, "col {}", col);
            } else if col < total {
                prop_assert!(back > col, "interior col {} normalized to {}", col, back);
                prop_assert!(well_formed.contains(&back), "interior col {}", col);
            } else {
                prop_assert_eq!(back, total, "past-EOL col {}", col);
            }
        }
    }

    /// **Validates: R9/S23** — byte indexes inside a multibyte character clamp
    /// down to that character's start instead of splitting it.
    #[test]
    fn prop_mid_char_bytes_clamp_to_char_start(line in multibyte_line()) {
        for i in 0..line.len() {
            if line.is_char_boundary(i) {
                continue;
            }
            let mut start = i;
            while !line.is_char_boundary(start) {
                start -= 1;
            }
            prop_assert_eq!(
                byte_to_utf16_col(&line, i),
                byte_to_utf16_col(&line, start),
                "byte {}",
                i
            );
        }
    }

    /// **Validates: R9/S23** — positions in any multi-line document resolve to
    /// the byte offsets a UTF-16 client means: line framing plus per-character
    /// `encode_utf16` prefix sums. Out-of-range lines resolve to `None`.
    #[test]
    fn prop_positions_match_utf16_client_columns(doc in multiline_doc()) {
        for (line_no, (start, text)) in server_lines(&doc).iter().enumerate() {
            for (col, byte_in_line) in char_boundaries(text) {
                let pos = Position {
                    line: line_no as u32,
                    character: col,
                };
                prop_assert_eq!(
                    position_to_byte_offset(&doc, &pos),
                    Some(start + byte_in_line),
                    "line {} col {}",
                    line_no,
                    col
                );
            }
            // Past-EOL columns clamp to the end of the line.
            let total = text.encode_utf16().count() as u32;
            prop_assert_eq!(
                position_to_byte_offset(
                    &doc,
                    &Position {
                        line: line_no as u32,
                        character: total.saturating_add(7),
                    }
                ),
                Some(start + text.len()),
                "line {} past EOL",
                line_no
            );
        }
        prop_assert_eq!(
            position_to_byte_offset(
                &doc,
                &Position {
                    line: server_lines(&doc).len() as u32 + 3,
                    character: 0,
                }
            ),
            None,
            "out-of-range line"
        );
    }

    /// **Validates: R9/S23** — an incremental edit addressed with UTF-16
    /// columns splices exactly the spanned characters: the result equals a
    /// character-aligned splice and stays valid UTF-8.
    #[test]
    fn prop_incremental_edit_on_utf16_span_preserves_text(
        line in multibyte_line(),
        replacement in "[a-z ]{0,8}",
        a in 0usize..32,
        b in 0usize..32,
    ) {
        prop_assume!(!line.is_empty());
        let bounds = char_boundaries(&line);
        let n = bounds.len();
        let (mut i, mut j) = (a % n, b % n);
        if i > j {
            std::mem::swap(&mut i, &mut j);
        }
        let (start_col, start_byte) = bounds[i];
        let (end_col, end_byte) = bounds[j];
        let mut doc = test_doc(&line);
        doc.apply_content_changes(
            2,
            vec![TextDocumentContentChangeEvent {
                range: Some(Range {
                    start: Position {
                        line: 0,
                        character: start_col,
                    },
                    end: Position {
                        line: 0,
                        character: end_col,
                    },
                }),
                range_length: None,
                text: replacement.clone(),
            }],
        );
        let mut expected = line.clone();
        expected.replace_range(start_byte..end_byte, &replacement);
        prop_assert_eq!(doc.content, expected);
    }
}

/// The ZWJ family emoji is one grapheme cluster but 7 scalars / 11 UTF-16
/// units / 25 bytes; totals must match the client-side UTF-16 encoding.
#[test]
fn zwj_family_totals_match_utf16_client_encoding() {
    let family = "👨‍👩‍👧‍👦";
    assert_eq!(family.chars().count(), 7);
    assert_eq!(family.encode_utf16().count(), 11);
    assert_eq!(family.len(), 25);
    let bounds = char_boundaries(family);
    assert_eq!(bounds.len(), 8);
    assert_eq!(bounds[bounds.len() - 1], (11, 25));
    for (col, byte) in bounds {
        assert_eq!(utf16_to_byte_index(family, col), byte);
        assert_eq!(byte_to_utf16_col(family, byte), col);
    }
}

/// Empty text has no lines under `str::lines` semantics, so line 0 does not
/// resolve; an empty line inside a document resolves with zero width.
#[test]
fn empty_lines_resolve_with_zero_width() {
    assert_eq!(utf16_to_byte_index("", 4), 0);
    assert_eq!(byte_to_utf16_col("", 4), 0);
    let doc = test_doc("a\n\nb");
    let empty = Position {
        line: 1,
        character: 0,
    };
    assert_eq!(position_to_byte_offset(&doc.content, &empty), Some(2));
    assert_eq!(
        doc.get_range(&Range {
            start: empty,
            end: Position {
                line: 1,
                character: 9,
            },
        }),
        Some(String::new())
    );
    assert_eq!(
        position_to_byte_offset(
            "",
            &Position {
                line: 0,
                character: 0,
            }
        ),
        None
    );
}
