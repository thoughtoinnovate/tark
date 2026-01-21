//! Risk levels and trust levels for tool operations.
//!
//! This module defines the risk classification system for tools and the
//! trust levels that determine when user confirmation is required.

use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Risk level classification for tool operations.
///
/// Tools are classified by their potential impact:
/// - `ReadOnly`: Safe read operations (read_file, list_dir, grep)
/// - `Write`: Write operations within working directory (write_file, patch_file)
/// - `Risky`: Shell commands and external operations
/// - `Dangerous`: Destructive operations (delete_file)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Safe read operations - never cause data loss
    ReadOnly,
    /// Write operations within working directory
    Write,
    /// Shell commands and external operations
    Risky,
    /// Destructive operations like delete
    Dangerous,
}

impl RiskLevel {
    /// Get a display icon for this risk level
    pub fn icon(&self) -> &'static str {
        match self {
            Self::ReadOnly => "📖",
            Self::Write => "✏️",
            Self::Risky => "⚠️",
            Self::Dangerous => "🔴",
        }
    }

    /// Get a short label for this risk level
    pub fn label(&self) -> &'static str {
        match self {
            Self::ReadOnly => "Read",
            Self::Write => "Write",
            Self::Risky => "Risky",
            Self::Dangerous => "Dangerous",
        }
    }

    /// Get the color for TUI display (as ratatui Color)
    pub fn color(&self) -> ratatui::style::Color {
        match self {
            Self::ReadOnly => ratatui::style::Color::Green,
            Self::Write => ratatui::style::Color::Blue,
            Self::Risky => ratatui::style::Color::Yellow,
            Self::Dangerous => ratatui::style::Color::Red,
        }
    }
}

/// Trust level determines when user confirmation is required.
///
/// Three levels provide different amounts of agent autonomy:
/// - `Balanced`: Auto-run reads & writes, prompt for shell/delete
/// - `Careful`: Auto-run reads only, prompt for all writes and above
/// - `Manual`: Prompt for everything including reads
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TrustLevel {
    /// Auto-run reads & writes, prompt for risky/dangerous only
    Balanced,
    /// Auto-run reads only, prompt for writes and above (default)
    #[default]
    Careful,
    /// Prompt for everything including reads
    Manual,
}

impl TrustLevel {
    /// Get display icon for this level
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Balanced => "🟡",
            Self::Careful => "🔵",
            Self::Manual => "🔴",
        }
    }

    /// Get short label for this level
    pub fn label(&self) -> &'static str {
        match self {
            Self::Balanced => "Balanced",
            Self::Careful => "Careful",
            Self::Manual => "Manual",
        }
    }

    /// Get description for this level
    pub fn description(&self) -> &'static str {
        match self {
            Self::Balanced => "Auto-run reads & writes, prompt for shell/delete",
            Self::Careful => "Auto-run reads only, prompt for writes+",
            Self::Manual => "Prompt for everything including reads",
        }
    }

    /// Check if this risk level requires approval checking at this trust level
    pub fn needs_approval_check(&self, risk: RiskLevel) -> bool {
        match self {
            Self::Balanced => matches!(risk, RiskLevel::Risky | RiskLevel::Dangerous),
            Self::Careful => matches!(
                risk,
                RiskLevel::Write | RiskLevel::Risky | RiskLevel::Dangerous
            ),
            Self::Manual => true,
        }
    }

    /// Cycle to the next level
    pub fn cycle_next(&self) -> Self {
        match self {
            Self::Balanced => Self::Careful,
            Self::Careful => Self::Manual,
            Self::Manual => Self::Balanced,
        }
    }

    /// Get all levels in order
    pub fn all() -> &'static [TrustLevel] {
        &[Self::Balanced, Self::Careful, Self::Manual]
    }

    /// Get the index of this level (for UI selection)
    pub fn index(&self) -> usize {
        match self {
            Self::Balanced => 0,
            Self::Careful => 1,
            Self::Manual => 2,
        }
    }

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => Self::Balanced,
            1 => Self::Careful,
            2 => Self::Manual,
            _ => Self::Balanced,
        }
    }

    /// Get string representation (for session storage)
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            Self::Balanced => "balanced",
            Self::Careful => "careful",
            Self::Manual => "manual",
        }
    }
}

