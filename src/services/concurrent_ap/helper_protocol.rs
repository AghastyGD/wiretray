use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::models::hotspot::HotspotConfig;

use super::runtime_state::RuntimeState;

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "kebab-case")]
pub enum HelperRequest {
    Start {
        runtime_directory: PathBuf,
        hotspot: HotspotConfig,
    },
    Stop {
        runtime_directory: PathBuf,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HelperResponse {
    pub state: Option<RuntimeState>,
    pub error: Option<String>,
}

impl HelperResponse {
    pub fn success(state: Option<RuntimeState>) -> Self {
        Self { state, error: None }
    }

    pub fn failure(error: anyhow::Error) -> Self {
        Self {
            state: None,
            error: Some(format!("{error:#}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_round_trips_without_putting_secrets_in_process_arguments() {
        let request = HelperRequest::Start {
            runtime_directory: "/run/wiretray/1000".into(),
            hotspot: HotspotConfig {
                interface: "wlp3s0".into(),
                ssid: "Wiretray".into(),
                passphrase: Some("secret123".into()),
            },
        };

        let json = serde_json::to_vec(&request).unwrap();
        let decoded: HelperRequest = serde_json::from_slice(&json).unwrap();

        assert!(matches!(
            decoded,
            HelperRequest::Start {
                hotspot: HotspotConfig { passphrase: Some(passphrase), .. },
                ..
            } if passphrase == "secret123"
        ));
    }
}
