//! Centralized build metadata.
//!
//! These `env!` values are injected by `build.rs`. They MUST be read from a
//! single module: `BUILD_TIMESTAMP` (and the git hash after a fresh checkout)
//! change on every build, and Rust bakes `env!` at each call site's compile
//! time. If several modules read them directly, incremental compilation can
//! leave stale copies in modules that weren't recompiled, so `/api/version`
//! and the startup banner would disagree about which binary is running.
//! Reading them here once keeps every consumer consistent.

/// Semantic crate version (`Cargo.toml`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Rust target triple the binary was built for.
pub const ARCH: &str = env!("BUILD_TARGET");

/// UTC RFC-3339 timestamp of the build.
pub const BUILD_TIMESTAMP: &str = env!("BUILD_TIMESTAMP");

/// Short git commit hash of the built source tree (`unknown` if unavailable).
pub const GIT_COMMIT: &str = env!("BUILD_GIT_COMMIT");
