mod app;
mod tray;
mod dbus;
mod models;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    tracing::info!("Starting WireTray...");

    app::run().await?;

    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}