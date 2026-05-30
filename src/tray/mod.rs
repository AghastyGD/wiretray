use anyhow::Result;

pub mod menu;

#[allow(dead_code)]
pub fn init() -> Result<()> {
    tracing::info!("Initializing tray");

    menu::build();

    Ok(())
}
