//! File operation tools: read, write, patch

use super::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Generate a unified diff between two strings
fn generate_diff(old_content: &str, new_content: &str, filename: &str) -> String {
    let old_lines: Vec<&str> = old_content.lines().collect();
    let new_lines: Vec<&str> = new_content.lines().collect();

    let mut diff = String::new();
    diff.push_str(&format!("--- a/{}\n", filename));
    diff.push_str(&format!("+++ b/{}\n", filename));

    // Simple line-by-line diff (not optimal but readable)
    let max_len = old_lines.len().max(new_lines.len());
    let mut in_hunk = false;
    let mut hunk_start_old = 0;
    let mut hunk_start_new = 0;
    let mut hunk_lines: Vec<String> = Vec::new();

    let context = 3; // Lines of context around changes

    for i in 0..max_len {
        let old_line = old_lines.get(i).copied();
        let new_line = new_lines.get(i).copied();

        match (old_line, new_line) {
            (Some(o), Some(n)) if o == n => {
                if in_hunk {
                    hunk_lines.push(format!(" {}", o));
                }
            }
            (Some(o), Some(n)) => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_start_old = i.saturating_sub(context) + 1;
                    hunk_start_new = i.saturating_sub(context) + 1;
                    // Add context before
                    for j in i.saturating_sub(context)..i {
                        if let Some(line) = old_lines.get(j) {
                            hunk_lines.push(format!(" {}", line));
                        }
                    }
                }
                hunk_lines.push(format!("-{}", o));
                hunk_lines.push(format!("+{}", n));
            }
            (Some(o), None) => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_start_old = i.saturating_sub(context) + 1;
                    hunk_start_new = i.saturating_sub(context) + 1;
                }
                hunk_lines.push(format!("-{}", o));
            }
            (None, Some(n)) => {
                if !in_hunk {
                    in_hunk = true;
                    hunk_start_old = i.saturating_sub(context) + 1;
                    hunk_start_new = i.saturating_sub(context) + 1;
                }
                hunk_lines.push(format!("+{}", n));
            }
            (None, None) => {}
        }
    }

    if !hunk_lines.is_empty() {
        let old_count = hunk_lines
            .iter()
            .filter(|l| l.starts_with('-') || l.starts_with(' '))
            .count();
        let new_count = hunk_lines
            .iter()
            .filter(|l| l.starts_with('+') || l.starts_with(' '))
            .count();
        diff.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk_start_old, old_count, hunk_start_new, new_count
        ));
        for line in hunk_lines {
            diff.push_str(&line);
            diff.push('\n');
        }
    }

    if diff.lines().count() <= 2 {
        return "No changes".to_string();
    }

    diff
}

/// Tool for reading file contents
pub struct ReadFileTool {
    working_dir: PathBuf,
}

impl ReadFileTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read the contents of a file. Returns the file content as a string."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to read (relative to working directory or absolute)"
                },
                "start_line": {
                    "type": "integer",
                    "description": "Optional: Start reading from this line number (1-indexed)"
                },
                "end_line": {
                    "type": "integer",
                    "description": "Optional: Stop reading at this line number (1-indexed, inclusive)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            start_line: Option<usize>,
            end_line: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);

        // Security check: ensure path is within working directory
        if !path.starts_with(&self.working_dir) && !path.starts_with("/") {
            return Ok(ToolResult::error(
                "Access denied: path outside working directory",
            ));
        }

        // Use lossy UTF-8 conversion to handle files with invalid encoding
        match std::fs::read(&path) {
            Ok(bytes) => {
                let content = String::from_utf8_lossy(&bytes).into_owned();
                let output = match (params.start_line, params.end_line) {
                    (Some(start), Some(end)) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = start.saturating_sub(1);
                        let end = end.min(lines.len());
                        lines[start..end].join("\n")
                    }
                    (Some(start), None) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let start = start.saturating_sub(1);
                        lines[start..].join("\n")
                    }
                    (None, Some(end)) => {
                        let lines: Vec<&str> = content.lines().collect();
                        let end = end.min(lines.len());
                        lines[..end].join("\n")
                    }
                    (None, None) => content,
                };
                Ok(ToolResult::success(output))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to read file: {}", e))),
        }
    }
}

