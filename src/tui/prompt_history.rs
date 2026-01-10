//! Prompt history management for TUI
//!
//! This module provides persistent prompt history with navigation support.
//! History is stored in `.tark/prompt_history.json` and limited to 100 entries.

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Maximum number of history entries to store
const MAX_HISTORY_ENTRIES: usize = 100;

/// History storage filename
const HISTORY_FILENAME: &str = "prompt_history.json";

/// Prompt history with navigation support
#[derive(Debug)]
pub struct PromptHistory {
    /// History entries (oldest first)
    entries: Vec<String>,
    /// Current navigation index (None = at end, editing new input)
    current_index: Option<usize>,
    /// Saved input when navigating away from new input
    saved_input: String,
    /// Maximum entries to store
    max_entries: usize,
    /// Path to storage file
    storage_path: PathBuf,
}

/// Serializable history data
#[derive(Debug, Serialize, Deserialize)]
struct HistoryData {
    entries: Vec<String>,
}

impl PromptHistory {
    /// Create a new PromptHistory and load from disk if available
    pub fn load(storage_path: PathBuf) -> Self {
        let entries = Self::load_from_disk(&storage_path).unwrap_or_default();

        Self {
            entries,
            current_index: None,
            saved_input: String::new(),
            max_entries: MAX_HISTORY_ENTRIES,
            storage_path,
        }
    }

    /// Create a new PromptHistory for a workspace directory
    pub fn for_workspace(workspace_dir: &std::path::Path) -> Self {
        let storage_path = workspace_dir.join(".tark").join(HISTORY_FILENAME);
        Self::load(storage_path)
    }

    /// Load history entries from disk
    fn load_from_disk(path: &PathBuf) -> Result<Vec<String>> {
        if !path.exists() {
            return Ok(Vec::new());
        }

        let content =
            std::fs::read_to_string(path).context("Failed to read prompt history file")?;
        let data: HistoryData =
            serde_json::from_str(&content).context("Failed to parse prompt history")?;

        Ok(data.entries)
    }

    /// Save history entries to disk
    pub fn save(&self) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let data = HistoryData {
            entries: self.entries.clone(),
        };
        let content = serde_json::to_string_pretty(&data)?;
        std::fs::write(&self.storage_path, content)?;

