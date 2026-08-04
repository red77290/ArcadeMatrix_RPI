# ArcadeMatrix Smart Deploy (Windows)
# Auto-detects the Raspberry Pi architecture, builds via Docker, and deploys.

param (
    [string]$PI_IP = "192.168.1.149",
    [string]$PI_USER = "pi",
    [string]$PI_PASS = "raspberry"
)

# Test if SSH is available
if (-not (Get-Command ssh -ErrorAction SilentlyContinue)) {
    Write-Host "❌ Error: Windows OpenSSH client is not installed."
    Write-Host "Please install it via Windows Settings > Apps > Optional Features."
    exit 1
}

Write-Host "=========================================================="
Write-Host "      ArcadeMatrix Smart Deploy to Raspberry Pi"
Write-Host "=========================================================="
Write-Host "🚀 Connecting to ${PI_USER}@${PI_IP} to detect OS architecture..."

# Detect remote architecture
# Use plink or standard ssh. We will use standard ssh since Windows 10/11 has it.
# We have to bypass strict host checking
$REMOTE_ARCH = ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "uname -m"

if ($REMOTE_ARCH -match "aarch64") {
    Write-Host "✅ Detected 64-bit OS (aarch64) on Raspberry Pi."
    $BIN_PATH = "target/aarch64-unknown-linux-gnu/release/arcadematrix"
} elseif ($REMOTE_ARCH -match "armv") {
    Write-Host "✅ Detected 32-bit OS ($REMOTE_ARCH) on Raspberry Pi."
    $BIN_PATH = "target/armv7-unknown-linux-gnueabihf/release/arcadematrix"
} else {
    Write-Host "❌ Unknown architecture: $REMOTE_ARCH"
    exit 1
}

Write-Host "🔨 1. Building binary for $REMOTE_ARCH using Docker..."
# Convert path for WSL/Docker if needed, or just rely on bash
bash scripts/build.sh "$REMOTE_ARCH"

if (-not (Test-Path -Path $BIN_PATH)) {
    Write-Host "❌ Error: Compiled binary not found at $BIN_PATH"
    exit 1
}

Write-Host "🛑 2. Stopping arcadematrix service on Raspberry Pi..."
ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S systemctl stop arcadematrix.service || true"

Write-Host "📤 3. Uploading correct binary..."
scp -o StrictHostKeyChecking=no $BIN_PATH "${PI_USER}@${PI_IP}:/home/${PI_USER}/arcadematrix_temp"

Write-Host "⚙️  4. Moving binary and starting service..."
ssh -o StrictHostKeyChecking=no "${PI_USER}@${PI_IP}" "echo '${PI_PASS}' | sudo -S mv /home/${PI_USER}/arcadematrix_temp /usr/local/bin/arcadematrix && echo '${PI_PASS}' | sudo -S chmod +x /usr/local/bin/arcadematrix && if systemctl list-unit-files | grep -q arcadematrix.service; then echo '✅ Service already installed, restarting only...'; echo '${PI_PASS}' | sudo -S systemctl restart arcadematrix.service; else echo '⚠️ Service not found, running full autoInstall.sh setup...'; echo '${PI_PASS}' | sudo -S env SKIP_BUILD=1 bash /home/${PI_USER}/ArcadeMatrix_RPi/autoInstall.sh; fi"

Write-Host "✅ Deployment successful!"
Write-Host "=========================================================="
