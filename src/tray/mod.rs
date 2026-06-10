use std::sync::Arc;

use anyhow::{Context, Result};
use ksni::{MenuItem, Tray, TrayMethods, menu::StandardItem};
use tokio::sync::mpsc;

use crate::application::hotspot as hotspot_app;

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
                enabled: !self.active,
                activate: Box::new(move |_tray: &mut Self| {
                    let tx = Arc::clone(&tx_start);
                    tokio::spawn(async move {
                        match hotspot_app::start().await {
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
                enabled: self.active,
                activate: Box::new(move |_tray: &mut Self| {
                    let tx = Arc::clone(&tx_stop);
                    tokio::spawn(async move {
                        match hotspot_app::stop().await {
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
                label: "Settings".into(),
                activate: Box::new(|_: &mut Self| {
                    if let Err(err) = std::process::Command::new("wiretray-settings").spawn() {
                        tracing::error!("Failed to launch settings window: {err}");
                    }
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
    let initial_active = hotspot_app::is_active().await;

    let (tx, mut rx) = mpsc::channel::<TrayCommand>(8);

    let tx_poll = tx.clone();

    tokio::spawn(async move {
        let mut last_state = initial_active;

        loop {
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;

            let active = hotspot_app::is_active().await;

            if active != last_state {
                let _ = tx_poll.send(TrayCommand::SetActive(active)).await;
                last_state = active;
            }
        }
    });

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
