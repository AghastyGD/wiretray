use std::collections::HashMap;

use zbus::{
    Connection, Proxy,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub type ConnectionSettings = HashMap<&'static str, HashMap<&'static str, Value<'static>>>;
pub type ActivationOptions = HashMap<&'static str, Value<'static>>;

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

pub async fn wireless_hardware_enabled(connection: &Connection) -> zbus::Result<bool> {
    let proxy = create_proxy(connection).await?;

    proxy.get_property("WirelessHardwareEnabled").await
}

pub async fn get_devices(connection: &Connection) -> zbus::Result<Vec<OwnedObjectPath>> {
    let proxy = create_proxy(connection).await?;

    proxy.call("GetDevices", &()).await
}

pub async fn active_connections(connection: &Connection) -> zbus::Result<Vec<OwnedObjectPath>> {
    let proxy = create_proxy(connection).await?;

    proxy.get_property("ActiveConnections").await
}

pub async fn add_and_activate_connection2(
    connection: &Connection,
    settings: ConnectionSettings,
    device_path: &str,
    options: ActivationOptions,
) -> zbus::Result<(OwnedObjectPath, OwnedObjectPath)> {
    let proxy = create_proxy(connection).await?;
    let device_path = OwnedObjectPath::try_from(device_path)?;
    let specific_object = OwnedObjectPath::try_from("/")?;

    let (connection_path, active_connection_path, _result): (
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    ) = proxy
        .call(
            "AddAndActivateConnection2",
            &(settings, device_path, specific_object, options),
        )
        .await?;

    Ok((connection_path, active_connection_path))
}

pub async fn deactivate_connection(
    connection: &Connection,
    active_connection_path: &str,
) -> zbus::Result<()> {
    let proxy = create_proxy(connection).await?;
    let active_connection_path = OwnedObjectPath::try_from(active_connection_path)?;

    proxy
        .call::<_, _, ()>("DeactivateConnection", &(active_connection_path,))
        .await
}
