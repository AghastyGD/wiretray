mod app;
mod dbus;
mod models;
mod services;
mod tray;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    tracing::info!("Starting wiretray...");

    app::run().await?;

    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
