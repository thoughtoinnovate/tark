//! Git Service for viewing diffs and staging files

use anyhow::Result;
use std::path::PathBuf;
use std::process::Command;

/// Git operations service
pub struct GitService {
    working_dir: PathBuf,
}

impl GitService {
    /// Create a new git service
    pub fn new(working_dir: PathBuf) -> Self {
        Self { working_dir }
    }

    /// Get diff for a specific file
    pub fn get_file_diff(&self, file_path: &str) -> Result<String> {
        let output = Command::new("git")
            .arg("diff")
            .arg("HEAD")
            .arg("--")
            .arg(file_path)
            .current_dir(&self.working_dir)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!(
                "Failed to get diff: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Get diff for unstaged changes
    pub fn get_unstaged_diff(&self, file_path: &str) -> Result<String> {
        let output = Command::new("git")
            .arg("diff")
            .arg("--")
            .arg(file_path)
            .current_dir(&self.working_dir)
            .output()?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            Err(anyhow::anyhow!(
                "Failed to get unstaged diff: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Stage a file for commit
    pub fn stage_file(&self, file_path: &str) -> Result<()> {
        let output = Command::new("git")
            .arg("add")
            .arg(file_path)
            .current_dir(&self.working_dir)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to stage file: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Unstage a file
    pub fn unstage_file(&self, file_path: &str) -> Result<()> {
        let output = Command::new("git")
            .arg("reset")
            .arg("HEAD")
            .arg("--")
            .arg(file_path)
            .current_dir(&self.working_dir)
            .output()?;

        if output.status.success() {
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Failed to unstage file: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    /// Check if working directory is a git repository
    pub fn is_git_repo(&self) -> bool {
        Command::new("git")
            .arg("rev-parse")
            .arg("--git-dir")
            .current_dir(&self.working_dir)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_git_repo() -> Result<(TempDir, PathBuf)> {
        let temp = TempDir::new()?;
        let path = temp.path().to_path_buf();

        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()?;

        Command::new("git")
            .arg("config")
            .arg("user.name")
            .arg("Test User")
            .current_dir(&path)
            .output()?;

        Command::new("git")
            .arg("config")
            .arg("user.email")
            .arg("test@example.com")
            .current_dir(&path)
            .output()?;

        Ok((temp, path))
    }

    #[test]
    fn test_git_service_creation() {
        let service = GitService::new(PathBuf::from("."));
        assert!(!service.working_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_is_git_repo() -> Result<()> {
        let (_temp, path) = create_git_repo()?;
        let service = GitService::new(path);
        assert!(service.is_git_repo());
        Ok(())
    }

    #[test]
    fn test_stage_unstage_file() -> Result<()> {
        let (_temp, path) = create_git_repo()?;
        let service = GitService::new(path.clone());

        // Create a test file
        let test_file = path.join("test.txt");
        std::fs::write(&test_file, "test content")?;

        // Stage it
        service.stage_file("test.txt")?;

        // Unstage it
        service.unstage_file("test.txt")?;

        Ok(())
    }
}
