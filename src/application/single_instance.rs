use std::fs::{File, OpenOptions};
use std::path::PathBuf;

use anyhow::{Context, Result};
use fs2::FileExt;

pub fn acquire_lock() -> Result<File> {
    let lock_path = lock_file_path().context("Failed to resolve Wiretray lock path")?;

    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create lock directory at {}", parent.display()))?;
    }

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("Failed to open lock file at {}", lock_path.display()))?;

    tracing::info!("Using Wiretray lock file at {}", lock_path.display());

    file.try_lock_exclusive()
        .context("Wiretray is already running")?;

    Ok(file)
}

fn lock_file_path() -> Result<PathBuf> {
    if let Some(runtime_dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return Ok(PathBuf::from(runtime_dir).join("wiretray/wiretray.lock"));
    }

    let temp_dir = std::env::temp_dir();
    Ok(temp_dir.join("wiretray/wiretray.lock"))
}
