use zbus::Proxy;
use zbus::Connection;

use crate::models::device::Device;

pub async fn load_device(
    connection: &Connection,
    path: &str,
) -> zbus::Result<Device> {
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

    Ok(Device {
        path: path.to_string(),
        interface,
        device_type,
        state,
    })
}
