pub mod command_runner;
pub mod runtime_state;

use anyhow::{Result, bail};

use self::{
    command_runner::{CommandRunner, SystemCommandRunner},
    runtime_state::RuntimePaths,
};

const REQUIRED_COMMANDS: [&str; 5] = ["iw", "ip", "hostapd", "dnsmasq", "iptables"];

/// Skeleton for the concurrent AP + client backend.
///
/// This backend is intentionally not connected to `HotspotService` yet. The
/// existing NetworkManager start/stop behavior remains the only active path.
pub struct ConcurrentApBackend<R = SystemCommandRunner> {
    command_runner: R,
    runtime_paths: RuntimePaths,
}

impl ConcurrentApBackend<SystemCommandRunner> {
    pub fn new() -> Self {
        Self::with_runner(SystemCommandRunner, RuntimePaths::discover())
    }
}

impl Default for ConcurrentApBackend<SystemCommandRunner> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R> ConcurrentApBackend<R> {
    pub fn with_runner(command_runner: R, runtime_paths: RuntimePaths) -> Self {
        Self {
            command_runner,
            runtime_paths,
        }
    }

    pub fn runtime_paths(&self) -> &RuntimePaths {
        &self.runtime_paths
    }

    pub fn command_runner(&self) -> &R {
        &self.command_runner
    }
}

impl<R> ConcurrentApBackend<R>
where
    R: CommandRunner,
{
    /// Checks only the command-line dependencies needed by the future backend.
    /// Adapter and upstream-interface checks belong to later startup stages.
    pub fn check_dependencies(&self) -> Result<()> {
        let missing = REQUIRED_COMMANDS
            .iter()
            .copied()
            .filter(|command| !self.command_runner.command_exists(command))
            .collect::<Vec<_>>();

        if !missing.is_empty() {
            bail!(
                "Concurrent hotspot mode is unavailable: missing required tools: {}",
                missing.join(", ")
            );
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use anyhow::Result;

    use super::*;
    use crate::services::concurrent_ap::command_runner::CommandOutput;

    struct FakeCommandRunner {
        available: HashSet<&'static str>,
    }

    impl CommandRunner for FakeCommandRunner {
        fn command_exists(&self, command: &str) -> bool {
            self.available.contains(command)
        }

        async fn run(&self, _command: &str, _args: &[&str]) -> Result<CommandOutput> {
            unreachable!("dependency checks must not execute commands")
        }
    }

    #[test]
    fn dependency_check_succeeds_when_all_tools_exist() {
        let runner = FakeCommandRunner {
            available: REQUIRED_COMMANDS.into_iter().collect(),
        };
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        assert!(backend.check_dependencies().is_ok());
    }

    #[test]
    fn dependency_check_lists_every_missing_tool() {
        let runner = FakeCommandRunner {
            available: HashSet::from(["ip", "iptables"]),
        };
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend.check_dependencies().unwrap_err().to_string();

        assert_eq!(
            error,
            "Concurrent hotspot mode is unavailable: missing required tools: iw, hostapd, dnsmasq"
        );
    }
}
