use super::device_state::DeviceState;
use super::device_type::DeviceType;

#[derive(Debug)]
pub struct Device {
    pub path: String,
    pub interface: String,
    pub device_type: DeviceType,
    pub state: DeviceState,
}
