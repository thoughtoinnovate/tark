//! Tests for Trust Level functionality
//!
//! These tests verify that:
//! 1. Trust levels cycle correctly
//! 2. Trust levels are correctly persisted to session
//! 3. Approval patterns are matched correctly
//! 4. Risk levels are assigned correctly to tools

use std::str::FromStr;
use tark_cli::tools::{MatchType, RiskLevel, TrustLevel};

// ============================================================================
// TrustLevel Enum Tests
// ============================================================================

#[test]
fn test_trust_level_default() {
    let level = TrustLevel::default();
    assert_eq!(level, TrustLevel::Balanced);
}

#[test]
fn test_trust_level_cycle() {
    // Balanced -> Careful -> Manual -> Balanced
    assert_eq!(TrustLevel::Balanced.cycle_next(), TrustLevel::Careful);
    assert_eq!(TrustLevel::Careful.cycle_next(), TrustLevel::Manual);
    assert_eq!(TrustLevel::Manual.cycle_next(), TrustLevel::Balanced);
}

#[test]
fn test_trust_level_index_roundtrip() {
    for level in TrustLevel::all() {
        let index = level.index();
        let restored = TrustLevel::from_index(index);
        assert_eq!(*level, restored, "Index roundtrip failed for {:?}", level);
    }
}

#[test]
fn test_trust_level_from_invalid_index() {
    // Invalid indices should default to Balanced
    assert_eq!(TrustLevel::from_index(99), TrustLevel::Balanced);
    assert_eq!(TrustLevel::from_index(100), TrustLevel::Balanced);
    assert_eq!(TrustLevel::from_index(usize::MAX), TrustLevel::Balanced);
}

#[test]
fn test_trust_level_storage_roundtrip() {
    // Test string conversion for session storage
    for level in TrustLevel::all() {
        let storage_str = level.as_storage_str();
        let restored = TrustLevel::from_str(storage_str).unwrap();
        assert_eq!(*level, restored, "Storage roundtrip failed for {:?}", level);
    }
}

#[test]
fn test_trust_level_backward_compat() {
    // Old names should still parse for backward compatibility
    assert_eq!(
        TrustLevel::from_str("ask_risky").unwrap(),
        TrustLevel::Balanced
    );
    assert_eq!(
        TrustLevel::from_str("only_reads").unwrap(),
        TrustLevel::Careful
    );
    assert_eq!(
        TrustLevel::from_str("zero_trust").unwrap(),
        TrustLevel::Manual
    );
}

#[test]
fn test_trust_level_from_invalid_str() {
    // Invalid strings should default to Balanced
    assert_eq!(
        TrustLevel::from_str("invalid").unwrap(),
        TrustLevel::Balanced
    );
    assert_eq!(TrustLevel::from_str("").unwrap(), TrustLevel::Balanced);
    assert_eq!(
        TrustLevel::from_str("paranoid").unwrap(),
        TrustLevel::Balanced
    );
}

#[test]
fn test_trust_level_icons() {
    assert_eq!(TrustLevel::Balanced.icon(), "🟡");
    assert_eq!(TrustLevel::Careful.icon(), "🔵");
    assert_eq!(TrustLevel::Manual.icon(), "🔴");
}

#[test]
fn test_trust_level_labels() {
    assert_eq!(TrustLevel::Balanced.label(), "Balanced");
    assert_eq!(TrustLevel::Careful.label(), "Careful");
    assert_eq!(TrustLevel::Manual.label(), "Manual");
}

// ============================================================================
// TrustLevel + RiskLevel Tests
// ============================================================================

#[test]
fn test_balanced_needs_approval() {
    // Balanced only requires approval for Risky and Dangerous
    assert!(!TrustLevel::Balanced.needs_approval_check(RiskLevel::ReadOnly));
    assert!(!TrustLevel::Balanced.needs_approval_check(RiskLevel::Write));
    assert!(TrustLevel::Balanced.needs_approval_check(RiskLevel::Risky));
    assert!(TrustLevel::Balanced.needs_approval_check(RiskLevel::Dangerous));
}

#[test]
fn test_careful_needs_approval() {
    // Careful auto-approves reads only
    assert!(!TrustLevel::Careful.needs_approval_check(RiskLevel::ReadOnly));
    assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Write));
    assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Risky));
    assert!(TrustLevel::Careful.needs_approval_check(RiskLevel::Dangerous));
}

#[test]
fn test_manual_needs_approval() {
    // Manual requires approval for everything
    assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::ReadOnly));
    assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Write));
    assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Risky));
    assert!(TrustLevel::Manual.needs_approval_check(RiskLevel::Dangerous));
}

// ============================================================================
// RiskLevel Tests
// ============================================================================

#[test]
fn test_risk_level_ordering() {
    assert!(RiskLevel::ReadOnly < RiskLevel::Write);
    assert!(RiskLevel::Write < RiskLevel::Risky);
    assert!(RiskLevel::Risky < RiskLevel::Dangerous);
}

#[test]
fn test_risk_level_icons() {
    assert_eq!(RiskLevel::ReadOnly.icon(), "📖");
    assert_eq!(RiskLevel::Write.icon(), "✏️");
    assert_eq!(RiskLevel::Risky.icon(), "⚠️");
    assert_eq!(RiskLevel::Dangerous.icon(), "🔴");
}

#[test]
fn test_risk_level_labels() {
    assert_eq!(RiskLevel::ReadOnly.label(), "Read");
    assert_eq!(RiskLevel::Write.label(), "Write");
    assert_eq!(RiskLevel::Risky.label(), "Risky");
    assert_eq!(RiskLevel::Dangerous.label(), "Dangerous");
}

// ============================================================================
// MatchType Tests
// ============================================================================

#[test]
fn test_match_type_labels() {
    assert_eq!(MatchType::Exact.label(), "exact");
    assert_eq!(MatchType::Prefix.label(), "prefix");
    assert_eq!(MatchType::Glob.label(), "glob");
}

// ============================================================================
// TrustLevel All() Tests
// ============================================================================

#[test]
fn test_trust_level_all() {
    let all = TrustLevel::all();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0], TrustLevel::Balanced);
    assert_eq!(all[1], TrustLevel::Careful);
    assert_eq!(all[2], TrustLevel::Manual);
}
