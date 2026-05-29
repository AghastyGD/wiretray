use anyhow::Result;

use crate::services::{hotspot_service::HotspotService, network_service::NetworkService};

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

    let wifi_devices = network.wifi_devices().await?;

    for device in wifi_devices {
        tracing::info!(
            interface = device.interface,
            state = ?device.state,
            "WiFi device"
        );
    }

    let hotspot = HotspotService::new().await?;
    let hotspot_capability = hotspot.capability().await?;

    tracing::info!(
        backend = ?hotspot_capability.backend,
        radio_available = hotspot_capability.radio_state.is_available(),
        supported = hotspot_capability.is_supported(),
        candidate_devices = hotspot_capability.wifi_devices.len(),
        "Hotspot capability"
    );

    for device in hotspot_capability.wifi_devices {
        tracing::info!(
            interface = device.interface,
            state = ?device.state,
            "Hotspot candidate device"
        );
    }

    Ok(())
}
