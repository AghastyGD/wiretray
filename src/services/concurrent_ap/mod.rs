pub mod cleanup;
pub mod command_runner;
pub mod firewall;
pub mod process_manager;
pub mod runtime_state;

use anyhow::{Context, Result, bail};

use self::{
    command_runner::{CommandRunner, SystemCommandRunner},
    firewall::FirewallState,
    process_manager::ServicePids,
    runtime_state::RuntimePaths,
};
use crate::models::hotspot::HotspotConfig;

const REQUIRED_COMMANDS: [&str; 6] = ["iw", "ip", "hostapd", "dnsmasq", "iptables", "sysctl"];
pub const DEFAULT_AP_INTERFACE: &str = "wiretray-ap0";
pub const DEFAULT_AP_ADDRESS: &str = "192.168.50.1/24";

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

    /// Finds the interface used by the current IPv4 default route.
    pub async fn detect_upstream_interface(&self) -> Result<String> {
        let output = self
            .command_runner
            .run("ip", &["route", "show", "default"])
            .await
            .context("Failed to inspect the default route")?;

        parse_default_route_interface(&output.stdout)
    }

    /// Creates and configures Wiretray's virtual AP interface.
    ///
    /// The parent interface is the Wi-Fi device selected for AP + client mode;
    /// it is independent from the default-route interface used for NAT.
    pub async fn create_ap_interface(&self, parent_interface: &str) -> Result<()> {
        validate_interface_name(parent_interface, "Parent Wi-Fi interface")?;

        if self.interface_exists(DEFAULT_AP_INTERFACE).await? {
            bail!(
                "Cannot create {DEFAULT_AP_INTERFACE}: the interface already exists and was not created by this startup"
            );
        }

        self.command_runner
            .run(
                "iw",
                &[
                    "dev",
                    parent_interface,
                    "interface",
                    "add",
                    DEFAULT_AP_INTERFACE,
                    "type",
                    "__ap",
                ],
            )
            .await
            .with_context(|| {
                format!(
                    "Failed to create AP interface {DEFAULT_AP_INTERFACE} from {parent_interface}"
                )
            })?;

        if let Err(error) = self.configure_ap_interface().await {
            return match self.delete_ap_interface().await {
                Ok(()) => Err(error.context(format!(
                    "Failed to configure {DEFAULT_AP_INTERFACE}; the interface was rolled back"
                ))),
                Err(cleanup_error) => Err(anyhow::anyhow!(
                    "Failed to configure {DEFAULT_AP_INTERFACE}: {error:#}; rollback also failed: {cleanup_error:#}"
                )),
            };
        }

        Ok(())
    }

    /// Writes hostapd and dnsmasq configuration without starting either daemon.
    pub fn write_service_configs(&self, hotspot: &HotspotConfig) -> Result<()> {
        let hostapd_config = build_hostapd_config(hotspot)?;
        let dnsmasq_config = build_dnsmasq_config(&self.runtime_paths)?;

        self.runtime_paths
            .write_secure_file(
                &self.runtime_paths.hostapd_config,
                hostapd_config.as_bytes(),
            )
            .context("Failed to write hostapd configuration")?;

        if let Err(error) = self.runtime_paths.write_secure_file(
            &self.runtime_paths.dnsmasq_config,
            dnsmasq_config.as_bytes(),
        ) {
            return match std::fs::remove_file(&self.runtime_paths.hostapd_config) {
                Ok(()) => Err(error.context(
                    "Failed to write dnsmasq configuration; hostapd configuration was removed",
                )),
                Err(cleanup_error) => Err(anyhow::anyhow!(
                    "Failed to write dnsmasq configuration: {error:#}; failed to remove hostapd configuration: {cleanup_error}"
                )),
            };
        }

        Ok(())
    }

    /// Starts only the hostapd and dnsmasq instances configured by Wiretray.
    pub async fn start_services(&self) -> Result<ServicePids> {
        process_manager::start_services(&self.command_runner, &self.runtime_paths).await
    }

    /// Enables IPv4 forwarding and applies only Wiretray-owned firewall rules.
    pub async fn enable_network_sharing(&self, upstream_interface: &str) -> Result<FirewallState> {
        validate_interface_name(upstream_interface, "Upstream interface")?;
        firewall::enable_network_sharing(&self.command_runner, upstream_interface).await
    }

    /// Cleans resources recorded in state.json. Returns false when no state exists.
    pub async fn cleanup(&self) -> Result<bool> {
        cleanup::cleanup(&self.command_runner, &self.runtime_paths).await
    }

    async fn interface_exists(&self, interface: &str) -> Result<bool> {
        let output = self
            .command_runner
            .output("ip", &["link", "show", "dev", interface])
            .await
            .with_context(|| format!("Failed to check whether interface {interface} exists"))?;

        match output.status.code() {
            Some(0) => Ok(true),
            Some(1) => Ok(false),
            _ => bail!(
                "Failed to check whether interface {interface} exists: {}",
                output.stderr.trim()
            ),
        }
    }

    async fn configure_ap_interface(&self) -> Result<()> {
        self.command_runner
            .run(
                "ip",
                &[
                    "address",
                    "add",
                    DEFAULT_AP_ADDRESS,
                    "dev",
                    DEFAULT_AP_INTERFACE,
                ],
            )
            .await
            .context("Failed to assign the AP address")?;

        self.command_runner
            .run("ip", &["link", "set", "dev", DEFAULT_AP_INTERFACE, "up"])
            .await
            .context("Failed to bring the AP interface up")?;

        Ok(())
    }

    async fn delete_ap_interface(&self) -> Result<()> {
        self.command_runner
            .run("iw", &["dev", DEFAULT_AP_INTERFACE, "del"])
            .await
            .context("Failed to delete the AP interface during rollback")?;

        Ok(())
    }
}

