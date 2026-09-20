//! Per-start bearer credential for Tark-owned loopback endpoints (R6 S14).
//!
//! A loopback bridge must reject unauthenticated non-loopback-style access by
//! default; even on loopback, a per-start bearer credential ensures only the
//! Tark process tree that started the bridge can use it:
//!
//! - The token is 32 bytes from the OS RNG, stored hex-encoded in a file
//!   created atomically (`O_CREAT | O_EXCL | O_NOFOLLOW`) with owner-only
//!   (`0600`) permissions. Symlink finals are refused fail-closed.
//! - The file lives in the runtime dir (`$XDG_RUNTIME_DIR/tark`, else a
//!   per-UID temp dir) under a unique name containing pid + timestamp +
//!   randomness, so every start rotates to a fresh credential.
//! - Verification uses a constant-time byte comparison (no data-dependent
//!   early exit over the token bytes).
//! - Dropping the credential removes the file (invalidated at shutdown);
//!   [`cleanup_stale`] sweeps files whose owner pid no longer exists.
//! - The token value never appears in logs, traces, or [`Debug`] output.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Env var naming the file that holds the current loopback bearer token.
///
/// A Tark-owned bridge sets this for the processes it spawns; clients read
/// the token from the file at request time so rotation needs no restart.
pub const LOOPBACK_TOKEN_FILE_ENV: &str = "TARK_LOOPBACK_TOKEN_FILE";

/// Token size in bytes (64 hex chars on disk).
const TOKEN_BYTES: usize = 32;

/// Filename prefix for loopback credential files.
const FILE_PREFIX: &str = "tark-loopback-";

/// Per-start loopback bearer credential.
///
/// Removing the value (dropping it) removes the token file best-effort.
pub struct LoopbackCredential {
    token: [u8; TOKEN_BYTES],
    path: PathBuf,
}

impl LoopbackCredential {
    /// Issue a fresh credential in the default runtime dir.
    pub fn issue() -> Result<Self> {
        let dir = default_runtime_dir()?;
        Self::issue_in(&dir)
    }

    /// Issue a fresh credential in `dir` (created with owner-only perms).
    pub fn issue_in(dir: &Path) -> Result<Self> {
        ensure_owner_only_dir(dir)?;
        cleanup_stale(dir);

        let mut token = [0u8; TOKEN_BYTES];
        {
            use rand::RngCore;
            rand::rngs::OsRng.fill_bytes(&mut token);
        }
        let path = unique_token_path(dir);
        write_owner_only_file(&path, hex::encode(token).as_bytes())
            .with_context(|| format!("Failed to create credential file {}", path.display()))?;
        Ok(Self { token, path })
    }

    /// Path of the token file (safe to share; the file is owner-only).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Verify a presented bearer token with a constant-time comparison.
    ///
    /// Non-hex or wrong-length input is rejected without touching the
    /// comparison loop. Length is not secret (tokens are always 32 bytes).
    pub fn verify(&self, presented: &str) -> bool {
        let Ok(bytes) = hex::decode(presented.trim()) else {
            return false;
        };
        constant_time_eq(&bytes, &self.token)
    }
}

impl Drop for LoopbackCredential {
    fn drop(&mut self) {
        // Best-effort: shutdown must never fail because of cleanup.
        let _ = std::fs::remove_file(&self.path);
    }
}

impl std::fmt::Debug for LoopbackCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The token value must never appear in diagnostics (R3/R6).
        f.debug_struct("LoopbackCredential")
            .field("path", &self.path)
            .field("token", &"[REDACTED]")
            .finish()
    }
}

/// Constant-time equality over secret bytes (no data-dependent early exit).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Default runtime dir: `$XDG_RUNTIME_DIR/tark`, else a per-UID temp dir.
fn default_runtime_dir() -> Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
        let candidate = PathBuf::from(&xdg);
        if candidate.is_absolute() {
            return Ok(candidate.join("tark"));
        }
    }
    #[cfg(unix)]
    // SAFETY: getuid() is async-signal-safe and infallible.
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = 0u32;
    Ok(std::env::temp_dir().join(format!("tark-loopback-{uid}")))
}

/// Create `dir` (and parents) with owner-only permissions.
fn ensure_owner_only_dir(dir: &Path) -> Result<()> {
    std::fs::create_dir_all(dir)
        .with_context(|| format!("Failed to create runtime dir {}", dir.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("Failed to restrict runtime dir {}", dir.display()))?;
    }
    Ok(())
}

