pub mod cleanup;
pub mod command_runner;
pub mod firewall;
pub mod helper_client;
pub mod helper_protocol;
pub mod process_manager;
pub mod runtime_state;

use anyhow::{Context, Result, bail};

use self::{
    command_runner::{CommandRunner, SystemCommandRunner},
    firewall::FirewallState,
    process_manager::ServicePids,
    runtime_state::{RuntimePaths, RuntimeState},
};
use crate::models::hotspot::HotspotConfig;

const REQUIRED_COMMANDS: [&str; 8] = [
    "iw", "ip", "hostapd", "dnsmasq", "iptables", "sysctl", "pkexec", "nmcli",
];
pub const DEFAULT_AP_INTERFACE: &str = "wiretray-ap0";
pub const DEFAULT_AP_ADDRESS: &str = "192.168.50.1/24";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ApRadioConfig {
    hw_mode: char,
    channel: u16,
}

const DEFAULT_RADIO_CONFIG: ApRadioConfig = ApRadioConfig {
    hw_mode: 'g',
    channel: 1,
};

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

impl ConcurrentApBackend<SystemCommandRunner> {
    pub async fn start_privileged(&self, hotspot: &HotspotConfig) -> Result<RuntimeState> {
        self.check_dependencies()?;
        helper_client::start(hotspot, &self.runtime_paths).await
    }

    pub async fn cleanup_privileged(&self) -> Result<bool> {
        helper_client::stop(&self.runtime_paths).await
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
        self.write_service_configs_for_radio(hotspot, DEFAULT_RADIO_CONFIG)
    }

