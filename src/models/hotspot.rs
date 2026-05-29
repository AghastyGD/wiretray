use super::device::Device;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotspotBackend {
    NetworkManager,
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

impl HotspotCapability {
    pub fn is_supported(&self) -> bool {
        self.radio_state.hardware_enabled && !self.wifi_devices.is_empty()
    }
}
