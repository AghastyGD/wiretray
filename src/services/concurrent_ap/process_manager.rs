use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail};

use super::{command_runner::CommandRunner, runtime_state::RuntimePaths};

const STOP_RETRIES: usize = 20;
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServicePids {
    pub hostapd: u32,
    pub dnsmasq: u32,
}

pub async fn start_services<R>(runner: &R, paths: &RuntimePaths) -> Result<ServicePids>
where
    R: CommandRunner,
{
    ensure_pid_file_absent(&paths.hostapd_pid)?;
    ensure_pid_file_absent(&paths.dnsmasq_pid)?;

    let hostapd_config = path_argument(&paths.hostapd_config, "hostapd configuration")?;
    let hostapd_pid_file = path_argument(&paths.hostapd_pid, "hostapd PID file")?;
    let hostapd_log = path_argument(&paths.hostapd_log, "hostapd log")?;

    if let Err(error) = runner
        .run(
            "hostapd",
            &[
                "-B",
                "-P",
                hostapd_pid_file,
                "-f",
                hostapd_log,
                hostapd_config,
            ],
        )
        .await
    {
        let details = fs::read_to_string(&paths.hostapd_log)
            .ok()
            .map(|log| log.trim().to_string())
            .filter(|log| !log.is_empty());

        return match details {
            Some(details) => {
                Err(error).context(format!("Failed to start hostapd. hostapd log:\n{details}"))
            }
            None => Err(error).context("Failed to start hostapd (no diagnostic log was written)"),
        };
    }

    let hostapd_pid = read_pid(&paths.hostapd_pid).context("Failed to track hostapd PID")?;
    let dnsmasq_config = path_argument(&paths.dnsmasq_config, "dnsmasq configuration")?;
    let dnsmasq_argument = format!("--conf-file={dnsmasq_config}");

    let dnsmasq_result = runner.run("dnsmasq", &[&dnsmasq_argument]).await;
    let dnsmasq_pid = match dnsmasq_result {
        Ok(_) => match read_pid(&paths.dnsmasq_pid) {
            Ok(pid) => pid,
            Err(error) => {
                return rollback_hostapd(
                    runner,
                    paths,
                    hostapd_pid,
                    error.context("Failed to track dnsmasq PID"),
                )
                .await;
            }
        },
        Err(error) => {
            return rollback_hostapd(
                runner,
                paths,
                hostapd_pid,
                error.context("Failed to start dnsmasq"),
            )
            .await;
        }
    };

    Ok(ServicePids {
        hostapd: hostapd_pid,
        dnsmasq: dnsmasq_pid,
    })
}

async fn rollback_hostapd<R, T>(
    runner: &R,
    paths: &RuntimePaths,
    hostapd_pid: u32,
    startup_error: anyhow::Error,
) -> Result<T>
where
    R: CommandRunner,
{
    let cleanup_result = stop_pid(runner, hostapd_pid).await;
    let remove_result = remove_pid_file(&paths.hostapd_pid);

    match (cleanup_result, remove_result) {
        (Ok(()), Ok(())) => Err(startup_error.context("hostapd was stopped during rollback")),
        (cleanup, remove) => bail!(
            "{startup_error:#}; hostapd rollback failed: stop result: {}; PID file removal result: {}",
            describe_result(cleanup),
            describe_result(remove)
        ),
    }
}

async fn stop_pid<R>(runner: &R, pid: u32) -> Result<()>
where
    R: CommandRunner,
{
    let pid = pid.to_string();
    runner
        .run("kill", &["-TERM", &pid])
        .await
        .with_context(|| format!("Failed to stop process {pid}"))?;

    Ok(())
}

pub(super) async fn stop_tracked_process<R>(
    runner: &R,
    expected_name: &str,
    expected_config: &Path,
    pid: u32,
    pid_file: &Path,
) -> Result<()>
where
    R: CommandRunner,
{
    let process_name = match fs::read_to_string(format!("/proc/{pid}/comm")) {
        Ok(name) => Some(name.trim().to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(error).with_context(|| format!("Failed to inspect process {pid}"));
        }
    };

    if let Some(process_name) = process_name {
        if process_name != expected_name {
            bail!("Refusing to stop PID {pid}: expected {expected_name}, found {process_name}");
        }

        let command_line = fs::read(format!("/proc/{pid}/cmdline"))
            .with_context(|| format!("Failed to inspect command line for process {pid}"))?;
        let expected_config = expected_config
            .to_str()
            .context("Tracked process configuration path is not valid UTF-8")?;
        let owns_config = command_line
            .split(|byte| *byte == 0)
            .filter_map(|argument| std::str::from_utf8(argument).ok())
            .any(|argument| {
                argument == expected_config
                    || argument
                        .strip_prefix("--conf-file=")
                        .is_some_and(|path| path == expected_config)
            });

        if !owns_config {
            bail!(
                "Refusing to stop PID {pid}: its command line does not reference Wiretray's {expected_name} configuration"
            );
        }

        stop_pid(runner, pid).await?;
        wait_for_process_exit(pid, expected_name).await?;
    }

    remove_pid_file(pid_file)
}

