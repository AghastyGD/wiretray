use crate::models::hotspot::HotspotBackend;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotspotSettings {
    pub ssid: String,
    pub passphrase: String,
    pub backend: HotspotBackend,
}
