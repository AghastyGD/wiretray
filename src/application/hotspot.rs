use anyhow::{Result, bail};

use crate::{
    models::{
        device_state::DeviceState,
        hotspot::{HotspotBackend, HotspotConfig},
    },
    services::{concurrent_ap::ConcurrentApBackend, hotspot_service::HotspotService},
    settings::service::SettingsService,
};

pub async fn start() -> Result<()> {
    if active_backend_kind().await?.is_some() {
        bail!("A Wiretray hotspot is already active");
    }

    let backend = SettingsService::new().hotspot_backend();
    let hotspot = HotspotService::new_with_backend(backend).await?;

    let devices = hotspot.candidate_devices().await?;
    let interface = if backend == HotspotBackend::ConcurrentAp {
        devices
            .iter()
            .find(|device| device.state == DeviceState::Activated)
            .or_else(|| devices.first())
    } else {
        devices.first()
    }
    .map(|device| device.interface.clone())
    .ok_or_else(|| anyhow::anyhow!("No Wi-Fi device available for hotspot"))?;

    let mut config = load_settings();
    config.interface = interface;

    let active = hotspot.start(config).await?;
    tracing::info!(
        interface = active.interface,
        state = ?active.state,
        "Hotspot started"
    );

    Ok(())
}

pub async fn stop() -> Result<()> {
    let Some(backend) = active_backend_kind().await? else {
        tracing::info!("No Wiretray hotspot is active");
        return Ok(());
    };

    let hotspot = HotspotService::new_with_backend(backend).await?;
    let stopped = hotspot.stop(None).await?;
    tracing::info!(stopped = stopped.is_some(), "Hotspot stopped");
    Ok(())
}

pub async fn is_active() -> bool {
    match active_backend_kind().await {
        Ok(active) => active.is_some(),
        Err(e) => {
            tracing::warn!("Failed to determine hotspot state: {e:#}");
            false
        }
    }
}

async fn active_backend_kind() -> Result<Option<HotspotBackend>> {
    let concurrent = ConcurrentApBackend::new();
    if concurrent.load_runtime_state()?.is_some() {
        return Ok(Some(HotspotBackend::ConcurrentAp));
    }

    let network_manager = HotspotService::new().await?;
    if network_manager.active_hotspot(None).await?.is_some() {
        return Ok(Some(HotspotBackend::NetworkManager));
    }

    Ok(None)
}

fn load_settings() -> HotspotConfig {
    match SettingsService::new().load() {
        Ok(s) => HotspotConfig {
            interface: String::new(),
            ssid: if s.ssid.is_empty() {
                system_hostname()
            } else {
                s.ssid
            },
            passphrase: if s.passphrase.is_empty() {
                None
            } else {
                Some(s.passphrase)
            },
        },
        Err(e) => {
            tracing::warn!("Failed to load settings, using hostname and open network: {e:#}");
            HotspotConfig {
                interface: String::new(),
                ssid: system_hostname(),
                passphrase: None,
            }
        }
    }
}

fn system_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .or_else(|_| std::fs::read_to_string("/proc/sys/kernel/hostname"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Wiretray".to_string())
}
