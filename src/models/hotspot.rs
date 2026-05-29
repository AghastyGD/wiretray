use super::device::Device;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotspotBackend {
    NetworkManager,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotStartRequest {
    pub interface: String,
    pub ssid: String,
    pub passphrase: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotspotRadioState {
    pub software_enabled: bool,
    pub hardware_enabled: bool,
}

impl HotspotRadioState {
    pub fn is_available(self) -> bool {
        self.software_enabled && self.hardware_enabled
    }
}

#[derive(Debug)]
pub struct HotspotCapability {
    pub backend: HotspotBackend,
    pub radio_state: HotspotRadioState,
    pub wifi_devices: Vec<Device>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotspotConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
    Other(u32),
}

impl From<u32> for HotspotConnectionState {
    fn from(value: u32) -> Self {
        match value {
            0 => Self::Unknown,
            1 => Self::Activating,
            2 => Self::Activated,
            3 => Self::Deactivating,
            4 => Self::Deactivated,
            other => Self::Other(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveHotspot {
    pub id: String,
    pub uuid: String,
    pub interface: String,
    pub connection_path: String,
    pub active_connection_path: String,
    pub state: HotspotConnectionState,
}

impl HotspotCapability {
    pub fn is_supported(&self) -> bool {
        self.radio_state.hardware_enabled && !self.wifi_devices.is_empty()
    }
}
