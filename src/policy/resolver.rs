use crate::policy::types::{RiskLevel, TrustId};
use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

/// Approval behavior for a given rule
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalBehavior {
    /// Auto-approve without prompting
    AutoApprove,
    /// Prompt user, allow saving pattern
    Prompt,
    /// Prompt user, disallow saving pattern
    PromptNoSave,
}

impl std::str::FromStr for ApprovalBehavior {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto_approve" => Ok(ApprovalBehavior::AutoApprove),
            "prompt" => Ok(ApprovalBehavior::Prompt),
            "prompt_no_save" => Ok(ApprovalBehavior::PromptNoSave),
            _ => Err(format!("Invalid approval behavior: {}", s)),
        }
    }
}

impl ApprovalBehavior {
    pub fn needs_approval(&self) -> bool {
        matches!(
            self,
            ApprovalBehavior::Prompt | ApprovalBehavior::PromptNoSave
        )
    }

    pub fn allow_save_pattern(&self) -> bool {
        matches!(self, ApprovalBehavior::Prompt)
    }
}

/// Key for looking up approval rules
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RuleKey {
    pub risk: RiskLevel,
    pub trust: TrustId,
    pub in_workdir: bool,
}

/// Default approval policies loaded from config
#[derive(Debug, Clone)]
pub struct ApprovalDefaults {
    rules: HashMap<RuleKey, ApprovalBehavior>,
}

impl ApprovalDefaults {
    /// Create empty defaults
    pub fn new() -> Self {
        Self {
            rules: HashMap::new(),
        }
    }

    /// Insert a default rule
    pub fn insert(&mut self, key: RuleKey, behavior: ApprovalBehavior) {
        self.rules.insert(key, behavior);
    }

    /// Get approval behavior for a given rule key
    pub fn get(&self, risk: RiskLevel, trust: TrustId, in_workdir: bool) -> ApprovalBehavior {
        let key = RuleKey {
            risk,
            trust,
            in_workdir,
        };

        self.rules.get(&key).copied().unwrap_or_else(|| {
            // Fallback: safer default
            tracing::warn!("No default rule found for {:?}, defaulting to Prompt", key);
            ApprovalBehavior::Prompt
        })
    }
}

impl Default for ApprovalDefaults {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolves approval decisions dynamically from defaults and overrides
pub struct RuleResolver {
    defaults: ApprovalDefaults,
    overrides: HashMap<RuleKey, ApprovalBehavior>,
}

impl RuleResolver {
    /// Create a new resolver with the given defaults
    pub fn new(defaults: ApprovalDefaults) -> Self {
        Self {
            defaults,
            overrides: HashMap::new(),
        }
    }

    /// Add an override for a specific rule
    pub fn add_override(&mut self, key: RuleKey, behavior: ApprovalBehavior) {
        self.overrides.insert(key, behavior);
    }

    /// Resolve approval behavior for a given context
    pub fn resolve(&self, risk: RiskLevel, trust: TrustId, in_workdir: bool) -> ApprovalBehavior {
        let key = RuleKey {
            risk,
            trust,
            in_workdir,
        };

        // 1. Check for specific override
        if let Some(&behavior) = self.overrides.get(&key) {
            return behavior;
        }

        // 2. Fall back to defaults
        self.defaults.get(risk, trust, in_workdir)
    }

    /// Resolve with detailed decision information
    pub fn resolve_decision(
        &self,
        risk: RiskLevel,
        trust: TrustId,
        in_workdir: bool,
    ) -> ResolvedDecision {
        let behavior = self.resolve(risk, trust, in_workdir);
        let key = RuleKey {
            risk,
            trust,
            in_workdir,
        };

        ResolvedDecision {
            needs_approval: behavior.needs_approval(),
            allow_save_pattern: behavior.allow_save_pattern(),
            behavior,
            was_override: self.overrides.contains_key(&key),
            rationale: self.generate_rationale(risk, trust, in_workdir, behavior),
        }
    }

    fn generate_rationale(
        &self,
        risk: RiskLevel,
        trust: TrustId,
        in_workdir: bool,
        behavior: ApprovalBehavior,
    ) -> String {
        let location = if in_workdir {
            "in working directory"
        } else {
            "outside working directory"
        };

        match behavior {
            ApprovalBehavior::AutoApprove => {
                format!(
                    "Auto-approved: {:?} risk with {:?} trust {}",
                    risk, trust, location
                )
            }
            ApprovalBehavior::Prompt => {
                format!(
                    "Approval required: {:?} risk with {:?} trust {}",
                    risk, trust, location
                )
            }
            ApprovalBehavior::PromptNoSave => {
                format!(
                    "Approval required (no pattern save): {:?} risk with {:?} trust {}",
                    risk, trust, location
                )
            }
        }
    }
}

/// Result of resolving an approval decision
#[derive(Debug, Clone)]
pub struct ResolvedDecision {
    pub needs_approval: bool,
    pub allow_save_pattern: bool,
    pub behavior: ApprovalBehavior,
    pub was_override: bool,
    pub rationale: String,
}

/// Parse approval defaults from TOML config
#[derive(Debug, Deserialize)]
pub struct DefaultsConfig {
    pub approval_defaults: HashMap<String, String>,
}

impl DefaultsConfig {
    /// Parse from TOML string
    pub fn from_toml(toml: &str) -> Result<Self> {
        Ok(toml::from_str(toml)?)
    }

