# Sinner Saint

A high-performance, WebAssembly-extensible Telegram bot engine written entirely in Rust.

This project acts as the Host environment, decoupling the Telegram networking layer from the actual business logic (Guest). By compiling business logic into WebAssembly (`wasm32-unknown-unknown`), bot features, commands, and state management can be updated, patched, and hot-swapped on the server without ever bringing the main network process offline.

## Architecture & Tech Stack

* **Host Engine:** Rust + `teloxide` (using pure `rustls` for C-free, static cross-compilation).
* **WASM Runtime:** `wasmtime` (JIT-compiles guest logic to native machine code at runtime).
* **Plugin System:** The host routes incoming Telegram updates (Messages, Callbacks, Inline Queries) directly into the loaded `.wasm` plugin, allowing the guest to handle all state, database interactions, and response generation dynamically.

## Development Setup (macOS / Apple Silicon)

Because this project can rely heavily on C-bindings during the linking phase, the easiest way to cross-compile from an M-series Mac to a Linux server is using Zig as the linker.

### 1. Install Dependencies

```bash
brew install zig
cargo install cargo-zigbuild
rustup target add x86_64-unknown-linux-gnu

```

### 2. Bypass macOS File Limits

Compiling the WASM toolchain requires opening hundreds of object files simultaneously. Before building, raise your terminal's file descriptor limit to prevent macOS from killing the linker:

```bash
ulimit -n 10000

```

### 3. Build for Production (Statically Linked Linux Binary)

```bash
cargo zigbuild --release --target x86_64-unknown-linux-gnu

```

## Deployment (Systemd)

The bot runs as a lightweight, naked binary managed by `systemd`. No Docker or container overhead is required.

**1. Create the working directory on your server:**

```bash
sudo mkdir -p /opt/sinner-saint

```

*Note: Move your compiled `sinner-saint` binary, `.env` file, and your compiled `plugin.wasm` file into this directory.*

**2. Create the Systemd Service (`/etc/systemd/system/sinnersaint.service`):**

```ini
[Unit]
Description=Sinner Saint Telegram Bot Host
After=network-online.target
Wants=network-online.target

[Service]
ExecStart=/opt/sinner-saint/sinner-saint
WorkingDirectory=/opt/sinner-saint
Restart=always
RestartSec=3
EnvironmentFile=/opt/sinner-saint/.env
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target

```

**3. Enable and Start:**

```bash
sudo systemctl daemon-reload
sudo systemctl enable sinnersaint
sudo systemctl start sinnersaint

```

**4. Watch the Live Logs:**

```bash
sudo journalctl -u sinnersaint -f

```
