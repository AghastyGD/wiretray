use gtk::gio::Settings;
use gtk::prelude::SettingsExt;
use anyhow::Result;

use super::hotspot_settings::HotspotSettings;

const SCHEMA_ID: &str = "io.github.AghastyGD.Wiretray";

const KEY_SSID: &str = "ssid";
const KEY_PASSWORD: &str = "password";

pub struct SettingsService {
    settings: Settings,
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
            password: self.settings.string(KEY_PASSWORD).to_string(),
        })
    }
    pub fn save(&self, settings: &HotspotSettings) -> Result<()> {
        self.settings
            .set_string(KEY_SSID, &settings.ssid)?;

        self.settings
            .set_string(KEY_PASSWORD, &settings.password)?;

        Ok(())
    }
}