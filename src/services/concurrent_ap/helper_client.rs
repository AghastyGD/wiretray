use std::{env, path::PathBuf, process::Stdio};

use anyhow::{Context, Result, bail};
use tokio::{io::AsyncWriteExt, process::Command};

use crate::models::hotspot::HotspotConfig;

use super::{
    helper_protocol::{HelperRequest, HelperResponse},
    runtime_state::{RuntimePaths, RuntimeState},
};

pub async fn start(hotspot: &HotspotConfig, paths: &RuntimePaths) -> Result<RuntimeState> {
    let response = invoke(HelperRequest::Start {
        runtime_directory: paths.directory.clone(),
        hotspot: hotspot.clone(),
    })
    .await?;

    response
        .state
        .context("Privileged helper completed startup without returning runtime state")
}

pub async fn stop(paths: &RuntimePaths) -> Result<bool> {
    if !paths.state.exists() {
        return Ok(false);
    }

    invoke(HelperRequest::Stop {
        runtime_directory: paths.directory.clone(),
    })
    .await?;

    Ok(true)
}

async fn invoke(request: HelperRequest) -> Result<HelperResponse> {
    let helper = resolve_helper_binary()?;
    let payload = serde_json::to_vec(&request).context("Failed to serialize helper request")?;

    let mut child = Command::new("pkexec")
        .arg(&helper)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to start privileged helper {}", helper.display()))?;

    child
        .stdin
        .take()
        .context("Failed to open privileged helper input")?
        .write_all(&payload)
        .await
        .context("Failed to send request to privileged helper")?;

    let output = child
        .wait_with_output()
        .await
        .context("Failed while waiting for privileged helper")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "Privileged helper exited with status {}: {}",
            output.status,
            stderr.trim()
        );
    }

    let response: HelperResponse = serde_json::from_slice(&output.stdout)
        .context("Privileged helper returned an invalid response")?;
    if let Some(error) = response.error.as_deref() {
        bail!("Privileged helper failed: {error}");
    }

    Ok(response)
}

fn resolve_helper_binary() -> Result<PathBuf> {
    if let Ok(current_exe) = env::current_exe() {
        let sibling = current_exe.with_file_name("wiretray-helper");
        if sibling.is_file() {
            return Ok(sibling);
        }
    }

    let helper = env::var_os("PATH")
        .and_then(|path| {
            env::split_paths(&path)
                .map(|directory| directory.join("wiretray-helper"))
                .find(|candidate| candidate.is_file())
        })
        .context(
            "wiretray-helper was not found; install it or build all binaries with `cargo build --bins`",
        )?;

    helper
        .canonicalize()
        .context("Failed to resolve wiretray-helper path")
}
