use anyhow::Result;

pub mod menu;

pub fn init() -> Result<()> {
    tracing::info!("Initializing tray");

    menu::build();

    Ok(())
}
