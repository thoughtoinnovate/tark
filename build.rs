use std::process::Command;

fn main() {
    // Get git commit hash (short)
    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Get build timestamp (use date command to avoid chrono dependency in build.rs)
    let build_time = Command::new("date")
        .args(["+%Y%m%d-%H%M%S"])
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    // Check if we're on a tagged release
    let is_release = Command::new("git")
        .args(["describe", "--exact-match", "--tags", "HEAD"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);

    // Set version suffix for development builds
    if !is_release {
        println!(
            "cargo:rustc-env=TARK_VERSION_SUFFIX=-dev.{}.{}",
            build_time, git_hash
        );
    } else {
        println!("cargo:rustc-env=TARK_VERSION_SUFFIX=");
    }

    // Pass individual components to runtime
    println!("cargo:rustc-env=TARK_GIT_HASH={}", git_hash);
    println!("cargo:rustc-env=TARK_BUILD_TIME={}", build_time);
    println!("cargo:rustc-env=TARK_IS_RELEASE={}", is_release);

    // Rebuild if git HEAD changes
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
