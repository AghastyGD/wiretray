<div align="center">

<img src="assets/icons/app/logo.png" width="180">

# Wiretray

**A modern Wi-Fi hotspot manager for Linux.**

![CI](https://github.com/aghastygd/wiretray/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/github/license/aghastygd/wiretray)
![Issues](https://img.shields.io/github/issues/aghastygd/wiretray)
</div>

Wiretray is a Linux hotspot manager focused on simplicity and quick access from the system tray.

It currently integrates with NetworkManager through D-Bus.

## Motivation

This project was inspired by [Linux WiFi Hotspot](https://github.com/lakinduakash/linux-wifi-hotspot).

I used it for a while and it worked well, but one thing always bothered me: every time I wanted to enable or disable the hotspot, I had to open the application window first.

I wanted a solution that could stay in the system tray and make hotspot management available with a couple of clicks.

Wiretray began as an experiment, but quickly evolved into a project focused on improving the Linux hotspot experience.

The long-term goal is simple: make hotspot management feel like any other background service on Linux.

## Current Status

The project is still under active development.

Implemented so far:

* System tray integration
* Wi-Fi device discovery
* Hotspot creation through NetworkManager
* Hotspot shutdown
* Hotspot status detection
* D-Bus integration
* Async services powered by Tokio

Currently in progress:

* Desktop configuration interface
* Improved hotspot state handling
* Better error reporting 

## Requirements

`wiretray` currently requires:

* Linux
* NetworkManager
* D-Bus

Unsupported environments:

* WSL
* Systems without NetworkManager
* Systems without Wi-Fi hardware

## Building

Install the required system packages before building.

**Debian / Ubuntu:**

```bash
sudo apt install libgtk-3-dev libxdo-dev libappindicator3-dev
```

**Arch Linux / Manjaro:**

```bash
sudo pacman -S gtk3 xdotool libappindicator-gtk3
```

Then build with Cargo:

```bash
cargo build
```

## Running

```bash
cargo run
```

## Development

Format code:

```bash
cargo fmt
```

Run Clippy:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

Run tests:

```bash
cargo test
```

## Roadmap

Planned work includes:

* Tray controls
* QR code generation
* Connected client monitoring
* Hotspot notifications
* Wi-Fi capability detection
* Advanced hotspot configuration
* Concurrent AP + Client support
* Alternative hotspot backends

As with most side projects, priorities may change over time.

## Contributing

Issues, suggestions, and pull requests are welcome.

## License

This project is licensed under the [MIT License](LICENSE).
