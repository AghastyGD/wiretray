use std::{env, ffi::OsStr, path::PathBuf, process::ExitStatus};

use anyhow::{Context, Result, bail};
use tokio::process::Command;

#[derive(Debug)]
pub struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Boundary for all external commands used by the concurrent AP backend.
///
/// A privileged implementation can replace this runner later without spreading
/// elevation logic throughout the backend.
#[allow(async_fn_in_trait)]
pub trait CommandRunner {
    fn command_exists(&self, command: &str) -> bool;

    async fn run(&self, command: &str, args: &[&str]) -> Result<CommandOutput>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn command_exists(&self, command: &str) -> bool {
        find_in_path(command).is_some()
    }

    async fn run(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
        let output = Command::new(command)
            .args(args)
            .output()
            .await
            .with_context(|| format!("Failed to execute {command}"))?;

        let result = CommandOutput {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        };

        if !result.status.success() {
            let details = result.stderr.trim();
            if details.is_empty() {
                bail!("Command {command} failed with status {}", result.status);
            }
            bail!(
                "Command {command} failed with status {}: {details}",
                result.status
            );
        }

        Ok(result)
    }
}

fn find_in_path(command: impl AsRef<OsStr>) -> Option<PathBuf> {
    let command = command.as_ref();

    if PathBuf::from(command).components().count() > 1 {
        return is_executable(PathBuf::from(command));
    }

    env::var_os("PATH").and_then(|path| {
        env::split_paths(&path)
            .map(|directory| directory.join(command))
            .find_map(is_executable)
    })
}

#[cfg(unix)]
fn is_executable(path: PathBuf) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .ok()
        .filter(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .map(|_| path)
}

#[cfg(not(unix))]
fn is_executable(path: PathBuf) -> Option<PathBuf> {
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_known_command_in_path() {
        assert!(find_in_path("sh").is_some());
    }

    #[test]
    fn rejects_missing_command() {
        assert!(find_in_path("wiretray-command-that-does-not-exist").is_none());
    }
}
