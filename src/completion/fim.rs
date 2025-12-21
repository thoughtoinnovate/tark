//! Fill-in-Middle (FIM) prompt building

use std::path::Path;

/// Builder for FIM prompts
pub struct FimBuilder {
    context_lines_before: usize,
    context_lines_after: usize,
}

impl Default for FimBuilder {
    fn default() -> Self {
        Self {
            context_lines_before: 50,
            context_lines_after: 20,
        }
    }
}

impl FimBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_context_lines(mut self, before: usize, after: usize) -> Self {
        self.context_lines_before = before;
        self.context_lines_after = after;
        self
    }

    /// Split document content into prefix and suffix at the cursor position
    pub fn split_at_cursor(&self, content: &str, line: usize, col: usize) -> (String, String) {
        let lines: Vec<&str> = content.lines().collect();

        if line >= lines.len() {
            return (content.to_string(), String::new());
        }

        // Calculate start line for prefix (with context limit)
        let start_line = line.saturating_sub(self.context_lines_before);

        // Calculate end line for suffix (with context limit)
        let end_line = (line + self.context_lines_after + 1).min(lines.len());

        // Build prefix
        let mut prefix = String::new();
        for (i, line_content) in lines.iter().enumerate().skip(start_line).take(line - start_line)
        {
            prefix.push_str(line_content);
            prefix.push('\n');
        }

        // Add the current line up to cursor
        if let Some(current_line) = lines.get(line) {
            let col = col.min(current_line.len());
            prefix.push_str(&current_line[..col]);
        }

        // Build suffix
        let mut suffix = String::new();
        if let Some(current_line) = lines.get(line) {
            let col = col.min(current_line.len());
            suffix.push_str(&current_line[col..]);
            suffix.push('\n');
        }

        // Add remaining lines for context
        for line_content in lines.iter().skip(line + 1).take(end_line - line - 1) {
            suffix.push_str(line_content);
            suffix.push('\n');
        }

        (prefix, suffix)
    }

    /// Detect language from file extension
    pub fn detect_language(file_path: &Path) -> &'static str {
        match file_path.extension().and_then(|e| e.to_str()) {
            Some("rs") => "rust",
            Some("py") => "python",
            Some("js") => "javascript",
            Some("ts") => "typescript",
            Some("tsx") => "typescript",
            Some("jsx") => "javascript",
            Some("go") => "go",
            Some("java") => "java",
            Some("c") => "c",
            Some("cpp") | Some("cc") | Some("cxx") => "cpp",
            Some("h") | Some("hpp") => "cpp",
            Some("rb") => "ruby",
            Some("php") => "php",
            Some("swift") => "swift",
            Some("kt") | Some("kts") => "kotlin",
            Some("scala") => "scala",
            Some("cs") => "csharp",
            Some("fs") => "fsharp",
            Some("lua") => "lua",
            Some("vim") => "vim",
            Some("sh") | Some("bash") | Some("zsh") => "shell",
            Some("json") => "json",
            Some("yaml") | Some("yml") => "yaml",
            Some("toml") => "toml",
            Some("xml") => "xml",
            Some("html") => "html",
            Some("css") => "css",
            Some("scss") | Some("sass") => "scss",
            Some("sql") => "sql",
            Some("md") | Some("markdown") => "markdown",
            _ => "text",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_at_cursor_middle() {
        let content = "line 0\nline 1\nline 2\nline 3\nline 4";
        let builder = FimBuilder::new();

        let (prefix, suffix) = builder.split_at_cursor(content, 2, 3);

        assert_eq!(prefix, "line 0\nline 1\nlin");
        assert_eq!(suffix, "e 2\nline 3\nline 4\n");
    }

    #[test]
    fn test_split_at_cursor_start() {
        let content = "line 0\nline 1\nline 2";
        let builder = FimBuilder::new();

        let (prefix, suffix) = builder.split_at_cursor(content, 0, 0);

        assert_eq!(prefix, "");
        assert_eq!(suffix, "line 0\nline 1\nline 2\n");
    }

    #[test]
    fn test_split_at_cursor_end() {
        let content = "line 0\nline 1\nline 2";
        let builder = FimBuilder::new();

        let (prefix, suffix) = builder.split_at_cursor(content, 2, 6);

        assert_eq!(prefix, "line 0\nline 1\nline 2");
        assert_eq!(suffix, "\n");
    }
}

