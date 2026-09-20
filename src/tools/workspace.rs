//! Race-resistant workspace confinement (R1, S1–S3).
//!
//! All native filesystem tools must resolve user-supplied paths through
//! [`WorkspaceCap`] instead of `working_dir.join()` + lexical `starts_with`.
//!
//! Guarantees (fail-closed):
//! - Relative paths resolve against the primary root only.
//! - Absolute paths are allowed **only** when they fall inside a granted root.
//! - `..` traversal that escapes every granted root is denied.
//! - Symlinks are resolved via `canonicalize` on the longest existing prefix;
//!   a symlink pointing outside the workspace denies instead of escaping.
//! - Non-existent paths are validated via their closest existing ancestor;
//!   there is no `canonicalize().unwrap_or(lexical)` escape hatch.
//! - Denial messages identify the missing scope without exposing unrelated
//!   filesystem content (S2).
//!
//! TOCTOU note: [`WorkspaceCap::resolve`] closes the authorization gap
//! (lexical + symlink escape). The `open_*` / `remove_file` /
//! `ensure_parent_dir` helpers below go further: on Linux they perform the
//! open itself with `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | ...)`,
//! so the kernel enforces containment atomically and a symlink swap between
//! authorization and use cannot escape. Prefer these helpers over
//! `resolve()` + `std::fs` wherever a file is opened, created, or deleted.

use std::fmt;
use std::path::{Component, Path, PathBuf};

/// Why a path was denied. Carries the user input for diagnostics but never
/// echoes unrelated filesystem content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDenied {
    /// The original user-supplied path string.
    pub input: String,
    /// Short machine-readable reason: `empty`, `nul`, `escape`, `symlink-escape`, `unresolvable`.
    pub reason: &'static str,
}

impl fmt::Display for WorkspaceDenied {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Access denied: '{}' is outside the granted workspace scope ({})",
            self.input, self.reason
        )
    }
}

impl std::error::Error for WorkspaceDenied {}

/// Failure of a confined filesystem operation.
///
/// Authorization denials and IO failures are distinct variants so callers
/// can report "file not found" vs "access denied" without conflating
/// them (denial messages never leak unrelated filesystem content).
#[derive(Debug)]
pub enum CapIoError {
    /// Path authorization failed (fail-closed, no operation performed).
    Denied(WorkspaceDenied),
    /// Authorized, but the filesystem operation itself failed.
    Io(std::io::Error),
}

impl fmt::Display for CapIoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Denied(denied) => write!(f, "{}", denied),
            Self::Io(e) => write!(f, "Filesystem error: {}", e),
        }
    }
}

impl std::error::Error for CapIoError {}

impl From<WorkspaceDenied> for CapIoError {
    fn from(denied: WorkspaceDenied) -> Self {
        Self::Denied(denied)
    }
}

/// Capability holding the explicitly granted workspace roots.
///
/// Today constructed from a single `working_dir`; multi-root grant (explicit
/// permission interaction per R1) is supported via [`WorkspaceCap::with_roots`].
#[derive(Debug, Clone)]
pub struct WorkspaceCap {
    roots: Vec<PathBuf>,
    canonical_roots: Vec<PathBuf>,
}

impl WorkspaceCap {
    /// Create a capability for a single workspace root.
    pub fn new(primary: PathBuf) -> Self {
        Self::with_roots(vec![primary])
    }

    /// Create a capability for several explicitly granted roots.
    ///
    /// The first root is the primary root used for relative resolution.
    pub fn with_roots(roots: Vec<PathBuf>) -> Self {
        assert!(
            !roots.is_empty(),
            "WorkspaceCap requires at least one granted root"
        );
        let canonical_roots = roots.iter().map(|r| Self::canonicalize_root(r)).collect();
        Self {
            roots,
            canonical_roots,
        }
    }

    /// Primary root (relative paths resolve against this).
    pub fn primary(&self) -> &Path {
        &self.roots[0]
    }