        Ok(())
    }

    /// Add a new entry to history
    ///
    /// Duplicates of the most recent entry are ignored.
    /// If history exceeds max_entries, oldest entries are removed.
    pub fn add(&mut self, entry: String) {
        // Don't add empty entries
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return;
        }

        // Don't add duplicates of the most recent entry
        if self.entries.last().map(|s| s.as_str()) == Some(trimmed) {
            return;
        }

        self.entries.push(trimmed.to_string());

        // Trim to max entries
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }

        // Reset navigation state
        self.current_index = None;
        self.saved_input.clear();
    }

    /// Navigate to the previous (older) entry
    ///
    /// If currently editing new input, saves it and moves to most recent history.
    /// Returns the entry to display, or None if at the beginning.
    pub fn previous(&mut self, current_input: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }

        match self.current_index {
            None => {
                // Currently at new input, save it and go to most recent history
                self.saved_input = current_input.to_string();
                let idx = self.entries.len() - 1;
                self.current_index = Some(idx);
                Some(&self.entries[idx])
            }
            Some(0) => {
                // Already at oldest entry, can't go further back
                None
            }
            Some(idx) => {
                // Move to older entry
                let new_idx = idx - 1;
                self.current_index = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Navigate to the next (newer) entry
    ///
    /// If at the most recent history entry, returns to saved input.
    /// Returns the entry to display, or None if already at new input.
    pub fn next_entry(&mut self) -> Option<&str> {
        match self.current_index {
            None => {
                // Already at new input
                None
            }
            Some(idx) if idx >= self.entries.len() - 1 => {
                // At most recent history, return to saved input
                self.current_index = None;
                Some(&self.saved_input)
            }
            Some(idx) => {
                // Move to newer entry
                let new_idx = idx + 1;
                self.current_index = Some(new_idx);
                Some(&self.entries[new_idx])
            }
        }
    }

    /// Reset navigation state (e.g., when input is submitted or cleared)
    pub fn reset_navigation(&mut self) {
        self.current_index = None;
        self.saved_input.clear();
    }

    /// Check if currently navigating history
    pub fn is_navigating(&self) -> bool {
        self.current_index.is_some()
    }

    /// Get the current entry being viewed (if navigating)
    pub fn current_entry(&self) -> Option<&str> {
        self.current_index.map(|idx| self.entries[idx].as_str())
    }

    /// Get the saved input (for when returning from history navigation)
    pub fn saved_input(&self) -> &str {
        &self.saved_input
    }

    /// Get the number of entries in history
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if history is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all entries (for testing)
    #[cfg(test)]
    pub fn entries(&self) -> &[String] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use tempfile::TempDir;

    /// Strategy for generating non-empty prompt strings
    fn arb_prompt() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9 ]{1,100}".prop_map(|s| s.trim().to_string())
    }

    /// Strategy for generating a list of prompts
    fn arb_prompts(max_count: usize) -> impl Strategy<Value = Vec<String>> {
        prop::collection::vec(arb_prompt(), 0..max_count)
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        /// **Feature: tui-llm-integration, Property 14: History Navigation**
        /// **Validates: Requirements 14.1, 14.2, 14.4, 14.5, 14.6, 14.7**
        ///
        /// For any history navigation action (Up/Down arrow or j/k in normal mode),
        /// the TUI SHALL show previous/next prompts from history, preserve unsent
        /// input when navigating away and restore it when returning.
        #[test]
        fn prop_history_navigation_preserves_unsent_input(
            prompts in arb_prompts(10),
            current_input in "[a-zA-Z0-9 ]{0,50}",
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage_path = temp_dir.path().join("prompt_history.json");
            let mut history = PromptHistory::load(storage_path);

            // Add prompts to history
            for prompt in &prompts {
                if !prompt.trim().is_empty() {
                    history.add(prompt.clone());
                }
            }

            // Navigate to previous (if history is not empty)
            if !history.is_empty() {
                let _ = history.previous(&current_input);

                // Navigate back to current input
                while history.is_navigating() {
                    let _ = history.next_entry();
                }

                // Verify saved input is restored
                prop_assert_eq!(
                    history.saved_input(),
                    &current_input,
                    "Unsent input should be preserved"
                );
            }
        }

        /// **Feature: tui-llm-integration, Property 14: History Navigation**
        /// **Validates: Requirements 14.5**
        ///
        /// The TUI SHALL store up to 100 prompts in history.
        #[test]
        fn prop_history_max_entries_limit(
            prompts in prop::collection::vec(arb_prompt(), 0..150),
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage_path = temp_dir.path().join("prompt_history.json");
            let mut history = PromptHistory::load(storage_path);

            // Add all prompts
            for prompt in &prompts {
                if !prompt.trim().is_empty() {
                    history.add(prompt.clone());
                }
            }

            // Verify history doesn't exceed 100 entries
            prop_assert!(
                history.len() <= 100,
                "History should not exceed 100 entries, got {}",
                history.len()
            );
        }

        /// **Feature: tui-llm-integration, Property 14: History Navigation**
        /// **Validates: Requirements 14.6, 14.7**
        ///
        /// When a prompt from history is selected and modified, the original
        /// history entry SHALL remain unchanged. When the user presses Enter
        /// on a history item, the TUI SHALL send it as a new message.
        #[test]
        fn prop_history_entries_immutable(
            prompts in arb_prompts(10).prop_filter("need at least one prompt", |p| {
                p.iter().any(|s| !s.trim().is_empty())
            }),
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage_path = temp_dir.path().join("prompt_history.json");
            let mut history = PromptHistory::load(storage_path);

            // Add prompts to history
            let mut added_prompts = Vec::new();
            for prompt in &prompts {
                if !prompt.trim().is_empty() {
                    history.add(prompt.clone());
                    added_prompts.push(prompt.trim().to_string());
                }
            }

            // Deduplicate consecutive entries (as add() does)
            let mut expected: Vec<String> = Vec::new();
            for p in added_prompts {
                if expected.last().map(|s| s.as_str()) != Some(&p) {
                    expected.push(p);
                }
            }

            // Trim to max entries
            while expected.len() > 100 {
                expected.remove(0);
            }

            // Navigate through history and verify entries are unchanged
            let _ = history.previous("");
            let mut visited = Vec::new();
            while let Some(entry) = history.current_entry() {
                visited.push(entry.to_string());
                if history.previous("").is_none() {
                    break;
                }
            }

            // Reverse to get oldest-first order
            visited.reverse();

            // Verify entries match original
            prop_assert_eq!(
                visited.len(),
                expected.len(),
                "History entry count mismatch"
            );

            for (i, (visited_entry, expected_entry)) in visited.iter().zip(expected.iter()).enumerate() {
                prop_assert_eq!(
                    visited_entry,
                    expected_entry,
                    "History entry {} mismatch",
                    i
                );
            }
        }

        /// **Feature: tui-llm-integration, Property 14: History Navigation**
        /// **Validates: Requirements 14.3**
        ///
        /// The prompt history SHALL persist across sessions (save/load round-trip).
        #[test]
        fn prop_history_persistence_round_trip(
            prompts in arb_prompts(20),
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage_path = temp_dir.path().join("prompt_history.json");

            // Create history and add prompts
            let mut history = PromptHistory::load(storage_path.clone());
            let mut expected = Vec::new();
            for prompt in &prompts {
                if !prompt.trim().is_empty() {
                    history.add(prompt.clone());
                    // Track expected entries (deduped)
                    let trimmed = prompt.trim().to_string();
                    if expected.last().map(|s: &String| s.as_str()) != Some(&trimmed) {
                        expected.push(trimmed);
                    }
                }
            }

            // Save to disk
            let save_result = history.save();
            prop_assert!(save_result.is_ok(), "Failed to save history: {:?}", save_result.err());

            // Load from disk
            let loaded_history = PromptHistory::load(storage_path);

            // Verify entries match
            prop_assert_eq!(
                loaded_history.len(),
                history.len(),
                "Loaded history length mismatch"
            );

            // Verify entries are the same
            prop_assert_eq!(
                loaded_history.entries(),
                history.entries(),
                "Loaded history entries mismatch"
            );
        }

        /// **Feature: tui-llm-integration, Property 14: History Navigation**
        /// **Validates: Requirements 14.1, 14.2**
        ///
        /// For any history, navigating up then down should return to the same position.
        #[test]
        fn prop_history_navigation_bidirectional(
            prompts in arb_prompts(10).prop_filter("need at least 2 prompts", |p| {
                p.iter().filter(|s| !s.trim().is_empty()).count() >= 2
            }),
            nav_steps in 1usize..5usize,
        ) {
            let temp_dir = TempDir::new().expect("Failed to create temp dir");
            let storage_path = temp_dir.path().join("prompt_history.json");
            let mut history = PromptHistory::load(storage_path);

            // Add prompts to history
            for prompt in &prompts {
                if !prompt.trim().is_empty() {
                    history.add(prompt.clone());
                }
            }

            let current_input = "test input";

            // Navigate up nav_steps times
            for _ in 0..nav_steps {
                if history.previous(current_input).is_none() {
                    break;
                }
            }

            // Record position
            let position_after_up = history.current_entry().map(|s| s.to_string());

            // Navigate down then up (should return to same position)
            if history.next_entry().is_some() {
                let _ = history.previous("");
                let position_after_round_trip = history.current_entry().map(|s| s.to_string());
                prop_assert_eq!(
                    position_after_up,
                    position_after_round_trip,
                    "Navigation should be bidirectional"
                );
            }
        }
    }
}
