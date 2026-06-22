use anyhow::Result;

use wiretray::{application::single_instance, tray};

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let _instance_lock = match single_instance::acquire_lock() {
        Ok(lock) => lock,
        Err(err) => {
            tracing::info!("Wiretray is already running: {err:#}");
            return Ok(());
        }
    };

    tracing::info!("Starting Wiretray...");

    tray::run().await
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
