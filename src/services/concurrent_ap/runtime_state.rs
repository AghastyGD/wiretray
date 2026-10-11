use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    pub directory: PathBuf,
    pub hostapd_config: PathBuf,
    pub hostapd_log: PathBuf,
    pub dnsmasq_config: PathBuf,
    pub hostapd_pid: PathBuf,
    pub dnsmasq_pid: PathBuf,
    pub dnsmasq_leases: PathBuf,
    pub state: PathBuf,
    pub state_temporary: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> Self {
        // SAFETY: geteuid has no preconditions and does not retain pointers.
        let uid = unsafe { libc::geteuid() };
        let directory = PathBuf::from(format!("/run/wiretray/{uid}"));

        Self::from_directory(directory)
    }

    pub fn from_directory(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();

        Self {
            hostapd_config: directory.join("hostapd.conf"),
            hostapd_log: directory.join("hostapd.log"),
            dnsmasq_config: directory.join("dnsmasq.conf"),
            hostapd_pid: directory.join("hostapd.pid"),
            dnsmasq_pid: directory.join("dnsmasq.pid"),
            dnsmasq_leases: directory.join("dnsmasq.leases"),
            state: directory.join("state.json"),
            state_temporary: directory.join("state.json.tmp"),
            directory,
        }
    }

    pub fn ensure_directory(&self) -> Result<()> {
        fs::create_dir_all(&self.directory).with_context(|| {
            format!(
                "Failed to create Wiretray runtime directory {}",
                self.directory.display()
            )
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};

            let metadata = fs::metadata(&self.directory)?;
            let mode = if metadata.uid() == 0 && self.directory.starts_with("/run/wiretray") {
                0o711
            } else {
                0o700
            };

            fs::set_permissions(&self.directory, fs::Permissions::from_mode(mode)).with_context(
                || {
                    format!(
                        "Failed to secure Wiretray runtime directory {}",
                        self.directory.display()
                    )
                },
            )?;
        }

        Ok(())
    }

    pub fn write_secure_file(&self, path: &Path, contents: &[u8]) -> Result<()> {
        self.ensure_directory()?;

        let mut options = fs::OpenOptions::new();
        options.create(true).truncate(true).write(true);

        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }

        let mut file = options
            .open(path)
            .with_context(|| format!("Failed to open runtime file {}", path.display()))?;
        file.write_all(contents)
            .with_context(|| format!("Failed to write runtime file {}", path.display()))?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .with_context(|| format!("Failed to secure runtime file {}", path.display()))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IptablesRule {
    pub table: String,
    pub chain: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeState {
    pub ap_interface: String,
    #[serde(default = "default_ap_interface_created")]
    pub ap_interface_created: bool,
    pub upstream_interface: String,
    pub ap_ip: String,
    pub hostapd_pid: Option<u32>,
    pub dnsmasq_pid: Option<u32>,
    #[serde(default = "default_ip_forwarding_was_enabled")]
    pub ip_forwarding_was_enabled: bool,
    pub iptables_rules: Vec<IptablesRule>,
}

fn default_ip_forwarding_was_enabled() -> bool {
    // Older state files did not record ownership of this global setting. The
    // safe fallback is to leave forwarding enabled during cleanup.
    true
}

fn default_ap_interface_created() -> bool {
    // State files from before staged startup were only written for an interface
    // Wiretray intended to own.
    true
}

impl RuntimeState {
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let contents = match fs::read(path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("Failed to read runtime state {}", path.display()));
            }
        };

        serde_json::from_slice(&contents)
            .with_context(|| format!("Failed to parse runtime state {}", path.display()))
            .map(Some)
    }

    pub fn save(&self, paths: &RuntimePaths) -> Result<()> {
        paths.ensure_directory()?;

        let contents =
            serde_json::to_vec_pretty(self).context("Failed to serialize runtime state")?;

        paths
            .write_secure_file(&paths.state_temporary, &contents)
            .with_context(|| {
                format!(
                    "Failed to write temporary runtime state {}",
                    paths.state_temporary.display()
                )
            })?;
        fs::rename(&paths.state_temporary, &paths.state).with_context(|| {
            format!("Failed to publish runtime state {}", paths.state.display())
        })?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&paths.state, fs::Permissions::from_mode(0o644)).with_context(
                || {
                    format!(
                        "Failed to make runtime state readable {}",
                        paths.state.display()
                    )
                },
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_expected_runtime_paths() {
        let paths = RuntimePaths::from_directory("/run/user/1000/wiretray");

        assert_eq!(
            paths.hostapd_config,
            PathBuf::from("/run/user/1000/wiretray/hostapd.conf")
        );
        assert_eq!(
            paths.hostapd_log,
            PathBuf::from("/run/user/1000/wiretray/hostapd.log")
        );
        assert_eq!(
            paths.dnsmasq_config,
            PathBuf::from("/run/user/1000/wiretray/dnsmasq.conf")
        );
        assert_eq!(
            paths.state,
            PathBuf::from("/run/user/1000/wiretray/state.json")
        );
        assert_eq!(
            paths.dnsmasq_leases,
            PathBuf::from("/run/user/1000/wiretray/dnsmasq.leases")
        );
    }

    #[test]
    fn runtime_state_round_trips_as_json() {
        let state = RuntimeState {
            ap_interface: "wiretray-ap0".into(),
            ap_interface_created: true,
            upstream_interface: "wlp3s0".into(),
            ap_ip: "192.168.50.1/24".into(),
            hostapd_pid: Some(1234),
            dnsmasq_pid: Some(1235),
            ip_forwarding_was_enabled: false,
            iptables_rules: vec![IptablesRule {
                table: "nat".into(),
                chain: "POSTROUTING".into(),
                args: vec![
                    "-o".into(),
                    "wlp3s0".into(),
                    "-j".into(),
                    "MASQUERADE".into(),
                ],
            }],
        };

        let json = serde_json::to_string(&state).unwrap();
        let decoded: RuntimeState = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded, state);
    }

    #[test]
    fn legacy_state_does_not_claim_ownership_of_ip_forwarding() {
        let json = r#"{
            "ap_interface": "wiretray-ap0",
            "upstream_interface": "wlp3s0",
            "ap_ip": "192.168.50.1/24",
            "hostapd_pid": 1234,
            "dnsmasq_pid": 1235,
            "iptables_rules": []
        }"#;

        let state: RuntimeState = serde_json::from_str(json).unwrap();

        assert!(state.ip_forwarding_was_enabled);
    }

    #[cfg(unix)]
    #[test]
    fn saved_state_is_readable_but_not_writable_by_unprivileged_peers() {
        use std::{
            os::unix::fs::PermissionsExt,
            time::{SystemTime, UNIX_EPOCH},
        };

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!(
            "wiretray-state-test-{}-{unique}",
            std::process::id()
        ));
        let paths = RuntimePaths::from_directory(&directory);
        let state = RuntimeState {
            ap_interface: "wiretray-ap0".into(),
            ap_interface_created: true,
            upstream_interface: "wlp3s0".into(),
            ap_ip: "192.168.50.1/24".into(),
            hostapd_pid: Some(1234),
            dnsmasq_pid: Some(1235),
            ip_forwarding_was_enabled: false,
            iptables_rules: Vec::new(),
        };

        state.save(&paths).unwrap();

        assert_eq!(
            fs::metadata(&paths.state).unwrap().permissions().mode() & 0o777,
            0o644
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