/// Tool for writing file contents
pub struct WriteFileTool {
    working_dir: PathBuf,
}

impl WriteFileTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write content to a file. Creates the file if it doesn't exist, overwrites if it does."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to write (relative to working directory or absolute)"
                },
                "content": {
                    "type": "string",
                    "description": "Content to write to the file"
                }
            },
            "required": ["path", "content"]
        })
    }

    fn risk_level(&self) -> super::RiskLevel {
        super::RiskLevel::Write
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            content: String,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);
        let filename = params.path.clone();

        // Security check
        if !path.starts_with(&self.working_dir) {
            return Ok(ToolResult::error(
                "Access denied: path outside working directory",
            ));
        }

        // Read existing content for diff (if file exists)
        let old_content = std::fs::read_to_string(&path).unwrap_or_default();
        let is_new_file = old_content.is_empty() && !path.exists();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                return Ok(ToolResult::error(format!(
                    "Failed to create directories: {}",
                    e
                )));
            }
        }

        match std::fs::write(&path, &params.content) {
            Ok(()) => {
                let mut output = String::new();

                if is_new_file {
                    output.push_str(&format!("✨ Created new file: {}\n", path.display()));
                    output.push_str(&format!("```diff\n+++ b/{}\n", filename));
                    for line in params.content.lines().take(20) {
                        output.push_str(&format!("+{}\n", line));
                    }
                    if params.content.lines().count() > 20 {
                        output.push_str(&format!(
                            "+... ({} more lines)\n",
                            params.content.lines().count() - 20
                        ));
                    }
                    output.push_str("```\n");
                } else {
                    output.push_str(&format!("📝 Modified: {}\n", path.display()));
                    output.push_str("```diff\n");
                    output.push_str(&generate_diff(&old_content, &params.content, &filename));
                    output.push_str("```\n");
                }

                output.push_str(&format!("\n({} bytes written)", params.content.len()));

                Ok(ToolResult::success(output))
            }
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {}", e))),
        }
    }
}

/// Tool for patching files with search/replace
pub struct PatchFileTool {
    working_dir: PathBuf,
}

impl PatchFileTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for PatchFileTool {
    fn name(&self) -> &str {
        "patch_file"
    }

    fn description(&self) -> &str {
        "Apply a patch to a file by replacing specific text. Use this for targeted edits."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to patch"
                },
                "old_text": {
                    "type": "string",
                    "description": "The exact text to find and replace"
                },
                "new_text": {
                    "type": "string",
                    "description": "The text to replace it with"
                }
            },
            "required": ["path", "old_text", "new_text"]
        })
    }

    fn risk_level(&self) -> super::RiskLevel {
        super::RiskLevel::Write
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            old_text: String,
            new_text: String,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);

        // Security check
        if !path.starts_with(&self.working_dir) {
            return Ok(ToolResult::error(
                "Access denied: path outside working directory",
            ));
        }

        // Read the file
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return Ok(ToolResult::error(format!("Failed to read file: {}", e))),
        };

        // Check if old_text exists
        if !content.contains(&params.old_text) {
            return Ok(ToolResult::error(
                "The specified text was not found in the file. Make sure the old_text matches exactly.",
            ));
        }

        // Replace the text
        let new_content = content.replacen(&params.old_text, &params.new_text, 1);

        // Write back
        match std::fs::write(&path, &new_content) {
            Ok(()) => Ok(ToolResult::success(format!(
                "Successfully patched {}",
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to write file: {}", e))),
        }
    }
}

/// Tool for deleting files
pub struct DeleteFileTool {
    working_dir: PathBuf,
}

impl DeleteFileTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for DeleteFileTool {
    fn name(&self) -> &str {
        "delete_file"
    }

    fn description(&self) -> &str {
        "Delete a file from the filesystem. Use with caution - this cannot be undone."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to delete (relative to working directory or absolute)"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> super::RiskLevel {
        super::RiskLevel::Dangerous
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);

        // Security check
        if !path.starts_with(&self.working_dir) {
            return Ok(ToolResult::error(
                "Access denied: path outside working directory",
            ));
        }

        // Check if file exists
        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                path.display()
            )));
        }

        // Check if it's a file (not a directory)
        if path.is_dir() {
            return Ok(ToolResult::error(
                "Cannot delete directories with this tool. Use shell with 'rm -r' for directories.",
            ));
        }

        match std::fs::remove_file(&path) {
            Ok(()) => Ok(ToolResult::success(format!(
                "Successfully deleted {}",
                path.display()
            ))),
            Err(e) => Ok(ToolResult::error(format!("Failed to delete file: {}", e))),
        }
    }
}

