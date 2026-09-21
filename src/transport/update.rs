//! Self-update for the tark binary (`tark update`).
//!
//! Resolves the target version (explicit pin or latest GitHub release),
//! compares it against the running binary, and replaces the executable in
//! place with SHA256 verification and a backup for rollback. Mirrors the
//! behavior of `install.sh` so both entry points agree.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// GitHub repo hosting releases.
const RELEASE_REPO: &str = "thoughtoinnovate/tark";

/// Current binary version (bare number, e.g. `0.12.7`).
pub fn current_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Strip a leading `v` (`v0.12.7` -> `0.12.7`).
pub fn normalize_version(input: &str) -> String {
    input.strip_prefix('v').unwrap_or(input).to_string()
}

/// Parse `major.minor.patch` (leading `v` tolerated).
pub fn parse_version(input: &str) -> Result<(u64, u64, u64)> {
    let normalized = normalize_version(input);
    let parts: Vec<&str> = normalized.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid version '{}': expected form v0.12.7", input);
    }
    let nums: Result<Vec<u64>> = parts
        .iter()
        .map(|p| {
            p.parse::<u64>()
                .map_err(|_| anyhow::anyhow!("Invalid version '{}': non-numeric part", input))
        })
        .collect();
    let nums = nums?;
    Ok((nums[0], nums[1], nums[2]))
}

/// Ordering between two versions: -1 (older), 0 (equal), 1 (newer).
pub fn compare_versions(installed: &str, target: &str) -> Result<i8> {
    let a = parse_version(installed)?;
    let b = parse_version(target)?;
    Ok(a.cmp(&b) as i8)
}

/// Release asset name for this platform, mirroring release.yml artifacts.
/// Returns `None` on unsupported platforms instead of guessing.
pub fn asset_name() -> Option<String> {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;
    let base = match (os, arch) {
        ("linux", "x86_64") => "tark-linux-x86_64",
        ("linux", "aarch64") => "tark-linux-arm64",
        ("macos", "x86_64") => "tark-darwin-x86_64",
        ("macos", "aarch64") => "tark-darwin-arm64",
        ("windows", "x86_64") => "tark-windows-x86_64.exe",
        ("windows", "aarch64") => "tark-windows-arm64.exe",
        _ => return None,
    };
    Some(base.to_string())
}

#[derive(Debug, serde::Deserialize)]
struct ReleaseInfo {
    #[serde(default)]
    tag_name: String,
}

/// Fetch the latest release tag (e.g. `v0.12.7`) from the GitHub API.
pub async fn fetch_latest_tag() -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("tark-self-update")
        .build()
        .context("Failed to build HTTP client")?;
    let url = format!("https://api.github.com/repos/{RELEASE_REPO}/releases/latest");
    let info: ReleaseInfo = client
        .get(&url)
        .send()
        .await
        .context("Failed to query latest release (network/API issue?)")?
        .error_for_status()
        .context("GitHub API rejected the latest-release query")?
        .json()
        .await
        .context("Failed to parse latest-release response")?;
    parse_version(&info.tag_name)?;
    Ok(info.tag_name)
}

/// Download a release asset (plus its `.sha256` sidecar) into a temp dir.
/// Returns the verified binary path.
async fn download_verified(tag: &str, asset: &str, dir: &Path) -> Result<PathBuf> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .user_agent("tark-self-update")
        .build()
        .context("Failed to build HTTP client")?;
    let base = format!("https://github.com/{RELEASE_REPO}/releases/download/{tag}");
    let bin_path = dir.join(asset);
    let get = |url: String| {
        let client = client.clone();
        async move {
            let bytes = client
                .get(url)
                .send()
                .await?
                .error_for_status()?
                .bytes()
                .await?;
            Ok::<_, anyhow::Error>(bytes)
        }
    };

    let expected_hex = {
        let raw = get(format!("{base}/{asset}.sha256")).await?;
        let text = String::from_utf8_lossy(&raw);
        text.split_whitespace()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty checksum file"))?
            .to_string()
    };
    let bytes = get(format!("{base}/{asset}")).await?;
    verify_sha256(&bytes, &expected_hex)?;
    std::fs::write(&bin_path, &bytes)
        .with_context(|| format!("Failed to stage {}", bin_path.display()))?;
    Ok(bin_path)
}

/// Verify `bytes` against a lowercase hex SHA256 digest.
pub fn verify_sha256(bytes: &[u8], expected_hex: &str) -> Result<()> {
    use sha2::Digest;
    let actual = hex::encode(sha2::Sha256::digest(bytes));
    if actual.eq_ignore_ascii_case(expected_hex.trim()) {
        Ok(())
    } else {
        anyhow::bail!("Checksum mismatch: binary failed SHA256 verification");
    }
}

