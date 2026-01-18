//! Git information helper for sidebar
//!
//! Provides functions to get git repository information (branch, changed files, stats)
//! using shell commands. Handles non-git directories gracefully.

use crate::ui_backend::{GitChangeInfo, GitStatus};
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// Get the current git branch name
pub fn get_current_branch(working_dir: &Path) -> String {
    match run_git_command(working_dir, &["branch", "--show-current"]) {
        Ok(output) => output.trim().to_string(),
        Err(_) => "main".to_string(), // Default fallback
    }
}

/// Get list of git changes in working directory
pub fn get_git_changes(working_dir: &Path) -> Vec<GitChangeInfo> {
    // Get porcelain status
    let porcelain = match run_git_command(working_dir, &["status", "--porcelain"]) {
        Ok(output) => output,
        Err(_) => return Vec::new(),
    };

    // Get diff stats for additions/deletions
    let diff_stats = get_diff_stats(working_dir);

    // Parse porcelain status and build changes list
    parse_porcelain_status(&porcelain, &diff_stats)
}

/// Parse `git status --porcelain` output to GitChangeInfo vector
fn parse_porcelain_status(
    output: &str,
    diff_stats: &HashMap<String, (usize, usize)>,
) -> Vec<GitChangeInfo> {
    let mut changes = Vec::new();

    for line in output.lines() {
        if line.len() < 3 {
            continue;
        }

        let status_code = &line[0..2];
        let file_path = line[3..].trim();

        let status = match status_code {
            // XY format: first char is staging, second is working tree
            "M " | " M" | "MM" => GitStatus::Modified,
            "A " | " A" | "AA" => GitStatus::Added,
            "D " | " D" | "DD" => GitStatus::Deleted,
            "R " | " R" | "RR" => GitStatus::Renamed,
            "??" => GitStatus::Untracked,
            _ => continue,
        };

        let (additions, deletions) = diff_stats.get(file_path).copied().unwrap_or((0, 0));

        changes.push(GitChangeInfo {
            file: file_path.to_string(),
            status,
            additions,
            deletions,
        });
    }

    changes
}

/// Get diff statistics (additions/deletions per file)
fn get_diff_stats(working_dir: &Path) -> HashMap<String, (usize, usize)> {
    let mut stats = HashMap::new();

    // Get staged changes
    if let Ok(output) = run_git_command(working_dir, &["diff", "--cached", "--numstat"]) {
        parse_numstat(&output, &mut stats);
    }

    // Get unstaged changes
    if let Ok(output) = run_git_command(working_dir, &["diff", "--numstat"]) {
        parse_numstat(&output, &mut stats);
    }

    stats
}

/// Parse `git diff --numstat` output
/// Format: additions\tdeletions\tfilename
fn parse_numstat(output: &str, stats: &mut HashMap<String, (usize, usize)>) {
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            if let (Ok(additions), Ok(deletions)) =
                (parts[0].parse::<usize>(), parts[1].parse::<usize>())
            {
                let file = parts[2].to_string();
                // Accumulate stats (may appear in both staged and unstaged)
                let (add, del) = stats.entry(file).or_insert((0, 0));
                *add += additions;
                *del += deletions;
            }
        }
    }
}

/// Run a git command in the working directory
fn run_git_command(working_dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(working_dir)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to run git command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(format!(
            "Git command failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_porcelain_status() {
        let porcelain = " M src/main.rs\nA  src/new_file.rs\nD  old_file.rs\n?? untracked.rs\n";
        let mut diff_stats = HashMap::new();
        diff_stats.insert("src/main.rs".to_string(), (10, 5));
        diff_stats.insert("src/new_file.rs".to_string(), (50, 0));

        let changes = parse_porcelain_status(porcelain, &diff_stats);

        assert_eq!(changes.len(), 4);
        assert_eq!(changes[0].file, "src/main.rs");
        assert_eq!(changes[0].status, GitStatus::Modified);
        assert_eq!(changes[0].additions, 10);
        assert_eq!(changes[0].deletions, 5);

        assert_eq!(changes[1].file, "src/new_file.rs");
        assert_eq!(changes[1].status, GitStatus::Added);

        assert_eq!(changes[2].file, "old_file.rs");
        assert_eq!(changes[2].status, GitStatus::Deleted);

        assert_eq!(changes[3].file, "untracked.rs");
        assert_eq!(changes[3].status, GitStatus::Untracked);
    }

    #[test]
    fn test_parse_numstat() {
        let numstat = "10\t5\tsrc/main.rs\n20\t0\tsrc/new.rs\n";
        let mut stats = HashMap::new();

        parse_numstat(numstat, &mut stats);

        assert_eq!(stats.get("src/main.rs"), Some(&(10, 5)));
        assert_eq!(stats.get("src/new.rs"), Some(&(20, 0)));
    }
}
