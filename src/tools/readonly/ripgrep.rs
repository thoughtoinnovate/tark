//! Ripgrep-based search tool using grep-searcher for fast file content search.
//!
//! This replaces the basic grep tool with a much faster implementation
//! using the same libraries that power ripgrep.

use crate::tools::risk::RiskLevel;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use grep_regex::RegexMatcherBuilder;
use grep_searcher::sinks::Lossy;
use grep_searcher::SearcherBuilder;
use ignore::types::TypesBuilder;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Tool for searching content within files using ripgrep's grep-searcher.
pub struct RipgrepTool {
    working_dir: PathBuf,
}

impl RipgrepTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for RipgrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Fast regex search for patterns in file contents. Uses ripgrep for speed. Returns matching lines with file paths and line numbers."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Regex pattern to search for"
                },
                "path": {
                    "type": "string",
                    "description": "Optional: File or directory to search in (default: working directory)"
                },
                "file_type": {
                    "type": "string",
                    "description": "Optional: Filter by file type (e.g., 'rust', 'ts', 'py', 'js', 'go', 'java')"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional: Glob pattern for filenames (e.g., '*.rs', 'test_*.py')"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Optional: Case sensitive search (default: false)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of matching lines to return (default: 100)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Lines of context before and after each match (default: 0)"
                }
            },
            "required": ["pattern"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            pattern: String,
            path: Option<String>,
            file_type: Option<String>,
            file_pattern: Option<String>,
            case_sensitive: Option<bool>,
            max_results: Option<usize>,
            context_lines: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let search_path = params
            .path
            .map(|p| self.working_dir.join(p))
            .unwrap_or_else(|| self.working_dir.clone());
        let case_sensitive = params.case_sensitive.unwrap_or(false);
        let max_results = params.max_results.unwrap_or(100);
        let context_lines = params.context_lines.unwrap_or(0);

        // Build the regex matcher
        let matcher = RegexMatcherBuilder::new()
            .case_insensitive(!case_sensitive)
            .build(&params.pattern)
            .map_err(|e| anyhow::anyhow!("Invalid regex pattern: {}", e))?;

        // Build the searcher with context
        let searcher = SearcherBuilder::new()
            .before_context(context_lines)
            .after_context(context_lines)
            .line_number(true)
            .build();

        // Collect results thread-safely
        let results: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let match_count = Arc::new(Mutex::new(0usize));

        // Handle single file case
        if search_path.is_file() {
            let relative_path = search_path
                .strip_prefix(&self.working_dir)
                .unwrap_or(&search_path);

            search_file(
                &matcher,
                &searcher,
                &search_path,
                &relative_path.display().to_string(),
                &results,
                &match_count,
                max_results,
                context_lines > 0,
            )?;
        } else {
            // Build file type filter if specified
            let mut types_builder = TypesBuilder::new();
            types_builder.add_defaults();

            if let Some(ft) = normalize_file_type(params.file_type.as_deref()) {
                // Select only this file type
                types_builder.select(&ft);
            }

            let types = types_builder.build()?;

            // Build the walker
            let mut walker_builder = WalkBuilder::new(&search_path);
            walker_builder
                .hidden(false)
                .git_ignore(true)
                .git_global(true)
                .git_exclude(true)
                .types(types);

            let walker = walker_builder.build();

            for entry in walker.filter_map(|e| e.ok()) {
                // Check if we've hit max results
                if *match_count.lock().unwrap() >= max_results {
                    break;
                }

                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                // Check file pattern if specified
                if let Some(ref fp) = params.file_pattern {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !glob_match(fp, file_name) {
                        continue;
                    }
                }

                let relative_path = path.strip_prefix(&self.working_dir).unwrap_or(path);

                search_file(
                    &matcher,
                    &searcher,
                    path,
                    &relative_path.display().to_string(),
                    &results,
                    &match_count,
                    max_results,
                    context_lines > 0,
                )?;
            }
        }

        let results = results.lock().unwrap();
        let count = *match_count.lock().unwrap();

        if results.is_empty() {
            Ok(ToolResult::success("No matches found."))
        } else {
            let truncated = count >= max_results;
            let mut output = results.join("\n");
            if truncated {
                output.push_str(&format!("\n... (results truncated at {})", max_results));
            }
            Ok(ToolResult::success(output))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn search_file(
    matcher: &grep_regex::RegexMatcher,
    searcher: &grep_searcher::Searcher,
    path: &std::path::Path,
    relative_path: &str,
    results: &Arc<Mutex<Vec<String>>>,
    match_count: &Arc<Mutex<usize>>,
    max_results: usize,
    has_context: bool,
) -> Result<()> {
    let file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };

    let results_clone = results.clone();
    let match_count_clone = match_count.clone();
    let rel_path = relative_path.to_string();

    let mut current_file_matches: Vec<String> = Vec::new();
    let mut in_context = false;

    // Use a mutable searcher clone for this file
    let mut searcher = searcher.clone();

    searcher.search_file(
        matcher,
        &file,
        Lossy(|line_num, line| {
            let mut count = match_count_clone.lock().unwrap();
            if *count >= max_results {
                return Ok(false); // Stop searching
            }

            if has_context {
                // For context mode, format differently
                if !in_context {
                    current_file_matches.push(format!("{}:", rel_path));
                    in_context = true;
                }
                current_file_matches.push(format!("{:6}| {}", line_num, line.trim_end()));
            } else {
                // Simple format: file:line:content
                current_file_matches.push(format!("{}:{}:{}", rel_path, line_num, line.trim_end()));
            }
            *count += 1;
            Ok(true)
        }),
    )?;

    if !current_file_matches.is_empty() {
        let mut results = results_clone.lock().unwrap();
        if has_context && in_context {
            current_file_matches.push(String::new()); // Add blank line after context block
        }
        results.extend(current_file_matches);
    }

    Ok(())
}

/// Simple glob matching for file patterns
fn glob_match(pattern: &str, text: &str) -> bool {
    // Support simple brace expansion: `*.{rs,lua,toml,md}`
    // This is a common pattern produced by LLMs and users.
    if let Some((prefix, alts, suffix)) = split_first_brace_group(pattern) {
        for alt in alts {
            let expanded = format!("{prefix}{alt}{suffix}");
            if glob_match(&expanded, text) {
                return true;
            }
        }
        return false;
    }

    let pattern = pattern.to_lowercase();
    let text = text.to_lowercase();
    let mut pattern_chars = pattern.chars().peekable();
    let mut text_chars = text.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                if pattern_chars.peek().is_none() {
                    return true;
                }
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
                if text_chars.next().is_none() {
                    return false;
                }
            }
            c => {
                if text_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    text_chars.peek().is_none()
}

/// Normalize `file_type` to an ignore/ripgrep type name.
///
/// Returns `None` for empty/whitespace-only strings.
fn normalize_file_type(ft: Option<&str>) -> Option<String> {
    let ft = ft?.trim();
    if ft.is_empty() {
        return None;
    }

    // Common aliases seen in LLM outputs / user inputs
    let lowered = ft.to_lowercase();
    let normalized = match lowered.as_str() {
        "rs" | ".rs" => "rust",
        "py" | ".py" => "python",
        "js" | ".js" => "javascript",
        "ts" | ".ts" => "ts",
        other => other,
    };

    Some(normalized.to_string())
}

/// Split the first `{a,b,c}` brace group in a glob pattern.
///
/// Returns `(prefix, alternatives, suffix)` if a brace group exists, otherwise `None`.
fn split_first_brace_group(pattern: &str) -> Option<(String, Vec<String>, String)> {
    let start = pattern.find('{')?;
    let end = pattern[start..].find('}')? + start;
    if end <= start + 1 {
        return None;
    }

    let prefix = pattern[..start].to_string();
    let suffix = pattern[end + 1..].to_string();
    let inner = &pattern[start + 1..end];
    let alts: Vec<String> = inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    if alts.is_empty() {
        None
    } else {
        Some((prefix, alts, suffix))
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
        assert!(glob_match("test_*.py", "test_main.py"));
        assert!(!glob_match("test_*.py", "main.py"));
        assert!(glob_match("*", "anything"));
    }

    #[test]
    fn test_glob_match_brace_expansion() {
        assert!(glob_match("*.{rs,lua,toml,md}", "main.rs"));
        assert!(glob_match("*.{rs,lua,toml,md}", "init.lua"));
        assert!(!glob_match("*.{rs,lua,toml,md}", "main.py"));
    }

    #[test]
    fn test_normalize_file_type() {
        assert_eq!(normalize_file_type(None), None);
        assert_eq!(normalize_file_type(Some("")), None);
        assert_eq!(normalize_file_type(Some("   ")), None);
        assert_eq!(normalize_file_type(Some("rs")).as_deref(), Some("rust"));
        assert_eq!(normalize_file_type(Some(".rs")).as_deref(), Some("rust"));
        assert_eq!(normalize_file_type(Some("py")).as_deref(), Some("python"));
        assert_eq!(normalize_file_type(Some("rust")).as_deref(), Some("rust"));
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = RipgrepTool::new(PathBuf::from("."));
        assert_eq!(tool.name(), "grep");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(!tool.description().is_empty());

        let params = tool.parameters();
        assert!(params.get("properties").is_some());
    }
}
