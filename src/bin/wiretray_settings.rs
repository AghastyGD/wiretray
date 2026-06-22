use anyhow::Result;
use gtk::Application;
use gtk::prelude::*;
use tokio::runtime::Runtime;

use wiretray::settings::tray_service::TrayService;
use wiretray::ui::settings_window;

const APP_ID: &str = "io.github.AghastyGD.Wiretray";

fn main() -> Result<()> {
    setup_logging();

    if let Err(err) = TrayService::start() {
        tracing::warn!("Failed to start Wiretray tray: {err:#}")
    }

    let runtime = Runtime::new()?;

    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(move |app| {
        settings_window::present(app, runtime.handle().clone());
    });

    app.run();

    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
