//! Global memory allocator tuning, heap reclamation (`malloc_trim`), and shared HTTP clients.
//!
//! On GNU/Linux (Raspberry Pi OS), `glibc` (`ptmalloc2`) exhibits two behaviors that cause
//! severe RSS memory retention in multi-threaded media/streaming daemons:
//! 1. By default, `M_ARENA_MAX` is `8 * num_cores` (32 arenas on a 4-core Pi), and every
//!    transient `std::thread::spawn` worker allocates into separate thread arenas.
//! 2. By default, when an `mmap`-allocated buffer (such as a multi-megabyte video stream)
//!    is freed, `glibc` dynamically raises `M_MMAP_THRESHOLD` up to 16–32 MB and retains
//!    all subsequent buffers inside heap arenas instead of `munmap`-ing them back to the OS.
//!
//! Explicitly calling `mallopt(M_MMAP_THRESHOLD, 64 * 1024)` disables dynamic threshold
//! inflation and guarantees that every buffer >= 64 KB is allocated via `mmap` and
//! immediately returned to the Linux kernel via `munmap` upon `drop()`.

use std::sync::OnceLock;
use std::time::Duration;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
mod glibc_ffi {
    use std::os::raw::c_int;

    pub const M_TRIM_THRESHOLD: c_int = -1;
    pub const M_MMAP_THRESHOLD: c_int = -3;
    pub const M_ARENA_MAX: c_int = -8;

    extern "C" {
        pub fn mallopt(param: c_int, value: c_int) -> c_int;
        pub fn malloc_trim(pad: usize) -> c_int;
    }
}

/// Configure the system allocator at startup to bound arena proliferation and disable
/// glibc's dynamic `M_MMAP_THRESHOLD` inflation so large buffers are always `munmap`-ed.
pub fn configure_allocator() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        // Cap per-thread arenas to 2 instead of 8 * cores (32).
        let _ = glibc_ffi::mallopt(glibc_ffi::M_ARENA_MAX, 2);
        // Pin mmap threshold at 64 KB (disables dynamic threshold inflation up to 32 MB).
        let _ = glibc_ffi::mallopt(glibc_ffi::M_MMAP_THRESHOLD, 64 * 1024);
        // Trim top-of-heap whenever >= 64 KB is free.
        let _ = glibc_ffi::mallopt(glibc_ffi::M_TRIM_THRESHOLD, 64 * 1024);
        let _ = glibc_ffi::malloc_trim(0);
    }
}

/// Force the allocator to release all unused free pages across all arenas back to the OS kernel.
#[inline]
pub fn trim_heap() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        let _ = glibc_ffi::malloc_trim(0);
    }
}

/// Process-wide RSS memory pressure level (modeled after ESP32 `MemoryManager`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryPressureLevel {
    /// RSS < 200 MB
    Nominal,
    /// 200 MB <= RSS < 260 MB
    Moderate,
    /// RSS >= 260 MB
    Critical,
}

pub const RSS_MODERATE_THRESHOLD_BYTES: usize = 200 * 1024 * 1024;
pub const RSS_CRITICAL_THRESHOLD_BYTES: usize = 260 * 1024 * 1024;
pub const MAX_POST_DEACTIVATE_RSS_DELTA_BYTES: usize = 15 * 1024 * 1024;

/// Read the current process Resident Set Size (RSS) in bytes from `/proc/self/statm` on Linux.
pub fn current_rss_bytes() -> usize {
    #[cfg(target_os = "linux")]
    {
        if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
            if let Some(rss_pages_str) = statm.split_whitespace().nth(1) {
                if let Ok(pages) = rss_pages_str.parse::<usize>() {
                    return pages.saturating_mul(4096);
                }
            }
        }
    }
    0
}

/// Classify process RSS against the 200 MB (Moderate) and 260 MB (Critical) thresholds.
#[inline]
pub fn evaluate_rss_pressure(rss_bytes: usize) -> MemoryPressureLevel {
    if rss_bytes >= RSS_CRITICAL_THRESHOLD_BYTES {
        MemoryPressureLevel::Critical
    } else if rss_bytes >= RSS_MODERATE_THRESHOLD_BYTES {
        MemoryPressureLevel::Moderate
    } else {
        MemoryPressureLevel::Nominal
    }
}

/// Return `true` if an engine retained more than 15 MB of net RSS growth after `deactivate()`
/// or if overall process RSS is at/above the 200 MB moderate threshold.
#[inline]
pub fn should_recycle_after_deactivate(baseline_rss: usize, post_rss: usize) -> bool {
    if post_rss >= RSS_MODERATE_THRESHOLD_BYTES {
        return true;
    }
    if baseline_rss > 0
        && post_rss > baseline_rss.saturating_add(MAX_POST_DEACTIVATE_RSS_DELTA_BYTES)
    {
        return true;
    }
    false
}

static FAST_HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
static API_HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
static STREAM_HTTP_CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();

/// Shared blocking HTTP client for fast JSON/icon queries (3s connect, 5s total timeout).
/// Reuses a single internal Tokio runtime and TLS store across the entire process.
pub fn fast_http_client() -> &'static reqwest::blocking::Client {
    FAST_HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(5))
            .pool_max_idle_per_host(2)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

/// Shared blocking HTTP client for external APIs (3s connect, 10s total timeout).
pub fn api_http_client() -> &'static reqwest::blocking::Client {
    API_HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .pool_max_idle_per_host(2)
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}

/// Shared blocking HTTP client for binary media stream downloads (2s connect, 25s total timeout).
pub fn stream_http_client() -> &'static reqwest::blocking::Client {
    STREAM_HTTP_CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_millis(2000))
            .timeout(Duration::from_secs(25))
            .pool_max_idle_per_host(2)
            .user_agent("ArcadeMatrix-RPi/3.0")
            .build()
            .unwrap_or_else(|_| reqwest::blocking::Client::new())
    })
}