async fn wait_for_process_exit(pid: u32, expected_name: &str) -> Result<()> {
    for _ in 0..STOP_RETRIES {
        match fs::read_to_string(format!("/proc/{pid}/comm")) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Ok(name) if name.trim() != expected_name => return Ok(()),
            Ok(_) => tokio::time::sleep(STOP_POLL_INTERVAL).await,
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed while waiting for process {pid} to stop"));
            }
        }
    }

    bail!("Process {pid} did not stop after receiving SIGTERM")
}

fn read_pid(path: &Path) -> Result<u32> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read PID file {}", path.display()))?;
    let pid = contents
        .trim()
        .parse::<u32>()
        .with_context(|| format!("Invalid PID in {}", path.display()))?;

    if pid == 0 {
        bail!("Invalid PID 0 in {}", path.display());
    }

    Ok(pid)
}

fn remove_pid_file(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to remove PID file {}", path.display()))
        }
    }
}

fn ensure_pid_file_absent(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("Failed to inspect PID file {}", path.display()))
        }
        Ok(_) => bail!(
            "Refusing to start with an existing PID file {}; stale state must be cleaned first",
            path.display()
        ),
    }
}

fn path_argument<'a>(path: &'a Path, label: &str) -> Result<&'a str> {
    path.to_str()
        .with_context(|| format!("{label} path is not valid UTF-8"))
}

fn describe_result(result: Result<()>) -> String {
    match result {
        Ok(()) => "ok".to_string(),
        Err(error) => format!("{error:#}"),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        os::unix::process::ExitStatusExt,
        process::ExitStatus,
        sync::Mutex,
        time::{SystemTime, UNIX_EPOCH},
    };

    use anyhow::Result;

    use super::*;
    use crate::services::concurrent_ap::command_runner::CommandOutput;

    struct DaemonRunner {
        paths: RuntimePaths,
        fail_hostapd: bool,
        fail_dnsmasq: bool,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl DaemonRunner {
        fn new(paths: RuntimePaths, fail_dnsmasq: bool) -> Self {
            Self {
                paths,
                fail_hostapd: false,
                fail_dnsmasq,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for DaemonRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                command.to_string(),
                args.iter().map(|argument| argument.to_string()).collect(),
            ));

            let failed = (command == "hostapd" && self.fail_hostapd)
                || (command == "dnsmasq" && self.fail_dnsmasq);
            if command == "hostapd" && self.fail_hostapd {
                fs::write(
                    &self.paths.hostapd_log,
                    "nl80211: Could not configure driver mode\n",
                )?;
            } else if command == "hostapd" {
                fs::write(&self.paths.hostapd_pid, "1101\n")?;
            } else if command == "dnsmasq" && !failed {
                fs::write(&self.paths.dnsmasq_pid, "1102\n")?;
            }

            Ok(CommandOutput {
                status: ExitStatus::from_raw(i32::from(failed) << 8),
                stdout: String::new(),
                stderr: if failed {
                    "dnsmasq failed".to_string()
                } else {
                    String::new()
                },
            })
        }
    }

    #[tokio::test]
    async fn starts_services_and_tracks_their_pids() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        let runner = DaemonRunner::new(paths.clone(), false);

        let pids = start_services(&runner, &paths).await.unwrap();

        assert_eq!(
            pids,
            ServicePids {
                hostapd: 1101,
                dnsmasq: 1102,
            }
        );
        assert_eq!(runner.calls().len(), 2);

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn includes_hostapd_log_when_startup_fails() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        let mut runner = DaemonRunner::new(paths.clone(), false);
        runner.fail_hostapd = true;

        let error = start_services(&runner, &paths)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("nl80211: Could not configure driver mode"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn stops_only_started_hostapd_when_dnsmasq_fails() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        let runner = DaemonRunner::new(paths.clone(), true);

        let error = start_services(&runner, &paths)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("hostapd was stopped during rollback"));
        assert_eq!(
            runner.calls().last(),
            Some(&("kill".into(), vec!["-TERM".to_string(), "1101".to_string()]))
        );

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn refuses_to_overwrite_an_existing_pid_file() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        fs::write(&paths.hostapd_pid, "999\n").unwrap();
        let runner = DaemonRunner::new(paths.clone(), false);

        let error = start_services(&runner, &paths)
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("stale state must be cleaned first"));
        assert!(runner.calls().is_empty());

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn rejects_invalid_pid_files() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        fs::write(&paths.hostapd_pid, "not-a-pid\n").unwrap();

        assert!(read_pid(&paths.hostapd_pid).is_err());

        fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn refuses_to_stop_a_reused_pid() {
        let (directory, paths) = test_paths();
        paths.ensure_directory().unwrap();
        fs::write(&paths.hostapd_pid, std::process::id().to_string()).unwrap();
        let runner = DaemonRunner::new(paths.clone(), false);

        let error = stop_tracked_process(
            &runner,
            "hostapd",
            &paths.hostapd_config,
            std::process::id(),
            &paths.hostapd_pid,
        )
        .await
        .unwrap_err()
        .to_string();

        assert!(error.contains("Refusing to stop PID"));
        assert!(runner.calls().is_empty());

        fs::remove_dir_all(directory).unwrap();
    }

    fn test_paths() -> (std::path::PathBuf, RuntimePaths) {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "wiretray-process-test-{}-{unique}",
            std::process::id()
        ));
        let paths = RuntimePaths::from_directory(&directory);

        (directory, paths)
    }
}
