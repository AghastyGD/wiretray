use anyhow::Result;
use gio::Settings;
use gio::prelude::SettingsExt;

use super::hotspot_settings::HotspotSettings;

const SCHEMA_ID: &str = "io.github.AghastyGD.Wiretray";

const KEY_SSID: &str = "ssid";
const KEY_PASSPHRASE: &str = "passphrase";

pub struct SettingsService {
    settings: Settings,
}

impl Default for SettingsService {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsService {
    pub fn new() -> Self {
        Self {
            settings: Settings::new(SCHEMA_ID),
        }
    }

    pub fn load(&self) -> Result<HotspotSettings> {
        Ok(HotspotSettings {
            ssid: self.settings.string(KEY_SSID).to_string(),
            passphrase: self.settings.string(KEY_PASSPHRASE).to_string(),
        })
    }
    pub fn save(&self, settings: &HotspotSettings) -> Result<()> {
        self.settings.set_string(KEY_SSID, &settings.ssid)?;

        self.settings.set_string(KEY_PASSPHRASE, &settings.passphrase)?;

        Ok(())
    }
}
