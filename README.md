<div align="center">

<img src="assets/icons/app/io.github.AghastyGD.Wiretray.png" width="180">

# Wiretray

**A modern Wi-Fi hotspot manager for Linux.**

![CI](https://github.com/aghastygd/wiretray/actions/workflows/ci.yml/badge.svg)
![License](https://img.shields.io/github/license/aghastygd/wiretray)
![Issues](https://img.shields.io/github/issues/aghastygd/wiretray)
</div>
Wiretray is a Linux hotspot manager with system tray integration and a desktop configuration interface.

It integrates with NetworkManager to create and manage Wi-Fi hotspots.

## Motivation

This project was inspired by [Linux WiFi Hotspot](https://github.com/lakinduakash/linux-wifi-hotspot).

I used it for a while and had a good experience with it, but one thing always bothered me: every time I wanted to enable or disable the hotspot, I had to open the application window first.

I wanted a solution that could stay in the system tray and make hotspot management available with a couple of clicks.

wiretray began as an experiment, but quickly evolved into a tool I use daily for managing hotspots on Linux.

The long-term goal is simple: make hotspot management feel like any other background service on Linux.

## Current Status

The project is still under active development.

Implemented so far:

- [x] System tray integration

- [x] Desktop configuration interface
    
- [x] Wi-Fi device discovery
    
- [x] Hotspot creation and management
    
- [x] Hotspot status monitoring

- [x] Hotspot settings persistence

    

Currently in progress:
    
- Improved hotspot state handling
    
- Better error reporting
    

## Requirements

- Linux
    
- NetworkManager
    
- A Wi-Fi adapter with Access Point (AP) support
    

## Building

Build the project with Cargo:

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

- QR code generation
    
- Connected client monitoring
    
- Hotspot notifications
    
- Wi-Fi capability detection
    
- Advanced hotspot configuration
    
- Concurrent AP + Client support
    
- Alternative hotspot backends
    

As with most side projects, priorities may change over time.

## Contributing

Issues, suggestions, and pull requests are welcome.

## License

This project is licensed under the MIT License.