fn build_hostapd_config(hotspot: &HotspotConfig) -> Result<String> {
    if hotspot.ssid.is_empty() || hotspot.ssid.len() > 32 {
        bail!("Hotspot SSID must be between 1 and 32 bytes");
    }

    let mut config = format!(
        "interface={DEFAULT_AP_INTERFACE}\n\
         driver=nl80211\n\
         ssid2={}\n\
         hw_mode=g\n\
         channel=1\n\
         auth_algs=1\n",
        encode_hex(hotspot.ssid.as_bytes())
    );

    if let Some(passphrase) = &hotspot.passphrase {
        validate_hostapd_passphrase(passphrase)?;

        config.push_str("wpa=2\n");
        if passphrase.len() == 64 && passphrase.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            config.push_str(&format!("wpa_psk={passphrase}\n"));
        } else {
            config.push_str(&format!("wpa_passphrase={passphrase}\n"));
        }
        config.push_str("wpa_key_mgmt=WPA-PSK\nrsn_pairwise=CCMP\n");
    }

    Ok(config)
}

fn validate_hostapd_passphrase(passphrase: &str) -> Result<()> {
    let is_hex_psk =
        passphrase.len() == 64 && passphrase.bytes().all(|byte| byte.is_ascii_hexdigit());
    let is_printable_ascii_passphrase = (8..=63).contains(&passphrase.len())
        && passphrase.bytes().all(|byte| (0x20..=0x7e).contains(&byte));

    if !is_hex_psk && !is_printable_ascii_passphrase {
        bail!(
            "Concurrent hotspot passphrase must be 8-63 printable ASCII characters or a 64-character hexadecimal PSK"
        );
    }

    Ok(())
}

fn build_dnsmasq_config(paths: &RuntimePaths) -> Result<String> {
    let leases_path = config_path_value(&paths.dnsmasq_leases, "dnsmasq lease file")?;
    let pid_path = config_path_value(&paths.dnsmasq_pid, "dnsmasq PID file")?;

    Ok(format!(
        "interface={DEFAULT_AP_INTERFACE}\n\
         bind-interfaces\n\
         listen-address=192.168.50.1\n\
         dhcp-range=192.168.50.10,192.168.50.100,12h\n\
         dhcp-option=3,192.168.50.1\n\
         dhcp-option=6,8.8.8.8,1.1.1.1\n\
         dhcp-leasefile={leases_path}\n\
         pid-file={pid_path}\n"
    ))
}

fn config_path_value<'a>(path: &'a std::path::Path, label: &str) -> Result<&'a str> {
    let value = path
        .to_str()
        .with_context(|| format!("{label} path is not valid UTF-8"))?;

    if value.bytes().any(|byte| matches!(byte, b'\n' | b'\r')) {
        bail!("{label} path contains invalid characters");
    }

    Ok(value)
}

fn encode_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;

    bytes.iter().fold(
        String::with_capacity(bytes.len() * 2),
        |mut encoded, byte| {
            write!(encoded, "{byte:02x}").expect("writing to a String cannot fail");
            encoded
        },
    )
}

pub(super) fn validate_interface_name(interface: &str, label: &str) -> Result<()> {
    if interface.is_empty() || interface.len() > 15 {
        bail!("{label} must contain between 1 and 15 bytes");
    }

    if interface.starts_with('-') || interface.bytes().any(|byte| byte.is_ascii_whitespace()) {
        bail!("{label} contains invalid characters");
    }

    Ok(())
}

