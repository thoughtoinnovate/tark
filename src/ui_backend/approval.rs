//! Approval card types for risky operations

use serde::{Deserialize, Serialize};

use crate::tools::MatchType;

/// Approval item - unified list item (action or pattern)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApprovalItem {
    /// Run this command once
    #[default]
    RunOnce,
    /// Always allow this exact command
    AlwaysAllow,
    /// Allow with a specific pattern (index into suggested_patterns)
    Pattern(usize),
    /// Skip/deny this command
    Skip,
}

/// Approval action that can be selected (legacy, for compatibility)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ApprovalAction {
    /// Run this command once
    #[default]
    RunOnce,
    /// Always allow this exact command
    AlwaysAllow,
    /// Allow with pattern matching
    PatternMatch,
    /// Skip/deny this command
    Skip,
}

impl ApprovalAction {
    /// Get all actions in order
    pub fn all() -> &'static [ApprovalAction] {
        &[
            ApprovalAction::RunOnce,
            ApprovalAction::AlwaysAllow,
            ApprovalAction::PatternMatch,
            ApprovalAction::Skip,
        ]
    }

    /// Get index for this action
    pub fn index(&self) -> usize {
        match self {
            ApprovalAction::RunOnce => 0,
            ApprovalAction::AlwaysAllow => 1,
            ApprovalAction::PatternMatch => 2,
            ApprovalAction::Skip => 3,
        }
    }

    /// Create from index
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => ApprovalAction::RunOnce,
            1 => ApprovalAction::AlwaysAllow,
            2 => ApprovalAction::PatternMatch,
            3 => ApprovalAction::Skip,
            _ => ApprovalAction::RunOnce,
        }
    }
}

/// Approval card state for risky operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalCardState {
    /// The operation being requested
    pub operation: String,
    /// Risk level of the operation
    pub risk_level: RiskLevel,
    /// Detailed description of what will happen
    pub description: String,
    /// Command that will be executed (for display)
    pub command: String,
    /// Files/paths that will be affected
    pub affected_paths: Vec<String>,
    /// Tool call ID this approval is for
    pub tool_call_id: Option<String>,
    /// Suggested approval patterns for the user
    pub suggested_patterns: Vec<ApprovalPatternOption>,
    /// Currently selected index in the unified list
    /// Order: RunOnce, AlwaysAllow, Pattern0, Pattern1, ..., Skip
    pub selected_index: usize,
    /// Selected suggested pattern index (legacy, for compatibility)
    pub selected_pattern: usize,
    /// Currently selected action (legacy, for compatibility)
    pub selected_action: ApprovalAction,
    /// Whether the user has responded
    pub responded: bool,
    /// Whether the user approved
    pub approved: bool,
}

/// Risk level for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Read-only, safe operations
    Safe,
    /// Write operations (file modifications)
    Write,
    /// Risky operations (shell commands, deletions)
    Risky,
    /// Dangerous operations (destructive, irreversible)
    Dangerous,
}

/// Suggested approval pattern option for UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPatternOption {
    pub pattern: String,
    pub match_type: MatchType,
    pub description: String,
}

impl Default for ApprovalCardState {
    fn default() -> Self {
        Self {
            operation: String::new(),
            risk_level: RiskLevel::Safe,
            description: String::new(),
            command: String::new(),
            affected_paths: Vec::new(),
            tool_call_id: None,
            suggested_patterns: Vec::new(),
            selected_index: 0,
            selected_pattern: 0,
            selected_action: ApprovalAction::default(),
            responded: false,
            approved: false,
        }
    }
}

impl ApprovalCardState {
    /// Create a new approval request
    pub fn new(
        operation: String,
        risk_level: RiskLevel,
        description: String,
        command: String,
        affected_paths: Vec<String>,
        suggested_patterns: Vec<ApprovalPatternOption>,
    ) -> Self {
        Self {
            operation,
            risk_level,
            description,
            command,
            affected_paths,
            tool_call_id: None,
            suggested_patterns,
            selected_index: 0,
            selected_pattern: 0,
            selected_action: ApprovalAction::default(),
            responded: false,
            approved: false,
        }
    }

    /// Get total number of items in the unified list
    /// Order: RunOnce, AlwaysAllow, Pattern0, Pattern1, ..., Skip
    pub fn total_items(&self) -> usize {
        2 + self.suggested_patterns.len() + 1 // RunOnce + AlwaysAllow + patterns + Skip
    }

    /// Get the currently selected item
    pub fn get_selected_item(&self) -> ApprovalItem {
        let pattern_count = self.suggested_patterns.len();
        match self.selected_index {
            0 => ApprovalItem::RunOnce,
            1 => ApprovalItem::AlwaysAllow,
            n if n >= 2 && n < 2 + pattern_count => ApprovalItem::Pattern(n - 2),
            _ => ApprovalItem::Skip,
        }
    }

    /// Move selection to previous item in unified list
    pub fn select_prev(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
        self.sync_legacy_fields();
    }

    /// Move selection to next item in unified list
    pub fn select_next(&mut self) {
        if self.selected_index + 1 < self.total_items() {
            self.selected_index += 1;
        }
        self.sync_legacy_fields();
    }

    /// Sync legacy fields for compatibility
    fn sync_legacy_fields(&mut self) {
        match self.get_selected_item() {
            ApprovalItem::RunOnce => self.selected_action = ApprovalAction::RunOnce,
            ApprovalItem::AlwaysAllow => self.selected_action = ApprovalAction::AlwaysAllow,
            ApprovalItem::Pattern(idx) => {
                self.selected_action = ApprovalAction::PatternMatch;
                self.selected_pattern = idx;
            }
            ApprovalItem::Skip => self.selected_action = ApprovalAction::Skip,
        }
    }

    /// Move selection to previous suggested pattern (legacy)
    pub fn select_prev_pattern(&mut self) {
        if self.selected_pattern > 0 {
            self.selected_pattern -= 1;
        }
    }

    /// Move selection to next suggested pattern (legacy)
    pub fn select_next_pattern(&mut self) {
        if self.selected_pattern + 1 < self.suggested_patterns.len() {
            self.selected_pattern += 1;
        }
    }

    /// Move selection to previous action (legacy)
    pub fn select_prev_action(&mut self) {
        self.select_prev();
    }

    /// Move selection to next action (legacy)
    pub fn select_next_action(&mut self) {
        self.select_next();
    }

    /// Get the currently selected action (legacy)
    pub fn get_selected_action(&self) -> ApprovalAction {
        self.selected_action
    }

    /// Approve the operation
    pub fn approve(&mut self) {
        self.responded = true;
        self.approved = true;
    }

    /// Reject the operation
    pub fn reject(&mut self) {
        self.responded = true;
        self.approved = false;
    }

    /// Get color for risk level
    pub fn risk_color(&self) -> &'static str {
        match self.risk_level {
            RiskLevel::Safe => "green",
            RiskLevel::Write => "yellow",
            RiskLevel::Risky => "orange",
            RiskLevel::Dangerous => "red",
        }
    }

    /// Get icon for risk level
    pub fn risk_icon(&self) -> &'static str {
        match self.risk_level {
            RiskLevel::Safe => "✓",
            RiskLevel::Write => "✎",
            RiskLevel::Risky => "⚠",
            RiskLevel::Dangerous => "⚡",
        }
    }
}
