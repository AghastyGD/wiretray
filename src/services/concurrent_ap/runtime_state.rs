use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const RUNTIME_DIRECTORY_NAME: &str = "wiretray";
const FALLBACK_RUNTIME_DIRECTORY: &str = "/tmp/wiretray";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePaths {
    pub directory: PathBuf,
    pub hostapd_config: PathBuf,
    pub dnsmasq_config: PathBuf,
    pub hostapd_pid: PathBuf,
    pub dnsmasq_pid: PathBuf,
    pub dnsmasq_leases: PathBuf,
    pub state: PathBuf,
}

impl RuntimePaths {
    pub fn discover() -> Self {
        let directory = env::var_os("XDG_RUNTIME_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .map(|base| base.join(RUNTIME_DIRECTORY_NAME))
            .unwrap_or_else(|| PathBuf::from(FALLBACK_RUNTIME_DIRECTORY));

        Self::from_directory(directory)
    }

    pub fn from_directory(directory: impl Into<PathBuf>) -> Self {
        let directory = directory.into();

        Self {
            hostapd_config: directory.join("hostapd.conf"),
            dnsmasq_config: directory.join("dnsmasq.conf"),
            hostapd_pid: directory.join("hostapd.pid"),
            dnsmasq_pid: directory.join("dnsmasq.pid"),
            dnsmasq_leases: directory.join("dnsmasq.leases"),
            state: directory.join("state.json"),
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
            use std::os::unix::fs::PermissionsExt;

            fs::set_permissions(&self.directory, fs::Permissions::from_mode(0o700)).with_context(
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
            options.mode(0o600);
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
    pub upstream_interface: String,
    pub ap_ip: String,
    pub hostapd_pid: Option<u32>,
    pub dnsmasq_pid: Option<u32>,
    pub iptables_rules: Vec<IptablesRule>,
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

        let temporary_path = paths.state.with_extension("json.tmp");
        let contents =
            serde_json::to_vec_pretty(self).context("Failed to serialize runtime state")?;

        paths
            .write_secure_file(&temporary_path, &contents)
            .with_context(|| {
                format!(
                    "Failed to write temporary runtime state {}",
                    temporary_path.display()
                )
            })?;
        fs::rename(&temporary_path, &paths.state).with_context(|| {
            format!("Failed to publish runtime state {}", paths.state.display())
        })?;

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
            upstream_interface: "wlp3s0".into(),
            ap_ip: "192.168.50.1/24".into(),
            hostapd_pid: Some(1234),
            dnsmasq_pid: Some(1235),
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
}
