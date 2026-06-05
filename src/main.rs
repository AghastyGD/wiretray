use anyhow::Result;

use wiretray::tray;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    tracing::info!("Starting Wiretray...");

    tray::run().await
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
