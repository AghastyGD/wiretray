use gtk::Application;
use gtk::prelude::*;

use super::settings_window;

const APP_ID: &str = "io.github.AghastyGD.Wiretray";

pub fn run() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        settings_window::present(app);
    });

    app.run();
}
