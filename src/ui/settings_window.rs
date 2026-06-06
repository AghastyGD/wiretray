use gtk::Align;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box, Button, Grid, Label, Orientation, PasswordEntry,
    Entry,
};

use crate::settings::{hotspot_settings::HotspotSettings, service::SettingsService};

pub fn present(app: &Application) {
    let settings = SettingsService::new()
        .load()
        .expect("failed to load settings");

    let ssid_label = Label::builder()
        .label("SSID")
        .halign(Align::Start)
        .build();
    let ssid_entry = Entry::builder()
        .text(settings.ssid.as_str())
        .hexpand(true)
        .build();

    let passphrase_label = Label::builder()
        .label("Password")
        .halign(Align::Start)
        .build();
    let passphrase_entry = PasswordEntry::builder()
        .text(settings.passphrase.as_str())
        .show_peek_icon(true)
        .hexpand(true)
        .build();

    let grid = Grid::builder()
        .row_spacing(12)
        .column_spacing(16)
        .margin_top(24)
        .margin_bottom(8)
        .margin_start(24)
        .margin_end(24)
        .build();

    grid.attach(&ssid_label, 0, 0, 1, 1);
    grid.attach(&ssid_entry, 1, 0, 1, 1);
    grid.attach(&passphrase_label, 0, 1, 1, 1);
    grid.attach(&passphrase_entry, 1, 1, 1, 1);

    let save_btn = Button::builder()
        .label("Save")
        .halign(Align::End)
        .css_classes(["suggested-action"])
        .build();

    let ssid_entry_clone = ssid_entry.clone();
    let passphrase_entry_clone = passphrase_entry.clone();

    save_btn.connect_clicked(move |_| {
        let settings = HotspotSettings {
            ssid: ssid_entry_clone.text().to_string(),
            passphrase: passphrase_entry_clone.text().to_string(),
        };

        if let Err(err) = SettingsService::new().save(&settings) {
            tracing::error!("Failed to save settings: {err:#}");
        } else {
            tracing::info!("Settings saved");
        }
    });

    let content = Box::builder()
        .orientation(Orientation::Vertical)
        .spacing(16)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    content.append(&grid);
    content.append(&save_btn);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Wiretray")
        .default_width(420)
        .default_height(200)
        .resizable(true)
        .child(&content)
        .build();

    window.present();
}
