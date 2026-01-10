//! Grep tool for searching file contents

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use ignore::WalkBuilder;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

/// Tool for searching content within files
pub struct GrepTool {
    working_dir: PathBuf,
}

impl GrepTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &str {
        "grep"
    }

    fn description(&self) -> &str {
        "Search for a pattern in file contents. Returns matching lines with file paths and line numbers."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Text pattern to search for (case-insensitive by default)"
                },
                "path": {
                    "type": "string",
                    "description": "Optional: File or directory to search in (default: working directory)"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional: Only search files matching this glob (e.g., '*.rs')"
                },
                "case_sensitive": {
                    "type": "boolean",
                    "description": "Optional: Whether to match case-sensitively (default: false)"
                },
                "max_results": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default: 100)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Number of context lines before and after match (default: 0)"
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

        let pattern = if case_sensitive {
            params.pattern.clone()
        } else {
            params.pattern.to_lowercase()
        };

        let mut results = Vec::new();

        // Handle single file vs directory
        if search_path.is_file() {
            search_file(
                &search_path,
                &pattern,
                case_sensitive,
                context_lines,
                &self.working_dir,
                &mut results,
                max_results,
            )?;
        } else {
            let walker = WalkBuilder::new(&search_path)
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
                if !path.is_file() {
                    continue;
                }

                // Check file pattern if specified
                if let Some(ref file_pattern) = params.file_pattern {
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if !glob_match(file_pattern, file_name) {
                        continue;
                    }
                }

                // Skip binary files
                if is_binary(path) {
                    continue;
                }

                search_file(
                    path,
                    &pattern,
                    case_sensitive,
                    context_lines,
                    &self.working_dir,
                    &mut results,
                    max_results,
                )?;
            }
        }

        if results.is_empty() {
            Ok(ToolResult::success("No matches found."))
        } else {
            let truncated = results.len() >= max_results;
            let mut output = results.join("\n");
            if truncated {
                output.push_str(&format!("\n... (results truncated at {})", max_results));
            }
            Ok(ToolResult::success(output))
        }
    }
}

fn search_file(
    path: &std::path::Path,
    pattern: &str,
    case_sensitive: bool,
    context_lines: usize,
    working_dir: &std::path::Path,
    results: &mut Vec<String>,
    max_results: usize,
) -> Result<()> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => return Ok(()),
    };

    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

    let relative_path = path
        .strip_prefix(working_dir)
        .unwrap_or(path)
        .display()
        .to_string();

    for (i, line) in lines.iter().enumerate() {
        if results.len() >= max_results {
            break;
        }

        let matches = if case_sensitive {
            line.contains(pattern)
        } else {
            line.to_lowercase().contains(pattern)
        };

        if matches {
            if context_lines > 0 {
                let start = i.saturating_sub(context_lines);
                let end = (i + context_lines + 1).min(lines.len());

                results.push(format!("{}:{}:", relative_path, i + 1));
                for (j, line_content) in lines.iter().enumerate().take(end).skip(start) {
                    let prefix = if j == i { ">" } else { " " };
                    results.push(format!("{} {:4}| {}", prefix, j + 1, line_content));
                }
                results.push(String::new());
            } else {
                results.push(format!("{}:{}:{}", relative_path, i + 1, line));
            }
        }
    }

    Ok(())
}

/// Check if a file appears to be binary
fn is_binary(path: &std::path::Path) -> bool {
    // Check by extension first
    let binary_extensions = [
        "exe", "dll", "so", "dylib", "bin", "obj", "o", "a", "lib", "png", "jpg", "jpeg", "gif",
        "bmp", "ico", "pdf", "zip", "tar", "gz", "7z", "rar", "mp3", "mp4", "avi", "mov", "wasm",
    ];

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        if binary_extensions.contains(&ext.to_lowercase().as_str()) {
            return true;
        }
    }

    // Check first few bytes for null characters
    if let Ok(file) = File::open(path) {
        let mut reader = BufReader::new(file);
        let mut buffer = [0u8; 512];
        if let Ok(n) = std::io::Read::read(&mut reader, &mut buffer) {
            for &byte in &buffer[..n] {
                if byte == 0 {
                    return true;
                }
            }
        }
    }

    false
}

