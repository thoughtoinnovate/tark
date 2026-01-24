# Policy Configuration Refactoring - Summary

## What Changed

Successfully refactored the monolithic `builtin_policy.toml` (943 lines) into a loosely-coupled, split configuration system.

## New Architecture

### Split Configuration Files (in `src/policy/configs/`)

| File | Lines | Purpose |
|------|-------|---------|
| `modes.toml` | ~30 | Agent mode definitions (Ask, Plan, Build) |
| `trust.toml` | ~50 | Trust levels with default behaviors |
| `tools.toml` | ~300 | Tool declarations with metadata |
| `defaults.toml` | ~50 | Hierarchical approval defaults (replaces 250+ explicit rules) |
| **Total** | **~430** | **54% reduction from 943 lines** |

### Type-Safe Enums (in `src/policy/types.rs`)

```rust
pub enum ModeId { Ask, Plan, Build }
pub enum TrustId { Balanced, Careful, Manual }
```

**Benefits:**
- Single source of truth for mode/trust identifiers
- Compile-time errors instead of runtime string mismatches
- Easy conversion: `mode.as_str()`, `ModeId::from_str()`

### Dynamic Rule Resolution (new `src/policy/resolver.rs`)

**Before:** ~250 explicit approval rules in TOML
```toml
[[rules]]
classification = "readonly-safe"
mode = "build"
trust = "balanced"
in_workdir = true
needs_approval = false
# ...repeated 250+ times for each combination
```

**After:** ~18 hierarchical defaults
```toml
[approval_defaults]
safe.balanced.in_workdir = "auto_approve"
safe.balanced.out_workdir = "auto_approve"
moderate.balanced.in_workdir = "auto_approve"
moderate.balanced.out_workdir = "prompt"
# ... only 18 rules needed
```

The `RuleResolver` computes specific rules on-demand from these defaults.

### Tool Self-Declaration Pattern

Tools can now declare their own policy metadata:

```rust
impl Tool for ReadFileTool {
    fn policy_metadata(&self) -> Option<ToolPolicyMetadata> {
        Some(ToolPolicyMetadata {
            risk_level: RiskLevel::Safe,
            operation: Operation::Read,
            available_in_modes: &[ModeId::Ask, ModeId::Plan, ModeId::Build],
            classification_strategy: ClassificationStrategy::Static,
            category: Some("readonly"),
        })
    }
}
```

This moves policy declarations closer to implementation (better cohesion).

## Key Improvements

### 1. Loose Coupling

**Before:**
- Hardcoded strings scattered across 8+ files
- Mode IDs: `"ask"`, `"plan"`, `"build"`
- Trust IDs: `"balanced"`, `"careful"`, `"manual"`
- Tool names hardcoded in multiple places

**After:**
- Type-safe enums (`ModeId`, `TrustId`)
- Single source of truth
- Compile-time guarantees

### 2. Separation of Concerns

**Before:** One monolithic 943-line TOML file with:
- Mode definitions
- Trust levels
- Tool declarations
- Tool availability matrix
- Classifications
- Approval rules (250+ entries)
- Compound rules

**After:** Four focused files:
- `modes.toml` - Just modes
- `trust.toml` - Just trust levels
- `tools.toml` - Just tools
- `defaults.toml` - Just approval defaults

### 3. Reduced Redundancy

**Approval Rules:**
- Before: 250+ explicit rules
- After: 18 hierarchical defaults
- Reduction: **93% fewer rules**

**Example:** Every safe tool needed 6 explicit rules (3 trusts × 2 locations). Now it's covered by 2 defaults.

### 4. Better Extensibility

**Adding a new tool:**

Before:
```toml
# Add tool definition
[[tools]]
id = "new_tool"
...

# Add classification
[[classifications]]
id = "new_tool-safe"
...

# Add 6+ approval rules
[[rules]]
classification = "new_tool-safe"
mode = "build"
trust = "balanced"
...
# ...repeat 6+ times
```

After:
```toml
# Just add tool definition
[[tools]]
id = "new_tool"
risk = "safe"
modes = ["ask", "plan", "build"]
# Approval rules auto-generated from risk level!
```

### 5. Self-Documenting Code

**Before:** Tool policies defined far from implementation (in TOML)

**After:** Tools can self-declare (policy lives with code)

```rust
// Policy is right here with the tool!
fn policy_metadata(&self) -> Option<ToolPolicyMetadata> {
    Some(ToolPolicyMetadata {
        risk_level: RiskLevel::Safe,
        operation: Operation::Read,
        available_in_modes: &[ModeId::Ask, ModeId::Plan, ModeId::Build],
        ...
    })
}
```

## Files Changed

### New Files
- `src/policy/configs/modes.toml`
- `src/policy/configs/trust.toml`
- `src/policy/configs/tools.toml`
- `src/policy/configs/defaults.toml`
- `src/policy/resolver.rs`

### Modified Files
- `src/policy/types.rs` - Added `ModeId`, `TrustId`, `ToolPolicyMetadata`
- `src/policy/mod.rs` - Export new types and resolver
- `src/policy/seed.rs` - Load split configs, generate rules from defaults
- `src/tools/mod.rs` - Use type-safe enums, add `policy_metadata()` to Tool trait
- `src/tools/file_ops.rs` - Example implementation of `policy_metadata()`
- `src/policy/README.md` - Updated documentation

### Removed Files
- `src/policy/builtin_policy.toml` (943 lines) - Replaced by split configs

## Migration Path

The refactoring is **backward compatible** at the database level:
- Same SQLite schema (15 tables)
- Same seeding process
- Rules are generated dynamically from defaults
- Existing user patterns and MCP configs unchanged

## Benefits Summary

| Metric | Before | After | Improvement |
|--------|--------|-------|-------------|
| Config file size | 943 lines | ~430 lines | **54% reduction** |
| Explicit approval rules | 250+ | 18 defaults | **93% reduction** |
| Hardcoded mode/trust strings | 8+ locations | 0 (enums) | **100% elimination** |
| Files to maintain | 1 monolithic | 4 focused | **Better organization** |
| Type safety | Strings | Enums | **Compile-time checks** |
| Adding new tool | 10+ TOML entries | 1 TOML entry | **90% less config** |

## Next Steps (Optional)

1. **Migrate more tools** to use `policy_metadata()` self-declaration
2. **Add more sophisticated defaults** (e.g., per-category defaults)
3. **Cache resolved rules** in `RuleResolver` for performance
4. **Remove external tool configs** once all tools self-declare
5. **Version the config schema** for future migrations

## Testing

All existing tests pass:
```bash
cargo build --release  # ✓ Compiles successfully
cargo test --lib policy  # ✓ All policy tests pass
cargo clippy  # ✓ No warnings (after fixing unused imports)
```

The refactoring maintains the same runtime behavior while dramatically improving maintainability and extensibility.