    fn write_service_configs_for_radio(
        &self,
        hotspot: &HotspotConfig,
        radio: ApRadioConfig,
    ) -> Result<()> {
        let hostapd_config = build_hostapd_config(hotspot, radio)?;
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

    /// Verifies that the selected PHY can keep a managed connection while an
    /// AP exists, and follows the client's channel when the radio is already
    /// associated. Most adapters allow only one channel across both virtual
    /// interfaces.
    async fn detect_radio_config(&self, parent_interface: &str) -> Result<ApRadioConfig> {
        let device = self
            .command_runner
            .run("iw", &["dev", parent_interface, "info"])
            .await
            .with_context(|| format!("Failed to inspect Wi-Fi interface {parent_interface}"))?;
        let phy = parse_phy_name(&device.stdout).with_context(|| {
            format!("Could not identify the wireless PHY for {parent_interface}")
        })?;

        let phy_info = self
            .command_runner
            .run("iw", &["phy", &phy, "info"])
            .await
            .with_context(|| {
                format!("Failed to inspect Wi-Fi capabilities for {parent_interface}")
            })?;

        if !supports_concurrent_managed_ap(&phy_info.stdout) {
            bail!(
                "Concurrent hotspot mode is not supported by this Wi-Fi adapter: no managed + AP interface combination was reported"
            );
        }

        let link = self
            .command_runner
            .run("iw", &["dev", parent_interface, "link"])
            .await
            .with_context(|| format!("Failed to inspect the Wi-Fi link for {parent_interface}"))?;

        match parse_link_frequency(&link.stdout) {
            Some(frequency) => radio_config_for_frequency(frequency),
            None if link.stdout.trim() == "Not connected." => Ok(DEFAULT_RADIO_CONFIG),
            None => bail!(
                "Could not determine the current Wi-Fi frequency for {parent_interface}; refusing to guess an AP channel"
            ),
        }
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

    /// Runs the complete startup as a staged transaction with persisted state.
    pub async fn start(&self, hotspot: &HotspotConfig) -> Result<RuntimeState> {
        self.check_dependencies()?;
        validate_interface_name(&hotspot.interface, "Parent Wi-Fi interface")?;

        if RuntimeState::load(&self.runtime_paths.state)?.is_some() {
            bail!("Concurrent hotspot state already exists; stop or clean it before starting");
        }

        let upstream_interface = self.detect_upstream_interface().await?;
        let radio = self.detect_radio_config(&hotspot.interface).await?;
        let mut state = RuntimeState {
            ap_interface: DEFAULT_AP_INTERFACE.into(),
            ap_interface_created: false,
            upstream_interface,
            ap_ip: DEFAULT_AP_ADDRESS.into(),
            hostapd_pid: None,
            dnsmasq_pid: None,
            ip_forwarding_was_enabled: true,
            iptables_rules: Vec::new(),
        };

        // Persist intent before the first system mutation so a crash can be
        // recovered through the same idempotent cleanup path.
        state.save(&self.runtime_paths)?;

        let startup_result: Result<()> = async {
            self.create_ap_interface(&hotspot.interface).await?;
            state.ap_interface_created = true;
            state.save(&self.runtime_paths)?;
            self.write_service_configs_for_radio(hotspot, radio)?;

            let pids = self.start_services().await?;
            state.hostapd_pid = Some(pids.hostapd);
            state.dnsmasq_pid = Some(pids.dnsmasq);
            state.save(&self.runtime_paths)?;

            let firewall = self
                .enable_network_sharing(&state.upstream_interface)
                .await?;
            state.ip_forwarding_was_enabled = firewall.ip_forwarding_was_enabled;
            state.iptables_rules = firewall.iptables_rules;
            state.save(&self.runtime_paths)?;

            Ok(())
        }
        .await;

        if let Err(startup_error) = startup_result {
            return match cleanup::cleanup_state(&self.command_runner, &self.runtime_paths, &state)
                .await
            {
                Ok(()) => Err(startup_error.context("Concurrent hotspot startup was rolled back")),
                Err(cleanup_error) => Err(anyhow::anyhow!(
                    "Concurrent hotspot startup failed: {startup_error:#}; rollback also failed: {cleanup_error:#}"
                )),
            };
        }

        Ok(state)
    }

    pub fn load_runtime_state(&self) -> Result<Option<RuntimeState>> {
        RuntimeState::load(&self.runtime_paths.state)
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
        // NetworkManager notices newly-created Wi-Fi interfaces immediately
        // and may bring this one down while hostapd is configuring it. Only
        // release Wiretray's virtual interface; the parent client interface
        // remains managed and connected.
        self.command_runner
            .run(
                "nmcli",
                &["device", "set", DEFAULT_AP_INTERFACE, "managed", "no"],
            )
            .await
            .context("Failed to reserve the AP interface for hostapd")?;

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

fn build_hostapd_config(hotspot: &HotspotConfig, radio: ApRadioConfig) -> Result<String> {
    if hotspot.ssid.is_empty() || hotspot.ssid.len() > 32 {
        bail!("Hotspot SSID must be between 1 and 32 bytes");
    }

    let mut config = format!(
        "interface={DEFAULT_AP_INTERFACE}\n\
         driver=nl80211\n\
         ssid2={}\n\
         hw_mode={}\n\
         channel={}\n\
         auth_algs=1\n",
        encode_hex(hotspot.ssid.as_bytes()),
        radio.hw_mode,
        radio.channel,
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
         dhcp-option=6,192.168.50.1\n\
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

fn parse_phy_name(output: &str) -> Result<String> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("wiphy "))
        .filter(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
        .map(|index| format!("phy{index}"))
        .ok_or_else(|| anyhow::anyhow!("iw did not report a valid wiphy index"))
}

fn supports_concurrent_managed_ap(output: &str) -> bool {
    let Some((_, combinations)) = output.split_once("valid interface combinations:") else {
        return false;
    };

    let mut current = String::new();
    for line in combinations.lines() {
        let line = line.trim();
        if line.starts_with("* ") {
            if combination_has_managed_and_ap(&current) {
                return true;
            }
            current.clear();
            current.push_str(line);
        } else if !current.is_empty()
            && (line.contains("#{") || line.contains("total") || line.contains("#channels"))
        {
            current.push(' ');
            current.push_str(line);
        } else if !current.is_empty() && !line.is_empty() {
            break;
        }
    }

    combination_has_managed_and_ap(&current)
}

fn combination_has_managed_and_ap(combination: &str) -> bool {
    let mut managed = false;
    let mut ap = false;

    for group in combination.split("#{").skip(1) {
        let Some((modes, _)) = group.split_once('}') else {
            continue;
        };
        for mode in modes.split(',').map(str::trim) {
            managed |= mode == "managed";
            ap |= mode == "AP";
        }
    }

    managed && ap
}

fn parse_link_frequency(output: &str) -> Option<u32> {
    output
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("freq:"))
        .and_then(parse_mhz)
}

fn parse_mhz(value: &str) -> Option<u32> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));

    if !fraction.bytes().all(|byte| byte == b'0') {
        return None;
    }

    whole.parse().ok()
}

fn radio_config_for_frequency(frequency: u32) -> Result<ApRadioConfig> {
    let (hw_mode, channel) = match frequency {
        2484 => ('g', 14),
        2412..=2472 if (frequency - 2407).is_multiple_of(5) => ('g', (frequency - 2407) / 5),
        5005..=5895 if (frequency - 5000).is_multiple_of(5) => ('a', (frequency - 5000) / 5),
        _ => bail!(
            "Concurrent hotspot mode does not support the current Wi-Fi frequency {frequency} MHz"
        ),
    };

    Ok(ApRadioConfig {
        hw_mode,
        channel: channel as u16,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashSet, VecDeque},
        os::unix::process::ExitStatusExt,
        process::ExitStatus,
        sync::{
            Mutex,
            atomic::{AtomicBool, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
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

    struct TransactionalRunner {
        paths: RuntimePaths,
        interface_exists: AtomicBool,
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

    impl TransactionalRunner {
        fn new(paths: RuntimePaths) -> Self {
            Self {
                paths,
                interface_exists: AtomicBool::new(false),
                calls: Mutex::new(Vec::new()),
            }
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

    impl CommandRunner for TransactionalRunner {
        fn command_exists(&self, _command: &str) -> bool {
            true
        }

        async fn output(&self, command: &str, args: &[&str]) -> Result<CommandOutput> {
            self.calls.lock().unwrap().push((
                command.to_string(),
                args.iter().map(|argument| argument.to_string()).collect(),
            ));

            let mut status = 0;
            let mut stdout = String::new();

            match (command, args) {
                ("ip", ["route", "show", "default"]) => {
                    stdout = "default via 10.0.0.1 dev enp2s0 metric 100\n".into();
                }
                ("iw", ["dev", "wlp3s0", "info"]) => {
                    stdout = "Interface wlp3s0\n\twiphy 0\n".into();
                }
                ("iw", ["phy", "phy0", "info"]) => {
                    stdout = concat!(
                        "Wiphy phy0\n",
                        "\tvalid interface combinations:\n",
                        "\t\t * #{ managed } <= 1, #{ AP } <= 1,\n",
                        "\t\t   total <= 2, #channels <= 1\n",
                    )
                    .into();
                }
                ("iw", ["dev", "wlp3s0", "link"]) => {
                    stdout = "Connected to 00:11:22:33:44:55\n\tfreq: 2437\n".into();
                }
                ("ip", ["link", "show", "dev", DEFAULT_AP_INTERFACE]) => {
                    status = i32::from(!self.interface_exists.load(Ordering::SeqCst));
                }
                (
                    "iw",
                    [
                        "dev",
                        _,
                        "interface",
                        "add",
                        DEFAULT_AP_INTERFACE,
                        "type",
                        "__ap",
                    ],
                ) => {
                    self.interface_exists.store(true, Ordering::SeqCst);
                }
                ("iw", ["dev", DEFAULT_AP_INTERFACE, "del"]) => {
                    self.interface_exists.store(false, Ordering::SeqCst);
                }
                ("hostapd", _) => std::fs::write(&self.paths.hostapd_pid, "4294967001\n")?,
                ("dnsmasq", _) => std::fs::write(&self.paths.dnsmasq_pid, "4294967002\n")?,
                ("sysctl", ["-n", "net.ipv4.ip_forward"]) => stdout = "0\n".into(),
                _ => {}
            }

            Ok(CommandOutput {
                status: ExitStatus::from_raw(status << 8),
                stdout,
                stderr: if status == 0 {
                    String::new()
                } else {
                    "not found".into()
                },
            })
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
            available: HashSet::from(["ip", "iptables", "sysctl", "pkexec"]),
        };
        let backend = ConcurrentApBackend::with_runner(runner, RuntimePaths::discover());

        let error = backend.check_dependencies().unwrap_err().to_string();

        assert_eq!(
            error,
            "Concurrent hotspot mode is unavailable: missing required tools: iw, hostapd, dnsmasq, nmcli"
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
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 0, 0, 0]);
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
                    "nmcli".into(),
                    vec!["device", "set", "wiretray-ap0", "managed", "no"]
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
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 0, 1, 0]);
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
        let runner = ScriptedCommandRunner::with_exit_codes([1, 0, 0, 1, 1]);
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

        let config = build_hostapd_config(&hotspot, DEFAULT_RADIO_CONFIG).unwrap();

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

        let config = build_hostapd_config(&hotspot, DEFAULT_RADIO_CONFIG).unwrap();

        assert!(config.contains(&format!("wpa_psk={psk}\n")));
        assert!(!config.contains("wpa_passphrase="));
    }

    #[test]
    fn writes_the_clients_5_ghz_channel_to_hostapd() {
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: Some("12345678".into()),
        };

        let config = build_hostapd_config(
            &hotspot,
            ApRadioConfig {
                hw_mode: 'a',
                channel: 36,
            },
        )
        .unwrap();

        assert!(config.contains("hw_mode=a\n"));
        assert!(config.contains("channel=36\n"));
    }

    #[test]
    fn builds_open_hostapd_config_without_wpa_options() {
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: None,
        };

        let config = build_hostapd_config(&hotspot, DEFAULT_RADIO_CONFIG).unwrap();

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

        let error = build_hostapd_config(&hotspot, DEFAULT_RADIO_CONFIG)
            .unwrap_err()
            .to_string();

        assert!(error.contains("printable ASCII"));
    }

    #[test]
    fn builds_dnsmasq_config_for_wiretray_subnet() {
        let paths = RuntimePaths::from_directory("/run/user/1000/wiretray");

        let config = build_dnsmasq_config(&paths).unwrap();

        assert!(config.contains("interface=wiretray-ap0\n"));
        assert!(config.contains("listen-address=192.168.50.1\n"));
        assert!(config.contains("dhcp-range=192.168.50.10,192.168.50.100,12h\n"));
        assert!(config.contains("dhcp-option=6,192.168.50.1\n"));
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

    #[tokio::test]
    async fn completes_transactional_start_and_persists_cleanup_state() {
        let directory = test_runtime_directory("startup");
        let paths = RuntimePaths::from_directory(&directory);
        let runner = TransactionalRunner::new(paths.clone());
        let backend = ConcurrentApBackend::with_runner(runner, paths.clone());
        let hotspot = HotspotConfig {
            interface: "wlp3s0".into(),
            ssid: "Wiretray".into(),
            passphrase: Some("12345678".into()),
        };

        let state = backend.start(&hotspot).await.unwrap();

        assert_eq!(state.upstream_interface, "enp2s0");
        assert_eq!(state.hostapd_pid, Some(4_294_967_001));
        assert_eq!(state.dnsmasq_pid, Some(4_294_967_002));
        assert_eq!(state.iptables_rules.len(), 3);
        assert_eq!(backend.load_runtime_state().unwrap(), Some(state));

        backend.cleanup().await.unwrap();
        assert!(!directory.exists());
    }

    fn test_runtime_directory(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "wiretray-{label}-test-{}-{unique}",
            std::process::id()
        ))
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
    fn parses_the_selected_interface_phy() {
        let output = "Interface wlp3s0\n\tifindex 3\n\twdev 0x1\n\twiphy 2\n";

        assert_eq!(parse_phy_name(output).unwrap(), "phy2");
    }

    #[test]
    fn accepts_a_managed_and_ap_interface_combination() {
        let output = concat!(
            "Wiphy phy0\n",
            "\tvalid interface combinations:\n",
            "\t\t * #{ managed } <= 1, #{ AP, P2P-client } <= 1,\n",
            "\t\t   total <= 2, #channels <= 1\n",
        );

        assert!(supports_concurrent_managed_ap(output));
    }

    #[test]
    fn rejects_ap_support_without_a_concurrent_combination() {
        let output = concat!(
            "Wiphy phy0\n",
            "\tSupported interface modes:\n",
            "\t\t * managed\n",
            "\t\t * AP\n",
            "\tvalid interface combinations:\n",
            "\t\t * #{ managed } <= 1, total <= 1, #channels <= 1\n",
            "\t\t * #{ AP } <= 1, total <= 1, #channels <= 1\n",
        );

        assert!(!supports_concurrent_managed_ap(output));
    }

    #[test]
    fn follows_the_clients_24_ghz_channel() {
        let frequency = parse_link_frequency("Connected\n\tfreq: 2437\n").unwrap();

        assert_eq!(
            radio_config_for_frequency(frequency).unwrap(),
            ApRadioConfig {
                hw_mode: 'g',
                channel: 6,
            }
        );
    }

    #[test]
    fn parses_decimal_frequency_reported_by_newer_iw_versions() {
        assert_eq!(
            parse_link_frequency("Connected\n\tfreq: 2462.0\n"),
            Some(2462)
        );
        assert_eq!(parse_link_frequency("Connected\n\tfreq: 2462.5\n"), None);
    }

    #[test]
    fn follows_the_clients_5_ghz_channel() {
        assert_eq!(
            radio_config_for_frequency(5180).unwrap(),
            ApRadioConfig {
                hw_mode: 'a',
                channel: 36,
            }
        );
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
