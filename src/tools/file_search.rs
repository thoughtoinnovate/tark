//! File search tool using fuzzy matching

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;

/// Tool for searching files by name
pub struct FileSearchTool {
    working_dir: PathBuf,
}

impl FileSearchTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for FileSearchTool {
    fn name(&self) -> &str {
        "file_search"
    }

    fn description(&self) -> &str {
        "Search for files by name pattern. Supports glob patterns like '*.rs' or fuzzy matching."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "File name pattern to search for (glob or partial name)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional: Directory to search in (default: working directory)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 50)"
                }
            },
            "required": ["pattern"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            pattern: String,
            path: Option<String>,
            max_results: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let search_dir = params
            .path
            .map(|p| self.working_dir.join(p))
            .unwrap_or_else(|| self.working_dir.clone());
        let max_results = params.max_results.unwrap_or(50);
        let pattern = params.pattern.to_lowercase();

        let mut results = Vec::new();

        // Use the ignore crate to respect .gitignore
        let walker = WalkBuilder::new(&search_dir)
            .hidden(false)
            .git_ignore(true)
            .git_global(true)
            .git_exclude(true)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            if results.len() >= max_results {
                break;
            }

            let path = entry.path();
            if path.is_file() {
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                // Simple fuzzy matching: check if pattern chars appear in order
                let matches = if pattern.contains('*') || pattern.contains('?') {
                    // Glob-style matching
                    glob_match(&pattern, &file_name)
                } else {
                    // Fuzzy matching
                    fuzzy_match(&pattern, &file_name)
                };

                if matches {
                    if let Ok(relative) = path.strip_prefix(&self.working_dir) {
                        results.push(relative.display().to_string());
                    } else {
                        results.push(path.display().to_string());
                    }
                }
            }
        }

        if results.is_empty() {
            Ok(ToolResult::success("No files found matching the pattern."))
        } else {
            Ok(ToolResult::success(results.join("\n")))
        }
    }
}

/// Simple glob matching for * and ? wildcards
fn glob_match(pattern: &str, text: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                // If * is the last char, match everything
                if pattern_chars.peek().is_none() {
                    return true;
                }
                // Try matching the rest of the pattern at each position
                let remaining_pattern: String = pattern_chars.collect();
                while text_chars.peek().is_some() {
                    let remaining_text: String = text_chars.clone().collect();
                    if glob_match(&remaining_pattern, &remaining_text) {
                        return true;
                    }
                    text_chars.next();
                }
                return glob_match(&remaining_pattern, "");
            }
            '?' => {
                // Match any single character
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                // Match exact character
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    // Pattern consumed, text should also be consumed
    text_chars.peek().is_none()
}

/// Simple fuzzy matching - all pattern chars must appear in order
fn fuzzy_match(pattern: &str, text: &str) -> bool {
    let mut text_chars = text.chars();

    for p in pattern.chars() {
        loop {
            match text_chars.next() {
                Some(t) if t == p => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }

    true
}

/// Tool to get a high-level codebase overview without reading all files
pub struct CodebaseOverviewTool {
    working_dir: PathBuf,
}

impl CodebaseOverviewTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for CodebaseOverviewTool {
    fn name(&self) -> &str {
        "codebase_overview"
    }

    fn description(&self) -> &str {
        "Get a high-level overview of the codebase including directory structure, key files, and language breakdown. USE THIS FIRST instead of reading all files."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "depth": {
                    "type": "integer",
                    "description": "Maximum directory depth to show (default: 3)"
                },
                "include_file_counts": {
                    "type": "boolean",
                    "description": "Include file count per extension (default: true)"
                }
            }
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize, Default)]
        struct Params {
            depth: Option<usize>,
            include_file_counts: Option<bool>,
        }

        let params: Params = serde_json::from_value(params).unwrap_or_default();
        let max_depth = params.depth.unwrap_or(3);
        let include_counts = params.include_file_counts.unwrap_or(true);

        let mut output = String::new();

        // 1. Directory structure
        output.push_str("## Directory Structure\n```\n");
        let tree = build_tree(&self.working_dir, &self.working_dir, 0, max_depth);
        output.push_str(&tree);
        output.push_str("```\n\n");

        // 2. Key files (README, config files, entry points)
        output.push_str("## Key Files Found\n");
        let key_files = find_key_files(&self.working_dir);
        if key_files.is_empty() {
            output.push_str("No standard key files found.\n");
        } else {
            for (category, files) in key_files {
                output.push_str(&format!("**{}:**\n", category));
                for file in files {
                    output.push_str(&format!("- {}\n", file));
                }
            }
        }
        output.push('\n');

        // 3. Language breakdown
        if include_counts {
            output.push_str("## Language Breakdown\n");
            let stats = count_files_by_extension(&self.working_dir);
            let mut stats_vec: Vec<_> = stats.into_iter().collect();
            stats_vec.sort_by(|a, b| b.1.cmp(&a.1));

            for (ext, count) in stats_vec.iter().take(15) {
                let lang = extension_to_language(ext);
                output.push_str(&format!("- {} ({}): {} files\n", lang, ext, count));
            }

            let total: usize = stats_vec.iter().map(|(_, c)| c).sum();
            output.push_str(&format!("\n**Total:** {} files\n", total));
        }

        // 4. Suggested next steps
        output.push_str("\n## Suggested Next Steps\n");
        output.push_str("1. Read the README if present for project context\n");
        output.push_str("2. Use `grep` to search for specific patterns/functions\n");
        output.push_str("3. Read specific files of interest (don't read all files!)\n");

        Ok(ToolResult::success(output))
    }
}