    /// All granted roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Resolve user input to an absolute path inside the workspace.
    ///
    /// Fails closed with [`WorkspaceDenied`] on traversal, absolute escape,
    /// symlink escape, or unresolvable prefixes.
    pub fn resolve(&self, input: &str) -> Result<PathBuf, WorkspaceDenied> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return Err(WorkspaceDenied {
                input: input.to_string(),
                reason: "empty",
            });
        }
        if trimmed.contains('\0') {
            return Err(WorkspaceDenied {
                input: input.to_string(),
                reason: "nul",
            });
        }

        // Join relative inputs against the primary root; keep absolute inputs
        // as-is so the containment check below can deny escapes.
        let joined = if Path::new(trimmed).is_absolute() {
            PathBuf::from(trimmed)
        } else {
            self.roots[0].join(trimmed)
        };
        let normalized = Self::lexical_normalize(&joined);

        // Fast path: the normalized path already exists — canonicalize it
        // (resolves symlinks) and enforce containment. Symlink detection
        // runs on the pre-canonical path: the canonical target of an escape
        // is an ordinary outside path, while the user-supplied path holds
        // the symlink that performed the escape.
        if normalized.exists() {
            let canonical = std::fs::canonicalize(&normalized).map_err(|_| WorkspaceDenied {
                input: input.to_string(),
                reason: "unresolvable",
            })?;
            return self.enforce_containment(&canonical, &normalized, input);
        }

        // Creation path: find the closest existing ancestor, canonicalize it,
        // then re-append the non-existent remainder. The remainder is already
        // lexically normalized so it cannot contain `..` that escapes.
        let mut ancestor = normalized.as_path();
        let mut remainder: Vec<std::ffi::OsString> = Vec::new();
        loop {
            if ancestor.exists() {
                break;
            }
            match ancestor.file_name() {
                Some(name) => remainder.push(name.to_os_string()),
                None => {
                    return Err(WorkspaceDenied {
                        input: input.to_string(),
                        reason: "unresolvable",
                    });
                }
            }
            match ancestor.parent() {
                Some(parent) => ancestor = parent,
                None => {
                    return Err(WorkspaceDenied {
                        input: input.to_string(),
                        reason: "unresolvable",
                    });
                }
            }
        }
        // A remainder component must never be `..` / `.` after normalization;
        // if it is, the input attempted to traverse beyond the ancestor.
        for component in remainder.iter() {
            if component == ".." || component == "." {
                return Err(WorkspaceDenied {
                    input: input.to_string(),
                    reason: "escape",
                });
            }
        }
        let canonical_ancestor = std::fs::canonicalize(ancestor).map_err(|_| WorkspaceDenied {
            input: input.to_string(),
            reason: "unresolvable",
        })?;
        if !self.is_within_canonical(&canonical_ancestor) {
            return Err(WorkspaceDenied {
                input: input.to_string(),
                reason: if Self::is_symlink_escape(ancestor) {
                    "symlink-escape"
                } else {
                    "escape"
                },
            });
        }
        let mut resolved = canonical_ancestor;
        for component in remainder.iter().rev() {
            resolved.push(component);
        }
        // `resolved` starts from a verified canonical ancestor plus inert file
        // name components, so a lexical containment check is sufficient here.
        if self.is_within_canonical(&resolved) {
            return Ok(resolved);
        }
        Err(WorkspaceDenied {
            input: input.to_string(),
            reason: "escape",
        })
    }

    /// Workspace-relative display for a resolved path (S1 reporting).
    pub fn relative(&self, path: &Path) -> String {
        for root in self.canonical_roots.iter().chain(self.roots.iter()) {
            if let Ok(rel) = path.strip_prefix(root) {
                if rel.as_os_str().is_empty() {
                    return ".".to_string();
                }
                return rel.display().to_string();
            }
        }
        path.display().to_string()
    }

    fn enforce_containment(
        &self,
        canonical: &Path,
        pre_canonical: &Path,
        input: &str,
    ) -> Result<PathBuf, WorkspaceDenied> {
        if self.is_within_canonical(canonical) {
            return Ok(canonical.to_path_buf());
        }
        Err(WorkspaceDenied {
            input: input.to_string(),
            reason: if Self::is_symlink_escape(pre_canonical) {
                "symlink-escape"
            } else {
                "escape"
            },
        })
    }

    fn is_within_canonical(&self, path: &Path) -> bool {
        self.canonical_roots
            .iter()
            .any(|root| path.starts_with(root))
    }

    /// Canonical root containing `path`, if any.
    fn root_for(&self, path: &Path) -> Option<&PathBuf> {
        self.canonical_roots
            .iter()
            .find(|root| path.starts_with(root))
    }

    /// Open a workspace file for reading with race-resistant confinement.
    ///
    /// On Linux the open itself goes through `openat2` with
    /// `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS | RESOLVE_NO_XDEV`, so the
    /// kernel confines the result atomically: a symlink swap between
    /// [`WorkspaceCap::resolve`] and this open cannot escape the grant.
    /// Other platforms fall back to `resolve()` + `O_NOFOLLOW` on the final
    /// component (symlink finals denied fail-closed there).
    pub fn open_read(&self, input: &str) -> Result<std::fs::File, CapIoError> {
        let resolved = self.resolve(input)?;
        open_file_confined(self.root_for(&resolved), &resolved, OpenMode::Read, input)
    }

    /// Open a workspace file for writing (create if missing, truncate).
    pub fn open_write(&self, input: &str) -> Result<std::fs::File, CapIoError> {
        let resolved = self.resolve(input)?;
        // Ensure the parent chain exists first (verified below).
        if let Some(parent) = resolved.parent() {
            self.ensure_parent_dir_inner(parent, input)?;
        }
        open_file_confined(self.root_for(&resolved), &resolved, OpenMode::Write, input)
    }

    /// Delete a workspace file with race-resistant confinement.
    pub fn remove_file(&self, input: &str) -> Result<(), CapIoError> {
        let resolved = self.resolve(input)?;
        remove_file_confined(self.root_for(&resolved), &resolved, input)
    }

    /// Ensure the parent directory chain of a resolved path exists,
    /// re-verifying containment after creation (fail-closed on swap).
    pub fn ensure_parent_dir(&self, input: &str) -> Result<PathBuf, CapIoError> {
        let resolved = self.resolve(input)?;
        if let Some(parent) = resolved.parent() {
            self.ensure_parent_dir_inner(parent, input)?;
        }
        Ok(resolved)
    }

    fn ensure_parent_dir_inner(&self, parent: &Path, input: &str) -> Result<(), CapIoError> {
        std::fs::create_dir_all(parent).map_err(CapIoError::Io)?;
        // Re-verify: a swap between resolve and create must not escape.
        let canonical = std::fs::canonicalize(parent).map_err(CapIoError::Io)?;
        if self.is_within_canonical(&canonical) {
            Ok(())
        } else {
            Err(CapIoError::Denied(WorkspaceDenied {
                input: input.to_string(),
                reason: "symlink-escape",
            }))
        }
    }

    fn is_symlink_escape(path: &Path) -> bool {
        path.is_symlink() || path.ancestors().any(|ancestor| ancestor.is_symlink())
    }

    fn canonicalize_root(root: &Path) -> PathBuf {
        // Roots are granted explicitly; if the root does not exist yet we keep
        // the lexically normalized form so enforcement still applies.
        std::fs::canonicalize(root).unwrap_or_else(|_| Self::lexical_normalize(root))
    }

    fn lexical_normalize(path: &Path) -> PathBuf {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
                Component::RootDir => normalized.push(Component::RootDir.as_os_str()),
                Component::CurDir => {}
                Component::ParentDir => {
                    normalized.pop();
                }
                Component::Normal(part) => normalized.push(part),
            }
        }
        if normalized.as_os_str().is_empty() {
            PathBuf::from(".")
        } else {
            normalized
        }
    }
}

