use super::device_state::DeviceState;
use super::device_type::DeviceType;

#[derive(Debug)]
pub struct Device {
    pub path: String,
    pub interface: String,
    pub device_type: DeviceType,
    #[allow(dead_code)]
    pub state: DeviceState,

    pub capabilities: DeviceCapabilities,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceCapabilities {
    pub access_point: bool,
}

impl Device {
    pub fn supports_hotspot(&self) -> bool {
        self.capabilities.access_point
    }
}
