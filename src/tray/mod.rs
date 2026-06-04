use std::{sync::mpsc, time::Duration};

use anyhow::{Context, Result};
use tray_icon::{TrayIconBuilder, menu::MenuEvent};

use crate::{models::hotspot::HotspotStartRequest, services::hotspot_service::HotspotService};

pub mod menu;

const DEFAULT_SSID: &str = "Wiretray";
const DEFAULT_PASSPHRASE: &str = "wiretray1234";

const ICON_INACTIVE: &[u8] = include_bytes!("../../assets/icons/tray/inactive.png");
const ICON_ACTIVE: &[u8] = include_bytes!("../../assets/icons/tray/active.png");

enum HotspotUpdate {
    Started,
    Stopped,
}

pub fn run(handle: tokio::runtime::Handle) -> Result<()> {
    let (tray_menu, items) = menu::build()?;

    let hotspot_active = handle.block_on(async {
        match HotspotService::new().await {
            Ok(svc) => match svc.active_hotspot(None).await {
                Ok(active) => active.is_some(),
                Err(e) => {
                    tracing::warn!("Failed to determine initial hotspot state: {e:#}");
                    false
                }
            },
            Err(e) => {
                tracing::warn!("Failed to initialize hotspot service: {e:#}");
                false
            }
        }
    });

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Wiretray - Hotspot Manager")
        .with_icon(load_icon(if hotspot_active {
            ICON_ACTIVE
        } else {
            ICON_INACTIVE
        }))
        .build()
        .context("Failed to create tray icon")?;

    let (update_tx, update_rx) = mpsc::channel::<HotspotUpdate>();
    let menu_rx = MenuEvent::receiver();

    gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(update) = update_rx.try_recv() {
            let icon = match update {
                HotspotUpdate::Started => load_icon(ICON_ACTIVE),
                HotspotUpdate::Stopped => load_icon(ICON_INACTIVE),
            };
            if let Err(e) = tray.set_icon(Some(icon)) {
                tracing::warn!("Failed to update tray icon: {e}");
            }
        }

        while let Ok(event) = menu_rx.try_recv() {
            if event.id == items.quit {
                gtk::main_quit();
                return gtk::glib::ControlFlow::Break;
            }
            if event.id == items.start_hotspot {
                let h = handle.clone();
                let tx = update_tx.clone();
                h.spawn(async move {
                    match do_start_hotspot().await {
                        Ok(()) => {
                            let _ = tx.send(HotspotUpdate::Started);
                        }
                        Err(e) => tracing::error!("Failed to start hotspot: {e:#}"),
                    }
                });
            }
            if event.id == items.stop_hotspot {
                let h = handle.clone();
                let tx = update_tx.clone();
                h.spawn(async move {
                    match do_stop_hotspot().await {
                        Ok(()) => {
                            let _ = tx.send(HotspotUpdate::Stopped);
                        }
                        Err(e) => tracing::error!("Failed to stop hotspot: {e:#}"),
                    }
                });
            }
        }

        gtk::glib::ControlFlow::Continue
    });

    gtk::main();

    Ok(())
}

async fn do_start_hotspot() -> Result<()> {
    let hotspot = HotspotService::new().await?;

    let interface = hotspot
        .candidate_devices()
        .await?
        .into_iter()
        .next()
        .context("No Wi-Fi device available for hotspot")?
        .interface;

    let req = HotspotStartRequest {
        interface,
        ssid: DEFAULT_SSID.to_string(),
        passphrase: Some(DEFAULT_PASSPHRASE.to_string()),
    };

    let active = hotspot.start(req).await?;
    tracing::info!(
        interface = active.interface,
        state = ?active.state,
        "Hotspot started"
    );
    Ok(())
}

async fn do_stop_hotspot() -> Result<()> {
    let hotspot = HotspotService::new().await?;
    let stopped = hotspot.stop(None).await?;
    tracing::info!(stopped = stopped.is_some(), "Hotspot stop requested");
    Ok(())
}

fn load_icon(png_data: &[u8]) -> tray_icon::Icon {
    let img = image::load_from_memory(png_data)
        .expect("valid PNG data")
        .into_rgba8();
    let (w, h) = img.dimensions();
    tray_icon::Icon::from_rgba(img.into_raw(), w, h).expect("valid icon data")
}
