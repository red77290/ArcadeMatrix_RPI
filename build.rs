fn main() {
    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        chrono::Utc::now().to_rfc3339()
    );
    println!(
        "cargo:rustc-env=BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
    // Short git commit hash of the built tree, so a running binary can be traced
    // back to an exact source revision (e.g. to confirm a deploy actually landed).
    let git_commit = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=BUILD_GIT_COMMIT={git_commit}");
    println!("cargo:rerun-if-changed=Cargo.toml");
    // Rebuild the stamp whenever HEAD moves so the hash never goes stale.
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
}