/// Open mode for [`WorkspaceCap::open_read`] / [`WorkspaceCap::open_write`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenMode {
    Read,
    Write,
}

/// Open `resolved` (already authorized by `resolve()`) with kernel-enforced
/// confinement where the platform allows it.
///
/// `root` is the canonical grant root prefixing `resolved`, or `None` when
/// resolution succeeded lexically (unreachable in practice: `resolve()` only
/// succeeds inside a canonical root — treated as denial).
fn open_file_confined(
    root: Option<&PathBuf>,
    resolved: &Path,
    mode: OpenMode,
    input: &str,
) -> Result<std::fs::File, CapIoError> {
    let root = root.ok_or_else(|| {
        CapIoError::Denied(WorkspaceDenied {
            input: input.to_string(),
            reason: "escape",
        })
    })?;

    #[cfg(target_os = "linux")]
    {
        open_file_openat2(root, resolved, mode).map_err(CapIoError::Io)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        open_file_nofollow(resolved, mode).map_err(CapIoError::Io)
    }
}

/// Delete `resolved` (already authorized) with kernel-enforced confinement
/// where the platform allows it.
fn remove_file_confined(
    root: Option<&PathBuf>,
    resolved: &Path,
    input: &str,
) -> Result<(), CapIoError> {
    let root = root.ok_or_else(|| {
        CapIoError::Denied(WorkspaceDenied {
            input: input.to_string(),
            reason: "escape",
        })
    })?;

    #[cfg(target_os = "linux")]
    {
        remove_file_unlinkat(root, resolved).map_err(CapIoError::Io)
    }

    #[cfg(not(target_os = "linux"))]
    {
        let _ = root;
        // Best effort without openat2: refuse symlink finals, then unlink.
        if resolved.is_symlink() {
            return Err(CapIoError::Denied(WorkspaceDenied {
                input: input.to_string(),
                reason: "symlink-escape",
            }));
        }
        std::fs::remove_file(resolved).map_err(CapIoError::Io)
    }
}

