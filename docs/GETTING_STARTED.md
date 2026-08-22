🇬🇧 English | 🇫🇷 [Français](GETTING_STARTED_FR.md) | 🇪🇸 [Español](GETTING_STARTED_ES.md)

# Getting Started (Raspberry Pi Rust App, Developer Workspace Setup)

This guide is intended for developers setting up a **local development environment** on their workstation (Mac/Linux/Windows) to work on the ArcadeMatrix_RPi native **Rust** codebase.

---

## 1. System Requirements

- **Rust Toolchain (1.75+)**: Installable via `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Cargo**: Rust package manager and build tool (included with Rustup).

---

## 2. Compiling and Running Locally (Dev / Mock Matrix)

On any Mac, Linux, or Windows machine without a physical Raspberry Pi connected:

```bash
git clone <this-repo-url>
cd ArcadeMatrix_RPi

# Fast compilation check
cargo check

# Compile and run in dev mode using Mock Canvas
cargo run
```

By default on Mac/Windows, the project uses `MockMatrix`, simulating the LED matrix canvas in memory while running the Actix web server at `http://127.0.0.1:8080`.

---

## 3. Running the Test Suite

The Rust test suite validates configuration parsing, the engine registry and
lifecycle, the self-healing config sanitizer (`tests/test_sanitizer.rs`), Actix
REST endpoints, and OTA firmware upload validation (`POST /api/update`):

```bash
cargo test
```

Linter and formatting check:

```bash
cargo fmt --check
cargo clippy -- -D warnings
```

---

## 4. Cross-Compilation & Raspberry Pi Deployment

To cross-compile the standalone native binary for Raspberry Pi from your Mac/Linux workstation:

```bash
# Install cross
cargo install cross

# 64-bit ARM Cross-Compilation (Raspberry Pi 3, 4, Zero 2 W)
cross build --target aarch64-unknown-linux-gnu --release

# 32-bit ARM Cross-Compilation (Raspberry Pi 2, Zero)
cross build --target armv7-unknown-linux-gnueabihf --release
```

The compiled binary will be located at `target/aarch64-unknown-linux-gnu/release/arcadematrix`. You can copy it directly to the Pi or update it seamless via the Web UI (**Firmware Update (OTA)** section).
