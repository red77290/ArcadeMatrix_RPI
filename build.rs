fn main() {
    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        chrono::Utc::now().to_rfc3339()
    );
    println!(
        "cargo:rustc-env=BUILD_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );

    // Single Source of Truth version detection: VERSION file -> Cargo.toml -> CI tag override
    let mut version = std::fs::read_to_string("VERSION")
        .ok()
        .map(|s| s.trim().trim_start_matches('v').to_string())
        .filter(|s| !s.is_empty());

    if version.is_none() {
        version = Some(env!("CARGO_PKG_VERSION").to_string());
    }

    if let Ok(ci_tag) = std::env::var("GITHUB_REF_NAME") {
        if ci_tag.starts_with('v') {
            version = Some(ci_tag.trim_start_matches('v').to_string());
        }
    }

    let final_version = version.unwrap_or_else(|| "3.0.0".to_string());
    println!("cargo:rustc-env=APP_VERSION={final_version}");

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
    // Rebuild the stamp whenever HEAD or tags move
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs/heads");
    println!("cargo:rerun-if-changed=.git/refs/tags");
}
