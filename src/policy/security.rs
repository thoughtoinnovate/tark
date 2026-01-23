use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};

/// Validates patterns before saving to prevent dangerous patterns
pub struct PatternValidator {
    _working_dir: PathBuf,
}

impl PatternValidator {
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            _working_dir: working_dir,
        }
    }

    /// Validate pattern before saving
    pub fn validate(&self, tool_id: &str, pattern: &str, match_type: &str) -> Result<()> {
        // Check length
        if pattern.len() > 1000 {
            return Err(anyhow!("Pattern too long (max 1000 characters)"));
        }

        // Check for forbidden patterns
        self.check_forbidden(tool_id, pattern)?;

        // Validate match type
        match match_type {
            "exact" | "prefix" | "glob" => Ok(()),
            _ => Err(anyhow!("Invalid match type: {}", match_type)),
        }
    }

    /// Check if pattern matches any forbidden patterns
    fn check_forbidden(&self, tool_id: &str, pattern: &str) -> Result<()> {
        if tool_id == "shell" {
            // Forbidden shell patterns
            const FORBIDDEN: &[&str] = &[
                "rm -rf /",
                "rm -rf /*",
                ":(){ :|:& };:", // Fork bomb
                "dd if=/dev/zero of=/dev/",
                "mkfs.",
                "format ",
                "> /dev/sd",
            ];

            for forbidden in FORBIDDEN {
                if pattern.contains(forbidden) {
                    return Err(anyhow!(
                        "Forbidden pattern detected: {} contains dangerous operation",
                        forbidden
                    ));
                }
            }
        }

        Ok(())
    }
}

/// Sanitizes and validates file paths
pub struct PathSanitizer {
    working_dir: PathBuf,
}

impl PathSanitizer {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Canonicalize and verify path
    pub fn canonicalize(&self, path: &str) -> Result<PathBuf> {
        let path_buf = if path.starts_with('/') {
            PathBuf::from(path)
        } else {
            self.working_dir.join(path)
        };

        // Attempt to canonicalize (resolves .. and symlinks)
        match path_buf.canonicalize() {
            Ok(canonical) => Ok(canonical),
            Err(_) => {
                // Path doesn't exist yet - manually resolve .. components
                self.resolve_path(&path_buf)
            }
        }
    }

    /// Check if path is within working directory
    pub fn is_in_workdir(&self, path: &str) -> Result<bool> {
        let canonical_path = self.canonicalize(path)?;
        let canonical_workdir = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        Ok(canonical_path.starts_with(canonical_workdir))
    }

    /// Extract paths from command string
    pub fn extract_paths(&self, command: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let parts: Vec<&str> = command.split_whitespace().collect();

        for part in parts {
            // Skip flags
            if part.starts_with('-') && !part.starts_with("--") {
                continue;
            }

            // Check if looks like a path
            if part.contains('/') || part.ends_with(".txt") || part.ends_with(".rs") {
                let clean = part.trim_matches(|c| c == '"' || c == '\'');
                paths.push(clean.to_string());
            }
        }

        paths
    }

    /// Manually resolve path without requiring it to exist
    fn resolve_path(&self, path: &Path) -> Result<PathBuf> {
        let mut resolved = PathBuf::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    resolved.pop();
                }
                std::path::Component::CurDir => {
                    // Skip current directory
                }
                _ => {
                    resolved.push(component);
                }
            }
        }

        Ok(resolved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_pattern_validator() {
        let validator = PatternValidator::new(PathBuf::from("/work"));

        // Valid pattern
        assert!(validator.validate("shell", "cargo build", "prefix").is_ok());

        // Forbidden pattern
        assert!(validator.validate("shell", "rm -rf /", "prefix").is_err());

        // Invalid match type
        assert!(validator.validate("shell", "test", "invalid").is_err());
    }

    #[test]
    fn test_path_sanitizer() {
        let workdir = env::current_dir().unwrap();
        let sanitizer = PathSanitizer::new(workdir.clone());

        // Relative path
        assert!(sanitizer.is_in_workdir("./file.txt").unwrap());

        // Absolute path outside
        assert!(!sanitizer.is_in_workdir("/tmp/file.txt").unwrap());
    }

    #[test]
    fn test_extract_paths() {
        let sanitizer = PathSanitizer::new(PathBuf::from("/work"));

        let paths = sanitizer.extract_paths("cat file.txt another.txt");
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], "file.txt");

        let paths = sanitizer.extract_paths("rm -rf /tmp/test");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], "/tmp/test");
    }
}