    /// Convert to ApprovalDefaults
    pub fn to_approval_defaults(&self) -> Result<ApprovalDefaults> {
        let mut defaults = ApprovalDefaults::new();

        for (key, value) in &self.approval_defaults {
            let parts: Vec<&str> = key.split('.').collect();
            if parts.len() != 3 {
                tracing::warn!("Invalid default key format: {}", key);
                continue;
            }

            let risk = match parts[0] {
                "safe" => RiskLevel::Safe,
                "moderate" => RiskLevel::Moderate,
                "dangerous" => RiskLevel::Dangerous,
                _ => {
                    tracing::warn!("Invalid risk level: {}", parts[0]);
                    continue;
                }
            };

            let trust = match parts[1] {
                "balanced" => TrustId::Balanced,
                "careful" => TrustId::Careful,
                "manual" => TrustId::Manual,
                _ => {
                    tracing::warn!("Invalid trust level: {}", parts[1]);
                    continue;
                }
            };

            let in_workdir = match parts[2] {
                "in_workdir" => true,
                "out_workdir" => false,
                _ => {
                    tracing::warn!("Invalid location: {}", parts[2]);
                    continue;
                }
            };

            let behavior = match value.parse::<ApprovalBehavior>() {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!("Invalid approval behavior: {}", e);
                    continue;
                }
            };

            defaults.insert(
                RuleKey {
                    risk,
                    trust,
                    in_workdir,
                },
                behavior,
            );
        }

        Ok(defaults)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_defaults() {
        let mut defaults = ApprovalDefaults::new();

        defaults.insert(
            RuleKey {
                risk: RiskLevel::Safe,
                trust: TrustId::Balanced,
                in_workdir: true,
            },
            ApprovalBehavior::AutoApprove,
        );

        let behavior = defaults.get(RiskLevel::Safe, TrustId::Balanced, true);
        assert_eq!(behavior, ApprovalBehavior::AutoApprove);
    }

    #[test]
    fn test_rule_resolver() {
        let mut defaults = ApprovalDefaults::new();
        defaults.insert(
            RuleKey {
                risk: RiskLevel::Moderate,
                trust: TrustId::Careful,
                in_workdir: true,
            },
            ApprovalBehavior::Prompt,
        );

        let resolver = RuleResolver::new(defaults);
        let behavior = resolver.resolve(RiskLevel::Moderate, TrustId::Careful, true);
        assert_eq!(behavior, ApprovalBehavior::Prompt);
    }

    #[test]
    fn test_rule_resolver_with_override() {
        let mut defaults = ApprovalDefaults::new();
        defaults.insert(
            RuleKey {
                risk: RiskLevel::Moderate,
                trust: TrustId::Balanced,
                in_workdir: true,
            },
            ApprovalBehavior::AutoApprove,
        );

        let mut resolver = RuleResolver::new(defaults);
        resolver.add_override(
            RuleKey {
                risk: RiskLevel::Moderate,
                trust: TrustId::Balanced,
                in_workdir: true,
            },
            ApprovalBehavior::Prompt,
        );

        let behavior = resolver.resolve(RiskLevel::Moderate, TrustId::Balanced, true);
        assert_eq!(behavior, ApprovalBehavior::Prompt);
    }

    #[test]
    fn test_parse_defaults_config() {
        let toml = r#"
[approval_defaults]
"safe.balanced.in_workdir" = "auto_approve"
"moderate.careful.out_workdir" = "prompt"
"dangerous.manual.out_workdir" = "prompt_no_save"
        "#;

        let config = DefaultsConfig::from_toml(toml).unwrap();
        let defaults = config.to_approval_defaults().unwrap();

        assert_eq!(
            defaults.get(RiskLevel::Safe, TrustId::Balanced, true),
            ApprovalBehavior::AutoApprove
        );
        assert_eq!(
            defaults.get(RiskLevel::Moderate, TrustId::Careful, false),
            ApprovalBehavior::Prompt
        );
        assert_eq!(
            defaults.get(RiskLevel::Dangerous, TrustId::Manual, false),
            ApprovalBehavior::PromptNoSave
        );
    }
}
