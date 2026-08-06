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
    let subscriber = FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

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