impl FromStr for TrustLevel {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            // New names
            "balanced" => Self::Balanced,
            "careful" => Self::Careful,
            "manual" => Self::Manual,
            // Backward compatibility with old names
            "ask_risky" => Self::Balanced,
            "only_reads" => Self::Careful,
            "zero_trust" => Self::Manual,
            _ => Self::Balanced,
        })
    }
}

/// How to match a command against an approval pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchType {
    /// Exact string match: "rm src/temp.rs"
    Exact,
    /// Prefix match: "git push" matches "git push origin main"
    Prefix,
    /// Glob pattern: "rm src/*.bak" matches "rm src/file.bak"
    Glob,
}

impl MatchType {
    /// Get a short label for this match type
    pub fn label(&self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::Prefix => "prefix",
            Self::Glob => "glob",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_risk_level_ordering() {
        assert!(RiskLevel::ReadOnly < RiskLevel::Write);
        assert!(RiskLevel::Write < RiskLevel::Risky);
        assert!(RiskLevel::Risky < RiskLevel::Dangerous);
    }

    #[test]
    fn test_trust_level_needs_check() {
        // Balanced only checks Risky and Dangerous
        assert!(!TrustLevel::Balanced.needs_approval_check(RiskLevel::ReadOnly));
        assert!(!TrustLevel::Balanced.needs_approval_check(RiskLevel::Write));
        assert!(TrustLevel::Balanced.needs_approval_check(RiskLevel::Risky));
        assert!(TrustLevel::Balanced.needs_approval_check(RiskLevel::Dangerous));

        // Careful checks Write and above
        assert!(!TrustLevel::Careful.needs_approval_check(RiskLevel::ReadOnly));
        assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Write));
        assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Risky));
        assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Dangerous));

        // Manual checks everything
        assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::ReadOnly));
        assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Write));
        assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Risky));
        assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Dangerous));
    }

    #[test]
    fn test_trust_level_cycle() {
        assert_eq!(TrustLevel::Balanced.cycle_next(), TrustLevel::Careful);
        assert_eq!(TrustLevel::Careful.cycle_next(), TrustLevel::Manual);
        assert_eq!(TrustLevel::Manual.cycle_next(), TrustLevel::Balanced);
    }

    #[test]
    fn test_trust_level_index() {
        assert_eq!(TrustLevel::from_index(0), TrustLevel::Balanced);
        assert_eq!(TrustLevel::from_index(1), TrustLevel::Careful);
        assert_eq!(TrustLevel::from_index(2), TrustLevel::Manual);
        assert_eq!(TrustLevel::from_index(99), TrustLevel::Balanced); // fallback
    }

    #[test]
    fn test_trust_level_backward_compat() {
        // Old names should still parse
        assert_eq!(
            "ask_risky".parse::<TrustLevel>().unwrap(),
            TrustLevel::Balanced
        );
        assert_eq!(
            "only_reads".parse::<TrustLevel>().unwrap(),
            TrustLevel::Careful
        );
        assert_eq!(
            "zero_trust".parse::<TrustLevel>().unwrap(),
            TrustLevel::Manual
        );
        // New names
        assert_eq!(
            "balanced".parse::<TrustLevel>().unwrap(),
            TrustLevel::Balanced
        );
        assert_eq!(
            "careful".parse::<TrustLevel>().unwrap(),
            TrustLevel::Careful
        );
        assert_eq!("manual".parse::<TrustLevel>().unwrap(), TrustLevel::Manual);
    }
}
