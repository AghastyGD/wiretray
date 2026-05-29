use anyhow::Result;
use zbus::Connection;

use crate::dbus;
use crate::models::device::Device;

pub struct NetworkService {
    connection: Connection,
}

impl NetworkService {
    pub async fn new() -> Result<Self> {
        let connection = dbus::connection::system_connection().await?;

        Ok(Self { connection })
    }

    pub async fn wireless_enabled(&self) -> Result<bool> {
        Ok(dbus::network_manager::wireless_enabled(&self.connection).await?)
    }

    pub async fn wireless_hardware_enabled(&self) -> Result<bool> {
        Ok(dbus::network_manager::wireless_hardware_enabled(&self.connection).await?)
    }

    pub(crate) fn connection(&self) -> &Connection {
        &self.connection
    }

    pub async fn devices(&self) -> Result<Vec<Device>> {
        let paths = dbus::network_manager::get_devices(&self.connection).await?;

        let mut devices = Vec::new();

        for path in paths {
            let device = dbus::device::load_device(&self.connection, path.as_str()).await?;

            devices.push(device);
        }

        Ok(devices)
    }

    pub async fn wifi_devices(&self) -> Result<Vec<Device>> {
        let devices = self.devices().await?;

        Ok(devices
            .into_iter()
            .filter(|device| {
                matches!(
                    device.device_type,
                    crate::models::device_type::DeviceType::Wifi
                )
            })
            .collect())
    }
}
