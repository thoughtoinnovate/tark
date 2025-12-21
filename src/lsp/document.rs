//! Document management for the LSP server

use dashmap::DashMap;
use tower_lsp::lsp_types::*;

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

    /// Get content in a range
    pub fn get_range(&self, range: &Range) -> Option<String> {
        let lines: Vec<&str> = self.content.lines().collect();
        let start_line = range.start.line as usize;
        let end_line = range.end.line as usize;

        if start_line >= lines.len() {
            return None;
        }

        let end_line = end_line.min(lines.len() - 1);

        if start_line == end_line {
            // Single line range
            let line = lines.get(start_line)?;
            let start_char = (range.start.character as usize).min(line.len());
            let end_char = (range.end.character as usize).min(line.len());
            Some(line[start_char..end_char].to_string())
        } else {
            // Multi-line range
            let mut result = String::new();

            // First line
            if let Some(line) = lines.get(start_line) {
                let start_char = (range.start.character as usize).min(line.len());
                result.push_str(&line[start_char..]);
                result.push('\n');
            }

            // Middle lines
            for line in lines.iter().take(end_line).skip(start_line + 1) {
                result.push_str(line);
                result.push('\n');
            }

            // Last line
            if let Some(line) = lines.get(end_line) {
                let end_char = (range.end.character as usize).min(line.len());
                result.push_str(&line[..end_char]);
            }

            Some(result)
        }
    }

    /// Get the word at a position
    pub fn get_word_at(&self, position: &Position) -> Option<String> {
        let line = self.get_line(position.line as usize)?;
        let col = position.character as usize;

        if col > line.len() {
            return None;
        }

        // Find word boundaries
        let chars: Vec<char> = line.chars().collect();

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

    /// Get position as byte offset
    pub fn position_to_offset(&self, position: &Position) -> Option<usize> {
        let mut offset = 0;
        for (i, line) in self.content.lines().enumerate() {
            if i == position.line as usize {
                let col = (position.character as usize).min(line.len());
                return Some(offset + col);
            }
            offset += line.len() + 1; // +1 for newline
        }
        None
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
            doc.version = params.text_document.version;
            // For simplicity, we use full sync - take the last change
            if let Some(change) = params.content_changes.into_iter().last() {
                doc.content = change.text;
            }
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