/// Unique token file name: pid + nanos + randomness (unpredictable, per-start).
fn unique_token_path(dir: &Path) -> PathBuf {
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let mut rand_suffix = [0u8; 8];
    {
        use rand::RngCore;
        // Best-effort uniqueness salt; creation still uses O_EXCL.
        rand::rngs::OsRng.fill_bytes(&mut rand_suffix);
    }
    dir.join(format!(
        "{FILE_PREFIX}{pid}-{nanos}-{}.token",
        hex::encode(rand_suffix)
    ))
}

/// Write `contents` to `path` with symlink-safe, owner-only semantics.
///
/// Uses `O_CREAT | O_EXCL | O_NOFOLLOW` so creation is atomic: an existing
/// file (or symlink) at `path` fails instead of being followed or clobbered.
/// Post-creation metadata is verified (regular file, `0600` on unix).
pub(crate) fn write_owner_only_file(path: &Path, contents: &[u8]) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true).mode(0o600);
        opts.custom_flags(libc::O_NOFOLLOW);
        let mut file = opts
            .open(path)
            .with_context(|| format!("Failed to create {}", path.display()))?;
        file.write_all(contents)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        file.sync_all()
            .with_context(|| format!("Failed to sync {}", path.display()))?;
        drop(file);

        let meta = std::fs::symlink_metadata(path)
            .with_context(|| format!("Failed to stat {}", path.display()))?;
        if !meta.file_type().is_file() || meta.file_type().is_symlink() {
            let _ = std::fs::remove_file(path);
            anyhow::bail!("Refusing credential file with non-regular type");
        }
        if meta.permissions().mode() & 0o777 != 0o600 {
            let _ = std::fs::remove_file(path);
            anyhow::bail!("Refusing credential file without owner-only permissions");
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        // Best effort without O_NOFOLLOW: refuse symlink finals, then write
        // with restricted permissions where the platform supports them.
        let meta = std::fs::symlink_metadata(path);
        if let Ok(meta) = meta {
            if meta.file_type().is_symlink() {
                anyhow::bail!("Refusing credential file through symlink");
            }
            // O_EXCL equivalent: never clobber an existing credential.
            anyhow::bail!("Refusing to overwrite existing credential file");
        }
        std::fs::write(path, contents)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }
}

/// Read a bearer token from a credential file (client side, at request time).
///
/// Refuses symlink finals and non-owner-only permissions (unix) fail-closed;
/// the token value itself is never logged by callers.
pub fn read_token_file(path: &Path) -> Result<String> {
    let meta = std::fs::symlink_metadata(path)
        .with_context(|| format!("Failed to stat token file {}", path.display()))?;
    if !meta.file_type().is_file() || meta.file_type().is_symlink() {
        anyhow::bail!("Refusing token file with non-regular type");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            anyhow::bail!("Refusing token file without owner-only permissions");
        }
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read token file {}", path.display()))?;
    let token = raw.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("Token file is empty");
    }
    Ok(token)
}