/// Replace the running binary with `staged`, keeping a `.bak` copy.
/// Returns `(backup, installed)`. On failure the backup is restored when
/// possible. Callers must verify via the returned installed path: on Linux,
/// `current_exe()` still resolves to the renamed backup image.
fn replace_current_exe(staged: &Path) -> Result<(PathBuf, PathBuf)> {
    let current = std::env::current_exe().context("Cannot locate running binary")?;
    let backup = current.with_extension("bak");
    let _ = std::fs::remove_file(&backup);
    // Windows cannot overwrite a running image; rename aside first (rename
    // of a running executable is allowed, delete is not).
    std::fs::rename(&current, &backup)
        .with_context(|| format!("Failed to stage backup of {}", current.display()))?;
    if let Err(e) = std::fs::rename(staged, &current) {
        let _ = std::fs::rename(&backup, &current);
        return Err(e).with_context(|| format!("Failed to install {}", current.display()));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&current)?.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&current, perms)?;
    }
    Ok((backup, current))
}

/// Run `tark update`: resolve target, compare, and self-replace as needed.
pub async fn run_self_update(pinned: Option<&str>, force: bool, check_only: bool) -> Result<()> {
    let target = match pinned {
        Some(v) => {
            parse_version(v)?;
            v.to_string()
        }
        None => fetch_latest_tag().await?,
    };
    let current = current_version();

    if !force {
        match compare_versions(&current, &target)? {
            0 => {
                println!("tark {current} is already up to date (latest: {target}).");
                return Ok(());
            }
            1 if pinned.is_none() => {
                println!("Installed tark {current} is newer than latest release {target}; leaving it in place. Use --force to reinstall.");
                return Ok(());
            }
            _ => {}
        }
    }
    if check_only {
        println!("Update available: tark {current} -> {target}. Run `tark update` to install.");
        return Ok(());
    }

    let asset = asset_name()
        .ok_or_else(|| anyhow::anyhow!("Self-update is not supported on this platform"))?;
    println!("Updating tark {current} -> {target} ({asset})...");
    let dir = tempfile::tempdir().context("Failed to create staging dir")?;
    let staged = download_verified(&target, &asset, dir.path()).await?;
    let (backup, installed) = replace_current_exe(&staged)?;
    println!(
        "tark updated to {target}. Backup kept at {}.",
        backup.display()
    );
    let reported: String = std::process::Command::new(&installed)
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    if !reported.trim().is_empty() {
        println!("Reported version: {}", reported.trim());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_parse_and_compare() {
        assert_eq!(parse_version("v0.12.7").unwrap(), (0, 12, 7));
        assert_eq!(parse_version("0.12.7").unwrap(), (0, 12, 7));
        assert!(parse_version("0.12").is_err());
        assert!(parse_version("abc").is_err());
        assert_eq!(compare_versions("0.12.6", "v0.12.7").unwrap(), -1);
        assert_eq!(compare_versions("0.12.7", "0.12.7").unwrap(), 0);
        assert_eq!(compare_versions("v0.13.0", "0.12.7").unwrap(), 1);
        assert_eq!(compare_versions("0.9.10", "0.10.0").unwrap(), -1);
    }

    #[test]
    fn checksum_verification() {
        // SHA256("abc").
        let digest = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(verify_sha256(b"abc", digest).is_ok());
        assert!(verify_sha256(b"abc", &digest.to_uppercase()).is_ok());
        assert!(verify_sha256(b"abd", digest).is_err());
        assert!(verify_sha256(b"", digest).is_err());
    }

    #[test]
    fn release_tag_parsing() {
        let info: ReleaseInfo = serde_json::from_str(r#"{"tag_name": "v0.12.7"}"#).unwrap();
        assert_eq!(info.tag_name, "v0.12.7");
        assert!(parse_version(&info.tag_name).is_ok());
    }

    #[test]
    fn current_version_is_semver() {
        assert!(parse_version(&current_version()).is_ok());
    }

    #[test]
    fn asset_name_matches_release_artifacts() {
        // Must mirror the artifact names published by release.yml.
        if let Some(name) = asset_name() {
            assert!(name.starts_with("tark-"));
            #[cfg(windows)]
            assert!(name.ends_with(".exe"));
            #[cfg(not(windows))]
            assert!(!name.ends_with(".exe"));
        }
    }
}
