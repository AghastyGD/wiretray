use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

pub struct TrayService;

impl TrayService {
    pub fn start() -> Result<()> {
        let binary = resolve_wiretray_binary();
        tracing::info!("Starting Wiretray tray from {}", binary.display());

        Command::new(&binary)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("Failed to start Wiretray from {}", binary.display()))?;

        Ok(())
    }
}

fn resolve_wiretray_binary() -> PathBuf {
    if let Ok(current_exe) = std::env::current_exe() {
        let sibling = current_exe.with_file_name("wiretray");

        if sibling.exists() {
            return sibling;
        }
    }

    PathBuf::from("wiretray")
}
