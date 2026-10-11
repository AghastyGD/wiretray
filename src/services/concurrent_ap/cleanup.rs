use std::{fs, path::Path};

use anyhow::{Result, bail};

use super::{
    DEFAULT_AP_ADDRESS, DEFAULT_AP_INTERFACE,
    command_runner::CommandRunner,
    firewall::{self, FirewallState, remove_network_sharing},
    process_manager::stop_tracked_process,
    runtime_state::{RuntimePaths, RuntimeState},
    validate_interface_name,
};

pub async fn cleanup<R>(runner: &R, paths: &RuntimePaths) -> Result<bool>
where
    R: CommandRunner,
{
    let Some(state) = RuntimeState::load(&paths.state)? else {
        return Ok(false);
    };

    cleanup_state(runner, paths, &state).await?;
    Ok(true)
}

pub(super) async fn cleanup_state<R>(
    runner: &R,
    paths: &RuntimePaths,
    state: &RuntimeState,
) -> Result<()>
where
    R: CommandRunner,
{
    validate_state(state)?;

    let mut errors = Vec::new();

    if let Some(pid) = state.hostapd_pid
        && let Err(error) = stop_tracked_process(
            runner,
            "hostapd",
            &paths.hostapd_config,
            pid,
            &paths.hostapd_pid,
        )
        .await
    {
        errors.push(format!("failed to stop hostapd PID {pid}: {error:#}"));
    }

    if let Some(pid) = state.dnsmasq_pid
        && let Err(error) = stop_tracked_process(
            runner,
            "dnsmasq",
            &paths.dnsmasq_config,
            pid,
            &paths.dnsmasq_pid,
        )
        .await
    {
        errors.push(format!("failed to stop dnsmasq PID {pid}: {error:#}"));
    }

    let firewall_state = FirewallState {
        ip_forwarding_was_enabled: state.ip_forwarding_was_enabled,
        iptables_rules: state.iptables_rules.clone(),
    };
    if let Err(error) = remove_network_sharing(runner, &firewall_state).await {
        errors.push(format!("failed to restore firewall state: {error:#}"));
    }

    if state.ap_interface_created
        && let Err(error) = remove_ap_interface(runner, state).await
    {
        errors.push(format!("failed to remove AP interface: {error:#}"));
    }

    for path in runtime_artifacts(paths) {
        if let Err(error) = remove_file_if_present(path) {
            errors.push(format!(
                "failed to remove runtime file {}: {error:#}",
                path.display()
            ));
        }
    }

    if errors.is_empty() {
        remove_file_if_present(&paths.state)?;
        remove_directory_if_empty(&paths.directory)?;
        Ok(())
    } else {
        bail!(errors.join("; "))
    }
}

fn validate_state(state: &RuntimeState) -> Result<()> {
    if state.ap_interface != DEFAULT_AP_INTERFACE {
        bail!(
            "Refusing cleanup for unexpected AP interface {}",
            state.ap_interface
        );
    }
    if state.ap_ip != DEFAULT_AP_ADDRESS {
        bail!("Refusing cleanup for unexpected AP address {}", state.ap_ip);
    }

    validate_interface_name(&state.upstream_interface, "Stored upstream interface")?;
    let expected_rules = firewall::sharing_rules(&state.upstream_interface);
    let contains_unexpected_or_duplicate_rule =
        state
            .iptables_rules
            .iter()
            .enumerate()
            .any(|(index, rule)| {
                !expected_rules.contains(rule) || state.iptables_rules[..index].contains(rule)
            });
    if state.iptables_rules.len() > expected_rules.len() || contains_unexpected_or_duplicate_rule {
        bail!("Refusing cleanup because state.json contains unexpected firewall rules");
    }

    Ok(())
}

async fn remove_ap_interface<R>(runner: &R, state: &RuntimeState) -> Result<()>
where
    R: CommandRunner,
{
    let output = runner
        .output("ip", &["link", "show", "dev", state.ap_interface.as_str()])
        .await?;

    match output.status.code() {
        Some(1) => return Ok(()),
        Some(0) => {}
        _ => bail!(
            "Failed to inspect stored AP interface {}: {}",
            state.ap_interface,
            output.stderr.trim()
        ),
    }

    let mut errors = Vec::new();
    if let Err(error) = runner
        .run(
            "ip",
            &[
                "address",
                "del",
                state.ap_ip.as_str(),
                "dev",
                state.ap_interface.as_str(),
            ],
        )
        .await
    {
        errors.push(format!("failed to remove AP address: {error:#}"));
    }

    if let Err(error) = runner
        .run("iw", &["dev", state.ap_interface.as_str(), "del"])
        .await
    {
        errors.push(format!("failed to delete virtual AP interface: {error:#}"));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        bail!(errors.join("; "))
    }
}