/// Simple glob matching
fn glob_match(pattern: &str, text: &str) -> bool {
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

/// Tool for finding all references to a symbol and tracing code flow
pub struct FindReferencesTool {
    working_dir: PathBuf,
}

impl FindReferencesTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for FindReferencesTool {
    fn name(&self) -> &str {
        "find_references"
    }

    fn description(&self) -> &str {
        "Find all references to a function, type, or variable. Shows definition and all usages with context. Great for understanding code flow."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "symbol": {
                    "type": "string",
                    "description": "The function, type, or variable name to find (e.g., 'handle_chat', 'UserService')"
                },
                "file_pattern": {
                    "type": "string",
                    "description": "Optional: Only search files matching this pattern (e.g., '*.rs', '*.ts')"
                },
                "include_definition": {
                    "type": "boolean",
                    "description": "Whether to show the full definition/implementation (default: true)"
                },
                "context_lines": {
                    "type": "integer",
                    "description": "Lines of context around each reference (default: 2)"
                }
            },
            "required": ["symbol"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            symbol: String,
            file_pattern: Option<String>,
            include_definition: Option<bool>,
            context_lines: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let include_definition = params.include_definition.unwrap_or(true);
        let _context_lines = params.context_lines.unwrap_or(2);
        let symbol = &params.symbol;

        let mut output = String::new();
        output.push_str(&format!("# References to `{}`\n\n", symbol));

        // Patterns that likely indicate a definition
        let definition_patterns = vec![
            format!("fn {}(", symbol),        // Rust function
            format!("fn {} (", symbol),       // Rust function with space
            format!("pub fn {}(", symbol),    // Rust pub function
            format!("async fn {}(", symbol),  // Rust async function
            format!("struct {} ", symbol),    // Rust struct
            format!("enum {} ", symbol),      // Rust enum
            format!("trait {} ", symbol),     // Rust trait
            format!("impl {} ", symbol),      // Rust impl
            format!("type {} ", symbol),      // Rust type alias
            format!("const {}:", symbol),     // Rust const
            format!("let {}:", symbol),       // Rust let
            format!("let {} =", symbol),      // Rust let
            format!("function {}(", symbol),  // JS/TS function
            format!("const {} =", symbol),    // JS/TS const
            format!("class {} ", symbol),     // JS/TS/Python class
            format!("def {}(", symbol),       // Python function
            format!("interface {} ", symbol), // TS interface
        ];

        let mut definitions: Vec<(String, usize, Vec<String>)> = Vec::new();
        let mut usages: Vec<(String, usize, String)> = Vec::new();

        // Walk the directory
        let walker = WalkBuilder::new(&self.working_dir)
            .hidden(false)
            .git_ignore(true)
            .build();

        for entry in walker.filter_map(|e| e.ok()) {
            let path = entry.path();
            if !path.is_file() || is_binary(path) {
                continue;
            }

            // Check file pattern
            if let Some(ref fp) = params.file_pattern {
                let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !glob_match(fp, file_name) {
                    continue;
                }
            }

            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let reader = BufReader::new(file);
            let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();

            let relative_path = path
                .strip_prefix(&self.working_dir)
                .unwrap_or(path)
                .display()
                .to_string();

            for (i, line) in lines.iter().enumerate() {
                if !line.contains(symbol) {
                    continue;
                }

                // Check if this is a definition
                let is_def = definition_patterns.iter().any(|p| line.contains(p));

                if is_def && include_definition {
                    // Capture definition with context
                    let start = i;
                    let mut end = i + 1;

                    // Try to capture the full definition (look for closing brace)
                    let mut brace_count = 0;
                    for (j, l) in lines.iter().enumerate().skip(i) {
                        brace_count += l.matches('{').count() as i32;
                        brace_count -= l.matches('}').count() as i32;
                        end = j + 1;
                        if brace_count == 0 && j > i {
                            break;
                        }
                        if j > i + 50 {
                            // Limit definition size
                            end = i + 20;
                            break;
                        }
                    }

                    let def_lines: Vec<String> = lines[start..end.min(lines.len())].to_vec();
                    definitions.push((relative_path.clone(), i + 1, def_lines));
                } else if !is_def {
                    // This is a usage
                    usages.push((relative_path.clone(), i + 1, line.trim().to_string()));
                }
            }
        }

        // Format output
        if definitions.is_empty() && usages.is_empty() {
            return Ok(ToolResult::success(format!(
                "No references found for `{}`",
                symbol
            )));
        }

        // Show definitions first
        if !definitions.is_empty() {
            output.push_str("## Definition(s)\n\n");
            for (file, line, lines) in definitions.iter().take(3) {
                output.push_str(&format!("**{}:{}**\n```\n", file, line));
                for l in lines.iter().take(30) {
                    output.push_str(l);
                    output.push('\n');
                }
                if lines.len() > 30 {
                    output.push_str("... (truncated)\n");
                }
                output.push_str("```\n\n");
            }
        }

        // Show usages
        if !usages.is_empty() {
            output.push_str(&format!("## Usages ({} found)\n\n", usages.len()));
            for (file, line, content) in usages.iter().take(20) {
                output.push_str(&format!(
                    "- `{}:{}` → `{}`\n",
                    file,
                    line,
                    if content.len() > 80 {
                        format!("{}...", &content[..77])
                    } else {
                        content.clone()
                    }
                ));
            }
            if usages.len() > 20 {
                output.push_str(&format!("\n... and {} more usages\n", usages.len() - 20));
            }
        }

        // Add call flow suggestion
        output.push_str("\n## Suggested Next Steps\n");
        output.push_str("- Use `find_references` on functions called within the definition\n");
        output.push_str("- Use `read_file` to see full context of specific files\n");

        Ok(ToolResult::success(output))
    }
}
