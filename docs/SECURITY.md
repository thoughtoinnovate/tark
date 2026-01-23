# Security Model - Policy Database Integrity

## Overview

The tark policy database (`policy.db`) contains critical security configuration that controls what operations require approval. To prevent tampering, tark implements integrity verification with automatic recovery.

## Threat Model

### What We Protect Against

✅ **Application Bugs** - Prevents tark itself from accidentally corrupting builtin policy  
✅ **Accidental Modifications** - Detects if someone accidentally modifies builtin tables  
✅ **Basic Tampering** - Detects modifications to builtin policy tables  
✅ **Database Corruption** - Identifies corrupted builtin data and auto-repairs

### What We DON'T Protect Against

❌ **Malicious Local User** - If an attacker has write access to `.tark/policy.db`, they can modify it  
❌ **System Compromise** - If the system is compromised, all bets are off  
❌ **Malware** - Malware running with user privileges can modify the database  

### Security Boundaries

The security model assumes:
- **File system permissions** are the primary security boundary
- Users with write access to `.tark/` directory are trusted
- The goal is **detection** and **recovery**, not **prevention**

## How It Works

### 1. Database Structure

`policy.db` contains two types of data:

**Builtin Tables (Protected by Integrity Hash):**
- `agent_modes` - Ask, Plan, Build modes
- `trust_levels` - Balanced, Careful, Manual
- `tool_types` - Builtin tool definitions
- `tool_categories` - Tool categorization
- `tool_classifications` - Operation classifications
- `approval_rules` - Approval decision matrix
- `tool_mode_availability` - Tool availability per mode
- `compound_command_rules` - Shell command composition rules

**User Data Tables (Not Protected):**
- `approval_patterns` - User's saved approval patterns
- `mcp_approval_patterns` - MCP tool approval patterns
- `approval_audit_log` - Audit trail
- `integrity_metadata` - Integrity hash storage

### 2. Integrity Verification

**On Startup:**

```
1. Open policy.db
2. Create tables (if first run)
3. Seed builtin policy (if empty)
4. Calculate SHA-256 hash of all builtin tables
5. Compare with stored hash
6. If mismatch → AUTO-REPAIR
   - Clear builtin tables
   - Reseed from embedded configs
   - Recalculate and store new hash
   - User data preserved
7. Continue loading user patterns
```

**Hash Calculation:**
- Query each builtin table in deterministic order
- Convert all rows to strings
- SHA-256 hash the concatenated result
- Store as hex string in `integrity_metadata`

### 3. Auto-Recovery

When tampering is detected:

```bash
⚠️  SECURITY WARNING: Tampering detected in policy.db!
Expected hash: abc123...
Actual hash:   def456...
Auto-repairing: clearing builtin tables and reseeding from embedded configs.
User approval patterns will be preserved.

✓ Policy database repaired successfully
```

**What Gets Replaced:**
- All builtin policy tables

**What Gets Preserved:**
- Your saved approval patterns
- MCP approval patterns  
- Audit logs
- All session data

## SQL Triggers (Defense in Depth)

In addition to integrity verification, builtin tables are protected by SQL triggers:

```sql
CREATE TRIGGER protect_modes_update
BEFORE UPDATE ON agent_modes
BEGIN
    SELECT RAISE(ABORT, 'Cannot modify builtin modes');
END;
```

These triggers:
- ✅ Prevent accidental modifications via SQL
- ✅ Protect against application bugs
- ❌ Can be bypassed by dropping the trigger first
- ❌ Don't prevent direct file modification

Triggers are a "safety net" not "security" - they catch honest mistakes but not determined tampering.

## Manual Verification

### Check Integrity

```bash
$ tark policy verify

Policy Database Integrity Check
Location: /path/to/project/.tark/policy.db

✓ Integrity check passed
Hash: abc123def456789...

No tampering detected. Builtin policy tables are intact.
```

### Force Repair

```bash
$ tark policy verify --fix

Forcing reseed from embedded configs...
✓ Policy database repaired successfully
New hash: 789def456abc123

User approval patterns were preserved.
```

