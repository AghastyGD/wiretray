use anyhow::Result;

use crate::models::{
    device::Device,
    hotspot::{HotspotBackend, HotspotCapability, HotspotRadioState},
};

use super::network_service::NetworkService;

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

    pub fn backend(&self) -> HotspotBackend {
        match self.backend {
            HotspotBackendHandle::NetworkManager(_) => HotspotBackend::NetworkManager,
        }
    }

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

    pub async fn capability(&self) -> Result<HotspotCapability> {
        Ok(HotspotCapability {
            backend: self.backend(),
            radio_state: self.radio_state().await?,
            wifi_devices: self.candidate_devices().await?,
        })
    }
}