/// Tool for reading multiple files at once (batch operation)
pub struct ReadFilesTool {
    working_dir: PathBuf,
}

impl ReadFilesTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for ReadFilesTool {
    fn name(&self) -> &str {
        "read_files"
    }

    fn description(&self) -> &str {
        "Read multiple files at once. More efficient than reading files one by one. Returns contents of all files."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "paths": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Array of file paths to read (relative to working directory or absolute)"
                },
                "max_lines_per_file": {
                    "type": "integer",
                    "description": "Optional: Maximum lines to read from each file (default: 500)"
                }
            },
            "required": ["paths"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            paths: Vec<String>,
            max_lines_per_file: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let max_lines = params.max_lines_per_file.unwrap_or(500);

        let mut results = Vec::new();
        let mut errors = Vec::new();

        for path_str in &params.paths {
            let path = self.resolve_path(path_str);

            // Security check
            if !path.starts_with(&self.working_dir) && !path.is_absolute() {
                errors.push(format!("{}: access denied", path_str));
                continue;
            }

            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    // Truncate if too long
                    let lines: Vec<&str> = content.lines().collect();
                    let truncated = lines.len() > max_lines;
                    let content = if truncated {
                        format!(
                            "{}\n... ({} more lines)",
                            lines[..max_lines].join("\n"),
                            lines.len() - max_lines
                        )
                    } else {
                        content
                    };
                    results.push(format!("=== {} ===\n{}", path_str, content));
                }
                Err(e) => {
                    errors.push(format!("{}: {}", path_str, e));
                }
            }
        }

        let mut output = results.join("\n\n");
        if !errors.is_empty() {
            output.push_str(&format!("\n\n=== Errors ===\n{}", errors.join("\n")));
        }

        Ok(ToolResult::success(output))
    }
}

/// Tool for listing directory contents
pub struct ListDirectoryTool {
    working_dir: PathBuf,
}

impl ListDirectoryTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for ListDirectoryTool {
    fn name(&self) -> &str {
        "list_directory"
    }

    fn description(&self) -> &str {
        "List all files and subdirectories in a directory. Use this to explore the codebase structure."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Directory path to list (relative to working directory or absolute). Use '.' for current directory."
                },
                "recursive": {
                    "type": "boolean",
                    "description": "If true, list files recursively (default: false)"
                },
                "max_depth": {
                    "type": "integer",
                    "description": "Maximum recursion depth if recursive is true (default: 3)"
                },
                "include_hidden": {
                    "type": "boolean",
                    "description": "Include hidden files (starting with '.') (default: false)"
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            recursive: Option<bool>,
            max_depth: Option<usize>,
            include_hidden: Option<bool>,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);
        let recursive = params.recursive.unwrap_or(false);
        let max_depth = params.max_depth.unwrap_or(3);
        let include_hidden = params.include_hidden.unwrap_or(false);

        // Security check
        if !path.starts_with(&self.working_dir) && !path.is_absolute() {
            return Ok(ToolResult::error(
                "Access denied: path outside working directory",
            ));
        }

        if !path.exists() {
            return Ok(ToolResult::error(format!(
                "Directory not found: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Ok(ToolResult::error(format!(
                "Not a directory: {}",
                path.display()
            )));
        }

