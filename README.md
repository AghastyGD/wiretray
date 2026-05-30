# wiretray

A Linux hotspot manager written in Rust.

`wiretray` uses D-Bus and NetworkManager to create and manage Wi-Fi hotspots, with a focus on simple tray-based controls.

## Motivation

This project was inspired by [Linux WiFi Hotspot](https://github.com/lakinduakash/linux-wifi-hotspot).

I used it for a while and it worked well, but one thing always bothered me: every time I wanted to enable or disable the hotspot, I had to open the application window first.

I wanted a solution that could stay in the system tray and make hotspot management available with a couple of clicks.

`wiretray` also serves as a way for me to learn more about D-Bus, NetworkManager, and desktop application development in Rust.

The long-term goal is simple: make hotspot management feel like any other background service on Linux.

## Current Status

The project is still under active development.

Implemented so far:

* Wi-Fi device discovery
* Hotspot creation through NetworkManager
* Hotspot shutdown
* Hotspot status detection
* D-Bus integration
* Async services powered by Tokio

Currently in progress:

* Tray integration
* Improved hotspot state handling
* Better error reporting

## Architecture

```text
wiretray
├── Tray UI
├── Hotspot Service
├── Network Service
├── D-Bus Layer
└── NetworkManager
```

### Components

#### NetworkService

Responsible for device discovery and network state information.

#### HotspotService

Handles hotspot lifecycle operations such as starting, stopping, and monitoring hotspot state.

#### D-Bus Layer

Provides communication with NetworkManager through D-Bus.

## Requirements

`wiretray currently requires:

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