#[allow(clippy::only_used_in_recursion)]
fn build_tree(root: &PathBuf, current: &PathBuf, depth: usize, max_depth: usize) -> String {
    if depth > max_depth {
        return String::new();
    }

    let mut output = String::new();
    let indent = "  ".repeat(depth);

    let dir_name = if depth == 0 {
        ".".to_string()
    } else {
        current
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?")
            .to_string()
    };

    output.push_str(&format!("{}📁 {}/\n", indent, dir_name));

    if let Ok(entries) = std::fs::read_dir(current) {
        let mut dirs = Vec::new();
        let mut files = Vec::new();

        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();

            // Skip hidden files and common ignore patterns
            if name.starts_with('.')
                || name == "node_modules"
                || name == "target"
                || name == "dist"
                || name == "build"
                || name == "__pycache__"
                || name == "venv"
                || name == ".git"
            {
                continue;
            }

            if path.is_dir() {
                dirs.push(path);
            } else {
                files.push(name);
            }
        }

        // Show up to 10 files at this level
        for file in files.iter().take(10) {
            output.push_str(&format!("{}  📄 {}\n", indent, file));
        }
        if files.len() > 10 {
            output.push_str(&format!(
                "{}  ... and {} more files\n",
                indent,
                files.len() - 10
            ));
        }

        // Recurse into directories
        dirs.sort();
        for dir in dirs {
            output.push_str(&build_tree(root, &dir, depth + 1, max_depth));
        }
    }

    output
}

fn find_key_files(dir: &PathBuf) -> Vec<(&'static str, Vec<String>)> {
    let key_patterns = vec![
        (
            "Documentation",
            vec![
                "README.md",
                "README",
                "readme.md",
                "CHANGELOG.md",
                "CONTRIBUTING.md",
            ],
        ),
        (
            "Configuration",
            vec![
                "package.json",
                "Cargo.toml",
                "pyproject.toml",
                "go.mod",
                "pom.xml",
                "build.gradle",
                "Makefile",
            ],
        ),
        (
            "Entry Points",
            vec![
                "main.rs", "lib.rs", "main.py", "app.py", "index.js", "index.ts", "main.go",
                "App.tsx", "App.jsx",
            ],
        ),
    ];

    let mut results = Vec::new();

    for (category, patterns) in key_patterns {
        let mut found = Vec::new();
        for pattern in patterns {
            // Check root and src directories
            let paths = vec![dir.join(pattern), dir.join("src").join(pattern)];

            for path in paths {
                if path.exists() {
                    if let Ok(rel) = path.strip_prefix(dir) {
                        found.push(rel.display().to_string());
                    }
                }
            }
        }
        if !found.is_empty() {
            results.push((category, found));
        }
    }

    results
}

fn count_files_by_extension(dir: &PathBuf) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();

    let walker = WalkBuilder::new(dir).hidden(true).git_ignore(true).build();

    for entry in walker.filter_map(|e| e.ok()) {
        if entry.path().is_file() {
            let ext = entry
                .path()
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("(no ext)")
                .to_lowercase();
            *counts.entry(ext).or_insert(0) += 1;
        }
    }

    counts
}

fn extension_to_language(ext: &str) -> &'static str {
    match ext {
        "rs" => "Rust",
        "py" => "Python",
        "js" => "JavaScript",
        "ts" => "TypeScript",
        "tsx" | "jsx" => "React",
        "go" => "Go",
        "java" => "Java",
        "c" | "h" => "C",
        "cpp" | "cc" | "hpp" => "C++",
        "rb" => "Ruby",
        "php" => "PHP",
        "swift" => "Swift",
        "kt" => "Kotlin",
        "lua" => "Lua",
        "sh" | "bash" => "Shell",
        "sql" => "SQL",
        "html" => "HTML",
        "css" | "scss" | "sass" => "CSS",
        "json" => "JSON",
        "yaml" | "yml" => "YAML",
        "toml" => "TOML",
        "md" => "Markdown",
        "(no ext)" => "No extension",
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "lib.rs"));
        assert!(!glob_match("*.rs", "main.py"));
        assert!(glob_match("test_*.rs", "test_main.rs"));
        assert!(glob_match("?ain.rs", "main.rs"));
    }

    #[test]
    fn test_fuzzy_match() {
        assert!(fuzzy_match("mr", "main.rs"));
        assert!(fuzzy_match("mn", "main.rs"));
        assert!(!fuzzy_match("xyz", "main.rs"));
    }
}