/// Remove credential files whose owner pid no longer exists (best-effort).
///
/// Only touches `{FILE_PREFIX}<pid>-*.token` names; anything else is left
/// alone. A live pid is never removed.
pub fn cleanup_stale(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(rest) = name
            .strip_prefix(FILE_PREFIX)
            .and_then(|s| s.strip_suffix(".token"))
        else {
            continue;
        };
        let Some(pid_str) = rest.split('-').next() else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        if !pid_alive(pid) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// True when a process with `pid` exists (unix `kill(pid, 0)` semantics).
fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    // SAFETY: kill() with sig 0 performs only error checking, no signal.
    let alive = unsafe {
        let r = libc::kill(pid as libc::pid_t, 0);
        if r == 0 {
            true
        } else {
            // EPERM means the process exists but belongs to another user.
            std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
        }
    };
    #[cfg(not(unix))]
    let alive = true;
    alive
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    #[test]
    fn issue_verify_round_trip() {
        let dir = test_dir();
        let cred = LoopbackCredential::issue_in(dir.path()).expect("issue");
        let presented = hex::encode(cred.token);
        assert!(cred.verify(&presented));
        assert!(cred.verify(&format!("  {presented}\n")));
        assert!(!cred.verify(""));
        assert!(!cred.verify("not-hex!!"));
        // Flip one hex digit: still valid hex, must not verify.
        let mut tampered = presented.clone();
        let last = tampered.pop().expect("non-empty");
        tampered.push(if last == '0' { '1' } else { '0' });
        assert_ne!(tampered, presented);
        assert!(!cred.verify(&tampered));
        // Wrong length decodes but must not verify.
        assert!(!cred.verify(&hex::encode([0u8; 16])));
    }

    #[test]
    fn debug_output_redacts_token() {
        let dir = test_dir();
        let cred = LoopbackCredential::issue_in(dir.path()).expect("issue");
        let rendered = format!("{:?}", cred);
        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains(&hex::encode(cred.token)));
    }

    #[test]
    fn token_file_is_owner_only_regular_file() {
        let dir = test_dir();
        let cred = LoopbackCredential::issue_in(dir.path()).expect("issue");
        let meta = std::fs::symlink_metadata(cred.path()).expect("stat");
        assert!(meta.file_type().is_file());
        assert!(!meta.file_type().is_symlink());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(meta.permissions().mode() & 0o777, 0o600);
        }
        let stored = std::fs::read_to_string(cred.path()).expect("read");
        assert_eq!(stored, hex::encode(cred.token));
    }

    #[test]
    fn symlink_final_is_refused() {
        let dir = test_dir();
        let target = dir.path().join("real-token");
        std::fs::write(&target, "secret").expect("write target");
        let link = dir.path().join("link-token");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&target, &link).expect("symlink");
        #[cfg(not(unix))]
        std::fs::write(&link, "secret").expect("write link stand-in");
        assert!(write_owner_only_file(&link, b"new-secret").is_err());
        // The symlink target is untouched.
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "secret");
    }

    #[test]
    fn rotation_issues_unique_credentials_and_drop_cleans_up() {
        let dir = test_dir();
        let first = LoopbackCredential::issue_in(dir.path()).expect("first");
        let second = LoopbackCredential::issue_in(dir.path()).expect("second");
        assert_ne!(first.path(), second.path());
        assert_ne!(first.token, second.token);
        let first_path = first.path().to_path_buf();
        let second_path = second.path().to_path_buf();
        drop(first);
        assert!(!first_path.exists());
        assert!(second_path.exists());
        drop(second);
        assert!(!second_path.exists());
    }

    #[test]
    fn cleanup_stale_removes_dead_pid_files_and_keeps_live() {
        let dir = test_dir();
        // A guaranteed-dead pid: spawn and reap a short-lived child. The
        // test binary itself with `--list` is the only executable known to
        // exist on every platform (`/bin/true` is absent on macOS/Windows;
        // u32::MAX as pid_t is -1, i.e. "all processes").
        let exe = std::env::current_exe().expect("current exe");
        let mut child = std::process::Command::new(exe)
            .arg("--list")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn list probe");
        let dead_pid = child.id();
        child.wait().expect("reap list probe");
        let dead_path = dir
            .path()
            .join(format!("{FILE_PREFIX}{dead_pid}-1-deadbeef.token"));
        std::fs::write(&dead_path, "stale").expect("write stale");
        let live_pid = std::process::id();
        let live_path = dir
            .path()
            .join(format!("{FILE_PREFIX}{live_pid}-1-livebeef.token",));
        std::fs::write(&live_path, "live").expect("write live");
        let unrelated = dir.path().join("notes.txt");
        std::fs::write(&unrelated, "notes").expect("write notes");
        cleanup_stale(dir.path());
        // Non-unix pid_alive() conservatively reports alive, so removal of
        // dead-pid files is only asserted where kill(pid, 0) works.
        #[cfg(unix)]
        assert!(!dead_path.exists());
        #[cfg(not(unix))]
        let _ = dead_path;
        assert!(live_path.exists());
        assert!(unrelated.exists());
    }

    #[test]
    fn read_token_file_round_trip_and_refusals() {
        let dir = test_dir();
        let cred = LoopbackCredential::issue_in(dir.path()).expect("issue");
        let token = read_token_file(cred.path()).expect("read back");
        assert_eq!(token, hex::encode(cred.token));

        assert!(read_token_file(dir.path().join("missing.token").as_path()).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let lax = dir.path().join("lax.token");
            std::fs::write(&lax, "s3cret").expect("write lax");
            std::fs::set_permissions(&lax, std::fs::Permissions::from_mode(0o644)).expect("chmod");
            assert!(read_token_file(&lax).is_err());
        }
    }
}