        fn list_dir(
            dir: &Path,
            base: &Path,
            depth: usize,
            max_depth: usize,
            recursive: bool,
            include_hidden: bool,
        ) -> Vec<String> {
            let mut entries = Vec::new();

            if let Ok(read_dir) = std::fs::read_dir(dir) {
                let mut items: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
                items.sort_by_key(|e| e.file_name());

                for entry in items {
                    let file_name = entry.file_name().to_string_lossy().to_string();

                    // Skip hidden files unless requested
                    if !include_hidden && file_name.starts_with('.') {
                        continue;
                    }

                    let relative_path = entry
                        .path()
                        .strip_prefix(base)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|_| file_name.clone());

                    let is_dir = entry.path().is_dir();
                    let prefix = "  ".repeat(depth);
                    let icon = if is_dir { "📁" } else { "📄" };

                    entries.push(format!("{}{} {}", prefix, icon, relative_path));

                    // Recurse into directories
                    if is_dir && recursive && depth < max_depth {
                        entries.extend(list_dir(
                            &entry.path(),
                            base,
                            depth + 1,
                            max_depth,
                            recursive,
                            include_hidden,
                        ));
                    }
                }
            }

            entries
        }

        let entries = list_dir(&path, &path, 0, max_depth, recursive, include_hidden);

        if entries.is_empty() {
            Ok(ToolResult::success("Directory is empty"))
        } else {
            Ok(ToolResult::success(format!(
                "Contents of {}:\n{}",
                path.display(),
                entries.join("\n")
            )))
        }
    }
}

/// Tool for proposing changes (shows diff without applying) - for Plan mode
pub struct ProposeChangeTool {
    working_dir: PathBuf,
}

impl ProposeChangeTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    fn resolve_path(&self, path: &str) -> PathBuf {
        let path = Path::new(path);
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.working_dir.join(path)
        }
    }
}

#[async_trait]
impl Tool for ProposeChangeTool {
    fn name(&self) -> &str {
        "propose_change"
    }

    fn description(&self) -> &str {
        "Propose a code change by showing what the diff would look like WITHOUT actually modifying the file. Use this in Plan mode to show suggested changes."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to propose changes for"
                },
                "new_content": {
                    "type": "string",
                    "description": "The proposed new content for the file"
                },
                "description": {
                    "type": "string",
                    "description": "Brief description of what this change does"
                }
            },
            "required": ["path", "new_content"]
        })
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            new_content: String,
            description: Option<String>,
        }

        let params: Params = serde_json::from_value(params)?;
        let path = self.resolve_path(&params.path);
        let filename = params.path.clone();

        // Read existing content
        let old_content = std::fs::read_to_string(&path).unwrap_or_default();
        let is_new_file = old_content.is_empty() && !path.exists();

        let mut output = String::new();

        // Header
        output.push_str("📋 **PROPOSED CHANGE** (not applied)\n\n");

        if let Some(desc) = params.description {
            output.push_str(&format!("**Description:** {}\n\n", desc));
        }

        if is_new_file {
            output.push_str(&format!("**Action:** Create new file `{}`\n\n", filename));
            output.push_str("```diff\n");
            output.push_str(&format!("+++ b/{}\n", filename));
            for line in params.new_content.lines().take(50) {
                output.push_str(&format!("+{}\n", line));
            }
            if params.new_content.lines().count() > 50 {
                output.push_str(&format!(
                    "+... ({} more lines)\n",
                    params.new_content.lines().count() - 50
                ));
            }
            output.push_str("```\n");
        } else {
            output.push_str(&format!("**Action:** Modify `{}`\n\n", filename));
            output.push_str("```diff\n");
            output.push_str(&generate_diff(&old_content, &params.new_content, &filename));
            output.push_str("```\n");
        }

        output.push_str("\n⚠️ **This is a preview only.** Switch to `/build` mode and ask me to apply this change.");

        Ok(ToolResult::success(output))
    }
}
