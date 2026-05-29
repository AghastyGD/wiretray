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

    pub async fn devices(&self) -> Result<Vec<Device>> {
        let paths = dbus::network_manager::get_devices(&self.connection).await?;

        let mut devices = Vec::new();

        for path in paths {
            let device = dbus::device::load_device(&self.connection, path.as_str()).await?;

            devices.push(device);
        }

        Ok(devices)
    }
}
