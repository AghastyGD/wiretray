use anyhow::Result;

use crate::services::network_service::NetworkService;

pub async fn run() -> Result<()> {
    tracing::info!("Application initialized");

    let network = NetworkService::new().await?;

    tracing::info!("Connected to system DBus");

    let enabled = network.wireless_enabled().await?;

    tracing::info!(wireless_enabled = enabled, "Wireless status");

    let devices = network.devices().await?;

    for device in devices {
        tracing::info!(
            interface = device.interface,
            device_type = ?device.device_type,
            state = ?device.state,
            "Device discovered"
        );
    }

    Ok(())
}
