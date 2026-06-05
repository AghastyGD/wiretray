use anyhow::Result;

use wiretray::tray;

fn main() -> Result<()> {
    setup_logging();

    tracing::info!("Starting wiretray...");

    gtk::init()?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    tray::run(rt.handle().clone())?;

    Ok(())
}

fn setup_logging() {
    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();
}
