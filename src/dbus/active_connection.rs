use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

#[derive(Debug)]
pub struct ActiveConnectionInfo {
    pub connection_path: String,
    pub id: String,
    pub uuid: String,
    pub connection_type: String,
    pub devices: Vec<OwnedObjectPath>,
    pub state: u32,
}

pub async fn load_active_connection(
    connection: &Connection,
    path: &str,
) -> zbus::Result<ActiveConnectionInfo> {
    let proxy = Proxy::new(
        connection,
        "org.freedesktop.NetworkManager",
        path,
        "org.freedesktop.NetworkManager.Connection.Active",
    )
    .await?;

    let connection_path: OwnedObjectPath = proxy.get_property("Connection").await?;
    let id: String = proxy.get_property("Id").await?;
    let uuid: String = proxy.get_property("Uuid").await?;
    let connection_type: String = proxy.get_property("Type").await?;
    let devices: Vec<OwnedObjectPath> = proxy.get_property("Devices").await?;
    let state: u32 = proxy.get_property("State").await?;

    Ok(ActiveConnectionInfo {
        connection_path: connection_path.to_string(),
        id,
        uuid,
        connection_type,
        devices,
        state,
    })
}
