use gtk::Align;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Entry, Grid, Label, Orientation,
    PasswordEntry, Separator,
};

use tokio::runtime::Handle;

use crate::application::hotspot as hotspot_app;
use crate::settings::{hotspot_settings::HotspotSettings, service::SettingsService};

const WINDOW_NAME: &str = "wiretray-settings";

pub fn present(app: &Application, tokio_handle: Handle) {
    for win in app.windows() {
        if win.widget_name() == WINDOW_NAME {
            win.present();
            return;
        }
    }

    let settings = SettingsService::new()
        .load()
        .expect("failed to load settings");

    let active = tokio_handle.block_on(hotspot_app::is_active());

    let status_label = Label::builder()
        .label(if active { "● Active" } else { "○ Inactive" })
        .halign(Align::Start)
        .margin_top(16)
        .margin_start(24)
        .margin_end(24)
        .build();

    let ssid_label = Label::builder().label("SSID").halign(Align::Start).build();

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

    let security_label = Label::builder()
        .label("Security")
        .halign(Align::Start)
        .build();

    let security_value = Label::builder()
        .label("WPA2 Personal (AES)")
        .halign(Align::Start)
        .hexpand(true)
        .css_classes(["dim-label"])
        .build();

    let band_label = Label::builder().label("Band").halign(Align::Start).build();

    let band_value = Label::builder()
        .label("2.4 GHz")
        .halign(Align::Start)
        .hexpand(true)
        .css_classes(["dim-label"])
        .build();

    let channel_label = Label::builder()
        .label("Channel")
        .halign(Align::Start)
        .build();

    let channel_value = Label::builder()
        .label("1")
        .halign(Align::Start)
        .hexpand(true)
        .css_classes(["dim-label"])
        .build();

    let grid = Grid::builder()
        .row_spacing(12)
        .column_spacing(16)
        .margin_top(12)
        .margin_bottom(8)
        .margin_start(24)
        .margin_end(24)
        .build();

    grid.attach(&ssid_label, 0, 0, 1, 1);
    grid.attach(&ssid_entry, 1, 0, 1, 1);

    grid.attach(&passphrase_label, 0, 1, 1, 1);
    grid.attach(&passphrase_entry, 1, 1, 1, 1);

    grid.attach(&security_label, 0, 2, 1, 1);
    grid.attach(&security_value, 1, 2, 1, 1);

    grid.attach(&band_label, 0, 3, 1, 1);
    grid.attach(&band_value, 1, 3, 1, 1);

    grid.attach(&channel_label, 0, 4, 1, 1);
    grid.attach(&channel_value, 1, 4, 1, 1);

    let hint = Label::builder()
        .label(
            "A password must contain 8–63 characters.\nLeave the field empty to create an open network.",
        )
        .halign(Align::Start)
        .wrap(true)
        .margin_start(24)
        .margin_end(24)
        .css_classes(["dim-label"])
        .build();

    let error_label = Label::builder()
        .halign(Align::Start)
        .margin_start(24)
        .margin_end(24)
        .visible(false)
        .build();

    let start_btn = Button::builder().label("Start").sensitive(!active).build();

    let stop_btn = Button::builder()
        .label("Stop")
        .css_classes(["destructive-action"])
        .sensitive(active)
        .build();

    let save_btn = Button::builder()
        .label("Save")
        .css_classes(["suggested-action"])
        .build();

    {
        let handle = tokio_handle.clone();

        let status_label = status_label.clone();
        let start_btn_for_cb = start_btn.clone();
        let stop_btn_for_cb = stop_btn.clone();

        start_btn.connect_clicked(move |_| match handle.block_on(hotspot_app::start()) {
            Ok(()) => {
                status_label.set_label("● Active");
                start_btn_for_cb.set_sensitive(false);
                stop_btn_for_cb.set_sensitive(true);
            }
            Err(err) => {
                tracing::error!("Failed to start hotspot: {err:#}")
            }
        });
    }

    {
        let handle = tokio_handle.clone();

        let status_label = status_label.clone();

        let start_btn_for_cb = start_btn.clone();
        let stop_btn_for_cb = stop_btn.clone();

        stop_btn.connect_clicked(move |_| match handle.block_on(hotspot_app::stop()) {
            Ok(()) => {
                status_label.set_label("○ Inactive");
                start_btn_for_cb.set_sensitive(true);
                stop_btn_for_cb.set_sensitive(false);
            }
            Err(err) => {
                tracing::error!("Failed to stop hotspot: {err:#}");
            }
        });
    }

    // Keep the window synchronized with NetworkManager state,
    // even when the hotspot is started/stopped outside this window.
    {
        let handle = tokio_handle.clone();

        let status_label_poll = status_label.clone();
        let start_btn_poll = start_btn.clone();
        let stop_btn_poll = stop_btn.clone();

        gtk::glib::timeout_add_seconds_local(2, move || {
            let active = handle.block_on(hotspot_app::is_active());

            status_label_poll.set_label(if active { "● Active" } else { "○ Inactive" });

            start_btn_poll.set_sensitive(!active);
            stop_btn_poll.set_sensitive(active);

            gtk::glib::ControlFlow::Continue
        });
    }
    {
        let ssid_entry = ssid_entry.clone();
        let passphrase_entry = passphrase_entry.clone();
        let error_label = error_label.clone();

        save_btn.connect_clicked(move |_| {
            let passphrase = passphrase_entry.text().to_string();

            let valid = passphrase.is_empty()
                || (8..=63).contains(&passphrase.len())
                || (passphrase.len() == 64 && passphrase.chars().all(|c| c.is_ascii_hexdigit()));

            if !valid {
                error_label.set_label(
                    "Password must be 8–63 characters or a 64-character hexadecimal PSK.",
                );
                error_label.set_visible(true);
                return;
            }

            error_label.set_visible(false);

            let settings = HotspotSettings {
                ssid: ssid_entry.text().to_string(),
                passphrase,
            };

            tracing::info!("Saving settings");

            if let Err(err) = SettingsService::new().save(&settings) {
                tracing::error!("Failed to save settings: {err:#}");
            } else {
                tracing::info!("Settings saved");
            }
        });
    }

    let action_bar = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .spacing(8)
        .margin_start(24)
        .margin_end(24)
        .margin_bottom(24)
        .build();

    let spacer = GtkBox::builder()
        .orientation(Orientation::Horizontal)
        .hexpand(true)
        .build();

    action_bar.append(&start_btn);
    action_bar.append(&stop_btn);
    action_bar.append(&spacer);
    action_bar.append(&save_btn);

    let separator = Separator::new(Orientation::Horizontal);

    let content = GtkBox::builder()
        .orientation(Orientation::Vertical)
        .spacing(12)
        .build();

    content.append(&status_label);
    content.append(&grid);
    content.append(&separator);
    content.append(&hint);
    content.append(&error_label);
    content.append(&action_bar);

    let window = ApplicationWindow::builder()
        .application(app)
        .title("Wiretray")
        .default_width(420)
        .resizable(true)
        .child(&content)
        .build();

    window.set_widget_name(WINDOW_NAME);
    window.present();

    // Prevent GTK from selecting the entire SSID text on open.
    ssid_entry.set_position(-1);
}
