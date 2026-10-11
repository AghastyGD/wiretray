pub mod command_runner;
pub mod runtime_state;

use self::{command_runner::SystemCommandRunner, runtime_state::RuntimePaths};

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
