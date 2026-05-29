#[derive(Debug)]
pub struct Device {
    pub path: String,
    pub interface: String,
    pub device_type: u32,
    pub state: u32,
}