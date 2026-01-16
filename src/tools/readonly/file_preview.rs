//! File preview tool for large files.
//!
//! Provides smart preview of large files showing head, tail, and file statistics
//! without loading the entire file into memory.

use crate::tools::risk::RiskLevel;
use crate::tools::{Tool, ToolResult};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::PathBuf;

/// Tool for previewing large files efficiently.
pub struct FilePreviewTool {
    working_dir: PathBuf,
}

impl FilePreviewTool {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }
}

#[async_trait]
impl Tool for FilePreviewTool {
    fn name(&self) -> &str {
        "file_preview"
    }

    fn description(&self) -> &str {
        "Preview a large file efficiently. Shows file statistics, the first N lines (head), \
         and the last M lines (tail) without loading the entire file. Use this for files \
         over 1000 lines or when you only need a quick overview."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "Path to the file to preview"
                },
                "head_lines": {
                    "type": "integer",
                    "description": "Number of lines to show from the start (default: 50)"
                },
                "tail_lines": {
                    "type": "integer",
                    "description": "Number of lines to show from the end (default: 20)"
                }
            },
            "required": ["path"]
        })
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    async fn execute(&self, params: Value) -> Result<ToolResult> {
        #[derive(Deserialize)]
        struct Params {
            path: String,
            head_lines: Option<usize>,
            tail_lines: Option<usize>,
        }

        let params: Params = serde_json::from_value(params)?;
        let file_path = self.working_dir.join(&params.path);
        let head_lines = params.head_lines.unwrap_or(50);
        let tail_lines = params.tail_lines.unwrap_or(20);

        // Check if file exists
        if !file_path.exists() {
            return Ok(ToolResult::error(format!(
                "File not found: {}",
                params.path
            )));
        }

        if !file_path.is_file() {
            return Ok(ToolResult::error(format!("{} is not a file", params.path)));
        }

        // Get file metadata
        let metadata = std::fs::metadata(&file_path)?;
        let file_size = metadata.len();

        // Read file to count lines and get head
        let file = File::open(&file_path)?;
        let reader = BufReader::new(&file);

        let mut head_content: Vec<String> = Vec::new();
        let mut total_lines = 0;
        let mut all_lines: Vec<String> = Vec::new();

        for line in reader.lines() {
            // Handle invalid UTF-8 gracefully by skipping problematic lines
            let line = match line {
                Ok(l) => l,
                Err(_) => continue, // Skip lines with invalid UTF-8
            };
            total_lines += 1;

            if total_lines <= head_lines {
                head_content.push(format!("{:6}| {}", total_lines, line));
            }

            // Keep a sliding window for tail
            all_lines.push(line);
            if all_lines.len() > tail_lines {
                all_lines.remove(0);
            }
        }

        // Get tail content (last N lines)
        let tail_start_line = total_lines.saturating_sub(tail_lines) + 1;
        let tail_content: Vec<String> = all_lines
            .iter()
            .enumerate()
            .map(|(i, line)| format!("{:6}| {}", tail_start_line + i, line))
            .collect();

        // Format output
        let mut output = String::new();

        // File info section
        output.push_str(&format!("# File Preview: {}\n\n", params.path));
        output.push_str("## File Information\n");
        output.push_str(&format!(
            "- **Size**: {} bytes ({:.2} KB)\n",
            file_size,
            file_size as f64 / 1024.0
        ));
        output.push_str(&format!("- **Total Lines**: {}\n", total_lines));

        // Detect file type by extension
        if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
            output.push_str(&format!("- **Type**: {}\n", ext));
        }
        output.push('\n');

        // Head section
        output.push_str(&format!("## First {} Lines\n", head_lines.min(total_lines)));
        output.push_str("```\n");
        for line in &head_content {
            output.push_str(line);
            output.push('\n');
        }
        output.push_str("```\n\n");

        // Show gap indicator if there's content in between
        if total_lines > head_lines + tail_lines {
            let hidden_lines = total_lines - head_lines - tail_lines;
            output.push_str(&format!("*... {} lines hidden ...*\n\n", hidden_lines));
        }

        // Tail section (only if different from head)
        if total_lines > head_lines {
            let actual_tail = tail_lines.min(total_lines.saturating_sub(head_lines));
            output.push_str(&format!("## Last {} Lines\n", actual_tail));
            output.push_str("```\n");
            for line in &tail_content {
                output.push_str(line);
                output.push('\n');
            }
            output.push_str("```\n");
        }

        // Suggestions
        output.push_str("\n## Next Steps\n");
        output.push_str(
            "- Use `read_file` with `start_line` and `end_line` to read specific sections\n",
        );
        output.push_str("- Use `grep` to search for specific patterns within this file\n");

        Ok(ToolResult::success(output))
    }
}

/// Read the last N lines of a file efficiently
#[allow(dead_code)]
fn read_tail(path: &PathBuf, n: usize) -> Result<Vec<String>> {
    let file = File::open(path)?;
    let file_size = file.metadata()?.len();

    if file_size == 0 {
        return Ok(Vec::new());
    }

    // For small files, just read everything
    if file_size < 65536 {
        let reader = BufReader::new(file);
        let lines: Vec<String> = reader.lines().map_while(Result::ok).collect();
        let start = lines.len().saturating_sub(n);
        return Ok(lines[start..].to_vec());
    }

    // For larger files, seek near the end and read backwards
    let mut file = file;
    let chunk_size = 8192u64;
    let mut pos = file_size;
    let mut buffer = Vec::new();
    let mut lines = Vec::new();

    while pos > 0 && lines.len() < n {
        let read_size = chunk_size.min(pos);
        pos -= read_size;

        file.seek(SeekFrom::Start(pos))?;
        let mut chunk = vec![0u8; read_size as usize];
        file.read_exact(&mut chunk)?;

        // Prepend to buffer
        buffer.splice(0..0, chunk);

        // Count lines in buffer
        lines = String::from_utf8_lossy(&buffer)
            .lines()
            .map(String::from)
            .collect();
    }

    // Return last N lines
    let start = lines.len().saturating_sub(n);
    Ok(lines[start..].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = FilePreviewTool::new(PathBuf::from("."));
        assert_eq!(tool.name(), "file_preview");
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
        assert!(!tool.description().is_empty());

        let params = tool.parameters();
        assert!(params.get("properties").is_some());
    }
}
