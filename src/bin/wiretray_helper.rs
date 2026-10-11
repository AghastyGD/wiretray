use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use wiretray::services::concurrent_ap::{
    ConcurrentApBackend,
    command_runner::SystemCommandRunner,
    helper_protocol::{HelperRequest, HelperResponse},
    runtime_state::RuntimePaths,
};

const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[tokio::main]
async fn main() {
    let response = match run().await {
        Ok(response) => response,
        Err(error) => HelperResponse::failure(error),
    };

    if let Err(error) = serde_json::to_writer(std::io::stdout(), &response) {
        let _ = writeln!(
            std::io::stderr(),
            "Failed to encode helper response: {error}"
        );
        std::process::exit(1);
    }
}

async fn run() -> Result<HelperResponse> {
    ensure_running_as_root()?;
    let caller_uid = caller_uid()?;
    let request = read_request()?;
    let runtime_directory = match &request {
        HelperRequest::Start {
            runtime_directory, ..
        }
        | HelperRequest::Stop { runtime_directory } => runtime_directory,
    };

    prepare_runtime_directory(runtime_directory, caller_uid)?;
    let paths = RuntimePaths::from_directory(runtime_directory);
    let backend = ConcurrentApBackend::with_runner(SystemCommandRunner, paths);

    match request {
        HelperRequest::Start { hotspot, .. } => {
            let state = backend.start(&hotspot).await?;
            Ok(HelperResponse::success(Some(state)))
        }
        HelperRequest::Stop { .. } => {
            backend.cleanup().await?;
            Ok(HelperResponse::success(None))
        }
    }
}

fn read_request() -> Result<HelperRequest> {
    let mut payload = Vec::new();
    std::io::stdin()
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut payload)
        .context("Failed to read helper request")?;
    if payload.len() as u64 > MAX_REQUEST_BYTES {
        bail!("Helper request exceeds {MAX_REQUEST_BYTES} bytes");
    }

    serde_json::from_slice(&payload).context("Invalid helper request")
}

fn ensure_running_as_root() -> Result<()> {
    // SAFETY: geteuid has no preconditions and does not retain pointers.
    if unsafe { libc::geteuid() } != 0 {
        bail!("wiretray-helper must be launched through pkexec");
    }
    Ok(())
}

fn caller_uid() -> Result<u32> {
    let value = std::env::var("PKEXEC_UID").context("pkexec did not provide PKEXEC_UID")?;
    let uid = value
        .parse::<u32>()
        .context("PKEXEC_UID is not a valid UID")?;
    if uid == 0 {
        bail!("wiretray-helper refuses requests originating from UID 0");
    }
    Ok(uid)
}

fn prepare_runtime_directory(path: &Path, caller_uid: u32) -> Result<()> {
    let expected = PathBuf::from(format!("/run/wiretray/{caller_uid}"));
    if path != expected {
        bail!("Unexpected Wiretray runtime directory {}", path.display());
    }

    ensure_root_directory(Path::new("/run/wiretray"), 0o711)?;
    ensure_root_directory(path, 0o711)?;

    Ok(())
}

fn ensure_root_directory(path: &Path, mode: u32) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.is_dir() || metadata.file_type().is_symlink() || metadata.uid() != 0 {
                bail!("Unsafe privileged runtime directory {}", path.display());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path).with_context(|| {
                format!(
                    "Failed to create privileged runtime directory {}",
                    path.display()
                )
            })?;
        }
        Err(error) => return Err(error.into()),
    }

    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
