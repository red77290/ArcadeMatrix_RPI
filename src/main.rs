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
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let app = ArcadeMatrixApp::new();
    app.run().await
}
