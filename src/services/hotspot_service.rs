use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result, bail};
use uuid::Uuid;
use zbus::zvariant::Value;

use crate::models::{
    device::Device,
    hotspot::{
        ActiveHotspot, HotspotBackend, HotspotCapability, HotspotConfig, HotspotConnectionState,
        HotspotRadioState,
    },
};

use super::network_service::NetworkService;

const HOTSPOT_ID_PREFIX: &str = "wiretray-hotspot";
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(30);
const ACTIVATION_POLL_INTERVAL: Duration = Duration::from_millis(200);

enum HotspotBackendHandle {
    NetworkManager(NetworkService),
}

pub struct HotspotService {
    backend: HotspotBackendHandle,
}

impl HotspotService {
    pub async fn new() -> Result<Self> {
        let network = NetworkService::new().await?;

        Ok(Self {
            backend: HotspotBackendHandle::NetworkManager(network),
        })
    }

    #[allow(dead_code)]
    pub fn backend(&self) -> HotspotBackend {
        match &self.backend {
            HotspotBackendHandle::NetworkManager(_) => HotspotBackend::NetworkManager,
        }
    }

    #[allow(dead_code)]
    pub async fn radio_state(&self) -> Result<HotspotRadioState> {
        match &self.backend {
            HotspotBackendHandle::NetworkManager(network) => Ok(HotspotRadioState {
                software_enabled: network.wireless_enabled().await?,
                hardware_enabled: network.wireless_hardware_enabled().await?,
            }),
        }
    }

    pub async fn candidate_devices(&self) -> Result<Vec<Device>> {
        match &self.backend {
            HotspotBackendHandle::NetworkManager(network) => network.wifi_devices().await,
        }
    }

#[allow(dead_code)]
    pub async fn capability(&self) -> Result<HotspotCapability> {
        let wifi_devices = self.candidate_devices().await?;

        for device in &wifi_devices {
            tracing::info!(
                "{} supports AP: {}",
                device.interface,
                device.capabilities.access_point
            );
        }

        Ok(HotspotCapability {
            backend: self.backend(),
            radio_state: self.radio_state().await?,
            wifi_devices,
        })
    }

    pub async fn active_hotspot(&self, interface: Option<&str>) -> Result<Option<ActiveHotspot>> {
        match &self.backend {
            HotspotBackendHandle::NetworkManager(network) => {
                find_active_hotspot(network, interface).await
            }
        }
    }

    pub async fn start(&self, request: HotspotConfig) -> Result<ActiveHotspot> {
        validate_start_request(&request)?;

        match &self.backend {
            HotspotBackendHandle::NetworkManager(network) => {
                if let Some(active_hotspot) =
                    find_active_hotspot(network, Some(&request.interface)).await?
                {
                    return Ok(active_hotspot);
                }

                let device = network
                    .wifi_devices()
                    .await?
                    .into_iter()
                    .find(|device| device.interface == request.interface)
                    .with_context(|| {
                        format!("No Wi-Fi device found for interface {}", request.interface)
                    })?;

                let connection_id = hotspot_connection_id(&request.interface);
                let connection_uuid = Uuid::new_v4().to_string();
                let settings = build_hotspot_settings(&request, &connection_id, &connection_uuid);
                let options = build_activation_options();
                let (connection_path, active_connection_path) =
                    crate::dbus::network_manager::add_and_activate_connection2(
                        network.connection(),
                        settings,
                        &device.path,
                        options,
                    )
                    .await?;

                wait_for_activation(network.connection(), active_connection_path.as_str()).await?;

                let active_connection = crate::dbus::active_connection::load_active_connection(
                    network.connection(),
                    active_connection_path.as_str(),
                )
                .await?;

                Ok(ActiveHotspot {
                    id: connection_id,
                    uuid: connection_uuid,
                    interface: request.interface,
                    connection_path: connection_path.to_string(),
                    active_connection_path: active_connection_path.to_string(),
                    state: HotspotConnectionState::from(active_connection.state),
                })
            }
        }
    }

    pub async fn stop(&self, interface: Option<&str>) -> Result<Option<ActiveHotspot>> {
        match &self.backend {
            HotspotBackendHandle::NetworkManager(network) => {
                let Some(active_hotspot) = find_active_hotspot(network, interface).await? else {
                    return Ok(None);
                };

                crate::dbus::network_manager::deactivate_connection(
                    network.connection(),
                    &active_hotspot.active_connection_path,
                )
                .await?;

                Ok(Some(active_hotspot))
            }
        }
    }
}

