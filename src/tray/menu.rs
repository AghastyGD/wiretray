use anyhow::Result;
use tray_icon::menu::{Menu, MenuId, MenuItem, PredefinedMenuItem};

pub struct MenuItems {
    pub start_hotspot: MenuId,
    pub stop_hotspot: MenuId,
    pub quit: MenuId,
}

pub fn build() -> Result<(Menu, MenuItems)> {
    let start_item = MenuItem::new("Start Hotspot", true, None);
    let stop_item = MenuItem::new("Stop Hotspot", true, None);
    let quit_item = MenuItem::new("Quit", true, None);

    let items = MenuItems {
        start_hotspot: start_item.id().clone(),
        stop_hotspot: stop_item.id().clone(),
        quit: quit_item.id().clone(),
    };

    let menu = Menu::new();
    menu.append_items(&[
        &start_item,
        &stop_item,
        &PredefinedMenuItem::separator(),
        &quit_item,
    ])?;

    Ok((menu, items))
}