#[cfg(target_os = "linux")]
fn open_file_openat2(
    root: &Path,
    resolved: &Path,
    mode: OpenMode,
) -> std::io::Result<std::fs::File> {
    use rustix::fd::AsFd;
    use rustix::fs::{open, openat2, Mode, OFlags, ResolveFlags};

    let rel = resolved
        .strip_prefix(root)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    // Open the grant root itself; every subsequent lookup is relative to it.
    let root_fd = open(
        root,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    // Atomic in-kernel confinement: symlinks resolve but the result must
    // stay beneath the root; procfs magic links and cross-device escapes
    // are refused outright.
    let resolve = ResolveFlags::BENEATH | ResolveFlags::NO_MAGICLINKS | ResolveFlags::NO_XDEV;
    let (oflags, file_mode) = match mode {
        OpenMode::Read => (OFlags::RDONLY | OFlags::CLOEXEC, Mode::empty()),
        OpenMode::Write => (
            OFlags::WRONLY | OFlags::CREATE | OFlags::TRUNC | OFlags::CLOEXEC,
            Mode::from_bits_truncate(0o666),
        ),
    };
    let fd = openat2(root_fd.as_fd(), rel, oflags, file_mode, resolve)?;
    Ok(std::fs::File::from(fd))
}

#[cfg(target_os = "linux")]
fn remove_file_unlinkat(root: &Path, resolved: &Path) -> std::io::Result<()> {
    use rustix::fd::AsFd;
    use rustix::fs::{open, unlinkat, AtFlags, Mode, OFlags};

    let rel = resolved
        .strip_prefix(root)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::PermissionDenied, e))?;
    let Some(final_name) = rel.file_name() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "refusing to unlink a directory root",
        ));
    };
    // Open the verified parent chain relative to the grant root, then unlink
    // only the final component: a swapped symlink can never redirect this.
    let root_fd = open(
        root,
        OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )?;
    let parent_rel = rel.parent().unwrap_or_else(|| std::path::Path::new(""));
    let parent_fd = if parent_rel.as_os_str().is_empty() {
        root_fd
    } else {
        use rustix::fs::{openat2, ResolveFlags};
        let resolve = ResolveFlags::BENEATH | ResolveFlags::NO_MAGICLINKS | ResolveFlags::NO_XDEV;
        openat2(
            root_fd.as_fd(),
            parent_rel,
            OFlags::PATH | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            resolve,
        )?
    };
    unlinkat(parent_fd.as_fd(), final_name, AtFlags::empty())?;
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn open_file_nofollow(resolved: &Path, mode: OpenMode) -> std::io::Result<std::fs::File> {
    let mut options = std::fs::OpenOptions::new();
    match mode {
        OpenMode::Read => {
            options.read(true);
        }
        OpenMode::Write => {
            options.write(true).create(true).truncate(true);
        }
    }
    // Refuse symlink finals fail-closed (best effort without openat2).
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    options.open(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;

    fn test_workspace() -> (tempfile::TempDir, WorkspaceCap) {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("allowed.txt"), "hello").expect("write");
        fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        let cap = WorkspaceCap::new(dir.path().to_path_buf());
        (dir, cap)
    }

    #[test]
    fn s1_allowed_workspace_file_succeeds() {
        let (_dir, cap) = test_workspace();
        let resolved = cap.resolve("allowed.txt").expect("allowed file");
        assert!(resolved.ends_with("allowed.txt"));
        assert_eq!(cap.relative(&resolved), "allowed.txt");
    }

    #[test]
    fn s2_traversal_and_absolute_escape_denied() {
        let (_dir, cap) = test_workspace();
        for input in [
            "../escape.txt",
            "sub/../../escape.txt",
            "/etc/passwd",
            "/tmp/outside-workspace.txt",
        ] {
            let err = cap.resolve(input).expect_err("must deny escape");
            assert!(
                !err.to_string().contains("/etc/passwd content"),
                "denial must not leak fs content"
            );
            assert!(err.to_string().contains("outside the granted workspace"));
        }
    }

    #[test]
    fn s3_symlink_escape_denied() {
        let (dir, cap) = test_workspace();
        let outside = tempfile::tempdir().expect("outside tempdir");
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "secret").expect("write secret");
        symlink(&secret, dir.path().join("link.txt")).expect("symlink");
        let err = cap
            .resolve("link.txt")
            .expect_err("symlink escape must deny");
        assert_eq!(err.reason, "symlink-escape");
    }

    #[test]
    fn creation_inside_workspace_allowed_escape_denied() {
        let (_dir, cap) = test_workspace();
        assert!(cap.resolve("sub/new-file.txt").is_ok());
        assert!(cap.resolve("../new-file.txt").is_err());
    }

    #[test]
    fn empty_and_nul_rejected() {
        let (_dir, cap) = test_workspace();
        assert!(cap.resolve("").is_err());
        assert!(cap.resolve("   ").is_err());
        assert!(cap.resolve("a\0b").is_err());
    }

    #[test]
    fn confined_open_read_round_trip() {
        let (_dir, cap) = test_workspace();
        let mut file = cap.open_read("allowed.txt").expect("open for read");
        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content).expect("read");
        assert_eq!(content, "hello");
    }

    #[test]
    fn confined_open_denies_escape_and_missing_is_io() {
        let (_dir, cap) = test_workspace();
        match cap.open_read("/etc/passwd") {
            Err(CapIoError::Denied(denied)) => assert_eq!(denied.reason, "escape"),
            other => panic!("expected denial, got {:?}", other.is_ok()),
        }
        match cap.open_read("no-such-file.txt") {
            Err(CapIoError::Io(_)) => {}
            other => panic!("expected IO error, got {:?}", other.is_ok()),
        }
    }

    #[test]
    fn confined_open_refuses_symlink_final() {
        let (dir, cap) = test_workspace();
        let outside = tempfile::tempdir().expect("outside tempdir");
        let secret = outside.path().join("secret.txt");
        fs::write(&secret, "secret").expect("write secret");
        symlink(&secret, dir.path().join("evil.txt")).expect("symlink");
        // Authorization denies first (fail-closed either way).
        assert!(cap.open_read("evil.txt").is_err());
    }

    #[test]
    fn confined_write_creates_and_removes() {
        let (_dir, cap) = test_workspace();
        {
            let mut file = cap.open_write("sub/created.txt").expect("open for write");
            std::io::Write::write_all(&mut file, b"data").expect("write");
        }
        let mut file = cap.open_read("sub/created.txt").expect("reopen");
        let mut content = String::new();
        std::io::Read::read_to_string(&mut file, &mut content).expect("read");
        assert_eq!(content, "data");
        cap.remove_file("sub/created.txt").expect("remove");
        assert!(cap.open_read("sub/created.txt").is_err());
    }

    #[test]
    fn confined_remove_denies_escape() {
        let (_dir, cap) = test_workspace();
        assert!(cap.remove_file("../escape.txt").is_err());
        assert!(cap.remove_file("/etc/passwd").is_err());
    }

    #[test]
    fn ensure_parent_dir_rejects_escape() {
        let (_dir, cap) = test_workspace();
        assert!(cap.ensure_parent_dir("sub/ok.txt").is_ok());
        assert!(cap.ensure_parent_dir("../evil.txt").is_err());
    }
}
