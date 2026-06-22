<div align="center">

<img src="assets/icons/app/io.github.AghastyGD.Wiretray.png" width="180">

# Wiretray

**A Wi-Fi hotspot manager for Linux.**

![CI](https://github.com/aghastygd/wiretray/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/github/license/aghastygd/wiretray)
![Issues](https://img.shields.io/github/issues/aghastygd/wiretray)
</div>

Wiretray is a Linux Wi-Fi hotspot manager designed to make hotspot control available directly from the system tray.

Built around NetworkManager, it provides hotspot management, configuration, and automatic startup through a desktop interface.

## Motivation

This project was inspired by [Linux WiFi Hotspot](https://github.com/lakinduakash/linux-wifi-hotspot).

I used it for a while and had a good experience with it, but one thing always bothered me: every time I wanted to enable or disable the hotspot, I had to open the application window first.

I wanted a solution that could stay in the system tray and make hotspot management available with a couple of clicks.

Wiretray began as an experiment, but quickly evolved into a tool I use daily for managing hotspots on Linux.

The long-term goal is simple: make hotspot management feel like any other background service on Linux.

## Features

Available today:

- [x] System tray integration
    
- [x] Desktop configuration interface
    
- [x] Wi-Fi device discovery
    
- [x] Hotspot creation and management
    
- [x] Hotspot status monitoring
    
- [x] Hotspot settings persistence
    
- [x] Autostart on login
    
- [x] Wi-Fi capability detection
    

Currently in progress:

-   Better error reporting
-   Concurrent AP + Client support

## Requirements

-   Linux
-   NetworkManager
-   A Wi-Fi adapter with Access Point (AP) support
-   A desktop environment or panel with StatusNotifierItem/AppIndicator support

## Installation

### Debian / Ubuntu

Download the latest `.deb` package from the [Releases](https://github.com/AghastyGD/wiretray/releases) page and install it with:

```
sudo apt install ./wiretray_*.deb
```

You can also install it by double-clicking the `.deb` package from your file manager.

After installation, Wiretray can be opened from the application menu.

The application launcher opens the settings window and starts the tray process automatically if it is not already running.

## Autostart

Wiretray can start automatically after login.

Open the settings window and enable:

```
Start automatically on login
```

This creates an XDG autostart entry at:

```
~/.config/autostart/io.github.AghastyGD.Wiretray.desktop
```

When enabled, Wiretray starts the tray/background process automatically the next time you log in.

## Installed Binaries

The Debian package installs two binaries:

```
wiretray
wiretray-settings
```

`wiretray` starts the tray/background application.

`wiretray-settings` opens the desktop configuration interface.

Normally, users do not need to run these manually. The application launcher and autostart entry handle this automatically.

## Building from Source

Install the required development packages for your distribution.

### Debian / Ubuntu

```
sudo apt install \
  libgtk-4-dev \
  meson \
  desktop-file-utils \
  gcc \
  gtk-update-icon-cache
```

### Fedora

```
sudo dnf install \
  gtk4-devel \
  meson \
  desktop-file-utils \
  gcc \
  glib2-devel \
  gtk4-update-icon-cache
```

### Arch Linux

```
sudo pacman -S \
  gtk4 \
  meson \
  desktop-file-utils \
  gcc
```

Build all binaries:

```
cargo build
```

Or build a specific binary:

```
cargo build --bin wiretray
cargo build --bin wiretray-settings
```

## Running from Source

Start the tray application:

```
cargo run --bin wiretray
```

Launch the settings application directly:

```
cargo run --bin wiretray-settings
```

## Development

Format code:

```
cargo fmt
```

Run Clippy:

```
cargo clippy --all-targets --all-features -- -D warnings
```

Run tests:

```
cargo test
```

## Roadmap

Planned work includes:

-   QR code generation
-   Connected client monitoring
-   Hotspot notifications
-   Advanced hotspot configuration
-   Alternative hotspot backends

As with most side projects, priorities may change over time.

## Contributing

Issues, suggestions, and pull requests are welcome.

## License

This project is licensed under the MIT License.