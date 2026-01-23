use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
