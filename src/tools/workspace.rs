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
//! TOCTOU note: this closes the authorization gap (lexical + symlink escape).
//! Full open-then-`fstat`-recheck (`openat2(RESOLVE_BENEATH)`) is tracked as a
//! hardening follow-up; callers must use the resolved path immediately and
//! must not re-join user input afterwards.

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
        // (resolves symlinks) and enforce containment.
        if normalized.exists() {
            let canonical = std::fs::canonicalize(&normalized).map_err(|_| WorkspaceDenied {
                input: input.to_string(),
                reason: "unresolvable",
            })?;
            return self.enforce_containment(&canonical, input);
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
        self.enforce_containment_lexical(&resolved, input)
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
        input: &str,
    ) -> Result<PathBuf, WorkspaceDenied> {
        if self.is_within_canonical(canonical) {
            return Ok(canonical.to_path_buf());
        }
        Err(WorkspaceDenied {
            input: input.to_string(),
            reason: if Self::is_symlink_escape(canonical) {
                "symlink-escape"
            } else {
                "escape"
            },
        })
    }

    fn enforce_containment_lexical(
        &self,
        resolved: &Path,
        input: &str,
    ) -> Result<PathBuf, WorkspaceDenied> {
        // `resolved` starts from a verified canonical ancestor plus inert file
        // name components, so a lexical check is sufficient here.
        if self.is_within_canonical(resolved) {
            return Ok(resolved.to_path_buf());
        }
        Err(WorkspaceDenied {
            input: input.to_string(),
            reason: "escape",
        })
    }

    fn is_within_canonical(&self, path: &Path) -> bool {
        self.canonical_roots
            .iter()
            .any(|root| path.starts_with(root))
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
}