## Recovery Procedures

### If Tampering Detected

**Option 1: Automatic (Recommended)**

Tark automatically repairs on startup. Just restart:

```bash
$ tark tui
```

**Option 2: Manual Repair**

```bash
$ tark policy verify --fix
```

**Option 3: Nuclear Option**

Delete and recreate:

```bash
$ rm .tark/policy.db
$ tark tui  # Will recreate with fresh builtin policy
```

⚠️ **Warning:** This deletes ALL approval patterns, not just builtin policy!

## Best Practices

### For Users

1. **Don't modify policy.db directly** - Use config files instead
2. **Review warnings** - If you see tampering warnings, investigate
3. **Backup important patterns** - Export to config files
4. **Use file permissions** - Restrict write access to `.tark/` if needed

### For Developers

1. **Never modify builtin tables in code** - Triggers will block it
2. **Test integrity verification** - Ensure tests cover tampering scenarios
3. **Preserve user data** - Auto-repair must never delete user patterns
4. **Log clearly** - Security warnings should be obvious and actionable

## File Permissions (Optional)

For higher security environments, you can restrict database permissions:

### Unix/Linux

```bash
# Make policy.db read-only after first run
$ chmod 444 .tark/policy.db

# Tark will detect and handle this gracefully
```

⚠️ **Limitation:** This also prevents user approval patterns from being saved!

## Limitations

### What This System CAN'T Prevent

1. **Modification before tark runs** - Attacker can modify DB, then tark auto-repairs on next run
2. **Persistent attacks** - Attacker can modify DB after each auto-repair
3. **Memory attacks** - Once loaded, policy data is in memory and can be modified
4. **Binary replacement** - Attacker can replace tark binary entirely

### Why These Limitations Exist

The goal is **reasonable protection for development workflows**, not **high-security isolation**. If you need stronger guarantees:

- Run tark in a container with read-only root filesystem
- Use AppArmor/SELinux profiles to restrict file access
- Mount `.tark/` from a trusted source
- Implement cryptographic signing (not currently supported)

## Technical Details

### Hash Algorithm

- **Algorithm:** SHA-256
- **Input:** Deterministic concatenation of all builtin table rows
- **Output:** 64-character hex string
- **Storage:** `integrity_metadata` table

### Performance

- **Hash calculation:** ~10-50ms (depends on # of tables)
- **Verification:** ~10-50ms (same as calculation)
- **Auto-repair:** ~100-500ms (includes clear + reseed)

### Database Schema

```sql
CREATE TABLE integrity_metadata (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
```

Currently stores only one key:
- `builtin_hash` - SHA-256 hash of all builtin tables

## FAQ

**Q: Why not just make the database read-only?**  
A: User approval patterns need to be writable. Separate databases would add complexity.

**Q: Can't an attacker just update the hash after modifying tables?**  
A: Yes! This is detection, not prevention. File system permissions are the real security boundary.

**Q: What if I have custom policy in config files?**  
A: Config files sync to database on startup but aren't included in the hash. Only builtin policy is verified.

**Q: Does this protect against SQL injection?**  
A: No. SQL injection is a separate concern. All database queries use parameterized statements.

**Q: Can I disable integrity checking?**  
A: Not currently. It's always enabled and auto-repairs. You can ignore warnings if you want.

**Q: What about the .tark directory itself?**  
A: File system permissions control access. OS security model handles that.

## Future Enhancements

Potential improvements for future versions:

1. **Cryptographic Signing** - Sign builtin policy with embedded public key
2. **Separate Databases** - Split into `policy_builtin.db` (readonly) + `policy_user.db` (writable)
3. **Config File Hashing** - Include user config files in integrity verification
4. **Audit Trail** - Log all integrity verification events
5. **Remote Attestation** - Verify policy against known-good remote source

## References

- Policy Engine Documentation: [POLICY_ENGINE_COMPLETE.md](../POLICY_ENGINE_COMPLETE.md)
- Source Code: `src/policy/integrity.rs`
- Tests: `src/policy/integrity.rs#tests`
