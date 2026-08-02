use std::time::Duration;

use anyhow::{Context, Result};
use wiretray::{application::single_instance, tray};
use zbus::Connection;

// StatusNotifierItem uses this D-Bus name even outside KDE.
// It is the tray/status notifier watcher used by AppIndicator/SNI hosts.
const STATUS_NOTIFIER_WATCHER: &str = "org.kde.StatusNotifierWatcher";

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let _instance_lock = match single_instance::acquire_lock() {
        Ok(lock) => lock,
        Err(err) => {
            tracing::info!("Wiretray is already running: {err:#}");
            return Ok(());
        }
    };

    tracing::info!("Starting Wiretray...");

    wait_for_status_notifier_watcher().await?;

    tray::run().await
}

async fn wait_for_status_notifier_watcher() -> Result<()> {
    const MAX_ATTEMPTS: usize = 40;
    const RETRY_DELAY: Duration = Duration::from_millis(500);

    let connection = Connection::session()
        .await
        .context("Failed to connect to D-Bus session bus")?;

    for attempt in 1..=MAX_ATTEMPTS {
        if status_notifier_watcher_available(&connection).await? {
            tracing::info!("StatusNotifierWatcher is available");
            return Ok(());
        }

        if attempt == 1 {
            tracing::warn!("StatusNotifierWatcher is not available yet; waiting...")
        }

        tokio::time::sleep(RETRY_DELAY).await;
    }

    anyhow::bail!("StatusNotifierWatcher was not available after waiting")
}

async fn status_notifier_watcher_available(connection: &Connection) -> Result<bool> {
    let proxy = zbus::Proxy::new(
        connection,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .await
    .context("Failed to create D-Bus proxy")?;

    let has_owner: bool = proxy
        .call("NameHasOwner", &(STATUS_NOTIFIER_WATCHER))
        .await
        .context("Failed to check StatusNotifierWatcher owner")?;

    Ok(has_owner)
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
