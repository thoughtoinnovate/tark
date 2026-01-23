use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Type-safe mode identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModeId {
    Ask,
    Plan,
    Build,
}

impl ModeId {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            ModeId::Ask => "ask",
            ModeId::Plan => "plan",
            ModeId::Build => "build",
        }
    }

    /// Get all mode IDs
    pub fn all() -> &'static [ModeId] {
        &[ModeId::Ask, ModeId::Plan, ModeId::Build]
    }
}

impl FromStr for ModeId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "ask" => Ok(ModeId::Ask),
            "plan" => Ok(ModeId::Plan),
            "build" => Ok(ModeId::Build),
            _ => Err(format!("Invalid mode: {}", s)),
        }
    }
}

impl fmt::Display for ModeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Type-safe trust level identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum TrustId {
    #[default]
    Balanced,
    Careful,
    Manual,
}

impl TrustId {
    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            TrustId::Balanced => "balanced",
            TrustId::Careful => "careful",
            TrustId::Manual => "manual",
        }
    }

    /// Get all trust IDs
    pub fn all() -> &'static [TrustId] {
        &[TrustId::Balanced, TrustId::Careful, TrustId::Manual]
    }
}

impl FromStr for TrustId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "balanced" => Ok(TrustId::Balanced),
            "careful" => Ok(TrustId::Careful),
            "manual" => Ok(TrustId::Manual),
            _ => Err(format!("Invalid trust level: {}", s)),
        }
    }
}

impl fmt::Display for TrustId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<crate::tools::TrustLevel> for TrustId {
    fn from(level: crate::tools::TrustLevel) -> Self {
        match level {
            crate::tools::TrustLevel::Balanced => TrustId::Balanced,
            crate::tools::TrustLevel::Careful => TrustId::Careful,
            crate::tools::TrustLevel::Manual => TrustId::Manual,
        }
    }
}

/// Operation type for command classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Operation {
    Read,
    Write,
    Delete,
    Execute,
}

impl std::fmt::Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operation::Read => write!(f, "read"),
            Operation::Write => write!(f, "write"),
            Operation::Delete => write!(f, "delete"),
            Operation::Execute => write!(f, "execute"),
        }
    }
}

/// Risk level for operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Safe,
    Moderate,
    Dangerous,
}

impl std::fmt::Display for RiskLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RiskLevel::Safe => write!(f, "safe"),
            RiskLevel::Moderate => write!(f, "moderate"),
            RiskLevel::Dangerous => write!(f, "dangerous"),
        }
    }
}

/// Result of classifying a command
#[derive(Debug, Clone)]
pub struct CommandClassification {
    /// Classification ID (e.g., "shell-read", "shell-write", "shell-rm")
    pub classification_id: String,
    /// Operation type
    pub operation: Operation,
    /// Whether paths are within working directory
    pub in_workdir: bool,
    /// Risk level based on operation
    pub risk_level: RiskLevel,
}

/// Pattern matching result
#[derive(Debug, Clone)]
pub struct PatternMatch {
    pub pattern_id: i64,
    pub pattern: String,
    pub match_type: MatchType,
    pub is_denial: bool,
}

/// Pattern match types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchType {
    Exact,
    Prefix,
    Glob,
}

impl std::fmt::Display for MatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchType::Exact => write!(f, "exact"),
            MatchType::Prefix => write!(f, "prefix"),
            MatchType::Glob => write!(f, "glob"),
        }
    }
}

/// Result of approval check
#[derive(Debug, Clone)]
pub struct ApprovalDecision {
    /// Whether approval is needed
    pub needs_approval: bool,
    /// Whether user can save pattern to skip future prompts
    pub allow_save_pattern: bool,
    /// Classification of the command
    pub classification: CommandClassification,
    /// Matched pattern (if any)
    pub matched_pattern: Option<PatternMatch>,
    /// Rationale for the decision
    pub rationale: String,
}

/// Audit log entry
#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub timestamp: String,
    pub tool_id: String,
    pub command: String,
    pub classification_id: Option<String>,
    pub mode_id: String,
    pub trust_id: Option<String>,
    pub decision: ApprovalDecisionType,
    pub matched_pattern_id: Option<i64>,
    pub session_id: String,
    pub working_directory: String,
}

/// Types of approval decisions for audit log
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalDecisionType {
    AutoApproved,
    UserApproved,
    UserDenied,
    PatternMatched,
    PatternDenied,
    Blocked,
}

impl std::fmt::Display for ApprovalDecisionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ApprovalDecisionType::AutoApproved => write!(f, "auto_approved"),
            ApprovalDecisionType::UserApproved => write!(f, "user_approved"),
            ApprovalDecisionType::UserDenied => write!(f, "user_denied"),
            ApprovalDecisionType::PatternMatched => write!(f, "pattern_matched"),
            ApprovalDecisionType::PatternDenied => write!(f, "pattern_denied"),
            ApprovalDecisionType::Blocked => write!(f, "blocked"),
        }
    }
}

/// Tool information
#[derive(Debug, Clone)]
pub struct ToolInfo {
    pub id: String,
    pub name: String,
    pub category: String,
    pub permissions: String,
    pub base_risk: RiskLevel,
}

/// Classification strategy for tools
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassificationStrategy {
    /// Tool has a static classification (most tools)
    Static,
    /// Tool classification depends on runtime command (e.g., shell)
    Dynamic,
}

/// Policy metadata that tools can self-declare
#[derive(Debug, Clone)]
pub struct ToolPolicyMetadata {
    /// Base risk level of the tool
    pub risk_level: RiskLevel,
    /// Primary operation type
    pub operation: Operation,
    /// Modes where this tool is available
    pub available_in_modes: &'static [ModeId],
    /// How the tool should be classified
    pub classification_strategy: ClassificationStrategy,
    /// Optional tool category
    pub category: Option<&'static str>,
}

impl Default for ToolPolicyMetadata {
    fn default() -> Self {
        Self {
            risk_level: RiskLevel::Safe,
            operation: Operation::Read,
            available_in_modes: &[ModeId::Ask, ModeId::Plan, ModeId::Build],
            classification_strategy: ClassificationStrategy::Static,
            category: None,
        }
    }
}

/// MCP tool policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPolicy {
    pub risk_level: RiskLevel,
    pub needs_approval: bool,
    pub allow_save_pattern: bool,
}

impl Default for McpPolicy {
    fn default() -> Self {
        Self {
            risk_level: RiskLevel::Moderate,
            needs_approval: true,
            allow_save_pattern: true,
        }
    }
}

/// User-defined approval pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalPattern {
    pub id: Option<i64>,
    pub tool: String,
    pub pattern: String,
    pub match_type: MatchType,
    pub is_denial: bool,
    pub source: PatternSource,
    pub description: Option<String>,
}

/// Approval pattern entry for display in UI
#[derive(Debug, Clone)]
pub struct ApprovalPatternEntry {
    pub id: i64,
    pub tool: String,
    pub pattern: String,
    pub match_type: String,
    pub is_denial: bool,
    pub description: Option<String>,
}

/// Source of a pattern
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternSource {
    User,
    Workspace,
    Session,
}

impl std::fmt::Display for PatternSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternSource::User => write!(f, "user"),
            PatternSource::Workspace => write!(f, "workspace"),
            PatternSource::Session => write!(f, "session"),
        }
    }
}
