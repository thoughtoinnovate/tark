use std::path::PathBuf;

use crate::policy::types::{CommandClassification, Operation, RiskLevel};

/// Classifies shell commands to determine operation type and location
pub struct CommandClassifier {
    working_dir: PathBuf,
}

impl CommandClassifier {
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Classify a shell command
    pub fn classify(&self, command: &str) -> CommandClassification {
        let cmd = command.trim();

        // Check for delete operations first (most dangerous)
        if self.is_delete_command(cmd) {
            let in_workdir = self.paths_in_workdir(cmd);
            return CommandClassification {
                classification_id: "shell-rm".to_string(),
                operation: Operation::Delete,
                in_workdir,
                risk_level: if in_workdir {
                    RiskLevel::Moderate
                } else {
                    RiskLevel::Dangerous
                },
            };
        }

        // Check for write operations
        if self.is_write_command(cmd) {
            let in_workdir = self.paths_in_workdir(cmd);
            return CommandClassification {
                classification_id: "shell-write".to_string(),
                operation: Operation::Write,
                in_workdir,
                risk_level: if in_workdir {
                    RiskLevel::Moderate
                } else {
                    RiskLevel::Dangerous
                },
            };
        }

        // Check for read operations
        if self.is_read_command(cmd) {
            let in_workdir = self.paths_in_workdir(cmd);
            return CommandClassification {
                classification_id: "shell-read".to_string(),
                operation: Operation::Read,
                in_workdir,
                risk_level: RiskLevel::Safe,
            };
        }

        // Default: unknown command → treat as dangerous write outside workdir
        CommandClassification {
            classification_id: "shell-write".to_string(),
            operation: Operation::Execute,
            in_workdir: false, // Assume outside for safety
            risk_level: RiskLevel::Dangerous,
        }
    }

    /// Check if command is a read-only operation
    fn is_read_command(&self, cmd: &str) -> bool {
        // Use existing safe command prefixes from safe_shell.rs
        const READ_COMMANDS: &[&str] = &[
            // File reading
            "cat ",
            "head ",
            "tail ",
            "less ",
            "more ",
            // Listing
            "ls",
            "ll ",
            "dir ",
            "tree ",
            // Searching
            "grep ",
            "rg ",
            "ag ",
            "find ",
            "fd ",
            "which ",
            "whereis ",
            "locate ",
            // Info/Status
            "pwd",
            "whoami",
            "date",
            "uname ",
            "df ",
            "du ",
            "free ",
            "top",
            "ps ",
            "env",
            "printenv",
            "echo $",
            // Version checks
            "node --version",
            "npm --version",
            "cargo --version",
            "python --version",
            "java --version",
            "rustc --version",
            // Git read-only
            "git status",
            "git log",
            "git diff",
            "git branch",
            "git show",
            "git ls-files",
            "git rev-parse",
            // Package managers (list/info only)
            "npm list",
            "npm ls",
            "pip list",
            "pip show",
            "cargo search",
            "cargo tree",
        ];

        READ_COMMANDS.iter().any(|prefix| cmd.starts_with(prefix))
    }

    /// Check if command writes/modifies files
    fn is_write_command(&self, cmd: &str) -> bool {
        const WRITE_PATTERNS: &[&str] = &[
            // Output redirection
            " > ",
            " >> ",
            // File creation/modification
            "touch ",
            "mkdir ",
            "sed -i",
            "chmod ",
            "chown ",
            "chgrp ",
            // Package install/build
            "npm install",
            "npm i ",
            "pip install",
            "cargo build",
            "cargo install",
            "make ",
            "mvn ",
            // Git modifications
            "git add",
            "git commit",
            "git push",
            "git pull",
            "git merge",
            "git rebase",
            "git cherry-pick",
            "git stash apply",
            "git checkout",
            // Copy/move
            "cp ",
            "mv ",
            "rsync ",
        ];

        WRITE_PATTERNS.iter().any(|p| cmd.contains(p))
    }

    /// Check if command deletes files
    fn is_delete_command(&self, cmd: &str) -> bool {
        const DELETE_PATTERNS: &[&str] = &[
            "rm ",
            "rmdir ",
            "unlink ",
            "git clean",
            "git reset --hard",
            "npm uninstall",
            "pip uninstall",
            "cargo uninstall",
        ];

        DELETE_PATTERNS.iter().any(|p| cmd.starts_with(p))
    }

    /// Extract paths from command and check if all are within workdir
    fn paths_in_workdir(&self, cmd: &str) -> bool {
        let paths = self.extract_paths(cmd);

        // If no paths extracted, assume in workdir (safest for commands without paths)
        if paths.is_empty() {
            return true;
        }

        // All paths must be within workdir
        paths.iter().all(|p| self.is_path_in_workdir(p))
    }

    /// Extract file paths from command string
    fn extract_paths(&self, cmd: &str) -> Vec<String> {
        let mut paths = Vec::new();
        let parts: Vec<&str> = cmd.split_whitespace().collect();

        for part in parts {
            // Skip flags and options
            if part.starts_with('-') {
                continue;
            }

            // Check if looks like a path
            if part.contains('/') || part.contains('.') {
                // Remove quotes if present
                let clean = part.trim_matches(|c| c == '"' || c == '\'');
                paths.push(clean.to_string());
            }
        }

        paths
    }

