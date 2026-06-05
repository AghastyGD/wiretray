use std::sync::Arc;

use anyhow::{Context, Result};
use ksni::{MenuItem, Tray, TrayMethods, menu::StandardItem};
use tokio::sync::mpsc;

use crate::{
    models::hotspot::HotspotConfig, services::hotspot_service::HotspotService,
    settings::service::SettingsService,
};

const ICON_INACTIVE: &[u8] = include_bytes!("../../assets/icons/tray/inactive.png");
const ICON_ACTIVE: &[u8] = include_bytes!("../../assets/icons/tray/active.png");

enum TrayCommand {
    SetActive(bool),
}

struct WireTray {
    active: bool,
    tx: Arc<mpsc::Sender<TrayCommand>>,
}

impl Tray for WireTray {
    fn id(&self) -> String {
        env!("CARGO_PKG_NAME").into()
    }

    fn title(&self) -> String {
        "Wiretray".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![load_icon(if self.active {
            ICON_ACTIVE
        } else {
            ICON_INACTIVE
        })]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: "Wiretray - Hotspot Manager".into(),
            ..Default::default()
        }
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let tx_start = Arc::clone(&self.tx);
        let tx_stop = Arc::clone(&self.tx);
        vec![
            StandardItem {
                label: "Start Hotspot".into(),
                activate: Box::new(move |_tray: &mut Self| {
                    let tx = Arc::clone(&tx_start);
                    tokio::spawn(async move {
                        match do_start_hotspot().await {
                            Ok(()) => {
                                let _ = tx.send(TrayCommand::SetActive(true)).await;
                            }
                            Err(e) => tracing::error!("Failed to start hotspot: {e:#}"),
                        }
                    });
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Stop Hotspot".into(),
                activate: Box::new(move |_tray: &mut Self| {
                    let tx = Arc::clone(&tx_stop);
                    tokio::spawn(async move {
                        match do_stop_hotspot().await {
                            Ok(()) => {
                                let _ = tx.send(TrayCommand::SetActive(false)).await;
                            }
                            Err(e) => tracing::error!("Failed to stop hotspot: {e:#}"),
                        }
                    });
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|_| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub async fn run() -> Result<()> {
    let initial_active = match HotspotService::new().await {
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
    };

    let (tx, mut rx) = mpsc::channel::<TrayCommand>(8);
    let tray = WireTray {
        active: initial_active,
        tx: Arc::new(tx),
    };
    let handle = tray.spawn().await.context("Failed to create tray icon")?;

    tracing::info!("Tray running");

    while let Some(cmd) = rx.recv().await {
        match cmd {
            TrayCommand::SetActive(active) => {
                handle.update(|t: &mut WireTray| t.active = active).await;
            }
        }
    }

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

    let config = load_hotspot_config(interface);

    let active = hotspot.start(config).await?;
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
    tracing::info!(stopped = stopped.is_some(), "Hotspot stopped");
    Ok(())
}

fn load_hotspot_config(interface: String) -> HotspotConfig {
    match SettingsService::new().load() {
        Ok(s) => HotspotConfig {
            interface,
            ssid: if s.ssid.is_empty() {
                system_hostname()
            } else {
                s.ssid
            },
            passphrase: if s.passphrase.is_empty() {
                None
            } else {
                Some(s.passphrase)
            },
        },
        Err(e) => {
            tracing::warn!("Failed to load settings: {e:#}");
            HotspotConfig {
                interface,
                ssid: system_hostname(),
                passphrase: None,
            }
        }
    }
}

fn system_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .or_else(|_| std::fs::read_to_string("/proc/sys/kernel/hostname"))
        .map(|s| s.trim().to_string())
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Wiretray".to_string())
}

fn load_icon(png_data: &[u8]) -> ksni::Icon {
    let img = image::load_from_memory(png_data)
        .expect("valid PNG data")
        .into_rgba8();
    let (width, height) = img.dimensions();
    // StatusNotifierItem expects ARGB32 (big-endian 0xAARRGGBB)
    let argb: Vec<u8> = img
        .chunks_exact(4)
        .flat_map(|px| {
            let [r, g, b, a] = px else { unreachable!() };
            [*a, *r, *g, *b]
        })
        .collect();
    ksni::Icon {
        width: width as i32,
        height: height as i32,
        data: argb,
    }
}
