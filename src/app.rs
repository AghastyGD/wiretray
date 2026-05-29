use anyhow::Result;

pub async fn run() -> Result<()> {
    tracing::info!("Application initialized");

    let connection = crate::dbus::connection::system_connection()
        .await?;

    tracing::info!("Connected to system DBus");

    let enabled = crate::dbus::network_manager::wireless_enabled(&connection,)
        .await?;

    tracing::info!("Wireless enabled: {}", enabled);

    let devices = crate::dbus::network_manager::get_devices(&connection,)
        .await?;

    for device in devices {
        tracing::info!("Found device: {}", device)
    }

    Ok(())
}

