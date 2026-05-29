use anyhow::Result;
use std::env;

use crate::models::{device_state::DeviceState, hotspot::HotspotStartRequest};
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

    if let Some(active_hotspot) = hotspot.active_hotspot(None).await? {
        tracing::info!(
            interface = active_hotspot.interface,
            id = active_hotspot.id,
            state = ?active_hotspot.state,
            "Active WireTray hotspot"
        );
    } else {
        tracing::info!("Active WireTray hotspot not found");
    }

    handle_hotspot_test_action(&hotspot).await?;

    Ok(())
}

async fn handle_hotspot_test_action(hotspot: &HotspotService) -> Result<()> {
    let Ok(action) = env::var("WIRETRAY_HOTSPOT_ACTION") else {
        return Ok(());
    };

    match action.as_str() {
        "status" => {
            let active_hotspot = hotspot.active_hotspot(None).await?;

            tracing::info!(
                active = active_hotspot.is_some(),
                "Hotspot status request handled"
            );
        }
        "start" => {
            let interface = match env::var("WIRETRAY_HOTSPOT_INTERFACE") {
                Ok(interface) => interface,
                Err(_) => default_hotspot_interface(hotspot).await?,
            };
            let ssid = env::var("WIRETRAY_HOTSPOT_SSID")?;
            let passphrase = env::var("WIRETRAY_HOTSPOT_PASSPHRASE").ok();
            let active_hotspot = hotspot
                .start(HotspotStartRequest {
                    interface,
                    ssid,
                    passphrase,
                })
                .await?;

            tracing::info!(
                interface = active_hotspot.interface,
                id = active_hotspot.id,
                state = ?active_hotspot.state,
                "Hotspot start request handled"
            );
        }
        "stop" => {
            let interface = env::var("WIRETRAY_HOTSPOT_INTERFACE").ok();
            let stopped_hotspot = hotspot.stop(interface.as_deref()).await?;

            tracing::info!(
                stopped = stopped_hotspot.is_some(),
                "Hotspot stop request handled"
            );
        }
        other => {
            tracing::warn!(action = other, "Unknown hotspot test action");
        }
    }

    Ok(())
}

async fn default_hotspot_interface(hotspot: &HotspotService) -> Result<String> {
    let mut devices = hotspot.candidate_devices().await?;

    devices.sort_by_key(|device| matches!(device.state, DeviceState::Unmanaged));

    let device = devices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Wi-Fi device available for hotspot testing"))?;

    Ok(device.interface)
}