fn runtime_artifacts(paths: &RuntimePaths) -> [&Path; 6] {
    [
        &paths.hostapd_config,
        &paths.dnsmasq_config,
        &paths.hostapd_pid,
        &paths.dnsmasq_pid,
        &paths.dnsmasq_leases,
        &paths.state_temporary,
    ]
}

fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn remove_directory_if_empty(path: &Path) -> Result<()> {
    match fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::VecDeque,
        os::unix::process::ExitStatusExt,
        process::ExitStatus,
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;

    use super::*;
    use crate::services::concurrent_ap::{
        command_runner::CommandOutput, runtime_state::IptablesRule,
    };

    struct ScriptedRunner {
        statuses: Mutex<VecDeque<i32>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl ScriptedRunner {
        fn new(statuses: impl IntoIterator<Item = i32>) -> Self {
            Self {
                statuses: Mutex::new(statuses.into_iter().collect()),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for ScriptedRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                command.to_string(),
                args.iter().map(|argument| argument.to_string()).collect(),
            ));
            let status = self
                .statuses
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected command execution");

            Ok(CommandOutput {
                status: ExitStatus::from_raw(status << 8),
                stdout: String::new(),
                stderr: if status == 0 {
                    String::new()
                } else {
                    "scripted failure".into()
                },
            })
        }
    }

    #[tokio::test]
    async fn reports_when_there_is_no_runtime_state() {
        let (directory, paths) = test_paths();
        let runner = ScriptedRunner::new([]);

        assert!(!cleanup(&runner, &paths).await.unwrap());
        assert!(runner.calls().is_empty());
        assert!(!directory.exists());
    }

    #[tokio::test]
    async fn cleans_exact_recorded_resources_and_removes_state_last() {
        let (directory, paths) = test_paths();
        let state = valid_state(firewall::sharing_rules("enp2s0"));
        state.save(&paths).unwrap();
        paths
            .write_secure_file(&paths.hostapd_config, b"hostapd")
            .unwrap();
        paths
            .write_secure_file(&paths.dnsmasq_config, b"dnsmasq")
            .unwrap();
        let runner = ScriptedRunner::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

        assert!(cleanup(&runner, &paths).await.unwrap());

        let calls = runner.calls();
        assert_eq!(
            calls
                .iter()
                .filter(|(command, args)| command == "iptables" && args.contains(&"-D".into()))
                .count(),
            3
        );
        assert!(calls.iter().all(|(_, args)| !args.contains(&"-F".into())));
        assert!(!paths.state.exists());
        assert!(!directory.exists());
    }

    #[tokio::test]
    async fn rejects_tampered_firewall_state_before_running_commands() {
        let (directory, paths) = test_paths();
        let state = valid_state(vec![IptablesRule {
            table: "filter".into(),
            chain: "FORWARD".into(),
            args: vec!["-F".into()],
        }]);
        state.save(&paths).unwrap();
        let runner = ScriptedRunner::new([]);

        let error = cleanup(&runner, &paths).await.unwrap_err().to_string();

        assert!(error.contains("unexpected firewall rules"));
        assert!(runner.calls().is_empty());
        assert!(paths.state.exists());

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn retains_state_when_cleanup_is_incomplete() {
        let (directory, paths) = test_paths();
        let rule = firewall::sharing_rules("enp2s0")
            .into_iter()
            .next()
            .unwrap();
        let state = valid_state(vec![rule]);
        state.save(&paths).unwrap();
        let runner = ScriptedRunner::new([2, 0, 1]);

        assert!(cleanup(&runner, &paths).await.is_err());
        assert!(paths.state.exists());

        fs::remove_dir_all(directory).unwrap();
    }

    fn valid_state(iptables_rules: Vec<IptablesRule>) -> RuntimeState {
        RuntimeState {
            ap_interface: DEFAULT_AP_INTERFACE.into(),
            ap_interface_created: true,
            upstream_interface: "enp2s0".into(),
            ap_ip: DEFAULT_AP_ADDRESS.into(),
            hostapd_pid: None,
            dnsmasq_pid: None,
            ip_forwarding_was_enabled: false,
            iptables_rules,
        }
    }

    fn test_paths() -> (std::path::PathBuf, RuntimePaths) {
        static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "wiretray-cleanup-test-{}-{unique}-{sequence}",
            std::process::id()
        ));
        let paths = RuntimePaths::from_directory(&directory);

        (directory, paths)
    }
}
