//! Canonical type definitions for the core domain
//!
//! This module contains the single source of truth for types used across
//! multiple modules (tools, ui_backend, services) to prevent type drift.
//!
//! All other modules should `pub use` these types rather than defining their own.

use serde::{Deserialize, Serialize};

/// Agent operation mode determines which tools are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentMode {
    /// Ask mode: read-only tools for exploring and answering questions
    Ask,
    /// Plan mode: read-only tools + propose_change for planning
    Plan,
    /// Build mode: all tools for executing changes
    #[default]
    Build,
}

impl AgentMode {
    /// Get display label for this mode
    pub fn label(&self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Plan => "Plan",
            Self::Build => "Build",
        }
    }

    /// Get display name (same as label)
    pub fn display_name(&self) -> &'static str {
        self.label()
    }

    /// Get icon for this mode
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Ask => "❓",
            Self::Plan => "📋",
            Self::Build => "🔨",
        }
    }

    /// Get description for this mode
    pub fn description(&self) -> &'static str {
        match self {
            Self::Ask => "Read-only exploration and Q&A",
            Self::Plan => "Read + propose changes (no execution)",
            Self::Build => "Full access to read, write, and execute",
        }
    }

    /// Get the next mode in the cycle (Build → Plan → Ask → Build)
    pub fn next(self) -> Self {
        match self {
            Self::Build => Self::Plan,
            Self::Plan => Self::Ask,
            Self::Ask => Self::Build,
        }
    }

    /// Get the previous mode in the cycle (Build → Ask → Plan → Build)
    pub fn prev(self) -> Self {
        match self {
            Self::Build => Self::Ask,
            Self::Ask => Self::Plan,
            Self::Plan => Self::Build,
        }
    }
}

impl std::str::FromStr for AgentMode {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.to_lowercase().as_str() {
            "ask" => Self::Ask,
            "plan" => Self::Plan,
            _ => Self::Build,
        })
    }
}

impl From<&str> for AgentMode {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "ask" => Self::Ask,
            "plan" => Self::Plan,
            _ => Self::Build,
        }
    }
}

impl std::fmt::Display for AgentMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Build => write!(f, "build"),
            Self::Plan => write!(f, "plan"),
            Self::Ask => write!(f, "ask"),
        }
    }
}

/// Build mode (only active in Build agent mode)
/// Controls the trust level for risky operations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildMode {
    /// Manual: prompt for every risky operation
    Manual,
    /// Balanced: use learned patterns + prompt for new operations
    #[default]
    Balanced,
    /// Careful: more conservative pattern matching
    Careful,
}

impl BuildMode {
    /// Get the next mode in the cycle
    pub fn next(self) -> Self {
        match self {
            Self::Manual => Self::Balanced,
            Self::Balanced => Self::Careful,
            Self::Careful => Self::Manual,
        }
    }

    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Manual => "Manual",
            Self::Balanced => "Balanced",
            Self::Careful => "Careful",
        }
    }

    /// Get description
    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Self::Manual => "Prompt for every risky operation",
            Self::Balanced => "Learn patterns, prompt for new operations",
            Self::Careful => "Conservative pattern matching",
        }
    }
}

/// Thinking level for extended reasoning
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThinkLevel {
    /// No extended thinking
    #[default]
    Off,
    /// Low effort thinking (budget_tokens: 1000)
    Low,
    /// Normal effort thinking (budget_tokens: 3000)
    Normal,
    /// High effort thinking (budget_tokens: 10000)
    High,
}

impl ThinkLevel {
    /// Get the budget tokens for this thinking level
    #[allow(dead_code)]
    pub fn budget_tokens(&self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Low => Some(1000),
            Self::Normal => Some(3000),
            Self::High => Some(10000),
        }
    }

    /// Get display name
    #[allow(dead_code)]
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
        }
    }
}

impl From<&str> for ThinkLevel {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "off" => Self::Off,
            "low" => Self::Low,
            "normal" => Self::Normal,
            "high" => Self::High,
            _ => Self::Off,
        }
    }
}

impl std::fmt::Display for ThinkLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Low => write!(f, "low"),
            Self::Normal => write!(f, "normal"),
            Self::High => write!(f, "high"),
        }
    }
}