async fn wait_for_activation(
    connection: &zbus::Connection,
    active_connection_path: &str,
) -> Result<()> {
    use tokio::time::{sleep, timeout};

    match timeout(ACTIVATION_TIMEOUT, async {
        loop {
            let info = crate::dbus::active_connection::load_active_connection(
                connection,
                active_connection_path,
            )
            .await
            .context("Failed to read active connection state")?;

            match HotspotConnectionState::from(info.state) {
                HotspotConnectionState::Activated => return Ok(()),
                HotspotConnectionState::Deactivating | HotspotConnectionState::Deactivated => {
                    bail!("Hotspot activation failed: connection deactivated unexpectedly");
                }
                _ => {
                    sleep(ACTIVATION_POLL_INTERVAL).await;
                }
            }
        }
    })
    .await
    {
        Ok(result) => result,
        Err(_) => bail!(
            "Hotspot activation timed out after {}s",
            ACTIVATION_TIMEOUT.as_secs()
        ),
    }
}

async fn find_active_hotspot(
    network: &NetworkService,
    interface: Option<&str>,
) -> Result<Option<ActiveHotspot>> {
    let active_connection_paths =
        crate::dbus::network_manager::active_connections(network.connection()).await?;

    for path in active_connection_paths {
        let active_connection = crate::dbus::active_connection::load_active_connection(
            network.connection(),
            path.as_str(),
        )
        .await?;

        if active_connection.connection_type != "802-11-wireless"
            || !active_connection.id.starts_with(HOTSPOT_ID_PREFIX)
        {
            continue;
        }

        let Some(device_path) = active_connection.devices.first() else {
            continue;
        };

        let device =
            crate::dbus::device::load_device(network.connection(), device_path.as_str()).await?;

        if interface.is_some_and(|expected| device.interface != expected) {
            continue;
        }

        return Ok(Some(ActiveHotspot {
            id: active_connection.id,
            uuid: active_connection.uuid,
            interface: device.interface,
            connection_path: active_connection.connection_path,
            active_connection_path: path.to_string(),
            state: HotspotConnectionState::from(active_connection.state),
        }));
    }

    Ok(None)
}

fn validate_start_request(request: &HotspotConfig) -> Result<()> {
    if request.interface.trim().is_empty() {
        bail!("Hotspot interface is required");
    }

    let ssid_len = request.ssid.len();
    if ssid_len == 0 || ssid_len > 32 {
        bail!("Hotspot SSID must be between 1 and 32 bytes");
    }

    if let Some(passphrase) = &request.passphrase {
        let is_ascii_passphrase = (8..=63).contains(&passphrase.len());
        let is_hex_psk = passphrase.len() == 64
            && passphrase
                .chars()
                .all(|character| character.is_ascii_hexdigit());

        if !is_ascii_passphrase && !is_hex_psk {
            bail!("Hotspot passphrase must be 8-63 characters or a 64-character hexadecimal PSK");
        }
    }

    Ok(())
}

fn hotspot_connection_id(interface: &str) -> String {
    format!("{HOTSPOT_ID_PREFIX}-{interface}")
}

fn build_hotspot_settings(
    request: &HotspotConfig,
    connection_id: &str,
    connection_uuid: &str,
) -> crate::dbus::network_manager::ConnectionSettings {
    let mut settings = HashMap::new();

    let mut connection = HashMap::new();
    connection.insert("id", Value::from(connection_id.to_string()));
    connection.insert("type", Value::from("802-11-wireless".to_string()));
    connection.insert("uuid", Value::from(connection_uuid.to_string()));
    connection.insert("autoconnect", Value::from(false));
    connection.insert("interface-name", Value::from(request.interface.clone()));
    settings.insert("connection", connection);

    let mut wireless = HashMap::new();
    wireless.insert("mode", Value::from("ap".to_string()));
    wireless.insert("ssid", Value::from(request.ssid.as_bytes().to_vec()));
    wireless.insert("band", Value::from("bg".to_string()));
    wireless.insert("channel", Value::from(1_u32));
    settings.insert("802-11-wireless", wireless);

    let mut ipv4 = HashMap::new();
    ipv4.insert("method", Value::from("shared".to_string()));
    settings.insert("ipv4", ipv4);

    let mut ipv6 = HashMap::new();
    ipv6.insert("method", Value::from("disabled".to_string()));
    settings.insert("ipv6", ipv6);

    if let Some(passphrase) = &request.passphrase {
        let mut wireless_security = HashMap::new();
        wireless_security.insert("key-mgmt", Value::from("wpa-psk".to_string()));
        wireless_security.insert("proto", Value::from(vec!["rsn".to_string()]));
        wireless_security.insert("pairwise", Value::from(vec!["ccmp".to_string()]));
        wireless_security.insert("group", Value::from(vec!["ccmp".to_string()]));
        wireless_security.insert("psk", Value::from(passphrase.clone()));
        settings.insert("802-11-wireless-security", wireless_security);
    }

    settings
}

fn build_activation_options() -> crate::dbus::network_manager::ActivationOptions {
    let mut options = HashMap::new();
    options.insert("persist", Value::from("volatile".to_string()));

    options
}