    /// Check if path is within working directory
    fn is_path_in_workdir(&self, path_str: &str) -> bool {
        // Absolute paths starting with / are outside unless they're under workdir
        if path_str.starts_with('/') {
            if let Ok(canonical_path) = std::fs::canonicalize(path_str) {
                if let Ok(canonical_workdir) = std::fs::canonicalize(&self.working_dir) {
                    return canonical_path.starts_with(canonical_workdir);
                }
            }
            // If canonicalization fails, treat absolute paths as outside
            return false;
        }

        // Relative paths are within workdir
        // Check for path traversal attempts
        if path_str.contains("..") {
            // Resolve and check
            let full_path = self.working_dir.join(path_str);
            if let Ok(canonical) = full_path.canonicalize() {
                if let Ok(canonical_workdir) = self.working_dir.canonicalize() {
                    return canonical.starts_with(canonical_workdir);
                }
            }
            return false; // Failed to resolve, assume outside
        }

        // Simple relative path without traversal
        true
    }

    /// Parse compound commands (&&, ||, ;, |)
    pub fn parse_compound(&self, command: &str) -> Vec<CommandSegment> {
        let mut segments = Vec::new();

        // Simple split for now - TODO: Handle quoted strings properly
        let mut current = String::new();
        let mut chars = command.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '&' && chars.peek() == Some(&'&') {
                chars.next(); // consume second &
                if !current.trim().is_empty() {
                    segments.push(CommandSegment {
                        command: current.trim().to_string(),
                        separator: Some("&&".to_string()),
                    });
                }
                current.clear();
            } else if ch == '|' {
                if chars.peek() == Some(&'|') {
                    chars.next(); // consume second |
                    if !current.trim().is_empty() {
                        segments.push(CommandSegment {
                            command: current.trim().to_string(),
                            separator: Some("||".to_string()),
                        });
                    }
                    current.clear();
                } else {
                    // Single pipe
                    if !current.trim().is_empty() {
                        segments.push(CommandSegment {
                            command: current.trim().to_string(),
                            separator: Some("|".to_string()),
                        });
                    }
                    current.clear();
                }
            } else if ch == ';' {
                if !current.trim().is_empty() {
                    segments.push(CommandSegment {
                        command: current.trim().to_string(),
                        separator: Some(";".to_string()),
                    });
                }
                current.clear();
            } else {
                current.push(ch);
            }
        }

        // Add last segment
        if !current.trim().is_empty() {
            segments.push(CommandSegment {
                command: current.trim().to_string(),
                separator: None,
            });
        }

        segments
    }

    /// Classify compound command (returns highest risk)
    pub fn classify_compound(&self, command: &str) -> CommandClassification {
        let segments = self.parse_compound(command);

        if segments.is_empty() {
            return self.classify(command);
        }

        // Classify each segment and return highest risk
        let mut classifications: Vec<CommandClassification> = segments
            .iter()
            .map(|seg| self.classify(&seg.command))
            .collect();

        // Sort by risk level (Dangerous > Moderate > Safe)
        classifications.sort_by(|a, b| match (&a.risk_level, &b.risk_level) {
            (RiskLevel::Dangerous, _) => std::cmp::Ordering::Less,
            (_, RiskLevel::Dangerous) => std::cmp::Ordering::Greater,
            (RiskLevel::Moderate, RiskLevel::Safe) => std::cmp::Ordering::Less,
            (RiskLevel::Safe, RiskLevel::Moderate) => std::cmp::Ordering::Greater,
            _ => std::cmp::Ordering::Equal,
        });

        classifications.into_iter().next().unwrap()
    }
}

/// A segment of a compound command
pub struct CommandSegment {
    pub command: String,
    pub separator: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_read() {
        let classifier = CommandClassifier::new(PathBuf::from("/work"));

        let classification = classifier.classify("cat file.txt");
        assert_eq!(classification.operation, Operation::Read);
        assert_eq!(classification.classification_id, "shell-read");
        assert!(classification.in_workdir);

        let classification = classifier.classify("ls -la /tmp");
        assert_eq!(classification.operation, Operation::Read);
        assert!(!classification.in_workdir);
    }

    #[test]
    fn test_classify_write() {
        let classifier = CommandClassifier::new(PathBuf::from("/work"));

        let classification = classifier.classify("echo x > file.txt");
        assert_eq!(classification.operation, Operation::Write);
        assert_eq!(classification.classification_id, "shell-write");

        let classification = classifier.classify("npm install express");
        assert_eq!(classification.operation, Operation::Write);
    }

    #[test]
    fn test_classify_delete() {
        let classifier = CommandClassifier::new(PathBuf::from("/work"));

        let classification = classifier.classify("rm file.txt");
        assert_eq!(classification.operation, Operation::Delete);
        assert_eq!(classification.classification_id, "shell-rm");
        assert!(classification.in_workdir);

        let classification = classifier.classify("rm -rf /tmp/test");
        assert_eq!(classification.operation, Operation::Delete);
        assert!(!classification.in_workdir);
    }

    #[test]
    fn test_parse_compound() {
        let classifier = CommandClassifier::new(PathBuf::from("/work"));

        let segments = classifier.parse_compound("ls && cat file.txt");
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].command, "ls");
        assert_eq!(segments[0].separator, Some("&&".to_string()));

        let segments = classifier.parse_compound("cmd1 | cmd2 | cmd3");
        assert_eq!(segments.len(), 3);
    }

    #[test]
    fn test_classify_compound() {
        let classifier = CommandClassifier::new(PathBuf::from("/work"));

        // Should return highest risk (rm is Delete/Dangerous)
        let classification = classifier.classify_compound("ls && rm file.txt");
        assert_eq!(classification.operation, Operation::Delete);
        assert_eq!(classification.risk_level, RiskLevel::Moderate);
    }
}