fn parse_default_route_interface(output: &str) -> Result<String> {
    output
        .lines()
        .filter_map(|line| {
            let fields = line.split_whitespace().collect::<Vec<_>>();

            if fields.first() != Some(&"default") {
                return None;
            }

            fields
                .windows(2)
                .find(|pair| pair[0] == "dev")
                .map(|pair| pair[1].to_string())
        })
        .next()
        .ok_or_else(|| {
            anyhow::anyhow!("No active default route with an upstream interface was found")
        })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashSet, VecDeque},
        os::unix::process::ExitStatusExt,
        process::ExitStatus,
        sync::Mutex,
    };

    use anyhow::Result;

    use super::*;
    use crate::services::concurrent_ap::command_runner::CommandOutput;

    struct FakeCommandRunner {
        available: HashSet<&'static str>,
    }

    struct RouteCommandRunner;

    struct ScriptedCommandRunner {
        outputs: Mutex<VecDeque<CommandOutput>>,
        calls: Mutex<Vec<(String, Vec<String>)>>,
    }

    impl ScriptedCommandRunner {
        fn with_exit_codes(exit_codes: impl IntoIterator<Item = i32>) -> Self {
            let outputs = exit_codes
                .into_iter()
                .map(|code| CommandOutput {
                    status: ExitStatus::from_raw(code << 8),
                    stdout: String::new(),
                    stderr: if code != 0 {
                        "scripted failure".to_string()
                    } else {
                        String::new()
                    },
                })
                .collect();

            Self {
                outputs: Mutex::new(outputs),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<(String, Vec<String>)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn command_exists(&self, command: &str) -> bool {
            self.available.contains(command)
        }

        async fn output(&self, _command: &str, _args: &[&str]) -> Result<CommandOutput> {
            unreachable!("dependency checks must not execute commands")
        }
    }

    impl CommandRunner for RouteCommandRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            assert_eq!(command, "ip");
            assert_eq!(args, ["route", "show", "default"]);

            Ok(CommandOutput {
                status: ExitStatus::from_raw(0),
                stdout: "default via 10.0.0.1 dev enp2s0 proto dhcp metric 100\n".into(),
                stderr: String::new(),
            })
        }
    }

    impl CommandRunner for ScriptedCommandRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                command.to_string(),
                args.iter().map(|argument| argument.to_string()).collect(),
            ));

            Ok(self
                .outputs
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected command execution"))
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
            available: HashSet::from(["ip", "iptables", "sysctl"]),
        };
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend.check_dependencies().unwrap_err().to_string();

        assert_eq!(
            error,
            "Concurrent hotspot mode is unavailable: missing required tools: iw, hostapd, dnsmasq"
        );
    }

    #[tokio::test]
    async fn detects_upstream_using_ip_route() {
        let backend =
            ConcurrentApBackend::with_runner(RouteCommandRunner, RuntimePaths::discover());

        assert_eq!(backend.detect_upstream_interface().await.unwrap(), "enp2s0");
    }

    #[tokio::test]
    async fn creates_and_configures_ap_interface_in_order() {
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 0, 0]);
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        backend.create_ap_interface("wlp3s0").await.unwrap();

        assert_eq!(
            backend.command_runner().calls(),
            vec![
                (
                    "ip".into(),
                    vec!["link", "show", "dev", "wiretray-ap0"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                (
                    "iw".into(),
                    vec![
                        "dev",
                        "wlp3s0",
                        "interface",
                        "add",
                        "wiretray-ap0",
                        "type",
                        "__ap",
                    ]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                ),
                (
                    "ip".into(),
                    vec!["address", "add", "192.168.50.1/24", "dev", "wiretray-ap0"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
                (
                    "ip".into(),
                    vec!["link", "set", "dev", "wiretray-ap0", "up"]
                        .into_iter()
                        .map(String::from)
                        .collect(),
                ),
            ]
        );
    }

    #[tokio::test]
    async fn deletes_created_interface_when_configuration_fails() {
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 1, 0]);
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend
            .create_ap_interface("wlp3s0")
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("the interface was rolled back"));
        assert_eq!(
            backend.command_runner().calls().last(),
            Some(&(
                "iw".into(),
                vec!["dev", "wiretray-ap0", "del"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
            ))
        );
    }

    #[tokio::test]
    async fn reports_when_configuration_and_rollback_both_fail() {
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 1, 1]);
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend
            .create_ap_interface("wlp3s0")
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("rollback also failed"));
    }

    #[tokio::test]
    async fn refuses_to_replace_an_existing_ap_interface() {
        let runner = ScriptedCommandRunner::with_exit_codes([0]);
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend
            .create_ap_interface("wlp3s0")
            .await
            .unwrap_err()
            .to_string();

        assert!(error.contains("interface already exists"));
        assert_eq!(backend.command_runner().calls().len(), 1);
    }

    #[test]
    fn builds_wpa2_hostapd_config_with_encoded_ssid() {
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray-Test".into(),
            passphrase: Some("12345678".into()),
        };

        let config = build_hostapd_config(&hotspot).unwrap();

        assert!(config.contains("interface=wiretray-ap0\n"));
        assert!(config.contains("ssid2=57697265747261792d54657374\n"));
        assert!(config.contains("wpa_passphrase=12345678\n"));
        assert!(config.contains("wpa_key_mgmt=WPA-PSK\n"));
    }

    #[test]
    fn uses_precomputed_hex_psk_without_exposing_it_as_a_passphrase() {
        let psk = "a".repeat(64);
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: Some(psk.clone()),
        };

        let config = build_hostapd_config(&hotspot).unwrap();

        assert!(config.contains(&format!("wpa_psk={psk}\n")));
        assert!(!config.contains("wpa_passphrase="));
    }

    #[test]
    fn builds_open_hostapd_config_without_wpa_options() {
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: None,
        };

        let config = build_hostapd_config(&hotspot).unwrap();

        assert!(!config.contains("wpa="));
        assert!(!config.contains("wpa_passphrase="));
        assert!(!config.contains("wpa_psk="));
    }

    #[test]
    fn rejects_hostapd_config_injection_through_passphrase() {
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: Some("password\nwpa=0".into()),
        };

        let error = build_hostapd_config(&hotspot).unwrap_err().to_string();

        assert!(error.contains("printable ASCII"));
    }

    #[test]
    fn builds_dnsmasq_config_for_wiretray_subnet() {
        let paths = RuntimePaths::from_directory("/run/user/1000/wiretray");

        let config = build_dnsmasq_config(&paths).unwrap();

        assert!(config.contains("interface=wiretray-ap0\n"));
        assert!(config.contains("listen-address=192.168.50.1\n"));
        assert!(config.contains("dhcp-range=192.168.50.10,192.168.50.100,12h\n"));
        assert!(config.contains("dhcp-leasefile=/run/user/1000/wiretray/dnsmasq.leases\n"));
    }

    #[cfg(unix)]
    #[test]
    fn writes_service_configs_with_private_permissions() {
        use std::{
            os::unix::fs::PermissionsExt,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "wiretray-config-test-{}-{unique}",
            std::process::id()
        ));
        let paths = RuntimePaths::from_directory(&directory);
        let runner = FakeCommandRunner {
            available: HashSet::new(),
        };
        let backend = ConcurrentApBackend::with_runner(runner, paths.clone());
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: Some("12345678".into()),
        };

        backend.write_service_configs(&hotspot).unwrap();

        assert!(paths.hostapd_config.is_file());
        assert!(paths.dnsmasq_config.is_file());
        assert_eq!(
            std::fs::metadata(&paths.hostapd_config)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(&paths.dnsmasq_config)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );

        std::fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn parses_wifi_default_route() {
        let output = "default via 192.168.1.1 dev wlp3s0 proto dhcp metric 600\n";

        assert_eq!(parse_default_route_interface(output).unwrap(), "wlp3s0");
    }

    #[test]
    fn accepts_ethernet_as_upstream() {
        let output = "default via 10.0.0.1 dev enp2s0 proto dhcp metric 100\n";

        assert_eq!(parse_default_route_interface(output).unwrap(), "enp2s0");
    }

    #[test]
    fn uses_first_valid_default_route() {
        let output = concat!(
            "default via 10.0.0.1 proto dhcp metric 50\n",
            "default via 192.168.1.1 dev wlp3s0 proto dhcp metric 600\n",
            "default via 10.0.0.1 dev enp2s0 proto dhcp metric 100\n",
        );

        assert_eq!(parse_default_route_interface(output).unwrap(), "wlp3s0");
    }

    #[test]
    fn rejects_output_without_a_default_route_interface() {
        let error = parse_default_route_interface("default via 192.168.1.1 proto dhcp\n")
            .unwrap_err()
            .to_string();

        assert_eq!(
            error,
            "No active default route with an upstream interface was found"
        );
    }
}
