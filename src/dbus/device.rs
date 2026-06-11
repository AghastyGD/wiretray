use zbus::{Connection, Proxy};

use crate::models::{device::{Device, DeviceCapabilities}, device_state::DeviceState, device_type::DeviceType};

const NM_WIFI_DEVICE_CAP_AP: u32 = 0x00000040;

pub async fn load_device(connection: &Connection, path: &str) -> zbus::Result<Device> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.NetworkManager",
        path,
        "org.freedesktop.NetworkManager.Device",
    )
    .await?;

    let interface: String = proxy.get_property("Interface").await?;

    let device_type: u32 = proxy.get_property("DeviceType").await?;

    let state: u32 = proxy.get_property("State").await?;

    let mut access_point = false;

    if DeviceType::from(device_type) == DeviceType::Wifi {
        let wifi_proxy = Proxy::new(
            connection,
            "org.freedesktop.NetworkManager",
            path,
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .await?;

        let capabilites: u32 = wifi_proxy
            .get_property("WirelessCapabilities")
            .await?;

        access_point = (capabilites & NM_WIFI_DEVICE_CAP_AP) != 0;
    }

    Ok(Device {
        path: path.to_string(),
        interface,
        device_type: DeviceType::from(device_type),
        state: DeviceState::from(state),
        capabilities: DeviceCapabilities {
            access_point,
        }
    })
}
