//! Debug logger with rotation, redaction, and error context
//!
//! Provides structured JSON logging for troubleshooting with:
//! - Correlation IDs for tracing requests
//! - Log rotation (10MB max, 3 files)
//! - Sensitive data redaction
//! - Full error context capture

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Serialize, Serializer};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024; // 10MB
const MAX_ROTATED_FILES: usize = 3;

/// Configuration for debug logging
#[derive(Debug, Clone)]
pub struct DebugLoggerConfig {
    pub log_dir: PathBuf,
    pub max_file_size: u64,
    pub max_rotated_files: usize,
}

impl Default for DebugLoggerConfig {
    fn default() -> Self {
        Self {
            log_dir: PathBuf::from(".tark/debug"),
            max_file_size: MAX_FILE_SIZE,
            max_rotated_files: MAX_ROTATED_FILES,
        }
    }
}

/// Core debug logger with rotation support
pub struct DebugLogger {
    config: DebugLoggerConfig,
    file: Arc<Mutex<File>>,
    current_size: Arc<AtomicU64>,
    redactor: SensitiveDataRedactor,
}

impl DebugLogger {
    /// Create a new debug logger
    pub fn new(config: DebugLoggerConfig) -> Result<Self> {
        // Create log directory
        std::fs::create_dir_all(&config.log_dir).context("Failed to create debug log directory")?;

        let log_path = config.log_dir.join("tark-debug.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .context("Failed to open debug log file")?;

        let current_size = file.metadata().map(|m| m.len()).unwrap_or(0);

        Ok(Self {
            config,
            file: Arc::new(Mutex::new(file)),
            current_size: Arc::new(AtomicU64::new(current_size)),
            redactor: SensitiveDataRedactor::new(),
        })
    }

    /// Log a debug entry
    pub fn log(&self, mut entry: DebugLogEntry) {
        // Redact sensitive data
        self.redactor.redact_entry(&mut entry);

        // Serialize to JSON
        let json = match serde_json::to_string(&entry) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("Failed to serialize debug log entry: {}", e);
                return;
            }
        };

        // Write to file
        if let Ok(mut file) = self.file.lock() {
            if let Err(e) = writeln!(file, "{}", json) {
                eprintln!("Failed to write debug log: {}", e);
                return;
            }

            // Update size
            let new_size = self
                .current_size
                .fetch_add(json.len() as u64 + 1, Ordering::Relaxed)
                + json.len() as u64
                + 1;

            // Check if rotation needed
            if new_size >= self.config.max_file_size {
                drop(file); // Release lock before rotating
                self.rotate();
            }
        }
    }

    /// Rotate log files
    fn rotate(&self) {
        let log_path = self.config.log_dir.join("tark-debug.log");

        // Rotate existing files: .2.log -> .3.log (delete), .1.log -> .2.log, .log -> .1.log
        for i in (1..self.config.max_rotated_files).rev() {
            let from = self.config.log_dir.join(format!("tark-debug.{}.log", i));
            let to = self
                .config
                .log_dir
                .join(format!("tark-debug.{}.log", i + 1));

            if from.exists() {
                if i + 1 == self.config.max_rotated_files {
                    // Delete oldest file
                    let _ = std::fs::remove_file(&from);
                } else {
                    // Rename to next number
                    let _ = std::fs::rename(&from, &to);
                }
            }
        }

        // Rotate current log to .1.log
        if log_path.exists() {
            let rotated = self.config.log_dir.join("tark-debug.1.log");
            let _ = std::fs::rename(&log_path, &rotated);
        }

        // Open new log file
        if let Ok(new_file) = OpenOptions::new().create(true).append(true).open(&log_path) {
            if let Ok(mut file) = self.file.lock() {
                *file = new_file;
                self.current_size.store(0, Ordering::Relaxed);
            }
        }
    }
}

/// Structured log entry
#[derive(Debug, Clone, Serialize)]
pub struct DebugLogEntry {
    pub timestamp: String,
    pub correlation_id: String,
    pub category: LogCategory,
    pub event: String,
    #[serde(flatten)]
    pub data: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_context: Option<ErrorContext>,
}

impl DebugLogEntry {
    /// Create a new debug log entry
    pub fn new(
        correlation_id: impl Into<String>,
        category: LogCategory,
        event: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            correlation_id: correlation_id.into(),
            category,
            event: event.into(),
            data: serde_json::json!({}),
            error_context: None,
        }
    }

    /// Add data to the entry
    pub fn with_data(mut self, data: serde_json::Value) -> Self {
        self.data = data;
        self
    }

    /// Add error context to the entry
    pub fn with_error_context(mut self, context: ErrorContext) -> Self {
        self.error_context = Some(context);
        self
    }
}

/// Log category
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogCategory {
    Service,
    Tui,
    LlmRaw,
}

impl Serialize for LogCategory {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            LogCategory::Service => "service",
            LogCategory::Tui => "tui",
            LogCategory::LlmRaw => "llm_raw",
        })
    }
}

/// Full error context for troubleshooting
#[derive(Debug, Clone, Serialize)]
pub struct ErrorContext {
    pub error_type: String,
    pub error_message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backtrace: Option<String>,
    pub env: HashMap<String, String>,
    pub system: SystemInfo,
}

impl ErrorContext {
    /// Create error context from an error
    pub fn from_error(error: &dyn std::error::Error) -> Self {
        let error_type = std::any::type_name_of_val(error).to_string();
        let error_message = error.to_string();

        // Try to capture backtrace
        let backtrace = std::backtrace::Backtrace::capture();
        let backtrace_str = match backtrace.status() {
            std::backtrace::BacktraceStatus::Captured => Some(backtrace.to_string()),
            _ => None,
        };

        Self {
            error_type,
            error_message,
            backtrace: backtrace_str,
            env: Self::capture_env(),
            system: SystemInfo::capture(),
        }
    }

