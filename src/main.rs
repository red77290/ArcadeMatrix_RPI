#![recursion_limit = "256"]
#![allow(dead_code, unused_variables, clippy::all)]

mod api;
mod app;
mod core;
mod engines;

use app::ArcadeMatrixApp;
use tracing_subscriber::FmtSubscriber;

#[tokio::main(flavor = "current_thread")]
async fn main() -> std::io::Result<()> {
    // 1. Agressively hunt for the asset directory
    // Since the user might have named their folder anything (ArcadeMatrix_RPI, arcade, etc.)
    // we scan /home/*/* to find the folder containing "gifs" and "fonts".
    let mut dir_set = false;

    // First, check the most common paths directly
    let common_paths = [
        "/home/pi/ArcadeMatrix_RPI",
        "/home/pi/ArcadeMatrix_RPi",
        "/opt/arcadematrix",
    ];
    for path in common_paths.iter() {
        let p = std::path::Path::new(path);
        if p.join("gifs").exists() && p.join("fonts").exists() {
            if std::env::set_current_dir(p).is_ok() {
                dir_set = true;
                break;
            }
        }
    }

    // If not found, aggressively scan all folders in /home
    if !dir_set {
        if let Ok(home_entries) = std::fs::read_dir("/home") {
            for user_dir in home_entries.flatten() {
                if let Ok(sub_entries) = std::fs::read_dir(user_dir.path()) {
                    for entry in sub_entries.flatten() {
                        if entry.path().is_dir() {
                            if entry.path().join("gifs").exists()
                                && entry.path().join("fonts").exists()
                            {
                                if std::env::set_current_dir(entry.path()).is_ok() {
                                    dir_set = true;
                                    break;
                                }
                            }
                        }
                    }
                }
                if dir_set {
                    break;
                }
            }
        }
    }

    // Fallback to executable directory if EVERYTHING fails
    if !dir_set {
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let _ = std::env::set_current_dir(exe_dir);
            }
        }
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    tracing::info!(
        "Working directory forced to: {:?}",
        std::env::current_dir().unwrap_or_default()
    );

    // Log panics (including those on the isolated render thread) with their
    // exact location so a silent thread death / black screen can be diagnosed
    // from the application log (journalctl -u arcadematrix).
    std::panic::set_hook(Box::new(|info| {
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "unknown".to_string());
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| info.payload().downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "<non-string panic payload>".to_string());
        let thread = std::thread::current()
            .name()
            .unwrap_or("unnamed")
            .to_string();
        tracing::error!(target: "panic", thread = %thread, location = %location, message = %msg, "THREAD PANIC");
    }));

    core::wifi::start_wifi_monitor();

    let app = ArcadeMatrixApp::new();
    app.run().await
}
