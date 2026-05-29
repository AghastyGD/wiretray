use zbus::{Connection, Proxy, zvariant::OwnedObjectPath};

pub async fn create_proxy(connection: &Connection) -> zbus::Result<Proxy<'_>> {
    Proxy::new(
        connection,
        "org.freedesktop.NetworkManager",
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
    )
    .await
}

pub async fn wireless_enabled(connection: &Connection) -> zbus::Result<bool> {
    let proxy = create_proxy(connection).await?;

    proxy.get_property("WirelessEnabled").await
}

pub async fn get_devices(connection: &Connection) -> zbus::Result<Vec<OwnedObjectPath>> {
    let proxy = create_proxy(connection).await?;

    proxy.call("GetDevices", &()).await
}