    /// Capture relevant environment variables (filtered and redacted)
    fn capture_env() -> HashMap<String, String> {
        let mut env = HashMap::new();

        // List of environment variables we care about (non-sensitive ones)
        let allowed_vars = [
            "RUST_LOG",
            "RUST_BACKTRACE",
            "TARK_CONFIG",
            "HOME",
            "USER",
            "SHELL",
        ];

        for var in &allowed_vars {
            if let Ok(value) = std::env::var(var) {
                env.insert(var.to_string(), value);
            }
        }

        // Check for API key presence (but don't log values)
        for var in &[
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GEMINI_API_KEY",
            "OPENROUTER_API_KEY",
        ] {
            if std::env::var(var).is_ok() {
                env.insert(var.to_string(), "[REDACTED]".to_string());
            }
        }

        env
    }
}

/// System information
#[derive(Debug, Clone, Serialize)]
pub struct SystemInfo {
    pub os: String,
    pub tark_version: String,
    pub working_dir: String,
}

impl SystemInfo {
    /// Capture system information
    pub fn capture() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            tark_version: env!("CARGO_PKG_VERSION").to_string(),
            working_dir: std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".to_string()),
        }
    }
}

/// Redacts sensitive data before logging
pub struct SensitiveDataRedactor {
    patterns: Vec<(Regex, &'static str)>,
}

impl SensitiveDataRedactor {
    /// Create a new redactor with predefined patterns
    pub fn new() -> Self {
        let patterns = vec![
            // API keys
            (Regex::new(r"sk-[a-zA-Z0-9]{20,}").unwrap(), "sk-[REDACTED]"),
            (
                Regex::new(r"key-[a-zA-Z0-9]{20,}").unwrap(),
                "key-[REDACTED]",
            ),
            // Bearer tokens
            (
                Regex::new(r"Bearer [a-zA-Z0-9._-]+").unwrap(),
                "Bearer [REDACTED]",
            ),
            // GitHub tokens
            (
                Regex::new(r"ghp_[a-zA-Z0-9]{36}").unwrap(),
                "ghp_[REDACTED]",
            ),
            (
                Regex::new(r"gho_[a-zA-Z0-9]{36}").unwrap(),
                "gho_[REDACTED]",
            ),
            (
                Regex::new(r"ghs_[a-zA-Z0-9]{36}").unwrap(),
                "ghs_[REDACTED]",
            ),
            // Environment variable assignments
            (
                Regex::new(r"(?i)(api_key|secret|token|password|auth)=\S+").unwrap(),
                "$1=[REDACTED]",
            ),
        ];
        Self { patterns }
    }

    /// Redact sensitive data from a string
    pub fn redact(&self, input: &str) -> String {
        let mut output = input.to_string();
        for (pattern, replacement) in &self.patterns {
            output = pattern.replace_all(&output, *replacement).to_string();
        }
        output
    }

    /// Redact sensitive data from JSON
    pub fn redact_json(&self, value: &mut serde_json::Value) {
        match value {
            serde_json::Value::String(s) => {
                *s = self.redact(s);
            }
            serde_json::Value::Array(arr) => {
                for item in arr {
                    self.redact_json(item);
                }
            }
            serde_json::Value::Object(obj) => {
                for (key, val) in obj.iter_mut() {
                    // Check if key suggests sensitive data
                    let key_lower = key.to_lowercase();
                    if key_lower.contains("api_key")
                        || key_lower.contains("secret")
                        || key_lower.contains("token")
                        || key_lower.contains("password")
                        || key_lower.contains("auth")
                    {
                        if let serde_json::Value::String(_) = val {
                            *val = serde_json::Value::String("[REDACTED]".to_string());
                        }
                    } else {
                        self.redact_json(val);
                    }
                }
            }
            _ => {}
        }
    }

    /// Redact a debug log entry
    pub fn redact_entry(&self, entry: &mut DebugLogEntry) {
        // Redact data field
        self.redact_json(&mut entry.data);

        // Redact error context if present
        if let Some(ref mut ctx) = entry.error_context {
            ctx.error_message = self.redact(&ctx.error_message);
            if let Some(ref mut bt) = ctx.backtrace {
                *bt = self.redact(bt);
            }
        }
    }
}

impl Default for SensitiveDataRedactor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_redactor_api_keys() {
        let redactor = SensitiveDataRedactor::new();

        let input = "Using API key sk-1234567890abcdefghij for authentication";
        let output = redactor.redact(input);
        assert!(output.contains("sk-[REDACTED]"));
        assert!(!output.contains("sk-1234567890"));
    }

    #[test]
    fn test_redactor_bearer_tokens() {
        let redactor = SensitiveDataRedactor::new();

        let input = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9";
        let output = redactor.redact(input);
        assert!(output.contains("Bearer [REDACTED]"));
        assert!(!output.contains("eyJhbGciOi"));
    }

    #[test]
    fn test_redactor_env_vars() {
        let redactor = SensitiveDataRedactor::new();

        let input = "OPENAI_API_KEY=sk-test123 RUST_LOG=debug";
        let output = redactor.redact(input);
        assert!(output.contains("OPENAI_API_KEY=[REDACTED]"));
        assert!(output.contains("RUST_LOG=debug"));
    }

    #[test]
    fn test_redactor_json() {
        let redactor = SensitiveDataRedactor::new();

        let mut value = serde_json::json!({
            "api_key": "sk-secret123",
            "model": "gpt-4",
            "password": "hunter2"
        });

        redactor.redact_json(&mut value);

        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["model"], "gpt-4");
        assert_eq!(value["password"], "[REDACTED]");
    }
}
