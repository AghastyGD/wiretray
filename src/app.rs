use anyhow::Result;

pub async fn run() -> Result<()> {
    tracing::info!("Application initialized");

    crate::tray::init()?;

    Ok(())
}

