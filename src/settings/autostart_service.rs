use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};

const AUTOSTART_FILE_NAME: &str = "io.github.AghastyGD.Wiretray.desktop";

pub struct AutostartService;

impl AutostartService {
    pub fn is_enabled() -> bool {
        autostart_file_path().is_ok_and(|path| path.is_file())
    }

    pub fn enable() -> Result<()> {
        let autostart_dir = autostart_dir().context("Failed to resolve autostart directory")?;

        fs::create_dir_all(&autostart_dir).with_context(|| {
            format!(
                "Failed to create autostart directory at {}",
                autostart_dir.display()
            )
        })?;

        let desktop_file =
            autostart_file_path().context("Failed to resolve autostart desktop file path")?;

        fs::write(&desktop_file, desktop_entry()).with_context(|| {
            format!(
                "Failed to write autostart desktop file at {}",
                desktop_file.display()
            )
        })?;

        Ok(())
    }

    pub fn disable() -> Result<()> {
        let desktop_file =
            autostart_file_path().context("Failed to resolve autostart desktop file path")?;

        if desktop_file.is_file() {
            fs::remove_file(&desktop_file).with_context(|| {
                format!(
                    "Failed to remove autostart desktop file at {}",
                    desktop_file.display()
                )
            })?;
        } else if desktop_file.exists() {
            bail!(
                "Autostart path exists but is not a file: {}",
                desktop_file.display()
            );
        }

        Ok(())
    }
}

fn autostart_file_path() -> Result<PathBuf> {
    Ok(autostart_dir()?.join(AUTOSTART_FILE_NAME))
}

fn autostart_dir() -> Result<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(config_home).join("autostart"));
    }

    let home = std::env::var_os("HOME").context("HOME environment variable is not set")?;

    Ok(PathBuf::from(home).join(".config/autostart"))
}

fn desktop_entry() -> &'static str {
    r#"[Desktop Entry]
Type=Application
Name=Wiretray
Comment=Wi-Fi hotspot manager for Linux
Exec=/usr/bin/wiretray
Icon=io.github.AghastyGD.Wiretray
Terminal=false
StartupNotify=false
Hidden=false
X-GNOME-Autostart-enabled=true
X-GNOME-Autostart-Delay=5
"#
}
