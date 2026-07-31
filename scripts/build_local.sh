#!/bin/bash
# ArcadeMatrix Fast Local Builder
# Cross-compiles the Rust project for ARM64 using Docker

set -e

echo "=========================================================="
echo "      ArcadeMatrix RPi Fast Local Builder"
echo "=========================================================="
echo "This will compile the Rust binary for ARM64."
echo ""

docker run --rm \
    -v "$(pwd)":/workspace \
    -w /workspace \
    rust:bookworm \
    bash -c "dpkg --add-architecture arm64 && apt-get update && apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64 && rustup target add aarch64-unknown-linux-gnu && export PKG_CONFIG_ALLOW_CROSS=1 && export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig && CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu"

echo "✅ Compilation Complete! Binary is located at:"
echo "target/aarch64-unknown-linux-gnu/release/arcadematrix"
echo "=========================================================="
