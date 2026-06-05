use anyhow::Result;

use crate::{
    models::hotspot::HotspotConfig,
    services::hotspot_service::HotspotService,
    settings::service::SettingsService,
};

pub async fn start() -> Result<()> {
    let hotspot = HotspotService::new().await?;

    let interface = hotspot
        .candidate_devices()
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow::anyhow!("No Wi-Fi device available for hotspot"))?
        .interface;

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
    let hotspot = HotspotService::new().await?;
    let stopped = hotspot.stop(None).await?;
    tracing::info!(stopped = stopped.is_some(), "Hotspot stopped");
    Ok(())
}

pub async fn is_active() -> bool {
    match HotspotService::new().await {
        Ok(svc) => match svc.active_hotspot(None).await {
            Ok(active) => active.is_some(),
            Err(e) => {
                tracing::warn!("Failed to determine initial hotspot state: {e:#}");
                false
            }
        },
        Err(e) => {
            tracing::warn!("Failed to initialize hotspot service: {e:#}");
            false
        }
    }
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
