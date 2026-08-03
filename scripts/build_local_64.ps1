<#
.SYNOPSIS
ArcadeMatrix Fast Local Builder (64-bit) for Windows

.DESCRIPTION
Cross-compiles the Rust project for ARM64 (aarch64) only using Docker.
Requires Docker Desktop to be installed and running.
#>

Write-Host "=========================================================="
Write-Host "      ArcadeMatrix RPi Fast Local Builder (64-bit)"
Write-Host "=========================================================="
Write-Host "This will compile the Rust binary for 64-bit OS only."
Write-Host "Please wait, this uses Docker and may take a minute..."
Write-Host ""

$pwd = Get-Location
$dockerCommand = @"
echo '=> Installing cross-compilation toolchains...'
dpkg --add-architecture arm64
apt-get update -qq
apt-get install -y -qq gcc-aarch64-linux-gnu g++-aarch64-linux-gnu libc6-dev-arm64-cross libssl-dev:arm64

echo '=> Adding Rust targets...'
rustup target add aarch64-unknown-linux-gnu

export PKG_CONFIG_ALLOW_CROSS=1

echo '=> Compiling for 64-bit (aarch64)...'
export PKG_CONFIG_PATH=/usr/lib/aarch64-linux-gnu/pkgconfig
export CC=aarch64-linux-gnu-gcc
export CXX=aarch64-linux-gnu-g++
CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER=aarch64-linux-gnu-gcc cargo build --release --target aarch64-unknown-linux-gnu
"@

docker run --rm -v "${pwd}:/workspace" -w /workspace rust:bookworm bash -c $dockerCommand

if ($LASTEXITCODE -eq 0) {
    Write-Host ""
    Write-Host "✅ 64-bit Compilation Complete!"
    Write-Host "- Binary: target/aarch64-unknown-linux-gnu/release/arcadematrix"
    Write-Host "=========================================================="
} else {
    Write-Host ""
    Write-Host "❌ Compilation Failed!" -ForegroundColor Red
    exit $LASTEXITCODE
}
