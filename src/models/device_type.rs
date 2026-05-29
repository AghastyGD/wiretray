#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    Ethernet,
    Wifi,
    Bridge,
    Veth,
    Loopback,
    Unknown(u32),
}

impl From<u32> for DeviceType {
    fn from(value: u32) -> Self {
        match value {
            1 => Self::Ethernet,
            2 => Self::Wifi,
            13 => Self::Bridge,
            20 => Self::Veth,
            32 => Self::Loopback,
            other => Self::Unknown(other),
        }
    }
}
